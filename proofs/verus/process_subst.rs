// Model of `process-subst` (`sh2perl/src/transforms/process_subst.rs`).
//
// CONDITIONAL, NOT Tier 1: the theorems state the routing and residue
// laws over a file-store / output / status model.  They mention the
// denotation of a program (`prun`), so this is the conditional "middle
// rung", the same status the previous 6-obligation model had.  The bar
// here is the bigint_range/maskable bar applied inside that rung: the law
// is stated on the transform's own routing decision tree (mode ×
// consumer shape × stdin-only × process-spec kind), refusals are pinned,
// real divergences are counterexample theorems, and the mutants/negative
// control are named.
//
// ## The law of record
//
// bash's `cmd <(P)` passes the producer's stdout through a `/dev/fd/N`
// path argument; `cmd >(P)` passes a path the consumer writes and the
// producer reads.  POSIX targets cannot express either, so the pass
// materializes them as temp files.  The routing is sound iff:
//
//   - process-in: the consumer sees exactly the producer's output, byte
//     for byte, on the route the consumer shape selects (arg path,
//     stdin redirect, or retargeted fd-0 read), the temp path is
//     unlinked, and other temp files are untouched;
//   - process-out: the producer reads exactly what the consumer wrote,
//     the producer runs after the consumer (before cleanup), the temp
//     path is unlinked, and other temp files are untouched.
//
// `process_in_*` / `process_out_*` below are those laws, proved per
// routing shape the transform emits.
//
// ## The routing decision tree (the transform's own structure)
//
//   Mode        = PIn | POut
//   Consumer    = Arg          -- appendable exec, path arg appended
//               | StdinOnly    -- mapfile/readarray, stdin r redirect
//               | Retarget     -- non-appendable/compound consumer
//               | PipeStage    -- pipeline stage hoisted outside pipeline
//               | CallForm     -- redirect(Arrow([exec]),[spec]) expr form
//
// For PIn the producer fills the temp BEFORE the consumer reads it; for
// POut the consumer fills the temp and the producer reads it AFTER the
// consumer, BEFORE rm.  StdinOnly is PIn-only in the source (the
// stdin-only table is `mapfile|readarray`; a process-out on such a
// consumer is not a shape the transform accepts as stdin-only).  The
// CallForm process-in-only variant uses `and(exec, rm)` to preserve the
// exec's failure status; every other variant is a plain sequence whose
// final status is rm's.
//
// ## Refusals / guards pinned
//
//   - a non-string producer target, a parse failure / Debug-leak
//     producer, and an un-appendable exec arg shape refuse the rewrite
//     (`None`/left untouched) -- modelled as `refused`.
//   - only `process-in` / `process-out` spec modes are materialized; any
//     other mode is kept as a plain redirect spec.
//   - `stdin_only_command` is exactly {mapfile, readarray}.
//   - the producer/consumer fd pair is the INVERSE: PIn producer writes
//     fd 1, consumer reads fd 0; POut consumer writes fd 1, producer
//     reads fd 0.  The inverted pair is the negative control.
//
// ## Divergences / counterexample theorems (source untouched)
//
//   - `rm_clobbers_status`: a plain-sequence lowering ends in rm's
//     status, so the consumer's nonzero status is lost.  Real, accepted
//     (header documents it); the CallForm process-in-only `and` variant
//     preserves failure status (`and_preserves_failure_status`).
//   - `process_out_before_consumer_is_wrong`: running the POut producer
//     before the consumer writes the temp makes the producer read empty
//     bytes -- the ordering is load-bearing.
//   - `process_in_after_consumer_is_wrong`: reading before the producer
//     fills yields empty bytes -- the inverse ordering bug.
//   - `inverted_pair_is_empty`: the fd/mode inversion the tests pin gives
//     the consumer empty input (negative control, kept from the prior
//     model).
//
// ## Out of scope, named
//
//   - real-time concurrency / producer that blocks forever (`<(yes)`);
//     the temp-file model is the fully-read-after-write approximation
//     the pass assumes.
//   - path/word quoting, the exact shell-text reconstruction of
//     `parse_producer` beyond the Debug-leak refusal, and renderer
//     emission.
//   - the parse-back of producer shell text as an AST law (it is a
//     parser/lowering concern; the refusal surface is pinned here).
//   - nested process substitution inside a producer is modelled as
//     "producer bytes are the producer's output" -- the recursive
//     materialize() call preserves the same contract.
//
// Command: cd sh2perl && RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
//   $HOME/verus-0.2026.10.18.7d82994/verus --triggers-mode silent \
//   --rlimit 200 --multiple-errors 10 proofs/verus/process_subst.rs
use vstd::prelude::*;

verus! {

// ── routing decision tree ───────────────────────────────────────────

#[derive(PartialEq, Eq, Structural, Copy, Clone)]
pub enum Mode {
    PIn,
    POut,
}

#[derive(PartialEq, Eq, Structural, Copy, Clone)]
pub enum Consumer {
    /// Appendable exec: temp path appended as arg (bash /dev/fd/N arg).
    Arg,
    /// mapfile/readarray: stdin `r` redirect, no path arg.
    StdinOnly,
    /// Non-appendable/compound consumer: redirect retargeted.
    Retarget,
    /// Pipeline stage: hoisted outside the pipeline.
    PipeStage,
    /// Expression-form redirect(Arrow([exec]), [spec]).
    CallForm,
}

#[derive(PartialEq, Eq, Structural, Copy, Clone)]
pub enum SpecKind {
    Process(Mode),
    /// Any other redirect mode -- kept, not materialized.
    Plain,
}

// ── file-store / output / status model ──────────────────────────────

pub struct F {
    pub store: Map<int, Seq<int>>,
    /// bytes the consumer received (arg path read / stdin read)
    pub out: Seq<int>,
    /// bytes the POut producer received (reads the temp after consumer)
    pub prod_in: Seq<int>,
    /// exit status observable ($?)
    pub status: int,
}

pub open spec fn content(f: F, p: int) -> Seq<int> {
    if f.store.dom().contains(p) { f.store[p] } else { Seq::empty() }
}

pub enum P {
    /// mktemp + producer-write: producer output lands in a fresh temp.
    Fill(int, Seq<int>),
    /// consumer writes temp (process-out exec/retarget consumer).
    WriteTmp(int, Seq<int>),
    /// consumer reads temp (process-in: arg / stdin / retarget).
    Read(int),
    /// producer reads temp (process-out).
    ReadProd(int),
    /// rm -f tmp (sets status 0, like rm -f).
    Rm(int),
    /// consumer exec with a visible status.
    ExecStatus(int),
    Seq(Box<P>, Box<P>),
    /// and(a,b): run b only if a succeeded (status == 0).
    And(Box<P>, Box<P>),
}

pub open spec fn prun(s: P, f: F) -> F
    decreases s,
{
    match s {
        P::Fill(p, v) => F { store: f.store.insert(p, v), out: f.out, prod_in: f.prod_in, status: f.status },
        P::WriteTmp(p, v) => F { store: f.store.insert(p, v), out: f.out, prod_in: f.prod_in, status: f.status },
        P::Read(p) => F { store: f.store, out: f.out + content(f, p), prod_in: f.prod_in, status: f.status },
        P::ReadProd(p) => F { store: f.store, out: f.out, prod_in: f.prod_in + content(f, p), status: f.status },
        P::Rm(p) => F { store: f.store.remove(p), out: f.out, prod_in: f.prod_in, status: 0int },
        P::ExecStatus(s0) => F { store: f.store, out: f.out, prod_in: f.prod_in, status: s0 },
        P::Seq(a, b) => prun(*b, prun(*a, f)),
        P::And(a, b) => {
            let fa = prun(*a, f);
            if fa.status == 0 { prun(*b, fa) } else { fa }
        },
    }
}

// ── source decision predicates ──────────────────────────────────────

pub open spec fn is_process_mode(m: SpecKind) -> bool {
    matches!(m, SpecKind::Process(_))
}

pub open spec fn stdin_only_command(name: Seq<char>) -> bool {
    name == seq!['m','a','p','f','i','l','e'] || name == seq!['r','e','a','d','a','r','r','a','y']
}

/// The transform's routing decision: which consumer shape is used.
/// StdinOnly is only valid for PIn; POut on a stdin-only command takes
/// the Arg route (the source only special-cases stdin_only for PIn).
pub open spec fn route(mode: Mode, consumer: Consumer, stdin_only: bool) -> Consumer {
    if mode == Mode::PIn && stdin_only && consumer == Consumer::Arg {
        Consumer::StdinOnly
    } else {
        consumer
    }
}

/// A materializable spec: process mode, and a string target the parser
/// can reconstruct.  `ok` is the parse/shape guard (parse_producer
/// returned Some and the exec arg shape is appendable where required).
pub open spec fn materializable(kind: SpecKind, ok: bool) -> bool {
    is_process_mode(kind) && ok
}

// ── lowerings (the transform's own output shapes) ───────────────────
//
// The four non-CallForm PIn routes (Arg, StdinOnly, Retarget, PipeStage)
// are the same file-store/output/status program: producer fills the temp,
// consumer reads it, exec runs, rm removes the temp.  The arg-vs-fd0 vs
// retarget vs hoist distinction is renderer/routing placement, not a
// distinct semantic program in this model, so we prove one PIn sequence
// shape and cover the routing table via dispatch.  CallForm is genuinely
// distinct: it uses `and(exec, rm)` to preserve exec failure status.

/// PIn, plain-sequence route (Arg/StdinOnly/Retarget/PipeStage): producer
/// fills, consumer reads, exec runs, rm removes temp.
pub open spec fn lower_pin_seq(p: int, v: Seq<int>, exec_s: int) -> P {
    P::Seq(Box::new(P::Fill(p, v)),
        Box::new(P::Seq(Box::new(P::Read(p)),
            Box::new(P::Seq(Box::new(P::ExecStatus(exec_s)),
                Box::new(P::Rm(p)))))))
}

/// PIn, call-form, process-in only: and(exec-seq, rm) preserves exec
/// failure status.
pub open spec fn lower_pin_call(p: int, v: Seq<int>, exec_s: int) -> P {
    P::And(
        Box::new(P::Seq(Box::new(P::Fill(p, v)),
            Box::new(P::Seq(Box::new(P::Read(p)),
                Box::new(P::ExecStatus(exec_s)))))),
        Box::new(P::Rm(p)))
}

/// POut, all routing shapes: exec/consumer writes tmp, producer reads
/// after, rm last.  The routing shapes differ in placement, not in this
/// file-store/output/status program.
pub open spec fn lower_pout_seq(p: int, w: Seq<int>, exec_s: int) -> P {
    P::Seq(Box::new(P::ExecStatus(exec_s)),
        Box::new(P::Seq(Box::new(P::WriteTmp(p, w)),
            Box::new(P::Seq(Box::new(P::ReadProd(p)),
                Box::new(P::Rm(p)))))))
}

/// The dispatch: the transform's own routing decision.
pub open spec fn lower(mode: Mode, consumer: Consumer, stdin_only: bool, p: int, bytes: Seq<int>, exec_s: int) -> P {
    let c = route(mode, consumer, stdin_only);
    match mode {
        Mode::PIn => match c {
            Consumer::Arg | Consumer::StdinOnly | Consumer::Retarget | Consumer::PipeStage => {
                lower_pin_seq(p, bytes, exec_s)
            }
            Consumer::CallForm => lower_pin_call(p, bytes, exec_s),
        },
        Mode::POut => {
            // All POut routes share the same file-store/output/status
            // program (stdin_only falls through by route()).
            let _ = c;
            lower_pout_seq(p, bytes, exec_s)
        }
    }
}

/// The buggy fd/mode inversion the tests pin (producer reads, never
/// writes, so consumer sees empty).
pub open spec fn inverted_pin(p: int, exec_s: int) -> P {
    P::Seq(Box::new(P::Read(p)),
        Box::new(P::Seq(Box::new(P::ExecStatus(exec_s)),
            Box::new(P::Rm(p)))))
}

// ── process-in laws ─────────────────────────────────────────────────

proof fn pin_seq_preserves_output(p: int, v: Seq<int>, exec_s: int, f: F)
    requires !f.store.dom().contains(p),
    ensures prun(lower_pin_seq(p, v, exec_s), f).out == f.out + v,
{
    reveal_with_fuel(prun, 8);
    let f1 = F { store: f.store.insert(p, v), out: f.out, prod_in: f.prod_in, status: f.status };
    assert(content(f1, p) == v);
}

proof fn pin_seq_no_residue(p: int, v: Seq<int>, exec_s: int, f: F)
    ensures !prun(lower_pin_seq(p, v, exec_s), f).store.dom().contains(p),
{
    reveal_with_fuel(prun, 8);
    assert(!f.store.insert(p, v).remove(p).dom().contains(p));
}

proof fn pin_seq_preserves_other(p: int, q: int, v: Seq<int>, exec_s: int, f: F)
    requires q != p,
    ensures content(prun(lower_pin_seq(p, v, exec_s), f), q) == content(f, q),
{
    reveal_with_fuel(prun, 8);
    let mid = f.store.insert(p, v);
    let fin = mid.remove(p);
    assert(fin.dom().contains(q) == mid.dom().contains(q));
    if f.store.dom().contains(q) {
        assert(mid[q] == f.store[q]);
        assert(fin[q] == f.store[q]);
    }
    assert(content(prun(lower_pin_seq(p, v, exec_s), f), q) == content(f, q)) by { reveal(content); };
}

proof fn pin_call_preserves_output(p: int, v: Seq<int>, exec_s: int, f: F)
    requires !f.store.dom().contains(p), exec_s == 0,
    ensures prun(lower_pin_call(p, v, exec_s), f).out == f.out + v,
{
    reveal_with_fuel(prun, 10);
    let f1 = F { store: f.store.insert(p, v), out: f.out, prod_in: f.prod_in, status: f.status };
    assert(content(f1, p) == v);
}

proof fn pin_call_preserves_output_on_failure(p: int, v: Seq<int>, exec_s: int, f: F)
    requires !f.store.dom().contains(p), exec_s != 0,
    ensures prun(lower_pin_call(p, v, exec_s), f).out == f.out + v,
{
    reveal_with_fuel(prun, 10);
    let f1 = F { store: f.store.insert(p, v), out: f.out, prod_in: f.prod_in, status: f.status };
    assert(content(f1, p) == v);
}

proof fn pin_call_no_residue_on_success(p: int, v: Seq<int>, exec_s: int, f: F)
    requires exec_s == 0,
    ensures !prun(lower_pin_call(p, v, exec_s), f).store.dom().contains(p),
{
    reveal_with_fuel(prun, 10);
}

proof fn pin_call_residue_on_failure(p: int, v: Seq<int>, exec_s: int, f: F)
    requires exec_s != 0,
    ensures prun(lower_pin_call(p, v, exec_s), f).store.dom().contains(p),
{
    // and() short-circuits: rm does not run, so the temp remains.  This is
    // the status-preservation tradeoff of the call-form PIn-only lowering.
    reveal_with_fuel(prun, 10);
    assert(f.store.insert(p, v).dom().contains(p));
}

// ── process-out laws ────────────────────────────────────────────────

proof fn pout_seq_producer_gets_consumer_bytes(p: int, w: Seq<int>, exec_s: int, f: F)
    requires !f.store.dom().contains(p),
    ensures prun(lower_pout_seq(p, w, exec_s), f).prod_in == f.prod_in + w,
{
    reveal_with_fuel(prun, 8);
    let f1 = prun(P::ExecStatus(exec_s), f);
    let f2 = F { store: f1.store.insert(p, w), out: f1.out, prod_in: f1.prod_in, status: f1.status };
    assert(content(f2, p) == w);
}

proof fn pout_seq_no_residue(p: int, w: Seq<int>, exec_s: int, f: F)
    ensures !prun(lower_pout_seq(p, w, exec_s), f).store.dom().contains(p),
{
    reveal_with_fuel(prun, 8);
}

proof fn pout_seq_preserves_other(p: int, q: int, w: Seq<int>, exec_s: int, f: F)
    requires q != p,
    ensures content(prun(lower_pout_seq(p, w, exec_s), f), q) == content(f, q),
{
    reveal_with_fuel(prun, 8);
    let mid = f.store.insert(p, w);
    let fin = mid.remove(p);
    assert(fin.dom().contains(q) == mid.dom().contains(q));
    if f.store.dom().contains(q) {
        assert(mid[q] == f.store[q]);
        assert(fin[q] == f.store[q]);
    }
    assert(content(prun(lower_pout_seq(p, w, exec_s), f), q) == content(f, q)) by { reveal(content); };
}

// ── dispatch / acceptance table ─────────────────────────────────────

proof fn dispatch_pin_seq_routes(p: int, v: Seq<int>, s: int)
    ensures
        lower(Mode::PIn, Consumer::Arg, false, p, v, s) == lower_pin_seq(p, v, s),
        lower(Mode::PIn, Consumer::Arg, true, p, v, s) == lower_pin_seq(p, v, s),
        lower(Mode::PIn, Consumer::Retarget, false, p, v, s) == lower_pin_seq(p, v, s),
        lower(Mode::PIn, Consumer::PipeStage, false, p, v, s) == lower_pin_seq(p, v, s),
{ reveal(route); }

proof fn dispatch_pin_call(p: int, v: Seq<int>, s: int)
    ensures lower(Mode::PIn, Consumer::CallForm, false, p, v, s) == lower_pin_call(p, v, s),
{ reveal(route); }

proof fn dispatch_pout_all_routes(p: int, v: Seq<int>, s: int)
    ensures
        lower(Mode::POut, Consumer::Arg, false, p, v, s) == lower_pout_seq(p, v, s),
        lower(Mode::POut, Consumer::Arg, true, p, v, s) == lower_pout_seq(p, v, s),
        lower(Mode::POut, Consumer::Retarget, false, p, v, s) == lower_pout_seq(p, v, s),
        lower(Mode::POut, Consumer::PipeStage, false, p, v, s) == lower_pout_seq(p, v, s),
        lower(Mode::POut, Consumer::CallForm, false, p, v, s) == lower_pout_seq(p, v, s),
{ reveal(route); }

// ── guard / refusal pins ────────────────────────────────────────────

proof fn stdin_only_table_is_exact()
    ensures
        stdin_only_command(seq!['m','a','p','f','i','l','e']),
        stdin_only_command(seq!['r','e','a','d','a','r','r','a','y']),
        !stdin_only_command(seq!['c','a','t']),
        !stdin_only_command(seq!['t','e','e']),
        !stdin_only_command(seq!['g','r','e','p']),
{
    reveal(stdin_only_command);
}

proof fn plain_mode_is_not_materialized(ok: bool)
    ensures !materializable(SpecKind::Plain, ok),
{
    reveal(is_process_mode);
    reveal(materializable);
}

proof fn process_mode_needs_ok(m: Mode)
    ensures
        materializable(SpecKind::Process(m), true),
        !materializable(SpecKind::Process(m), false),
{
    reveal(is_process_mode);
    reveal(materializable);
}

proof fn only_process_modes_materialize(kind: SpecKind, ok: bool)
    ensures materializable(kind, ok) ==> is_process_mode(kind),
{
    reveal(materializable);
}

// ── divergences / counterexamples (source untouched) ────────────────

/// DIVERGENCE: rm -f clobbers $? in plain-sequence lowerings.  The
/// header documents this accepted limitation; the model makes it a
/// theorem so it cannot be silently "fixed" in the proof.
proof fn rm_clobbers_status_pin_seq(p: int, v: Seq<int>, exec_s: int, f: F)
    requires exec_s != 0,
    ensures prun(lower_pin_seq(p, v, exec_s), f).status == 0,
{
    reveal_with_fuel(prun, 8);
}

proof fn rm_clobbers_status_pout_seq(p: int, w: Seq<int>, exec_s: int, f: F)
    requires exec_s != 0,
    ensures prun(lower_pout_seq(p, w, exec_s), f).status == 0,
{
    reveal_with_fuel(prun, 8);
}

/// The CallForm PIn-only `and` variant preserves the exec's failure
/// status (the rm runs only on success).  This is the intentional
/// exception to the rm-clobbers-status caveat.
proof fn and_preserves_failure_status(p: int, v: Seq<int>, exec_s: int, f: F)
    requires !f.store.dom().contains(p), exec_s != 0,
    ensures prun(lower_pin_call(p, v, exec_s), f).status == exec_s,
{
    reveal_with_fuel(prun, 10);
    let f1 = F { store: f.store.insert(p, v), out: f.out, prod_in: f.prod_in, status: f.status };
    assert(content(f1, p) == v);
}

/// COUNTEREXAMPLE: running a POut producer BEFORE the consumer writes
/// the temp makes the producer read empty bytes.  This pins the
/// ordering the source enforces (producer in post, after exec/consumer).
proof fn process_out_before_consumer_is_wrong(p: int, w: Seq<int>, exec_s: int, f: F)
    requires !f.store.dom().contains(p),
    ensures
        prun(P::Seq(Box::new(P::ReadProd(p)),
                Box::new(P::Seq(Box::new(P::ExecStatus(exec_s)),
                    Box::new(P::Seq(Box::new(P::WriteTmp(p, w)),
                        Box::new(P::Rm(p))))))),
            f).prod_in == f.prod_in,
{
    reveal_with_fuel(prun, 8);
    assert(content(f, p) == Seq::<int>::empty());
    assert(f.prod_in + Seq::<int>::empty() == f.prod_in);
}

/// COUNTEREXAMPLE: reading before the PIn producer fills yields empty
/// bytes — the inverse ordering bug.
proof fn process_in_after_consumer_is_wrong(p: int, v: Seq<int>, exec_s: int, f: F)
    requires !f.store.dom().contains(p),
    ensures
        prun(P::Seq(Box::new(P::Read(p)),
                Box::new(P::Seq(Box::new(P::Fill(p, v)),
                    Box::new(P::Seq(Box::new(P::ExecStatus(exec_s)),
                        Box::new(P::Rm(p))))))),
            f).out == f.out,
{
    reveal_with_fuel(prun, 8);
    assert(content(f, p) == Seq::<int>::empty());
    assert(f.out + Seq::<int>::empty() == f.out);
}

/// NEGATIVE CONTROL: the fd/mode inversion the tests pin gives the
/// consumer empty input.  Kept from the prior model; load-bearing.
proof fn inverted_pair_is_empty(p: int, exec_s: int, f: F)
    requires !f.store.dom().contains(p),
    ensures prun(inverted_pin(p, exec_s), f).out == f.out,
{
    reveal_with_fuel(prun, 8);
    assert(content(f, p) == Seq::<int>::empty());
    assert(f.out + Seq::<int>::empty() == f.out);
}

// ── law of record over the routing decision tree ────────────────────

/// PIn LAW OF RECORD (consumer input): for every routing shape the
/// transform emits, the consumer sees exactly the producer's output
/// when the temp is fresh.  Stated over `lower(...)`, so it covers the
/// dispatch, not one hardcoded shape.
proof fn process_in_preserves_consumer_input(
    c: Consumer, stdin_only: bool, p: int, v: Seq<int>, exec_s: int, f: F,
)
    requires
        !f.store.dom().contains(p),
    ensures
        prun(lower(Mode::PIn, c, stdin_only, p, v, exec_s), f).out == f.out + v,
{
    reveal(route);
    reveal(lower);
    let r = route(Mode::PIn, c, stdin_only);
    match r {
        Consumer::Arg | Consumer::StdinOnly | Consumer::Retarget | Consumer::PipeStage => {
            pin_seq_preserves_output(p, v, exec_s, f);
        }
        Consumer::CallForm => {
            if exec_s == 0 {
                pin_call_preserves_output(p, v, exec_s, f);
            } else {
                pin_call_preserves_output_on_failure(p, v, exec_s, f);
            }
        }
    }
}

/// PIn residue law: every PIn routing removes the temp, EXCEPT the
/// CallForm failure-status-preserving `and` variant, which is pinned
/// separately as `pin_call_residue_on_failure`.
proof fn process_in_removes_temp(
    c: Consumer, stdin_only: bool, p: int, v: Seq<int>, exec_s: int, f: F,
)
    requires !(route(Mode::PIn, c, stdin_only) == Consumer::CallForm && exec_s != 0),
    ensures !prun(lower(Mode::PIn, c, stdin_only, p, v, exec_s), f).store.dom().contains(p),
{
    reveal(route);
    reveal(lower);
    let r = route(Mode::PIn, c, stdin_only);
    match r {
        Consumer::Arg | Consumer::StdinOnly | Consumer::Retarget | Consumer::PipeStage => {
            pin_seq_no_residue(p, v, exec_s, f);
        }
        Consumer::CallForm => { pin_call_no_residue_on_success(p, v, exec_s, f); }
    }
}

/// POut LAW OF RECORD: for every routing shape, the producer reads
/// exactly the bytes the consumer wrote, and the temp is removed.
proof fn process_out_preserves_producer_input(
    c: Consumer, stdin_only: bool, p: int, w: Seq<int>, exec_s: int, f: F,
)
    requires !f.store.dom().contains(p),
    ensures prun(lower(Mode::POut, c, stdin_only, p, w, exec_s), f).prod_in == f.prod_in + w,
{
    reveal(route);
    reveal(lower);
    let _ = route(Mode::POut, c, stdin_only);
    pout_seq_producer_gets_consumer_bytes(p, w, exec_s, f);
}

proof fn process_out_removes_temp(
    c: Consumer, stdin_only: bool, p: int, w: Seq<int>, exec_s: int, f: F,
)
    ensures !prun(lower(Mode::POut, c, stdin_only, p, w, exec_s), f).store.dom().contains(p),
{
    reveal(route);
    reveal(lower);
    let _ = route(Mode::POut, c, stdin_only);
    pout_seq_no_residue(p, w, exec_s, f);
}

fn main() {}

} // verus!
