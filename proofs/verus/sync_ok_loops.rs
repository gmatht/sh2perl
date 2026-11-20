// Standalone model of sync-ok-loops (sh2perl/src/transforms/sync_ok_loops.rs):
// an ANALYSIS-ONLY pass that decides how a loop is scheduled by the ESTree
// renderer. It never rewrites the IR; it publishes two pointer-keyed verdicts:
//
//   sync_ok   the loop may run as ONE synchronous chunk (no per-iteration
//             `await`): its body produces no output and the loop is finite, or
//             its whole run is statically proven to cost <= ~200ms.
//   batch_ok  the loop is a "checkpointed" loop: sync-executable body, run in
//             chunks of BATCH (1024) with a `setImmediate` yield between
//             chunks instead of one await per iteration.
//
// Consumers (both estree-only — no C/Rust consumer exists by design):
//   shir.rs `compute_async_region_loops` drops a `sync_ok` loop from the
//   async-region set; the While/For lowerings emit `forLoopBatch` /
//   `whileLoopBatch(..., 1024)` for a `batch_ok` loop.
//
// ## The laws (one obligation each, so each claim is separately attributable)
//
//  L1 chunked_emission_preserves_order — the ONLY thing the verdict may
//     change is the yield cadence. For every body, every iteration count n and
//     every checkpoint factor k >= 1, the chunked run emits exactly the eager
//     per-iteration bytes: `batch(b, 0, n, k) == eager(b, 0, n)`. k = 1 is the
//     per-iteration async form, k = BATCH the checkpointed form and k >= n one
//     single chunk, so this one equation covers all three strategies.
//  L2 sync_ok_implies_finite — a full-sync run cannot be interrupted, so the
//     census must never select a loop that may not terminate (the module's own
//     rule: "never mark them full-sync (a sync infinite loop freezes the
//     harness)").
//  L3 sync_ok_implies_budget — an accepted loop's whole run is <= BUDGET_US by
//     its own pricing: trip_count * (body cost + per-iteration loop overhead).
//     The per-iteration term is load-bearing: pricing only the body makes
//     `n * 0 <= budget` pass for ANY n.
//  L4 batch_ok_implies_sync_executable — a chunk of BATCH iterations must not
//     contain a blocking op (read/wait/sleep) or an unknown-cost call.
//  L5 batch_factor_is_a_power_of_two — BATCH == 1024 and `BATCH & (BATCH-1)
//     == 0`, which is exactly the bitmask checkpoint the runtime uses.
//
// ## What is modelled and what is NOT
//
//   - The body is a CENSUS SUMMARY (priced statements, emitting statements,
//     blocking ops, unknown-cost ops) — the same facts `iter_cost` /
//     `body_output_free` / `batch_executable` compute from the statement list.
//     Modelling the statement list itself would only re-derive the census; the
//     laws are about what the census CLAIMS, so the summary is the honest
//     domain.
//   - HYPOTHESIS (stated, not assumed in the law): a data-dependent while
//     condition terminates (`WhileDep`). The code takes the same stance ("the
//     corpus has no infinite loops, and bash spins on them too"); a const-true
//     condition is NOT covered by it and must be `Spins`.
//   - The yield itself emits nothing and costs no bytes — that is what the
//     runtime's `forLoopBatch`/`setImmediate` contract says; L1 is the claim
//     that relies on it.
//   - Interleaving with background jobs (the responsiveness benefit) is not a
//     byte property and is not modelled; only that the bytes are unchanged.
//
// ## Defects the model forced (all repaired in the transform/renderer, tests
// pin them; the first two were red BEFORE the fix)
//
//   1. `const_true_like` read only Bool/Int/Str conditions, but the parser
//      emits `while (( 1 ))` as `exec "let" <words>` and `while [ 1 ]` as
//      `test <words>`. Both are a constant TRUE that never changes, so the
//      loop never ends; unrecognized, they were filed as data-dependent (hence
//      finite) and, with an output-free body, marked `sync_ok` — an INFINITE
//      loop running with NO yield, exactly the freeze the finiteness rule
//      exists to prevent. Repaired: a single integer-literal `let` word (and a
//      single operator-free `test` word) is constant truthiness.
//      Tests: a_while_let_true_is_not_finite, a_while_bracket_true_is_not_finite.
//   2. The verdict reached the renderer through a POINTER-keyed set
//      (ASYNC_REGION_LOOPS), but a producer body is CLONED into an arrow
//      before it is lowered, so a loop inside `( … )` or `$( … )` never
//      matched its recorded address and kept the NATIVE form — no yield, no
//      runtime `_capExceeded` cap. Repaired: the async-region set now carries
//      the loop NODES as well and the lookup falls back to content equality
//      (an over-match only costs a runtime loop where a native one would do).
//      Test: estree::an_infinite_while_in_a_producer_keeps_the_runtime_loop,
//      red with either fix reverted, green with both.
//   3. `cheap_total` priced only the body statements, so `n * 0 <= budget`
//      held for any trip count. Repaired with a per-iteration loop-overhead
//      term, which bounds the ITERATIONS too. HONEST SCOPE: every priced
//      EMITTING statement costs >= 3µs, so a real emitting loop was already
//      bounded; the hole was reachable only for a zero-cost body (which the
//      output-free branch accepts anyway), so this repair tightens the census
//      rather than changing a current verdict.
//      Test: a_huge_trip_trivial_body_is_not_cheap.
//
// ## Mutants (checked red, each a real edit of this file)
//
//   identity (sync_ok false)              -> the completeness pins
//   flag everything (sync_ok true)        -> L2 + the finite-refusal pins
//   drop finiteness from the output-free
//   branch                                -> L2
//   call a Spins finite                   -> L2 (a different witness family)
//   drop the budget check                 -> L3
//   loosen the budget (x1000)             -> L3
//   drop the per-iteration overhead       -> L3
//   drop the sync-executable conjunct     -> L4
//   BATCH 1024 -> 1000                    -> L5
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 sync_ok_loops.rs
//   -> verification results:: NN verified, 0 errors
use vstd::prelude::*;

verus! {

/// The checkpoint cadence (power of two, so the runtime's test is a bitmask).
pub const BATCH: u64 = 1024;
/// The "about 200ms" budget, in microseconds.
pub const BUDGET_US: u64 = 200_000;
/// Per-statement upper-bound cost, in microseconds.
pub const STMT_COST_US: u64 = 3;
/// The loop's own per-iteration cost (increment, compare, jump), in µs.
pub const LOOP_OVERHEAD_US: u64 = 1;

/// How the loop's iteration count is known.
pub enum Run {
    /// a `for` loop with a static trip count
    Trip(nat),
    /// a while/do-while whose condition is data-dependent (the census's
    /// hypothesis: it terminates, exactly as bash would)
    DataDep,
    /// a constant-true condition: never terminates
    Spins,
}

/// One iteration of the body, as the census prices it.
pub struct Body {
    /// priced statements (assign / arithmetic / echo / test / `:`)
    pub stmts: nat,
    /// statements that write output
    pub emits: nat,
    /// blocking or signal builtins: read/wait/sleep/eval/exit/exec
    pub blocks: nat,
    /// unknown-cost ops: function calls, captures, pipelines, redirects
    pub calls: nat,
}

pub open spec fn finite(r: Run) -> bool {
    match r {
        Run::Spins => false,
        _ => true,
    }
}

/// Can the whole body be priced at all? (A blocking or unknown-cost op is not
/// bounded by the statement cost model.)
pub open spec fn bounded(b: Body) -> bool {
    b.blocks == 0 && b.calls == 0
}

pub open spec fn body_cost(b: Body) -> nat {
    b.stmts * (STMT_COST_US as nat)
}

/// The cost of one iteration: the body plus the loop's own overhead.
pub open spec fn per_iter(b: Body) -> nat {
    body_cost(b) + (LOOP_OVERHEAD_US as nat)
}

pub open spec fn output_free(b: Body) -> bool {
    b.emits == 0
}

/// Every op runs in-process without awaiting: no blocking builtin, no
/// unknown-cost call.
pub open spec fn sync_executable(b: Body) -> bool {
    b.blocks == 0 && b.calls == 0
}

/// The "statically cheap" criterion: a known trip count times a bounded
/// per-iteration cost, within the budget.
pub open spec fn cheap(r: Run, b: Body) -> bool {
    match r {
        Run::Trip(n) => bounded(b) && n * per_iter(b) <= (BUDGET_US as nat),
        _ => false,
    }
}

/// The verdict the renderer reads: run this loop as ONE sync chunk.
pub open spec fn sync_ok(r: Run, b: Body) -> bool {
    (output_free(b) && finite(r)) || cheap(r, b)
}

/// The checkpointed verdict: not sync-ok, but every chunk is sync-executable
/// and the loop does produce output (otherwise a full sync run would do).
pub open spec fn batch_ok(r: Run, b: Body) -> bool {
    !sync_ok(r, b) && sync_executable(b) && b.emits > 0
}

/// What L3 claims independently of the census's formula: an accepted loop's
/// whole run is inside the budget by the pricing above.
pub open spec fn within_budget(r: Run, b: Body) -> bool {
    match r {
        Run::Trip(n) => n * per_iter(b) <= (BUDGET_US as nat),
        // a data-dependent or const-true loop has no static trip count, so the
        // cheap criterion never applies to it
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// The emitted bytes
// ---------------------------------------------------------------------------

/// A byte of output, abstracted: iteration `i` emits `i` (so order and
/// identity are both observable), and an output-free body emits nothing.
pub type Bytes = Seq<nat>;

pub open spec fn out(b: Body, i: nat) -> Bytes {
    if b.emits == 0 {
        Seq::<nat>::empty()
    } else {
        seq![i]
    }
}

/// The reference semantics: every iteration runs once, in order.
pub open spec fn eager(b: Body, i: nat, n: nat) -> Bytes
    decreases n - i,
{
    if i >= n {
        Seq::<nat>::empty()
    } else {
        out(b, i) + eager(b, i + 1, n)
    }
}

/// The scheduled semantics: the iterations in [i, n) run in chunks of `k`,
/// with a yield between chunks. A yield emits nothing, so the emitted bytes
/// must equal the reference ones. `k == 0` degenerates to one chunk (there is
/// no checkpoint to fire).
pub open spec fn batch(b: Body, i: nat, n: nat, k: nat) -> Bytes
    decreases n - i,
{
    if i >= n {
        Seq::<nat>::empty()
    } else if k == 0 {
        eager(b, i, n)
    } else {
        let m: nat = if i + k >= n {
            n
        } else {
            (i + k) as nat
        };
        eager(b, i, m) + batch(b, m, n, k)
    }
}

/// How many checkpoints a run of `n` iterations fires with factor `k`.
pub open spec fn yields(n: nat, k: nat) -> nat
    decreases n,
{
    if k == 0 || n <= k {
        0
    } else {
        1nat + yields((n - k) as nat, k)
    }
}

// ---------------------------------------------------------------------------
// L1: chunking preserves the emitted bytes (all three strategies at once)
// ---------------------------------------------------------------------------

/// Splitting a run at any interior point: the two halves concatenate.
proof fn eager_split(b: Body, a: nat, m: nat, c: nat)
    requires
        a <= m <= c,
    ensures
        eager(b, a, m) + eager(b, m, c) == eager(b, a, c),
    decreases m - a,
{
    reveal_with_fuel(eager, 2);
    if a == m {
        // eager(b, a, a) is empty
    } else {
        eager_split(b, (a + 1) as nat, m, c);
        assert(eager(b, a, m) == out(b, a) + eager(b, (a + 1) as nat, m));
        assert(eager(b, a, c) == out(b, a) + eager(b, (a + 1) as nat, c));
        // (out + half1) + half2 == out + (half1 + half2), by associativity
    }
}

/// THE L1 LAW: for every body, every iteration count and every checkpoint
/// factor k >= 1, the chunked run emits exactly the reference bytes. So the
/// verdict may change the yield cadence and nothing else.
proof fn chunked_emission_preserves_order(b: Body, i: nat, n: nat, k: nat)
    requires
        i <= n,
        k >= 1,
    ensures
        batch(b, i, n, k) == eager(b, i, n),
    decreases n - i,
{
    reveal_with_fuel(batch, 2);
    reveal_with_fuel(eager, 2);
    if i >= n {
    } else {
        let m: nat = if i + k >= n {
            n
        } else {
            (i + k) as nat
        };
        assert(m > i);
        assert(m <= n);
        // the rest of the run is the same chunk sequence, shifted
        chunked_emission_preserves_order(b, m, n, k);
        eager_split(b, i, m, n);
        // batch(i) == eager(i,m) + batch(m,n) == eager(i,m) + eager(m,n)
        //          == eager(i,n)
    }
}

// ---------------------------------------------------------------------------
// L2, L3, L4: what the verdicts claim
// ---------------------------------------------------------------------------

/// L2: `sync_ok` implies the loop terminates.
proof fn sync_ok_implies_finite(r: Run, b: Body)
    ensures
        sync_ok(r, b) ==> finite(r),
{
    match r {
        Run::Spins => {
            // cheap() needs a static trip count, and the output-free branch
            // needs finite(r): neither can hold for a spin
        },
        _ => {},
    }
}

/// L3a: the cheap criterion really does bound the run by the budget. This is
/// the NUMERIC claim, stated against the uncensored pricing, so a census that
/// loosens the budget or prices the body only cannot satisfy it.
proof fn cheap_is_within_budget(r: Run, b: Body)
    ensures
        cheap(r, b) ==> within_budget(r, b),
{
    match r {
        Run::Trip(n) => {},
        _ => {},
    }
}

/// L3b: an accepted loop either emits nothing and terminates, or its whole
/// run is priced within budget.
proof fn sync_ok_implies_budget(r: Run, b: Body)
    ensures
        sync_ok(r, b) ==> (output_free(b) && finite(r)) || within_budget(r, b),
{
    if sync_ok(r, b) {
        match r {
            Run::Trip(n) => {
                if !(output_free(b) && finite(r)) {
                    // the accepted branch was cheap()
                    assert(cheap(r, b));
                }
            },
            _ => {},
        }
    }
}

/// L4: `batch_ok` implies a sync-executable body.
proof fn batch_ok_implies_sync_executable(r: Run, b: Body)
    ensures
        batch_ok(r, b) ==> sync_executable(b),
{
}

/// L5: the checkpoint factor is the power of two the runtime's bitmask
/// test assumes.
proof fn batch_factor_is_a_power_of_two()
    ensures
        BATCH == 1024u64,
        BATCH > 0u64,
        (BATCH & ((BATCH - 1) as u64)) == 0u64,
{
    assert((BATCH & ((BATCH - 1) as u64)) == 0u64) by (bit_vector);
}

// ---------------------------------------------------------------------------
// Completeness / non-vacuity: the verdict is neither always false nor always
// true, and each refusal is reachable.
// ---------------------------------------------------------------------------

pub open spec fn b_pure() -> Body {
    Body { stmts: 2, emits: 0, blocks: 0, calls: 0 }
}

pub open spec fn b_echo() -> Body {
    Body { stmts: 2, emits: 1, blocks: 0, calls: 0 }
}

pub open spec fn b_block() -> Body {
    Body { stmts: 1, emits: 1, blocks: 1, calls: 0 }
}

pub open spec fn b_unknown() -> Body {
    Body { stmts: 1, emits: 1, blocks: 0, calls: 1 }
}

pub open spec fn b_empty() -> Body {
    Body { stmts: 0, emits: 0, blocks: 0, calls: 0 }
}

/// A statically cheap loop IS marked sync (an identity pass is red here).
proof fn a_cheap_loop_is_sync_ok()
    ensures
        sync_ok(Run::Trip(1000nat), b_pure()),
        sync_ok(Run::Trip(200000nat), b_empty()),
{
}

/// An output-free, finite loop IS marked sync regardless of its trip count.
proof fn an_output_free_while_is_sync_ok()
    ensures
        sync_ok(Run::DataDep, b_pure()),
{
}

/// A long loop that produces output is CHECKPOINTED, not awaited per
/// iteration.
proof fn a_long_output_loop_is_batched()
    ensures
        batch_ok(Run::Trip(10_000_000nat), b_echo()),
{
}

/// An infinite loop is NEVER marked sync — this is the freeze the finiteness
/// rule exists to prevent.
proof fn an_infinite_loop_is_not_sync_ok()
    ensures
        !sync_ok(Run::Spins, b_pure()),
        !sync_ok(Run::Spins, b_echo()),
        !sync_ok(Run::Spins, b_empty()),
{
}

/// A blocking or unknown-cost body is neither sync nor batch.
proof fn a_blocking_body_is_not_accepted()
    ensures
        !sync_ok(Run::Trip(10_000_000nat), b_block()),
        !batch_ok(Run::Trip(10_000_000nat), b_block()),
        !sync_ok(Run::Trip(10_000_000nat), b_unknown()),
        !batch_ok(Run::Trip(10_000_000nat), b_unknown()),
{
}

/// THE DEFECT PIN: a huge trip count with a trivial body is not cheap. Pricing
/// only the body made `n * 0 <= budget` hold for every n.
proof fn a_huge_trip_trivial_body_is_not_cheap()
    ensures
        // an empty body costs 0 in the body-only pricing; the per-iteration
        // term is what refuses the billion-iteration loop
        !cheap(Run::Trip(1_000_000_000nat), b_empty()),
        cheap(Run::Trip(200_000nat), b_empty()),
{
}

/// The verdict is a decision: both answers are reachable for both verdicts.
proof fn the_verdict_is_not_constant()
    ensures
        sync_ok(Run::Trip(10nat), b_pure()),
        !sync_ok(Run::Spins, b_pure()),
        batch_ok(Run::Trip(10_000_000nat), b_echo()),
        !batch_ok(Run::Trip(10_000_000nat), b_block()),
{
}

/// L1 is not vacuous: with a factor smaller than the run there IS at least one
/// checkpoint, i.e. the batched run really is chunked.
proof fn the_batch_run_really_chunks()
    ensures
        forall|n: nat, k: nat| k >= 1 && n > k ==> yields(n, k) >= 1,
        yields(2048nat, 1024nat) == 1,
{
    reveal_with_fuel(yields, 4);
}

fn main() {}

}
