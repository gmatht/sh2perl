// Standalone model of arith_bounds (sh2perl/src/transforms/i32_provable.rs)
// with its soundness obligation. Mirrors the enum's relevant constructors;
// the Rust's Box<ArithAst> becomes Verus's Box<> directly.
use vstd::prelude::*;
use vstd::math::{min, max};

verus! {

pub enum ArithAst {
    Num(i64),
    Bin(u8, Box<ArithAst>, Box<ArithAst>),
    Un(u8, Box<ArithAst>),
    Cond(Box<ArithAst>, Box<ArithAst>, Box<ArithAst>),
    Other,
}

pub open spec fn i32_min() -> int { -2147483648int }
pub open spec fn i32_max() -> int { 2147483647int }

pub open spec fn in_i32(lo: int, hi: int) -> bool {
    i32_min() <= lo && lo <= hi && hi <= i32_max()
}

// Spec-level value of a node, in unbounded int. Only + - * and unary -
// reach it under the verdict; everything else is 0 (unreachable).
pub open spec fn ev(a: ArithAst) -> int
    decreases a
{
    match a {
        ArithAst::Num(n) => n as int,
        ArithAst::Bin(op, l, r) =>
            if op == 0u8 { ev(*l) + ev(*r) }
            else if op == 1u8 { ev(*l) - ev(*r) }
            else if op == 2u8 { ev(*l) * ev(*r) }
            else { 0 },
        ArithAst::Un(op, x) => if op == 1u8 { -ev(*x) } else { 0 },
        ArithAst::Cond(t, a, b) => if ev(*t) != 0 { ev(*a) } else { ev(*b) },
        ArithAst::Other => 0,
    }
}

pub open spec fn bounds(a: ArithAst) -> Option<(int, int)>
    decreases a
{
    match a {
        ArithAst::Num(n) => {
            let v = n as int;
            if in_i32(v, v) { Some((v, v)) } else { None }
        },
        ArithAst::Bin(op, l, r) => {
            match bounds(*l) {
                None => None,
                Some(p) => {
                    let (l0, l1) = p;
                    match bounds(*r) {
                        None => None,
                        Some(q) => {
                            let (r0, r1) = q;
                            if op == 0u8 {
                                if in_i32(l0 + r0, l1 + r1) { Some((l0 + r0, l1 + r1)) } else { None }
                            } else if op == 1u8 {
                                if in_i32(l0 - r1, l1 - r0) { Some((l0 - r1, l1 - r0)) } else { None }
                            } else if op == 2u8 {
                                let c1 = l0 * r0;
                                let c2 = l0 * r1;
                                let c3 = l1 * r0;
                                let c4 = l1 * r1;
                                let lo = min(min(c1, c2), min(c3, c4));
                                let hi = max(max(c1, c2), max(c3, c4));
                                if in_i32(lo, hi) { Some((lo, hi)) } else { None }
                            } else { None }
                        },
                    }
                },
            }
        },
        ArithAst::Un(op, x) => {
            match bounds(*x) {
                None => None,
                Some(p) => {
                    let (x0, x1) = p;
                    if op == 1u8 {
                        if in_i32(-x1, -x0) { Some((-x1, -x0)) } else { None }
                    } else { None }
                },
            }
        },
        ArithAst::Cond(t, a, b) => {
            match bounds(*t) {
                None => None,
                Some(_) => match bounds(*a) {
                    None => None,
                    Some(p) => {
                        let (a0, a1) = p;
                        match bounds(*b) {
                            None => None,
                            Some(q) => {
                                let (b0, b1) = q;
                                let lo = min(a0, b0);
                                let hi = max(a1, b1);
                                if in_i32(lo, hi) { Some((lo, hi)) } else { None }
                            },
                        }
                    },
                },
            }
        },
        ArithAst::Other => None,
    }
}

// THE OBLIGATION.
proof fn bounds_sound(a: ArithAst)
    ensures
        forall|lo: int, hi: int| bounds(a) == Some((lo, hi)) ==>
            lo <= ev(a) && ev(a) <= hi && in_i32(lo, hi),
    decreases a
{
    match a {
        ArithAst::Num(n) => {},
        ArithAst::Bin(op, l, r) => {
            bounds_sound(*l);
            bounds_sound(*r);
            if op == 2u8 {
                // The product corner lemma, for the values the children's
                // own bounds actually give.  `nonlinear_arith` handles the
                // product reasoning; the min/max unfolding is supplied.
                let p = bounds(*l);
                let q = bounds(*r);
                if p is Some && q is Some {
                    let (l0, l1) = p.unwrap();
                    let (r0, r1) = q.unwrap();
                    assert(l0 <= ev(*l) && ev(*l) <= l1) by {
                        bounds_sound(*l);
                    }
                    assert(r0 <= ev(*r) && ev(*r) <= r1) by {
                        bounds_sound(*r);
                    }
                    assert(
                        min(min(l0*r0, l0*r1), min(l1*r0, l1*r1)) <= ev(*l)*ev(*r)
                        && ev(*l)*ev(*r) <= max(max(l0*r0, l0*r1), max(l1*r0, l1*r1))
                    ) by (nonlinear_arith)
                        requires
                            l0 <= ev(*l) && ev(*l) <= l1,
                            r0 <= ev(*r) && ev(*r) <= r1,
                            min(min(l0*r0, l0*r1), min(l1*r0, l1*r1)) == l0*r0
                                || min(min(l0*r0, l0*r1), min(l1*r0, l1*r1)) == l0*r1
                                || min(min(l0*r0, l0*r1), min(l1*r0, l1*r1)) == l1*r0
                                || min(min(l0*r0, l0*r1), min(l1*r0, l1*r1)) == l1*r1,
                            max(max(l0*r0, l0*r1), max(l1*r0, l1*r1)) == l0*r0
                                || max(max(l0*r0, l0*r1), max(l1*r0, l1*r1)) == l0*r1
                                || max(max(l0*r0, l0*r1), max(l1*r0, l1*r1)) == l1*r0
                                || max(max(l0*r0, l0*r1), max(l1*r0, l1*r1)) == l1*r1,
                    ;
                }
            }
        },
        ArithAst::Un(op, x) => {
            bounds_sound(*x);
        },
        ArithAst::Cond(t, a2, b) => {
            bounds_sound(*t);
            bounds_sound(*a2);
            bounds_sound(*b);
        },
        ArithAst::Other => {},
    }
}

fn main() {}

}
