//! BigInt ranges for the Python reading — exact unbounded integer
//! intervals where i128 cannot go (past ~1.7e38, and anywhere the bash
//! walk wraps: `big = 10000000000000000000` once presented as
//! -8446744073709551616).
//!
//! Python ints are unbounded, so this lattice is exact: literals of any
//! size, unbounded `+,-,*`, floor `%`/`//`, capped `**`. There is no
//! fixpoint — straight-line binds join (whole-life), branches join,
//! loop bodies contribute only provable `for`-counters; everything else
//! a body assigns widens to unknown. Callers MERGE with the i128 walk
//! (prefer exact-BigInt when present, else i128): the i128 fixpoint keeps
//! loop-carried convergence, BigInt keeps huge-value exactness, and each
//! covers the other's blindness. Vetoes/widths keep their i128 inputs.

use num_bigint::BigInt;
use std::collections::{HashMap, HashSet};

use crate::ir::{ArithAst, IrExpr, IrStmt, IrType};
use crate::shir::{collect_assigned, for_iter_range};
use crate::shir_nodes::ExtNode;

/// Unbounded integer interval; None = unknown (decline).
pub type BigRange = Option<(BigInt, BigInt)>;

fn bhull(a: BigRange, b: BigRange) -> BigRange {
    match (a, b) {
        (Some((l1, h1)), Some((l2, h2))) => {
            Some((if l1 < l2 { l1 } else { l2 }, if h1 > h2 { h1 } else { h2 }))
        }
        _ => None,
    }
}

fn bbind(state: &mut HashMap<String, BigRange>, name: String, r: BigRange) {
    let next = match state.get(&name) {
        Some(prev) => bhull(prev.clone(), r),
        None => r,
    };
    state.insert(name, next);
}

/// Parse an integer literal of any size (underscores, 0x/0o/0b
/// prefixes, decimal). Unbounded — no domain cap (Python ints).
fn big_parse_int(sv: &str) -> Option<BigInt> {
    let s = sv.trim().replace('_', "");
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(&s)),
    };
    let (radix, digits) = if let Some(h) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        (16u32, h)
    } else if let Some(h) = rest.strip_prefix("0b").or_else(|| rest.strip_prefix("0B")) {
        (2u32, h)
    } else if let Some(h) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O")) {
        (8u32, h)
    } else {
        (10u32, rest)
    };
    if digits.is_empty() {
        return None;
    }
    let v = BigInt::parse_bytes(digits.as_bytes(), radix)?;
    Some(if neg { -v } else { v })
}

fn floor_div(a: &BigInt, b: &BigInt) -> Option<BigInt> {
    if b == &BigInt::from(0) {
        return None;
    }
    let q = a / b;
    let r = a % b;
    if r != BigInt::from(0) && (r < BigInt::from(0)) != (b < &BigInt::from(0)) {
        Some(q - 1)
    } else {
        Some(q)
    }
}

fn floor_mod(a: &BigInt, b: &BigInt) -> Option<BigInt> {
    if b == &BigInt::from(0) {
        return None;
    }
    let r = a % b;
    if r != BigInt::from(0) && (r < BigInt::from(0)) != (b < &BigInt::from(0)) {
        Some(r + b)
    } else {
        Some(r)
    }
}

/// Exact evaluation of an arith tree (Python-int reading). Unknown
/// nodes, division by zero, negative `**` exponents, and float-producing
/// `/` yield None. Unbounded — overflow never degrades to unknown.
pub(crate) fn big_arith_range(
    a: &ArithAst,
    env: &HashMap<String, (BigInt, BigInt)>,
) -> BigRange {
    match a {
        ArithAst::Num(n) => Some((BigInt::from(*n), BigInt::from(*n))),
        ArithAst::Var(n) | ArithAst::Ident(n) => env.get(n).cloned(),
        ArithAst::Index { .. } => None,
        ArithAst::Bin { op, lhs, rhs } => {
            let o = op.as_str();
            // comparisons are exact single bits
            if matches!(o, "==" | "!=" | "<" | ">" | "<=" | ">=") {
                big_arith_range(lhs, env)?;
                big_arith_range(rhs, env)?;
                return Some((BigInt::from(0), BigInt::from(1)));
            }
            let (l0, l1) = big_arith_range(lhs, env)?;
            let (r0, r1) = big_arith_range(rhs, env)?;
            match o {
                "+" => Some((l0 + &r0, l1 + &r1)),
                "-" => Some((l0 - &r1, l1 - &r0)),
                "*" => {
                    let c = [&l0 * &r0, &l0 * &r1, &l1 * &r0, &l1 * &r1];
                    let mut lo = c[0].clone();
                    let mut hi = c[0].clone();
                    for v in &c[1..] {
                        if *v < lo {
                            lo = v.clone();
                        }
                        if *v > hi {
                            hi = v.clone();
                        }
                    }
                    Some((lo, hi))
                }
                "**" => {
                    // exact only for a single nonnegative exponent; cap
                    // the size so evaluation itself stays cheap
                    if l0 != l1 || r0 != r1 || r0 < BigInt::from(0) {
                        return None;
                    }
                    if r0 > BigInt::from(4096) {
                        return None;
                    }
                    let e: u32 = r0.to_string().parse().ok()?;
                    if l0.to_string().len() > 10000 {
                        return None;
                    }
                    let v = l0.pow(e);
                    if v.to_string().len() > 40000 {
                        return None;
                    }
                    Some((v.clone(), v))
                }
                "%" => {
                    // Floor-mod's value set over an interval is an interval
                    // the *corners* miss (the DivModPow2 class): corner
                    // min/max here was unsound. The sign-of-divisor
                    // property needs NO dividend bound -- the old comment
                    // was right, the corner loop beside it was not -- so:
                    // point-exact when both ranges are points, otherwise
                    // the divisor's sign alone bounds the residue, and a
                    // straddling divisor range is refused.
                    // (floor_mod's b = 0 → None is the point-branch guard.)
                    if l0 == l1 && r0 == r1 {
                        let m = floor_mod(&l0, &r0)?;
                        Some((m.clone(), m))
                    } else if r0 > BigInt::from(0) {
                        Some((BigInt::from(0), &r1 - 1))
                    } else if r1 < BigInt::from(0) {
                        Some((&r0 + 1, BigInt::from(0)))
                    } else {
                        None
                    }
                }
                "//" => {
                    // Corners suffice only for POSITIVE divisor ranges:
                    // floor-div jumps at the pole ([-5,3] claimed (-2,2),
                    // true 6 // -1 = -6), and divisor monotonicity -- the
                    // reason corners work at all -- is only available here
                    // for 0 < b. Refuse every other divisor range; with
                    // r0 > 0 neither endpoint is zero, so the loop needs
                    // no zero check.
                    if r0 <= BigInt::from(0) {
                        return None;
                    }
                    let mut first = true;
                    let (mut lo, mut hi) = (BigInt::from(0), BigInt::from(0));
                    for b in [&r0, &r1] {
                        for x in [&l0, &l1] {
                            let q = floor_div(x, b)?;
                            if first {
                                lo = q.clone();
                                hi = q;
                                first = false;
                            } else {
                                if q < lo {
                                    lo = q.clone();
                                }
                                if q > hi {
                                    hi = q;
                                }
                            }
                        }
                    }
                    Some((lo, hi))
                }
                // `/` is true division (float domain); shifts/bitwise
                // have unmodelled C semantics here
                _ => None,
            }
        }
        ArithAst::Un { op, arg } => {
            let (l0, l1) = big_arith_range(arg, env)?;
            if op == "-" {
                Some((-l1, -l0))
            } else {
                None
            }
        }
        ArithAst::Cond { test, then, else_ } => {
            big_arith_range(test, env)?;
            bhull(big_arith_range(then, env), big_arith_range(else_, env))
        }
        // in-expression writes (`x=$((x++))`): the target's new value is
        // unmodelled here (the statement walk binds it) — unknown value.
        ArithAst::Assign { .. } | ArithAst::IncDec { .. } => None,
        // A float-typed cast has no integer value to read -- name a
        // range for it only if eval reads it, and eval does not (the
        // fix mirrors Cast f a: f ⇒ refuse). Integer-identity casts
        // recurse, as v0 did.
        ArithAst::Cast { ty, arg } if matches!(ty, IrType::Float(_)) => None,
        ArithAst::Cast { arg, .. } => big_arith_range(arg, env),
        ArithAst::Sizeof(_) => None,
    }
}

fn big_ir_range(e: &IrExpr, state: &HashMap<String, BigRange>) -> BigRange {
    match e {
        IrExpr::Int(i) => Some((BigInt::from(*i), BigInt::from(*i))),
        // numeric strings parse unbounded (no domain cap — Python ints);
        // booleans are NOT integers for typing (True prints as "True").
        IrExpr::Str(sv, _) => {
            let v = big_parse_int(sv.trim())?;
            Some((v.clone(), v))
        }
        IrExpr::Bool(_) => None,
        IrExpr::Var(n, _) => state.get(n).cloned().flatten(),
        IrExpr::Call { func, args } if func == "getVar" => match args.as_slice() {
            [IrExpr::Str(n, _)] => state.get(n).cloned().flatten(),
            _ => None,
        },
        IrExpr::Call { func, args } if func == "assign" => match args.as_slice() {
            [IrExpr::Str(n, _), IrExpr::Str(op, _), e] if op == "=" => {
                let _ = n;
                big_ir_range(e, state)
            }
            [IrExpr::Str(n, _), IrExpr::Str(op, _), e] if op == "+=" => {
                let (k0, k1) = big_ir_range(e, state)?;
                let (lo, hi) = state.get(n).cloned().flatten()?;
                Some((lo + &k0, hi + &k1))
            }
            _ => None,
        },
        IrExpr::Call { func, args } if func == "setVar" => match args.as_slice() {
            [IrExpr::Str(_, _), v] => big_ir_range(v, state),
            _ => None,
        },
        IrExpr::Arith(a) => {
            let env: HashMap<String, (BigInt, BigInt)> = state
                .iter()
                .filter_map(|(n, r)| r.clone().map(|r| (n.clone(), r)))
                .collect();
            big_arith_range(a, &env)
        }
        _ => None,
    }
}

/// Per-scope exact-BigInt ranges (no fixpoint — see module docs).
/// Loop bodies contribute only provable `for`-counters; everything else
/// a body assigns widens to unknown. No descent into nested scopes
/// (the caller runs each scope separately, mirroring analyze_scopes).
pub fn analyze_var_ranges_big(stmts: &[IrStmt]) -> HashMap<String, (BigInt, BigInt)> {
    use crate::ir::IrStmt;
    /// join-merge a list of arm bodies (each arm may or may not run -
    /// whole-life is the union).
    fn walk_arms(arms: &[&Vec<IrStmt>], state: &mut HashMap<String, BigRange>) {
        let mut merged: HashMap<String, BigRange> = HashMap::new();
        for body in arms {
            let mut s = state.clone();
            walk(body, &mut s);
            for (k, v) in s {
                let cur = merged.remove(&k).flatten();
                merged.insert(k, bhull(cur, v));
            }
        }
        // names the arms never touched keep the pre-state
        for (k, v) in merged {
            let cur = state.get(&k).cloned().flatten();
            state.insert(k, bhull(cur, v));
        }
    }
    /// widen every name assigned under these statements to unknown,
    /// except `keep` (a provable counter).
    fn widen_assigned(stmts: &[IrStmt], keep: &HashSet<String>, state: &mut HashMap<String, BigRange>) {
        let mut assigned = HashSet::new();
        for s in stmts {
            collect_assigned(s, &mut assigned);
        }
        for n in assigned {
            if !keep.contains(&n) {
                state.insert(n, None);
            }
        }
    }
    /// In-expression arith writes (`x=$((x++))`) invalidate the target:
    /// the written value is unmodelled here, so the target goes unknown.
    fn poison_arith_writes(a: &ArithAst, state: &mut HashMap<String, BigRange>) {
        let mut names = HashSet::new();
        crate::shir::arith_write_names(a, &mut names);
        for n in names {
            state.insert(n, None);
        }
    }
    /// Poison arith-write targets anywhere inside a value expression.
    fn poison_expr_writes(e: &IrExpr, state: &mut HashMap<String, BigRange>) {
        use crate::ir::IrExpr;
        match e {
            IrExpr::Arith(a) => poison_arith_writes(a, state),
            IrExpr::Call { args, .. } => {
                for a in args {
                    poison_expr_writes(a, state);
                }
            }
            IrExpr::Array(items) => {
                for a in items {
                    poison_expr_writes(a, state);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                poison_expr_writes(lhs, state);
                poison_expr_writes(rhs, state);
            }
            _ => {}
        }
    }
    fn walk(stmts: &[IrStmt], state: &mut HashMap<String, BigRange>) {
        for s in stmts {
            match s {
                IrStmt::Assign { targets, expr, .. }
                    if targets.len() == 1 && targets[0].indices.is_empty() =>
                {
                    let name = targets[0].var.clone();
                    poison_expr_writes(expr, state);
                    bbind(state, name, big_ir_range(expr, state));
                }
                IrStmt::Assign { targets, .. } => {
                    // multi-target / indexed: distribution unmodelled
                    for t in targets {
                        if t.indices.is_empty() {
                            state.insert(t.var.clone(), None);
                        }
                    }
                }
                IrStmt::Declare { vars, init, .. } => {
                    if vars.len() == 1 {
                        if let Some(init) = init {
                            poison_expr_writes(init, state);
                            bbind(state, vars[0].name.clone(), big_ir_range(init, state));
                            continue;
                        }
                    }
                    for v in vars {
                        state.insert(v.name.clone(), None);
                    }
                }
                IrStmt::Block(b) => walk(b, state),
                IrStmt::Expr(crate::ir::IrExpr::Call { func, args })
                    if func == "setVar" || func == "assign" =>
                {
                    if let [crate::ir::IrExpr::Str(name, _), ..] = args.as_slice() {
                        if let Some(v) = args.last() {
                            poison_expr_writes(v, state);
                            bbind(state, name.clone(), big_ir_range(v, state));
                        }
                    }
                }
                IrStmt::Expr(crate::ir::IrExpr::Call { func, args }) if func == "exec" => {
                    if let [crate::ir::IrExpr::Str(name, _), crate::ir::IrExpr::Array(items)] =
                        args.as_slice()
                    {
                        if matches!(
                            name.as_str(),
                            "local" | "declare" | "readonly" | "export" | "typeset"
                        ) {
                            for it in items {
                                if let crate::ir::IrExpr::Str(sv, _) = it {
                                    let mut parts = sv.splitn(2, '=');
                                    if let (Some(n), Some(v)) = (parts.next(), parts.next()) {
                                        if !n.is_empty()
                                            && n.chars().next().is_some_and(|c| {
                                                c.is_ascii_alphabetic() || c == '_'
                                            })
                                            && n.chars().all(|c| {
                                                c.is_ascii_alphanumeric() || c == '_'
                                            })
                                        {
                                            // failed parse ⇒ unknown (strictly
                                            // sounder than leaving a stale range)
                                            bbind(state, n.to_string(), big_parse_int(v.trim()).map(|x| (x.clone(), x)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                IrStmt::Ext(n) => {
                    for c in ExtNode::children(&**n) {
                        walk(std::slice::from_ref(c), state);
                    }
                }
                IrStmt::If { cond: _, then, elsifs, else_, .. } => {
                    let mut arms: Vec<&Vec<IrStmt>> = vec![then];
                    for (_, b) in elsifs {
                        arms.push(b);
                    }
                    arms.push(else_);
                    walk_arms(&arms, state);
                }
                IrStmt::Try { body, excepts, else_body, finally_body } => {
                    // an exception can exit at any point: join the
                    // pre-state with every arm, like the i128 walk.
                    let pre = state.clone();
                    let mut arms: Vec<&Vec<IrStmt>> = vec![body];
                    for e in excepts {
                        arms.push(&e.body);
                    }
                    arms.push(else_body);
                    arms.push(finally_body);
                    // seed each arm from pre (not sequentially — arms are
                    // exclusive), then union with pre for the fall-through.
                    let mut merged: HashMap<String, BigRange> = HashMap::new();
                    for a in &arms {
                        let mut st = pre.clone();
                        walk(a, &mut st);
                        for (k, v) in st {
                            let cur = merged.remove(&k).flatten();
                            merged.insert(k, bhull(cur, v));
                        }
                    }
                    for (k, v) in merged {
                        let cur = pre.get(&k).cloned().flatten();
                        state.insert(k, bhull(cur, v));
                    }
                    for (k, v) in pre {
                        state.entry(k).or_insert(v);
                    }
                }
                IrStmt::For { var, iter, body } => {
                    // provable counter only; the body may run zero times
                    // and carried values need a fixpoint we don't run —
                    // widen everything else the body assigns.
                    if let Some((lo, hi)) = for_iter_range(iter) {
                        bbind(
                            state,
                            var.clone(),
                            Some((BigInt::from(lo), BigInt::from(hi))),
                        );
                    }
                    let mut keep = HashSet::new();
                    keep.insert(var.clone());
                    widen_assigned(body, &keep, state);
                }
                IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                    widen_assigned(body, &HashSet::new(), state);
                }
                IrStmt::ForInit { .. } => {
                    // C-style loops need trip/step analysis to bound —
                    // decline everything assigned under them.
                    widen_assigned(std::slice::from_ref(s), &HashSet::new(), state);
                }
                IrStmt::Function { .. } => {}
                _ => {
                    // anything unmodelled: its writes are unknown (sound;
                    // the i128 walk may still know them via merge).
                    widen_assigned(std::slice::from_ref(s), &HashSet::new(), state);
                }
            }
        }
    }
    let mut state: HashMap<String, BigRange> = HashMap::new();
    walk(stmts, &mut state);
    state
        .into_iter()
        .filter_map(|(n, r)| r.map(|r| (n, r)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    fn bigranges_of(src: &str) -> HashMap<String, (BigInt, BigInt)> {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir_raw(&cmds);
        analyze_var_ranges_big(&prog.stmts)
    }

    fn bi(n: i128) -> BigInt {
        BigInt::from(n)
    }

    #[test]
    fn huge_literal_is_exact_not_unknown() {
        // past i128::MAX — the i128 walk declines; BigInt names it
        let r = bigranges_of("x=100000000000000000000000000000000000000000");
        let (lo, hi) = r.get("x").expect("x has a range");
        assert_eq!((lo, hi), (&bi(10).pow(41), &bi(10).pow(41)));
    }

    #[test]
    fn u64_literal_is_exact_not_wrapped() {
        // 1e19 once presented as -8446744073709551616 (i64 wrap)
        let r = bigranges_of("big=10000000000000000000");
        assert_eq!(
            r.get("big"),
            Some(&(bi(10000000000000000000), bi(10000000000000000000)))
        );
    }

    #[test]
    fn huge_intermediates_stay_exact() {
        // 2**100 * 3: checked-i128 gives up; BigInt is exact
        let r = bigranges_of("x=$((2**100 * 3))");
        let want = BigInt::from(2).pow(100u32) * BigInt::from(3);
        assert_eq!(r.get("x"), Some(&(want.clone(), want)));
    }

    #[test]
    fn floor_mod_negative_dividend() {
        // Python sign-of-divisor needs no dividend bound: -7 % 3 == 2
        let env: HashMap<String, (BigInt, BigInt)> = HashMap::new();
        let neg7 = ArithAst::Bin {
            op: "%".to_string(),
            lhs: Box::new(ArithAst::Un {
                op: "-".to_string(),
                arg: Box::new(ArithAst::Num(7)),
            }),
            rhs: Box::new(ArithAst::Num(3)),
        };
        assert_eq!(
            big_arith_range(&neg7, &env),
            Some((bi(2), bi(2)))
        );
    }

    /// The DivModPow2-class corner bug: min/max of floor-mod at the
    /// interval corners misses the value set (29 % 30 = 29 ∉ [0,10]).
    #[test]
    fn floor_mod_positive_range_is_sign_bounded() {
        let mut env: HashMap<String, (BigInt, BigInt)> = HashMap::new();
        env.insert("x".to_string(), (bi(0), bi(100)));
        let m = ArithAst::Bin {
            op: "%".to_string(),
            lhs: Box::new(ArithAst::Var("x".to_string())),
            rhs: Box::new(ArithAst::Num(30)),
        };
        // sound (0 ≤ r < 30) where the corner loop claimed (0, 10)
        assert_eq!(big_arith_range(&m, &env), Some((bi(0), bi(29))));
    }

    /// Floor-div jumps at the pole: 6 // -1 = -6 is outside any
    /// corner-min/max of [-5,3] -- straddling divisor ranges refuse.
    #[test]
    fn floor_div_straddling_divisor_refused() {
        let mut env: HashMap<String, (BigInt, BigInt)> = HashMap::new();
        env.insert("b".to_string(), (bi(-5), bi(3)));
        let d = ArithAst::Bin {
            op: "//".to_string(),
            lhs: Box::new(ArithAst::Num(6)),
            rhs: Box::new(ArithAst::Var("b".to_string())),
        };
        assert_eq!(big_arith_range(&d, &env), None);
        // positive divisor ranges keep their exact corners
        env.insert("b".to_string(), (bi(2), bi(5)));
        assert_eq!(
            big_arith_range(&d, &env),
            Some((bi(1), bi(3)))
        );
    }

    /// A float-typed cast has no integer value to name a range for;
    /// integer-identity casts still recurse (v0 behavior).
    #[test]
    fn float_cast_refused_integer_cast_recurse() {
        let env: HashMap<String, (BigInt, BigInt)> = HashMap::new();
        let f = ArithAst::Cast {
            ty: IrType::Float(64),
            arg: Box::new(ArithAst::Num(5)),
        };
        let i = ArithAst::Cast {
            ty: IrType::Int64,
            arg: Box::new(ArithAst::Num(5)),
        };
        assert_eq!(big_arith_range(&f, &env), None);
        assert_eq!(big_arith_range(&i, &env), Some((bi(5), bi(5))));
    }

    #[test]
    fn straight_redefinition_joins_huge() {
        let r = bigranges_of("x=3000000000000000000000\necho $x\nx=1");
        assert_eq!(r.get("x"), Some(&(bi(1), bi(3000000000000000000000))));
    }

    #[test]
    fn loop_body_widens_non_counters() {
        // counter exact, accumulator unknown (the i128 fixpoint covers it)
        let r = bigranges_of("s=0\nfor n in 1 2 3; do s=$((s+n)); done");
        assert_eq!(r.get("n"), Some(&(bi(1), bi(3))));
        assert!(!r.contains_key("s"));
    }
}


#[cfg(test)]
mod horner_deep_probe {
    use super::*;
    use crate::ir::{ArithAst, IrType};
    use num_bigint::BigInt;
    use std::collections::HashMap;

    #[test]
    fn deep_horner_evaluates() {
        // ((0*10+1)*10+0)*10+... 40 ops like the frontend's bignum conveyance
        let mut acc = ArithAst::Num(0);
        for d in [1i64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
            let step = ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(ArithAst::Bin {
                    op: "*".to_string(),
                    lhs: Box::new(ArithAst::Cast {
                        ty: IrType::Int64,
                        arg: Box::new(acc),
                    }),
                    rhs: Box::new(ArithAst::Cast {
                        ty: IrType::Int64,
                        arg: Box::new(ArithAst::Num(10)),
                    }),
                }),
                rhs: Box::new(ArithAst::Cast {
                    ty: IrType::Int64,
                    arg: Box::new(ArithAst::Num(d)),
                }),
            };
            acc = step;
        }
        let env: HashMap<String, (BigInt, BigInt)> = HashMap::new();
        let v = big_arith_range(&acc, &env);
        assert_eq!(v, Some((BigInt::from(10000000000000000000u64), BigInt::from(10000000000000000000u64))));
    }
}

#[cfg(test)]
mod horner_walk_probe {
    use super::super::bigint_range::*;
    use crate::ir::{ArithAst, AssignTarget, IrExpr, IrStmt, IrType};
    use num_bigint::BigInt;

    #[test]
    fn walk_records_horner_assign() {
        let mut acc = ArithAst::Num(0);
        for d in [1i64, 0] {
            acc = ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(ArithAst::Bin {
                    op: "*".to_string(),
                    lhs: Box::new(ArithAst::Cast {
                        ty: IrType::Int64,
                        arg: Box::new(acc),
                    }),
                    rhs: Box::new(ArithAst::Cast {
                        ty: IrType::Int64,
                        arg: Box::new(ArithAst::Num(10)),
                    }),
                }),
                rhs: Box::new(ArithAst::Cast {
                    ty: IrType::Int64,
                    arg: Box::new(ArithAst::Num(d)),
                }),
            };
        }
        let stmts = vec![IrStmt::Assign {
            targets: vec![AssignTarget { var: "big".into(), sigil: None, indices: vec![] }],
            expr: IrExpr::Arith(Box::new(acc)),
            asm: None,
        }];
        let r = analyze_var_ranges_big(&stmts);
        assert_eq!(r.get("big"), Some(&(BigInt::from(10), BigInt::from(10))));
    }
}
