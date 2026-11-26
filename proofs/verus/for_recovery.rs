// for_recovery.rs — model of `transforms/for_recovery.rs`.
//
// The pass rewrites a COUNTER WHILE into an inclusive `Range` For:
//
//     i = a;  while (i <op> b) { B; i = i ± 1 };  rest
//         =>  For { var: i, iter: Range { start: lo, end: hi }, body: B };  rest
//
// with the `end` table of `try_convert_at`:
//     (Lt, +1) => b - 1     (Le, +1) => b     (Gt, -1) => b + 1
//     (Ge, -1) => b
// then `(lo, hi) = (a, end)` for step +1 and `(end, a)` for step -1, and a
// refusal (`None`) when `lo > hi` (empty loop).
//
// What this model states and discharges:
//
//   lt_recovery_preserves_the_trace / le_recovery_preserves_the_trace
//       For the ACCEPTED ascending shapes the recovered `For` visits
//       exactly the `while`'s counter values, in the same order
//       (`while_run == for_run`), with the same trip count.
//   the_ascending_recovery_matches_the_while /
//   the_le_recovery_matches_the_while
//       The same two laws stated through `fires`, the guard predicate that
//       mirrors `try_convert_at`'s order: tail-is-increment, body scan
//       (`stmt_mentions` over everything but the trailing increment),
//       post-loop scan, immediately-preceding `i = literal`, `end` table.
//   the_structural_guards_refuse
//       Each of those guards is pinned by refutation (no init, tail not an
//       increment, a second counter mention, an escaping read), with the
//       accepted control beside it.
//   while_final_lt / the_escape_guard_is_load_bearing
//       The `while` leaves the counter AT the bound, the `For` at its last
//       iterated value — the divergence that makes the post-loop scan
//       necessary, proved in general (not just as an instance).
//   the_table_refuses_what_it_cannot_express
//       `step = 2`, `(Lt, -1)`, `(Gt, +1)`, and both empty shapes are
//       refused; the accepted shapes return the exact pairs.
//
// Real divergences, recorded as explicit counterexample theorems (NOT
// closed, NOT weakened, NOT patched — the transform code is untouched):
//
//   descending_recovery_reverses_the_order
//       The table accepts `(Gt, -1)` / `(Ge, -1)` and then NORMALISES the
//       bounds to `lo <= hi`, so the `For` it builds runs ASCENDING while
//       the `while` ran descending. For a body whose effect is
//       order-sensitive (any output), the recovery changes the program.
//       `i = 3; while [ $i -gt 0 ]; do echo $i; i=$((i-1)); done` prints
//       3 2 1; the recovered `For 1..=3` prints 1 2 3. This is a defect in
//       the rewrite, reported rather than modelled away.
//   a_continue_body_breaks_the_law
//       The body scan is expression-only, so `IrStmt::Continue` is
//       invisible to it: a `continue` in the body makes the `while` skip
//       the trailing increment (the counter stalls) while the `For` always
//       advances. `fires` returns Some for that shape (the guard does not
//       look), and the traces differ for every fuel above the For's trip
//       count. Also a defect, reported.
//   the_gt_corner_wraps_and_invents_a_loop /
//   the_lt_corner_wraps_and_invents_a_loop
//       The `end` table computes `b ± 1` in i64 without a checked op. At
//       `i64_max` the `Gt` arm wraps to `i64_min`, so `while [ $i -gt
//       i64_max ]` (empty) becomes `Range { i64_min, i64_max }` (2^64
//       iterations); symmetrically `Lt` at `i64_min`.
//
// Out of scope, named and not proved:
// - the RENDERERS. The `For`/`Range` node is printed by backends (C, Go,
//   estree, ...); the laws above are about the IR the pass builds and the
//   trace a consumer must reproduce. What each printer emits is not in this
//   model.
// - whether the pass FIRES on shell input at all: the audit's own note
//   (the proof-campaign plan) records that the parser hands the lowered comparison
//   operands wrapped in `Arith`, so `loop_cond`'s `Var/Int` match never
//   sees them and the transform is dormant on the corpus. That is a
//   reachability fact about the front end, measured there, not a law this
//   model can state; the laws above hold for the IR shapes `fires`
//   describes.
// - the Isabelle development `proofs/ForRecovery/ForRecovery.thy` proves
//   the ascending trace laws and only SET equality for the descending
//   direction (order deliberately not claimed). It could not be rebuilt in
//   this environment (no Isabelle binary, mirrors unreachable), so it is
//   corroboration, not the proof of record.
//
// Verus 0.2026.10.18.7d82994:
//   verus --triggers-mode silent --rlimit 60 for_recovery.rs
use vstd::prelude::*;

verus! {

pub enum Cmp { Lt, Le, Gt, Ge }

pub open spec fn end_of(cmp: Cmp, step: int, bound: int) -> Option<int> {
    match cmp {
        Cmp::Lt => if step == 1 { Some(bound - 1) } else { None },
        Cmp::Le => if step == 1 { Some(bound) } else { None },
        Cmp::Gt => if step == -1 { Some(bound + 1) } else { None },
        Cmp::Ge => if step == -1 { Some(bound) } else { None },
    }
}

pub open spec fn decide(cmp: Cmp, step: int, start: int, bound: int) -> Option<(int, int)> {
    match end_of(cmp, step, bound) {
        None => None,
        Some(e) => {
            let (lo, hi) = if step > 0 { (start, e) } else { (e, start) };
            if lo > hi { None } else { Some((lo, hi)) }
        }
    }
}

pub open spec fn test(cmp: Cmp, i: int, bound: int) -> bool {
    match cmp {
        Cmp::Lt => i < bound,
        Cmp::Le => i <= bound,
        Cmp::Gt => i > bound,
        Cmp::Ge => i >= bound,
    }
}

pub open spec fn while_run(cmp: Cmp, step: int, i: int, bound: int, fuel: nat) -> Seq<int>
    decreases fuel,
{
    if fuel == 0 || !test(cmp, i, bound) {
        Seq::empty()
    } else {
        seq![i] + while_run(cmp, step, i + step, bound, (fuel - 1) as nat)
    }
}

pub open spec fn for_run(lo: int, hi: int) -> Seq<int>
    decreases if hi >= lo { (hi - lo + 1) as nat } else { 0nat },
{
    if lo > hi { Seq::empty() } else { seq![lo] + for_run(lo + 1, hi) }
}

proof fn for_run_len(lo: int, hi: int)
    ensures for_run(lo, hi).len() == if hi >= lo { (hi - lo + 1) as nat } else { 0nat },
    decreases if hi >= lo { (hi - lo + 1) as nat } else { 0nat },
{
    if lo > hi {
    } else {
        for_run_len(lo + 1, hi);
    }
}

proof fn lt_trace(a: int, b: int, fuel: nat)
    requires a <= b, fuel >= (b - a) as nat,
    ensures while_run(Cmp::Lt, 1, a, b, fuel) == for_run(a, b - 1),
    decreases (b - a) as nat,
{
    if a == b {
        assert(test(Cmp::Lt, a, b) == false);
    } else {
        assert(test(Cmp::Lt, a, b) == true);
        assert(fuel > 0);
        lt_trace(a + 1, b, (fuel - 1) as nat);
    }
}

proof fn le_trace(a: int, b: int, fuel: nat)
    requires a <= b + 1, fuel >= (b + 1 - a) as nat,
    ensures while_run(Cmp::Le, 1, a, b, fuel) == for_run(a, b),
    decreases (b + 1 - a) as nat,
{
    if a == b + 1 {
        assert(test(Cmp::Le, a, b) == false);
    } else {
        assert(test(Cmp::Le, a, b) == true);
        assert(fuel > 0);
        le_trace(a + 1, b, (fuel - 1) as nat);
    }
}

// the accepted ascending shapes, as the table spells them
proof fn lt_recovery_preserves_the_trace(start: int, bound: int, fuel: nat)
    requires start <= bound - 1, fuel >= (bound - start) as nat,
    ensures decide(Cmp::Lt, 1, start, bound) == Some((start, bound - 1)),
            while_run(Cmp::Lt, 1, start, bound, fuel) == for_run(start, bound - 1),
            while_run(Cmp::Lt, 1, start, bound, fuel).len() == (bound - start) as nat,
{
    lt_trace(start, bound, fuel);
    for_run_len(start, bound - 1);
}

proof fn le_recovery_preserves_the_trace(start: int, bound: int, fuel: nat)
    requires start <= bound, fuel >= (bound - start + 1) as nat,
    ensures decide(Cmp::Le, 1, start, bound) == Some((start, bound)),
            while_run(Cmp::Le, 1, start, bound, fuel) == for_run(start, bound),
            while_run(Cmp::Le, 1, start, bound, fuel).len() == (bound - start + 1) as nat,
{
    le_trace(start, bound, fuel);
    for_run_len(start, bound);
}

// the descending shape the table also accepts; the For it builds runs ASCENDING
proof fn descending_recovery_reverses_the_order(a: int, b: int, fuel: nat)
    requires a >= b + 2, fuel >= (a - b) as nat,
    ensures decide(Cmp::Gt, -1, a, b) == Some((b + 1, a)),
            while_run(Cmp::Gt, -1, a, b, fuel)[0] == a,
            for_run(b + 1, a)[0] == b + 1,
            while_run(Cmp::Gt, -1, a, b, fuel) != for_run(b + 1, a),
{
    assert(a > b);
    assert(fuel > 0);
    assert(b + 1 <= a);
    if while_run(Cmp::Gt, -1, a, b, fuel) == for_run(b + 1, a) {
        assert(while_run(Cmp::Gt, -1, a, b, fuel)[0] == for_run(b + 1, a)[0]);
    }
}

// the refused shapes
proof fn the_table_refuses_what_it_cannot_express()
{
    assert(decide(Cmp::Lt, 2, 0, 3) == None::<(int, int)>) by (compute_only);
    assert(decide(Cmp::Lt, -1, 3, 0) == None::<(int, int)>) by (compute_only);
    assert(decide(Cmp::Gt, 1, 0, 3) == None::<(int, int)>) by (compute_only);
    assert(decide(Cmp::Lt, 1, 3, 3) == None::<(int, int)>) by (compute_only);
    assert(decide(Cmp::Gt, -1, 3, 3) == None::<(int, int)>) by (compute_only);
    assert(decide(Cmp::Lt, 1, 0, 1) == Some((0, 0))) by (compute_only);
    assert(decide(Cmp::Le, 1, 0, 3) == Some((0, 3))) by (compute_only);
    assert(decide(Cmp::Gt, -1, 3, 0) == Some((1, 3))) by (compute_only);
    assert(decide(Cmp::Ge, -1, 3, 0) == Some((0, 3))) by (compute_only);
}

// the counter AFTER the loop: the escape guard is what makes this irrelevant
pub open spec fn while_final(cmp: Cmp, step: int, i: int, bound: int, fuel: nat) -> int
    decreases fuel,
{
    if fuel == 0 || !test(cmp, i, bound) { i } else { while_final(cmp, step, i + step, bound, (fuel - 1) as nat) }
}

// the For leaves the loop variable at its last ITERATED value (`hi`)
pub open spec fn for_final(lo: int, hi: int) -> int { hi }

// the general law: the `while` leaves the counter AT the bound
proof fn while_final_lt(a: int, b: int, fuel: nat)
    requires a <= b, fuel >= (b - a) as nat,
    ensures while_final(Cmp::Lt, 1, a, b, fuel) == b,
    decreases (b - a) as nat,
{
    if a == b {
    } else {
        assert(fuel > 0);
        assert(while_final(Cmp::Lt, 1, a, b, fuel)
            == while_final(Cmp::Lt, 1, a + 1, b, (fuel - 1) as nat));
        while_final_lt(a + 1, b, (fuel - 1) as nat);
    }
}

// ... while the `For` leaves it at the last ITERATED value, so the two
// forms disagree after the loop: this is why the escape guard exists.
proof fn the_escape_guard_is_load_bearing(a: int, b: int, fuel: nat)
    requires a < b, fuel >= (b - a) as nat,
    ensures while_final(Cmp::Lt, 1, a, b, fuel) == b,
            for_final(a, b - 1) == b - 1,
            while_final(Cmp::Lt, 1, a, b, fuel) != for_final(a, b - 1),
{
    while_final_lt(a, b, fuel);
}

proof fn post_loop_counter_differs(fuel: nat)
    requires fuel >= 3,
    ensures while_final(Cmp::Lt, 1, 0, 3, fuel) == 3,
            for_final(0, 2) == 2,
            while_final(Cmp::Lt, 1, 0, 3, fuel) != for_final(0, 2),
{
    assert(fuel > 0);
    assert(while_final(Cmp::Lt, 1, 0, 3, fuel)
        == while_final(Cmp::Lt, 1, 1, 3, (fuel - 1) as nat));
    assert(fuel - 1 > 0);
    assert(while_final(Cmp::Lt, 1, 1, 3, (fuel - 1) as nat)
        == while_final(Cmp::Lt, 1, 2, 3, (fuel - 2) as nat));
    assert(fuel - 2 > 0);
    assert(while_final(Cmp::Lt, 1, 2, 3, (fuel - 2) as nat)
        == while_final(Cmp::Lt, 1, 3, 3, (fuel - 3) as nat));
    assert(while_final(Cmp::Lt, 1, 3, 3, (fuel - 3) as nat) == 3);
}

// ── the firing guard, as try_convert_at orders it ──────────────────────
// `start` is the initial value the immediately-preceding Assign supplies
// (None = no `i = literal` there); `rest_mentions` is the body scan
// (`stmt_mentions` over everything but the trailing increment);
// `after_mentions` the post-loop scan. `continue_in_body` is NOT consulted
// by the Rust — the scan walks expressions only, and `IrStmt::Continue`
// carries none — so it is a parameter here to make that visible.
pub open spec fn fires(
    cmp: Cmp,
    step: int,
    start: Option<int>,
    bound: int,
    tail_is_increment: bool,
    rest_mentions: bool,
    after_mentions: bool,
    continue_in_body: bool,
) -> Option<(int, int)> {
    match start {
        None => None,
        Some(a) => {
            if !tail_is_increment || rest_mentions || after_mentions {
                None
            } else {
                decide(cmp, step, a, bound)
            }
        }
    }
}

proof fn the_ascending_recovery_matches_the_while(start: int, bound: int, fuel: nat)
    requires start <= bound - 1, fuel >= (bound - start) as nat,
    ensures fires(Cmp::Lt, 1, Some(start), bound, true, false, false, false)
                == Some((start, bound - 1)),
            fires(Cmp::Lt, 1, Some(start), bound, true, false, false, false)
                != None::<(int, int)>,
            while_run(Cmp::Lt, 1, start, bound, fuel) == for_run(start, bound - 1),
{
    lt_recovery_preserves_the_trace(start, bound, fuel);
}

proof fn the_le_recovery_matches_the_while(start: int, bound: int, fuel: nat)
    requires start <= bound, fuel >= (bound - start + 1) as nat,
    ensures fires(Cmp::Le, 1, Some(start), bound, true, false, false, false)
                == Some((start, bound)),
            while_run(Cmp::Le, 1, start, bound, fuel) == for_run(start, bound),
{
    le_recovery_preserves_the_trace(start, bound, fuel);
}

// every structural guard is load-bearing: each refusal, and why
proof fn the_structural_guards_refuse()
{
    // no `i = literal` immediately before the loop
    assert(fires(Cmp::Lt, 1, None::<int>, 3, true, false, false, false)
        == None::<(int, int)>) by (compute_only);
    // the body's last statement is not the increment
    assert(fires(Cmp::Lt, 1, Some(0), 3, false, false, false, false)
        == None::<(int, int)>) by (compute_only);
    // the body mentions the counter elsewhere (a second writer / a read)
    assert(fires(Cmp::Lt, 1, Some(0), 3, true, true, false, false)
        == None::<(int, int)>) by (compute_only);
    // the counter escapes: read after the loop
    assert(fires(Cmp::Lt, 1, Some(0), 3, true, false, true, false)
        == None::<(int, int)>) by (compute_only);
    // the accepted control
    assert(fires(Cmp::Lt, 1, Some(0), 3, true, false, false, false)
        == Some((0, 2))) by (compute_only);
}

// ── the divergence the guard does NOT cover: `continue` ────────────────
// In the `while`, `continue` skips the trailing increment, so the counter
// never advances; the `For` always advances. The scan cannot see it:
// `stmt_mentions` visits expressions, and `IrStmt::Continue` has none.
pub open spec fn while_continue_run(i: int, bound: int, fuel: nat) -> Seq<int>
    decreases fuel,
{
    if fuel == 0 || !(i < bound) {
        Seq::empty()
    } else {
        seq![i] + while_continue_run(i, bound, (fuel - 1) as nat)
    }
}

proof fn continue_run_len(i: int, bound: int, fuel: nat)
    requires i < bound,
    ensures while_continue_run(i, bound, fuel).len() == fuel,
    decreases fuel,
{
    if fuel == 0 {
    } else {
        continue_run_len(i, bound, (fuel - 1) as nat);
    }
}

proof fn a_continue_body_breaks_the_law(fuel: nat)
    requires fuel >= 3,
    ensures fires(Cmp::Lt, 1, Some(0), 2, true, false, false, true) != None::<(int, int)>,
            while_continue_run(0, 2, fuel).len() == fuel,
            for_run(0, 1).len() == 2,
            while_continue_run(0, 2, fuel) != for_run(0, 1),
{
    continue_run_len(0, 2, fuel);
    for_run_len(0, 1);
}

// ── the unguarded i64 corners in the `end` table ───────────────────────
pub open spec fn i64_min() -> int { -9223372036854775808 }
pub open spec fn i64_max() -> int { 9223372036854775807 }
pub open spec fn wrap64(x: int) -> int { ((x + 9223372036854775808) % 18446744073709551616) - 9223372036854775808 }

// the machine's `end` computation: same table, wrapping i64 arithmetic
pub open spec fn end_of_machine(cmp: Cmp, step: int, bound: int) -> Option<int> {
    match cmp {
        Cmp::Lt => if step == 1 { Some(wrap64(bound - 1)) } else { None },
        Cmp::Le => if step == 1 { Some(bound) } else { None },
        Cmp::Gt => if step == -1 { Some(wrap64(bound + 1)) } else { None },
        Cmp::Ge => if step == -1 { Some(bound) } else { None },
    }
}

proof fn the_gt_corner_wraps_and_invents_a_loop(fuel: nat)
    ensures end_of_machine(Cmp::Gt, -1, i64_max()) == Some(i64_min()),
            while_run(Cmp::Gt, -1, i64_max(), i64_max(), fuel) == Seq::<int>::empty(),
            for_run(i64_min(), i64_max()).len() > 0,
{
    assert(wrap64(i64_max() + 1) == i64_min()) by (compute_only);
    assert(!test(Cmp::Gt, i64_max(), i64_max()));
    for_run_len(i64_min(), i64_max());
}

proof fn the_lt_corner_wraps_and_invents_a_loop(fuel: nat)
    ensures end_of_machine(Cmp::Lt, 1, i64_min()) == Some(i64_max()),
            while_run(Cmp::Lt, 1, i64_min(), i64_min(), fuel) == Seq::<int>::empty(),
            for_run(i64_min(), i64_max()).len() > 0,
{
    assert(wrap64(i64_min() - 1) == i64_max()) by (compute_only);
    assert(!test(Cmp::Lt, i64_min(), i64_min()));
    for_run_len(i64_min(), i64_max());
}

fn main() {}
}
