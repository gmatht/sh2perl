//! arith-normalize — rewrite arithmetic into the single canonical form
//! every backend would otherwise special-case.
//!
//! ## Need
//! Frontends lower the same source construct in different shapes. The
//! py-sh-go frontend emits unary minus as `Bin{ "-", Num(0), X }`, so
//! `y = -x` rendered as `let y: i64 = 0 - x;` in Rust (and `0 - x` in
//! every other backend) instead of the source's `-x`. A renderer could
//! peephole it, but then EVERY backend needs the same peephole — the
//! shape is a program property, so it belongs in one shared pass.
//!
//! ## Scope — the sound rewrites
//!   * `0 - X` → `Un{ "-", X }`                  (negation)
//!   * `0 + X` / `X + 0` → `X`                    (no-op; the fold is
//!     value-preserving in every numeric domain, and `-0.0` cannot arise
//!     from shell/py integer sources)
//!   * `X * 1` / `1 * X` → `X`
//! Refuse-on-doubt: `0 / X`, `0 % X` and `0 * X` are NOT folded (a zero
//! divisor is an error the shell reports; multiplying by zero still
//! forces evaluation of X's errors), and nothing with a `/`/`%`/`**`
//! subtree is touched, so no error behaviour can move.
//!
//! ## Placement
//! Registered in `transforms.rs` and run by the frontend ingress (it is
//! a purely local rewrite of an already-lowered node).

use crate::ir::{ArithAst, InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut [IrStmt]) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    c
}

fn stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr: rhs, .. } => expr(rhs),
        IrStmt::Declare { init: Some(e), .. } => expr(e),
        IrStmt::Expr(e) => expr(e),
        IrStmt::Output { value, .. } => expr(value),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => expr(e),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = expr(cond);
            c |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr(ec);
                c |= transform(eb);
            }
            c |= transform(else_);
            c
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            expr(cond) | transform(body)
        }
        IrStmt::For { iter, body, .. } => expr(iter) | transform(body),
        IrStmt::ForInit { init, cond, step, body } => {
            let mut c = transform(init);
            c |= expr(cond);
            c |= transform(step);
            c |= transform(body);
            c
        }
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = transform(body);
            for (_, b) in named_blocks.iter_mut() {
                c |= transform(b);
            }
            c
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => transform(b),
        IrStmt::Redirect { inner, .. } => transform(inner),
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for s in stages.iter_mut() {
                c |= transform(s);
            }
            c
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut c = transform(body);
            for e in excepts.iter_mut() {
                c |= transform(&mut e.body);
            }
            c |= transform(else_body);
            c |= transform(finally_body);
            c
        }
        IrStmt::Case { discriminant, clauses } => {
            let mut c = expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= transform(&mut cl.body);
            }
            c
        }
        IrStmt::WriteFile { path, content, .. } => expr(path) | expr(content),
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for e in elements.iter_mut() {
                c |= expr(e);
            }
            c
        }
        _ => false,
    }
}

fn expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arith(a) => arith(a),
        IrExpr::Index { key, .. } => expr(key),
        IrExpr::BinOp { lhs, rhs, .. } => expr(lhs) | expr(rhs),
        IrExpr::Ternary { cond, then, else_ } => expr(cond) | expr(then) | expr(else_),
        IrExpr::DefinedOr { expr: x, default } => expr(x) | expr(default),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= expr(x);
                }
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for i in items.iter_mut() {
                c |= expr(i);
            }
            c
        }
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::Capture { expr: x, .. } => expr(x),
        IrExpr::Arrow(body) => transform(body),
        _ => false,
    }
}

/// Rewrite one arith tree (bottom-up, so nested shapes normalise too).
fn arith(a: &mut ArithAst) -> bool {
    // children first
    let mut c = match a {
        ArithAst::Bin { lhs, rhs, .. } => arith(lhs) | arith(rhs),
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => arith(arg),
        ArithAst::Index { key, .. } => arith(key),
        ArithAst::Cond { test, then, else_ } => arith(test) | arith(then) | arith(else_),
        ArithAst::Assign { rhs, .. } => arith(rhs),
        _ => false,
    };
    // then this node
    let replacement = match a {
        ArithAst::Bin { op, lhs, rhs } => {
            let lz = matches!(&**lhs, ArithAst::Num(0));
            let rz = matches!(&**rhs, ArithAst::Num(0));
            let lo = matches!(&**lhs, ArithAst::Num(1));
            let ro = matches!(&**rhs, ArithAst::Num(1));
            match op.as_str() {
                // `0 - X` is unary minus
                "-" if lz => Some(ArithAst::Un {
                    op: "-".to_string(),
                    arg: rhs.clone(),
                }),
                // additive/multiplicative identities (never `/`, `%`,
                // `**`, or `0 * X` — see the module doc)
                "+" if lz => Some((**rhs).clone()),
                "+" if rz => Some((**lhs).clone()),
                "*" if lo => Some((**rhs).clone()),
                "*" if ro => Some((**lhs).clone()),
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(r) = replacement {
        *a = r;
        c = true;
    }
    c
}
