//! ternary-desugar — lower the C frontend's `ternary(cond, a, b)` sh2.*
//! call into the backend-neutral `IrExpr::Ternary` node with a `test`
//! call condition.
//!
//! ## Need
//!
//! The C frontend lowers `c ? a : b` to `Call { func: "ternary", args:
//! [Str(test-string), a, b] }`. The ESTree renderer has a native arm for
//! that call (shir.rs `try_native_test` machinery), but every OTHER
//! backend (perl / python / sh / go / rust / java / zig / c / js) renders
//! an unknown call — perl emits `ternary(...)` (an undefined sub), sh
//! refuses ("word call not renderable"), so any C program using the
//! conditional operator fails on 9 of 10 backends.
//!
//! This transform rewrites the call into `IrExpr::Ternary { cond: Call {
//! func: "test", args: [Str(c)] }, then: a, else_: b }` — nodes every
//! renderer already lowers natively (perl's test-string grammar, python's
//! intVal compare, go's cond3, ...). The branch values are the
//! already-lowered A1 expressions, unchanged. NOT applied on the estree
//! path (its native arm is byte-pinned by the c-sh-go corpus; the
//! rewrite would only perturb it).
//!
//! ## Placement
//!
//! Run at the A1 ingress in the cli `--shir-in-{perl,sh,python,go,rust,
//! java,zig,c,js}` arms (drop-in file; wired next to strip_cfor /
//! restructure_goto_only). Semantics: bash `[ ... ]` status protocol per
//! segment is preserved because each renderer's existing `test`
//！ lowering implements it.

use crate::ir::{IrExpr, IrProgram, IrStmt};

pub fn transform_program(prog: &mut IrProgram) -> bool {
    let mut changed = false;
    for st in prog.stmts.iter_mut() {
        changed |= walk_stmt(st);
    }
    for sub in prog.subs.iter_mut() {
        for st in sub.body.iter_mut() {
            changed |= walk_stmt(st);
        }
    }
    changed
}

fn walk_stmt(st: &mut IrStmt) -> bool {
    let mut changed = false;
    match st {
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            changed |= walk_expr(cond);
            for b in then.iter_mut() { changed |= walk_stmt(b); }
            for (_, b) in elsifs.iter_mut() { for x in b.iter_mut() { changed |= walk_stmt(x); } }
            for b in else_.iter_mut() { changed |= walk_stmt(b); }
        }
        IrStmt::Assign { expr, .. } => changed |= walk_expr(expr),
        IrStmt::Expr(e) => changed |= walk_expr(e),
        IrStmt::Output { value, .. } => changed |= walk_expr(value),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => changed |= walk_expr(e),
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
            changed |= walk_expr(cond);
            for b in body.iter_mut() { changed |= walk_stmt(b); }
        }
        IrStmt::Block(body)
        | IrStmt::Function { body, .. }
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => {
            for b in body.iter_mut() { changed |= walk_stmt(b); }
        }
        _ => {}
    }
    changed
}

fn is_test_string(s: &str) -> bool {
    [" -eq ", " -ne ", " -gt ", " -ge ", " -lt ", " -le ", " = ", " != "]
        .iter()
        .any(|op| s.contains(op))
}

/// The single argument of a Call expr (for test/testArith/arith conds).
fn cond_args_clone(cond: &IrExpr) -> IrExpr {
    match cond {
        IrExpr::Call { args, .. } => args.first().cloned().unwrap_or(IrExpr::Bool(false)),
        _ => IrExpr::Bool(false),
    }
}

fn walk_expr(e: &mut IrExpr) -> bool {
    let mut changed = false;
    match e {
        IrExpr::Call { func, args } => {
            // ternary/testArith checks BEFORE descending (a nested
            // testArith cond must be rewritten as part of the ternary,
            // not pre-rewritten to arith)
            if func == "ternary" {
                if let [cond, then, else_] = args.as_slice() {
                    let cond2: Option<IrExpr> = match cond {
                        IrExpr::Str(c, _) => {
                            if is_test_string(c) {
                                Some(IrExpr::Call { func: "test".to_string(), args: vec![cond.clone()] })
                            } else {
                                Some(IrExpr::Call { func: "arith".to_string(), args: vec![cond.clone()] })
                            }
                        }
                        IrExpr::Call { func: cf, .. }
                            if cf == "test" || cf == "arith" || cf == "testArith" =>
                        {
                            if cf == "testArith" {
                                // normalize to the universally-rendered arith
                                Some(IrExpr::Call { func: "arith".to_string(), args: vec![cond_args_clone(cond)] })
                            } else {
                                Some(cond.clone())
                            }
                        }
                        _ => None,
                    };
                    if let Some(cnd) = cond2 {
                        *e = IrExpr::Ternary {
                            cond: Box::new(cnd),
                            then: Box::new(then.clone()),
                            else_: Box::new(else_.clone()),
                        };
                        return true;
                    }
                }
            }
            if func == "testArith" {
                if let [IrExpr::Str(_, _)] = args.as_slice() {
                    let a = args[0].clone();
                    *e = IrExpr::Call { func: "arith".to_string(), args: vec![a] };
                    return true;
                }
            }
            for a in args.iter_mut() {
                changed |= walk_expr(a);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            changed |= walk_expr(lhs);
            changed |= walk_expr(rhs);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            changed |= walk_expr(cond);
            changed |= walk_expr(then);
            changed |= walk_expr(else_);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    changed |= walk_expr(x);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items.iter_mut() {
                changed |= walk_expr(i);
            }
        }
        IrExpr::Capture { expr, .. } => changed |= walk_expr(expr),
        IrExpr::Index { key, .. } => changed |= walk_expr(key),
        _ => {}
    }
    changed
}
