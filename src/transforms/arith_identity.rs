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
//!     divide-by-zero behavior — folding would swallow the error);
//!   `x - x` → `0`                                     — ONLY when x is pure
//!     arithmetic with no `/`/`%` (no error, no side effect to duplicate).
//! `x / x` and anything touching a capture/call is left alone.
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest). Registered in
//! `transforms.rs` (DEBASHC_TRANSFORMS gated).

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
                } else if num(lhs) == Some(0) && arith_no_err(rhs) {
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
