// div_mod_pow2.rs — model of `transforms/div_mod_pow2.rs`.
//
// The pass is ANALYSIS-ONLY: it rewrites nothing. It marks a `/` or `%`
// node SHIFTABLE when (a) the divisor is a numeral that is an exact power
// of two and (b) the dividend is PROVABLY NON-NEGATIVE. The evidence type
// `NonNeg` has one producer, `prove_non_neg`, so the pass's own stated
// soundness obligation is exactly (b): "fire => the dividend provably
// non-negative" — a false positive would licence `-3 >> 1` where bash's
// `-3/2` truncates (the negative control below).
//
// What is proved here:
//
//   firing_guard:  prove_non_neg(a)  =>  0 <= ev(a)
//       (structural induction. The proof's only accepting arms are
//        `Num(n)` with n >= 0 and `Add`/`Mul` of two accepting operands;
//        every other shape — `-`, `/`, `%`, a variable read — is refused,
//        so the arithmetic cannot manufacture a `NonNeg` for an operand
//        whose value can be negative.)
//
//   the_guard_refuses_negative_shapes: the refusals, as inequalities
//   the_pow2_test_refuses_non_powers:  the divisor test's accept/refuse
//       surface (`pow2` = `n > 1 && n & (n-1) == 0 && n < 2^31`)
//   negative_dividend_breaks_the_rewrite: truncating `/` vs flooring shift
//       on -3, which is WHY the guard exists
//
// Out of model, named rather than assumed:
// - "the mask test implies n == 2^k for some k in 1..=30". This needs an
//   induction in which `bit_vector` uses the hypothesis `n & (n-1) == 0`;
//   in this Verus build `by (bit_vector)` bit-blasts a CLOSED goal and
//   treats hypotheses as opaque, so the step `n & (n-1) == 0 ==> (n>>1) &
//   ((n>>1)-1) == 0` is not discharged here. The law is carried by the
//   bit-level Isabelle development `proofs/DivModPow2/DivModPow2.thy`
//   (which cannot be rebuilt in this environment: no Isabelle binary and
//   the distribution mirrors are unreachable). The corresponding test
//   surface is pinned by the transform's own `pow2` cases.
// - the backends' arithmetic (`x >> k` for `x / 2^k`, `x & (2^k - 1)` for
//   `x % 2^k`) is a property of the CONSUMER, not of this pass (module doc:
//   the verdict has no consumer in this tree). It is exact for
//   non-negative words; the guard proved here is what makes that domain
//   apply.
// - the verdict cache is per thread and keyed by (fn, stmt index); the
//   model states the property of the marked node, not the cache's diffing.
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

// ── the pass's obligation: fire => non-negative dividend ───────────────

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

// ── the divisor test's surface ─────────────────────────────────────────

pub open spec fn pow2(n: u64) -> bool {
    n > 1 && (n & ((n - 1u64) as u64)) == 0u64 && n < (1u64 << 31u64)
}

proof fn the_pow2_test_refuses_non_powers()
{
    assert(!pow2(3u64)) by (compute_only);
    assert(!pow2(0u64)) by (compute_only);
    assert(!pow2(1u64)) by (compute_only);
    assert(!pow2(1u64 << 31u64)) by (compute_only);
    assert(pow2(4u64)) by (compute_only);
    assert(pow2(1u64 << 30u64)) by (compute_only);
}

// ── the guard's refusals, and why it exists ────────────────────────────

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
