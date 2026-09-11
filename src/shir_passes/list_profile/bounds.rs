//! Frequency bounds (LISTS.MD §4.2) — prototype.
//!
//! Two flavors: absolute trip-count upper bounds (the
//! `analyze_string_lengths` pattern — product of enclosing loop trips,
//! `None` when any enclosing loop is unbounded) and big-O loop-depth
//! classes per site. Upper bounds must never err LOW (choosing a fixed
//! array for an overflowing list is the exit-127 bug); erring HIGH only
//! pessimizes (chooses Vec over Fixed — always correct).

use std::collections::HashMap;

use crate::ir::{ArithAst, IrExpr};

use super::census::{ListInfo, Op};

/// Analysis context for trip inference: whole-program var ranges.
pub struct TripCtx {
    pub ranges: HashMap<String, (i128, i128)>,
}

/// Trip bound of one `while`/`do-while`/`c-for` iteration over `cond`:
/// `i < H` / `i <= H` / `i > L` / `i >= L` with a ranged counter and a
/// literal bound. Anything else is unbounded (`None`).
pub fn trip_of_loop(cond: &IrExpr, ranges: &HashMap<String, (i128, i128)>) -> Option<u64> {
    if let IrExpr::Arith(a) = cond {
        if let ArithAst::Bin { op, lhs, rhs } = a.as_ref() {
            return cmp_trip(op, lhs, rhs, ranges);
        }
    }
    None
}

fn num_leaf(a: &ArithAst) -> Option<i128> {
    match a {
        ArithAst::Num(n) => Some(*n as i128),
        _ => None,
    }
}

fn var_leaf(a: &ArithAst) -> Option<&str> {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => Some(v),
        _ => None,
    }
}

fn cmp_trip(
    op: &str,
    lhs: &ArithAst,
    rhs: &ArithAst,
    ranges: &HashMap<String, (i128, i128)>,
) -> Option<u64> {
    // normalize to (var, bound, upper?) — flip for `H > i` forms
    let (v, b, upper) = match (var_leaf(lhs), num_leaf(rhs), var_leaf(rhs), num_leaf(lhs)) {
        (Some(v), Some(b), _, _) => (v, b, op == "<" || op == "<="),
        (_, _, Some(v), Some(b)) => (v, b, op == ">" || op == ">="),
        _ => return None,
    };
    if op != "<" && op != "<=" && op != ">" && op != ">=" {
        return None;
    }
    let (lo, hi) = ranges.get(v).copied()?;
    let trips: i128 = if upper {
        let end = if op == "<" || op == ">" { b } else { b + 1 };
        // counter starts at lo (ascending): iterations ≈ end - lo
        end - lo
    } else {
        let start = if op == ">" || op == "<" { b } else { b - 1 };
        // descending from hi: iterations ≈ hi - start
        hi - start
    };
    if trips < 0 {
        return Some(0);
    }
    u64::try_from(trips).ok()
}

/// Absolute append upper bound: Σ elements × trip over append AND def
/// sites (a `setArray` def bulk-counts — see `SiteKind::Def`), widened
/// by constant indexed-write targets (`arr[5]=x` may grow len to 6).
/// `None` when the class has no Def site (param-fed/unknown length) or
/// any factor is unbounded. Never errs LOW: every counted site
/// over-approximates, and the census walkers are exhaustive (an
/// uncounted growth site would be a soundness bug).
/// Never errs LOW (every counted site over-approximates; an uncounted
/// site would be a soundness bug — the census walkers are exhaustive).
pub fn append_hi(info: &ListInfo) -> Option<u64> {
    // No in-program define: param-fed or otherwise unknown length.
    // (The literal floor is best-effort smallness for SmallVec gating —
    // it never feeds the upper bound, so def bulk is never double
    // counted.)
    if !info.ops.contains(&Op::Def) {
        return None;
    }
    let mut total = 0u64;
    for s in &info.append_sites {
        let trip = s.trip?;
        total = total.saturating_add(s.count.saturating_mul(trip));
    }
    if let Some(ix) = info.max_const_index {
        // len must exceed the max written index
        let need = ix + 1;
        if need > 0 {
            total = total.max(need.min(u64::MAX as i128) as u64);
        }
    }
    if info.dyn_index_write {
        return None;
    }
    Some(total)
}

/// Max enclosing-loop depth over an op class (big-O classifier).
pub fn max_site_depth(info: &ListInfo) -> usize {
    info.append_sites
        .iter()
        .chain(&info.midmut_sites)
        .chain(&info.concat_sites)
        .map(|s| s.depth)
        .max()
        .unwrap_or(0)
}

/// Any site of any counted class with an unbounded trip.
pub fn has_unbounded_site(info: &ListInfo) -> bool {
    info.append_sites
        .iter()
        .chain(&info.midmut_sites)
        .chain(&info.concat_sites)
        .any(|s| s.trip.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trip_lt_literal() {
        let mut ranges = HashMap::new();
        ranges.insert("i".to_string(), (1, 100));
        let cond = IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "<".to_string(),
            lhs: Box::new(ArithAst::Var("i".to_string())),
            rhs: Box::new(ArithAst::Num(11)),
        }));
        assert_eq!(trip_of_loop(&cond, &ranges), Some(10));
    }

    #[test]
    fn trip_unknown_counter_is_none() {
        let ranges = HashMap::new();
        let cond = IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "<".to_string(),
            lhs: Box::new(ArithAst::Var("i".to_string())),
            rhs: Box::new(ArithAst::Var("n".to_string())),
        }));
        assert_eq!(trip_of_loop(&cond, &ranges), None);
    }
}
