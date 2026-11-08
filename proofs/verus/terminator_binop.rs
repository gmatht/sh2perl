// Standalone model of `terminator_binop`
// (sh2perl/src/transforms/terminator_binop.rs): when the LEFT operand of
// `&&`/`||` is a control call (`break`, `continue`, `exit`, `die`,
// `return`), the right operand can never run, so
// `Expr(BinOp{op: And|Or, lhs: <control call>, rhs})` folds to
// `Expr(<control call>)` — the statement keeps the control call's own
// status and the renderer's `break`/`continue`/`exit` arms take over.
//
// THE OBLIGATIONS
//   bin_control_dead:  with a control LHS, the BinOp statement and the
//                      bare LHS statement have the same run — trace,
//                      status and abort flag — for either operator, any
//                      RHS, and ANY status the control call reports.
//   fold_stmt_sound:   the per-statement rewrite preserves the step.
//   fold_list_sound:   the per-list rewrite preserves the run, by
//                      induction over the list. The transform's body
//                      recursion is this theorem instantiated at the
//                      body list — the fold is per-list.
//   capture_and_fold_unsound: in CAPTURE position the same fold LOSES
//                      output: `x=$(break && echo RHS)` prints RHS
//                      (measured), so folding `&&` there would drop it.
//                      That is why the transform only visits a
//                      statement-position BinOp.
//
// BASH FACTS THE MODEL ENCODES (measured on this box, bash 5.2):
//   while true; do break 99; done; echo $?     -> 0      (the control
//                                                         status is not
//                                                         the operator's
//                                                         business)
//   while true; do break && echo RHS; done     -> no RHS
//   while true; do break || echo RHS; done     -> no RHS
//   exit 3 && echo RHS; echo $?                -> no RHS, rc=3
//   f() { return 2 || echo RHS; }; f           -> no RHS, rc=2
//   x=$(break && echo RHS); echo "[$x]"        -> "[RHS]"  (subshell!)
//   x=$(break || echo RHS); echo "[$x]"        -> "[]"     (break is 0)
//   i=0; while [ $i -lt 1 ]; do i=1; if true; then break; fi; echo TAIL; done
//                                              -> no TAIL (abort crosses If)
//
// The status is a parameter of the control call precisely because the
// theorem must not depend on it: the operator is never consulted, so
// `break` (0), `exit 3` (3) and `return 2` (2) all fold.
//
// WHAT IS MODELLED AND WHAT IS NOT
//   - Statement position vs capture position is the `subshell` flag. In
//     a subshell a control word is a SUBSHELL's business (bash warns
//     "break: only meaningful in a `for', `while', or `until' loop");
//     it is a no-op with status 0 and control does NOT escape.
//   - Bodies: `If`/`Nested` exist so the abort flag is visible crossing
//     a body boundary (pin_abort_through_if). The transform's body
//     recursion is `fold_list_sound` applied to each body list — a body
//     is just another list, so the recursion adds no obligation.
//   - Name recognition (`control_call_name`) is modelled as data: the
//     bare spelling and the `builtin`/`exec` wrapper both resolve to a
//     control word, and a non-static word (`"$cmd" || …`) is refused.
//     That is a lookup, not a value theorem — pinned, not proved.
//   - The emitted targets are not modelled: the obligation is that the
//     rewrite preserves the shell run, not that a renderer's `break`
//     arm emits the right bytes (renderer obligations live in
//     C_UB_PROOF.md and the per-backend models).
//   - `ran` distinguishes "this run emitted/decided something" from an
//     empty tail, so a list's final status is its last real step's.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 terminator_binop.rs
use vstd::prelude::*;

verus! {

/// Printed values, in order (one int per emitted word).
pub type Trace = Seq<int>;

/// An operand. `Control(s)` is a `break`/`continue`/`exit`/`die`/`return`
/// call; `s` is the status it reports at statement position.
pub enum Expr {
    /// ordinary command that succeeds: prints its id, status 0
    Cmd(int),
    /// ordinary command that fails: prints its id, status 1
    Fail(int),
    /// control call: transfers control out of the list
    Control(int),
}

pub enum Stmt {
    Plain(Expr),
    Bin { lhs: Expr, is_and: bool, rhs: Expr },
    If { cond: bool, then: Seq<Stmt>, els: Seq<Stmt> },
    /// a body run as a unit (Block/loop body): the abort flag must cross
    /// this boundary
    Nested(Seq<Stmt>),
}

pub struct Out {
    pub trace: Trace,
    pub status: int,
    pub aborted: bool,
    /// did any step in this run actually happen?
    pub ran: bool,
}

pub open spec fn is_control(e: Expr) -> bool {
    match e {
        Expr::Control(_) => true,
        _ => false,
    }
}

/// `pre` then `post`: traces concatenate; the status is the last step's,
/// and an empty tail cannot clobber it.
pub open spec fn merge(pre: Out, post: Out) -> Out {
    Out {
        trace: pre.trace.add(post.trace),
        status: if post.ran {
            post.status
        } else {
            pre.status
        },
        aborted: post.aborted,
        ran: pre.ran || post.ran,
    }
}

pub open spec fn empty_out() -> Out {
    Out { trace: Seq::empty(), status: 0, aborted: false, ran: false }
}

/// Evaluate one operand. `subshell` is the position flag: inside `$( )` a
/// control word is a no-op with status 0 and does not abort.
pub open spec fn ev(e: Expr, subshell: bool) -> Out {
    match e {
        Expr::Cmd(id) => Out { trace: seq![id], status: 0, aborted: false, ran: true },
        Expr::Fail(id) => Out { trace: seq![id], status: 1, aborted: false, ran: true },
        Expr::Control(s) => if subshell {
            Out { trace: Seq::empty(), status: 0, aborted: false, ran: true }
        } else {
            Out { trace: Seq::empty(), status: s, aborted: true, ran: true }
        },
    }
}

pub open spec fn stmt_size(s: Stmt) -> nat
    decreases s,
{
    match s {
        Stmt::Plain(_) => 1,
        Stmt::Bin { .. } => 1,
        Stmt::If { then, els, .. } => 1 + seq_size(then) + seq_size(els),
        Stmt::Nested(body) => 1 + seq_size(body),
    }
}

pub open spec fn seq_size(p: Seq<Stmt>) -> nat
    decreases p,
{
    if p.len() == 0 {
        0
    } else {
        stmt_size(p[0]) + seq_size(p.drop_first()) + 1
    }
}

pub open spec fn step(s: Stmt, subshell: bool) -> Out
    decreases stmt_size(s),
{
    match s {
        Stmt::Plain(e) => ev(e, subshell),
        // `&&` runs rhs iff lhs status is 0; `||` iff it is non-zero. An
        // aborting lhs never produces a status for the operator to test.
        Stmt::Bin { lhs, is_and, rhs } => {
            let l = ev(lhs, subshell);
            if l.aborted {
                l
            } else if is_and {
                if l.status == 0 {
                    merge(l, ev(rhs, subshell))
                } else {
                    l
                }
            } else if l.status != 0 {
                merge(l, ev(rhs, subshell))
            } else {
                l
            }
        },
        Stmt::If { cond, then, els } => if cond { run_list(then) } else { run_list(els) },
        Stmt::Nested(body) => run_list(body),
    }
}

/// Run a statement list. Once a step aborts, the tail is skipped — the
/// vocabulary the other termination folds reuse.
pub open spec fn run_list(p: Seq<Stmt>) -> Out
    decreases seq_size(p),
{
    if p.len() == 0 {
        empty_out()
    } else {
        let h = step(p[0], false);
        if h.aborted {
            h
        } else {
            merge(h, run_list(p.drop_first()))
        }
    }
}

// ---- the fold ----

pub open spec fn fold_stmt(s: Stmt) -> Stmt {
    match s {
        Stmt::Bin { lhs, is_and, rhs } => if is_control(lhs) {
            Stmt::Plain(lhs)
        } else {
            Stmt::Bin { lhs, is_and, rhs }
        },
        _ => s,
    }
}

pub open spec fn fold_list(p: Seq<Stmt>) -> Seq<Stmt>
    decreases p.len(),
{
    if p.len() == 0 {
        Seq::empty()
    } else {
        seq![fold_stmt(p[0])].add(fold_list(p.drop_first()))
    }
}

// ---- THE OBLIGATIONS ----

/// THE MAIN THEOREM: with a control left operand, the BinOp statement and
/// the bare left operand run identically — for both operators, any right
/// operand, and any status the control call reports.
proof fn bin_control_dead(lhs: Expr, is_and: bool, rhs: Expr)
    requires
        is_control(lhs),
    ensures
        step(Stmt::Bin { lhs, is_and, rhs }, false) == step(Stmt::Plain(lhs), false),
{
    reveal_with_fuel(step, 2);
}

proof fn fold_stmt_sound(s: Stmt)
    ensures
        step(fold_stmt(s), false) == step(s, false),
{
    match s {
        Stmt::Bin { lhs, is_and, rhs } => {
            if is_control(lhs) {
                bin_control_dead(lhs, is_and, rhs);
                assert(fold_stmt(s) == Stmt::Plain(lhs));
            } else {
                assert(fold_stmt(s) == s);
            }
        },
        _ => {
            assert(fold_stmt(s) == s);
        },
    }
}

/// Unfold `run_list` at its head.
proof fn run_list_cons(h: Stmt, t: Seq<Stmt>, hh: Out)
    requires
        hh == step(h, false),
    ensures
        run_list(seq![h].add(t)) == if hh.aborted {
            hh
        } else {
            merge(hh, run_list(t))
        },
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
    reveal_with_fuel(run_list, 4);
    reveal_with_fuel(seq_size, 4);
}

/// THE LIST THEOREM: folding a list preserves its run.
proof fn fold_list_sound(p: Seq<Stmt>)
    ensures
        run_list(fold_list(p)) == run_list(p),
    decreases p.len(),
{
    if p.len() == 0 {
        reveal_with_fuel(fold_list, 2);
        reveal_with_fuel(run_list, 2);
    } else {
        let h = p[0];
        let t = p.drop_first();
        reveal_with_fuel(fold_list, 2);
        assert(fold_list(p) == seq![fold_stmt(h)].add(fold_list(t)));
        fold_stmt_sound(h);
        fold_list_sound(t);
        let hh = step(h, false);
        assert(step(fold_stmt(h), false) == hh);
        run_list_cons(h, t, hh);
        run_list_cons(fold_stmt(h), fold_list(t), hh);
        if hh.aborted {
            assert(run_list(fold_list(p)) == hh);
            assert(run_list(p) == hh);
        } else {
            assert(run_list(fold_list(p)) == merge(hh, run_list(fold_list(t))));
            assert(run_list(p) == merge(hh, run_list(t)));
        }
    }
}

/// THE REFUSAL: in capture position the same fold loses output.
/// `x=$(break && echo RHS)` prints RHS (measured), so the fold must be
/// statement-position only.
proof fn capture_and_fold_unsound(k: int)
    ensures
        step(Stmt::Bin { lhs: Expr::Control(0), is_and: true, rhs: Expr::Cmd(k) }, true).trace
            == seq![k],
        step(Stmt::Plain(Expr::Control(0)), true).trace == Seq::<int>::empty(),
{
    reveal_with_fuel(step, 2);
}

// ---- name recognition (`control_call_name`) ----

/// The control words: break, continue, exit, die, return.
pub open spec fn is_control_word(w: int) -> bool {
    0 <= w <= 4
}

/// The bare spelling `Call{func: "break"}`.
pub open spec fn bare_is_control(f: int) -> bool {
    is_control_word(f)
}

/// The lifted spelling `builtin("exit", words)` / `exec("exit", words)`:
/// the command word sits in the first argument, so it must be static
/// (0 = builtin wrapper, 1 = exec wrapper).
pub open spec fn wrapped_is_control(wrapper: int, word: Option<int>) -> bool {
    (wrapper == 0 || wrapper == 1) && match word {
        Some(w) => is_control_word(w),
        None => false,
    }
}

// ---- fidelity pins (mirror the source's 7 unit tests) ----

/// or_after_break_collapses_to_the_break: `break || exit 1` prints
/// nothing, keeps status 0, aborts — and the fold is transparent.
proof fn pin_or_break_drops_rhs()
    ensures
        run_list(seq![Stmt::Bin { lhs: Expr::Control(0), is_and: false, rhs: Expr::Cmd(7) }]).trace
            == Seq::<int>::empty(),
        run_list(seq![Stmt::Bin { lhs: Expr::Control(0), is_and: false, rhs: Expr::Cmd(7) }]).aborted,
        run_list(seq![Stmt::Bin { lhs: Expr::Control(0), is_and: false, rhs: Expr::Cmd(7) }]).status
            == 0,
        run_list(fold_list(seq![Stmt::Bin { lhs: Expr::Control(0), is_and: false, rhs: Expr::Cmd(7) }]))
            == run_list(seq![Stmt::Bin { lhs: Expr::Control(0), is_and: false, rhs: Expr::Cmd(7) }]),
{
    reveal_with_fuel(run_list, 6);
    reveal_with_fuel(fold_list, 4);
    reveal_with_fuel(seq_size, 6);
    reveal_with_fuel(step, 2);
}

/// or_after_a_lifted_break_collapses: the wrapper spelling resolves, and
/// a non-static word or a non-lift wrapper does not.
proof fn pin_lifted_break_recognised()
    ensures
        bare_is_control(0),
        wrapped_is_control(0, Some(0)),
        !wrapped_is_control(0, None),
        !wrapped_is_control(3, Some(0)),
{
}

/// or_after_exit_collapses: `exit 3 || …` keeps status 3 and aborts.
proof fn pin_exit_status_kept()
    ensures
        run_list(seq![Stmt::Bin { lhs: Expr::Control(3), is_and: false, rhs: Expr::Cmd(7) }]).status
            == 3,
        run_list(seq![Stmt::Plain(Expr::Control(3)), Stmt::Plain(Expr::Cmd(9))]).trace
            == Seq::<int>::empty(),
        run_list(seq![Stmt::Plain(Expr::Control(3))]).aborted,
{
    reveal_with_fuel(run_list, 6);
    reveal_with_fuel(seq_size, 6);
    reveal_with_fuel(step, 2);
}

/// or_after_an_ordinary_command_is_kept: the fold does NOT fire, and the
/// right operand really runs.
proof fn pin_ordinary_guard_kept()
    ensures
        fold_stmt(Stmt::Bin { lhs: Expr::Fail(1), is_and: false, rhs: Expr::Cmd(7) })
            == (Stmt::Bin { lhs: Expr::Fail(1), is_and: false, rhs: Expr::Cmd(7) }),
        run_list(seq![Stmt::Bin { lhs: Expr::Fail(1), is_and: false, rhs: Expr::Cmd(7) }]).trace
            == seq![1int, 7int],
{
    reveal_with_fuel(run_list, 6);
    reveal_with_fuel(step, 2);
}

/// and_after_a_control_call_collapses: `&&` is dead in the same way.
proof fn pin_and_control_drops_rhs()
    ensures
        run_list(seq![Stmt::Bin { lhs: Expr::Control(0), is_and: true, rhs: Expr::Cmd(7) }]).trace
            == Seq::<int>::empty(),
        run_list(seq![Stmt::Bin { lhs: Expr::Control(0), is_and: true, rhs: Expr::Cmd(7) }]).aborted,
{
    reveal_with_fuel(run_list, 6);
    reveal_with_fuel(step, 2);
}

/// the abort flag crosses a body boundary: `if true; then break; fi; echo
/// TAIL` never prints TAIL.
proof fn pin_abort_through_if()
    ensures
        run_list(
            seq![
                Stmt::If {
                    cond: true,
                    then: seq![Stmt::Plain(Expr::Control(0))],
                    els: Seq::<Stmt>::empty(),
                },
                Stmt::Plain(Expr::Cmd(9)),
            ],
        ).trace == Seq::<int>::empty(),
{
    reveal_with_fuel(run_list, 8);
    reveal_with_fuel(seq_size, 8);
    reveal_with_fuel(step, 3);
}

/// a_nested_binop_is_left_alone: in capture position the right operand
/// runs, so folding there would drop it — the refutation, instantiated.
proof fn pin_capture_right_operand_runs()
    ensures
        step(Stmt::Bin { lhs: Expr::Control(0), is_and: true, rhs: Expr::Cmd(7) }, true).trace
            == seq![7int],
{
    capture_and_fold_unsound(7);
}

/// a_loop_body_is_recurred_into: a body is just another list, so the
/// transform's body recursion is this theorem at the body.
proof fn pin_body_is_a_list(body: Seq<Stmt>)
    ensures
        run_list(fold_list(body)) == run_list(body),
{
    fold_list_sound(body);
}

fn main() {}

}
