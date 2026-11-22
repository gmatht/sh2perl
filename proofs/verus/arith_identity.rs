// arith_identity.rs — model of `transforms/arith_identity.rs`.
//
// The pass folds arithmetic identities on non-literal operands. Its
// registered rule set (module doc + `simplify_in`) is:
//
//   unconditional:  x - 0 -> x,  x * 1 -> x,  1 * x -> x,
//                   x / 1 -> x,  0 / positive-literal -> 0
//   INTEGER DOMAIN: x + 0 -> x,  0 + x -> x        (the `full` flag)
//
// and it deliberately does NOT fold the operand-eliminating forms
// (`x % 1`, `x * 0`, `0 * x`, `x - x`) because replacing the operand drops
// its parse/coercion fate; `0 / 0` and `0 / negative` are refused because
// they are a divide-by-zero and a `-0.0` flip respectively.
//
// What is proved here (the rewrite's soundness, not a re-statement):
//
//   simplify_sound: for EVERY store and every tree,
//       ev(env, simplify(a, full)) = ev(env, a)
//   with errors modelled (`None` = the evaluation raises), so the theorem
//   says the fold preserves both the value AND the divide-by-zero. The
//   `full` flag is the source's `int_domain()` gate: with `full = false`
//   the `+ 0` arms do not fire, and the theorem still holds — which is the
//   statement that the FLOAT domain keeps a correct (smaller) rewrite.
//
//   The per-arm lemmas are named so a mutant that widens one arm goes red;
//   the refusal pins below are inequalities that a missing guard would
//   violate (`0 / 0` folded to 0, `x * 0` folded, `x - x` folded, ...).
//
// Out of model, stated:
// - the float/IEEE value of the unconditional arms (the `-0.0` subtlety is
//   why `+ 0` is gated in the first place); this model is the integer
//   arithmetic the transform's shell domain evaluates.
// - wrapping: every rule either returns a SUBTERM of the original (so the
//   same operations run on the same values) or a literal the original also
//   produces; `x +% 0`, `x *% 1`, `x -% 0`, `x / 1` and `0 / n` are exact
//   for wrapping i64 too, so the int theorem transfers. `0 / n` for a
//   positive literal cannot overflow.
// - the mutation machinery (`simplify` is in-place, `changed` is decided
//   separately): the model states the rewrite's result, not its diffing.
//
// Verus 0.2026.10.18.7d82994:
//   verus --triggers-mode silent --rlimit 60 arith_identity.rs
//   -> verification results:: 19 verified, 0 errors
use vstd::prelude::*;
use vstd::map::*;

verus! {

pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Other,
}

pub enum A {
    Num(int),
    Var(int),
    Bin(Op, Box<A>, Box<A>),
    Un(Op, Box<A>),
    Cond(Box<A>, Box<A>, Box<A>),
    Other,
}

pub open spec fn num(n: int) -> A { A::Num(n) }
pub open spec fn var(i: int) -> A { A::Var(i) }
pub open spec fn bin(op: Op, l: A, r: A) -> A { A::Bin(op, Box::new(l), Box::new(r)) }
pub open spec fn cond(t: A, x: A, y: A) -> A { A::Cond(Box::new(t), Box::new(x), Box::new(y)) }

// ── the evaluation the transform preserves ──────────────────────────────

pub open spec fn apply_op(op: Op, a: int, b: int) -> Option<int> {
    match op {
        Op::Add => Option::Some(a + b),
        Op::Sub => Option::Some(a - b),
        Op::Mul => Option::Some(a * b),
        Op::Div => if b == 0 { Option::None } else { Option::Some(a / b) },
        Op::Mod => if b == 0 { Option::None } else { Option::Some(a % b) },
        Op::Other => Option::None,
    }
}

pub open spec fn ev(env: Map<int, int>, a: A) -> Option<int>
    decreases a,
{
    match a {
        A::Num(n) => Option::Some(n),
        A::Var(i) => if env.dom().contains(i) {
            Option::Some(env[i])
        } else {
            Option::None
        },
        A::Bin(op, l, r) => match ev(env, *l) {
            Option::None => Option::None,
            Option::Some(x) => match ev(env, *r) {
                Option::None => Option::None,
                Option::Some(y) => apply_op(op, x, y),
            },
        },
        A::Un(op, x) => match ev(env, *x) {
            Option::None => Option::None,
            Option::Some(v) => if op == Op::Sub { Option::Some(-v) } else { Option::None },
        },
        A::Cond(t, x, y) => match ev(env, *t) {
            Option::None => Option::None,
            Option::Some(v) => if v != 0 { ev(env, *x) } else { ev(env, *y) },
        },
        A::Other => Option::None,
    }
}

// The source's `arith_no_err`: no `/` or `%` anywhere.
pub open spec fn no_err(a: A) -> bool
    decreases a,
{
    match a {
        A::Num(_) | A::Var(_) => true,
        A::Bin(op, l, r) => op != Op::Div && op != Op::Mod && no_err(*l) && no_err(*r),
        A::Un(_, x) => no_err(*x),
        A::Cond(t, x, y) => no_err(*t) && no_err(*x) && no_err(*y),
        A::Other => false,
    }
}

pub open spec fn is_num(a: A, n: int) -> bool {
    match a {
        A::Num(m) => m == n,
        _ => false,
    }
}

pub open spec fn is_positive_literal(a: A) -> bool {
    match a {
        A::Num(n) => n > 0,
        _ => false,
    }
}

// ── the rewrite, mirroring `simplify_in` arm for arm ────────────────────

pub open spec fn simplify(a: A, full: bool) -> A
    decreases a,
{
    match a {
        A::Bin(op, l, r) => {
            let l2 = simplify(*l, full);
            let r2 = simplify(*r, full);
            match op {
                Op::Add if full && is_num(r2, 0) => l2,
                Op::Add if full && is_num(l2, 0) => r2,
                Op::Sub if is_num(r2, 0) => l2,
                Op::Mul if is_num(r2, 1) => l2,
                Op::Mul if is_num(l2, 1) => r2,
                Op::Div if is_num(r2, 1) => l2,
                Op::Div if is_num(l2, 0) && is_positive_literal(r2) => A::Num(0),
                _ => bin(op, l2, r2),
            }
        }
        A::Un(op, x) => A::Un(op, Box::new(simplify(*x, full))),
        A::Cond(t, x, y) => cond(simplify(*t, full), simplify(*x, full), simplify(*y, full)),
        _ => a,
    }
}

// ── per-arm soundness ───────────────────────────────────────────────────

proof fn add_zero_right(env: Map<int, int>, l: A)
    ensures ev(env, bin(Op::Add, l, num(0))) == ev(env, l)
{
    reveal_with_fuel(ev, 2);
}

proof fn add_zero_left(env: Map<int, int>, r: A)
    ensures ev(env, bin(Op::Add, num(0), r)) == ev(env, r)
{
    reveal_with_fuel(ev, 2);
}

proof fn sub_zero_right(env: Map<int, int>, l: A)
    ensures ev(env, bin(Op::Sub, l, num(0))) == ev(env, l)
{
    reveal_with_fuel(ev, 2);
}

proof fn mul_one_right(env: Map<int, int>, l: A)
    ensures ev(env, bin(Op::Mul, l, num(1))) == ev(env, l)
{
    reveal_with_fuel(ev, 2);
}

proof fn mul_one_left(env: Map<int, int>, r: A)
    ensures ev(env, bin(Op::Mul, num(1), r)) == ev(env, r)
{
    reveal_with_fuel(ev, 2);
}

proof fn div_one_right(env: Map<int, int>, l: A)
    ensures ev(env, bin(Op::Div, l, num(1))) == ev(env, l)
{
    reveal_with_fuel(ev, 2);
}

proof fn zero_over_positive(env: Map<int, int>, n: int)
    requires n > 0
    ensures ev(env, bin(Op::Div, num(0), num(n))) == Option::Some(0)
{
    reveal_with_fuel(ev, 2);
}

// ── the theorem ─────────────────────────────────────────────────────────

proof fn simplify_sound(a: A, full: bool, env: Map<int, int>)
    ensures ev(env, simplify(a, full)) == ev(env, a),
    decreases a,
{
    reveal_with_fuel(ev, 2);
    match a {
        A::Bin(op, l, r) => {
            simplify_sound(*l, full, env);
            simplify_sound(*r, full, env);
            let l2 = simplify(*l, full);
            let r2 = simplify(*r, full);
            match op {
                Op::Add => {
                    if full && is_num(r2, 0) {
                        add_zero_right(env, l2);
                    } else if full && is_num(l2, 0) {
                        add_zero_left(env, r2);
                    }
                }
                Op::Sub => {
                    if is_num(r2, 0) {
                        sub_zero_right(env, l2);
                    }
                }
                Op::Mul => {
                    if is_num(r2, 1) {
                        mul_one_right(env, l2);
                    } else if is_num(l2, 1) {
                        mul_one_left(env, r2);
                    }
                }
                Op::Div => {
                    if is_num(r2, 1) {
                        div_one_right(env, l2);
                    } else if is_num(l2, 0) && is_positive_literal(r2) {
                        match r2 {
                            A::Num(n) => zero_over_positive(env, n),
                            _ => {},
                        }
                    }
                }
                _ => {}
            }
        }
        A::Un(op, x) => {
            simplify_sound(*x, full, env);
        }
        A::Cond(t, x, y) => {
            simplify_sound(*t, full, env);
            simplify_sound(*x, full, env);
            simplify_sound(*y, full, env);
        }
        _ => {}
    }
}

proof fn the_registered_rewrite_is_sound(a: A, full: bool)
    ensures forall|env: Map<int, int>| ev(env, simplify(a, full)) == ev(env, a)
{
    assert forall|env: Map<int, int>| ev(env, simplify(a, full)) == ev(env, a) by {
        simplify_sound(a, full, env);
    }
}

// ── refusal pins: each guard is load-bearing ────────────────────────────

// `0 / 0` stays unfolded, and folding it would have changed the error.
proof fn zero_divided_by_zero_is_refused()
{
    assert(simplify(bin(Op::Div, num(0), num(0)), false)
        == bin(Op::Div, num(0), num(0))) by (compute_only);
    assert(ev(Map::<int, int>::empty(), bin(Op::Div, num(0), num(0))) == Option::None)
        by (compute_only);
    assert(ev(Map::<int, int>::empty(), num(0)) == Option::Some(0)) by (compute_only);
}

// `0 / -5` stays unfolded (negative divisor).
proof fn zero_over_negative_is_refused()
{
    assert(simplify(bin(Op::Div, num(0), num(-5)), false)
        == bin(Op::Div, num(0), num(-5))) by (compute_only);
}

// `x * 0` and `0 * x` stay unfolded (operand-eliminating).
proof fn multiply_by_zero_is_refused()
{
    assert(simplify(bin(Op::Mul, var(0), num(0)), false)
        == bin(Op::Mul, var(0), num(0))) by (compute_only);
    assert(simplify(bin(Op::Mul, num(0), var(0)), false)
        == bin(Op::Mul, num(0), var(0))) by (compute_only);
}

// `x - x` and `x % 1` stay unfolded.
proof fn subtract_self_and_mod_one_are_refused()
{
    assert(simplify(bin(Op::Sub, var(0), var(0)), false)
        == bin(Op::Sub, var(0), var(0))) by (compute_only);
    assert(simplify(bin(Op::Mod, var(0), num(1)), false)
        == bin(Op::Mod, var(0), num(1))) by (compute_only);
}

// The `+ 0` arms are GATED: with `full = false` the tree is unchanged,
// with `full = true` it folds.
proof fn add_zero_is_gated()
{
    assert(simplify(bin(Op::Add, var(0), num(0)), false)
        == bin(Op::Add, var(0), num(0))) by (compute_only);
    assert(simplify(bin(Op::Add, var(0), num(0)), true) == var(0)) by (compute_only);
}

// Ungated `0 / 0` would be the wrong fold: the error is observable.
proof fn the_zero_divisor_guard_is_load_bearing()
{
    assert(ev(Map::<int, int>::empty(), bin(Op::Div, num(0), num(0))) == Option::None)
        by (compute_only);
    assert(ev(Map::<int, int>::empty(), num(0)) != Option::None) by (compute_only);
}

fn main() {}
}
