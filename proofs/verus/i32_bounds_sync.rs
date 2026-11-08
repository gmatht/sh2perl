// GENERATED from sh2perl/src/transforms/i32_provable.rs @ 172be07162a2
// DO NOT EDIT — run harness/verus-sync.sh
// Region: the two abs-bound consts (hoisted), fn arith_abs_bound,
// fn arith_is_i32 — the LIVE estimator (target retargeted 2026-11-08).
// Rewrites: match-on-&str -> if-chains; (pred).then_some(e) -> explicit if.
// TRANSCRIPT, not paste-ready Verus: `?`, the `cap` closure and
// checked_* are kept verbatim on purpose.
//
// enum ArithAst constructors in scope: Num Var Ident Index Bin Un Cond Assign IncDec Sizeof Cast
verus! {

const I32_ABS_BOUND: i128 = 1i128 << 31;
const LITERAL_ABS_BOUND: i128 = 1i128 << 30;

fn arith_abs_bound(a: &ArithAst) -> Option<i128> {
    /// Refuse anything not provably `|v| < 2^31`; literals keep the
    /// stricter `|n| < 2^30` they had before the bound was composed.
    let cap = |b: i128| (if b < I32_ABS_BOUND { Some(b) } else { None });
    match a {
        ArithAst::Num(n) => {
            // `unsigned_abs`, not `abs`: i64::MIN.abs() overflows (and
            // panics in debug builds) instead of just being refused.
            let b = n.unsigned_abs() as i128;
            (if b < LITERAL_ABS_BOUND { Some(b) } else { None })
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let (l, r) = (arith_abs_bound(lhs)?, arith_abs_bound(rhs)?);
            // |lhs - rhs| <= |lhs| + |rhs|, exactly as for `+`
            if op.as_str() == "+" || op.as_str() == "-" { cap(l.checked_add(r)?) }
            else if op.as_str() == "*" { cap(l.checked_mul(r)?) }
            else { None }
        }
        ArithAst::Un { op, arg } => {
            if op.as_str() == "-" { arith_abs_bound(arg) }
            else { None }
        },
        ArithAst::Cond { test, then, else_, .. } => {
            arith_abs_bound(test)?; // the test is part of the compound
            // Each branch is already < 2^31, so their max is too; the
            // value is one of them, never an arithmetic combination.
            Some(arith_abs_bound(then)?.max(arith_abs_bound(else_)?))
        }
        // untyped var / ident / index / assignment / ++-- / sizeof / cast:
        // refuse (escape-classes may sharpen the Var case later)
        _ => None,
    }
}

fn arith_is_i32(a: &ArithAst) -> bool {
    arith_abs_bound(a).is_some()
}

} // verus!
