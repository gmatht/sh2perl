// Standalone model of the RENDERER decision that consumes the i32-provable
// verdict (sh2perl/src/transforms/i32_provable.rs `arith_abs_bound` /
// `exact_i32_bound`) at the C backend's i64 tier
// (sh2perl/src/c_backend.rs `tiered_i64_expr` + `plain_i64_expr`).
//
// THE DECISION, verbatim in structure:
//   * `exact_i32_bound(a)` is `Some(b)`  ->  the EXACT-WIDTH form: emit
//     `plain_i64_expr(a)` (a bare C expression over `LL` literals) and drop
//     the `__builtin_add/sub/mul_overflow` guard for that subtree. Every
//     value in the tree — including every INTERMEDIATE result — is provably
//     inside i32, so neither the `long long` computation nor the i32 the
//     values are meant to occupy can overflow: the dropped check is dead.
//   * `None`  ->  the WIDTH-SAFE path: every check the tier emits stays,
//     and the store keeps its GMP fallback (the `_tt*` temp + `ov |=` +
//     `if (ov) { ... mpz ... }` arms). Nothing is narrowed on a refusal.
//
// TWO ARMS, ONE TOTAL DECISION. `decide` is a function of the verdict only;
// `marked_plain_domain` shows the fast path's renderer (`plain_i64_expr`)
// covers every tree the verdict can mark, so a marked tree can never fall
// through into a checked arm and a checked arm can never be reached with a
// marked node — the arms are exhaustive and disjoint, not "fast path tries,
// checked path catches".
//
// WHY THIS EXISTS — AND ITS MEASURED REACH (2026-11-09). The transform has
// been registered since it landed and had NO reader: `is_i32` was called
// from nothing in src/. That is the shape `interval.rs` shipped in (a green
// proof of a decision nothing made), so the model comes with the call site,
// not instead of it. The call site is measured INERT on the corpus, which
// the model states rather than hides:
//   * 12 marked statements across 10 of 896 corpus examples (1.3%) — the
//     verdict's domain is literal-only, because `arith_abs_bound` refuses
//     every variable read (its own `prereqs:` line names the escape-classes
//     interval refinement as outstanding);
//   * and `tiered_i64_expr` is only reached from `tiered_store_general`,
//     which needs a tiered (growing) var in the tree; a literal-only
//     subtree is the only thing this verdict can mark, so the fast path
//     fires on 0 of the 896 examples today.
// A literal-only tree is a compile-time constant to every backend, so the
// dropped guard buys nothing until the estimator can bound variables — the
// theorem below is what that refinement must keep true, and the two
// counterexamples (`leaf_only_mutant_unsound`, `var_refused_by_design`) are
// the guard rails for the work.
//
// WHAT IS PROVED:
//   * bound_covers_all     — `bound(a) = Some(b)` makes EVERY node of the
//                            tree satisfy `|value| <= own bound < 2^31`
//                            (not just the root: intermediates included,
//                            which is exactly what the dropped guard needs;
//                            per-node bounds, not the root's, because a
//                            product can have a root bound SMALLER than an
//                            operand's — `2 * 0`);
//   * all_ok_fits_llong    — every such value therefore fits `long long`;
//   * dropped_check_dead   — the `__builtin_*_overflow` guard the fast path
//                            drops could not have fired;
//   * marked_plain_domain  — a marked tree is entirely Num/Un(-)/Bin(+,-,*)
//                            /Cond, i.e. `plain_i64_expr` has an arm for
//                            every node of it (no silent `None` fallback);
//   * decision_total       — `decide` returns Plain iff the verdict marks;
//                            an unmarked tree keeps every check (the
//                            width-safe path), so refusing costs only the
//                            narrow form and never correctness;
//   * leaf_only_mutant_unsound — the PRE-FIX estimator (leaves only) marks
//                            `2^29 * 2^29 * 2^29 = 2^87`; the shipped
//                            composed bound refuses it, and a renderer
//                            acting on the mutant's marking would emit the
//                            plain form, whose value overflows `long long`
//                            — the counterexample the composed estimate
//                            exists for (I32Provable.thy machine-checks the
//                            same shape Isabelle-side);
//   * var_refused_by_design — a variable read is never marked, which is why
//                            this call site cannot bypass the tiered
//                            mirrors, and why the refinement is the
//                            prerequisite rather than a nicety.
//
// WHAT IS NOT MODELLED: the emission TEXT (the model proves the decision and
// the arithmetic it licenses, not C string formatting), the GMP promotion
// in the width-safe arm beyond "it is exact", and the tiered var selection
// that decides when `tiered_i64_expr` is reached at all.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 i32_emit.rs
use vstd::prelude::*;

verus! {

/// The shipped estimator's tree, reduced to the shapes it can mark. `Other`
/// stands for every arm `arith_abs_bound` refuses (Var/Ident/Index/Assign/
/// ++--/sizeof/Cast/div/mod/`**`/comparisons/unary `+`).
pub enum Arith {
    Num(int),
    Un(int, Box<Arith>),                    // -1 = unary minus
    Bin(int, Box<Arith>, Box<Arith>),       // 0 = +, 1 = -, 2 = *
    Cond(Box<Arith>, Box<Arith>, Box<Arith>),
    Other,
}

pub open spec fn abs_b(x: int) -> int { if x < 0 { -x } else { x } }

/// `|v| <= b`, spelled with inequalities so the proofs stay linear.
pub open spec fn in_range(v: int, b: int) -> bool { -b <= v && v <= b }

/// `arith_abs_bound`'s two constants (src/transforms/i32_provable.rs).
pub open spec fn i32_abs_bound() -> int { 0x8000_0000 }     // 2^31
pub open spec fn literal_abs_bound() -> int { 0x4000_0000 } // 2^30

/// `LLONG_MAX` — the domain `tiered_i64_expr` computes in.
pub open spec fn llong_max() -> int { 0x7FFF_FFFF_FFFF_FFFF }

pub open spec fn eval(a: Arith) -> int
    decreases a,
{
    match a {
        Arith::Num(n) => n,
        Arith::Un(s, x) => if s < 0 { -eval(*x) } else { eval(*x) },
        Arith::Bin(k, l, r) => if k == 0 {
            eval(*l) + eval(*r)
        } else if k == 1 {
            eval(*l) - eval(*r)
        } else {
            eval(*l) * eval(*r)
        },
        Arith::Cond(t, a1, a2) => if eval(*t) != 0 { eval(*a1) } else { eval(*a2) },
        Arith::Other => 0,
    }
}

/// `arith_abs_bound`, verbatim: literals keep the stricter 2^30; `+`/`-`
/// compose `l + r`; `*` composes `l * r`; every COMPOSED bound must stay
/// under 2^31 or the node is refused; `Cond` needs a bounded test and both
/// branches bounded (the value is one of them); everything else refuses.
pub open spec fn bound(a: Arith) -> Option<int>
    decreases a,
{
    match a {
        Arith::Num(n) => if abs_b(n) < literal_abs_bound() {
            Some(abs_b(n))
        } else {
            None
        },
        Arith::Un(s, x) => if s < 0 { bound(*x) } else { None },
        Arith::Bin(k, l, r) => {
            if !(k == 0 || k == 1 || k == 2) {
                None
            } else {
                match (bound(*l), bound(*r)) {
                    (Some(x), Some(y)) => {
                        let s = if k == 2 { x * y } else { x + y };
                        if s < i32_abs_bound() { Some(s) } else { None }
                    },
                    _ => None,
                }
            }
        },
        Arith::Cond(t, a1, a2) => match (bound(*t), bound(*a1), bound(*a2)) {
            (Some(_), Some(x), Some(y)) => {
                let m = if x > y { x } else { y };
                if m < i32_abs_bound() { Some(m) } else { None }
            },
            _ => None,
        },
        Arith::Other => None,
    }
}

/// This node's value is inside its own bound, and that bound is inside i32.
pub open spec fn node_ok(a: Arith) -> bool {
    match bound(a) {
        Some(b) => in_range(eval(a), b) && b < i32_abs_bound(),
        None => false,
    }
}

/// ... and the same for every node of the tree.
pub open spec fn all_ok(a: Arith) -> bool
    decreases a,
{
    node_ok(a)
        && match a {
            Arith::Num(_) => true,
            Arith::Un(_, x) => all_ok(*x),
            Arith::Bin(_, l, r) => all_ok(*l) && all_ok(*r),
            Arith::Cond(t, a1, a2) => all_ok(*t) && all_ok(*a1) && all_ok(*a2),
            Arith::Other => true,
        }
}

/// Every node's value fits the `long long` the tier computes in.
pub open spec fn all_fits_llong(a: Arith) -> bool
    decreases a,
{
    in_range(eval(a), llong_max())
        && match a {
            Arith::Num(_) => true,
            Arith::Un(_, x) => all_fits_llong(*x),
            Arith::Bin(_, l, r) => all_fits_llong(*l) && all_fits_llong(*r),
            Arith::Cond(t, a1, a2) => {
                all_fits_llong(*t) && all_fits_llong(*a1) && all_fits_llong(*a2)
            },
            Arith::Other => true,
        }
}

/// Does `plain_i64_expr` have an arm for every node? (Num, unary -, +, -,
/// *, ?: — and Cast, which the estimator refuses, so it is not required.)
pub open spec fn plain_ok(a: Arith) -> bool
    decreases a,
{
    match a {
        Arith::Num(_) => true,
        Arith::Un(s, x) => s < 0 && plain_ok(*x),
        Arith::Bin(k, l, r) => (k == 0 || k == 1 || k == 2) && plain_ok(*l) && plain_ok(*r),
        Arith::Cond(t, a1, a2) => plain_ok(*t) && plain_ok(*a1) && plain_ok(*a2),
        Arith::Other => false,
    }
}

pub enum Emit {
    Plain,
    Checked,
}

/// The two-armed decision the renderer makes, in `tiered_i64_expr`'s order:
/// the verdict first, the checked arms otherwise.
pub open spec fn decide(a: Arith) -> Emit {
    match bound(a) {
        Some(_) => Emit::Plain,
        None => Emit::Checked,
    }
}

// ── the verdict is a range, node by node ─────────────────────────────────

proof fn bound_nonneg(a: Arith)
    ensures
        match bound(a) {
            Some(b) => b >= 0,
            None => true,
        },
    decreases a,
{
    match a {
        Arith::Num(_) => {}
        Arith::Un(_s, x) => bound_nonneg(*x),
        Arith::Bin(_k, l, r) => {
            bound_nonneg(*l);
            bound_nonneg(*r);
        }
        Arith::Cond(t, a1, a2) => {
            bound_nonneg(*t);
            bound_nonneg(*a1);
            bound_nonneg(*a2);
        }
        Arith::Other => {}
    }
}

proof fn abs_covers(x: int)
    ensures
        in_range(x, abs_b(x)),
{
    if x < 0 {
    } else {
    }
}

proof fn bound_covers_all(a: Arith)
    ensures
        match bound(a) {
            Some(_) => all_ok(a),
            None => true,
        },
    decreases a,
{
    match a {
        Arith::Num(n) => {
            abs_covers(n);
        }
        Arith::Un(_s, x) => {
            bound_covers_all(*x);
        }
        Arith::Bin(k, l, r) => {
            bound_covers_all(*l);
            bound_covers_all(*r);
            bound_nonneg(*l);
            bound_nonneg(*r);
            match (bound(*l), bound(*r)) {
                (Some(x), Some(y)) => {
                    assert(in_range(eval(*l), x));
                    assert(in_range(eval(*r), y));
                    if k == 2 {
                        // |l * r| <= x * y for non-negative x, y
                        assert(in_range(eval(*l) * eval(*r), x * y)) by (nonlinear_arith)
                            requires
                                in_range(eval(*l), x),
                                in_range(eval(*r), y),
                                x >= 0,
                                y >= 0,
                        ;
                    } else {
                        assert(in_range(eval(*l) + eval(*r), x + y));
                        assert(in_range(eval(*l) - eval(*r), x + y));
                    }
                }
                _ => {}
            }
        }
        Arith::Cond(t, a1, a2) => {
            bound_covers_all(*t);
            bound_covers_all(*a1);
            bound_covers_all(*a2);
            bound_nonneg(*a1);
            bound_nonneg(*a2);
        }
        Arith::Other => {}
    }
}

proof fn all_ok_fits_llong(a: Arith)
    requires
        all_ok(a),
    ensures
        all_fits_llong(a),
    decreases a,
{
    match a {
        Arith::Num(_) => {}
        Arith::Un(_s, x) => all_ok_fits_llong(*x),
        Arith::Bin(_k, l, r) => {
            all_ok_fits_llong(*l);
            all_ok_fits_llong(*r);
        }
        Arith::Cond(t, a1, a2) => {
            all_ok_fits_llong(*t);
            all_ok_fits_llong(*a1);
            all_ok_fits_llong(*a2);
        }
        Arith::Other => {}
    }
}

proof fn range_fits_llong(v: int, b: int)
    requires
        in_range(v, b),
        b < i32_abs_bound(),
    ensures
        in_range(v, llong_max()),
{
}

proof fn bound_sound(a: Arith)
    ensures
        match bound(a) {
            Some(b) => in_range(eval(a), b),
            None => true,
        },
    decreases a,
{
    bound_covers_all(a);
}

// ── the fast path cannot meet a shape it cannot render ────────────────────

proof fn marked_plain_domain(a: Arith)
    requires
        bound(a) is Some,
    ensures
        plain_ok(a),
    decreases a,
{
    match a {
        Arith::Num(_) => {}
        Arith::Un(_s, x) => {
            assert(bound(*x) is Some);
            marked_plain_domain(*x);
        }
        Arith::Bin(_k, l, r) => {
            assert(bound(*l) is Some);
            assert(bound(*r) is Some);
            marked_plain_domain(*l);
            marked_plain_domain(*r);
        }
        Arith::Cond(t, a1, a2) => {
            assert(bound(*t) is Some);
            assert(bound(*a1) is Some);
            assert(bound(*a2) is Some);
            marked_plain_domain(*t);
            marked_plain_domain(*a1);
            marked_plain_domain(*a2);
        }
        Arith::Other => {}
    }
}

proof fn marked_plain_domain_if(a: Arith)
    ensures
        bound(a) is Some ==> plain_ok(a),
{
    if bound(a) is Some {
        marked_plain_domain(a);
    }
}

// ── the guard the fast path drops is dead ────────────────────────────────

proof fn dropped_check_dead(a: Arith)
    ensures
        bound(a) is Some ==> all_fits_llong(a),
    decreases a,
{
    if bound(a) is Some {
        bound_covers_all(a);
        all_ok_fits_llong(a);
    }
}

// ── the decision is total, and a refusal keeps the width-safe path ────────

proof fn decision_total(a: Arith)
    ensures
        bound(a) is Some ==> decide(a) == Emit::Plain,
        bound(a) is None ==> decide(a) == Emit::Checked,
{
}

// ── negative controls ───────────────────────────────────────────────────

/// The PRE-FIX estimator: a leaf-only check (every literal under 2^30) with
/// no composed bound. This is the mutant the composed estimate replaced.
pub open spec fn leaf_bound(a: Arith) -> Option<int>
    decreases a,
{
    match a {
        Arith::Num(n) => if abs_b(n) < literal_abs_bound() {
            Some(abs_b(n))
        } else {
            None
        },
        Arith::Un(s, x) => if s < 0 { leaf_bound(*x) } else { None },
        Arith::Bin(_k, l, r) => match (leaf_bound(*l), leaf_bound(*r)) {
            (Some(x), Some(y)) => Some(if x > y { x } else { y }),
            _ => None,
        },
        Arith::Cond(t, a1, a2) => match (leaf_bound(*t), leaf_bound(*a1), leaf_bound(*a2)) {
            (Some(x), Some(y), Some(z)) => {
                let m = if y > z { y } else { z };
                Some(if x > m { x } else { m })
            }
            _ => None,
        },
        Arith::Other => None,
    }
}

pub open spec fn two29() -> int { 0x2000_0000 } // 2^29

/// `2^29 * 2^29 * 2^29` — three literals under 2^30, only `*`, value 2^87.
pub open spec fn mut_t() -> Arith {
    Arith::Bin(
        2,
        Box::new(Arith::Bin(2, Box::new(Arith::Num(two29())), Box::new(Arith::Num(two29())))),
        Box::new(Arith::Num(two29())),
    )
}

proof fn leaf_only_mutant_unsound()
    ensures
        leaf_bound(mut_t()) is Some,
        bound(mut_t()) is None,
        decide(mut_t()) == Emit::Checked,
        eval(mut_t()) > llong_max(),
{
    // The mutant marks the tree (all leaves are small); the shipped composed
    // bound refuses it (2^58 escapes 2^31 at the first compose step) and the
    // decision takes the checked arm; and the tree's actual value is 2^87,
    // i.e. a plain `long long` computation of it would overflow — exactly the
    // false positive I32Provable.thy records Isabelle-side.
    assert(leaf_bound(mut_t()) is Some) by (compute_only);
    assert(bound(mut_t()) is None) by (compute_only);
    assert(eval(mut_t()) == 154742504910672534362390528) by (compute_only);
}

proof fn var_refused_by_design(x: Arith)
    ensures
        bound(Arith::Other) is None,
        decide(Arith::Bin(0, Box::new(Arith::Other), Box::new(x))) == Emit::Checked,
{
    // A variable read is `Other`, so any tree containing one takes the
    // checked arm: the tiered mirrors are never bypassed, and this call site
    // stays inert until the estimator can bound variables.
}

fn main() {}

} // verus!
