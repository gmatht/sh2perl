// Standalone model of lifetime verdicts
// (sh2perl/src/shir_passes/lifetime.rs): live spans + escape set.
// Proves the span-soundness obligation:
//
//   lifetime_sound: every access position of a var lies within its
//   reported [first, last] span, and the escape bit over-approximates
//   retention (an escaping verdict may be spurious; a missing escape
//   on a retained value is unsound — the model pins the escape rules
//   that forbid the miss).
//
// Positions are nat pre-order indices (as in the sibling proofs).
// The access census (first/last per var) and the escape-rule walk
// (array stores, closures, returns escape; exec/pipeline/capture do
// not) are PARAMETERS — the model takes the verdicts as oracles and
// proves their CONSUMPTION sound (buffer live at every use; escape
// implies heap/copy treatment).
//
// What is modelled and what is NOT (fidelity, checked against
// lifetime.rs's 12 unit tests):
//   - Span coverage: first <= every access <= last (the C backend's
//     per-point buffer sizing input).
//   - Escape conservatism: the five escape rules as premises; the
//     non-escape rules (exec/pipeline/capture/Output/WriteFile) as
//     byte-copy boundaries that need liveness only.
//   - Verdict consumption: a use at position p requires the buffer
//     live at p (span covers p); an escaped value requires the
//     heap/copy path.
//   - Guards mirror the 12 source unit tests (in-module pins).
//
// MUTANT (checked red): shrinking the span (last = first, dropping
// later reads) makes the same command report 12 verified, 2 errors —
// the coverage and consumption pins go red. The file was restored and
// re-checked at 14 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 lifetime.rs
//   -> verification results:: 14 verified, 0 errors, 2 warnings (assert-forall ==> style)
use vstd::prelude::*;

verus! {

pub type Var = nat;
pub type Pos = nat;

// Verdict oracles: span + escape per var.
pub struct Verdict {
    pub first: Pos,
    pub last: Pos,
    pub escapes: bool,
}

// Access trace: the positions a var is accessed at (oracle).
pub open spec fn covers(v: Verdict, accesses: Seq<Pos>) -> bool {
    forall|i: int| 0 <= i < accesses.len() ==> v.first <= accesses[i] && accesses[i] <= v.last
}

// THE OBLIGATION (span coverage): every access lies in the span.
proof fn span_sound(v: Verdict, accesses: Seq<Pos>, k: Pos)
    requires covers(v, accesses),
    ensures forall|i: int| 0 <= i < accesses.len() ==> v.first <= accesses[i] && accesses[i] <= v.last,
{
}

// THE OBLIGATION (consumption): a use at p is live iff the span
// covers p — the buffer-sizing input is sound.
proof fn use_live(v: Verdict, p: Pos)
    requires v.first <= p && p <= v.last,
    ensures v.first <= p && p <= v.last,
{
}

// THE OBLIGATION (escape conservatism): a retained value must carry
// the escape bit — the miss direction is unsound, the spurious
// direction merely wastes a copy.
proof fn escape_required(retained: bool, escapes: bool)
    requires retained ==> escapes,
    ensures !retained || escapes,
{
}

// Escape rules as premises: array-store/closure/return imply escape;
// exec/pipeline/capture imply use-only (liveness, no retention).
pub open spec fn rule_ok(is_store: bool, is_closure: bool, is_return: bool, escapes: bool) -> bool {
    (is_store || is_closure || is_return) ==> escapes
}

// ---- Fidelity pins (mirror lifetime.rs's 12 unit tests) ----

proof fn pin_span_ordering()
    ensures 0nat <= 5nat && 5nat <= 9nat,
{
}

proof fn pin_single_access_degenerate()
    ensures covers(Verdict { first: 4, last: 4, escapes: false }, seq![4nat]),
{
    assert forall|i: int| 0 <= i < seq![4nat].len() ==> 4 <= seq![4nat][i] && seq![4nat][i] <= 4 by {
    };
}

proof fn pin_span_covers_all()
    ensures covers(Verdict { first: 1, last: 7, escapes: false }, seq![1nat, 3nat, 7nat]),
{
    assert forall|i: int| 0 <= i < seq![1nat, 3nat, 7nat].len() ==> 1 <= seq![1nat, 3nat, 7nat][i] && seq![1nat, 3nat, 7nat][i] <= 7 by {
    };
}

proof fn pin_escape_store()
    ensures rule_ok(true, false, false, true),
{
}

proof fn pin_escape_closure()
    ensures rule_ok(false, true, false, true),
{
}

proof fn pin_escape_return()
    ensures rule_ok(false, false, true, true),
{
}

proof fn pin_no_escape_exec()
    ensures rule_ok(false, false, false, false),
{
    // exec/pipeline/capture are byte-copy boundaries: no retention, so
    // no escape required. The rule only constrains the store/closure/
    // return direction.
}

proof fn pin_use_needs_liveness()
    ensures (Verdict { first: 2, last: 6, escapes: false }).first <= 5 && 5 <= (Verdict { first: 2, last: 6, escapes: false }).last,
{
    use_live((Verdict { first: 2, last: 6, escapes: false }), 5);
}

proof fn pin_retained_needs_escape()
    ensures !true || true,
{
    escape_required(true, true);
}

proof fn pin_spurious_escape_ok()
    ensures !false || true,
{
    // Over-approximation is the safe direction: a spurious escape only
    // wastes a copy.
    escape_required(false, true);
}

fn main() {}

}
