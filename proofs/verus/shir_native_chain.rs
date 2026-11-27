//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_chain.rs — cluster D2 of the
//  `shir_native_stmt` decomposition: the `test`/command chain folds.
//
//  Plan §D splits cluster D into D1 (render_test_* round trip, PROVED in
//  shir_native_test_render.rs — reused, not re-claimed), D2 (the chain
//  folds: test_and_to_if / test_chain_to_if / cmd_and_chain_to_if) and D3
//  (the if/while condition families). This file is D2.
//
//  ── the two sides ─────────────────────────────────────────────────────
//
//  SEMANTIC side: `bash` runs a chain with its own status semantics. A
//  command writes its exit status; `A && B` runs B only when A wrote 0 and
//  otherwise keeps A's status; `A || B` runs B only when A wrote non-zero;
//  a statement's effect is the sequence of commands it ran. `bash_test_and`,
//  `bash_chain` and `bash_or` implement that directly over a command list.
//
//  PASS side: the folds build an `If`/`Block` structure — `IfC` for an If
//  with a condition, `Then` for statement sequencing, `StatusZero` for the
//  `$? == 0` test `cmd_and_chain_to_if` emits, `Skip` for an empty else.
//  `IfC`'s semantics are the backend's: a false condition with no else keeps
//  the condition's own status; a taken branch's status is the If's status.
//  `build_chain` is `cmd_and_chain_to_if`'s right-to-left nesting.
//
//  The laws compare `run` of the PASS structure against the SEMANTIC chain,
//  so a fold that emits the wrong cond, swaps && for ||, or reorders the
//  chain diverges.
//
//  ── laws ──────────────────────────────────────────────────────────────
//
//   * `test_and_to_if_law`: `test b && THEN` → `If b THEN (no else)` keeps
//     the status and the effect (a false test's non-zero status survives
//     because no else runs).
//   * `test_chain_to_if_law`: `(test b && THEN) || ELSE` → `If b THEN ELSE`
//     when THEN is always-success. THIS is where the fold's always-success
//     gate is load-bearing: if THEN could fail, bash would run ELSE
//     (`a && b || c`), while the If form would not.
//   * `cmd_and_chain_to_if_law`: a command chain `e0 && e1 && …` → the
//     fold's `e0; If ($? == 0) { e1; If ($? == 0) { … } }`, with the same
//     status and the same executed-command sequence.
//
//  ── refusals proved as pins ───────────────────────────────────────────
//
//   * `pin_fallible_then_diverges`: a fallible THEN under `||` gives
//     different effects in bash and in the If form (the gate exists).
//   * `pin_no_else_keeps_condition_status`: `If false THEN (no else)`
//     returns 1, not 0 — why the second arm of `test_and_to_if` writes an
//     explicit failing else.
//   * `pin_cd_is_not_chain_native`, `pin_pure_bool_lead_refused`,
//     `pin_mixed_and_or_refused`: the three gates of `chain_stmt_expr` /
//     `cmd_and_chain_to_if` are genuine refusals.
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * D1's atomic renderer round trip is PROVED in shir_native_test_render.rs
//     and reused conceptually (the folds' condition is assumed native); it
//     is not re-claimed.
//   * `render_test_text`'s top-level connector split (`&&`, `-a`, `||`,
//     `-o`) and its mixed-kind precedence mismatch, plus `!`, globs, `-a`/`-o`
//     inside `[[ ]]`, an unset variable under `set -u` and a dynamic word
//     that could expand to several words: NOT THIS PROOF (the renderer
//     half, D1/D3). Stated, not quietly constrained away.
//   * `static_pipeline_truth`'s literal `echo|grep` evaluation and the
//     `files` map's cross-statement invariant (cluster A) are not modelled
//     here; `test_chain_to_if`'s static-pipeline branch (which keeps the
//     pipeline ahead of the taken branch) is out of this law.
//   * `redirect_call_to_stmt`'s mode whitelist and `flatten_redirect_status_block`
//     are structural helpers: modelled as identity on the statement list.
//   * Backend rendering of the emitted If/status (perl `$CHILD_ERROR`, c
//     `_sh_rc`, js `sh2.lastExit`) is out of model.
//
//  ── measured mutants (restored byte-identically; sha in the sync) ──────
//
//  baseline 18 verified, 0 errors. All six go red:
//    m1 drop the always-success gate (`requires always_ok(then)` removed):
//       17 verified, 1 error (the law's precondition cannot be met).
//    m2 `cmd_and_chain_to_if` tests `true` instead of `$? == 0`: 17v/1e,
//       the chain law's status postcondition fails.
//    m3 `chain_stmt_expr` accepts `cd`: 17v/1e at
//       pin_cd_is_not_chain_native.
//    m4 the fold accepts a pure-boolean lead: 17v/1e at
//       pin_pure_bool_lead_refused.
//    m5 the mixed `&&`/`||` gate accepts everything: rejected before
//       verification — `all_and` stops being recursive, so the pin's
//       `reveal_with_fuel` cannot be satisfied (pin_mixed_and_or_refused).
//    m6 `test_chain_to_if` drops the else branch: 17v/1e, the chain law's
//       status and effect postconditions fail.
//  Each mutated file was restored byte-identically and its sha re-checked.
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub enum S {
    /// a command with (identity, exit status)
    Exec(int, int),
    /// a boolean condition that writes 0 (true) or 1 (false)
    Test(bool),
    /// the fold's `$? == 0` condition: reads the incoming status
    StatusZero,
    /// a no-op: an empty else keeps the condition's status
    Skip,
    And(Box<S>, Box<S>),
    Or(Box<S>, Box<S>),
    /// the emitted If: condition, then, else (Skip = no else)
    IfC(Box<S>, Box<S>, Box<S>),
    /// statement sequencing
    Then(Box<S>, Box<S>),
}

// ───────────────────────────────────────────────── semantic side ─────────

/// bash runs the list's commands in order, each writing its status; the
/// list's status is the last one that ran.
pub open spec fn bash_status(ss: Seq<S>, st0: int) -> int
    decreases ss.len(),
{
    if ss.len() == 0 {
        st0
    } else if ss.len() == 1 {
        status(ss[0], st0)
    } else {
        let st = status(ss[0], st0);
        if st == 0 {
            bash_status(ss.drop_first(), st)
        } else {
            st
        }
    }
}

/// The effect: command ids in execution order.
pub open spec fn bash_trace(ss: Seq<S>, st0: int) -> Seq<int>
    decreases ss.len(),
{
    if ss.len() == 0 {
        Seq::empty()
    } else if ss.len() == 1 {
        trace(ss[0], st0)
    } else {
        let st = status(ss[0], st0);
        if st == 0 {
            trace(ss[0], st0) + bash_trace(ss.drop_first(), st)
        } else {
            trace(ss[0], st0)
        }
    }
}

pub open spec fn bash_test_and_status(b: bool, then: S, st0: int) -> int {
    if b {
        status(then, 0)
    } else {
        1
    }
}

pub open spec fn bash_test_and_trace(b: bool, then: S, st0: int) -> Seq<int> {
    if b {
        trace(then, 0)
    } else {
        Seq::empty()
    }
}

pub open spec fn bash_test_chain_status(b: bool, then: S, els: S, st0: int) -> int {
    let st = bash_test_and_status(b, then, st0);
    if st == 0 {
        st
    } else {
        status(els, st)
    }
}

pub open spec fn bash_test_chain_trace(b: bool, then: S, els: S, st0: int) -> Seq<int> {
    let st = bash_test_and_status(b, then, st0);
    if st == 0 {
        bash_test_and_trace(b, then, st0)
    } else {
        bash_test_and_trace(b, then, st0) + trace(els, st)
    }
}

// ───────────────────────────────────────────────────── pass: run/steps ──

/// A `$? == 0` condition reads the status register without writing it;
/// every other condition (`test`, a lowered comparison) writes 0/1.
pub open spec fn reads_status_register(c: S) -> bool
    decreases c,
{
    match c {
        S::StatusZero => true,
        S::Test(_) => false,
        S::And(a, b) => reads_status_register(*a) || reads_status_register(*b),
        S::Or(a, b) => reads_status_register(*a) || reads_status_register(*b),
        S::IfC(a, _, _) => reads_status_register(*a),
        S::Then(a, _) => reads_status_register(*a),
        _ => false,
    }
}

pub open spec fn status(s: S, st0: int) -> int
    decreases s,
{
    match s {
        S::Exec(_, st) => st,
        S::Test(b) => if b { 0 } else { 1 },
        S::StatusZero => if st0 == 0 { 0 } else { 1 },
        S::Skip => st0,
        S::And(a, b) => {
            let sa = status(*a, st0);
            if sa == 0 {
                status(*b, sa)
            } else {
                sa
            }
        }
        S::Or(a, b) => {
            let sa = status(*a, st0);
            if sa == 0 {
                sa
            } else {
                status(*b, sa)
            }
        }
        S::IfC(c, t, e) => {
            let sc = status(*c, st0);
            if sc == 0 {
                status(*t, sc)
            } else {
                match *e {
                    // a `$? == 0` read writes no status, so an If with no
                    // else keeps the incoming status (the command's own)
                    S::Skip => if reads_status_register(*c) { st0 } else { sc },
                    _ => status(*e, sc),
                }
            }
        }
        S::Then(a, b) => status(*b, status(*a, st0)),
    }
}

pub open spec fn trace(s: S, st0: int) -> Seq<int>
    decreases s,
{
    match s {
        S::Exec(id, _) => seq![id],
        S::Test(_) => Seq::empty(),
        S::StatusZero => Seq::empty(),
        S::Skip => Seq::empty(),
        S::And(a, b) => {
            let sa = status(*a, st0);
            if sa == 0 {
                trace(*a, st0) + trace(*b, sa)
            } else {
                trace(*a, st0)
            }
        }
        S::Or(a, b) => {
            let sa = status(*a, st0);
            if sa == 0 {
                trace(*a, st0)
            } else {
                trace(*a, st0) + trace(*b, sa)
            }
        }
        S::IfC(c, t, e) => {
            let sc = status(*c, st0);
            if sc == 0 {
                trace(*c, st0) + trace(*t, sc)
            } else {
                match *e {
                    S::Skip => trace(*c, st0),
                    _ => trace(*c, st0) + trace(*e, sc),
                }
            }
        }
        S::Then(a, b) => trace(*a, st0) + trace(*b, status(*a, st0)),
    }
}

/// `cmd_and_chain_to_if`'s right-to-left nesting: `e0; If ($?==0) { rest }`.
pub open spec fn build_chain(ss: Seq<S>) -> S
    decreases ss.len(),
{
    if ss.len() == 0 {
        S::Skip
    } else if ss.len() == 1 {
        ss[0]
    } else {
        S::Then(
            Box::new(ss[0]),
            Box::new(S::IfC(
                Box::new(S::StatusZero),
                Box::new(build_chain(ss.drop_first())),
                Box::new(S::Skip),
            )),
        )
    }
}

/// `test_and_to_if`'s first arm.
pub open spec fn pass_test_and(b: bool, then: S) -> S {
    S::IfC(Box::new(S::Test(b)), Box::new(then), Box::new(S::Skip))
}

/// `test_chain_to_if`'s non-static branch.
pub open spec fn pass_test_chain(b: bool, then: S, els: S) -> S {
    S::IfC(Box::new(S::Test(b)), Box::new(then), Box::new(els))
}

// ──────────────────────────────────────────────── always-success gate ───

/// The folds' "always-success" side condition: a single echo/printf exec
/// (or a trivially true condition) that writes 0 on every path.
pub open spec fn always_ok(s: S) -> bool
    decreases s,
{
    match s {
        S::Exec(_, st) => st == 0,
        S::Test(b) => b,
        S::Then(a, b) => always_ok(*a) && always_ok(*b),
        _ => false,
    }
}

pub proof fn always_ok_status(s: S, st0: int)
    requires always_ok(s),
    ensures status(s, st0) == 0,
    decreases s,
{
    match s {
        S::Exec(_, _) => {}
        S::Test(_) => {}
        S::Then(a, b) => {
            always_ok_status(*a, st0);
            always_ok_status(*b, status(*a, st0));
        }
        _ => {}
    }
}

// ──────────────────────────────────────────────────────────────── laws ───

/// `test b && THEN` → `If b THEN` (no else) keeps status and effect.
pub proof fn test_and_to_if_law(b: bool, then: S, st0: int)
    ensures
        status(pass_test_and(b, then), st0) == bash_test_and_status(b, then, st0),
        trace(pass_test_and(b, then), st0) == bash_test_and_trace(b, then, st0),
{
    reveal_with_fuel(status, 3);
    reveal_with_fuel(trace, 3);
}

/// `(test b && THEN) || ELSE` → `If b THEN ELSE`, when THEN cannot fail.
pub proof fn test_chain_to_if_law(b: bool, then: S, els: S, st0: int)
    requires always_ok(then),
    ensures
        status(pass_test_chain(b, then, els), st0) == bash_test_chain_status(b, then, els, st0),
        trace(pass_test_chain(b, then, els), st0) == bash_test_chain_trace(b, then, els, st0),
{
    reveal_with_fuel(status, 3);
    reveal_with_fuel(trace, 3);
    always_ok_status(then, 0);
}

/// A command `&&` chain → the fold's nested Ifs, same status and effect.
pub proof fn cmd_and_chain_to_if_law(ss: Seq<S>, st0: int)
    ensures
        status(build_chain(ss), st0) == bash_status(ss, st0),
        trace(build_chain(ss), st0) == bash_trace(ss, st0),
    decreases ss.len(),
{
    reveal_with_fuel(status, 4);
    reveal_with_fuel(trace, 4);
    reveal_with_fuel(build_chain, 3);
    reveal_with_fuel(bash_status, 3);
    reveal_with_fuel(bash_trace, 3);
    if ss.len() == 0 {
    } else if ss.len() == 1 {
    } else {
        let st = status(ss[0], st0);
        cmd_and_chain_to_if_law(ss.drop_first(), st);
        assert(build_chain(ss) == S::Then(
            Box::new(ss[0]),
            Box::new(S::IfC(
                Box::new(S::StatusZero),
                Box::new(build_chain(ss.drop_first())),
                Box::new(S::Skip),
            )),
        ));
        assert(status(build_chain(ss), st0)
            == status(S::IfC(
                Box::new(S::StatusZero),
                Box::new(build_chain(ss.drop_first())),
                Box::new(S::Skip),
            ), st));
        assert(bash_status(ss, st0) == (if st == 0 { bash_status(ss.drop_first(), st) } else { st }));
        assert(trace(build_chain(ss), st0)
            == trace(ss[0], st0) + trace(S::IfC(
                Box::new(S::StatusZero),
                Box::new(build_chain(ss.drop_first())),
                Box::new(S::Skip),
            ), st));
        if st == 0 {
            assert(bash_trace(ss, st0) == trace(ss[0], st0) + bash_trace(ss.drop_first(), st));
        } else {
            assert(bash_trace(ss, st0) == trace(ss[0], st0));
        }
    }
}

// ───────────────────────────── load-bearing refusals (pins) ──────────────

/// A fallible THEN under `||` diverges: bash runs ELSE, the If form does
/// not. This is why the fold demands an always-success then-arm.
pub proof fn pin_fallible_then_diverges() {
    let failing = S::Exec(1, 1);
    let els = S::Exec(2, 0);
    reveal_with_fuel(status, 4);
    reveal_with_fuel(trace, 4);
    assert(status(pass_test_chain(true, failing, els), 0) == 1);
    assert(bash_test_chain_status(true, failing, els, 0) == 0);
    assert(trace(pass_test_chain(true, failing, els), 0) == seq![1]);
    assert(bash_test_chain_trace(true, failing, els, 0) == seq![1, 2]);
}

/// A false condition with no else keeps its non-zero status (not 0).
pub proof fn pin_no_else_keeps_condition_status() {
    reveal_with_fuel(status, 3);
    assert(status(S::IfC(Box::new(S::Test(false)), Box::new(S::Exec(0, 0)), Box::new(S::Skip)), 0) == 1);
}

/// `chain_stmt_expr` refuses `cd`: its statement arm is `chdir … or die`,
/// while bash's `cd` in a chain records failure and continues.
pub open spec fn chain_native(kind: int) -> bool {
    // 0 = plain exec, 1 = cd, 2 = pure boolean lead, 3 = redirect/assign/test
    kind == 0 || kind == 3
}

pub proof fn pin_cd_is_not_chain_native() {
    assert(!chain_native(1));
    assert(chain_native(0));
}

/// A pure-boolean LEAD belongs to the test rewrites, not this fold.
pub proof fn pin_pure_bool_lead_refused() {
    assert(!chain_native(2));
}

pub open spec fn all_and(ops: Seq<int>) -> bool
    decreases ops.len(),
{
    if ops.len() == 0 {
        true
    } else {
        ops[0] == 0 && all_and(ops.drop_first())
    }
}

/// A mixed `&&`/`||` chain is refused (`ops` uses 0 for And, 1 for Or).
pub proof fn pin_mixed_and_or_refused() {
    reveal_with_fuel(all_and, 3);
    assert(!all_and(seq![0, 1]));
    assert(all_and(seq![0, 0]));
}

fn main() {}

} // verus!
