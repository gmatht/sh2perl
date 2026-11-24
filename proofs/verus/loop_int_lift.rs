// loop_int_lift.rs — model of `transforms/loop_int_lift.rs`.
//
// The pass rewrites a `for` loop over a LITERAL list of decimal strings to
// integer elements when the loop variable carries an INT verdict and the
// body never observes the variable as text:
//
//   For { var, iter: Array(Str("1"), Str("2"), ...) }
//     ->  For { var, iter: Array(Int(1), Int(2), ...) }
//
// Shell loops bind TEXT, so the rewrite is only sound where the two
// spellings cannot be told apart. This file states and proves that the
// three guards buy exactly that:
//
//   lift_sound: for every canonical element `s`, every integer `k` equal to
//   its value, and every body with no text-read of `var`,
//       eval(body, var := Text(s)) == eval(body, var := Int(k))
//   — the body's observation is identical whether `var` is bound to the
//   string spelling or to the integer the rewrite substitutes.
//
// The model carries BOTH views of a value (`num` and `txt`), because that
// is what makes the guard load-bearing: where the body only reads the
// variable numerically the two views collapse to the same `render`, so the
// binding is unobservable; where the body reads it as text, `txt` is the
// original spelling `s` on one side and `render(canonical_value(s))` on the
// other, and no rewrite is sound. Dropping the guard (mutant a) makes the
// law unprovable for exactly that reason.
//
// What is modelled, and what is NOT (fidelity checked against
// transforms/loop_int_lift.rs):
//   - `canonical_int`: optional '-', ASCII digits, no leading zero (except
//     "0"), parses as i64, "-0" refused. Modelled as the shape predicate
//     `is_canonical` (digit sequence, length > 0, no leading zero, not
//     "-0") plus the uninterpreted value function `parse_canonical`. The
//     parser's digit arithmetic is NOT re-proved — no model in this suite
//     proves `str::parse` — but the shape is, and the shape is where the
//     refusals live (pins below).
//   - The guard walkers `expr_reads_as_text` / `stmts_read_as_text`: modelled
//     as `expr_text_reads` / `stmt_text_reads`. The source's structure is
//     kept: `Arith(_)` is a numeric context and is never a text read; a
//     bare `Var`/`Ident` naming the loop variable is; every text-use node
//     is (the source inspects the string for `$i`/`${i}`; the model takes
//     every text-use as a read, which is the conservative direction — it
//     refuses more, never fewer).
//   - Numeric coercion of a text value (`coerce_text`): canonical text has
//     exactly its integer value; any non-canonical text is `None`, i.e. the
//     body's numeric read is not the literal bash would parse. That is what
//     makes `is_canonical` a load-bearing premise rather than decoration:
//     without it the two sides are `None` vs `Some(k)` (mutant c).
//   - Out of model, stated: the caller's type verdict (`int_typed`) is taken
//     as a given premise, not re-derived from the A2 analysis; the
//     in-place `lift` walker's traversal order and `changed` bookkeeping
//     (each `For` is an independent site); the Python backend's opt-in
//     consumption (the rewrite is shared, `transforms::all()` does not
//     register it — the plain `transform` entry returns false by design).
//
// MUTANTS (measured on this file: 22 verified, 0 errors baseline)
//   (a) THE PASS IGNORES THE BODY GUARD — `lift_sound`'s
//       `!stmt_text_reads(body, var)` premise dropped, i.e. the lift fires
//       on a body that reads the variable as text: 21 verified, 1 error,
//       `precondition not satisfied` at the `stmt_agrees` call INSIDE THE
//       LAW. This is the mutant of record.
//   (a2) same hole one level down — the guard DEFINITION widened so a bare
//       `Var` naming the loop variable no longer counts as a text read
//       (`expr_text_reads(Var(_)) => false`): 15 verified, 3 errors, the
//       assertion in `eval_agrees` (the invariant that carries the guard),
//       the `stmt_agrees` precondition, and `pin_text_read_blocks`.
//   (b) `is_canonical` dropped from the law's premises: 21 verified, 1
//       error inside the law (a non-canonical spelling coerces to `None`
//       while the integer side is `Some(k)`).
//   (c) leading zeros accepted (`no_leading_zero` => true): 19 verified, 3
//       errors — `pin_leading_zero_refused`, `pin_lift_fires` and the
//       list-gate pin. Caught by the FIDELITY PINS, not by the law, and
//       that is honest: for a body that only reads the variable
//       arithmetically "007" and 7 ARE indistinguishable (bash coerces to
//       7), so the numeric law still holds. The source refuses leading
//       zeros because the guard inspects the body syntactically — a use it
//       does not see (a captured value read later, a spelling outside the
//       walked tree) would still observe the text. The refusal is defence
//       in depth; the law covers the hole, the pins cover the shape.
//
// Verus 0.2026.10.18.7d82994:
//   verus --triggers-mode silent --rlimit 40 loop_int_lift.rs
//   -> verification results:: 22 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Name = nat;

// ── canonical decimal literals ──────────────────────────────────────────

/// A decimal spelling: a sign and its digits (each < 10).
pub struct Strn {
    pub neg: bool,
    pub digits: Seq<nat>,
}

pub open spec fn is_digit_seq(ds: Seq<nat>) -> bool {
    forall|i: int| 0 <= i < ds.len() ==> #[trigger] ds[i] < 10nat
}

pub open spec fn no_leading_zero(ds: Seq<nat>) -> bool {
    ds.len() <= 1 || ds[0] != 0nat
}

/// The source's `canonical_int` acceptance shape.
pub open spec fn is_canonical(s: Strn) -> bool {
    &&& s.digits.len() > 0
    &&& is_digit_seq(s.digits)
    &&& no_leading_zero(s.digits)
    &&& !(s.neg && s.digits.len() == 1 && s.digits[0] == 0nat)   // "-0" refused
}

/// The value a canonical spelling denotes (`s.parse::<i64>()` in the
/// source). Uninterpreted: the transform never needs the digit arithmetic,
/// only that one canonical spelling denotes one value.
pub uninterp spec fn parse_canonical(s: Strn) -> int;

pub open spec fn canonical_value(s: Strn) -> int {
    parse_canonical(s)
}

/// What a canonical spelling means to ARITHMETIC (`$(( i + 1 ))`): its
/// value. A non-canonical spelling has no such value here — bash would
/// either accept a form this pass refuses to lift (`" 1"`, `"+1"`) or
/// treat it as an error/0 (`"1x"`), and both readings make the rewrite
/// unsound, which is why the lift refuses them before this point.
pub open spec fn coerce_text(s: Strn) -> Option<int> {
    if is_canonical(s) {
        Option::Some(parse_canonical(s))
    } else {
        Option::None
    }
}

/// The text a copied integer prints as (`$i` after the lift, where the
/// integer is rendered). Uninterpreted: the law never relates this to the
/// input spelling — that is precisely the distinction the guard refuses.
pub uninterp spec fn render(k: int) -> Strn;

// ── values and observations ─────────────────────────────────────────────

/// A loop-variable binding: the shell's text before the rewrite, the
/// integer after it.
pub enum Val {
    Text(Strn),
    Int(int),
}

/// A value as the body can observe it: its numeric view and its text view.
/// Both are always present — a value has one integer meaning and one
/// spelling, and which one a use reaches is what the guard decides.
pub struct RV {
    pub num: Option<int>,
    pub txt: Strn,
}

pub open spec fn rv(num: Option<int>, txt: Strn) -> RV {
    RV { num, txt }
}

/// The opaque value for nodes that observe nothing (a non-`var` binding,
/// an unknown node). Chosen so both bindings give the SAME value: this is
/// the fact the model needs, not a claim about a particular node.
pub open spec fn other_value() -> RV {
    rv(Option::None, render(0))
}

// ── the body ────────────────────────────────────────────────────────────

pub enum E {
    IntLit(int),
    StrLit(Strn),
    /// A bare `Var`/`Ident` naming the loop variable.
    Var(Name),
    /// A numeric context: `Arith(_)` in the source. Never a text read.
    Arith(Box<E>),
    /// A text context (interpolation part, `Str` spelling `$i`, an object
    /// key): observes its child's TEXT.
    TxtUse(Box<E>),
    /// Sequential composition of two subexpressions (argument order).
    Pair(Box<E>, Box<E>),
    /// A node the pass does not inspect.
    Opaque,
}

pub enum S {
    ExprS(E),
    SeqS(Box<S>, Box<S>),
    Empty,
}

// ── the guard ───────────────────────────────────────────────────────────
//
// Mirrors `expr_reads_as_text`: `Arith(_)` is a numeric context (false);
// a bare mention of the loop variable is a text read; anything that uses
// a subtree as text is a text read.

pub open spec fn expr_text_reads(e: E, var: Name) -> bool
    decreases e,
{
    match e {
        E::IntLit(_) => false,
        E::StrLit(_) => false,
        E::Var(n) => n == var,
        E::Arith(_) => false,
        // conservative: every text-use site counts as a read. The source
        // only counts spellings that NAME the variable, so this refuses a
        // superset — sound for the lift, which is what the law needs.
        E::TxtUse(_) => true,
        E::Pair(a, b) => expr_text_reads(*a, var) || expr_text_reads(*b, var),
        E::Opaque => false,
    }
}

pub open spec fn stmt_text_reads(s: S, var: Name) -> bool
    decreases s,
{
    match s {
        S::ExprS(e) => expr_text_reads(e, var),
        S::SeqS(a, b) => stmt_text_reads(*a, var) || stmt_text_reads(*b, var),
        S::Empty => false,
    }
}

// ── evaluation ──────────────────────────────────────────────────────────

pub open spec fn eval_expr(e: E, var: Name, b: Val) -> RV
    decreases e,
{
    match e {
        E::IntLit(n) => rv(Option::Some(n), render(n)),
        E::StrLit(s) => rv(coerce_text(s), s),
        E::Var(n) => if n == var {
            match b {
                Val::Text(s) => rv(coerce_text(s), s),
                Val::Int(k) => rv(Option::Some(k), render(k)),
            }
        } else {
            other_value()
        },
        // a numeric context: the numeric view, re-rendered
        E::Arith(x) => match eval_expr(*x, var, b).num {
            Option::Some(v) => rv(Option::Some(v), render(v)),
            Option::None => rv(Option::None, render(0)),
        },
        // a text context: the text view, no numeric view
        E::TxtUse(x) => rv(Option::None, eval_expr(*x, var, b).txt),
        E::Pair(a, c) => {
            let _ = eval_expr(*a, var, b);
            eval_expr(*c, var, b)
        }
        E::Opaque => other_value(),
    }
}

pub open spec fn eval_stmt(s: S, var: Name, b: Val) -> RV
    decreases s,
{
    match s {
        S::ExprS(e) => eval_expr(e, var, b),
        S::SeqS(a, c) => {
            let _ = eval_stmt(*a, var, b);
            eval_stmt(*c, var, b)
        }
        S::Empty => other_value(),
    }
}

// ── the rewrite ─────────────────────────────────────────────────────────

/// The per-element rewrite: a canonical `Str` becomes the `Int` carrying
/// its value; anything else refuses (no lift).
pub open spec fn lift_elem(e: E) -> Option<E> {
    match e {
        E::StrLit(s) => if is_canonical(s) {
            Option::Some(E::IntLit(canonical_value(s)))
        } else {
            Option::None
        },
        _ => Option::None,
    }
}

/// The source's two list-level gates: the caller's INT verdict must name the
/// loop variable (`int_typed.contains(var)`, taken as a premise), and EVERY
/// element must be canonical — one bad element refuses the whole list
/// (`lifted: Option<Vec<_>>` collects, so a single `None` aborts the site).
pub open spec fn all_canonical(items: Seq<Strn>) -> bool {
    forall|i: int| 0 <= i < items.len() ==> #[trigger] is_canonical(items[i])
}

pub open spec fn lift_iter_ok(items: Seq<Strn>, verdict: bool) -> bool {
    verdict && all_canonical(items)
}

// ── the laws ────────────────────────────────────────────────────────────

/// The numeric view of any expression is the same under both bindings, as
/// soon as the text spelling is canonical and `k` is its value: the only
/// node that consults the binding is `Var`, and both of its views of `var`
/// give `Some(canonical_value(s))`.
pub proof fn num_view_agrees(e: E, var: Name, s: Strn, k: int)
    requires
        is_canonical(s),
        k == canonical_value(s),
    ensures
        eval_expr(e, var, Val::Text(s)).num == eval_expr(e, var, Val::Int(k)).num,
    decreases e,
{
    match e {
        E::IntLit(_) => {}
        E::StrLit(_) => {}
        E::Var(n) => {}
        E::Arith(x) => {
            num_view_agrees(*x, var, s, k);
        }
        E::TxtUse(x) => {
            num_view_agrees(*x, var, s, k);
        }
        E::Pair(a, c) => {
            num_view_agrees(*a, var, s, k);
            num_view_agrees(*c, var, s, k);
        }
        E::Opaque => {}
    }
}

/// THE INVARIANT: a body that never reads `var` as text observes the same
/// `RV` under both bindings — the text view is either a literal's own
/// spelling (identical on both sides) or a `render` of the numeric view
/// (equal by `num_view_agrees`).
pub proof fn eval_agrees(e: E, var: Name, s: Strn, k: int)
    requires
        is_canonical(s),
        k == canonical_value(s),
        !expr_text_reads(e, var),
    ensures
        eval_expr(e, var, Val::Text(s)) == eval_expr(e, var, Val::Int(k)),
    decreases e,
{
    match e {
        E::IntLit(_) => {}
        E::StrLit(_) => {}
        E::Var(n) => {
            // the guard says n != var, so both sides are `other_value()`
            assert(n != var);
        }
        E::Arith(x) => {
            num_view_agrees(*x, var, s, k);
        }
        E::TxtUse(_) => {
            // the guard refuses every text-use node
            assert(false);
        }
        E::Pair(a, c) => {
            eval_agrees(*a, var, s, k);
            eval_agrees(*c, var, s, k);
        }
        E::Opaque => {}
    }
}

pub proof fn stmt_agrees(s: S, var: Name, st: Strn, k: int)
    requires
        is_canonical(st),
        k == canonical_value(st),
        !stmt_text_reads(s, var),
    ensures
        eval_stmt(s, var, Val::Text(st)) == eval_stmt(s, var, Val::Int(k)),
    decreases s,
{
    match s {
        S::ExprS(e) => {
            eval_agrees(e, var, st, k);
        }
        S::SeqS(a, c) => {
            stmt_agrees(*a, var, st, k);
            stmt_agrees(*c, var, st, k);
        }
        S::Empty => {}
    }
}

/// THE THEOREM OF RECORD. For every canonical element spelling `s`, every
/// integer `k` that is its value, and every loop body that does not read
/// the loop variable as text, the body observes the same thing whether the
/// variable is bound to the string (the shell's text loop, before the
/// rewrite) or to the integer the rewrite substitutes.
///
/// This is what makes the transform sound: the guard refuses exactly the
/// bodies where the two bindings are distinguishable, and for the rest the
/// rewrite is unobservable.
pub proof fn lift_sound(body: S, var: Name, s: Strn, k: int)
    requires
        is_canonical(s),
        k == canonical_value(s),
        !stmt_text_reads(body, var),
    ensures
        eval_stmt(body, var, Val::Text(s)) == eval_stmt(body, var, Val::Int(k)),
        // and the rewrite really does emit that integer for that spelling
        lift_elem(E::StrLit(s)) == Option::Some(E::IntLit(k)),
{
    stmt_agrees(body, var, s, k);
    assert(is_canonical(s));
    assert(lift_elem(E::StrLit(s)) == Option::Some(E::IntLit(canonical_value(s))));
}

/// The corollary the pass's shape needs: the rewritten iteration list
/// binds the same value the original did, element by element.
pub proof fn lift_preserves_element(s: Strn)
    requires is_canonical(s),
    ensures
        lift_elem(E::StrLit(s)) == Option::Some(E::IntLit(canonical_value(s))),
        // the value the integer carries is the value the text coerces to
        coerce_text(s) == Option::Some(canonical_value(s)),
{
}

// ── fidelity pins (shape of the refusals) ───────────────────────────────

/// "007" is refused: bash prints 007, the integer 7 prints 7.
pub proof fn pin_leading_zero_refused()
    ensures !is_canonical(Strn { neg: false, digits: seq![0nat, 0nat, 7nat] }),
{
}

/// "-0" is refused: it prints and compares differently from 0.
pub proof fn pin_minus_zero_refused()
    ensures !is_canonical(Strn { neg: true, digits: seq![0nat] }),
{
}

/// "" is refused (no digits at all).
pub proof fn pin_empty_refused()
    ensures !is_canonical(Strn { neg: false, digits: Seq::empty() }),
{
}

/// "0" is accepted.
pub proof fn pin_plain_zero_ok()
    ensures is_canonical(Strn { neg: false, digits: seq![0nat] }),
{
}

/// "12" and "-42" are accepted.
pub proof fn pin_canonical_forms()
    ensures
        is_canonical(Strn { neg: false, digits: seq![1nat, 2nat] }),
        is_canonical(Strn { neg: true, digits: seq![4nat, 2nat] }),
{
}

/// A digit outside 0..=9 is refused (the source only ever sees ASCII
/// digits from its byte check).
pub proof fn pin_non_digit_refused()
    ensures !is_canonical(Strn { neg: false, digits: seq![1nat, 10nat] }),
{
}

/// The lift fires on a canonical element and refuses a non-canonical one.
pub proof fn pin_lift_fires()
    ensures
        lift_elem(E::StrLit(Strn { neg: false, digits: seq![7nat] }))
            == Option::Some(E::IntLit(canonical_value(Strn { neg: false, digits: seq![7nat] }))),
        lift_elem(E::StrLit(Strn { neg: false, digits: seq![0nat, 7nat] })) == Option::None::<E>,
{
}

/// The guard: a text-use of the loop variable is a read; arithmetic alone
/// is not.
pub proof fn pin_text_read_blocks()
    ensures
        stmt_text_reads(S::ExprS(E::Var(1nat)), 1nat),
        stmt_text_reads(S::ExprS(E::TxtUse(Box::new(E::IntLit(1)))), 1nat),
        !stmt_text_reads(S::ExprS(E::Arith(Box::new(E::Var(1nat)))), 1nat),
{
}

/// The whole-list rewrite: when the verdict holds and every element is
/// canonical, every emitted element is the `Int` carrying the original
/// element's value — the list the backend builds is the same values in the
/// same order.
pub proof fn lift_iter_sound(items: Seq<Strn>, verdict: bool)
    requires
        lift_iter_ok(items, verdict),
    ensures
        forall|i: int| 0 <= i < items.len() ==> #[trigger] lift_elem(E::StrLit(items[i]))
            == Option::Some(E::IntLit(canonical_value(items[i]))),
{
}

/// The verdict gate: without the caller's INT verdict nothing is lifted.
pub proof fn pin_no_verdict_no_lift(items: Seq<Strn>)
    ensures !lift_iter_ok(items, false),
{
}

/// The list gate: an all-canonical list under a true verdict is lifted.
pub proof fn pin_all_canonical_lifts()
    ensures
        lift_iter_ok(seq![Strn { neg: false, digits: seq![1nat] }, Strn { neg: false, digits: seq![2nat] }], true),
        !lift_iter_ok(seq![Strn { neg: false, digits: seq![1nat] }, Strn { neg: false, digits: seq![0nat, 2nat] }], true),
{
    // one leading-zero element refuses the whole list: instantiate the
    // `all_canonical` quantifier at that index and contradict it
    let xs = seq![Strn { neg: false, digits: seq![1nat] }, Strn { neg: false, digits: seq![0nat, 2nat] }];
    assert(!is_canonical(Strn { neg: false, digits: seq![0nat, 2nat] }));
    if all_canonical(xs) {
        assert(is_canonical(xs[1]));
    }
}

/// A body that reads the variable only arithmetically passes the guard —
/// this is the case the transform exists for.
pub proof fn pin_arith_read_allowed()
    ensures !stmt_text_reads(S::ExprS(E::Arith(Box::new(E::Var(1nat)))), 1nat),
{
}

fn main() {}

} // verus!
