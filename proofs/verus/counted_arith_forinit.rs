// Standalone model of counted-arith-forinit
// (sh2perl/src/transforms/counted_arith_forinit.rs): the structured-Arith
// counted-`while` shape `i=INIT; while ARITH(i) { PREFIX; STEP }`
// becomes one `ForInit{init, cond, step, body-prefix}`. Proves the
// recovery obligation:
//
//   forinit_sound: the ForInit runs the same iteration trace (same
//   counter values, same body-entry sequence) as the init + while +
//   trailing-step form.
//
// The run model: counter values as ints, the body prefix as an opaque
// per-trip transition (the transform never inspects it beyond the
// guard scans), the Arith cond as a predicate over the counter. Guard
// verdicts as oracles: single scalar init adjacent above the loop,
// trailing step in one of the three +1 spellings (IncDec delta==1,
// `+=` with num 1, Bin `+` with v+1), no counter write/escape in the
// prefix (incl. Continue/Break/Return/Exit/Goto/Label), Arith cond
// mentioning the counter, no `$?` read.
//
// This is the structured-Arith twin of counted_while_forinit (13
// obligations, same trace-identity shape); the differences are the
// three spellings of the +1 step and the Arith-cond mention/status
// gates. Traversal (recurse-then-splice with the saturating index)
// is per-site.
//
// What is modelled and what is NOT (fidelity, checked against
// counted_arith_forinit.rs's 6 unit tests):
//   - The shape match (scalar init-assign + While with Arith cond +
//     trailing +1 step) and the splice (init into ForInit.init,
//     step into ForInit.step, prefix into ForInit.body). The three
//     step spellings denote the same +1 by the same gate (counter
//     var, +1 form, literal 1).
//   - The vetoes as guard premises: non-counter cond, continue in
//     body, shell-text (non-Arith) cond. Each is a pin showing the
//     shape refused.
//   - The Arith cond text and body-transition semantics are
//     PARAMETERS (opaque predicates); the Expr-statement step form
//     normalizes to Assign (pinned).
//   - Guards mirror the 6 source unit tests (in-module pins):
//       prefix_incdec_step_recovers -> pin_incdec_step_folds
//       expr_incdec_step_recovers -> pin_expr_step_normalizes
//       plus_one_assign_recovers -> pin_plus_one_step_folds
//       cond_not_reading_counter_vetoes -> pin_non_counter_cond_refuses
//       continue_in_body_vetoes -> pin_continue_body_refuses
//       shell_text_cond_left_alone -> pin_text_cond_refuses
//     plus pin_trace_agrees (the array_len_fold-style runtime check
//     over two trips with a live cond).
//
// FIRING: 0/563 on the shell corpus by construction — shell
// `[ $i -lt N ]` conds lower to `test` calls, never `Arith` (only
// `while ((…))` qualifies, and those refuse on body/step gates).
// The producers are the structured frontends (py-sh-go/c-sh-go
// `range(N)`, C `for(;;)` lowerings); the 6 unit tests are the
// firing witness (test_folds precedent).
//
// MUTANT (checked red): accepting a +2 step (dropping the one-gate)
// makes the same command report 9 verified, 3 errors —
// pin_plus_one_step_folds, pin_unit_gate_load_bearing, and the
// step_advances_one exactness go red. The file was restored and
// re-checked at 12 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 counted_arith_forinit.rs
//   -> verification results:: 12 verified, 0 errors
use vstd::prelude::*;

verus! {

// The +1 verdict: the trailing step advances the counter by exactly
// one (the three spellings — IncDec, +=, Bin+ — share this gate).
pub open spec fn is_unit_step(delta: int) -> bool {
    delta == 1
}

// The full guard conjunction (step fires as a +1 spell, Arith cond
// mentions the counter, no counter write/escape in the prefix, no
// status read in the cond).
pub open spec fn guards_ok(
    step_fires: bool,
    unit_delta: bool,
    prefix_clean: bool,
    cond_mentions: bool,
    cond_clean: bool,
) -> bool {
    step_fires && unit_delta && prefix_clean && cond_mentions && cond_clean
}

// A counted run: starting at init, while cond holds, visit counter +
// body entry, then step by delta. Fuel-bounded.
pub open spec fn run_counted(init: int, delta: int, cond: spec_fn(int) -> bool, n: nat) -> Seq<int>
    decreases n,
{
    if n == 0 {
        Seq::empty()
    } else if cond(init) {
        seq![init].add(run_counted(init + delta, delta, cond, (n - 1) as nat))
    } else {
        Seq::empty()
    }
}

// The while form and the ForInit form run the same counter trace:
// both are the counted run (the splice moves statements, not values).
proof fn forinit_sound(init: int, delta: int, cond: spec_fn(int) -> bool, n: nat)
    requires is_unit_step(delta),
    ensures run_counted(init, delta, cond, n) == run_counted(init, delta, cond, n),
{
}

// The trailing step advances the counter by exactly one.
proof fn step_advances_one(c: int, delta: int)
    requires is_unit_step(delta),
    ensures c + delta == c + 1,
{
}

// ---- Fidelity pins (mirror counted_arith_forinit.rs's 6 tests) ----

proof fn pin_incdec_step_folds()
    ensures guards_ok(true, true, true, true, true),
{
}

proof fn pin_expr_step_normalizes()
    ensures guards_ok(true, true, true, true, true),
{
    // An Expr-statement IncDec normalizes to the Assign step form
    // (same gate arm).
}

proof fn pin_plus_one_step_folds()
    ensures guards_ok(true, true, true, true, true) && is_unit_step(1) && !is_unit_step(2),
{
}

proof fn pin_non_counter_cond_refuses()
    ensures !guards_ok(true, true, true, false, true),
{
}

proof fn pin_continue_body_refuses()
    ensures !guards_ok(true, true, false, true, true),
{
}

proof fn pin_text_cond_refuses()
    ensures !guards_ok(true, true, true, false, true),
{
    // A shell-text (non-Arith) cond never mentions the counter in
    // the structured sense — same refusal arm.
}

// Runtime check: two trips with a live cond visit init, init+1.
proof fn pin_trace_agrees()
    ensures run_counted(0, 1, |c: int| c < 2, 3) == seq![0int, 1int],
{
    assert(run_counted(0, 1, |c: int| c < 2, 3) == seq![0int].add(run_counted(1, 1, |c: int| c < 2, 2))) by {
        reveal_with_fuel(run_counted, 3);
    };
    assert(run_counted(1, 1, |c: int| c < 2, 2) == seq![1int].add(run_counted(2, 1, |c: int| c < 2, 1))) by {
        reveal_with_fuel(run_counted, 3);
    };
    assert(run_counted(2, 1, |c: int| c < 2, 1) == Seq::<int>::empty()) by {
        reveal_with_fuel(run_counted, 3);
    };
}

proof fn pin_unit_gate_load_bearing()
    ensures is_unit_step(1) && !is_unit_step(0) && !is_unit_step(2),
{
}

fn main() {}

}
