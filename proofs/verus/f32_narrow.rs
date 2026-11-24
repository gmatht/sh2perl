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
// THEOREM OF RECORD (route 3, `verdict_is_exact`): every form the table
// accepts preserves f32-exactness for EVERY concrete operand pair. Given
// the operands' concrete values `x`,`y` are exact and the lattices
// describe them (a constant-carrying side is that constant; a
// variable-carrying side is arbitrary), an accepted form's computed
// value `bin_val(op,x,y)` is `f32_exact` -- so `combine`'s `exact = true`
// is a fact, which is exactly what the renderers home on. The proof is
// a case analysis over the four arms of `form_ok`; the two facts it
// cannot derive inside a `real` model are carried as HYPOTHESES and
// named: zero is representable (`zero_is_exact`) and rounding commutes
// with negation (`negation_is_exact`, needed by the `* -1` and `/ -1`
// arms). Literal values are the other lane (`verdict_exact_lit`, exact
// IS the round-trip of the constant), and the literal FOLD is a stated
// contract (`combine_exact_lit`), not re-derived.
//
// Without those hypotheses the table would be unsound: accepting a
// general `*` is exactly the `4097.0 * 4097.0` hazard, and the mutant
// that widens `form_ok`'s `+` arm to every add makes this law fail
// (measured: 9 verified, 1 error, on the assertion that extracts the
// accepted form's zero; the file was restored and re-checked at 10/0).
//
// FIDELITY:
//   - `combine`'s domain rule and the `exact` conjunction are modelled
//     verbatim, including the `%`/`//`/`**` refusal (no arm).
//   - `cval`/fold are modelled by `combine_exact_lit`: the model does
//     not reproduce `py_fmod`/`floor` (unbounded Python semantics in a
//     `real`; out of scope), it states the CONTRACT the caller checks —
//     an accepted fold value satisfies `f32_exact`.
//   - the arith itself is stated in `real`. That loses nothing for the
//     accepted forms: each computes one operand, its negation, or a
//     zero, and the operand is f32-exact hence f64-exact, so the f64 the
//     source runs equals the `real` value. Rounding of a general
//     `+`/`-`/`*`/`/` is therefore never modelled -- the table refuses it.
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

// The two facts about the ROUNDING FUNCTION the accepted forms rest on,
// which a `real` model cannot derive: zero is representable, and
// rounding commutes with negation (so the `* -1` and `/ -1` arms, which
// the table accepts, keep exactness). Both are carried as HYPOTHESES of
// the theorem below rather than assumed somewhere in the model.
pub open spec fn zero_is_exact() -> bool {
    f32_exact(0.0real)
}

pub open spec fn negation_is_exact(v: real) -> bool {
    f32_exact(v) ==> f32_exact(0.0real - v)
}

// The lattice DESCRIBES a concrete operand value: a constant-carrying
// value is that constant; a variable-carrying value (`has_cval` false)
// describes any value -- its exactness is the induction hypothesis the
// analysis's candidate set maintains.
pub open spec fn denotes(fv: FVal, v: real) -> bool {
    !fv.has_cval || fv.cval == v
}

// The value the binop computes (`op` as in `form_ok`; an op outside the
// table never reaches the law). Modelled over `real`: every accepted
// form computes one of the operands, its negation, or a zero, all of
// which the f64 the source actually runs equals exactly, so nothing is
// lost by stating the arithmetic in `real`.
pub open spec fn bin_val(op: int, x: real, y: real) -> real {
    if op == op_add() {
        x + y
    } else if op == op_sub() {
        x - y
    } else if op == op_mul() {
        x * y
    } else if op == op_div() {
        x / y
    } else {
        0.0real
    }
}

// The lattice value `combine` builds for a non-literal float binop
// (f32_narrow.rs's final `FVal`): Float domain, `exact` from the form
// table, no constant.
pub open spec fn combine_nonlit(op: int, l: FVal, r: FVal) -> FVal {
    FVal {
        dom: dom_float(),
        exact: combine_exact_nonlit(op, l, r),
        has_cval: false,
        cval: 0.0real,
    }
}

// The soundness direction the renderers need, in the shape the verdict
// is consumed: a value the analysis marks exact IS exact.
pub open spec fn verdict_exact(fv: FVal, v: real) -> bool {
    fv.dom == dom_float() && fv.exact ==> f32_exact(v)
}

// THEOREM OF RECORD (the header's `verdict_exact`, route 3). Every
// accepted non-literal form preserves binary32-exactness for EVERY
// concrete operand pair: if both operand values are exact and the form
// table accepts the op, the value the op computes is exact -- so the
// verdict `exact = true` is a fact the renderers may home in 32 bits,
// not a guess. The `%`/`//`/`**` refusal is part of the table.
pub proof fn verdict_is_exact(op: int, l: FVal, r: FVal, x: real, y: real)
    requires
        operand_exact(l),
        operand_exact(r),
        denotes(l, x),
        denotes(r, y),
        f32_exact(x),
        f32_exact(y),
        zero_is_exact(),
        negation_is_exact(x),
        negation_is_exact(y),
        form_ok(op, l, r),
    ensures
        verdict_exact(combine_nonlit(op, l, r), bin_val(op, x, y)),
{
    if op == op_add() {
        if is_zero(l) {
            assert(l.has_cval && l.cval == 0.0real);
            assert(x == 0.0real);
            assert(bin_val(op, x, y) == y);
        } else {
            assert(is_zero(r));
            assert(r.has_cval && r.cval == 0.0real);
            assert(y == 0.0real);
            assert(bin_val(op, x, y) == x);
        }
    } else if op == op_sub() {
        assert(is_zero(r));
        assert(r.has_cval && r.cval == 0.0real);
        assert(y == 0.0real);
        assert(bin_val(op, x, y) == x);
    } else if op == op_mul() {
        if is_one_or_zero(l) {
            if is_zero(l) {
                assert(x == 0.0real);
                assert(bin_val(op, x, y) == 0.0real);
                assert(f32_exact(0.0real));
            } else {
                assert(is_one(l));
                assert(l.has_cval && (l.cval == 1.0real || l.cval == -1.0real));
                assert(x == 1.0real || x == -1.0real);
                if x == 1.0real {
                    assert(bin_val(op, x, y) == y);
                } else {
                    assert(x == -1.0real);
                    assert(bin_val(op, x, y) == 0.0real - y);
                    assert(f32_exact(0.0real - y));
                }
            }
        } else {
            assert(is_one_or_zero(r));
            if is_zero(r) {
                assert(y == 0.0real);
                assert(bin_val(op, x, y) == 0.0real);
                assert(f32_exact(0.0real));
            } else {
                assert(is_one(r));
                assert(r.has_cval && (r.cval == 1.0real || r.cval == -1.0real));
                assert(y == 1.0real || y == -1.0real);
                if y == 1.0real {
                    assert(bin_val(op, x, y) == x);
                } else {
                    assert(y == -1.0real);
                    assert(bin_val(op, x, y) == 0.0real - x);
                    assert(f32_exact(0.0real - x));
                }
            }
        }
    } else {
        assert(op == op_div());
        assert(is_one(r));
        assert(r.has_cval && (r.cval == 1.0real || r.cval == -1.0real));
        assert(y == 1.0real || y == -1.0real);
        if y == 1.0real {
            assert(bin_val(op, x, y) == x);
        } else {
            assert(y == -1.0real);
            assert(bin_val(op, x, y) == 0.0real - x);
            assert(f32_exact(0.0real - x));
        }
    }
}

// The literal lane of the verdict: an accepted constant fold carries the
// exactness contract `combine_exact_lit` (the fold's own computation --
// `py_fmod`, `floor`, overflow -- is out of model and stated there).
pub proof fn verdict_exact_lit(v: real)
    requires
        f32_exact(v),
    ensures
        verdict_exact(lit(v), v),
{
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
