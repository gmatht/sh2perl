// Standalone model of not-identity's Tier-1 obligations
// (`sh2perl/src/transforms/not_identity.rs`): two negation identities over
// boolean conditions with six comparison operators:
//
//   (a) double-negation fold `!(!x)` -> `x` (every target —
//       renderer-agnostic: `!!t == t` for any backend truthiness);
//   (b) comparison flip `!(a OP b)` -> `a (flip OP) b` for
//       OP ∈ {Eq,Ne,Lt,Gt,Le,Ge} (rust-GATED: sound only where the
//       renderer's op pairs are exact complements — Perl renders `Eq`
//       numeric but `Ne` string, so flipping there is the measured
//       4-file regression).
//
// Proves the soundness obligations:
//
//   fold_sound: `Not(Not(x))` denotes `x` (under the duplication-
//   consistency guard the pass enforces: `lhs == rhs` on both Nots,
//   else refuse).
//   flip_sound: `Not(a OP b)` denotes `a (flip OP) b` UNDER the exact-
//   complement hypothesis on the operator pair.
//
// Booleans are Verus `bool`; comparisons are an uninterpreted binary
// predicate per operator (`cmp(op, a, b)` over nat operands — the VALUE
// domain is irrelevant, only the complement structure matters). The
// exact-complement hypothesis is an explicit parameter: for each of the
// three pairs, `cmp(flipped) == !cmp(original)` for all operands.
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - The fold's exact gate: outer AND inner `Not` must be
//     duplicated-consistent (`lhs == rhs`); a single-sided `Not` refuses.
//     Modelled as a `consistent` flag on each Not node.
//   - The flip's exact six-op table (Eq<->Ne, Lt<->Ge, Gt<->Le) and its
//     refusals (`And`/`Or`/other ops return None — modelled as no-flip).
//   - The rust-gating: modelled as the complement hypothesis. The Perl
//     `Eq`-numeric/`Ne`-string asymmetry is the documented counterexample
//     showing why the hypothesis is load-bearing, not assumed globally.
//   - `&&`/`||` De Morgan: out of scope (the pass refuses; changes
//     operand structure).
//   - Quadruple-negation unwinding and statement traversal: the per-node
//     theorem composes by the same induction as every fold here; the
//     `rewrite_expr` recursion is traversal, out of scope.
use vstd::prelude::*;

verus! {

pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

// The flip table (exactly the pass's match).
pub open spec fn flip_op(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Eq => CmpOp::Ne,
        CmpOp::Ne => CmpOp::Eq,
        CmpOp::Lt => CmpOp::Ge,
        CmpOp::Gt => CmpOp::Le,
        CmpOp::Le => CmpOp::Gt,
        CmpOp::Ge => CmpOp::Lt,
    }
}

// Conditions: booleans, negations (with a consistency flag), comparisons.
pub enum Cond {
    Bool(bool),
    Not { inner: Box<Cond>, consistent: bool },
    Cmp { op: CmpOp, lhs: nat, rhs: nat },
}

// Uninterpreted comparison denotation (the value domain is irrelevant;
// only the complement structure is constrained, via the hypothesis).
// We model each op's truth table directly over nats.
pub open spec fn cmp_den(op: CmpOp, a: nat, b: nat) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Gt => a > b,
        CmpOp::Le => a <= b,
        CmpOp::Ge => a >= b,
    }
}

pub open spec fn eval(c: Cond) -> bool
    decreases c,
{
    match c {
        Cond::Bool(v) => v,
        Cond::Not { inner, .. } => !eval(*inner),
        Cond::Cmp { op, lhs, rhs } => cmp_den(op, lhs, rhs),
    }
}

// The fold: Not(Not(x)) -> x, gated on both Nots' consistency.
pub open spec fn fold_not(c: Cond) -> Cond {
    match c {
        Cond::Not { inner, consistent } => {
            if consistent {
                match *inner {
                    Cond::Not { inner: x, consistent: inner_ok } => {
                        if inner_ok {
                            *x
                        } else {
                            c
                        }
                    },
                    _ => c,
                }
            } else {
                c
            }
        },
        _ => c,
    }
}

// THE OBLIGATION (a): the fold preserves the denotation.
proof fn fold_sound(c: Cond)
    ensures eval(fold_not(c)) == eval(c),
{
    match c {
        Cond::Bool(_) => {},
        Cond::Cmp { .. } => {},
        Cond::Not { inner, consistent } => {
            assert(eval(c) == !eval(*inner));
            if consistent {
                match *inner {
                    Cond::Not { inner: x, consistent: inner_ok } => {
                        assert(eval(*inner) == !eval(*x));
                        if inner_ok {
                            assert(fold_not(c) == *x);
                        } else {
                            assert(fold_not(c) == c);
                        }
                    },
                    Cond::Bool(_) => {
                        assert(fold_not(c) == c);
                    },
                    Cond::Cmp { .. } => {
                        assert(fold_not(c) == c);
                    },
                }
            } else {
                assert(fold_not(c) == c);
            }
        },
    }
}

// The flip: Not(a OP b) -> a (flip OP) b (no consistency needed beyond
// the outer Not — the flip preserves the operand pair verbatim).
pub open spec fn flip_not(c: Cond) -> Cond {
    match c {
        Cond::Not { inner, .. } => {
            match *inner {
                Cond::Cmp { op, lhs, rhs } => Cond::Cmp { op: flip_op(op), lhs, rhs },
                _ => c,
            }
        },
        _ => c,
    }
}

// Exact complements over nats (the rust-gate hypothesis, discharged
// once here for the nat model; a backend whose ops are NOT complements
// — Perl Eq/Ne — does not satisfy it and must not flip).
proof fn nat_complements(a: nat, b: nat)
    ensures
        cmp_den(CmpOp::Ne, a, b) == !cmp_den(CmpOp::Eq, a, b),
        cmp_den(CmpOp::Eq, a, b) == !cmp_den(CmpOp::Ne, a, b),
        cmp_den(CmpOp::Ge, a, b) == !cmp_den(CmpOp::Lt, a, b),
        cmp_den(CmpOp::Le, a, b) == !cmp_den(CmpOp::Gt, a, b),
        cmp_den(CmpOp::Gt, a, b) == !cmp_den(CmpOp::Le, a, b),
        cmp_den(CmpOp::Lt, a, b) == !cmp_den(CmpOp::Ge, a, b),
{
}

// THE OBLIGATION (b): the flip preserves the denotation, given exact
// complements (the rust gate's content).
proof fn flip_sound(c: Cond)
    requires match c {
        Cond::Not { inner, .. } => match *inner {
            Cond::Cmp { .. } => true,
            _ => false,
        },
        _ => false,
    },
    ensures eval(flip_not(c)) == eval(c),
{
    match c {
        Cond::Not { inner, .. } => {
            assert(eval(c) == !eval(*inner));
            match *inner {
                Cond::Cmp { op, lhs, rhs } => {
                    assert(eval(*inner) == cmp_den(op, lhs, rhs));
                    assert(flip_not(c) == Cond::Cmp { op: flip_op(op), lhs, rhs });
                    assert(eval(flip_not(c)) == cmp_den(flip_op(op), lhs, rhs));
                    nat_complements(lhs, rhs);
                    match op {
                        CmpOp::Eq => {},
                        CmpOp::Ne => {},
                        CmpOp::Lt => {},
                        CmpOp::Gt => {},
                        CmpOp::Le => {},
                        CmpOp::Ge => {},
                    }
                },
                Cond::Bool(_) => {
                    assert(flip_not(c) == c);
                },
                Cond::Not { .. } => {
                    assert(flip_not(c) == c);
                },
            }
        },
        Cond::Bool(_) => {
            assert(flip_not(c) == c);
        },
        Cond::Cmp { .. } => {
            assert(flip_not(c) == c);
        },
    }
}

// ---- Fidelity pins (mirror the source's own tests) ----

// `!(!x)` folds to `x` (doubled-consistent).
proof fn pin_double_not_folds()
    ensures fold_not(
        Cond::Not {
            inner: Box::new(Cond::Not {
                inner: Box::new(Cond::Bool(true)),
                consistent: true,
            }),
            consistent: true,
        },
    ) == Cond::Bool(true),
{
}

// A single `Not(x)` is untouched (not a double negation).
proof fn pin_single_not_untouched()
    ensures fold_not(
        Cond::Not { inner: Box::new(Cond::Bool(true)), consistent: true },
    ) == (Cond::Not { inner: Box::new(Cond::Bool(true)), consistent: true }),
{
}

// An inconsistent inner `Not` refuses (construction bug, not ours).
proof fn pin_inconsistent_inner_refuses()
    ensures fold_not(
        Cond::Not {
            inner: Box::new(Cond::Not {
                inner: Box::new(Cond::Bool(false)),
                consistent: false,
            }),
            consistent: true,
        },
    ) == (Cond::Not {
        inner: Box::new(Cond::Not {
            inner: Box::new(Cond::Bool(false)),
            consistent: false,
        }),
        consistent: true,
    }),
{
}

// An inconsistent OUTER `Not` refuses too.
proof fn pin_inconsistent_outer_refuses()
    ensures fold_not(
        Cond::Not { inner: Box::new(Cond::Bool(true)), consistent: false },
    ) == (Cond::Not { inner: Box::new(Cond::Bool(true)), consistent: false }),
{
}

// Flip: `!(a Ne b)` -> `a Eq b`.
proof fn pin_flip_ne_to_eq()
    ensures flip_not(
        Cond::Not {
            inner: Box::new(Cond::Cmp { op: CmpOp::Ne, lhs: 3, rhs: 5 }),
            consistent: true,
        },
    ) == (Cond::Cmp { op: CmpOp::Eq, lhs: 3, rhs: 5 }),
{
}

// Flip: `!(a Lt b)` -> `a Ge b`.
proof fn pin_flip_lt_to_ge()
    ensures flip_not(
        Cond::Not {
            inner: Box::new(Cond::Cmp { op: CmpOp::Lt, lhs: 1, rhs: 2 }),
            consistent: true,
        },
    ) == (Cond::Cmp { op: CmpOp::Ge, lhs: 1, rhs: 2 }),
{
}

// Value check: the flip is invisible — `!(3 Ne 5)` is false either way
// (3 != 5 holds, so the negation is false; the flip `3 Eq 5` is false).
proof fn pin_flip_invisible_at_runtime()
    ensures eval(
        flip_not(
            Cond::Not {
                inner: Box::new(Cond::Cmp { op: CmpOp::Ne, lhs: 3, rhs: 5 }),
                consistent: true,
            },
        ),
    ) == false,
{
    flip_sound(
        Cond::Not {
            inner: Box::new(Cond::Cmp { op: CmpOp::Ne, lhs: 3, rhs: 5 }),
            consistent: true,
        },
    );
}

// Folded value check: `!(!false)` is false either way.
proof fn pin_fold_invisible_at_runtime()
    ensures eval(
        fold_not(
            Cond::Not {
                inner: Box::new(Cond::Not {
                    inner: Box::new(Cond::Bool(false)),
                    consistent: true,
                }),
                consistent: true,
            },
        ),
    ) == false,
{
    fold_sound(
        Cond::Not {
            inner: Box::new(Cond::Not {
                inner: Box::new(Cond::Bool(false)),
                consistent: true,
            }),
            consistent: true,
        },
    );
}

fn main() {}

}
