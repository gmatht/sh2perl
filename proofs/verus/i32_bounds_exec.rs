// Verus proof of arith_bounds over the REAL ArithAst shape, with the
// EXECUTABLE function carrying the postcondition.
//
// Proven: bounds_sound on the EXECUTABLE `arith_bounds`:
//   arith_bounds(a) == Some((lo,hi)) ==> lo <= ev(a) <= hi && in_i32(lo,hi)
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 60 i32_bounds_exec.rs
//   -> verification results:: 7 verified, 0 errors
//
// The LANDING file for this proof is interval.rs (proofs/verus): the same
// obligation on a standalone model. This file keeps the full-enum,
// exec-carrying form as the stronger artifact; see its DESIGN note.
//
// DESIGN, learned from 30 probes. Verus cannot express `r == bd(*a)` where
// exec returns `(i128, i128)` and the spec is over `int`, and a `match r`
// postcondition over a tuple the function itself built proves nothing about
// the elements. The working shape is:
//
//   fn arith_bounds(a: &ArithAst) -> (r: (bool, i128, i128))
//     ensures r.0 ==> r.1 as int <= ev(a) && ev(a) <= r.2 as int
//                   && within_i32(r.1 as int, r.2 as int),
//
// i.e. the `Option` becomes a `(found, lo, hi)` triple and `?` becomes
// explicit `if !l.0 || !r.0` guards. `checked_*` calls cross into `as int`
// via vstd's own specs (probe19), and each arm asserts the recursive facts
// before composing (probes 27-34 showed bare propagation fails).
//
// This is a DUPLICATE of sh2perl/src/transforms/i32_provable.rs's
// arith_bounds, not a spec ON it: Verus cannot take a crate dependency on
// the 17k-line crate. Only two mechanical rewrites separate them:
// match-on-&str -> if-chains, and `x.then_some(y)` -> explicit if.
// Divergence risk is real and named in the worktree diff.
use vstd::prelude::*;
use vstd::math::{min, max};

verus! {

pub assume_specification[i128::checked_neg](x: i128) -> (r: Option<i128>)
    ensures
        r is Some ==> r.unwrap() == (0i128 - x),
        x == -170141183460469231731687303715884105728i128 ==> r is None;

pub enum ArithAst {
    Num(i64),
    Var(String),
    Ident(String),
    Index { var: String, key: Box<ArithAst> },
    Bin { op: String, lhs: Box<ArithAst>, rhs: Box<ArithAst> },
    Un { op: String, arg: Box<ArithAst> },
    Cond { test: Box<ArithAst>, then_: Box<ArithAst>, else_: Box<ArithAst> },
    Assign { var: String, op: String, rhs: Box<ArithAst> },
    IncDec { var: String, delta: i64, prefix: bool },
    Other,
}

pub open spec fn i32_min() -> int { -2147483648int }
pub open spec fn i32_max() -> int { 2147483647int }

pub open spec fn in_i32(lo: int, hi: int) -> bool {
    i32_min() <= lo && lo <= hi && hi <= i32_max()
}

pub open spec fn within_i32(lo: int, hi: int) -> bool {
    in_i32(lo, hi)
}

pub open spec fn op_is(op: String, c: char) -> bool {
    op@ == Seq::<char>::empty().push(c)
}

pub open spec fn ev(a: &ArithAst) -> int
    decreases *a
{
    match a {
        ArithAst::Num(n) => *n as int,
        ArithAst::Bin { op, lhs, rhs } =>
            if op_is(*op, '+') { ev(lhs) + ev(rhs) }
            else if op_is(*op, '-') { ev(lhs) - ev(rhs) }
            else if op_is(*op, '*') { ev(lhs) * ev(rhs) }
            else { 0 },
        ArithAst::Un { op, arg } =>
            if op@ == Seq::<char>::empty().push('-') { -ev(arg) } else { 0 },
        ArithAst::Cond { test, then_, else_ } =>
            if ev(test) != 0 { ev(then_) } else { ev(else_) },
        _ => 0,
    }
}

pub open spec fn within_i32_exec_spec(lo: i128, hi: i128) -> bool {
    -2147483648 <= lo && lo <= hi && hi <= 2147483647
}

pub fn within_i32_exec(lo: i128, hi: i128) -> (b: bool)
    ensures b == within_i32_exec_spec(lo, hi),
{
    const I32_MIN_I: i128 = -2147483648;
    const I32_MAX_I: i128 = 2147483647;
    I32_MIN_I <= lo && lo <= hi && hi <= I32_MAX_I
}

// THE EXECUTABLE FUNCTION + THE OBLIGATION. Returns (found, lo, hi) so the
// postcondition can name the elements (see DESIGN above).
#[verifier::external_body]
pub fn str_eq_plus(s: &String) -> (b: bool)
    ensures b == op_is(*s, '+'),
{
    s.as_str() == "+"
}

#[verifier::external_body]
pub fn str_eq_minus(s: &String) -> (b: bool)
    ensures b == op_is(*s, '-'),
{
    s.as_str() == "-"
}

#[verifier::external_body]
pub fn str_eq_star(s: &String) -> (b: bool)
    ensures b == op_is(*s, '*'),
{
    s.as_str() == "*"
}

pub fn arith_bounds(a: &ArithAst) -> (r: (bool, i128, i128))
    ensures
        r.0 ==> r.1 as int <= ev(a) && ev(a) <= r.2 as int
                && within_i32(r.1 as int, r.2 as int),
    decreases *a
{
    match a {
        ArithAst::Num(n) => {
            let v = *n as i128;
            assert(v as int == *n as int);
            if within_i32_exec(v, v) {
                assert(v as int <= ev(a) && ev(a) <= v as int);
                (true, v, v)
            } else {
                (false, 0, 0)
            }
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let l = arith_bounds(lhs);
            let r = arith_bounds(rhs);
            assert(l.0 ==> l.1 as int <= ev(lhs) && ev(lhs) <= l.2 as int
                            && within_i32(l.1 as int, l.2 as int));
            assert(r.0 ==> r.1 as int <= ev(rhs) && ev(rhs) <= r.2 as int
                            && within_i32(r.1 as int, r.2 as int));
            if !l.0 || !r.0 {
                (false, 0, 0)
            } else if str_eq_plus(op) {
                match l.1.checked_add(r.1) {
                    None => (false, 0, 0),
                    Some(x) => match l.2.checked_add(r.2) {
                        None => (false, 0, 0),
                        Some(y) => {
                            assert(x as int == l.1 as int + r.1 as int);
                            assert(y as int == l.2 as int + r.2 as int);
                            if within_i32_exec(x, y) {
                                assert(op@ == Seq::<char>::empty().push('+'));
                                assert(ev(a) == ev(lhs) + ev(rhs));
                                assert(x as int <= ev(lhs) + ev(rhs));
                                assert(ev(lhs) + ev(rhs) <= y as int);
                                (true, x, y)
                            } else {
                                (false, 0, 0)
                            }
                        },
                    },
                }
            } else if str_eq_minus(op) {
                match l.1.checked_sub(r.2) {
                    None => (false, 0, 0),
                    Some(x) => match l.2.checked_sub(r.1) {
                        None => (false, 0, 0),
                        Some(y) => {
                            assert(x as int == l.1 as int - r.2 as int);
                            assert(y as int == l.2 as int - r.1 as int);
                            if within_i32_exec(x, y) {
                                (true, x, y)
                            } else {
                                (false, 0, 0)
                            }
                        },
                    },
                }
            } else if str_eq_star(op) {
                match l.1.checked_mul(r.1) {
                    None => (false, 0, 0),
                    Some(_) => match l.1.checked_mul(r.2) {
                        None => (false, 0, 0),
                        Some(_) => match l.2.checked_mul(r.1) {
                            None => (false, 0, 0),
                            Some(_) => match l.2.checked_mul(r.2) {
                                None => (false, 0, 0),
                                Some(_) => {
                                    let c1 = (l.1 * r.1) as i128;
                                    let c2 = (l.1 * r.2) as i128;
                                    let c3 = (l.2 * r.1) as i128;
                                    let c4 = (l.2 * r.2) as i128;
                                    let lo = c1.min(c2).min(c3).min(c4);
                                    let hi = c1.max(c2).max(c3).max(c4);
                                    if within_i32_exec(lo, hi) {
                                        assert(
                                            min(min(l.1 as int*r.1 as int, l.1 as int*r.2 as int),
                                                min(l.2 as int*r.1 as int, l.2 as int*r.2 as int))
                                            <= ev(lhs)*ev(rhs)
                                            && ev(lhs)*ev(rhs)
                                            <= max(max(l.1 as int*r.1 as int, l.1 as int*r.2 as int),
                                                   max(l.2 as int*r.1 as int, l.2 as int*r.2 as int))
                                        ) by (nonlinear_arith)
                                            requires
                                                l.1 as int <= ev(lhs) && ev(lhs) <= l.2 as int,
                                                r.1 as int <= ev(rhs) && ev(rhs) <= r.2 as int,
                                        ;
                                        (true, lo, hi)
                                    } else {
                                        (false, 0, 0)
                                    }
                                },
                            },
                        },
                    },
                }
            } else {
                (false, 0, 0)
            }
        }
        ArithAst::Un { op, arg } => {
            if str_eq_minus(op) {
                let g = arith_bounds(arg);
                assert(g.0 ==> g.1 as int <= ev(arg) && ev(arg) <= g.2 as int
                                && within_i32(g.1 as int, g.2 as int));
                if !g.0 {
                    (false, 0, 0)
                } else {
                    match g.2.checked_neg() {
                        None => (false, 0, 0),
                        Some(x) => match g.1.checked_neg() {
                            None => (false, 0, 0),
                            Some(y) => {
                                assert(x as int == 0 - g.2 as int);
                                assert(y as int == 0 - g.1 as int);
                                if within_i32_exec(x, y) {
                                    (true, x, y)
                                } else {
                                    (false, 0, 0)
                                }
                            },
                        },
                    }
                }
            } else {
                (false, 0, 0)
            }
        }
        ArithAst::Cond { test, then_, else_ } => {
            let t = arith_bounds(test);
            let t2 = arith_bounds(then_);
            let e = arith_bounds(else_);
            assert(t.0 ==> t.1 as int <= ev(test) && ev(test) <= t.2 as int
                            && within_i32(t.1 as int, t.2 as int));
            assert(t2.0 ==> t2.1 as int <= ev(then_) && ev(then_) <= t2.2 as int
                            && within_i32(t2.1 as int, t2.2 as int));
            assert(e.0 ==> e.1 as int <= ev(else_) && ev(else_) <= e.2 as int
                            && within_i32(e.1 as int, e.2 as int));
            if !t.0 || !t2.0 || !e.0 {
                (false, 0, 0)
            } else {
                let lo = t2.1.min(e.1);
                let hi = t2.2.max(e.2);
                if within_i32_exec(lo, hi) {
                    (true, lo, hi)
                } else {
                    (false, 0, 0)
                }
            }
        }
        _ => (false, 0, 0),
    }
}

fn main() {}

}
