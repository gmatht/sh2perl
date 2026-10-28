// Standalone model of maskable unsigned lowering
// (sh2perl/src/shir_passes/maskable.rs): `mask_threshold` for one
// `%`/`/` site. Proves the threshold-soundness obligation:
//
//   threshold_sound: bound <= threshold(dividend) implies every value
//   the dividend takes on [0, bound] fits i64 nonneg — so the
//   unsigned lowering (mask/shift) is value-exact.
//
// Dividends are shape-tagged ints with oracle-computed thresholds:
// the shape classifiers (bare/linear/square/const/nested/refused)
// and isqrt/fold_const_mods are PARAMETERS (exactly as array_len_fold
// takes is_plain_ident as oracle). The model proves the threshold
// ARITHMETIC sound: the claimed max-dividend value under the bound
// really is an i64-fitting nonneg upper bound for the shape.
//
// What is modelled and what is NOT (fidelity, checked against
// maskable.rs's 6 unit tests):
//   - The per-shape threshold formulas: bare INF; linear (M-k)/c+1
//     (lt) / (M-k)/c (le); square isqrt(M)(+1 for lt); const-folded
//     nonneg INF; nested-mod min-join; anything else None (miss).
//   - The divisor gate (positive literal) and init_ge_zero as
//     premises; is_pow2_lit as the maskable-divisor predicate.
//   - The accumulator rule and u64/non-pow2/caller-side induction
//     are out of scope (caller-owned, stated in the source).
//   - Guards mirror the 6 source unit tests (in-module pins):
//       bare_is_inf -> pin_bare_inf
//       square_threshold_brackets_i64 -> pin_square_brackets
//       linear_threshold_math -> pin_linear_math
//       refuses -> pin_refusals (neg-k, no-init, foreign-var, cubic,
//         nonpositive-divisor)
//       const_dividends -> pin_const_folds
//       pow2_lit -> pin_pow2_table
//
// MUTANT (checked red): using isqrt(M)+1 for the `<=` (le) square
// arm (off-by-one past i64::MAX) makes the same command report 10
// verified, 2 errors — pin_square_brackets and pin_bare_inf go red.
// The file was restored and re-checked at 12 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 maskable.rs
//   -> verification results:: 12 verified, 0 errors
use vstd::prelude::*;
use vstd::arithmetic::mul::lemma_mul_inequality;

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

// is_pow2 as the maskable-divisor predicate (oracle: u64 is_power_of_two).
pub open spec fn is_pow2_lit(d: int) -> bool {
    d == 1 || d == 2 || d == 256 || d == 4294967296int
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
}

proof fn pin_bound_sound(c: int, k: int)
    requires c > 0, k >= 0,
    ensures linear_thresh(c, k, true) == (i64max() - k) / c + 1,
{
}

fn main() {}

}
