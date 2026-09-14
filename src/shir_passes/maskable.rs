//! Maskable unsigned lowering (shared analysis for C/CUDA/Vulkan).
//!
//! A `% C` / `/ C` (C a positive literal) with a dividend provably
//! nonneg with no i64 wrap lowers to masking/shifting — unconditionally
//! sound for `==0`/`!=0` tests (two's complement parity is a bit
//! property) and value-preserving for nonneg values. Thresholds let a
//! RUNTIME check discharge dynamic bounds exactly (loop versioning):
//! `bound <= THRESH` implies soundness, so adversarial bounds stay
//! correct via the slow path.
//!
//! Threshold = max bound value (inclusive) with soundness, derived per
//! dividend shape over `i in [init_v, max_i]` with `init_v >= 0`
//! (caller-verified literal init) and `max_i = n-1` (`<`) or `n` (`<=`):
//! - bare counter: no constraint (an i64 always fits) → INF.
//! - linear `i*c+k`, `c>0` and `k>=0` literals: min at init (`>= k >=
//!   0`); `max_i*c+k <= M` (M = i64::MAX) gives the threshold.
//! - square `i*i`: min `init_v^2 >= 0` always; `max_i <= isqrt(M)`.
//! - const-foldable literals: INF iff nonneg.
//! Anything else (nonpositive/negative coefficients, cubic+, foreign
//! vars — including accumulators, which the CALLER splits out — dynamic
//! divisors) → None (stays signed).
//!
//! Accumulator rule lives with callers (they own init info): an
//! `(acc+X)%m` site is maskable iff X is acc-free with threshold T(X)
//! and acc entry holds `0 <= init < m` (literal). The shared predicate
//! below never sees accumulator vars by design (keeps it total and
//! obviously sound; induction machinery stays in one place per caller).
//!
//! Consumers: the C backend (range-proofed lowering), the CUDA emitter
//! (`mask_fast` flag: mask every pow2-mod site when set — sound iff
//! every such site contributed to the min-threshold, which candidacy
//! guarantees or refuses wholesale), Vulkan/GLSL where bitwise is legal
//! (ES 1.00 cannot — same annotation pattern as div_mod_pow2, but with
//! live consumers this time).
//!
//! Deliberately NOT covered (documented misses, safe direction):
//! accumulator induction in-shared (caller-side, see above), negative
//! inits (square could use `|init|` bounds — V2), u64-range operands,
//! non-pow2 divisors for masking (unsigned magic needs the same proof
//! but different codegen — caller decides), ranged vars (bash-o4
//! callers carry no ranges; c_backend keeps its own).

use crate::ir::ArithAst;

/// No n-constraint from this site (entry checks suffice).
pub const VER_INF: i128 = i128::MAX;
/// i64::MAX as i128 (no-wrap ceiling).
pub const VER_M: i128 = i64::MAX as i128;

/// Integer floor-sqrt (binary search — f64 is inexact past 2^53).
pub fn isqrt_floor(x: i128) -> i128 {
    if x <= 0 {
        return 0;
    }
    let (mut lo, mut hi) = (1i128, x.min(1i128 << 64));
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= x / mid {
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }
    hi
}

/// True for positive-literal powers of two (the only maskable divisors).
pub fn is_pow2_lit(d: i64) -> bool {
    d > 0 && (d as u64).is_power_of_two()
}

/// Threshold for sound unsigned lowering of one `dividend % divisor`
/// (equally `/`) site, or None (stays signed). `counter` is the loop
/// counter var; `lt` selects `<` (max_i = n-1) vs `<=` (max_i = n);
/// `init_ge_zero` must hold (caller-verified literal-nonneg init).
/// Dividend vars must be ⊆ {counter} ∪ {literals} — anything else
/// refuses (accumulators are split out by callers, see above).
pub fn mask_threshold(
    dividend: &ArithAst,
    divisor: i64,
    counter: &str,
    lt: bool,
    init_ge_zero: bool,
) -> Option<i128> {
    if divisor <= 0 || !init_ge_zero {
        return None;
    }
    // Nested mods/divs: nested `%`-of-literal recurses (joins min);
    // nested `/` or dynamic divisors refuse (result range unknowable).
    // NOTE: nested nodes mentioning counter are fine (their own
    // threshold covers them); the outer skeleton must still classify
    // below with nested subtrees treated opaquely (range [0, m_i) only
    // when the recursion succeeded — enforced by `?` unwrapping here,
    // so reaching skel_threshold means all nested are versionable).
    let mut t_min: i128 = VER_INF;
    for (node, m) in nested_mods(dividend) {
        // Recurse on the nested mod's own dividend (structurally
        // smaller; terminates). Its threshold joins the min.
        if let ArithAst::Bin { lhs, .. } = &node {
            let t = mask_threshold(lhs, m, counter, lt, init_ge_zero)?;
            t_min = t_min.min(t);
        }
    }
    let nested_here = nested_mods(dividend);
    let t_outer = skel_threshold(dividend, counter, lt, &nested_here)?;
    Some(t_min.min(t_outer))
}

/// Full `%`-of-literal mod nodes nested inside a dividend, INCLUDING
/// the dividend itself when it is one. Callers recurse into each node's
/// dividend; the node itself substitutes as [0, m) once versioned.
fn nested_mods(dividend: &ArithAst) -> Vec<(ArithAst, i64)> {
    let mut out = Vec::new();
    fn walk(a: &ArithAst, out: &mut Vec<(ArithAst, i64)>) {
        match a {
            ArithAst::Bin { op, lhs, rhs } if op == "%" => {
                if let ArithAst::Num(d) = rhs.as_ref() {
                    if *d > 0 {
                        out.push((a.clone(), *d));
                    }
                }
                walk(lhs, out);
                walk(rhs, out);
            }
            ArithAst::Bin { lhs, rhs, .. } => {
                walk(lhs, out);
                walk(rhs, out);
            }
            ArithAst::Un { arg, .. } => walk(arg, out),
            ArithAst::Cond { test, then, else_ } => {
                walk(test, out);
                walk(then, out);
                walk(else_, out);
            }
            ArithAst::Assign { rhs, .. } => walk(rhs, out),
            ArithAst::Cast { arg, .. } => walk(arg, out),
            ArithAst::Index { key, .. } => walk(key, out),
            _ => {}
        }
    }
    walk(dividend, &mut out);
    out
}

/// Threshold for the outer shape:
/// - the dividend is itself a collected mod node → INF (its soundness
///   is exactly the recursion's threshold, joined by the caller);
/// - const-foldable nonneg → INF;
/// - pure-counter bare/linear/square → formula threshold;
/// - anything else → None (miss, safe — mixed counter+mod shapes are V2).
fn skel_threshold(
    dividend: &ArithAst,
    counter: &str,
    lt: bool,
    nested: &[(ArithAst, i64)],
) -> Option<i128> {
    if nested.iter().any(|(n, _)| n == dividend) {
        return Some(VER_INF);
    }
    // Fold const-mods first (`i + (5%8)` → `i+5`): exact when operands
    // and result fit i64 (no wrap either way — else refuse the fold and
    // let the shape matchers below miss safely).
    let folded = fold_const_mods(dividend);
    let d = folded.as_ref().unwrap_or(dividend);
    if let Some(v) = const_value(d) {
        return if v >= 0 { Some(VER_INF) } else { None };
    }
    counter_shape(d, counter, lt)
}

/// Clone with const `a % b` / `a / b` subtrees (fitting i64 throughout)
/// folded to Num. Used for shapes like `i + (5%8)`; anything not exactly
/// foldable is left structurally intact (miss downstream, safe).
fn fold_const_mods(a: &ArithAst) -> Option<ArithAst> {
    fn fits(v: i128) -> bool {
        v >= i64::MIN as i128 && v <= i64::MAX as i128
    }
    fn go(a: &ArithAst) -> (ArithAst, bool) {
        match a {
            ArithAst::Bin { op, lhs, rhs } if op == "%" || op == "/" => {
                let (l, lc) = go(lhs);
                let (r, rc) = go(rhs);
                match (&l, &r) {
                    (ArithAst::Num(aa), ArithAst::Num(bb)) if *bb != 0 => {
                        let (ai, bi) = (*aa as i128, *bb as i128);
                        let v = if op == "%" { ai % bi } else { ai / bi };
                        if fits(ai) && fits(bi) && fits(v) {
                            return (ArithAst::Num(v as i64), true);
                        }
                        (
                            ArithAst::Bin {
                                op: op.clone(),
                                lhs: Box::new(l),
                                rhs: Box::new(r),
                            },
                            lc || rc,
                        )
                    }
                    _ => (
                        ArithAst::Bin {
                            op: op.clone(),
                            lhs: Box::new(l),
                            rhs: Box::new(r),
                        },
                        lc || rc,
                    ),
                }
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let (l, lc) = go(lhs);
                let (r, rc) = go(rhs);
                if lc || rc {
                    (
                        ArithAst::Bin {
                            op: op.clone(),
                            lhs: Box::new(l),
                            rhs: Box::new(r),
                        },
                        true,
                    )
                } else {
                    ((*a).clone(), false)
                }
            }
            ArithAst::Un { op, arg } => {
                let (x, xc) = go(arg);
                if xc {
                    (
                        ArithAst::Un { op: op.clone(), arg: Box::new(x) },
                        true,
                    )
                } else {
                    ((*a).clone(), false)
                }
            }
            _ => ((*a).clone(), false),
        }
    }
    let (out, changed) = go(a);
    if changed {
        Some(out)
    } else {
        None
    }
}
/// Constant value of a pure-literal subtree, or None.
fn const_value(a: &ArithAst) -> Option<i128> {
    match a {
        ArithAst::Num(n) => Some(*n as i128),
        ArithAst::Bin { op, lhs, rhs } => {
            let (l, r) = (const_value(lhs)?, const_value(rhs)?);
            match op.as_str() {
                "+" => l.checked_add(r),
                "-" => l.checked_sub(r),
                "*" => l.checked_mul(r),
                _ => None,
            }
        }
        ArithAst::Un { op, arg } => {
            let v = const_value(arg)?;
            match op.as_str() {
                "-" => Some(-v),
                "+" => Some(v),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Threshold from a pure-counter skeleton (no nested mods inside —
/// those fall back above): bare (INF), linear `i*c+k` (c>0, k>=0
/// lits), square `i*i`. Caller guarantees init_ge_zero.
fn counter_shape(a: &ArithAst, counter: &str, lt: bool) -> Option<i128> {
    if matches!(a, ArithAst::Var(v) | ArithAst::Ident(v) if v == counter) {
        return Some(VER_INF);
    }
    if let Some((c, k)) = linear_parts(a, counter) {
        if c > 0 && k >= 0 {
            let f = (VER_M - k) / c;
            if f < 0 {
                return None;
            }
            return Some(if lt { f + 1 } else { f });
        }
        return None;
    }
    if let ArithAst::Bin { op, lhs, rhs } = a {
        if op == "*" {
            let li = matches!(lhs.as_ref(), ArithAst::Var(v) | ArithAst::Ident(v) if v == counter);
            let ri = matches!(rhs.as_ref(), ArithAst::Var(v) | ArithAst::Ident(v) if v == counter);
            if li && ri {
                let s = isqrt_floor(VER_M);
                return Some(if lt { s + 1 } else { s });
            }
        }
    }
    None
}

/// Split linear `i*c+k` (either order, k=0 allowed): returns (c, k).
fn linear_parts(a: &ArithAst, counter: &str) -> Option<(i128, i128)> {
    match a {
        ArithAst::Bin { op, lhs, rhs } if op == "+" => lin_const_side(lhs, rhs, counter),
        ArithAst::Bin { op, lhs, rhs } if op == "*" => {
            match (lin_term(lhs, counter), lin_term(rhs, counter)) {
                (Some((true, _)), Some((false, c))) => Some((c, 0)),
                (Some((false, c)), Some((true, _))) => Some((c, 0)),
                _ => None,
            }
        }
        ArithAst::Var(v) | ArithAst::Ident(v) if v == counter => Some((1, 0)),
        _ => None,
    }
}

/// (is_counter_side, const_value) for a multiply operand.
fn lin_term(a: &ArithAst, counter: &str) -> Option<(bool, i128)> {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) if v == counter => Some((true, 0)),
        ArithAst::Num(n) => Some((false, *n as i128)),
        _ => None,
    }
}

/// Split `X + K` where exactly one side mentions the counter; the
/// counter side must be `i*c`/`c*i`/bare-`i`, the other a Num literal.
fn lin_const_side(lhs: &ArithAst, rhs: &ArithAst, counter: &str) -> Option<(i128, i128)> {
    fn mentions(a: &ArithAst, counter: &str) -> bool {
        match a {
            ArithAst::Var(v) | ArithAst::Ident(v) => v == counter,
            ArithAst::Bin { lhs, rhs, .. } => mentions(lhs, counter) || mentions(rhs, counter),
            ArithAst::Un { arg, .. } => mentions(arg, counter),
            ArithAst::Cond { test, then, else_ } => {
                mentions(test, counter) || mentions(then, counter) || mentions(else_, counter)
            }
            ArithAst::Assign { var, rhs, .. } => var == counter || mentions(rhs, counter),
            ArithAst::Index { var, key } => var == counter || mentions(key, counter),
            _ => false,
        }
    }
    let (cside, kside) = if mentions(lhs, counter) && !mentions(rhs, counter) {
        (lhs, rhs)
    } else if mentions(rhs, counter) && !mentions(lhs, counter) {
        (rhs, lhs)
    } else {
        return None;
    };
    let (c, _) = match cside {
        ArithAst::Bin { op, lhs: l2, rhs: r2 } if op == "*" => {
            match (lin_term(l2, counter), lin_term(r2, counter)) {
                (Some((true, _)), Some((false, c))) => (c, 0),
                (Some((false, c)), Some((true, _))) => (c, 0),
                _ => return None,
            }
        }
        ArithAst::Var(v) | ArithAst::Ident(v) if v == counter => (1, 0),
        _ => return None,
    };
    let k = match kside {
        ArithAst::Num(n) => *n as i128,
        _ => return None,
    };
    Some((c, k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(nm: &str) -> ArithAst {
        ArithAst::Var(nm.to_string())
    }
    fn num(n: i64) -> ArithAst {
        ArithAst::Num(n)
    }
    fn bin(op: &str, l: ArithAst, r: ArithAst) -> ArithAst {
        ArithAst::Bin { op: op.to_string(), lhs: Box::new(l), rhs: Box::new(r) }
    }

    #[test]
    fn bare_is_inf() {
        assert_eq!(mask_threshold(&var("i"), 8, "i", true, true), Some(VER_INF));
    }

    #[test]
    fn square_threshold_brackets_i64() {
        let sq = bin("*", var("i"), var("i"));
        assert_eq!(mask_threshold(&sq, 251, "i", true, true), Some(3037000500));
        assert_eq!(mask_threshold(&sq, 251, "i", false, true), Some(3037000499));
        assert!((3037000499i128).pow(2) < (1i128 << 63));
        assert!((3037000500i128).pow(2) >= (1i128 << 63));
    }

    #[test]
    fn linear_threshold_math() {
        let lin = bin("+", bin("*", var("i"), num(37)), num(3));
        let m: i128 = (1 << 63) - 1;
        assert_eq!(mask_threshold(&lin, 251, "i", true, true), Some((m - 3) / 37 + 1));
    }

    #[test]
    fn refuses() {
        let negk = bin("+", bin("*", var("i"), num(53)), num(-1000000));
        assert_eq!(mask_threshold(&negk, 256, "i", true, true), None);
        let sq = bin("*", var("i"), var("i"));
        assert_eq!(mask_threshold(&sq, 251, "i", true, false), None);
        assert_eq!(mask_threshold(&var("w"), 8, "i", true, true), None);
        let cubic = bin("*", bin("*", var("i"), var("i")), var("i"));
        assert_eq!(mask_threshold(&cubic, 8, "i", true, true), None);
        assert_eq!(mask_threshold(&var("i"), 0, "i", true, true), None);
        assert_eq!(mask_threshold(&var("i"), -8, "i", true, true), None);
    }

    #[test]
    fn const_dividends() {
        assert_eq!(mask_threshold(&num(42), 8, "i", true, true), Some(VER_INF));
        assert_eq!(mask_threshold(&num(-5), 8, "i", true, true), None);
        assert_eq!(
            mask_threshold(&bin("+", num(20), num(22)), 8, "i", true, true),
            Some(VER_INF)
        );
    }

    #[test]
    fn pow2_lit() {
        assert!(is_pow2_lit(1) && is_pow2_lit(2) && is_pow2_lit(256) && is_pow2_lit(4294967296));
        assert!(!is_pow2_lit(0) && !is_pow2_lit(-8) && !is_pow2_lit(251) && !is_pow2_lit(3));
    }
}
