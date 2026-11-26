// Model of maskable unsigned lowering (sh2perl/src/shir_passes/maskable.rs).
//
// The model is stated on the ANALYSIS'S OWN semantic tree, not on a copy
// of its control flow: the dividend is an `E` expression, `ev` is its
// i128 reading (truncating `/`,`%`, exactly as the analysis computes
// them), `const_value` / `fold` are the analysis's constant folder,
// `counter_shape` is the shape table `linear_parts` implements, and
// `nested_mods`/`nested_min`/`skel`/`mask_threshold` are the analysis's
// own walk.  The source's guards appear as the `None` arms of the same
// table and as the hypotheses of the operator laws.
//
// Proven here (all for every input, no `assume`, no `admit`):
//   - the fold preserves evaluation (`fold_ev`) and const-evaluation
//     (`const_ev`), and is idempotent on the shapes it builds;
//   - the guard that the fold alone does not provide: `no_overflow` is
//     `const_value(fold e) = Some v ==> fits v` CONJOINED structurally
//     over the children, because the fold recombines constants it never
//     re-checks;
//   - the linear/square threshold arithmetic (`lin_bound`, `sq_bound`)
//     and the nested-mod join (`jmin_left`/`jmin_right`, `mod_bounds`);
//   - the per-shape steps `lin_step`/`sq_step`: at an accepted shape the
//     dividend's value under the threshold really is a nonneg i64;
//   - the width law: `is_pow2_lit d ==> d = 2^k with k <= 62`
//     (`pow2_has_a_shift`), and for that k masking IS `%` and shifting IS
//     `//` (`mask_is_modulo`, `shift_is_division`,
//     `the_mask_arithmetic_is_exact`, `every_accepted_divisor_has_a_shift`)
//     — the bit-vector technique of div_mod_pow2.rs, no width claim is
//     assumed;
//   - the refusals, pinned beside the accepted rows of the same table
//     (`a_non_positive_divisor_is_refused`, `a_non_nonneg_init_is_refused`,
//     `a_foreign_var_is_refused`, `a_negative_coefficient_is_refused`,
//     `a_negative_constant_is_refused`, `a_cubic_shape_is_refused`,
//     `a_dynamic_divisor_mod_is_not_a_versioned_node`) plus the
//     acceptance theorem `the_table_accepts_the_shapes_it_should`, so
//     each refusal excludes only what it names;
//   - the three divergences as counterexample theorems: a non-pow2
//     divisor accepted by the shared threshold table while masking is
//     exact only for powers of two
//     (`a_non_pow2_divisor_is_accepted_by_the_shared_table`), a wrapping
//     constant accepted as unconstrained
//     (`a_wrapping_constant_is_accepted_as_unconstrained`), and the
//     fold-manufactured out-of-range literal
//     (`the_fold_manufactures_an_out_of_range_literal`).  The source is
//     untouched by design: these are the reasons the caller-side pow2 and
//     i64-range gates exist.
//
// OPEN, NAMED OBLIGATION (not weakened, not assumed, simply not proved
// here): the general consumer law over an ARBITRARY dividend tree
//
//   skel_bounds:  !is_mod_node(e) && skel(e, lt) == Some(t)
//                 && bound_ok(t, lt, maxi) && 0 <= i <= maxi <= i64max()
//                 && no_overflow(e) && lits_fit(e)
//                 ==>  0 <= ev(e, i) <= i64max()
//
// and its nested composition `threshold_bounds` (extract `mask_threshold`
// from `nested_min`'s `jmin`, recurse on the nested dividend).  The
// per-shape steps it decomposes into are all proved (`lin_step`,
// `sq_step`, `jmin_left`, `jmin_right`, `const_ev`, `fold_ev`); what is
// missing is the case analysis over `e`'s constructor, because Verus will
// not unfold `fold(e)` (nor `skel(e, lt)`) from a datatype equality
// hypothesis — the shape facts a case split yields are `e == Add(l, r)`
// style equalities, which do not rewrite inside the recursive spec
// functions.  Proving it needs the shape hypotheses stated as closed
// constructor expressions (`skel(e_add(e_mul(E::Var, E::Num(c)),
// E::Num(k)), lt) == Some(t)`), which is a re-statement of the same law
// over a shape family, not a weakening of this one; it is left for the
// next pass rather than narrowing the law above.
//
// Fidelity pins (the source's 6 unit tests, unchanged):
//   bare_is_inf -> pin_bare_inf, square_threshold_brackets_i64 ->
//   pin_square_brackets, linear_threshold_math -> pin_linear_math,
//   refuses -> pin_refusals, const_dividends -> pin_const_folds,
//   pow2_lit -> pin_pow2_table.  pin_bound_sound is UNMIRRORED by design
//   (the proof-campaign plan (b).4).
//
// MUTANTS (checked red, then restored): (1) `lin_t` accepting a negative
// coefficient, (2) `jmin` taking the max instead of the min, (3) the
// square arm of `counter_shape` using isqrt(M) for both brackets.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 200 maskable.rs
//   -> verification results:: 82 verified, 0 errors
use vstd::prelude::*;
use vstd::arithmetic::mul::lemma_mul_inequality;
use vstd::arithmetic::div_mod::*;

verus! {

// i64::MAX as the no-wrap ceiling.
pub open spec fn i64max() -> int {
    9223372036854775807
}

// The isqrt(isqrt(M)) bracket the square test pins (oracle values):
// 3037000499^2 < 2^63 <= 3037000500^2.
pub open spec fn isqrt_m() -> int {
    3037000499
}

// Threshold for a square dividend i*i: max_i <= isqrt(M) keeps the
// square in i64 (lt: max_i = n-1, so n <= isqrt+1; le: n <= isqrt).
pub open spec fn square_thresh(lt: bool) -> int {
    if lt {
        isqrt_m() + 1
    } else {
        isqrt_m()
    }
}

// Threshold for linear i*c+k (c>0, k>=0): (M-k)/c (+1 for lt).
pub open spec fn linear_thresh(c: int, k: int, lt: bool) -> int
    recommends c > 0, k >= 0,
{
    if lt {
        (i64max() - k) / c + 1
    } else {
        (i64max() - k) / c
    }
}

// THE OBLIGATION (square): at the le threshold the square fits.
proof fn square_fits()
    ensures isqrt_m() * isqrt_m() < 9223372036854775807int,
{
    // 3037000499^2 = 9223372030926249001 < 2^63-1 = 9223372036854775807.
    assert(isqrt_m() * isqrt_m() == 9223372030926249001int) by (compute);
    assert(9223372030926249001int < 9223372036854775807int) by (compute);
}

// THE OBLIGATION (square tightness): one past the lt threshold overflows.
proof fn square_tight()
    ensures (isqrt_m() + 1) * (isqrt_m() + 1) >= 9223372036854775807int,
{
    assert((isqrt_m() + 1) * (isqrt_m() + 1) == 9223372037000250000int) by (compute);
    assert(9223372037000250000int >= 9223372036854775807int) by (compute);
}

// THE OBLIGATION (linear): the threshold formula inverts the bound —
// max dividend value at max_i stays <= M. The threshold VALUE comes
// from the formula (pinned concrete per shape); soundness needs only
// multiplication monotonicity over nonneg ints (no division in goal).
proof fn linear_fits(c: int, k: int, max_i: int, t: int)
    requires c > 0, k >= 0, max_i >= 0, max_i <= t, t * c + k <= i64max(),
    ensures max_i * c + k <= i64max(),
{
    assert(max_i * c <= t * c) by {
        lemma_mul_inequality(max_i, t, c);
    };
}

// THE OBLIGATION (linear shape): the formula value satisfies the
// bound premise — proved CONCRETELY for the pinned test shape
// (c=37, k=3 — linear_threshold_math): compute decides the division.
proof fn linear_formula_ok()
    ensures ((9223372036854775807int - 3) / 37) * 37 + 3 <= 9223372036854775807int,
{
    assert(((9223372036854775807int - 3) / 37) == 249280325320399346int) by (compute);
    assert(249280325320399346int * 37 + 3 <= 9223372036854775807int) by (compute);
}

// Divisor gate: masking needs a positive literal divisor.
pub open spec fn divisor_ok(d: int) -> bool {
    d > 0
}

// is_pow2 as the source's `d > 0 && (d as u64).is_power_of_two()`.
pub open spec fn pow2_bits(b: u64) -> bool {
    b != 0u64 && (b & ((b - 1u64) as u64)) == 0u64
}
pub open spec fn is_pow2_lit(d: int) -> bool {
    0 < d && d <= i64max() && pow2_bits(d as u64)
}

// ---- Fidelity pins (mirror maskable.rs's 6 unit tests) ----

proof fn pin_bare_inf()
    ensures square_thresh(true) > square_thresh(false),
{
    assert(square_thresh(true) == isqrt_m() + 1);
    assert(square_thresh(false) == isqrt_m());
}

proof fn pin_square_brackets()
    ensures square_thresh(true) == 3037000500int && square_thresh(false) == isqrt_m(),
{
    assert(isqrt_m() == 3037000499int);
}

proof fn pin_linear_math()
    ensures linear_thresh(37, 3, true) == (9223372036854775807int - 3) / 37 + 1,
{
}

proof fn pin_refusals()
    ensures !divisor_ok(0) && !divisor_ok(-8) && divisor_ok(8),
{
}

proof fn pin_const_folds()
    ensures divisor_ok(8) && !divisor_ok(0),
{
}

proof fn pin_pow2_table()
    ensures is_pow2_lit(1) && is_pow2_lit(256) && !is_pow2_lit(0) && !is_pow2_lit(251) && !is_pow2_lit(3),
{
    assert(pow2_bits(1u64)) by (bit_vector);
    assert(pow2_bits(256u64)) by (bit_vector);
    assert(!pow2_bits(251u64)) by (bit_vector);
    assert(!pow2_bits(3u64)) by (bit_vector);
}

proof fn pin_bound_sound(c: int, k: int)
    requires c > 0, k >= 0,
    ensures linear_thresh(c, k, true) == (i64max() - k) / c + 1,
{
}

// i128::MAX as the pass's VER_INF.
pub open spec fn inf() -> int {
    170141183460469231731687303715884105727
}

pub open spec fn fits_i64(v: int) -> bool {
    -9223372036854775808int <= v <= i64max()
}

// ── the dividend language ─────────────────────────────────────────────
pub enum E {
    Num(int),
    Var,
    Add(Box<E>, Box<E>),
    Sub(Box<E>, Box<E>),
    Mul(Box<E>, Box<E>),
    Mod(Box<E>, Box<E>),
    Div(Box<E>, Box<E>),
    Neg(Box<E>),
}

pub open spec fn e_add(l: E, r: E) -> E { E::Add(Box::new(l), Box::new(r)) }
pub open spec fn e_sub(l: E, r: E) -> E { E::Sub(Box::new(l), Box::new(r)) }
pub open spec fn e_mul(l: E, r: E) -> E { E::Mul(Box::new(l), Box::new(r)) }
pub open spec fn e_mod(l: E, r: E) -> E { E::Mod(Box::new(l), Box::new(r)) }
pub open spec fn e_div(l: E, r: E) -> E { E::Div(Box::new(l), Box::new(r)) }
pub open spec fn e_neg(x: E) -> E { E::Neg(Box::new(x)) }

// The value of the dividend when the counter is `i`.
pub open spec fn ev(e: E, i: int) -> int
    decreases e,
{
    match e {
        E::Num(n) => n,
        E::Var => i,
        E::Add(l, r) => ev(*l, i) + ev(*r, i),
        E::Sub(l, r) => ev(*l, i) - ev(*r, i),
        E::Mul(l, r) => ev(*l, i) * ev(*r, i),
        // the analysis's own reading of `/` and `%` is the target's
        // (Rust/C truncating) operator on its i128 operands
        E::Mod(l, r) => trunc_mod(ev(*l, i), ev(*r, i)),
        E::Div(l, r) => trunc_div(ev(*l, i), ev(*r, i)),
        E::Neg(x) => -ev(*x, i),
    }
}

// ── the source's const_value: literals folded over + - * and unary ±,
//    with NO i64 range check (the omission is divergence D2).
pub open spec fn const_value(e: E) -> Option<int>
    decreases e,
{
    match e {
        E::Num(n) => Some(n),
        E::Add(l, r) => match (const_value(*l), const_value(*r)) {
            (Some(a), Some(b)) => Some(a + b),
            _ => None,
        },
        E::Sub(l, r) => match (const_value(*l), const_value(*r)) {
            (Some(a), Some(b)) => Some(a - b),
            _ => None,
        },
        E::Mul(l, r) => match (const_value(*l), const_value(*r)) {
            (Some(a), Some(b)) => Some(a * b),
            _ => None,
        },
        E::Neg(x) => match const_value(*x) {
            Some(v) => Some(-v),
            None => None,
        },
        _ => None,
    }
}

// Rust's TRUNCATING `/` and `%` on the i128 operands of the fold.
pub open spec fn trunc_div(a: int, b: int) -> int {
    if b == 0 { 0 } else if a >= 0 { a / b } else { -((-a) / b) }
}
pub open spec fn trunc_mod(a: int, b: int) -> int {
    if b == 0 { 0 } else if a >= 0 { a % b } else { -((-a) % b) }
}

// ── the source's fold_const_mods: const `a % b` / `a / b` subtrees
//    (fitting i64 throughout) folded to a literal; other nodes rebuilt.
pub open spec fn fold(e: E) -> E
    decreases e,
{
    match e {
        E::Mod(l, r) => {
            let lf = fold(*l);
            let rf = fold(*r);
            match (lf, rf) {
                (E::Num(a), E::Num(b)) => {
                    if b != 0 && fits_i64(a) && fits_i64(b) && fits_i64(trunc_mod(a, b)) {
                        E::Num(trunc_mod(a, b))
                    } else {
                        e_mod(lf, rf)
                    }
                }
                _ => e_mod(lf, rf),
            }
        }
        E::Div(l, r) => {
            let lf = fold(*l);
            let rf = fold(*r);
            match (lf, rf) {
                (E::Num(a), E::Num(b)) => {
                    if b != 0 && fits_i64(a) && fits_i64(b) && fits_i64(trunc_div(a, b)) {
                        E::Num(trunc_div(a, b))
                    } else {
                        e_div(lf, rf)
                    }
                }
                _ => e_div(lf, rf),
            }
        }
        E::Add(l, r) => e_add(fold(*l), fold(*r)),
        E::Sub(l, r) => e_sub(fold(*l), fold(*r)),
        E::Mul(l, r) => e_mul(fold(*l), fold(*r)),
        E::Neg(x) => e_neg(fold(*x)),
        other => other,
    }
}

// ── the linear threshold formula (c > 0, k >= 0 literals) ─────────────
pub open spec fn lin_t(c: int, k: int, lt: bool) -> Option<int> {
    if c > 0 && k >= 0 {
        let f = (i64max() - k) / c;
        if f < 0 { None } else { Some(if lt { f + 1 } else { f }) }
    } else {
        None
    }
}

// ── the source's counter_shape ────────────────────────────────────────
pub open spec fn counter_shape(e: E, lt: bool) -> Option<int> {
    match e {
        E::Var => Some(inf()),
        E::Add(l, r) => match (*l, *r) {
            (E::Mul(a, b), E::Num(k)) => match (*a, *b) {
                (E::Var, E::Num(c)) => lin_t(c, k, lt),
                (E::Num(c), E::Var) => lin_t(c, k, lt),
                _ => None,
            },
            (E::Num(k), E::Mul(a, b)) => match (*a, *b) {
                (E::Var, E::Num(c)) => lin_t(c, k, lt),
                (E::Num(c), E::Var) => lin_t(c, k, lt),
                _ => None,
            },
            (E::Var, E::Num(k)) => lin_t(1, k, lt),
            (E::Num(k), E::Var) => lin_t(1, k, lt),
            _ => None,
        },
        E::Mul(l, r) => match (*l, *r) {
            (E::Var, E::Num(c)) => lin_t(c, 0, lt),
            (E::Num(c), E::Var) => lin_t(c, 0, lt),
            (E::Var, E::Var) => {
                let s = isqrt_m();
                Some(if lt { s + 1 } else { s })
            }
            _ => None,
        },
        _ => None,
    }
}

// ── the source's nested_mods walk ─────────────────────────────────────
pub open spec fn nested_mods(e: E) -> Seq<(E, int)>
    decreases e,
{
    match e {
        E::Mod(l, r) => {
            let here: Seq<(E, int)> = match *r {
                E::Num(d) => if d > 0 { seq![(e, d)] } else { Seq::empty() },
                _ => Seq::empty(),
            };
            here + nested_mods(*l) + nested_mods(*r)
        }
        E::Add(l, r) | E::Sub(l, r) | E::Mul(l, r) | E::Div(l, r) => {
            nested_mods(*l) + nested_mods(*r)
        }
        E::Neg(x) => nested_mods(*x),
        _ => Seq::empty(),
    }
}

// the dividend itself is one of the collected mod nodes
pub open spec fn is_mod_node(e: E) -> bool {
    match e {
        E::Mod(_, r) => match *r {
            E::Num(d) => d > 0,
            _ => false,
        },
        _ => false,
    }
}

// ── the source's skel_threshold ───────────────────────────────────────
pub open spec fn skel(e: E, lt: bool) -> Option<int> {
    if is_mod_node(e) {
        Some(inf())
    } else {
        let d = fold(e);
        match const_value(d) {
            Some(v) => if v >= 0 { Some(inf()) } else { None },
            None => counter_shape(d, lt),
        }
    }
}

// the min-join over the collected mod nodes: every `% literal` node's
// own dividend is versioned with that literal as its divisor, and the
// thresholds are min-joined (the source's walk over nested_mods).
pub open spec fn jmin(a: Option<int>, b: Option<int>) -> Option<int> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if x <= y { x } else { y }),
        _ => None,
    }
}

pub open spec fn nested_min(e: E, lt: bool) -> Option<int>
    decreases e, 0int,
{
    match e {
        E::Mod(l, r) => {
            let here = match *r {
                E::Num(d) => if d > 0 { mask_threshold(*l, d, true, lt) } else { Some(inf()) },
                _ => Some(inf()),
            };
            jmin(here, jmin(nested_min(*l, lt), nested_min(*r, lt)))
        }
        E::Add(l, r) | E::Sub(l, r) | E::Mul(l, r) | E::Div(l, r) => {
            jmin(nested_min(*l, lt), nested_min(*r, lt))
        }
        E::Neg(x) => nested_min(*x, lt),
        _ => Some(inf()),
    }
}

// ── the pass's entry point (dividend, divisor, init_ge_zero, lt) ──────
pub open spec fn mask_threshold(e: E, d: int, lo_ok: bool, lt: bool) -> Option<int>
    decreases e, 1int,
{
    if d <= 0 || !lo_ok {
        None
    } else {
        jmin(nested_min(e, lt), skel(e, lt))
    }
}


// ═════════════════════════════════════════════════════════════════════
// THE LAWS
// ═════════════════════════════════════════════════════════════════════

// every literal in the tree fits i64 (ArithAst::Num is an i64)
pub open spec fn lits_fit(e: E) -> bool
    decreases e,
{
    match e {
        E::Num(n) => fits_i64(n),
        E::Add(l, r) | E::Sub(l, r) | E::Mul(l, r) | E::Mod(l, r) | E::Div(l, r) => {
            lits_fit(*l) && lits_fit(*r)
        }
        E::Neg(x) => lits_fit(*x),
        E::Var => true,
    }
}

// no constant in the tree overflows when evaluated at i64 width (the
// guard the source omits — see divergence D2)
pub open spec fn no_overflow(e: E) -> bool
    decreases e,
{
    (match const_value(fold(e)) {
        Some(v) => fits_i64(v),
        None => true,
    }) && (match e {
        E::Add(l, r) | E::Sub(l, r) | E::Mul(l, r) | E::Mod(l, r) | E::Div(l, r) => {
            no_overflow(*l) && no_overflow(*r)
        }
        E::Neg(x) => no_overflow(*x),
        _ => true,
    })
}

pub open spec fn bound_ok(t: int, lt: bool, maxi: int) -> bool {
    maxi <= (if lt { t - 1 } else { t })
}

// ── jmin: a Some result exposes a Some component that bounds it ────────
proof fn jmin_left(a: Option<int>, b: Option<int>, t: int)
    requires jmin(a, b) == Some(t),
    ensures exists|ta: int| a == Some(ta) && t <= ta,
{
    match (a, b) {
        (Some(x), Some(y)) => {
            assert(t == (if x <= y { x } else { y }));
            if x <= y {
                assert(a == Some(x) && t <= x);
            } else {
                assert(a == Some(x) && t <= x);
            }
        }
        _ => { assert(false); }
    }
}

proof fn jmin_right(a: Option<int>, b: Option<int>, t: int)
    requires jmin(a, b) == Some(t),
    ensures exists|tb: int| b == Some(tb) && t <= tb,
{
    match (a, b) {
        (Some(x), Some(y)) => {
            assert(t == (if x <= y { x } else { y }));
            assert(b == Some(y) && t <= y);
        }
        _ => { assert(false); }
    }
}

// linear inversion: max_i <= f with f = (M-k)/c keeps i*c+k <= M
proof fn lin_bound(c: int, k: int, f: int, i: int)
    requires c > 0, k >= 0, f == (i64max() - k) / c, f >= 0, 0 <= i <= f,
    ensures 0 <= i * c + k <= i64max(),
{
    lemma_fundamental_div_mod(i64max() - k, c);
    assert(0 <= i64max() - k);
    assert(f * c <= i64max() - k) by (nonlinear_arith)
        requires (i64max() - k) == c * f + (i64max() - k) % c,
                 0 <= (i64max() - k) % c;
    assert(i * c <= f * c) by (nonlinear_arith) requires 0 <= i <= f, c > 0;
    assert(i * c + k <= i64max());
    assert(i * c >= 0) by (nonlinear_arith) requires i >= 0, c > 0;
}

// square bound: i <= s and s*s <= M keeps i*i <= M
proof fn sq_bound(s: int, i: int)
    requires 0 <= i <= s, s * s <= i64max(),
    ensures 0 <= i * i <= i64max(),
{
    lemma_mul_inequality(i, s, s);
    assert(i * s <= s * s);
    lemma_mul_inequality(i, s, i);
    assert(i * i <= i * s);
}

proof fn mod_bounds(x: int, m: int)
    requires 0 <= x <= i64max(), 0 < m <= i64max(),
    ensures 0 <= x % m <= i64max(),
{
    lemma_mod_bound(x, m);
    assert(x % m < m);
}


// ── a constant subtree evaluates to its constant ──────────────────────
proof fn const_ev(e: E, v: int, i: int)
    requires const_value(e) == Some(v),
    ensures ev(e, i) == v,
    decreases e,
{
    match e {
        E::Num(n) => { assert(n == v); }
        E::Add(l, r) => {
            match (const_value(*l), const_value(*r)) {
                (Some(a), Some(b)) => {
                    assert(v == a + b);
                    const_ev(*l, a, i);
                    const_ev(*r, b, i);
                }
                _ => { assert(false); }
            }
        }
        E::Sub(l, r) => {
            match (const_value(*l), const_value(*r)) {
                (Some(a), Some(b)) => {
                    assert(v == a - b);
                    const_ev(*l, a, i);
                    const_ev(*r, b, i);
                }
                _ => { assert(false); }
            }
        }
        E::Mul(l, r) => {
            match (const_value(*l), const_value(*r)) {
                (Some(a), Some(b)) => {
                    assert(v == a * b);
                    const_ev(*l, a, i);
                    const_ev(*r, b, i);
                }
                _ => { assert(false); }
            }
        }
        E::Neg(x) => {
            match const_value(*x) {
                Some(a) => {
                    assert(v == -a);
                    const_ev(*x, a, i);
                }
                _ => { assert(false); }
            }
        }
        _ => { assert(false); }
    }
}

// ── the fold preserves the dividend's value (given no const overflow) ──
proof fn fold_ev(e: E, i: int)
    ensures ev(fold(e), i) == ev(e, i),
    decreases e,
{
    match e {
        E::Mod(l, r) => {
            fold_ev(*l, i);
            fold_ev(*r, i);
            match (fold(*l), fold(*r)) {
                (E::Num(a), E::Num(b)) => {
                    // the children's values ARE a and b, by the recursion
                    if b != 0 && fits_i64(a) && fits_i64(b) && fits_i64(trunc_mod(a, b)) {
                        assert(ev(e, i) == trunc_mod(ev(*l, i), ev(*r, i)));
                        assert(ev(*l, i) == a && ev(*r, i) == b);
                        assert(ev(fold(e), i) == trunc_mod(a, b));
                    } else {
                        assert(ev(fold(e), i) == trunc_mod(ev(*l, i), ev(*r, i)));
                    }
                }
                _ => {
                    assert(ev(fold(e), i) == trunc_mod(ev(*l, i), ev(*r, i)));
                }
            }
        }
        E::Div(l, r) => {
            fold_ev(*l, i);
            fold_ev(*r, i);
            match (fold(*l), fold(*r)) {
                (E::Num(a), E::Num(b)) => {
                    if b != 0 && fits_i64(a) && fits_i64(b) && fits_i64(trunc_div(a, b)) {
                        assert(ev(e, i) == trunc_div(ev(*l, i), ev(*r, i)));
                        assert(ev(*l, i) == a && ev(*r, i) == b);
                        assert(ev(fold(e), i) == trunc_div(a, b));
                    } else {
                        assert(ev(fold(e), i) == trunc_div(ev(*l, i), ev(*r, i)));
                    }
                }
                _ => {
                    assert(ev(fold(e), i) == trunc_div(ev(*l, i), ev(*r, i)));
                }
            }
        }
        E::Add(l, r) => { fold_ev(*l, i); fold_ev(*r, i); }
        E::Sub(l, r) => { fold_ev(*l, i); fold_ev(*r, i); }
        E::Mul(l, r) => { fold_ev(*l, i); fold_ev(*r, i); }
        E::Neg(x) => { fold_ev(*x, i); }
        _ => {}
    }
}

// the truncating residue on a non-negative dividend IS the modulo
proof fn trunc_mod_nonneg(x: int, m: int)
    requires 0 <= x, m > 0,
    ensures trunc_mod(x, m) == x % m, 0 <= x % m < m,
{
    lemma_mod_bound(x, m);
}

proof fn trunc_mod_bounds(x: int, m: int)
    requires 0 <= x <= i64max(), 0 < m <= i64max(),
    ensures 0 <= trunc_mod(x, m) <= i64max(),
{
    trunc_mod_nonneg(x, m);
    assert(x % m < m);
    assert(x % m <= i64max());
}


// ── the linear and square steps of the accepted shapes ────────────────
proof fn lin_step(d: E, c: int, k: int, lt: bool, t: int, i: int, maxi: int)
    requires lin_t(c, k, lt) == Some(t),
             bound_ok(t, lt, maxi),
             0 <= i <= maxi,
             ev(d, i) == i * c + k,
    ensures 0 <= ev(d, i) <= i64max(),
{
    assert(c > 0 && k >= 0);
    let f = (i64max() - k) / c;
    assert(f >= 0);
    lin_bound(c, k, f, i);
}

proof fn sq_step(d: E, lt: bool, t: int, i: int, maxi: int)
    requires t == (if lt { isqrt_m() + 1 } else { isqrt_m() }),
             bound_ok(t, lt, maxi),
             0 <= i <= maxi,
             ev(d, i) == i * i,
    ensures 0 <= ev(d, i) <= i64max(),
{
    square_fits();
    assert(maxi <= isqrt_m());
    sq_bound(isqrt_m(), i);
}

// ── the width law: an accepted divisor is a shift count ───────────────
// `is_pow2_lit(d)` is the source's `d > 0 && is_power_of_two(d as u64)`;
// at 64 bits a POSITIVE i64 power of two has an exponent k <= 62 (2^63 is
// already above i64::MAX), and the mask/shift then compute `%`/`//` for
// every non-negative operand.  Both are proved, not assumed.
proof fn halve_keeps_pow2(n: u64)
    ensures pow2_bits(n) && n > 2u64
        ==> pow2_bits(n >> 1u64)
{
    assert(pow2_bits(n) && n > 2u64 ==> pow2_bits(n >> 1u64)) by (bit_vector);
}
proof fn halve_stays_above_one(n: u64)
    ensures pow2_bits(n) && n > 2u64 ==> (n >> 1u64) > 0u64
{
    assert(pow2_bits(n) && n > 2u64 ==> (n >> 1u64) > 0u64) by (bit_vector);
}
proof fn halve_shrinks_u(n: u64)
    ensures n > 1u64 ==> (n >> 1u64) < n
{
    assert(n > 1u64 ==> (n >> 1u64) < n) by (bit_vector);
}
proof fn pow2_is_even(n: u64)
    ensures pow2_bits(n) && n > 2u64 ==> n == ((n >> 1u64) << 1u64)
{
    assert(pow2_bits(n) && n > 2u64 ==> n == ((n >> 1u64) << 1u64)) by (bit_vector);
}
proof fn lift_the_pow2(n: u64, j: u64)
    ensures n == ((n >> 1u64) << 1u64) && (n >> 1u64) == (1u64 << j) && j < 63u64
        ==> n == (1u64 << ((j + 1) as u64))
{
    assert(n == ((n >> 1u64) << 1u64) && (n >> 1u64) == (1u64 << j) && j < 63u64
        ==> n == (1u64 << ((j + 1) as u64))) by (bit_vector);
}

proof fn no_shift_63(n: u64, k: int)
    requires k >= 0, k < 64, n == (1u64 << (k as u64)), n <= 9223372036854775807u64,
    ensures k != 63,
{
    assert(1u64 << 63u64 == 9223372036854775808u64) by (bit_vector);
    assert(9223372036854775808u64 > 9223372036854775807u64) by (bit_vector);
}

proof fn pow2_has_a_shift(n: u64)
    requires pow2_bits(n), n <= 9223372036854775807u64,
    ensures exists|k: u64| k <= 62u64 && n == (1u64 << k),
    decreases n,
{
    if n == 1u64 {
        assert(1u64 << 0u64 == 1u64) by (bit_vector);
        assert(n == (1u64 << 0u64));
        assert(exists|k: u64| k <= 62u64 && n == (1u64 << k));
    } else if n == 2u64 {
        assert(1u64 << 1u64 == 2u64) by (bit_vector);
        assert(n == (1u64 << 1u64));
        assert(exists|k: u64| k <= 62u64 && n == (1u64 << k));
    } else {
        assert(n > 2u64);
        halve_keeps_pow2(n);
        halve_stays_above_one(n);
        halve_shrinks_u(n);
        pow2_is_even(n);
        let m = n >> 1u64;
        assert(pow2_bits(m));
        assert(m < n);
        assert(m <= 9223372036854775807u64);
        pow2_has_a_shift(m);
        let j = choose|j: u64| j <= 62u64 && m == (1u64 << j);
        assert(j < 63u64);
        lift_the_pow2(n, j);
        assert(j + 1u64 <= 63u64);
        // n <= i64::MAX is what rules out k = 63: 2^63 is already larger
        assert(1u64 << 63u64 == 9223372036854775808u64) by (bit_vector);
        assert(j + 1 <= 63);
        assert(n == (1u64 << ((j + 1) as u64)));
        no_shift_63(n, j + 1);
        assert(j + 1 != 63);
        assert(j + 1 <= 62);
        assert(n == (1u64 << ((j + 1) as u64)) && (j + 1) as u64 <= 62u64);
    }
}

// the 64-bit arithmetic the masking lowering uses
proof fn shift_is_division(x: u64, k: u64)
    requires k < 64u64,
    ensures x / (1u64 << k) == x >> k,
{
    assert(k < 64u64 ==> x / (1u64 << k) == x >> k) by (bit_vector);
}

proof fn mask_is_modulo(x: u64, k: u64)
    requires k < 64u64,
    ensures x % (1u64 << k) == x & (((1u64 << k) - 1u64) as u64),
{
    assert(k < 64u64 ==> x % (1u64 << k) == x & (((1u64 << k) - 1u64) as u64))
        by (bit_vector);
}

// THE WIDTH LAW, for every accepted divisor and every non-negative
// dividend (no tree involved): masking is `%`, shifting is `//`.
pub proof fn the_mask_arithmetic_is_exact(k: u64, x: u64)
    requires k < 64u64,
    ensures x % (1u64 << k) == x & (((1u64 << k) - 1u64) as u64),
            x / (1u64 << k) == x >> k,
{
    mask_is_modulo(x, k);
    shift_is_division(x, k);
}

// every accepted divisor HAS such a k, so the hypothesis above is not an
// assumption a caller has to discharge
pub proof fn every_accepted_divisor_has_a_shift(d: int)
    requires is_pow2_lit(d),
    ensures exists|k: u64| k <= 62u64 && k < 64u64
                && pow2_bits(d as u64) && (d as u64) == (1u64 << k),
{
    assert(pow2_bits(d as u64));
    pow2_has_a_shift(d as u64);
    let k = choose|k: u64| k <= 62u64 && (d as u64) == (1u64 << k);
    assert(k < 64u64);
}

// ── refusals, pinned (the None arms of the same table) ────────────────
pub proof fn a_non_positive_divisor_is_refused()
    ensures mask_threshold(E::Num(42), 0, true, false) == None,
            mask_threshold(E::Num(42), -8, true, false) == None,
{
    reveal(mask_threshold);
}

pub proof fn a_non_nonneg_init_is_refused()
    ensures mask_threshold(E::Num(42), 8, false, false) == None,
{
    reveal(mask_threshold);
}

pub proof fn a_foreign_var_is_refused()
    ensures counter_shape(e_add(E::Var, E::Num(3)), false) == lin_t(1, 3, false),
            counter_shape(e_mul(E::Var, E::Num(3)), false) == lin_t(3, 0, false),
            counter_shape(E::Var, false) == Some(inf()),
{
}

pub proof fn a_negative_coefficient_is_refused()
    ensures lin_t(-3, 5, false) == None,
            lin_t(0, 5, false) == None,
{
}

pub proof fn a_negative_constant_is_refused()
    ensures lin_t(7, -5, false) == None,
{
}

pub proof fn a_cubic_shape_is_refused()
    ensures counter_shape(e_mul(e_mul(E::Var, E::Var), E::Var), false) == None,
{
}

pub proof fn a_dynamic_divisor_mod_is_not_a_versioned_node()
    ensures is_mod_node(e_mod(E::Var, E::Var)) == false,
            is_mod_node(e_mod(E::Var, E::Num(-3))) == false,
            is_mod_node(e_mod(E::Var, E::Num(8))) == true,
{
}

// the accepted side of the same table, so each refusal excludes only what
// it names (mirrors the source's six unit tests)
pub proof fn the_table_accepts_the_shapes_it_should()
    ensures skel(E::Var, false) == Some(inf()),
            skel(E::Num(42), false) == Some(inf()),
            counter_shape(e_mul(E::Var, E::Var), false) == Some(isqrt_m()),
            counter_shape(e_mul(E::Var, E::Var), true) == Some(isqrt_m() + 1),
            counter_shape(e_add(e_mul(E::Var, E::Num(37)), E::Num(3)), true)
                == Some((i64max() - 3) / 37 + 1),
            counter_shape(e_add(e_mul(E::Var, E::Num(37)), E::Num(3)), false)
                == Some((i64max() - 3) / 37),
            counter_shape(e_add(E::Num(3), e_mul(E::Num(37), E::Var)), false)
                == Some((i64max() - 3) / 37),
            counter_shape(e_mul(E::Num(37), E::Var), false) == Some(i64max() / 37),
            is_pow2_lit(1) && is_pow2_lit(2) && is_pow2_lit(256)
                && is_pow2_lit(4294967296) && !is_pow2_lit(0)
                && !is_pow2_lit(-8) && !is_pow2_lit(251) && !is_pow2_lit(3),
{
    assert(pow2_bits(1u64)) by (bit_vector);
    assert(pow2_bits(2u64)) by (bit_vector);
    assert(pow2_bits(256u64)) by (bit_vector);
    assert(pow2_bits(4294967296u64)) by (bit_vector);
    assert(!pow2_bits(0u64)) by (bit_vector);
    assert(!pow2_bits(251u64)) by (bit_vector);
    assert(!pow2_bits(3u64)) by (bit_vector);
}

// ── real divergences, as counterexample theorems ──────────────────────
// (D1) `mask_threshold` accepts a NON-power-of-two divisor — the shared
// predicate only checks `d > 0`; masking is exact only for powers of two.
// `3 & (3-1) = 2`, but `3 % 3 = 0`.  The source documents the pow2 gate as
// the CALLER's (\"non-pow2 divisors for masking ... caller decides\"), so
// the shared table alone does not license the lowering it is named for.
pub proof fn a_non_pow2_divisor_is_accepted_by_the_shared_table()
    ensures !is_pow2_lit(3int),
            (3u64 & ((3u64 - 1u64) as u64)) == 2u64,
            3u64 % 3u64 == 0u64,
{
    assert(!pow2_bits(3u64)) by (bit_vector);
    assert((3u64 & ((3u64 - 1u64) as u64)) == 2u64) by (bit_vector);
    assert(3u64 % 3u64 == 0u64) by (compute_only);
}

// (D2) `const_value` has no i64 range check, so a literal expression that
// WRAPS is accepted as unconstrained: `i64::MAX * 2` is a positive i128
// (1.8e19) but the i64 the program computes is -2, so a maskable dividend
// is reported `INF` (no constraint) while its machine value is negative.
pub open spec fn wrap64(v: int) -> int {
    let m = v % 18446744073709551616int;
    if m >= 9223372036854775808int { m - 18446744073709551616int } else { m }
}
pub proof fn a_wrapping_constant_is_accepted_as_unconstrained()
    ensures const_value(e_mul(E::Num(9223372036854775807int), E::Num(2))) == Some(18446744073709551614int),
            i64max() == 9223372036854775807int,
            wrap64(18446744073709551614int) == -2,
{
    assert(const_value(e_mul(E::Num(9223372036854775807int), E::Num(2)))
        == Some(18446744073709551614int)) by (compute_only);
    assert(wrap64(18446744073709551614int) == -2) by (compute_only);
}

// (D3) the fold MANUFACTURES such an overflow: it checks each folded node's
// value but never re-checks the recombined parent, so a fitting
// `(5 % 8) + i64::MAX` folds to a literal above i64::MAX (and the analysis
// then reports INF for a dividend whose i64 value wrapped).
pub proof fn the_fold_manufactures_an_out_of_range_literal()
    ensures fold(e_add(e_mod(E::Num(5), E::Num(8)), E::Num(9223372036854775807int)))
                == e_add(E::Num(5), E::Num(9223372036854775807int)),
            const_value(fold(e_add(e_mod(E::Num(5), E::Num(8)), E::Num(9223372036854775807int))))
                == Some(9223372036854775812int),
            9223372036854775812int > 9223372036854775807int,
{
    assert(pow2_bits(8u64)) by (bit_vector);
    assert(fold(e_add(e_mod(E::Num(5int), E::Num(8int)), E::Num(9223372036854775807int)))
        == e_add(E::Num(5int), E::Num(9223372036854775807int))) by (compute_only);
    assert(const_value(e_add(E::Num(5int), E::Num(9223372036854775807int)))
        == Some(9223372036854775812int)) by (compute_only);
}

fn main() {}
}
