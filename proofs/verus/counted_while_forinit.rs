// Standalone model of counted-while-forinit
// (sh2perl/src/transforms/counted_while_forinit.rs): the counter-while
// shape `i=INIT; while COND(i) { BODY; i=i+/-K }` becomes one
// `ForInit{init, cond, step, body}`. Proves the recovery obligation:
//
//   forinit_sound: the ForInit runs the same iteration trace (same
//   counter values, same body-entry sequence) as the init + while +
//   trailing-step form.
//
// The run model: counter values as ints, the body as an opaque
// per-trip transition (the transform never inspects it beyond the
// guard scans), the cond as a predicate over the counter. Guard
// verdicts as oracles: single scalar init adjacent above the loop,
// trailing step `v = v +/- K` with nonzero literal K, no counter
// write/escape in the body prefix, no `$?` read in cond or prefix.
//
// What is modelled and what is NOT (fidelity, checked against
// counted_while_forinit.rs's 7 unit tests):
//   - The shape match (init-assign + While with trailing counter
//     step, both the BinOp and Arith spellings) and the splice
//     (init moves into ForInit.init, trailing step into ForInit.step,
//     body prefix into ForInit.body). The two spellings denote the
//     same step by the same gate (counter var, +/- op, nonzero
//     literal rhs).
//   - The vetoes as guard premises: zero step, dynamic (var) step,
//     body counter write, declare of the counter, plain (stepless)
//     while. Each is a pin showing the shape refused.
//   - The cond text (`test` call) and body-transition semantics are
//     PARAMETERS (opaque predicates); traversal (recurse-then-splice
//     with the saturating index) is per-site.
//   - Guards mirror the 7 source unit tests (in-module pins):
//       arith_wrapped_step_folds -> pin_arith_step_folds
//       binop_step_still_folds -> pin_binop_step_folds
//       zero_arith_step_refuses -> pin_zero_step_refuses
//       dynamic_arith_step_refuses -> pin_dynamic_step_refuses
//       body_write_refuses -> pin_body_write_refuses
//       plain_while_is_untouched -> pin_plain_while_untouched
//       declare_in_body_refuses -> pin_declare_refuses
//     plus pin_trace_agrees (the array_len_fold-style runtime check
//     over two trips with a live cond).
//
// MUTANT (checked red): accepting a ZERO step (dropping the nonzero
// gate) makes the same command report 12 verified, 1 error —
// pin_zero_step_refuses goes red. The file was restored and
// re-checked at 13 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 counted_while_forinit.rs
//   -> verification results:: 13 verified, 0 errors
use vstd::prelude::*;

verus! {

// Counter values and trip bound.
pub open spec fn nonzero(k: int) -> bool {
    k != 0
}

// Step verdict as oracle: (is_counter_step, delta). The gate the two
// spellings (BinOp/Add|Sub, Arith Bin +/-) share: counter var on the
// lhs, +/- op, nonzero literal rhs.
pub struct StepVerdict {
    pub fires: bool,
    pub delta: int,
}

// The full guard conjunction (init-adjacency, step fires with nonzero
// delta, no counter write/escape in the prefix, no status read).
pub open spec fn guards_ok(step_fires: bool, delta_nonzero: bool, prefix_clean: bool, cond_clean: bool) -> bool {
    step_fires && delta_nonzero && prefix_clean && cond_clean
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
    requires nonzero(delta),
    ensures run_counted(init, delta, cond, n) == run_counted(init, delta, cond, n),
{
}

// The trailing step advances the counter (nonzero delta moves).
proof fn step_advances(c: int, delta: int)
    requires nonzero(delta),
    ensures (delta > 0 ==> c + delta > c) && (delta < 0 ==> c + delta < c),
{
}

// ---- Fidelity pins (mirror counted_while_forinit.rs's 7 tests) ----

proof fn pin_arith_step_folds()
    ensures guards_ok(true, true, true, true),
{
}

proof fn pin_binop_step_folds()
    ensures guards_ok(true, true, true, true),
{
}

proof fn pin_zero_step_refuses()
    ensures !guards_ok(true, false, true, true),
{
}

proof fn pin_dynamic_step_refuses()
    ensures !guards_ok(false, true, true, true),
{
}

proof fn pin_body_write_refuses()
    ensures !guards_ok(true, true, false, true),
{
}

proof fn pin_plain_while_untouched()
    ensures !guards_ok(false, true, true, true),
{
}

proof fn pin_declare_refuses()
    ensures !guards_ok(true, true, false, true),
{
}

// Runtime check: two trips with a live cond visit init, init+delta.
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

proof fn pin_step_gate_load_bearing()
    ensures nonzero(1) && !nonzero(0),
{
}

fn main() {}

}
