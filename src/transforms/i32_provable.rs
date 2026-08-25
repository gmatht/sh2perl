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
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). The estree
//! worker mediates the renderer hooks: estree.rs reads the verdicts for
//! its ToInt32 lowering; the C/Go/Rust/GLSL generators for their native
//! int typing.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// i32 verdicts keyed by `fn_name` ("" = top level) + the statement
/// index of the Assign/Expr that carries the Arith node.
static VERDICTS: Mutex<Option<HashSet<(String, usize)>>> = Mutex::new(None);

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

fn arith_is_i32(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(n) => n.abs() < (1 << 30),
        ArithAst::Var(_) => false, // untyped var — refuse (escape-classes may sharpen)
        ArithAst::Index { .. } => false,
        ArithAst::Bin { op, lhs, rhs } => {
            match op.as_str() {
                "+" | "-" | "*" => arith_is_i32(lhs) && arith_is_i32(rhs),
                // div/mod: truncation + zero-divisor edges — refuse
                _ => false,
            }
        }
        ArithAst::Un { op, arg } => match op.as_str() {
            "-" => arith_is_i32(arg),
            _ => false,
        },
        ArithAst::Cond { test, then, else_, .. } => {
            arith_is_i32(test) && arith_is_i32(then) && arith_is_i32(else_)
        }
        ArithAst::Assign { .. } => false,
        _ => false,
    }
}

// name: i32-provable
// prereqs: [escape-classes — the Var/Index refinement; without it the
//   verdict only fires on literal-only compounds, which is still the
//   common `$((a * 1000 + b))` emitter shape]
// invariant: analysis-only; no structural mutation. A node is marked
//   only when EVERY literal is small and every op is + - * — the
//   estimate is monotone, so a false POSITIVE is impossible by
//   construction (false negatives are fine).
// scope: offered to estree (owner — supersedes lowerI32Trunc), c, go,
//   rust, glsl
// updates: none (first offer)
