//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_cond.rs — cluster D3 of the
//  `shir_native_stmt` decomposition: the if/while condition families.
//
//  Plan §D assigns D3 "the if/while condition families" and lists
//  `native_if_test_cond` (5833), `native_while_test_cond` (5892),
//  `native_test_exit_stmt` (5900) and `native_while_multi_cond` (5957).
//  D1's atomic renderer round trip and D2's chain folds are already proved
//  (shir_native_test_render.rs, shir_native_chain.rs); this file does not
//  re-claim them.
//
//  ── the two sides ─────────────────────────────────────────────────────
//
//  `native_if_test_cond` replaces `test COND && B` by two raw statements:
//  write BOTH status variables (`$main_exit_code`, `$CHILD_ERROR`) from the
//  condition, then run B under an `if`. SEMANTIC side `bash_and` applies
//  bash's `&&`: a false test writes 1 and B never runs; a true test runs B,
//  whose own status wins. PASS side `pass_if` performs the explicit
//  double-variable write and then the conditional body.
//
//  `native_test_exit_stmt` replaces `test COND || exit N` by
//  `unless (COND) { exit N }; $main_exit_code = $CHILD_ERROR = 0;`.
//  SEMANTIC side `bash_or_exit`: a true test leaves status 0 and continues;
//  a false test exits with N (no later statement runs). PASS side
//  `pass_unless` models the code order: the unless-block may exit, and the
//  trailing status write only runs when it did not.
//
//  `native_while_test_cond` renders a test guard natively so the loop can
//  see the loop variable (a `bash -c` child cannot). The semantic side
//  evaluates the comparison in an environment `Map<var, value>` with
//  bash's integer operators; the pass side is the renderer's operator
//  table. The law is that both decide the same guard for every value.
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * D1's atomic round trip (file tests, strings, numification) is reused
//     conceptually and proved in shir_native_test_render.rs; not re-claimed.
//   * D2's chain status laws are proved in shir_native_chain.rs; the
//     always-success gate and the `$? == 0` read are not re-proved here.
//   * `render_test_text`'s top-level `&&`/`-a`/`||`/`-o` split and its
//     mixed-kind precedence mismatch: the plan's §D3 wording names the
//     if/while condition families, so the flat-text split stays with the
//     renderer work and is NOT claimed here. `!`, parentheses, globs and
//     `-a`/`-o` inside `[[ ]]` are likewise out of model.
//   * `native_while_multi_cond`'s loop restructure (`while(1) { echo; last
//     unless COND; BODY }`): NOT THIS PROOF. Its equivalence needs a model
//     of the echoed output and of a guard whose truth is loop-invariant;
//     that is stated as unproved rather than assumed.
//   * `render_flag_substitution_test` (`$(...)`) is a runtime capture and
//     remains out of model.
//   * The renderer that turns the emitted Perl text into bytes, and the
//     backend status registers (perl `$CHILD_ERROR`, c `_sh_rc`, js
//     `sh2.lastExit`), are out of model.
//   * Clusters A, F and G are other proofs.
//
//  ── measured mutants (restored byte-identically; sha in the sync) ──────
//
//  baseline 10 verified, 0 errors. All six go red:
//    m1 write only `$CHILD_ERROR` from the condition: 8 verified, 2 errors
//       (if_test_cond_law and pin_body_failure_wins).
//    m2 invert the condition's status: 8v/2e (the same two).
//    m3 drop the trailing `$main_exit_code = $CHILD_ERROR = 0` reset:
//       9v/1e at test_exit_stmt_law.
//    m4 negate the unless guard: 9v/1e at test_exit_stmt_law.
//    m5 render `-lt` as `<=`: 9v/1e at while_test_cond_law.
//    m6 treat the loop variable as literal 0: 9v/1e at
//       pin_while_reads_the_variable.
//  Each mutated file was restored byte-identically and its sha re-checked.
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

/// The two status variables the perl renderer maintains.
pub struct St {
    pub main_exit: int,
    pub child_error: int,
}

pub open spec fn set_both(s: St, v: int) -> St {
    St { main_exit: v, child_error: v }
}

// ─────────────────────────────────────── native_if_test_cond (statuses) ──

/// bash `test COND && B`: false writes 1 and skips B; true runs B, whose
/// own status wins.
pub open spec fn bash_and(cond: bool, body_status: int, s: St) -> St {
    if cond {
        set_both(s, body_status)
    } else {
        set_both(s, 1)
    }
}

/// the fold: write both variables from the condition, then the conditional
/// body.
pub open spec fn pass_if(cond: bool, body_status: int, s: St) -> St {
    let s1 = set_both(s, if cond { 0 } else { 1 });
    if cond {
        set_both(s1, body_status)
    } else {
        s1
    }
}

pub proof fn if_test_cond_law(cond: bool, body_status: int, s: St)
    ensures pass_if(cond, body_status, s) == bash_and(cond, body_status, s),
{
}

/// A body that fails still wins on the true path (the leading write is only
/// the short-circuit case).
pub proof fn pin_body_failure_wins() {
    assert(pass_if(true, 7, St { main_exit: 3, child_error: 4 }).main_exit == 7);
    assert(pass_if(false, 7, St { main_exit: 3, child_error: 4 }) == set_both(St { main_exit: 3, child_error: 4 }, 1));
}

// ────────────────────────────────────── native_test_exit_stmt (|| exit) ──

pub struct Out {
    pub st: St,
    pub exited: bool,
}

/// bash `test COND || exit N`: true leaves status 0 and continues; false
/// exits with N and nothing later runs.
pub open spec fn bash_or_exit(cond: bool, n: int, s: St) -> Out {
    if cond {
        Out { st: set_both(s, 0), exited: false }
    } else {
        Out { st: set_both(s, n), exited: true }
    }
}

/// the fold, in code order: `unless (COND) { exit N }; write 0;`. The exit
/// skips the trailing write; the continuing path performs it.
pub open spec fn pass_unless(cond: bool, n: int, s: St) -> Out {
    let s1 = if cond { s } else { set_both(s, n) };
    if cond {
        Out { st: set_both(s1, 0), exited: false }
    } else {
        Out { st: s1, exited: true }
    }
}

pub proof fn test_exit_stmt_law(cond: bool, n: int, s: St)
    ensures pass_unless(cond, n, s) == bash_or_exit(cond, n, s),
{
}

/// The trailing write is load-bearing: without it the true path keeps the
/// stale status instead of 0.
pub open spec fn pass_no_reset(cond: bool, n: int, s: St) -> Out {
    let s1 = if cond { s } else { set_both(s, n) };
    if cond {
        Out { st: s1, exited: false }
    } else {
        Out { st: s1, exited: true }
    }
}

pub proof fn pin_missing_reset_diverges() {
    let s = St { main_exit: 5, child_error: 5 };
    assert(pass_no_reset(true, 9, s).st == s);
    assert(bash_or_exit(true, 9, s).st == set_both(s, 0));
    assert(pass_no_reset(true, 9, s) != bash_or_exit(true, 9, s));
}

/// Negating the guard exits on the true path (the classic unless inversion).
pub open spec fn pass_wrong_negation(cond: bool, n: int, s: St) -> Out {
    if cond {
        Out { st: set_both(s, n), exited: true }
    } else {
        Out { st: set_both(s, 0), exited: false }
    }
}

pub proof fn pin_wrong_negation_diverges() {
    let s = St { main_exit: 5, child_error: 5 };
    assert(pass_wrong_negation(true, 9, s) != bash_or_exit(true, 9, s));
}

// ──────────────────────────────── native_while_test_cond (guard values) ──

/// The guard's operand: a loop variable or a literal. A `bash -c` child
/// cannot see the loop variable, so rendering it natively is the point.
pub enum Operand {
    Var(int),
    Lit(int),
}

pub open spec fn operand_value(o: Operand, env: Seq<(int, int)>) -> int {
    match o {
        Operand::Var(i) => look(env, i),
        Operand::Lit(v) => v,
    }
}

pub open spec fn look(env: Seq<(int, int)>, i: int) -> int
    decreases env.len(),
{
    if env.len() == 0 {
        0
    } else if env[0].0 == i {
        env[0].1
    } else {
        look(env.drop_first(), i)
    }
}

/// bash's integer test operators.
pub open spec fn bash_cmp(op: Seq<char>, a: int, b: int) -> Option<bool> {
    if op == seq!['-', 'e', 'q'] { Some(a == b) }
    else if op == seq!['-', 'n', 'e'] { Some(a != b) }
    else if op == seq!['-', 'l', 't'] { Some(a < b) }
    else if op == seq!['-', 'l', 'e'] { Some(a <= b) }
    else if op == seq!['-', 'g', 't'] { Some(a > b) }
    else if op == seq!['-', 'g', 'e'] { Some(a >= b) }
    else { None }
}

/// the renderer's operator table for the same six tests.
pub open spec fn render_cmp(op: Seq<char>, a: int, b: int) -> Option<bool> {
    if op == seq!['-', 'l', 't'] { Some(a < b) }
    else if op == seq!['-', 'g', 't'] { Some(a > b) }
    else if op == seq!['-', 'l', 'e'] { Some(a <= b) }
    else if op == seq!['-', 'g', 'e'] { Some(a >= b) }
    else if op == seq!['-', 'e', 'q'] { Some(a == b) }
    else if op == seq!['-', 'n', 'e'] { Some(a != b) }
    else { None }
}

/// The while guard decides the same way after rendering: the same operand
/// value (environment lookup, not the variable's name) and the same
/// comparison.
pub proof fn while_test_cond_law(op: Seq<char>, a: Operand, b: Operand, env: Seq<(int, int)>)
    ensures render_cmp(op, operand_value(a, env), operand_value(b, env))
        == bash_cmp(op, operand_value(a, env), operand_value(b, env)),
{
    reveal_with_fuel(look, 2);
}

/// The variable is read from the environment, not frozen as its name.
pub proof fn pin_while_reads_the_variable() {
    let env = seq![(0int, 7int), (1, 3)];
    reveal_with_fuel(look, 3);
    assert(operand_value(Operand::Var(0), env) == 7);
    assert(operand_value(Operand::Var(1), env) == 3);
    assert(operand_value(Operand::Lit(9), env) == 9);
}

/// An operator outside the six is refused (the guard keeps its old
/// rendering rather than being guessed).
pub proof fn pin_unknown_operator_refused() {
    assert(bash_cmp(seq!['-', 'x'], 1, 2) == None::<bool>);
    assert(render_cmp(seq!['-', 'x'], 1, 2) == None::<bool>);
}

fn main() {}

} // verus!
