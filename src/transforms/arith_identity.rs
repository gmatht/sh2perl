//! arith-identity — simplify arithmetic identities (`$((x+0))` → `x`,
//! `$((x*1))` → `x`, `$((x/1))` → `x`, `$((x%1))` → `0`, …) over `Arith`
//! trees. Distinct from the landed const-fold-arith (which folds LITERAL
//! subtrees): this folds identities on NON-constant operands, which
//! appears in generated math (`$((idx + 0))`, `$((v * 1))`, the `+ 0`
//! residue the estree path leaves).
//!
//! ## Need
//! Renderers emit `$((x+0))` / `$((x*1))` verbatim; each costs an op and
//! a temp. These identities are universally sound in shell arithmetic
//! (every operand coerces to a number), so folding them is safe in every
//! backend — and it feeds const-condition-elim and copy-propagation
//! (a folded `x*0` → `0` is a literal a later pass can use).
//!
//! ## Scope — the sound idioms
//! Only identities that NEVER alter error behavior or value:
//!   `x + 0` → `x`, `0 + x` → `x`, `x - 0` → `x`      (add/sub of 0)
//!   `x * 1` → `x`, `1 * x` → `x`,                     (mul of 1)
//!   `x / 1` → `x`, `x % 1` → `0`                      (/ or % of 1)
//!   `x * 0` → `0`, `0 * x` → `0`, `0 / x` → `0`       — ONLY when the
//!     OTHER side has no `/` or `%` (a hidden `(1/0)*0` must keep its
//!     divide-by-zero behavior — folding would swallow the error), AND
//!     for `0 / x` the divisor must additionally be a POSITIVE nonzero
//!     literal (`0 / 5` → `0`; a `0 / 0` divisor is itself divide-by-zero
//!     and a negative divisor flips `-0.0` to `+0.0` — both refused);
//!   `x - x` → `0`                                     — ONLY when x is pure
//!     arithmetic with no `/`/`%` (no error, no side effect to duplicate).
//! `x / x` and anything touching a capture/call is left alone.
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest) — CURRENTLY
//! UNREGISTERED (present as a module only; `all()` has no entry).
//!
//! 2026-09-09 registration review (estree lowering work): deliberately
//! NOT registering yet — two gaps need var-type info this layer lacks:
//!   1. (FIXED here) `0 / 0` folded to `0` — now requires a positive
//!      nonzero literal divisor; `0 / x-var` and `0 / negative` refuse.
//!   2. (OPEN, owner decision) float-domain rules (`x % 1 → 0` on 5.5,
//!      `x * 0 → 0` on negatives/Inf, `x + 0 → x` on -0.0) assume
//!      shell-int domain. Sound for shell-derived IR (bash ints have no
//!      -0/fractions/Inf); needs var-type gating for C/go/rust/py-float
//!      IR before general registration. The unconditionally-sound subset
//!      (`* 1`, `/ 1`, `0 / positive-literal`) is pinned by unit tests
//!      below for whoever registers.

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= rewrite_stmt(st);
    }
    changed
}

fn rewrite_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => rewrite_expr(expr),
        IrStmt::Expr(e) => rewrite_expr(e),
        IrStmt::Output { value, .. } => rewrite_expr(value),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = rewrite_expr(cond);
            for s in then.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= rewrite_expr(ec);
                for s in eb.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            for s in else_.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = rewrite_expr(cond);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = rewrite_expr(iter);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Block(v) | IrStmt::Subshell(v) | IrStmt::Background(v) => {
            let mut c = false;
            for s in v.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        _ => false,
    }
}

fn rewrite_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arith(a) => {
            let before = format!("{:?}", a);
            simplify(a);
            format!("{:?}", a) != before
        }
        IrExpr::BinOp { lhs, rhs, .. } => rewrite_expr(lhs) | rewrite_expr(rhs),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= rewrite_expr(a);
            }
            c
        }
        _ => false,
    }
}

/// In-place identity simplification of an Arith tree.
fn simplify(a: &mut ArithAst) {
    if let ArithAst::Bin { op, lhs, rhs } = a {
        simplify(lhs);
        simplify(rhs);
        let num = |x: &ArithAst| match x {
            ArithAst::Num(n) => Some(*n),
            _ => None,
        };
        match op.as_str() {
            "+" => {
                if num(rhs) == Some(0) {
                    *a = lhs.as_ref().clone();
                } else if num(lhs) == Some(0) {
                    *a = rhs.as_ref().clone();
                }
            }
            "-" => {
                if num(rhs) == Some(0) {
                    *a = lhs.as_ref().clone();
                } else if lhs == rhs && arith_no_err(lhs) {
                    *a = ArithAst::Num(0);
                }
            }
            "*" => {
                if num(rhs) == Some(1) {
                    *a = lhs.as_ref().clone();
                } else if num(lhs) == Some(1) {
                    *a = rhs.as_ref().clone();
                } else if num(rhs) == Some(0) && arith_no_err(lhs) {
                    *a = ArithAst::Num(0);
                } else if num(lhs) == Some(0) && arith_no_err(rhs) {
                    *a = ArithAst::Num(0);
                }
            }
            "/" => {
                if num(rhs) == Some(1) {
                    *a = lhs.as_ref().clone();
                } else if num(lhs) == Some(0)
                    && matches!(num(rhs), Some(n) if n > 0)
                    && arith_no_err(rhs)
                {
                    // `0 / positive-literal` → `0` (exact in every domain:
                    // `0 / positive` is `+0` from ints through IEEE).
                    // A zero divisor (`0 / 0`) is divide-by-zero (bash
                    // abort / NaN / SIGFPE — never foldable), and a
                    // negative one flips `-0.0` to `+0.0`; non-literals
                    // stay (unprovable without var types at this layer).
                    *a = ArithAst::Num(0);
                }
            }
            "%" => {
                if num(rhs) == Some(1) {
                    *a = ArithAst::Num(0);
                }
            }
            _ => {}
        }
    }
}

/// True when the tree cannot raise a divide-by-zero (no `/`/`%`).
fn arith_no_err(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) => true,
        ArithAst::Bin { op, lhs, rhs } => {
            !matches!(op.as_str(), "/" | "%") && arith_no_err(lhs) && arith_no_err(rhs)
        }
        ArithAst::Un { arg, .. } => arith_no_err(arg),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_no_err(test) && arith_no_err(then) && arith_no_err(else_)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ArithAst;

    fn num(n: i64) -> ArithAst {
        ArithAst::Num(n)
    }
    fn var(name: &str) -> ArithAst {
        ArithAst::Var(name.to_string())
    }
    fn bin(op: &str, lhs: ArithAst, rhs: ArithAst) -> ArithAst {
        ArithAst::Bin {
            op: op.to_string(),
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }
    fn simplified(mut a: ArithAst) -> ArithAst {
        simplify(&mut a);
        a
    }
    fn is_num(a: &ArithAst, n: i64) -> bool {
        matches!(a, ArithAst::Num(m) if *m == n)
    }

    #[test]
    fn zero_divided_by_zero_refuses() {
        // `0 / 0` is divide-by-zero in every domain (bash abort, NaN,
        // SIGFPE) — folding it to `0` would swallow the error.
        let out = simplified(bin("/", num(0), num(0)));
        assert!(
            matches!(out, ArithAst::Bin { .. }),
            "0/0 must stay unfolded: {out:?}"
        );
    }

    #[test]
    fn zero_divided_by_positive_literal_folds() {
        // `0 / 5` → `0`: exact from ints through IEEE (`+0` either way).
        let out = simplified(bin("/", num(0), num(5)));
        assert!(is_num(&out, 0), "0/5 folds to 0: {out:?}");
    }

    #[test]
    fn zero_divided_by_var_or_negative_refuses() {
        // Unprovable (var) or sign-flipping (negative divisor turns
        // `-0.0` into `+0.0`) — keep the division.
        let out = simplified(bin("/", num(0), var("x")));
        assert!(
            matches!(out, ArithAst::Bin { .. }),
            "0/x-var stays unfolded: {out:?}"
        );
        let out = simplified(bin("/", num(0), num(-5)));
        assert!(
            matches!(out, ArithAst::Bin { .. }),
            "0/-5 stays unfolded: {out:?}"
        );
    }

    #[test]
    fn unconditional_identities_fold() {
        // `* 1` and `/ 1` are exact in every domain (IEEE preserves -0,
        // Inf and NaN through both) — no divisor/value proof needed
        // beyond the literal itself.
        let out = simplified(bin("*", var("x"), num(1)));
        assert!(
            matches!(out, ArithAst::Var(_)),
            "x*1 folds to x: {out:?}"
        );
        let out = simplified(bin("/", var("x"), num(1)));
        assert!(
            matches!(out, ArithAst::Var(_)),
            "x/1 folds to x: {out:?}"
        );
    }
}
