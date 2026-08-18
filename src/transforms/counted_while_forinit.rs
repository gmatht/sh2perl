//! counted-while-forinit — recover the language-neutral C-style loop
//! already present in the A1 contract from the shell frontend's canonical
//! counter-while shape.
//!
//!     i=INIT
//!     while [ CONDITION(i) ]; do
//!         BODY
//!         i=i+STEP
//!     done
//!
//! becomes one `IrStmt::ForInit { init, cond, step, body }`. This is the
//! shIR counterpart of lower.js::nativeForLoops: the transformation does
//! not emit JavaScript and does not assume `sh2.*` runtime wrappers. Each
//! backend can render ForInit as its native counted loop or reject it.
//!
//! The implementation is deliberately strict. It only recognizes a plain
//! scalar assignment immediately before the loop and a final `i = i +/- K`
//! assignment. Any control escape, nested write, or `$?` observation causes
//! the original while form to remain intact.

use crate::ir::{BinOpKind, IrExpr, IrStmt};

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= recurse(st);
    }

    let mut i = 1;
    while i < stmts.len() {
        let init = match &stmts[i - 1] {
            IrStmt::Assign {
                targets, asm: None, ..
            } if targets.len() == 1 && targets[0].indices.is_empty() => targets[0].var.clone(),
            _ => {
                i += 1;
                continue;
            }
        };
        let (cond, body, step) = match &stmts[i] {
            IrStmt::While { cond, body } => {
                let Some(step) = body.last().and_then(|s| counter_step(s)) else {
                    i += 1;
                    continue;
                };
                (cond.clone(), body.clone(), step)
            }
            _ => {
                i += 1;
                continue;
            }
        };
        if step.0 != init
            || body[..body.len() - 1]
                .iter()
                .any(|s| writes_or_escapes(s, &init))
            || contains_status_read(&cond)
            || body[..body.len() - 1]
                .iter()
                .any(|s| contains_status_read_stmt(s))
        {
            i += 1;
            continue;
        }
        let body = body[..body.len() - 1].to_vec();
        let step_stmt = step.1;
        let init_stmt = stmts.remove(i - 1);
        // The while moved one slot left when init was removed.
        stmts[i - 1] = IrStmt::ForInit {
            init: vec![init_stmt],
            cond,
            step: vec![step_stmt],
            body,
        };
        changed = true;
        i = i.saturating_sub(1);
    }
    changed
}

/// Return (counter name, update statement) for `v = v +/- literal` or
/// `v +=/-= literal`. The literal gate keeps the loop's induction step
/// explicit and avoids changing dynamic shell arithmetic semantics.
fn counter_step(st: &IrStmt) -> Option<(String, IrStmt)> {
    let IrStmt::Assign {
        targets,
        expr,
        asm: None,
    } = st
    else {
        return None;
    };
    if targets.len() != 1 || !targets[0].indices.is_empty() {
        return None;
    }
    let name = targets[0].var.clone();
    let ok = match expr {
        IrExpr::BinOp {
            lhs,
            op: BinOpKind::Add | BinOpKind::Sub,
            rhs,
        } if matches!(lhs.as_ref(), IrExpr::Var(v, _) if v == &name)
            && matches!(rhs.as_ref(), IrExpr::Int(n) if *n != 0) =>
        {
            true
        }
        _ => false,
    };
    if ok {
        Some((name, st.clone()))
    } else {
        None
    }
}

fn writes_or_escapes(st: &IrStmt, name: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => targets.iter().any(|t| t.var == name),
        IrStmt::Declare { vars, .. } => vars.iter().any(|v| v.name == name),
        IrStmt::DeclareArray { var, .. } => var == name,
        IrStmt::Break
        | IrStmt::Continue
        | IrStmt::Return(_)
        | IrStmt::Exit(_)
        | IrStmt::Goto(_)
        | IrStmt::Label(_) => true,
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            then.iter().any(|s| writes_or_escapes(s, name))
                || elsifs
                    .iter()
                    .any(|(_, b)| b.iter().any(|s| writes_or_escapes(s, name)))
                || else_.iter().any(|s| writes_or_escapes(s, name))
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. } => body.iter().any(|s| writes_or_escapes(s, name)),
        IrStmt::ForInit {
            init, step, body, ..
        } => init
            .iter()
            .chain(step)
            .chain(body)
            .any(|s| writes_or_escapes(s, name)),
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            body.iter().any(|s| writes_or_escapes(s, name))
                || named_blocks
                    .iter()
                    .any(|(_, b)| b.iter().any(|s| writes_or_escapes(s, name)))
        }
        IrStmt::Try {
            body,
            else_body,
            finally_body,
            excepts,
        } => {
            body.iter()
                .chain(else_body)
                .chain(finally_body)
                .any(|s| writes_or_escapes(s, name))
                || excepts
                    .iter()
                    .any(|e| e.body.iter().any(|s| writes_or_escapes(s, name)))
        }
        IrStmt::Case { clauses, .. } => clauses
            .iter()
            .any(|c| c.body.iter().any(|s| writes_or_escapes(s, name))),
        IrStmt::Select { clauses } => clauses
            .iter()
            .any(|c| c.body.iter().any(|s| writes_or_escapes(s, name))),
        _ => false,
    }
}

fn expr_reads(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == name,
        IrExpr::Index { var, key } => var == name || expr_reads(key, name),
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads(lhs, name) || expr_reads(rhs, name),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads(cond, name) || expr_reads(then, name) || expr_reads(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => expr_reads(expr, name) || expr_reads(default, name),
        IrExpr::Call { args, .. } | IrExpr::Array(args) => args.iter().any(|e| expr_reads(e, name)),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Lit(s) => s.contains("$?") && name == "?",
            crate::ir::InterpPart::Expr(e) => expr_reads(e, name),
        }),
        IrExpr::Arrow(body) => body.iter().any(|s| writes_or_escapes(s, name)),
        IrExpr::Lambda { body, .. } => body.iter().any(|s| writes_or_escapes(s, name)),
        _ => false,
    }
}

fn contains_status_read(e: &IrExpr) -> bool {
    expr_reads(e, "?")
}

fn contains_status_read_stmt(st: &IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. }
        | IrStmt::Expr(expr)
        | IrStmt::SetChildError(expr)
        | IrStmt::Return(Some(expr))
        | IrStmt::Exit(Some(expr))
        | IrStmt::Die { expr, .. }
        | IrStmt::Warn { expr, .. } => contains_status_read(expr),
        IrStmt::Declare { init, .. } => init.as_ref().is_some_and(contains_status_read),
        IrStmt::DeclareArray { elements, .. } => elements.iter().any(contains_status_read),
        IrStmt::Output { value, .. } => contains_status_read(value),
        IrStmt::WriteFile { path, content, .. } => {
            contains_status_read(path) || contains_status_read(content)
        }
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            contains_status_read(cmd)
                || args.iter().any(contains_status_read)
                || redirects.iter().any(contains_status_read)
                || env.iter().any(|(_, e)| contains_status_read(e))
        }
        IrStmt::Pipeline { stages, .. } => stages
            .iter()
            .flat_map(|stage| stage.iter())
            .any(contains_status_read_stmt),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            contains_status_read(cond)
                || then.iter().any(contains_status_read_stmt)
                || elsifs.iter().any(|(c, b)| {
                    contains_status_read(c) || b.iter().any(contains_status_read_stmt)
                })
                || else_.iter().any(contains_status_read_stmt)
        }
        IrStmt::For { iter, body, .. } => {
            contains_status_read(iter) || body.iter().any(contains_status_read_stmt)
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            init.iter().any(contains_status_read_stmt)
                || contains_status_read(cond)
                || step.iter().any(contains_status_read_stmt)
                || body.iter().any(contains_status_read_stmt)
        }
        IrStmt::While { cond, body } => {
            contains_status_read(cond) || body.iter().any(contains_status_read_stmt)
        }
        IrStmt::DoWhile { body, cond, .. } => {
            body.iter().any(contains_status_read_stmt) || contains_status_read(cond)
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            contains_status_read(discriminant)
                || clauses
                    .iter()
                    .any(|c| c.body.iter().any(contains_status_read_stmt))
        }
        IrStmt::Redirect { inner, redirects } => {
            inner.iter().any(contains_status_read_stmt)
                || redirects.iter().any(|r| contains_status_read(&r.target))
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            body.iter().any(contains_status_read_stmt)
                || named_blocks
                    .iter()
                    .any(|(_, b)| b.iter().any(contains_status_read_stmt))
        }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            body.iter().any(contains_status_read_stmt)
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            body.iter().any(contains_status_read_stmt)
                || excepts
                    .iter()
                    .any(|e| e.body.iter().any(contains_status_read_stmt))
                || else_body.iter().any(contains_status_read_stmt)
                || finally_body.iter().any(contains_status_read_stmt)
        }
        IrStmt::Select { clauses } => clauses
            .iter()
            .any(|c| c.body.iter().any(contains_status_read_stmt)),
        IrStmt::Asm {
            inputs, outputs, ..
        } => {
            inputs.iter().any(|(_, e)| contains_status_read(e))
                || outputs.iter().any(|(_, e)| contains_status_read(e))
        }
        _ => false,
    }
}

fn recurse(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            let mut c = transform(then) | transform(else_);
            for (_, b) in elsifs {
                c |= transform(b);
            }
            c
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. } => transform(body),
        IrStmt::ForInit {
            init, step, body, ..
        } => transform(init) | transform(step) | transform(body),
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            let mut c = transform(body);
            for (_, b) in named_blocks {
                c |= transform(b);
            }
            c
        }
        IrStmt::Try {
            body,
            else_body,
            finally_body,
            excepts,
        } => {
            let mut c = transform(body) | transform(else_body) | transform(finally_body);
            for e in excepts {
                c |= transform(&mut e.body);
            }
            c
        }
        IrStmt::Case { clauses, .. } => clauses
            .iter_mut()
            .map(|c| transform(&mut c.body))
            .any(|x| x),
        IrStmt::Select { clauses } => clauses
            .iter_mut()
            .map(|c| transform(&mut c.body))
            .any(|x| x),
        _ => false,
    }
}
