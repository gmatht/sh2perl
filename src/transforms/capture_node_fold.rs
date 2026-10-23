//! capture-node-fold — `Call { "capture", [Arrow] }` → the first-class
//! `IrExpr::Capture` node.
//!
//! ## Need
//! The A1 emits the first-class `Capture` node for a scalar command
//! substitution (`x=$(…)`), but several transform authors spelled the
//! same thing as the contract call `Call { func: "capture", args:
//! [Arrow(stmts)] }` (process_subst's mktemp helper, grep-o, …). Both
//! render — but through DIFFERENT arms, and the call spelling is the
//! weaker one:
//!
//!   * Perl (`ir_expr_to_perl`): the `Capture` arm is a strict superset —
//!     the native pipeline capture, the UU-FFI in-process capture, the
//!     native `echo | tr` fold and the fd-1 redirect buffer all fire
//!     there; the `Call` arm falls straight through to the rebuilt
//!     `bash -c` shell-out.
//!   * sh: the `Capture` arm's own comment records the equivalence
//!     ("same `"$(…)"` rendering as the `capture` call arm") plus a
//!     POSIX idiom lift on top.
//!   * ESTree: `expr_to_estree` converts `Capture` BACK to the call
//!     form, so the rendered tree is byte-identical either way.
//!
//! Every other backend carries both arms (c `capture_expr`, go
//! `capture_expr(expr, native)`, rust `capture_expr_single`, …). The
//! rewrite therefore lowers the sh2.* call-site tally (the `Metric`,
//! PLAN §9.2) without changing any renderer's contract — and makes the
//! Perl render pick its native, shell-out-free paths.
//!
//! ## Scope — the sound rewrite
//!   * `Call { "capture", [Arrow(stmts)] }`
//!       → `Capture { expr: Arrow(stmts), native: false }`
//!
//! REFUSE (each keeps the status-quo rendering):
//!   * `captureWords` — the WORD-position spelling; its render
//!     word-splits on IFS and `Capture` does not (`basename $(pwd)`
//!     needs the split), so folding it would glue words together.
//!   * a payload that is not an `Arrow` (some frontends spell the
//!     operand as text) — REFUSE > GUESS.
//!   * a body that reads `$?` (`getVar("?")`) — a couple of render
//!     arms still key on the call spelling for status save/restore;
//!     re-proving `$?` equivalence across every backend is not worth
//!     it for this fold.
//!
//! ## Placement
//! Registered in `transforms.rs` (`capture-node-fold`, the shared
//! `ast_to_ir` channel; codegen targets only — an unset target or
//! `shir` is the byte-pinned embed/A1-export path, `case-lowering`
//! precedent) AND called at the end of otranspilerl's `ingest`
//! (the `else-if-flatten` precedent): `process_subst` runs INSIDE
//! ingest and builds the call form after the registry already ran.

use crate::ir::{IrExpr, IrProgram, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    stmts_list(stmts)
}

#[allow(dead_code)]
pub fn run_program(prog: &mut IrProgram) -> bool {
    let mut c = stmts_list(&mut prog.stmts);
    for sub in &mut prog.subs {
        c |= stmts_list(&mut sub.body);
    }
    c
}

fn stmts_list(stmts: &mut [IrStmt]) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    c
}

fn stmt(st: &mut IrStmt) -> bool {
    let mut c = false;
    match st {
        IrStmt::Output { value, .. } => c |= expr(value),
        IrStmt::WriteFile { path, content, .. } => {
            c |= expr(path);
            c |= expr(content);
        }
        IrStmt::Assign { targets, expr: e, .. } => {
            c |= expr(e);
            for t in targets {
                for i in &mut t.indices {
                    c |= expr(i);
                }
            }
        }
        IrStmt::Declare { init: Some(e), .. } => c |= expr(e),
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                c |= expr(e);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            c |= expr(cond);
            c |= stmts_list(then);
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr(ec);
                c |= stmts_list(eb);
            }
            c |= stmts_list(else_);
        }
        IrStmt::For { iter, body, .. } => {
            c |= expr(iter);
            c |= stmts_list(body);
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            c |= stmts_list(init);
            c |= expr(cond);
            c |= stmts_list(step);
            c |= stmts_list(body);
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            c |= expr(cond);
            c |= stmts_list(body);
        }
        IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => c |= expr(e),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
            c |= expr(e)
        }
        IrStmt::Expr(e) => c |= expr(e),
        IrStmt::Function { body, .. } | IrStmt::Block(body) => c |= stmts_list(body),
        IrStmt::Redirect { inner, .. } => c |= stmts_list(inner),
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            c |= expr(cmd);
            for a in args.iter_mut().chain(redirects.iter_mut()) {
                c |= expr(a);
            }
            for (_, v) in env.iter_mut() {
                c |= expr(v);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages.iter_mut() {
                c |= stmts_list(s);
            }
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            c |= expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= stmts_list(&mut cl.body);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            c |= stmts_list(body);
            for ex in excepts.iter_mut() {
                c |= stmts_list(&mut ex.body);
            }
            c |= stmts_list(else_body);
            c |= stmts_list(finally_body);
        }
        // Select / Ext / Asm / Label / Goto / RawText / Shaderize and
        // friends: no fold sites reached in them today — an unfolded
        // node is the status quo, never a wrong rewrite.
        _ => {}
    }
    c
}

fn expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr(a);
            }
            if func == "capture" && args.len() == 1 {
                if let IrExpr::Arrow(body) = &args[0] {
                    if !reads_exit_status(body) {
                        *e = IrExpr::Capture {
                            expr: Box::new(args[0].clone()),
                            native: false,
                        };
                        return true;
                    }
                }
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = expr(obj);
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
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
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            let mut c = expr(iter);
            c |= expr(elem);
            if let Some(cond) = cond {
                c |= expr(cond);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            let mut c = expr(lhs);
            c |= expr(rhs);
            c
        }
        IrExpr::Index { key, .. } => expr(key),
        IrExpr::Ternary { cond, then, else_ } => {
            let mut c = expr(cond);
            c |= expr(then);
            c |= expr(else_);
            c
        }
        IrExpr::DefinedOr { expr: x, default } => {
            let mut c = expr(x);
            c |= expr(default);
            c
        }
        // Already the first-class node: recurse only (idempotent).
        IrExpr::Capture { expr: x, .. } => expr(x),
        IrExpr::Arrow(body) | IrExpr::Lambda { body, .. } => stmts_list(body),
        // Assign { expr } for C-frontend asm operands, Splice, Range,
        // Object, Str/Var/Int/…: no call sites to fold.
        _ => false,
    }
}

/// Does the capture body read `$?`? The refusal predicate — see module
/// docs. Walks statements and expressions; unknown statement variants
/// with nested expressions are treated as NOT reading (the fold is the
/// unsafe direction only when it would erase a status read — an
/// unknown shape with a status read simply keeps the call spelling
/// today, and every unknown-shape site we keep is a plain non-read).
fn reads_exit_status(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(stmt_reads_exit_status)
}

fn stmt_reads_exit_status(st: &IrStmt) -> bool {
    match st {
        IrStmt::Output { value, .. } => expr_reads_exit_status(value),
        IrStmt::WriteFile { path, content, .. } => {
            expr_reads_exit_status(path) || expr_reads_exit_status(content)
        }
        IrStmt::Assign { targets, expr, .. } => {
            if expr_reads_exit_status(expr) {
                return true;
            }
            targets.iter().any(|t| t.indices.iter().any(expr_reads_exit_status))
        }
        IrStmt::Declare { init: Some(e), .. } => expr_reads_exit_status(e),
        IrStmt::DeclareArray { elements, .. } => {
            elements.iter().any(expr_reads_exit_status)
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_reads_exit_status(cond)
                || reads_exit_status(then)
                || elsifs
                    .iter()
                    .any(|(c, b)| expr_reads_exit_status(c) || reads_exit_status(b))
                || reads_exit_status(else_)
        }
        IrStmt::For { iter, body, .. } => {
            expr_reads_exit_status(iter) || reads_exit_status(body)
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            reads_exit_status(init)
                || expr_reads_exit_status(cond)
                || reads_exit_status(step)
                || reads_exit_status(body)
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            expr_reads_exit_status(cond) || reads_exit_status(body)
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_reads_exit_status(expr),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
            expr_reads_exit_status(e)
        }
        IrStmt::Expr(e) => expr_reads_exit_status(e),
        IrStmt::Function { body, .. } | IrStmt::Block(body) => reads_exit_status(body),
        IrStmt::Redirect { inner, .. } => reads_exit_status(inner),
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            expr_reads_exit_status(cmd)
                || args.iter().chain(redirects.iter()).any(expr_reads_exit_status)
                || env.iter().any(|(_, v)| expr_reads_exit_status(v))
        }
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| reads_exit_status(s)),
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            expr_reads_exit_status(discriminant)
                || clauses.iter().any(|cl| reads_exit_status(&cl.body))
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            reads_exit_status(body)
                || excepts.iter().any(|ex| reads_exit_status(&ex.body))
                || reads_exit_status(else_body)
                || reads_exit_status(finally_body)
        }
        _ => false,
    }
}

fn expr_reads_exit_status(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            if func == "getVar"
                && matches!(args.first(), Some(IrExpr::Str(v, _)) if v == "?")
            {
                return true;
            }
            args.iter().any(expr_reads_exit_status)
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads_exit_status(obj) || args.iter().any(expr_reads_exit_status)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Expr(x) => expr_reads_exit_status(x),
            _ => false,
        }),
        IrExpr::Array(items) => items.iter().any(expr_reads_exit_status),
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            expr_reads_exit_status(iter)
                || expr_reads_exit_status(elem)
                || cond.as_deref().is_some_and(expr_reads_exit_status)
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_reads_exit_status(lhs) || expr_reads_exit_status(rhs)
        }
        IrExpr::Index { key, .. } => expr_reads_exit_status(key),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads_exit_status(cond)
                || expr_reads_exit_status(then)
                || expr_reads_exit_status(else_)
        }
        IrExpr::DefinedOr { expr: x, default } => {
            expr_reads_exit_status(x) || expr_reads_exit_status(default)
        }
        IrExpr::Capture { expr: x, .. } | IrExpr::Splice(x) => expr_reads_exit_status(x),
        IrExpr::Arrow(body) | IrExpr::Lambda { body, .. } => reads_exit_status(body),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn call_capture(body: Vec<IrStmt>) -> IrExpr {
        IrExpr::Call {
            func: "capture".to_string(),
            args: vec![IrExpr::Arrow(body)],
        }
    }

    fn echo_exec() -> IrStmt {
        IrStmt::Exec {
            cmd: IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
            args: vec![IrExpr::Str("hi".to_string(), StrStyle::DoubleQuoted)],
            capture: None,
            redirects: vec![],
            env: vec![],
        }
    }

    fn assign(expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "x".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    #[test]
    fn folds_the_call_form_to_the_node() {
        let mut stmts = vec![assign(call_capture(vec![echo_exec()]))];
        assert!(transform(&mut stmts));
        let IrStmt::Assign { expr: e, .. } = &stmts[0] else {
            panic!("assign expected");
        };
        assert!(
            matches!(e, IrExpr::Capture { native: false, .. }),
            "got {e:?}"
        );
    }

    #[test]
    fn is_idempotent_on_the_node_form() {
        let mut stmts = vec![assign(IrExpr::Capture {
            expr: Box::new(IrExpr::Arrow(vec![echo_exec()])),
            native: false,
        })];
        assert!(!transform(&mut stmts), "already the node — no change");
    }

    #[test]
    fn refuses_capture_words_the_word_split_spelling() {
        let mut stmts = vec![assign(IrExpr::Call {
            func: "captureWords".to_string(),
            args: vec![IrExpr::Arrow(vec![echo_exec()])],
        })];
        assert!(!transform(&mut stmts));
        assert!(matches!(&stmts[0], IrStmt::Assign { expr: IrExpr::Call { func, .. }, .. } if func == "captureWords"));
    }

    #[test]
    fn refuses_a_body_that_reads_status() {
        let status_read = IrStmt::Expr(IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![IrExpr::Str("?".to_string(), StrStyle::DoubleQuoted)],
        });
        let mut stmts = vec![assign(call_capture(vec![echo_exec(), status_read]))];
        assert!(!transform(&mut stmts));
        assert!(matches!(&stmts[0], IrStmt::Assign { expr: IrExpr::Call { func, .. }, .. } if func == "capture"));
    }

    #[test]
    fn refuses_a_non_arrow_payload() {
        let mut stmts = vec![assign(IrExpr::Call {
            func: "capture".to_string(),
            args: vec![IrExpr::Str("echo hi".to_string(), StrStyle::DoubleQuoted)],
        })];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn folds_nested_inside_an_interpolated_word() {
        let word = IrExpr::Interpolate(vec![
            crate::ir::InterpPart::Lit("v=".to_string()),
            crate::ir::InterpPart::Expr(Box::new(call_capture(vec![echo_exec()]))),
        ]);
        let mut stmts = vec![IrStmt::Expr(word)];
        assert!(transform(&mut stmts));
        let IrStmt::Expr(IrExpr::Interpolate(parts)) = &stmts[0] else {
            panic!("interpolate expected");
        };
        assert!(matches!(
            &parts[1],
            crate::ir::InterpPart::Expr(x) if matches!(**x, IrExpr::Capture { .. })
        ));
    }
}
