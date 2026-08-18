//! const-capture-fold — fold a command substitution that runs only a
//! constant `echo` into the string literal (`v=$(echo foo)` → `v="foo"`),
//! the constant half of capture lowering (direct-calls handles
//! function-of-constant; this handles the plain literal echo).
//!
//! ## Need
//! Scripts and generated code capture constants via `$(echo …)`; every
//! backend spawns a process (or a string-parse) for a value known at IR
//! time. Folding it to a `Str` literal removes the spawn in all backends.
//!
//! ## Scope — the sound rule
//! A `Capture` folds to a `Str` only when its arrow body is EXACTLY ONE
//! `Output` of a literal `Str` (no interpolation, no fd redirect, no
//! `target`), and the result strips the trailing newlines the way command
//! substitution does:
//!   - `Output { value: Str(s), newline: true, target: None }`
//!     → `Str(s)` with ALL trailing `\n` removed;
//!   - `Output { value: Str(s), newline: false, … }` → `Str(s)`.
//! Any capture that embeds a variable, a non-`echo` command, multiple
//! statements, or a redirect is left untouched (refuse > guess).
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest). Registered in
//! `transforms.rs` (DEBASHC_TRANSFORMS gated). Complements direct-calls.

use crate::ir::{ArithAst, InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt_pass(st);
    }
    c
}

fn stmt_pass(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_pass(expr),
        IrStmt::Expr(e) => expr_pass(e),
        IrStmt::Output { value, .. } => expr_pass(value),
        IrStmt::Declare { init, .. } => init.as_mut().map(expr_pass).unwrap_or(false),
        IrStmt::WriteFile { path, content, .. } => {
            expr_pass(path) | expr_pass(content)
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = expr_pass(cond);
            for s in then.iter_mut() {
                c |= stmt_pass(s);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr_pass(ec);
                for s in eb.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            for s in else_.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, var: _ } => {
            let mut c = expr_pass(cond);
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        _ => false,
    }
}

fn expr_pass(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Capture { expr, native } => {
            if *native {
                return false; // already a native value — leave it
            }
            let mut changed = false;
            if let Some(s) = fold_const_echo(expr) {
                *e = IrExpr::Str(s, crate::ir::StrStyle::DoubleQuoted);
                changed = true;
            } else {
                changed = expr_pass(expr);
            }
            changed
        }
        IrExpr::BinOp { lhs, rhs, .. } => expr_pass(lhs) | expr_pass(rhs),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr_pass(a);
            }
            c
        }
        IrExpr::Index { key, .. } => expr_pass(key),
        IrExpr::Arith(a) => arith_pass(a),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= expr_pass(x);
                }
            }
            c
        }
        IrExpr::Arrow(body) => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        _ => false,
    }
}

fn arith_pass(a: &mut ArithAst) -> bool {
    match a {
        ArithAst::Un { arg, .. } => arith_pass(arg),
        ArithAst::Bin { lhs, rhs, .. } => arith_pass(lhs) | arith_pass(rhs),
        _ => false,
    }
}

/// If `expr` (the capture's inner Arrow) is exactly one literal-echo
/// Output, return the folded string (trailing newlines stripped); else
/// None.
fn fold_const_echo(expr: &IrExpr) -> Option<String> {
    let body = match expr {
        IrExpr::Arrow(body) => body,
        _ => return None,
    };
    let output = match body.as_slice() {
        [IrStmt::Output { value, newline, target: None }] => (value, newline),
        _ => return None,
    };
    let s = match output.0 {
        IrExpr::Str(s, _) => s,
        _ => return None, // not a literal — maybe an Interpolate of consts? refuse
    };
    Some(strip_all_trailing_newlines(s))
}

fn strip_all_trailing_newlines(s: &str) -> String {
    s.trim_end_matches('\n').to_string()
}
