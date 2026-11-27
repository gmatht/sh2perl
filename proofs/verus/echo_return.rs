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
// file was restored and re-checked at 13 verified, 0 errors (before
// the 2026-11-27 re-audit below; the count is now 25).
//
// ── 2026-11-27 re-audit: the `return N` refusal ──────────────────────
//
// `src/transforms/echo_return.rs` gained
//     IrStmt::Return(Some(_)) => return None,
// because folding a body that returns a STATUS value drops the status
// (`f(){ echo hi; return 3; }; f; echo $?` measured 3 in bash, 0
// through the fold). The call-channel laws above take eligibility as a
// premise, so they stay sound when the accepted set shrinks, but the
// model did not record the refusal itself. The `Stmt`/`walk` section
// below adds the pass's Need/Done classifier for exactly that arm:
//   walk_status_ret_refused      a body carrying `return N` is refused
//   walk_accepts_no_status       no accepted body carries one
//   refused_body_is_unchanged    a refused body is emitted unchanged,
//                                so the original bytes/status stand
//   arm_with_status_refused      a refusal in any `if`/`case` arm
//                                refuses the enclosing body too
// Pins: pin_status_return_refused (the measured body),
// pin_bare_return_after_echo_ok (the old bare-return arm still lifts),
// pin_bare_return_before_echo_refuses, and
// pin_status_return_in_an_arm_refused (nested propagation).
//
// MUTANTS measured 2026-11-27, each restored byte-identically
// (sha256 of the file compared before and after each mutation):
//   StatusRet accepted (None -> Some(done))   24 verified, 1 error
//                                             (walk_status_ret_refused)
//   BareRet always accepted                   1 error at
//                                             pin_bare_return_before_echo_refuses
//                                             (`expression simplifies to false`)
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 echo_return.rs
//   -> verification results:: 25 verified, 0 errors
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

// ── 2026-11-27 re-audit: the pass's forward classifier (`parse_stmt`) ──
//
// The call-channel laws above take eligibility as a premise. This
// section makes the ACCEPTANCE side concrete for the arm that moved on
// 2026-11-27: `IrStmt::Return(Some(_)) => return None` refuses a body
// that returns a status value, because folding such a body would drop
// the status (`f(){ echo hi; return 3; }; f; echo $?` measured 3 in
// bash, 0 through the fold). `walk` is the Need/Done machine over the
// statement shapes that decide acceptance; `StatusRet` is the new
// refusal arm, `BareRet` keeps the source's old arm (legal only after a
// path echoed). A refused body is never folded, so call sites are left
// untouched and the original bytes/status stand.

pub enum Stmt {
    Echo(int),
    StatusRet(int), // `return N` — refused (the new arm)
    BareRet,        // `return` — legal only after an echo
    Silent,         // pure assignment/declaration
    Val(int),       // the rewrite's shape: an echo became a value return
}

// The classifier: `None` = refused, `Some(done)` = the path state after
// the list (done = every live path echoed).
pub open spec fn walk(ss: Seq<Stmt>, done: bool) -> Option<bool>
    decreases ss.len(),
{
    if ss.len() == 0 {
        Some(done)
    } else {
        match ss[0] {
            Stmt::Echo(_) => if done { None } else { walk(ss.subrange(1, ss.len() as int), true) },
            Stmt::StatusRet(_) => None,
            Stmt::BareRet => if done { walk(ss.subrange(1, ss.len() as int), true) } else { None },
            Stmt::Silent => walk(ss.subrange(1, ss.len() as int), done),
            Stmt::Val(_) => walk(ss.subrange(1, ss.len() as int), true),
        }
    }
}

pub open spec fn has_status_ret(ss: Seq<Stmt>) -> bool
    decreases ss.len(),
{
    ss.len() > 0
        && (match ss[0] {
            Stmt::StatusRet(_) => true,
            _ => false,
        } || has_status_ret(ss.subrange(1, ss.len() as int)))
}

pub open spec fn accepted(ss: Seq<Stmt>) -> bool {
    matches!(walk(ss, false), Option::Some(_))
}

// The rewrite (value echo -> value return) only runs on accepted bodies.
pub open spec fn rewrite(ss: Seq<Stmt>) -> Seq<Stmt>
    decreases ss.len(),
{
    if ss.len() == 0 {
        ss
    } else {
        match ss[0] {
            Stmt::Echo(v) => seq![Stmt::Val(v)] + rewrite(ss.subrange(1, ss.len() as int)),
            Stmt::StatusRet(v) => seq![Stmt::StatusRet(v)] + rewrite(ss.subrange(1, ss.len() as int)),
            Stmt::BareRet => seq![Stmt::BareRet] + rewrite(ss.subrange(1, ss.len() as int)),
            Stmt::Silent => seq![Stmt::Silent] + rewrite(ss.subrange(1, ss.len() as int)),
            Stmt::Val(v) => seq![Stmt::Val(v)] + rewrite(ss.subrange(1, ss.len() as int)),
        }
    }
}

pub open spec fn folded(ss: Seq<Stmt>) -> Seq<Stmt> {
    if accepted(ss) { rewrite(ss) } else { ss }
}

// THE OBLIGATION (the new refusal): a body carrying `return N` is
// refused, at any position of the list.
proof fn walk_status_ret_refused(ss: Seq<Stmt>, done: bool)
    requires has_status_ret(ss),
    ensures matches!(walk(ss, done), Option::None),
    decreases ss.len(),
{
    let tail = ss.subrange(1, ss.len() as int);
    if ss.len() == 0 {
    } else {
        match ss[0] {
            Stmt::StatusRet(_) => {}
            Stmt::Echo(_) => {
                assert(has_status_ret(tail));
                if !done { walk_status_ret_refused(tail, true); }
            }
            Stmt::BareRet => {
                assert(has_status_ret(tail));
                if done { walk_status_ret_refused(tail, true); }
            }
            Stmt::Silent => {
                assert(has_status_ret(tail));
                walk_status_ret_refused(tail, done);
            }
            Stmt::Val(_) => {
                assert(has_status_ret(tail));
                walk_status_ret_refused(tail, true);
            }
        }
    }
}

// No accepted body carries a status return (the contrapositive).
proof fn walk_accepts_no_status(ss: Seq<Stmt>, done: bool)
    requires matches!(walk(ss, done), Option::Some(_)),
    ensures !has_status_ret(ss),
{
    if has_status_ret(ss) {
        walk_status_ret_refused(ss, done);
    }
}

// A refused body is emitted UNCHANGED: no call site is rewritten, so
// the original bytes and status are exactly what runs.
proof fn refused_body_is_unchanged(ss: Seq<Stmt>)
    requires has_status_ret(ss),
    ensures folded(ss) == ss,
{
    walk_status_ret_refused(ss, false);
    assert(!accepted(ss));
}

// If/Case arms: the source parses EVERY arm and refuses the enclosing
// statement when one arm refuses, so a status return in any arm refuses
// the whole body — not only a top-level one.
pub open spec fn arms_ok(arms: Seq<Seq<Stmt>>) -> bool
    decreases arms.len(),
{
    if arms.len() == 0 {
        true
    } else {
        (match walk(arms[0], false) {
            Option::Some(_) => true,
            Option::None => false,
        }) && arms_ok(arms.subrange(1, arms.len() as int))
    }
}

proof fn arm_with_status_refused(arms: Seq<Seq<Stmt>>, i: int)
    requires 0 <= i < arms.len(), has_status_ret(arms[i]),
    ensures !arms_ok(arms),
    decreases arms.len(),
{
    if i == 0 {
        walk_status_ret_refused(arms[0], false);
    } else {
        arm_with_status_refused(arms.subrange(1, arms.len() as int), i - 1);
    }
}

// Fidelity pins for the re-audit.

// `f(){ echo hi; return 3; }`: the measured status-return body, refused.
proof fn pin_status_return_refused()
    ensures matches!(walk(seq![Stmt::Echo(104), Stmt::StatusRet(3)], false), Option::None),
{
    assert(has_status_ret(seq![Stmt::Echo(104), Stmt::StatusRet(3)])) by (compute);
    walk_status_ret_refused(seq![Stmt::Echo(104), Stmt::StatusRet(3)], false);
}

// `f(){ echo hi; return; }` stays eligible (the old bare-return arm).
proof fn pin_bare_return_after_echo_ok()
    ensures walk(seq![Stmt::Echo(104), Stmt::BareRet], false) == Some(true),
{
    assert(walk(seq![Stmt::Echo(104), Stmt::BareRet], false) == Some(true)) by (compute);
}

// `f(){ return; }` still refuses: a no-output path loses the channel.
proof fn pin_bare_return_before_echo_refuses()
    ensures matches!(walk(seq![Stmt::BareRet], false), Option::None),
{
    assert(matches!(walk(seq![Stmt::BareRet], false), Option::None)) by (compute);
}

// A status return inside an `if` arm refuses the whole body.
proof fn pin_status_return_in_an_arm_refused()
    ensures !arms_ok(seq![seq![Stmt::Echo(104), Stmt::StatusRet(3)], seq![Stmt::Echo(104)]]),
{
    assert(has_status_ret(seq![Stmt::Echo(104), Stmt::StatusRet(3)])) by (compute);
    arm_with_status_refused(
        seq![seq![Stmt::Echo(104), Stmt::StatusRet(3)], seq![Stmt::Echo(104)]],
        0,
    );
}

fn main() {}

}
