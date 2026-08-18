//! div-mod-pow2 — mark integer division/modulo by an exact power of two
//! on a provably non-negative operand as shiftable, the shIR annotation
//! the shift-capable backends (C/Go/Rust/Zig/JS) consume to emit
//! `>> n` / `& (2^n - 1)` instead of a division emulation.
//!
//! ## Need
//! The colour/type math in the texture generators and shaders does
//! fixed-point division by constants (`/1000`, `/128`, `/256`, `% 6`,
//! `% 97`). For the power-of-two divisors (`/128`, `/256`), a shift
//! (`>> 7`, `>> 8`) is one op where an integer division is several — but
//! ONLY on non-negative operands (C truncation `-3/2 = -1` vs `-3>>1 =
//! -2` differ). The GLSL backend CANNOT shift (ES 1.00 has no bitwise),
//! so this must be an ANNOTATION, not a neutral rewrite: each backend
//! chooses `>>`/`&` (C/Go/Rust/Zig/JS) or keeps `/` (glsl).
//!
//! ## The verdict (analysis-only, like i32-provable / sync-ok-loops)
//! An `Arith` division/modulo node is marked SHIFTABLE when:
//!   - the divisor is `Num(2^n)` (n in 1..=30), AND
//!   - the dividend is PROVABLY NON-NEGATIVE — a `Num >= 0`, or a
//!     `Bin{op: +|*, …}` over non-negative operands (no division/mod/var
//!     reads — a runtime var's sign is unprovable here). The i32-provable
//!     verdict can widen this later.
//! Anything else stays unmarked (a false-positive would change `-3/2`),
//! so the pass only fires on literal-known non-negative dividends.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). Renderer
//! hooks: the C/Go/Rust/Zig/JS generators emit `x >> n` (for `Div`) and
//! `x & (2^n - 1)` (for `Mod`) when the verdict is set; the GLSL backend
//! ignores the verdict and keeps `/`. Prereq: i32-provable (the widening
//! refinement).

use std::collections::HashSet;
use std::sync::Mutex;

use crate::ir::{ArithAst, IrStmt};

/// verdicts keyed by (fn-name, stmt index) — mirrors i32-provable.
static VERDICTS: Mutex<Option<HashSet<(String, usize)>>> = Mutex::new(None);

pub fn shiftable(fn_name: &str, stmt_idx: usize) -> bool {
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
        if let IrStmt::Assign { expr, .. } = st {
            if contains_shiftable_divmod(expr) {
                out.insert((String::new(), i));
            }
        }
    }
    *VERDICTS.lock().unwrap() = Some(out);
    false // analysis-only
}

/// Does this expression contain a pow2 non-negative div/mod anywhere?
fn contains_shiftable_divmod(e: &crate::ir::IrExpr) -> bool {
    match e {
        crate::ir::IrExpr::Arith(a) => arith_any_shiftable(a),
        crate::ir::IrExpr::BinOp { lhs, rhs, .. } => {
            contains_shiftable_divmod(lhs) || contains_shiftable_divmod(rhs)
        }
        _ => false,
    }
}

fn arith_any_shiftable(a: &ArithAst) -> bool {
    match a {
        ArithAst::Bin { op, lhs, rhs } => {
            let this = (matches!(op.as_str(), "/" | "%"))
                && pow2(rhs)
                && non_negative(lhs);
            this || arith_any_shiftable(lhs) || arith_any_shiftable(rhs)
        }
        ArithAst::Un { arg, .. } => arith_any_shiftable(arg),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_any_shiftable(test) || arith_any_shiftable(then) || arith_any_shiftable(else_)
        }
        _ => false,
    }
}

/// divisor is an exact power of two (Num(2^n), n in 1..=30)
fn pow2(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(n) if *n > 1 => (*n & (*n - 1)) == 0 && *n < (1 << 31),
        _ => false,
    }
}

/// provably non-negative: a literal `Num >= 0`, or a `+`/`*` of two
/// non-negative operands (no var reads, no div/mod)
fn non_negative(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(n) => *n >= 0,
        ArithAst::Bin { op, lhs, rhs } => {
            matches!(op.as_str(), "+" | "*") && non_negative(lhs) && non_negative(rhs)
        }
        _ => false,
    }
}

