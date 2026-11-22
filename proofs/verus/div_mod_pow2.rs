// div_mod_pow2.rs — model of `transforms/div_mod_pow2.rs`.
//
// The pass is ANALYSIS-ONLY: it rewrites nothing. It marks a `/` or `%`
// node SHIFTABLE when (a) the divisor is a numeral that is an exact power
// of two (`pow2`: n > 1, n & (n-1) == 0, n < 2^31) and (b) the dividend is
// PROVABLY NON-NEGATIVE (`prove_non_neg`, whose only witness is the
// `NonNeg` evidence type). A backend that consumes the annotation may then
// emit `x >> k` for the division and `x & (2^k - 1)` for the modulo.
//
// What is proved here, against those exact semantics:
//
//   firing_guard:       prove_non_neg(a)  =>  0 <= ev(a)
//   pow2_shape:         pow2(n)  =>  exists k. 1 <= k <= 30 /\ n == 2^k
//   shift_is_division:  k < 64  =>  x / 2^k == x >> k          (all u64 x)
//   mask_is_modulo:     k < 64  =>  x % 2^k == x & (2^k - 1)   (all u64 x)
//   mask_clears_low_bits / mask_is_a_remainder: the same mask as the
//       "clear the low k bits" pair, with the quotient restored
//   the_annotation_is_exact: prove_non_neg(a) && pow2(n) =>
//       the `/` rewrite and the `%` rewrite both compute the source value
//
// The bit-level facts are stated as CLOSED implications, so `assert(..) by
// (bit_vector)` sees the assumption inside the goal instead of relying on
// context (this Verus build bit-blasts a closed goal and treats hypotheses
// as opaque). `pow2_shape` is a halving induction whose step is one such
// closed implication; the induction itself is ordinary Verus.
//
// Out of model, named (not proved, not faked, not weakened):
// - the CONSUMERS. The module records that the verdict has NO consumer in
//   this tree (`div_mod_pow2::shiftable` is read only by its own tests). A
//   law about what some other backend emits is a property of code that is
//   not here, so it is out of scope: the exactness theorem above is the
//   contract such a consumer must meet, and the verdict carries it.
// - i64 overflow: the guard only accepts literal-known non-negative
//   dividends, whose compile-time value is the machine value; a dividend
//   that wraps negative is never marked.
// - the verdict cache (per-thread, keyed by (fn, stmt index)): the model
//   states the property of a marked node, not the cache's bookkeeping.
//
// Verus 0.2026.10.18.7d82994:
//   verus --triggers-mode silent --rlimit 60 div_mod_pow2.rs
use vstd::prelude::*;

verus! {

// ── the dividend: only `Num >= 0` and `+`/`*` of proven operands ───────

pub enum A {
    Num(int),
    Add(Box<A>, Box<A>),
    Mul(Box<A>, Box<A>),
    Sub(Box<A>, Box<A>),
    Div(Box<A>, Box<A>),
    Mod(Box<A>, Box<A>),
    Var,
}

pub open spec fn a_num(n: int) -> A { A::Num(n) }
pub open spec fn add(l: A, r: A) -> A { A::Add(Box::new(l), Box::new(r)) }
pub open spec fn mul(l: A, r: A) -> A { A::Mul(Box::new(l), Box::new(r)) }
pub open spec fn a_sub(l: A, r: A) -> A { A::Sub(Box::new(l), Box::new(r)) }
pub open spec fn a_div(l: A, r: A) -> A { A::Div(Box::new(l), Box::new(r)) }
pub open spec fn a_mod(l: A, r: A) -> A { A::Mod(Box::new(l), Box::new(r)) }

pub open spec fn prove_non_neg(a: A) -> bool
    decreases a,
{
    match a {
        A::Num(n) => n >= 0,
        A::Add(l, r) => prove_non_neg(*l) && prove_non_neg(*r),
        A::Mul(l, r) => prove_non_neg(*l) && prove_non_neg(*r),
        _ => false,
    }
}

pub open spec fn ev(a: A) -> int
    decreases a,
{
    match a {
        A::Num(n) => n,
        A::Add(l, r) => ev(*l) + ev(*r),
        A::Mul(l, r) => ev(*l) * ev(*r),
        A::Sub(l, r) => ev(*l) - ev(*r),
        A::Div(l, r) => if ev(*r) == 0 { 0 } else { ev(*l) / ev(*r) },
        A::Mod(l, r) => if ev(*r) == 0 { 0 } else { ev(*l) % ev(*r) },
        A::Var => 0,
    }
}

// ── the pass's first obligation: fire => non-negative dividend ─────────

proof fn firing_guard(a: A)
    ensures prove_non_neg(a) ==> 0 <= ev(a),
    decreases a,
{
    match a {
        A::Add(l, r) => {
            firing_guard(*l);
            firing_guard(*r);
        }
        A::Mul(l, r) => {
            firing_guard(*l);
            firing_guard(*r);
        }
        _ => {}
    }
}

// ── the pass's second obligation: the divisor's shape ──────────────────

pub open spec fn pow2(n: u64) -> bool {
    n > 1 && (n & ((n - 1u64) as u64)) == 0u64 && n < (1u64 << 31u64)
}

// Each premise of the halving step, stated as a CLOSED implication so
// bit-vector reasoning can use it.
proof fn halve_keeps_the_mask(n: u64)
    ensures (n & ((n - 1u64) as u64)) == 0u64 && n > 2u64
        ==> (((n >> 1u64) & (((n >> 1u64) - 1u64) as u64)) == 0u64)
{
    assert((n & ((n - 1u64) as u64)) == 0u64 && n > 2u64
        ==> (((n >> 1u64) & (((n >> 1u64) - 1u64) as u64)) == 0u64)) by (bit_vector);
}

proof fn halve_stays_above_one(n: u64)
    ensures (n & ((n - 1u64) as u64)) == 0u64 && n > 2u64 ==> (n >> 1u64) > 1u64
{
    assert((n & ((n - 1u64) as u64)) == 0u64 && n > 2u64
        ==> (n >> 1u64) > 1u64) by (bit_vector);
}

proof fn halve_stays_in_range(n: u64)
    ensures n < (1u64 << 31u64) && n > 2u64 ==> (n >> 1u64) < (1u64 << 31u64)
{
    assert(n < (1u64 << 31u64) && n > 2u64
        ==> (n >> 1u64) < (1u64 << 31u64)) by (bit_vector);
}

proof fn halve_shrinks(n: u64)
    ensures n > 1u64 ==> (n >> 1u64) < n
{
    assert(n > 1u64 ==> (n >> 1u64) < n) by (bit_vector);
}

proof fn mask_is_even(n: u64)
    ensures (n & ((n - 1u64) as u64)) == 0u64 && n > 2u64
        ==> n == ((n >> 1u64) << 1u64)
{
    assert((n & ((n - 1u64) as u64)) == 0u64 && n > 2u64
        ==> n == ((n >> 1u64) << 1u64)) by (bit_vector);
}

proof fn lift_the_shift(n: u64, j: u64)
    ensures n == ((n >> 1u64) << 1u64) && (n >> 1u64) == (1u64 << j) && j < 63u64
        ==> n == (1u64 << (j + 1u64))
{
    assert(n == ((n >> 1u64) << 1u64) && (n >> 1u64) == (1u64 << j) && j < 63u64
        ==> n == (1u64 << (j + 1u64))) by (bit_vector);
}

proof fn shift_count_in_range(n: u64, k: u64)
    ensures n == (1u64 << k) && n > 1u64 && n < (1u64 << 31u64) && k < 64u64
        ==> 1u64 <= k && k <= 30u64
{
    assert(n == (1u64 << k) && n > 1u64 && n < (1u64 << 31u64) && k < 64u64
        ==> 1u64 <= k && k <= 30u64) by (bit_vector);
}

// pow2 => n is 2^k, by halving induction (the step uses the closed
// implications above).
proof fn pow2_is_a_shift(n: u64)
    requires pow2(n),
    ensures exists|k: u64| 1u64 <= k && k <= 30u64 && n == (1u64 << k),
    decreases n,
{
    if n == 2u64 {
        assert((1u64 << 1u64) == 2u64) by (compute_only);
        assert(n == (1u64 << 1u64));
    } else {
        assert(n > 2u64);
        halve_keeps_the_mask(n);
        halve_stays_above_one(n);
        halve_stays_in_range(n);
        halve_shrinks(n);
        mask_is_even(n);
        let m = n >> 1u64;
        assert(pow2(m));
        pow2_is_a_shift(m);
        let j = choose|j: u64| 1u64 <= j && j <= 30u64 && m == (1u64 << j);
        assert(j < 63u64);
        lift_the_shift(n, j);
        assert(n == (1u64 << (j + 1u64)));
        assert(1u64 <= j + 1u64 && j + 1u64 <= 30u64);
    }
}

// ── the arithmetic the annotation licenses, on the target's word ───────

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

proof fn mask_clears_low_bits(x: u64, k: u64)
    requires k < 64u64,
    ensures x & (((1u64 << k) - 1u64) as u64) == x - ((x >> k) << k),
{
    assert(k < 64u64 ==> x & (((1u64 << k) - 1u64) as u64) == x - ((x >> k) << k))
        by (bit_vector);
}

proof fn mask_is_a_remainder(x: u64, k: u64)
    requires k < 64u64,
    ensures x == (x & (((1u64 << k) - 1u64) as u64)) + ((x >> k) << k),
            (x & (((1u64 << k) - 1u64) as u64)) < (1u64 << k),
{
    assert(k < 64u64
        ==> x == (x & (((1u64 << k) - 1u64) as u64)) + ((x >> k) << k)) by (bit_vector);
    assert(k < 64u64 ==> (x & (((1u64 << k) - 1u64) as u64)) < (1u64 << k))
        by (bit_vector);
}

// ── the verdict's contract, end to end ─────────────────────────────────

proof fn the_annotation_is_exact(a: A, n: u64)
    requires prove_non_neg(a), pow2(n),
    ensures 0 <= ev(a),
            exists|k: u64| 1u64 <= k && k <= 30u64 && n == (1u64 << k)
                && (forall|x: u64| x / n == x >> k
                    && x % n == x & ((n - 1u64) as u64)),
{
    firing_guard(a);
    pow2_is_a_shift(n);
    let k = choose|k: u64| 1u64 <= k && k <= 30u64 && n == (1u64 << k);
    assert(k < 64u64);
    assert forall|x: u64| x / n == x >> k && x % n == x & ((n - 1u64) as u64) by {
        shift_is_division(x, k);
        mask_is_modulo(x, k);
    }
}

// ── the guard's refusals, and why it exists ────────────────────────────

proof fn the_pow2_test_refuses_non_powers()
{
    assert(!pow2(3u64)) by (compute_only);
    assert(!pow2(0u64)) by (compute_only);
    assert(!pow2(1u64)) by (compute_only);
    assert(!pow2(1u64 << 31u64)) by (compute_only);
    assert(pow2(4u64)) by (compute_only);
    assert(pow2(1u64 << 30u64)) by (compute_only);
}

proof fn the_guard_refuses_negative_shapes()
{
    assert(!prove_non_neg(a_sub(a_num(0), a_num(12)))) by (compute_only);
    assert(!prove_non_neg(a_div(a_num(12), a_num(4)))) by (compute_only);
    assert(!prove_non_neg(a_mod(a_num(12), a_num(4)))) by (compute_only);
    assert(!prove_non_neg(A::Var)) by (compute_only);
    assert(!prove_non_neg(a_num(-1))) by (compute_only);
    // a product of two negatives is positive but refused (incomplete in
    // the safe direction, as documented)
    assert(!prove_non_neg(mul(a_num(-12), a_num(-1)))) by (compute_only);
    // the positive control
    assert(prove_non_neg(add(a_num(2), mul(a_num(3), a_num(4))))) by (compute_only);
}

// truncation toward zero (the shell/C convention); Verus's int `/` floors
pub open spec fn trunc_div(x: int, d: int) -> int
{ if x >= 0 { x / d } else { -((-x) / d) } }
// floor (what an arithmetic shift does on a signed operand)
pub open spec fn floor_div(x: int, d: int) -> int { x / d }

// On a SIGNED value the rewrite is wrong: -3/2 = -1 but -3 >> 1 = -2.
proof fn negative_dividend_breaks_the_rewrite()
{
    assert(trunc_div(-3, 2) == -1) by (compute_only);
    assert(floor_div(-3, 2) == -2) by (compute_only);
    assert(trunc_div(-3, 2) != floor_div(-3, 2));
}

fn main() {}
}
