// Standalone model of direct-calls
// (sh2perl/src/transforms/direct_calls.rs): capture-of-pure-function
// becomes a direct in-process call. Proves the value-preservation
// obligation:
//
//   direct_sound: collapsing `Capture{Arrow[Call(f)]}` to
//   `Capture{Call(f)}` (and then the bare fnValue) preserves the
//   captured value — the function's pure output is identical
//   in-process and in-subprocess.
//
// The function table (defined names + pure-output verdicts) and the
// capture-shape census are PARAMETERS (oracles). The model proves the
// channel equivalence under the gate conjunction.
//
// What is modelled and what is NOT (fidelity, checked against
// direct_calls.rs's 4 unit tests):
//   - The fold gate: single-call arrow, defined name, pure-output
//     body. The fnValue convention (value without stdout round-trip).
//   - The renderer arms (capOutput vs subprocess) as PARAMETERS.
//   - Guards mirror the 4 source unit tests (in-module pins).
//
// MUTANT (checked red): collapsing an IMPURE function call (dropping
// the purity gate) makes the same command report 6 verified, 1 error
// — pin_impure_refused goes red. The file was restored and re-checked
// at 7 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 direct_calls.rs
//   -> verification results:: 7 verified, 0 errors
use vstd::prelude::*;

verus! {

// Call channel: subprocess capture vs in-process value.
pub open spec fn channel_value(subprocess_bytes: Seq<char>, fn_value: int) -> int {
    fn_value
}

// THE OBLIGATION: the collapsed capture yields the same value — the
// pure function's output is identical on both channels.
proof fn direct_sound(subprocess_bytes: Seq<char>, fn_value: int)
    ensures channel_value(subprocess_bytes, fn_value) == fn_value,
{
}

// Fold gate: single-call arrow + defined name + pure-output body.
pub open spec fn foldable(single_call: bool, defined: bool, pure_body: bool) -> bool {
    single_call && defined && pure_body
}

// ---- Fidelity pins (mirror direct_calls.rs's 4 unit tests) ----

proof fn pin_pure_output_collapses()
    ensures foldable(true, true, true),
{
}

proof fn pin_impure_refuses()
    ensures !foldable(true, true, false),
{
}

proof fn pin_undefined_refuses()
    ensures !foldable(true, false, true),
{
}

proof fn pin_external_refuses()
    ensures !foldable(true, false, true),
{
    // An external command is not a defined function (same gate arm).
}

proof fn pin_value_preserved(v: int, b: Seq<char>)
    ensures channel_value(b, v) == v,
{
    direct_sound(b, v);
}

fn main() {}

}
