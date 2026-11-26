// ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/uae_terminators.rs — `unreachable_after_exit`'s OTHER four
//  terminator kinds (the direct `Exit` arm is proved in elim_trio.rs).
//
//  `shir.rs::stmt_terminates` is shared with the renderers, and the pass
//  drops every statement after a statement the verdict calls a terminator.
//  The verdict's arms, and the run each one has to be sound for:
//
//    Exit / Break / Continue   the list really stops there
//    Return                    … but ONLY inside a function body (`in_fn`):
//                              at top level bash prints an error and keeps
//                              going, so the tail still runs
//    While/ForInit             a CONST-TRUE condition and NO escaping break:
//                              the loop never falls through
//    anything else             not a terminator (an `if`/`case` arm may not
//                              run, so a terminator nested in it deadens
//                              nothing after the ENCLOSING statement)
//
//  The laws:
//
//    trunc_list_sound   truncating a list at its first verdict-terminator
//                       preserves the observed trace AND the control signal,
//                       for every starting state and every list
//    trunc_stmt_sound   the same under the per-statement recursion the pass
//                       performs (so a nested body is truncated too)
//    loop_without_escape_never_falls_through
//                       a break-free constant-true loop contributes nothing
//                       after it: the tail's prints never appear
//    return_truncates_only_a_function_body
//                       `Return` is a terminator exactly when `in_fn`
//    break_truncates_its_own_list / continue_… / exit_…
//                       the three signal-terminators, as instances
//
//  ── the run model ──────────────────────────────────────────────────────
//
//  (trace, signal): `trace` is what the program prints, `signal` is how the
//  statement list ends — None (falls through), Brk, Cont, Ret, Exit, or
//  Spin. `Spin` is the loop arm's outcome for "not finished": a constant-
//  true loop whose body completes (or `continue`s) starts another iteration
//  instead of falling through. The model unrolls the body ONCE and returns
//  Spin — enough for the reachability claim this pass makes (that nothing
//  after the loop is reached), not a full loop semantics; the same
//  single-unroll modelling elim_trio.rs uses.
//
//  What is modelled and what is NOT:
//   * `cond_const`/`cond` are the source's `is_true_cond` (`IrExpr::Bool(true)`),
//     so a `while (( 1 ))` is OUT of this model — its verdict is the
//     separate `const_true_like` question in the ESTree pass.
//   * `Case { fires, body }`: `fires` is whether the clause matches on this
//     run. That is the whole point of the refusal: a `case`-nested `exit`
//     only aborts when a clause matches, so the enclosing statement is not a
//     terminator (`pin_case_exit_may_not_fire`).
//   * `BreakCall` models `break || exit 1`, where `break` arrives as a
//     CONTROL CALL inside a `BinOp` rather than as `IrStmt::Break`
//     (`pin_break_call_escapes`).
//   * Not modelled, stated: the pass's `changed` bookkeeping and the
//     `named_blocks` rebuilding (a separate bug class, pinned by the
//     source's own `a_functions_named_blocks_survive`); the renderer's
//     `stmt_terminates` consumers (they read the same verdict, so a verdict
//     change is the same theorem); `Die`, which this model folds into Exit.
//
//  ── measured mutants (2026-11-26; all restored byte-identically, sha in the
//     sync file) ──
//
//  baseline 31 verified, 0 errors
//
//  counted (a verification failure, after the mutation compiles):
//    m2  truncate after a terminator nested in an `if`      30 verified, 1 error
//        (`trunc_sound`)
//    m3  treat a top-level `Return` as a terminator         29 verified, 2 errors
//        (`trunc_sound`, `return_truncates_only_a_function_body`)
//    m4  truncate after a `case`-nested `Exit`              30 verified, 1 error
//        (`trunc_sound`)
//
//  refuted one stage EARLIER, at simplification, by a concrete pin (the
//  computed value is false, so the mutated model never reaches a proof):
//    m1  count a nested loop's `break` as escaping its parent
//        (pin_nested_break_does_not_escape)
//    m5  drop the terminator itself from the kept prefix (pin_trunc_shapes)
//    m6  let a `case` break `continue` instead of ending the loop
//        (pin_case_break_escapes)
//    m7  run past a non-None signal (pin_break_stops_the_list)
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub enum Sig {
    None,
    Brk,
    Cont,
    Ret,
    Exit,
    /// the loop did not finish (another iteration would start)
    Spin,
}

pub enum Stmt {
    Print(int),
    Exit,
    Return,
    Break,
    Continue,
    /// `break || exit 1`: the control-call spelling
    BreakCall,
    /// a CONST condition selects the branch
    If { cond: bool, then: Seq<Stmt>, els: Seq<Stmt> },
    /// constant-true loop (`cond_const && cond`) or a loop that falls through
    While { cond_const: bool, cond: bool, body: Seq<Stmt> },
    ForS { cond_const: bool, cond: bool, body: Seq<Stmt> },
    /// `fires` = this clause matches on this run
    Case { fires: bool, body: Seq<Stmt> },
    Block(Seq<Stmt>),
}

pub open spec fn ssize(st: Stmt) -> nat
    decreases st,
{
    match st {
        Stmt::Print(_) => 1,
        Stmt::Exit => 1,
        Stmt::Return => 1,
        Stmt::Break => 1,
        Stmt::Continue => 1,
        Stmt::BreakCall => 1,
        Stmt::If { then, els, .. } => 2 + lsize(then) + lsize(els),
        Stmt::While { body, .. } => 2 + lsize(body),
        Stmt::ForS { body, .. } => 2 + lsize(body),
        Stmt::Case { body, .. } => 2 + lsize(body),
        Stmt::Block(b) => 2 + lsize(b),
    }
}

pub open spec fn lsize(l: Seq<Stmt>) -> nat
    decreases l,
{
    if l.len() == 0 {
        0
    } else {
        ssize(l[0]) + lsize(l.drop_first()) + 1
    }
}

// ─────────────────────────────────────────────────────────────── the run ──

pub open spec fn step(st: Stmt, in_fn: bool) -> (Seq<int>, Sig)
    decreases ssize(st),
{
    match st {
        Stmt::Print(v) => (seq![v], Sig::None),
        Stmt::Exit => (Seq::empty(), Sig::Exit),
        // bash: `return` outside a function is an error and the script
        // continues — the statement is NOT a terminator there
        Stmt::Return => if in_fn { (Seq::empty(), Sig::Ret) } else { (Seq::empty(), Sig::None) },
        Stmt::Break => (Seq::empty(), Sig::Brk),
        Stmt::Continue => (Seq::empty(), Sig::Cont),
        Stmt::BreakCall => (Seq::empty(), Sig::Brk),
        Stmt::If { cond, then, els } => run(if cond { then } else { els }, in_fn),
        Stmt::While { cond_const, cond, body } => {
            if cond_const && cond {
                // one unroll: break -> fall through, continue/complete ->
                // another iteration (Spin), return/exit -> propagate
                let r = run(body, in_fn);
                match r.1 {
                    Sig::Brk => (r.0, Sig::None),
                    Sig::Cont => (r.0, Sig::Spin),
                    Sig::None => (r.0, Sig::Spin),
                    Sig::Spin => (r.0, Sig::Spin),
                    Sig::Ret => (r.0, Sig::Ret),
                    Sig::Exit => (r.0, Sig::Exit),
                }
            } else {
                (Seq::empty(), Sig::None)
            }
        }
        Stmt::ForS { cond_const, cond, body } => {
            if cond_const && cond {
                let r = run(body, in_fn);
                match r.1 {
                    Sig::Brk => (r.0, Sig::None),
                    Sig::Cont => (r.0, Sig::Spin),
                    Sig::None => (r.0, Sig::Spin),
                    Sig::Spin => (r.0, Sig::Spin),
                    Sig::Ret => (r.0, Sig::Ret),
                    Sig::Exit => (r.0, Sig::Exit),
                }
            } else {
                (Seq::empty(), Sig::None)
            }
        }
        Stmt::Case { fires, body } => {
            if fires {
                run(body, in_fn)
            } else {
                (Seq::empty(), Sig::None)
            }
        }
        Stmt::Block(b) => run(b, in_fn),
    }
}

pub open spec fn run(l: Seq<Stmt>, in_fn: bool) -> (Seq<int>, Sig)
    decreases lsize(l),
{
    if l.len() == 0 {
        (Seq::empty(), Sig::None)
    } else {
        let h = step(l[0], in_fn);
        if h.1 != Sig::None {
            // the list stops: nothing after this statement runs
            (h.0, h.1)
        } else {
            let t = run(l.drop_first(), in_fn);
            (h.0.add(t.0), t.1)
        }
    }
}

// ─────────────────────────────────────────────────────── the verdicts ─────

/// `shir.rs::stmt_terminates`, transcribed.
pub open spec fn terminates(st: Stmt, in_fn: bool) -> bool {
    match st {
        Stmt::Exit => true,
        Stmt::Break => true,
        Stmt::Continue => true,
        Stmt::BreakCall => true,
        Stmt::Return => in_fn,
        Stmt::While { cond_const, cond, body } => cond_const && cond && !escapes(body),
        Stmt::ForS { cond_const, cond, body } => cond_const && cond && !escapes(body),
        _ => false,
    }
}

/// `shir.rs::loop_escapes` / `has_break`: a `break` that belongs to THIS loop.
pub open spec fn escapes(l: Seq<Stmt>) -> bool
    decreases lsize(l),
{
    if l.len() == 0 {
        false
    } else {
        esc_one(l[0]) || escapes(l.drop_first())
    }
}

pub open spec fn esc_one(st: Stmt) -> bool
    decreases ssize(st),
{
    match st {
        Stmt::Break => true,
        Stmt::BreakCall => true,
        // a nested loop owns its own breaks
        Stmt::While { .. } => false,
        Stmt::ForS { .. } => false,
        Stmt::If { then, els, .. } => escapes(then) || escapes(els),
        Stmt::Case { body, .. } => escapes(body),
        Stmt::Block(b) => escapes(b),
        _ => false,
    }
}

/// `list_terminates`: only the LAST statement decides.
pub open spec fn list_terminates(l: Seq<Stmt>, in_fn: bool) -> bool {
    l.len() > 0 && terminates(l[l.len() - 1], in_fn)
}

/// The transform: keep statements until the first verdict-terminator.
pub open spec fn trunc(l: Seq<Stmt>, in_fn: bool) -> Seq<Stmt>
    decreases lsize(l),
{
    if l.len() == 0 {
        Seq::empty()
    } else if terminates(l[0], in_fn) {
        seq![l[0]]
    } else {
        seq![trunc_stmt(l[0], in_fn)].add(trunc(l.drop_first(), in_fn))
    }
}

/// The per-statement recursion the pass performs.
pub open spec fn trunc_stmt(st: Stmt, in_fn: bool) -> Stmt
    decreases ssize(st),
{
    match st {
        Stmt::If { cond, then, els } => Stmt::If {
            cond,
            then: trunc(then, in_fn),
            els: trunc(els, in_fn),
        },
        Stmt::While { cond_const, cond, body } => Stmt::While {
            cond_const,
            cond,
            body: trunc(body, in_fn),
        },
        Stmt::ForS { cond_const, cond, body } => Stmt::ForS {
            cond_const,
            cond,
            body: trunc(body, in_fn),
        },
        Stmt::Case { fires, body } => Stmt::Case { fires, body: trunc(body, in_fn) },
        Stmt::Block(b) => Stmt::Block(trunc(b, in_fn)),
        other => other,
    }
}

// ─────────────────────────────────────────────── the run of a truncated list ──

pub proof fn step_trunc(st: Stmt, in_fn: bool)
    ensures step(trunc_stmt(st, in_fn), in_fn) == step(st, in_fn),
    decreases ssize(st),
{
    reveal_with_fuel(step, 2);
    reveal_with_fuel(trunc_stmt, 2);
    reveal_with_fuel(trunc, 2);
    reveal_with_fuel(run, 2);
    match st {
        Stmt::Print(_) => {}
        Stmt::Exit => {}
        Stmt::Return => {}
        Stmt::Break => {}
        Stmt::Continue => {}
        Stmt::BreakCall => {}
        Stmt::If { cond, then, els } => {
            trunc_sound(if cond { then } else { els }, in_fn);
        }
        Stmt::While { cond_const, cond, body } => {
            trunc_sound(body, in_fn);
        }
        Stmt::ForS { cond_const, cond, body } => {
            trunc_sound(body, in_fn);
        }
        Stmt::Case { fires, body } => {
            trunc_sound(body, in_fn);
        }
        Stmt::Block(b) => {
            trunc_sound(b, in_fn);
        }
    }
}

/// `run` of a list whose head stops the run is the head's step.
proof fn run_cons_stop(h: Stmt, t: Seq<Stmt>, in_fn: bool)
    requires step(h, in_fn).1 != Sig::None,
    ensures run(seq![h].add(t), in_fn) == step(h, in_fn),
{
    reveal_with_fuel(run, 2);
    reveal_with_fuel(step, 2);
    assert(seq![h].add(t).len() > 0);
    assert(seq![h].add(t)[0] == h);
}

/// `run` of a list whose head falls through continues into the tail.
proof fn run_cons_go(h: Stmt, t: Seq<Stmt>, in_fn: bool)
    requires step(h, in_fn).1 == Sig::None,
    ensures run(seq![h].add(t), in_fn)
        == (step(h, in_fn).0.add(run(t, in_fn).0), run(t, in_fn).1),
{
    reveal_with_fuel(run, 2);
    reveal_with_fuel(step, 2);
    assert(seq![h].add(t).len() > 0);
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

/// THE OBLIGATION: truncating at the verdict's terminators preserves the run.
pub proof fn trunc_sound(l: Seq<Stmt>, in_fn: bool)
    ensures run(trunc(l, in_fn), in_fn) == run(l, in_fn),
    decreases lsize(l),
{
    reveal_with_fuel(run, 2);
    reveal_with_fuel(trunc, 2);
    reveal_with_fuel(step, 2);
    if l.len() == 0 {
    } else {
        let head = l[0];
        let tail = l.drop_first();
        assert(seq![head].add(tail) =~= l);
        step_trunc(head, in_fn);
        if terminates(head, in_fn) {
            // the head really is a terminator: the run stops at it, so the
            // tail was never reachable
            assert(trunc(l, in_fn) == seq![head]);
            terminates_stops(head, in_fn);
            run_cons_stop(head, tail, in_fn);
            run_cons_stop(head, Seq::<Stmt>::empty(), in_fn);
            assert(run(seq![head], in_fn) == step(head, in_fn));
            assert(run(l, in_fn) == step(head, in_fn));
        } else {
            let h = step(head, in_fn);
            assert(trunc(l, in_fn) == seq![trunc_stmt(head, in_fn)].add(trunc(tail, in_fn)));
            if h.1 != Sig::None {
                // not a terminator, but this run still stops here (e.g. a
                // loop that spins, or an arm that exits): both sides stop.
                assert(step(trunc_stmt(head, in_fn), in_fn) == h);
                run_cons_stop(head, tail, in_fn);
                run_cons_stop(trunc_stmt(head, in_fn), trunc(tail, in_fn), in_fn);
                assert(run(l, in_fn) == (h.0, h.1));
            } else {
                trunc_sound(tail, in_fn);
                run_cons_go(head, tail, in_fn);
                run_cons_go(trunc_stmt(head, in_fn), trunc(tail, in_fn), in_fn);
                assert(run(trunc(l, in_fn), in_fn)
                    == (h.0.add(run(trunc(tail, in_fn), in_fn).0), run(trunc(tail, in_fn), in_fn).1));
                assert(run(l, in_fn) == (h.0.add(run(tail, in_fn).0), run(tail, in_fn).1));
            }
        }
    }
}

/// A verdict-terminator stops the run: its step is non-None.
pub proof fn terminates_stops(st: Stmt, in_fn: bool)
    requires terminates(st, in_fn),
    ensures step(st, in_fn).1 != Sig::None,
    decreases ssize(st),
{
    reveal_with_fuel(step, 2);
    match st {
        Stmt::Exit => {}
        Stmt::Break => {}
        Stmt::Continue => {}
        Stmt::BreakCall => {}
        Stmt::Return => {
            assert(in_fn);
        }
        Stmt::While { cond_const, cond, body } => {
            assert(cond_const && cond);
            assert(!escapes(body));
            // no escaping break, so the body cannot end in Brk
            no_escape_no_brk(body, in_fn);
            let r = run(body, in_fn);
            assert(r.1 != Sig::Brk);
        }
        Stmt::ForS { cond_const, cond, body } => {
            assert(cond_const && cond);
            assert(!escapes(body));
            no_escape_no_brk(body, in_fn);
            let r = run(body, in_fn);
            assert(r.1 != Sig::Brk);
        }
        _ => {}
    }
}

/// With no escaping break, a body's run never ends in Brk (nor Spin-as-Brk):
/// the loop cannot fall through.
pub proof fn no_escape_no_brk(l: Seq<Stmt>, in_fn: bool)
    requires !escapes(l),
    ensures run(l, in_fn).1 != Sig::Brk,
    decreases lsize(l),
{
    reveal_with_fuel(run, 2);
    reveal_with_fuel(step, 2);
    if l.len() > 0 {
        let head = l[0];
        no_escape_no_brk(l.drop_first(), in_fn);
        if step(head, in_fn).1 != Sig::None {
            assert(run(l, in_fn) == step(head, in_fn));
            no_escape_head(head, in_fn);
        } else {
            assert(run(l, in_fn) == (step(head, in_fn).0.add(run(l.drop_first(), in_fn).0), run(l.drop_first(), in_fn).1));
        }
    }
}

/// A single statement in a break-free body cannot step to Brk.
pub proof fn no_escape_head(st: Stmt, in_fn: bool)
    requires !esc_one(st),
    ensures step(st, in_fn).1 != Sig::Brk,
    decreases ssize(st),
{
    reveal_with_fuel(step, 2);
    reveal_with_fuel(esc_one, 1);
    match st {
        Stmt::While { cond_const, cond, body } => {
            // the loop arm maps its body's Brk to None and every other
            // signal to a non-Brk one, so it never steps to Brk
            let r = run(body, in_fn);
            if cond_const && cond {
                if r.1 == Sig::Brk {
                    assert(step(st, in_fn).1 == Sig::None);
                } else {
                    assert(step(st, in_fn).1 != Sig::Brk);
                }
            }
        }
        Stmt::ForS { cond_const, cond, body } => {
            let r = run(body, in_fn);
            if cond_const && cond {
                if r.1 == Sig::Brk {
                    assert(step(st, in_fn).1 == Sig::None);
                } else {
                    assert(step(st, in_fn).1 != Sig::Brk);
                }
            }
        }
        Stmt::If { cond, then, els } => {
            if cond {
                no_escape_no_brk(then, in_fn);
            } else {
                no_escape_no_brk(els, in_fn);
            }
        }
        Stmt::Case { fires, body } => {
            if fires {
                no_escape_no_brk(body, in_fn);
            }
        }
        Stmt::Block(b) => {
            no_escape_no_brk(b, in_fn);
        }
        _ => {}
    }
}

// ──────────────────────────────────────────────── the named laws + pins ───

/// A break terminates the list it sits in.
pub proof fn break_truncates_its_own_list(l: Seq<Stmt>, in_fn: bool)
    ensures run(trunc(l.add(seq![Stmt::Break]), in_fn), in_fn)
        == run(l.add(seq![Stmt::Break]), in_fn),
{
    trunc_sound(l.add(seq![Stmt::Break]), in_fn);
}

pub proof fn continue_truncates_its_own_list(l: Seq<Stmt>, in_fn: bool)
    ensures run(trunc(l.add(seq![Stmt::Continue]), in_fn), in_fn)
        == run(l.add(seq![Stmt::Continue]), in_fn),
{
    trunc_sound(l.add(seq![Stmt::Continue]), in_fn);
}

pub proof fn exit_truncates_its_own_list(l: Seq<Stmt>, in_fn: bool)
    ensures run(trunc(l.add(seq![Stmt::Exit]), in_fn), in_fn)
        == run(l.add(seq![Stmt::Exit]), in_fn),
{
    trunc_sound(l.add(seq![Stmt::Exit]), in_fn);
}

/// `Return` truncates exactly inside a function body; at top level it is not
/// a terminator (bash errors and continues), so the tail must be kept.
pub proof fn return_truncates_only_a_function_body(l: Seq<Stmt>)
    ensures
        terminates(Stmt::Return, true),
        !terminates(Stmt::Return, false),
        run(seq![Stmt::Return], true).1 == Sig::Ret,
        run(seq![Stmt::Return], false).1 == Sig::None,
        terminates(Stmt::Break, false),
{
    reveal_with_fuel(run, 2);
    reveal_with_fuel(step, 2);
    assert(terminates(Stmt::Return, true));
    assert(!terminates(Stmt::Return, false));
    assert(run(seq![Stmt::Return], true) == (Seq::<int>::empty(), Sig::Ret));
    assert(run(seq![Stmt::Return], false) == (Seq::<int>::empty(), Sig::None));
}

/// The loop arm: with no escaping break a constant-true loop contributes
/// nothing after it.
pub proof fn loop_without_escape_never_falls_through(body: Seq<Stmt>, tail: Seq<Stmt>, in_fn: bool)
    requires !escapes(body),
    ensures ({
        let loop_stmt = Stmt::While { cond_const: true, cond: true, body };
        terminates(loop_stmt, in_fn)
            && run(seq![loop_stmt].add(tail), in_fn) == run(seq![loop_stmt], in_fn)
            && run(seq![loop_stmt].add(tail), in_fn).0 == run(seq![loop_stmt], in_fn).0
    }),
{
    trunc_sound(seq![Stmt::While { cond_const: true, cond: true, body }].add(tail), in_fn);
    terminates_stops(Stmt::While { cond_const: true, cond: true, body }, in_fn);
    reveal_with_fuel(step, 2);
    reveal_with_fuel(run, 2);
    assert(run(
        seq![Stmt::While { cond_const: true, cond: true, body }].add(tail),
        in_fn,
    ) == run(seq![Stmt::While { cond_const: true, cond: true, body }], in_fn));
}

// ───────────────────────────────────────────────────────────────────── pins ──

pub open spec fn forever(body: Seq<Stmt>) -> Stmt {
    Stmt::While { cond_const: true, cond: true, body }
}

/// `break` stops its list: the tail's print never appears.
pub proof fn pin_break_stops_the_list()
    ensures run(seq![Stmt::Print(1), Stmt::Break, Stmt::Print(2)], false)
        == (seq![1], Sig::Brk),
{
    assert(run(seq![Stmt::Print(1), Stmt::Break, Stmt::Print(2)], false) == (seq![1], Sig::Brk)) by (compute);
}

/// `continue` stops its list the same way.
pub proof fn pin_continue_stops_the_list()
    ensures run(seq![Stmt::Print(1), Stmt::Continue, Stmt::Print(2)], false)
        == (seq![1], Sig::Cont),
{
    assert(run(seq![Stmt::Print(1), Stmt::Continue, Stmt::Print(2)], false) == (seq![1], Sig::Cont)) by (compute);
}

/// `while true { while true { break } }` — the inner break belongs to the
/// inner loop, so the OUTER loop still never falls through and the tail's
/// print never appears.
pub proof fn pin_nested_break_does_not_escape()
    ensures
        !escapes(seq![forever(seq![Stmt::Break])]),
        terminates(forever(seq![forever(seq![Stmt::Break])]), false),
        run(seq![forever(seq![forever(seq![Stmt::Break])]), Stmt::Print(5)], false).0
            == Seq::<int>::empty(),
{
    assert(!escapes(seq![forever(seq![Stmt::Break])])) by (compute);
    assert(terminates(forever(seq![forever(seq![Stmt::Break])]), false)) by (compute);
    assert(run(seq![forever(seq![forever(seq![Stmt::Break])]), Stmt::Print(5)], false).0
        == Seq::<int>::empty()) by (compute);
}

/// A `break` in a `case` inside the loop DOES escape it (bash's `case`
/// captures nothing): the loop falls through and the tail prints.
pub proof fn pin_case_break_escapes()
    ensures
        escapes(seq![Stmt::Case { fires: true, body: seq![Stmt::Break] }]),
        !terminates(forever(seq![Stmt::Case { fires: true, body: seq![Stmt::Break] }]), false),
        run(seq![forever(seq![Stmt::Case { fires: true, body: seq![Stmt::Break] }]), Stmt::Print(5)], false).0
            == seq![5],
{
    assert(escapes(seq![Stmt::Case { fires: true, body: seq![Stmt::Break] }])) by (compute);
    assert(!terminates(forever(seq![Stmt::Case { fires: true, body: seq![Stmt::Break] }]), false)) by (compute);
    assert(run(seq![forever(seq![Stmt::Case { fires: true, body: seq![Stmt::Break] }]), Stmt::Print(5)], false).0
        == seq![5]) by (compute);
}

/// The control-call spelling `break || exit 1` escapes too — missing it
/// pruned the live tail (the source's `a_break_control_call_makes_the_loop_escapable`).
pub proof fn pin_break_call_escapes()
    ensures
        escapes(seq![Stmt::BreakCall]),
        !terminates(forever(seq![Stmt::BreakCall]), false),
        run(seq![forever(seq![Stmt::BreakCall]), Stmt::Print(5)], false).0 == seq![5],
{
    assert(escapes(seq![Stmt::BreakCall])) by (compute);
    assert(!terminates(forever(seq![Stmt::BreakCall]), false)) by (compute);
    assert(run(seq![forever(seq![Stmt::BreakCall]), Stmt::Print(5)], false).0 == seq![5]) by (compute);
}

/// A terminator inside an `if` deadens NOTHING after the enclosing statement:
/// the `if` may not take that arm, so the tail still prints. (This is why
/// the pass only truncates on a DIRECT terminator.)
pub proof fn pin_conditional_exit_does_not_deaden()
    ensures
        !terminates(Stmt::If { cond: false, then: seq![Stmt::Exit], els: Seq::empty() }, false),
        run(seq![Stmt::If { cond: false, then: seq![Stmt::Exit], els: Seq::empty() }, Stmt::Print(7)], false).0
            == seq![7],
{
    assert(!terminates(Stmt::If { cond: false, then: seq![Stmt::Exit], els: Seq::empty() }, false)) by (compute);
    assert(run(seq![Stmt::If { cond: false, then: seq![Stmt::Exit], els: Seq::empty() }, Stmt::Print(7)], false).0
        == seq![7]) by (compute);
}

/// A `case`-nested `exit` only aborts when a clause matches.
pub proof fn pin_case_exit_may_not_fire()
    ensures
        !terminates(Stmt::Case { fires: false, body: seq![Stmt::Exit] }, false),
        run(seq![Stmt::Case { fires: false, body: seq![Stmt::Exit] }, Stmt::Print(3)], false).0 == seq![3],
{
    assert(!terminates(Stmt::Case { fires: false, body: seq![Stmt::Exit] }, false)) by (compute);
    assert(run(seq![Stmt::Case { fires: false, body: seq![Stmt::Exit] }, Stmt::Print(3)], false).0 == seq![3]) by (compute);
}

/// A top-level `return` does not stop the list: bash prints an error and the
/// script continues, so the tail's print happens.
pub proof fn pin_top_level_return_keeps_the_tail()
    ensures
        run(seq![Stmt::Return, Stmt::Print(1)], false).0 == seq![1],
        run(seq![Stmt::Return, Stmt::Print(1)], true).0 == Seq::<int>::empty(),
{
    assert(run(seq![Stmt::Return, Stmt::Print(1)], false).0 == seq![1]) by (compute);
    assert(run(seq![Stmt::Return, Stmt::Print(1)], true).0 == Seq::<int>::empty()) by (compute);
}

/// A loop whose body can `break` falls through normally: the tail prints.
pub proof fn pin_escapable_loop_keeps_the_tail()
    ensures
        escapes(seq![Stmt::Break]),
        !terminates(forever(seq![Stmt::Break]), false),
        run(seq![forever(seq![Stmt::Break]), Stmt::Print(9)], false).0 == seq![9],
{
    assert(escapes(seq![Stmt::Break])) by (compute);
    assert(!terminates(forever(seq![Stmt::Break]), false)) by (compute);
    assert(run(seq![forever(seq![Stmt::Break]), Stmt::Print(9)], false).0 == seq![9]) by (compute);
}

/// The transform on an infinite loop drops exactly the tail, and on a list
/// terminated by `break` keeps the break and drops what follows.
pub proof fn pin_trunc_shapes()
    ensures
        trunc(seq![forever(seq![Stmt::Print(1)]), Stmt::Print(5)], false)
            == seq![forever(seq![Stmt::Print(1)])],
        trunc(seq![Stmt::Print(1), Stmt::Break, Stmt::Print(5)], false)
            == seq![Stmt::Print(1), Stmt::Break],
{
    assert(trunc(seq![forever(seq![Stmt::Print(1)]), Stmt::Print(5)], false)
        == seq![forever(seq![Stmt::Print(1)])]) by (compute);
    assert(trunc(seq![Stmt::Print(1), Stmt::Break, Stmt::Print(5)], false)
        == seq![Stmt::Print(1), Stmt::Break]) by (compute);
}

fn main() {}

} // verus!
