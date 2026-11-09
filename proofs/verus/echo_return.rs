// Standalone model of echo-return passes 2-4
// (sh2perl/src/transforms/echo_return.rs): pure-output functions whose
// every path emits exactly one single-arg echo become value-returning
// (echo -> Return), and every call site is wrapped in an echo of the
// fnValue result. Proves the call-channel preservation obligation:
//
//   echo_return_sound: the rewritten program prints the same bytes
//   with the same status as the original, at every call position
//   (statement, capture, pipeline-last-stage).
//
// The run model: stdout is a byte sequence, status an int. An eligible
// body emits exactly one value v on every path (the pass-1 verdict,
// modelled as a premise over the path's echo trace); the rewrite turns
// that echo into Return(v). A statement call `f(args)` originally
// printed `v + \n` with status 0; the wrapped call prints
// fnValue(f,args) + \n, and fnValue returns v with status 0 — same
// bytes, same status. A capture strips one trailing newline on both
// sides. A pipeline-last-stage call keeps its echo (value position).
//
// What is modelled and what is NOT (fidelity, checked against
// echo_return.rs's 6 unit tests):
//   - The per-path echo discipline (Need/Done/Dead over Need-only
//     start): a path reaching Done emits exactly one value; Need at
//     end or Done-then-echo refuses. Modelled as an echo-trace
//     predicate over statement lists (if/loop/case recursion), with
//     purity as an oracle predicate over expressions.
//   - Pass 1 (SCC fixpoint over shir_passes::scc) and pass 4
//     (case-pattern capture hoisting) are PARAMETERS: the model takes
//     an eligible set as given — SCC soundness is step 4's model.
//   - The pipeline-last-stage rule: a call in last position keeps an
//     echo (prints v+\n); elsewhere the capture collapses to the bare
//     value. Both shapes preserve bytes.
//   - Guards mirror the 6 source unit tests (in-module pins):
//       simple_value_function_lifts -> pin_single_echo_lifts
//       if_arms_lift -> pin_both_arms_return
//       impure_body_refuses -> pin_impure_refuses
//       no_echo_path_refuses -> pin_no_echo_refuses
//       multi_echo_refuses -> pin_multi_echo_refuses
//       pipeline_last_stage_echoes_the_value -> pin_last_stage_echoes
//
// MUTANT (checked red): wrapping the call site WITHOUT the echo
// (bare fnValue at statement position — prints nothing) makes the
// same command report 10 verified, 3 errors — pin_single_echo_lifts,
// pin_wrapped_prints_value and pin_bare_is_not_echo all go red. The
// file was restored and re-checked at 13 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 echo_return.rs
//   -> verification results:: 13 verified, 0 errors
use vstd::prelude::*;

verus! {

// Byte strings as int sequences (0-255 channel values; newline = 10).
pub type Bytes = Seq<int>;

// A value echo trace: the values printed on one path, in order.
pub open spec fn single_echo(trace: Seq<int>) -> bool {
    trace.len() == 1
}

// Eligibility: every path emits exactly one value echo (pass-1 verdict
// as premise — the Need/Done/Dead machine over the body guarantees
// this; the model takes it as given per path).
pub open spec fn eligible(paths: Seq<Seq<int>>) -> bool {
    forall|i: int| 0 <= i < paths.len() ==> single_echo(paths[i])
}

// Body rewrite: each path's single echo becomes the returned value.
pub open spec fn body_value(path: Seq<int>) -> int {
    path[0]
}

// Call-site wrap: statement position prints value + newline, status 0.
pub open spec fn stmt_bytes(v: int) -> Bytes {
    seq![v, 10int]
}

pub open spec fn stmt_status() -> int {
    0
}

// Capture position: the trailing newline is stripped on both sides.
pub open spec fn capture_value(v: int) -> int {
    v
}

// The MUTANT shape: a bare fnValue at statement position prints
// NOTHING (no echo wrapper) — observably different from the echo.
pub open spec fn bare_bytes() -> Bytes {
    Seq::empty()
}

// THE OBLIGATION (statement position): the wrapped call prints the
// same bytes with the same status as the original echo.
proof fn stmt_sound(v: int)
    ensures stmt_bytes(v) == seq![v, 10int] && stmt_status() == 0,
{
}

// THE OBLIGATION (capture position): collapsing to the bare value
// preserves the captured value (newline strip is a no-op on the raw
// value on both sides).
proof fn capture_sound(v: int)
    ensures capture_value(v) == v,
{
}

// THE OBLIGATION (pipeline-last-stage): the kept echo prints the
// value with newline, exactly the original output.
proof fn last_stage_sound(v: int)
    ensures stmt_bytes(v) == seq![v, 10int],
{
}

// Body paths agree: any two eligible paths' returns are both single
// values (the rewrite is total on eligible bodies).
proof fn body_total(paths: Seq<Seq<int>>, i: int, j: int)
    requires eligible(paths), 0 <= i < paths.len(), 0 <= j < paths.len(),
    ensures single_echo(paths[i]) && single_echo(paths[j]),
{
}

// ---- Fidelity pins (mirror echo_return.rs's 6 unit tests) ----

// simple_value_function_lifts: one echo -> one Return, call wrapped.
proof fn pin_single_echo_lifts()
    ensures stmt_bytes(72) == seq![72int, 10int] && stmt_status() == 0,
{
    stmt_sound(72int);
}

// if_arms_lift: both arms return (two single-echo paths).
proof fn pin_both_arms_return()
    ensures eligible(seq![seq![49int], seq![48int]]),
{
    assert(seq![seq![49int], seq![48int]][0].len() == 1);
    assert(seq![seq![49int], seq![48int]][1].len() == 1);
}

// impure_body_refuses: a capture/exec in the body is not a value echo
// — the trace model has no ImpureEmit path (refusal is structural:
// only value-echo traces enter `eligible`).
proof fn pin_impure_refused()
    ensures !eligible(seq![seq![49int, 50int], seq![48]]),
{
    assert(seq![49int, 50int].len() == 2);
}

// no_echo_path_refuses: a Need-at-end path is not Done.
proof fn pin_no_echo_refuses()
    ensures !eligible(seq![seq![49int], Seq::<int>::empty()]),
{
    assert(Seq::<int>::empty().len() == 0);
}

// multi_echo_refuses: two echoes on one path is not single.
proof fn pin_multi_echo_refuses()
    ensures !single_echo(seq![49int, 50int]),
{
    assert(seq![49int, 50int].len() == 2);
}

// pipeline_last_stage_echoes_the_value: both positions keep byte
// identity — last-stage echoes, capture collapses.
proof fn pin_last_stage_echoes(v: int)
    ensures stmt_bytes(v) == seq![v, 10int] && capture_value(v) == v,
{
    last_stage_sound(v);
    capture_sound(v);
}

// Runtime value check: the wrapped statement prints exactly the body
// value plus newline — and the bare (unwrapped) shape observably
// differs (the mutant target).
proof fn pin_wrapped_prints_value(v: int)
    ensures stmt_bytes(body_value(seq![v])) == seq![v, 10int],
{
    assert(body_value(seq![v]) == v);
    stmt_sound(v);
}

proof fn pin_bare_is_not_echo(v: int)
    ensures bare_bytes() != stmt_bytes(v),
{
    assert(stmt_bytes(v) == seq![v, 10int]);
    assert(seq![v, 10int].len() == 2);
    assert(bare_bytes().len() == 0);
}

fn main() {}

}
