//! merge-init-assignments — fold the emitter's conservative declaration
//! default followed by a first unconditional assignment into one init.
//!
//! The ESTree lower.js pass performs the equivalent rewrite after rendering:
//!
//!     let x = 0; x = 6;
//!
//! becomes:
//!
//!     let x = 6;
//!
//! Keeping this at A1 means every backend can remove the same dead default
//! and, more importantly, every backend sees the same first-definition fact.
//! This is deliberately narrower than general copy propagation: it does not
//! move an expression across a statement and does not fold calls, captures,
//! arithmetic ASTs, or assignments with indices.
//!
//! REFUSE > GUESS: only adjacent `Declare` + `Assign` pairs are touched.

use crate::ir::{IrExpr, IrStmt};

/// Apply the transform recursively to every statement list.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= recurse(st);
    }
    let mut i = 0;
    while i + 1 < stmts.len() {
        let (name, default) = match &stmts[i] {
            IrStmt::Declare {
                vars,
                init: Some(init),
                ..
            } if vars.len() == 1 && is_literal(init) => (vars[0].name.clone(), init.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        let rhs = match &stmts[i + 1] {
            IrStmt::Assign {
                targets,
                expr,
                asm: None,
            } if targets.len() == 1
                && targets[0].var == name
                && targets[0].indices.is_empty()
                && pure(expr)
                && !reads_var(expr, &name) =>
            {
                expr.clone()
            }
            _ => {
                i += 1;
                continue;
            }
        };
        // `default` is intentionally retained only as the declaration's
        // shape marker while matching; the real first value replaces it.
        let _ = default;
        if let IrStmt::Declare { init, .. } = &mut stmts[i] {
            *init = Some(rhs);
        }
        stmts.remove(i + 1);
        changed = true;
        // Re-check the same index: a later pass may have exposed another
        // adjacent declaration/assignment pair after this removal.
    }
    changed
}

fn is_literal(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Bool(_))
}

/// Backend-neutral purity subset. Variable reads are harmless here because
/// the two original statements are adjacent and the assignment remains at
/// the same evaluation point relative to all other statements.
fn pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_)
        | IrExpr::Str(_, _)
        | IrExpr::Var(_, _)
        | IrExpr::Ident(_)
        | IrExpr::Bool(_)
        | IrExpr::Json(_) => true,
        IrExpr::BinOp { lhs, rhs, .. } => pure(lhs) && pure(rhs),
        IrExpr::Ternary { cond, then, else_ } => pure(cond) && pure(then) && pure(else_),
        IrExpr::DefinedOr { expr, default } => pure(expr) && pure(default),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            crate::ir::InterpPart::Lit(_) => true,
            crate::ir::InterpPart::Expr(x) => pure(x),
        }),
        IrExpr::Array(items) => items.iter().all(|x| pure(x)),
        // Calls, captures, redirects, arithmetic evaluators, and raw host
        // expressions can observe status or throw, so keep the pair intact.
        _ => false,
    }
}

fn reads_var(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == name,
        IrExpr::Index { var, key } => var == name || reads_var(key, name),
        IrExpr::BinOp { lhs, rhs, .. } => reads_var(lhs, name) || reads_var(rhs, name),
        IrExpr::Ternary { cond, then, else_ } => {
            reads_var(cond, name) || reads_var(then, name) || reads_var(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => reads_var(expr, name) || reads_var(default, name),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Lit(_) => false,
            crate::ir::InterpPart::Expr(x) => reads_var(x, name),
        }),
        IrExpr::Array(items) => items.iter().any(|x| reads_var(x, name)),
        _ => false,
    }
}

fn recurse(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = recurse_expr(cond);
            c |= transform(then);
            for (cond, body) in elsifs {
                c |= recurse_expr(cond);
                c |= transform(body);
            }
            c |= transform(else_);
            c
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = transform(body) | transform(else_body) | transform(finally_body);
            for ex in excepts {
                if let Some(e) = &mut ex.match_expr {
                    c |= recurse_expr(e);
                }
                c |= transform(&mut ex.body);
            }
            c
        }
        IrStmt::For { iter, body, .. } => recurse_expr(iter) | transform(body),
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let mut c = transform(init) | recurse_expr(cond) | transform(step) | transform(body);
            c
        }
        IrStmt::While { cond, body } => recurse_expr(cond) | transform(body),
        IrStmt::DoWhile { body, cond, .. } => transform(body) | recurse_expr(cond),
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = recurse_expr(discriminant);
            for cl in clauses {
                c |= transform(&mut cl.body);
            }
            c
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut c = transform(inner);
            for r in redirects {
                c |= recurse_expr(&mut r.target);
            }
            c
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            let mut c = transform(body);
            for (_, b) in named_blocks {
                c |= transform(b);
            }
            c
        }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => transform(body),
        IrStmt::Select { clauses } => {
            let mut c = false;
            for cl in clauses {
                if let Some(e) = &mut cl.ch {
                    c |= recurse_expr(e);
                }
                if let Some(e) = &mut cl.value {
                    c |= recurse_expr(e);
                }
                c |= transform(&mut cl.body);
            }
            c
        }
        _ => false,
    }
}

fn recurse_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::BinOp { lhs, rhs, .. } => recurse_expr(lhs) | recurse_expr(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            recurse_expr(cond) | recurse_expr(then) | recurse_expr(else_)
        }
        IrExpr::DefinedOr { expr, default } => recurse_expr(expr) | recurse_expr(default),
        IrExpr::Index { key, .. } => recurse_expr(key),
        IrExpr::Call { args, .. } | IrExpr::Array(args) => {
            args.iter_mut().map(recurse_expr).any(|x| x)
        }
        IrExpr::Interpolate(parts) => parts
            .iter_mut()
            .map(|p| match p {
                crate::ir::InterpPart::Lit(_) => false,
                crate::ir::InterpPart::Expr(x) => recurse_expr(x),
            })
            .any(|x| x),
        IrExpr::Arrow(body) => transform(body),
        IrExpr::Lambda { body, .. } => transform(body),
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            let mut c = recurse_expr(iter) | recurse_expr(elem);
            if let Some(x) = cond {
                c |= recurse_expr(x);
            }
            c
        }
        IrExpr::Splice(x) => recurse_expr(x),
        _ => false,
    }
}
