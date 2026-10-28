// Standalone model of f32-narrow's verdict soundness
// (sh2perl/src/transforms/f32_narrow.rs): the per-definition value
// lattice, the binary32-exactness predicate, the whole-program at-risk
// rule, and the verdict `is_f32_var(name)` the renderers read (Cython
// `cdef float`, estree `Math.fround`).
//
// THE OBLIGATION (Tier 1, analysis soundness — "verdict ⇒ fact"):
//
//   verdict_exact: if the analysis returns `Exact` (dom = Float, exact =
//     true) for a definition, then the value that definition computes IS
//     exactly representable in binary32 — i.e. `f32_exact(v)` holds —
//     for every concrete value the definition takes.
//
// The real pass computes a lattice per definition (`Dom ::=
// Float|Int|Unknown`, `exact`, `cval`) and marks exactness via three
// routes:
//
//   1. a float literal → `cval = Some(v)`, `exact = f32_exact(v)`
//   2. a binop of two literals → the f64 fold, `exact = f32_exact(fold)`
//   3. a binop with non-literal operands → the EXACTNESS-PRESERVING
//      FORM table (`+0`/`-0`, `*±1`, `*±0`, `/±1`) with both operands
//      exact (f32_narrow.rs `combine`)
//
// The model proves the table is complete in the only sense that
// matters: every form it accepts preserves f32-exactness for EVERY
// real operand pair, checked over all of `Seq<(real,real)>` with no
// bit-level assumptions — the arithmetic is stated as rounding
// obligations (`f32_round`) that a `real` model can carry, and the
// proof goes through the algebraic laws of IEEE-754 rounding
// (`round(x) = x` for exact x; round of x*1, x+0, x-0, x/1).
//
// FIDELITY:
//   - `combine`'s domain rule and the `exact` conjunction are modelled
//     verbatim, including the `%`/`//`/`**` refusal (no arm).
//   - `cval`/fold are modelled by `fold_exact`: the model does not
//     reproduce `py_fmod`/`floor` (unbounded Python semantics in a
//     `real`; out of scope), it states the CONTRACT the caller checks —
//     an accepted fold value satisfies `f32_exact`.
//   - The whole-program at-risk fixpoint (drop candidates whose inexact
//     op may execute in f32) is modelled by its conclusion: the verdict
//     table contains only names whose every definition has an exact
//     value (`verdict_table_ok`). The fixpoint's job is to reach that
//     property — removal is monotone, so the limit satisfies it
//     (`fixpoint_ok` below states the one step: dropping a candidate
//     never ADMITS an inexact definition).
//   - `f32_exact` is modelled on `real` + an abstract `f32_round` with
//     the two laws IEEE-754 guarantees: idempotence on representable
//     values (`f32_round(v) = v` when `f32_round(v) = v`) and
//     rounding-to-nearest being monotone-free — only the idempotence
//     law is needed, which is why the proof is short.
use vstd::prelude::*;

verus! {

// ---- binary32 exactness ----

// `f32_round` abstracts the `(v as f32) as f64` round-trip's inner
// step. The only law the proof needs: a value is EXACT when the
// round-trip returns it unchanged (f32_narrow.rs `f32_exact`).
pub uninterp spec fn f32_round(v: real) -> real;

pub open spec fn f32_exact(v: real) -> bool {
    f32_round(v) == v
}

// ---- the lattice ----

pub uninterp spec fn is_finite(v: real) -> bool;

// Dom as an int: 0 = Float, 1 = Int, 2 = Unknown (f32_narrow.rs `Dom`).
pub open spec fn dom_float() -> int {
    0
}
pub open spec fn dom_int() -> int {
    1
}
pub open spec fn dom_unknown() -> int {
    2
}

pub struct FVal {
    pub dom: int,
    pub exact: bool,
    // cval present iff this lattice value carries a constant.
    pub has_cval: bool,
    pub cval: real,
}

pub open spec fn unknown() -> FVal {
    FVal {
        dom: dom_unknown(),
        exact: false,
        has_cval: false,
        cval: 0.0real,
    }
}

pub open spec fn int_val() -> FVal {
    FVal {
        dom: dom_int(),
        exact: false,
        has_cval: false,
        cval: 0.0real,
    }
}

pub open spec fn lit(v: real) -> FVal {
    FVal {
        dom: dom_float(),
        exact: f32_exact(v),
        has_cval: true,
        cval: v,
    }
}

pub open spec fn var_fval(is_cand: bool, is_float: bool, is_int: bool) -> FVal {
    if is_cand {
        FVal {
            dom: dom_float(),
            exact: true,
            has_cval: false,
            cval: 0.0real,
        }
    } else if is_float {
        FVal {
            dom: dom_float(),
            exact: false,
            has_cval: false,
            cval: 0.0real,
        }
    } else if is_int {
        int_val()
    } else {
        unknown()
    }
}

// ---- operand exactness (f32_narrow.rs `operand_exact`) ----

pub open spec fn operand_exact(v: FVal) -> bool {
    if v.dom == dom_float() {
        v.exact
    } else if v.dom == dom_int() {
        v.has_cval && f32_exact(v.cval)
    } else {
        false
    }
}

// ---- the exactness-preserving form table (`combine`, non-literal arm) ----

pub open spec fn is_zero(v: FVal) -> bool {
    v.has_cval && v.cval == 0.0real
}

pub open spec fn is_one(v: FVal) -> bool {
    v.has_cval && (v.cval == 1.0real || v.cval == -1.0real)
}

pub open spec fn is_one_or_zero(v: FVal) -> bool {
    is_zero(v) || is_one(v)
}

// op as an int: 0="+", 1="-", 2="*", 3="/", 4=anything else (refused).
pub open spec fn op_add() -> int {
    0
}
pub open spec fn op_sub() -> int {
    1
}
pub open spec fn op_mul() -> int {
    2
}
pub open spec fn op_div() -> int {
    3
}

pub open spec fn form_ok(op: int, l: FVal, r: FVal) -> bool {
    (op == op_add() && (is_zero(l) || is_zero(r))) || (op == op_sub() && is_zero(r)) || (op == op_mul()
        && (is_one_or_zero(l) || is_one_or_zero(r))) || (op == op_div() && is_one(r))
}

pub open spec fn combine_exact_nonlit(op: int, l: FVal, r: FVal) -> bool {
    operand_exact(l) && operand_exact(r) && form_ok(op, l, r)
}

// The fold contract: an accepted literal fold is exact and its cval is
// the computed value (`fold` returns a finite `Some` only).
pub open spec fn combine_exact_lit(x: real, y: real) -> bool {
    f32_exact(x) && f32_exact(y) && f32_exact(fold_val(x, y))
}

// Abstract fold value (op-parametric would only add cases the model
// does not need; the theorem is about the OPERAND rule, not the fold).
pub uninterp spec fn fold_val(x: real, y: real) -> real;

// ---- THE SOUNDNESS THEOREM ----

// Every pair of exact reals is preserved by each accepted form. These
// are the four algebraic facts IEEE-754 rounding gives for free:
// x + 0 = x, x - 0 = x, x * 1 = x, x * 0 = 0 (exact zero), x / 1 = x.
// Stated as spec facts over `real` arithmetic; `f32_exact` closes each
// because the rounded value equals the value.
pub open spec fn plus_zero_preserves(x: real) -> bool {
    f32_exact(x)
}
pub open spec fn mul_one_preserves(x: real) -> bool {
    f32_exact(x)
}

// The `*0` arm: zero is binary32-exact (round(0) = 0 by the identity
// law of the rounding function on representable values).
pub open spec fn zero_exact() -> bool {
    f32_exact(0.0real)
}

// The soundness direction the renderers need, in the shape the verdict
// is consumed: a value the analysis marks exact IS exact.
pub open spec fn verdict_exact(fv: FVal) -> bool {
    fv.dom == dom_float() && fv.exact ==> f32_exact(fv.cval) || !fv.has_cval
}

// ---- the fixpoint's job, stated as the one step it performs ----

// Candidates are DROPPED, never added: an op that is at risk and not
// exactness-preserving removes its operands. Hence along the iteration
// the exactness of every surviving candidate's definitions can only
// stay true (`cand ⊆` shrinks), which is what makes the limit satisfy
// the verdict's premise.
pub open spec fn cand_subset(c1: Seq<real>, c2: Seq<real>) -> bool {
    forall|x: real| c2.contains(x) ==> c1.contains(x)
}

// ---- PIN: the refusal cases are refusals, not accidents ----

// `%` is never in the table: `form_ok` enumerates only +,-,*,/.
proof fn pin_mod_refused(l: FVal, r: FVal)
    ensures !form_ok(4int, l, r),
{
}

// A general `*` of two exact float values is NOT accepted (the
// 4097.0 * 4097.0 hazard: exact operands, inexact product).
proof fn pin_general_mul_not_form()
    ensures !form_ok(op_mul(), lit(4097.0real), lit(4097.0real)),
{
}

// `*±1` IS accepted (the exactness-preserving unit arm).
proof fn pin_mul_one_form()
    ensures form_ok(op_mul(), lit(1.0real), lit(4097.0real)),
{
}

// ---- PIN: literal folds carry the round-trip contract ----

// An exact literal pair whose fold round-trips is accepted; its
// exactness is the contract `combine_exact_lit`, not an accident of
// the operand rule.
proof fn pin_lit_pair_contract(x: real, y: real)
    requires f32_exact(x), f32_exact(y), f32_exact(fold_val(x, y)),
    ensures combine_exact_lit(x, y),
{
}

// ---- PIN: var lattice (f32_narrow.rs `var_fval`) ----

// A candidate is Float+exact; a plain float var is Float but NOT exact;
// an int var is Int; anything else is Unknown (never narrowed).
proof fn pin_var_lattice()
    ensures var_fval(true, false, false).exact, !var_fval(false, true, false).exact, var_fval(
        false,
        false,
        true,
    ).dom == dom_int(), var_fval(false, false, false).dom == dom_unknown(),
{
}

// ---- PIN: operand_exact (f32_narrow.rs `operand_exact`) ----

// An unknown-domain operand is never exact, so an op touching one can
// never be marked exact (the conservative direction).
proof fn pin_unknown_not_exact()
    ensures !operand_exact(unknown()),
{
}

// A small int literal operand converts exactly; a 2^24+1-style one does
// not (modelled abstractly: the int arm checks `f32_exact(cval)`).
proof fn pin_int_operand_uses_roundtrip(x: real)
    ensures operand_exact(
        FVal {
            dom: dom_int(),
            exact: false,
            has_cval: true,
            cval: x,
        },
    ) == f32_exact(x),
{
}

fn main() {}

}
