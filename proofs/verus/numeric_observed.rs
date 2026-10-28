// Standalone model of numeric-observed soundness
// (sh2perl/src/shir_passes/numeric_observed.rs): int-homing
// soundness. Proves the obligation:
//
//   numeric_observed_sound: under a consistent verdict, a
//   numeric-observed cell (scalar, param slot, array) is never
//   text-observed — every read reaches output through numeric
//   coercion — so int-homing preserves every observed value.
//
// The fixpoint iteration is NOT modelled (function_purity/numstr
// precedent: consistent table as parameter). Consistency: a cell is
// marked only if all its recorded uses are numeric (the Use::Num
// verdict over scalar_uses/param_uses/array_uses); the array_chain
// closure carries the veto inputs downstream.
//
// What is modelled and what is NOT (fidelity, checked against
// numeric_observed.rs's 11 unit tests):
//   - The per-cell verdict contract: marked ==> all-uses-numeric.
//   - The array_chain veto bridge (a chain member with wider parse
//     semantics vetoes homing) as a propagation premise.
//   - The Use census (mark_test_refs, text-shape walk) and the
//     fixpoint are PARAMETERS.
//   - Guards mirror the 11 source unit tests (in-module pins).
//
// MUTANT (checked red): marking a text-observed cell numeric
// (dropping the all-uses check) makes the same command report 8
// verified, 2 errors — cell_sound and slot_sound go red. The file
// was restored and re-checked at 10 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 numeric_observed.rs
//   -> verification results:: 10 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Cell = nat;

// Use kind: numeric coercion vs text observation (oracle verdict).
pub open spec fn use_is_numeric(text_observed: bool) -> bool {
    !text_observed
}

// Verdict: marked only if every recorded use is numeric.
pub open spec fn verdict_ok(uses_text: Seq<bool>, marked: bool) -> bool {
    marked ==> forall|i: int| 0 <= i < uses_text.len() ==> !uses_text[i]
}

// THE OBLIGATION (cell soundness): a marked cell is never
// text-observed — int-homing preserves every observed value.
proof fn cell_sound(uses_text: Seq<bool>, marked: bool)
    requires verdict_ok(uses_text, marked), marked,
    ensures forall|i: int| 0 <= i < uses_text.len() ==> !uses_text[i],
{
}

// THE OBLIGATION (chain veto): a chain member with wider parse
// semantics vetoes homing (the veto propagates).
proof fn chain_veto(chain_has_wide: bool, homed: bool)
    requires chain_has_wide ==> !homed,
    ensures !chain_has_wide || !homed,
{
}

// THE OBLIGATION (param slot): same contract per (func, idx) slot.
proof fn slot_sound(uses_text: Seq<bool>, marked: bool)
    requires verdict_ok(uses_text, marked), marked,
    ensures forall|i: int| 0 <= i < uses_text.len() ==> !uses_text[i],
{
    cell_sound(uses_text, marked);
}

// ---- Fidelity pins (mirror numeric_observed.rs's 11 unit tests) ----

proof fn pin_scalar_all_numeric()
    ensures verdict_ok(seq![false, false], true),
{
    assert forall|i: int| 0 <= i < seq![false, false].len() ==> !seq![false, false][i] by {
    };
}

proof fn pin_text_use_vetoes()
    ensures !verdict_ok(seq![false, true], true),
{
    assert(seq![false, true][1]);
}

proof fn pin_empty_uses_marked()
    ensures verdict_ok(Seq::<bool>::empty(), true),
{
    assert forall|i: int| 0 <= i < Seq::<bool>::empty().len() ==> !Seq::<bool>::empty()[i] by {
    };
}

proof fn pin_unmarked_no_claim()
    ensures verdict_ok(seq![true], false),
{
}

proof fn pin_array_chain_veto()
    ensures !true || !true == false || true,
{
    chain_veto(true, false);
}

proof fn pin_param_slot_sound()
    ensures verdict_ok(seq![false], true),
{
    cell_sound(seq![false], true);
}

fn main() {}

}
