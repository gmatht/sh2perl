//! i32-provable — annotate the arithmetic expressions that are PROVABLY
//! 32-bit integers, the shIR equivalent of sh2runtime's estree pass
//! `lowerI32Trunc` (JS-only today: `Math.trunc(<i32-provable compound>)`
//! → `(<compound>) | 0` — the trailing ToInt32 is cheaper than the
//! JIT's Math.trunc sequence on compound FP chains).
//!
//! ## Need
//! The JS backend's `lowerI32Trunc` pass re-derives, on the estree, the
//! set of arithmetic compounds whose value is provably an i32 (all
//! literals small, no division-with-remainder edge, no unprovable
//! variable). That PROVABILITY is a program property — a shIR verdict —
//! not a JS one:
//!   - the JS renderer emits `| 0` (ToInt32) instead of `Math.trunc`;
//!   - the C/Go/Rust backends emit native `int32_t`/`int`/`i32` where
//!     today they must keep a wider type (or a runtime coerce);
//!   - the GLSL backend's `mediump int` discipline is exactly this
//!     bound (the shader's interval proof is the same analysis).
//!
//! ## The verdict
//! An `IrExpr::Arith` node is I32 when:
//!   - every literal `Num` in its tree fits i64 and the tree's dynamic
//!     range is provably inside ±2^31 (conservative: literals with
//!     |v| < 2^30 and ops + - * only — NO div/mod (the zero-divisor and
//!     truncation edge) and no `**` — the estimate stays monotone), AND
//!   - every `Var`/`Index` read is of a variable the escape-classes
//!     verdict (or a prior I32 verdict) proves integer — otherwise the
//!     value could be a string/float (refuse).
//! The verdict is per-ARITH-NODE, keyed by a stable path (function
//! name + statement index + expression path) so renderers can map it
//! back without pointer fragility.
//!
//! ## Scope
//! Analysis-only (like `sync-ok-loops`): verdicts stored in module
//! statics, read by the renderers. No structural mutation.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). The estree
//! worker mediates the renderer hooks: estree.rs reads the verdicts for
//! its ToInt32 lowering; the C/Go/Rust/GLSL generators for their native
//! int typing.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// i32 verdicts keyed by `fn_name` ("" = top level) + the statement
/// index of the Assign/Expr that carries the Arith node.
static VERDICTS: Mutex<Option<HashSet<(String, usize)>>> = Mutex::new(None);

/// Drop the cached verdict — the out-of-band caller's reset (see
/// `transforms::reset_verdicts`). These transforms are analysis-only: they
/// return `false` from `run()` and a RENDERER reads the verdict, so without
/// a reset a second run observes the PREVIOUS input's verdict.
pub fn clear_verdicts() {
    *VERDICTS.lock().unwrap() = None;
}

pub fn is_i32(fn_name: &str, stmt_idx: usize) -> bool {
    VERDICTS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(&(fn_name.to_string(), stmt_idx)))
        .unwrap_or(false)
}

/// Apply the transform (analysis-only — computes + caches the verdicts).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut out = HashSet::new();
    for (i, st) in stmts.iter().enumerate() {
        if stmt_carries_i32(st) {
            out.insert((String::new(), i));
        }
    }
    *VERDICTS.lock().unwrap() = Some(out);
    false // analysis-only
}

fn stmt_carries_i32(st: &IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_is_i32(expr),
        IrStmt::Expr(e) => expr_is_i32(e),
        _ => false,
    }
}

fn expr_is_i32(e: &IrExpr) -> bool {
    match e {
        IrExpr::Arith(a) => arith_is_i32(a),
        _ => false,
    }
}

/// Upper bound on `|value|` of the tree, or `None` (unprovable — refuse).
///
/// Every node COMPOSES its subtree bounds and refuses as soon as the
/// result would leave i32.  A leaf-only check is not a range check:
/// `2^29 * 2^29 * 2^29` has three literals under 2^30 and only `*`, and
/// evaluates to 2^87 — the first version of this function marked it I32.
/// `proofs/I32Provable.thy` machine-checks that counterexample next to
/// the soundness theorem for this version, so the two cannot drift apart
/// again silently.
///
/// Refusals only ever mark FEWER nodes (the manifest allows false
/// negatives), so tightening this cannot turn a passing verdict into a
/// wrong one.
fn arith_abs_bound(a: &ArithAst) -> Option<i128> {
    /// Refuse anything not provably `|v| < 2^31`; literals keep the
    /// stricter `|n| < 2^30` they had before the bound was composed.
    const I32_ABS_BOUND: i128 = 1i128 << 31;
    const LITERAL_ABS_BOUND: i128 = 1i128 << 30;
    let cap = |b: i128| (b < I32_ABS_BOUND).then_some(b);
    match a {
        ArithAst::Num(n) => {
            // `unsigned_abs`, not `abs`: i64::MIN.abs() overflows (and
            // panics in debug builds) instead of just being refused.
            let b = n.unsigned_abs() as i128;
            (b < LITERAL_ABS_BOUND).then_some(b)
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let (l, r) = (arith_abs_bound(lhs)?, arith_abs_bound(rhs)?);
            match op.as_str() {
                // |lhs - rhs| <= |lhs| + |rhs|, exactly as for `+`
                "+" | "-" => cap(l.checked_add(r)?),
                "*" => cap(l.checked_mul(r)?),
                // div/mod: truncation + zero-divisor edges — refuse
                _ => None,
            }
        }
        ArithAst::Un { op, arg } => match op.as_str() {
            "-" => arith_abs_bound(arg),
            _ => None,
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

// name: i32-provable
// prereqs: [escape-classes — the Var/Index refinement; without it the
//   verdict only fires on literal-only compounds, which is still the
//   common `$((a * 1000 + b))` emitter shape]
// invariant: analysis-only; no structural mutation. A node is marked
//   only when EVERY literal is small AND every op is + - * AND the
//   composed |value| bound stays under 2^31 at EVERY node (see
//   arith_abs_bound — the bound used to be checked at the leaves only,
//   which accepted 2^29 * 2^29 * 2^29 = 2^87; that false positive is
//   machine-checked in proofs/I32Provable.thy next to the soundness
//   theorem for the current estimator). False negatives are fine.
// scope: offered to estree (owner — supersedes lowerI32Trunc), c, go,
//   rust, glsl
// updates: 2026-10-23 arith_abs_bound composes the per-node bound
//   (leaf-only check was unsound); proofs/I32Provable.thy is the
//   checked obligation.

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: i64) -> ArithAst {
        ArithAst::Num(n)
    }
    fn mul(l: ArithAst, r: ArithAst) -> ArithAst {
        ArithAst::Bin {
            op: "*".to_string(),
            lhs: Box::new(l),
            rhs: Box::new(r),
        }
    }

    /// The leaf-only check marked this I32: three literals under 2^30,
    /// only `*`, value 2^87. Refused since arith_abs_bound composed the
    /// bound per node; the same counterexample is machine-checked in
    /// proofs/I32Provable.thy.
    #[test]
    fn overflow_compound_is_refused() {
        let boom = mul(mul(num(1 << 29), num(1 << 29)), num(1 << 29));
        assert!(!arith_is_i32(&boom));
    }

    /// The fix must only remove FALSE positives: a compound that really
    /// fits i32 is still marked.
    #[test]
    fn in_range_compound_is_marked() {
        let ok = ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(mul(num(1 << 29), num(3))), // 1_610_612_736
            rhs: Box::new(num(7)),
        };
        assert!(arith_is_i32(&ok));
    }

    /// The bound composes ACROSS nodes: each leaf is a legal literal
    /// (< 2^30), but their product is ~2^60 and is refused. Two maximal
    /// literals added give 2^31 - 2, which still fits — so the cap is the
    /// i32 bound exactly, not something cruder.
    #[test]
    fn composed_bound_over_i32_is_refused() {
        let prod = mul(num((1 << 30) - 1), num((1 << 30) - 1));
        assert!(!arith_is_i32(&prod));

        let sum = ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(num((1 << 30) - 1)),
            rhs: Box::new(num((1 << 30) - 1)),
        };
        assert!(arith_is_i32(&sum));
        assert!(arith_is_i32(&num((1 << 30) - 1)));
    }
}
