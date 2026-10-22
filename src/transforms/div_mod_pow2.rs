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
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). **The verdict has
//! NO consumer in this tree** (verified 2026-10-22: `div_mod_pow2::shiftable`
//! is called only from this module's own tests — no renderer reads it), so
//! the pass currently computes and discards its verdict set. The intended
//! hooks: the C/Go/Rust/Zig/JS generators emit `x >> n` (for `Div`) and
//! `x & (2^n - 1)` (for `Mod`) when the verdict is set; the GLSL backend
//! ignores the verdict and keeps `/`. Until a renderer reads it, this
//! module is dead code with a live contract — wire a consumer or delete the
//! pass; do not let the description above stand in for the wiring.
//! Prereq: i32-provable (the widening refinement).
//!
//! ## The guard is checked, not commented
//! The soundness obligation (fire ⟹ dividend provably non-negative) is
//! carried by the [`NonNeg`] evidence type, whose only producer is
//! [`prove_non_neg`]; [`arith_any_shiftable`] cannot form a verdict without
//! it. [`tests::refuses_every_flipped_guard`] pins the refusing half — each
//! case flips exactly one premise — and the three unit tests are
//! mutation-verified (removing the sign test, accepting `Un{op:"-"}`, and
//! dropping the `pow2` check each turn the suite red).

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
            // The guard, as a type: no NonNeg witness => no verdict.
            let this = (matches!(op.as_str(), "/" | "%"))
                && pow2(rhs)
                && prove_non_neg(lhs).is_some();
            this || arith_any_shiftable(lhs) || arith_any_shiftable(rhs)
        }
        ArithAst::Un { arg, .. } => arith_any_shiftable(arg),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_any_shiftable(test) || arith_any_shiftable(then) || arith_any_shiftable(else_)
        }
        _ => false,
    }
}

/// Evidence that an `ArithAst` dividend is provably non-negative — the
/// ONE precondition this pass exists to establish (`-3/2 = -1` truncating
/// vs `-3 >> 1 = -2` arithmetic). It has NO public constructor and NO
/// fields: the only way to obtain one is [`prove_non_neg`], so a future
/// edit cannot add a shift verdict without going through the proof. The
/// doc comment's "a false-positive would change `-3/2`" is this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NonNeg {
    _private: (),
}

/// The only producer of [`NonNeg`]: a literal `Num >= 0`, or a `+`/`*`
/// of two non-negative operands. Deliberately INCOMPLETE in the safe
/// direction — `-12 * -1` is positive but refused (product of two
/// negatives), `12 / 2 ** 8` is refused (exponent, not a literal pow2).
/// Never widen this without a corresponding negative test: widening is
/// the only way this pass can become unsound.
fn prove_non_neg(a: &ArithAst) -> Option<NonNeg> {
    match a {
        ArithAst::Num(n) if *n >= 0 => Some(NonNeg { _private: () }),
        ArithAst::Bin { op, lhs, rhs } if matches!(op.as_str(), "+" | "*") => {
            // Both operands must be proven independently: one NonNeg is
            // not evidence for the pair, and `*(-1, -12)` must not pass.
            let _l = prove_non_neg(lhs)?;
            let _r = prove_non_neg(rhs)?;
            Some(NonNeg { _private: () })
        }
        _ => None,
    }
}

/// divisor is an exact power of two (Num(2^n), n in 1..=30)
fn pow2(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(n) if *n > 1 => (*n & (*n - 1)) == 0 && *n < (1 << 31),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verdict for statement `i` of a one-line program.
    fn fires(src: &str) -> bool {
        let cmds = crate::parser::commands::parse_commands_from_text(src).expect("parse");
        let mut prog = crate::shir::ast_to_ir_raw(&cmds);
        transform(&mut prog.stmts);
        shiftable("", 0)
    }

    /// The evidence type cannot be forged: a NonNeg is only obtainable
    /// through the proof, and the proof refuses every shape whose sign is
    /// not literal-known.
    #[test]
    fn nonneg_evidence_refuses_unproven_shapes() {
        assert!(prove_non_neg(&ArithAst::Num(0)).is_some());
        assert!(prove_non_neg(&ArithAst::Num(12)).is_some());
        assert!(prove_non_neg(&ArithAst::Num(-1)).is_none(), "negative literal");
        // a var read: the sign is a runtime property, unprovable here
        assert!(prove_non_neg(&ArithAst::Var("a".to_string())).is_none());
    }

    /// FIRING half — the shapes the pass exists for.
    #[test]
    fn fires_on_nonneg_dividend_by_pow2() {
        assert!(fires("x=$(( 12 / 4 ))"), "literal / pow2");
        assert!(fires("x=$(( 200 / 128 ))"), "texture-math shape");
        assert!(fires("x=$(( 12 % 8 ))"), "modulo arm");
        assert!(fires("x=$(( 2 + 4 / 8 ))"), "compound non-negative lhs");
    }

    /// REFUSING half — each case flips exactly ONE guarded premise from a
    /// firing case above. This is the half that pins the invariant: a
    /// positive test says nothing about the guard.
    #[test]
    fn refuses_every_flipped_guard() {
        // the `-3/2` case: NEGATIVE dividend (bash: -3/2 = -1; -3>>1 = -2).
        // Note the parse: the shell frontend lowers `-12` as
        // `Un{op:"-", arg:Num(12)}`, NOT as the literal `Num(-12)` — so the
        // end-to-end refusal here is produced by the `Un` arm of the proof,
        // not by the sign test. The sign test is pinned separately in
        // `nonneg_evidence_refuses_unproven_shapes`; keeping BOTH means a
        // mutation of either one is caught.
        assert!(!fires("x=$(( -12 / 4 ))"), "negative dividend (Un spelling)");
        assert!(!fires("x=$(( -12 * -1 / 4 ))"), "negative factors");
        // a genuinely negative LITERAL in the ArithAst (what the frontend
        // produces for a constant-folded negative, and what a direct IR
        // consumer may emit) — this one exercises the sign test itself
        assert!(prove_non_neg(&ArithAst::Num(-12)).is_none(), "negative literal");
        // sign unknown: a var read
        assert!(!fires("x=$(( a / 4 ))"), "var dividend, sign unprovable");
        // subtraction can go negative
        assert!(!fires("x=$(( (0-12) / 4 ))"), "subtraction dividend");
        // divisor not a power of two
        assert!(!fires("x=$(( 12 / 3 ))"), "non-pow2 divisor");
        // negative divisor: `pow2` requires n > 1, so -4 cannot slip in
        assert!(!fires("x=$(( 12 / -4 ))"), "negative divisor");
        // pow2 spelled as an exponent is not a literal Num
        assert!(!fires("x=$(( 12 / 2 ** 8 ))"), "exponent divisor");
    }
}
