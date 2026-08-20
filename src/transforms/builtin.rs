//! exec-to-builtin: `exec("cmd", args)` → `builtin("cmd", args)` for every
//! command in the shared builtins namespace.
//!
//! Core request `core-requests/shir-builtin-op-20260816.md`: the
//! native-lowering decision moves OUT of the renderers INTO the shIR, so
//! the backends become pure renderers of native A1. Before this
//! transform, each backend re-decided what "native" meant for `ls`/`echo`
//! at render time (estree's JS_SYNC_BUILTINS table, perl/rust/c shelling
//! out). After it, the A1 itself carries `builtin("cmd", args)` and a
//! single shared check machine ("the emitted A1 must not contain exec of
//! a builtins.json command") replaces the per-backend greps.
//!
//! Self-fallback contract (PLAN.md §11): a backend that has NOT accepted
//! the op renders it as the exec it came from — the shared
//! `shir_passes::builtin::fallback_builtin_to_exec` pass erases `builtin`
//! back to `exec` at that renderer's entry. The estree renderer accepts
//! natively (its emit already lowers exec-of-builtin to `sh2.builtin`);
//! the other nine canonical renderers currently fall back (each may adopt
//! a native arm later and drop the fallback — acceptance).
//!
//! The namespace table is EMBEDDED here (the submodule is self-contained;
//! `harness/builtins.json` in the workspace is the same contract copy).

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// The shared builtins namespace — harness/builtins.json (69 entries).
pub const BUILTINS: &[&str] = &[
    ".", ":", "basename", "break", "cd", "cmp", "comm", "command", "continue", "cp", "cut",
    "date", "declare", "diff", "dirname", "echo", "egrep", "eval", "exit", "export", "false",
    "find", "grep", "gunzip", "gzip", "head", "hostname", "let", "local", "ls", "mapfile",
    "mkdir", "mv", "paste", "printf", "pwd", "read", "readarray", "readlink", "readonly",
    "return", "rm", "rmdir", "sed", "seq", "set", "sha256sum", "sha512sum", "shift", "sleep",
    "sort", "source", "stat", "tail", "tee", "touch", "tr", "trap", "true", "type", "typeset",
    "uname", "uniq", "unset", "wait", "wc", "which", "whoami", "xargs",
];

/// Is `cmd` in the shared builtins namespace?
pub fn is_builtin(cmd: &str) -> bool {
    BUILTINS.contains(&cmd)
}

/// The word-shaping gate: a candidate `exec` must be the PLAIN
/// `exec(Str(cmd), Array(words))` statement/expression shape — the word
/// list is already shaped. `exec` calls inside a `capture(...)` (pure
/// command substitution — `IrExpr::Capture` wraps the exec) or with
/// env/redirect/arrow-stage args keep their exec (the async/typed paths
/// the renderers special-case). `exec` statements that ARE the capture
/// body keep exec: the capture renderer needs the async exec
/// (subshell/pipeline context).
fn plain_exec_shape<'a>(func: &str, args: &'a [IrExpr]) -> Option<&'a str> {
    if func != "exec" {
        return None;
    }
    match args {
        // exec("cmd", [words])
        [IrExpr::Str(cmd, _), IrExpr::Array(_)] => Some(cmd.as_str()),
        // exec(Ident("cmd"), [words]) — frontend-emitted shorthand.
        [IrExpr::Ident(cmd), IrExpr::Array(_)] => Some(cmd.as_str()),
        // exec("cmd", [words], env-object): env-carrying exec (IFS=: ls …)
        // — estree's builtin twin applies the command-scoped env exactly
        // like the async exec path, so the rewrite is safe there too.
        [IrExpr::Str(cmd, _), IrExpr::Array(_), IrExpr::Object(_)] => Some(cmd.as_str()),
        _ => None,
    }
}

fn rewrite_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            let mut changed = false;
            if let Some(cmd) = plain_exec_shape(func, args) {
                if is_builtin(cmd) {
                    *func = "builtin".to_string();
                    changed = true;
                }
            }
            // recurse into ALL args — pipeline/redirect/capture shapes
            // carry nested Arrow bodies in args[0] (the cmd literal is a
            // Str/Ident; recursing it is a no-op)
            for a in args.iter_mut() {
                changed |= rewrite_expr(a);
            }
            changed
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut changed = rewrite_expr(obj);
            for a in args.iter_mut() {
                changed |= rewrite_expr(a);
            }
            changed
        }
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Ident(_)
        | IrExpr::Bool(_) | IrExpr::Json(_) | IrExpr::RawExpr(_) | IrExpr::Regex { .. }
        | IrExpr::Range { .. } | IrExpr::Object(_) | IrExpr::Index { .. } => false,
        IrExpr::Interpolate(parts) => {
            let mut changed = false;
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(e) = p {
                    changed |= rewrite_expr(e);
                }
            }
            changed
        }
        IrExpr::Capture { .. } => {
            // pure command-substitution context: keep the inner exec's
            // async semantics — do NOT descend (the capture renderers'
            // native folds key on the exec command name).
            false
        }
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => rewrite_stmts(stmts),
        IrExpr::Array(items) => {
            let mut changed = false;
            for i in items.iter_mut() {
                changed |= rewrite_expr(i);
            }
            changed
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            let mut changed = rewrite_expr(iter);
            changed |= rewrite_expr(elem);
            if let Some(c) = cond {
                changed |= rewrite_expr(c);
            }
            changed
        }
        IrExpr::Splice(inner) => rewrite_expr(inner),
        IrExpr::Arith(ast) => rewrite_arith(ast),
        IrExpr::Ext(_) => unreachable!("Ext nodes lowered before rendering"),
        IrExpr::BinOp { lhs, rhs, .. } => rewrite_expr(lhs) | rewrite_expr(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            rewrite_expr(cond) | rewrite_expr(then) | rewrite_expr(else_)
        }
        IrExpr::DefinedOr { expr, default } => rewrite_expr(expr) | rewrite_expr(default),
    }
}

fn rewrite_arith(ast: &mut ArithAst) -> bool {
    match ast {
        ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::IncDec { .. } => false,
        ArithAst::Index { key, .. } => rewrite_arith(key),
        ArithAst::Bin { lhs, rhs, .. } => rewrite_arith(lhs) | rewrite_arith(rhs),
        ArithAst::Un { arg, .. } => rewrite_arith(arg),
        ArithAst::Cond { test, then, else_ } => {
            rewrite_arith(test) | rewrite_arith(then) | rewrite_arith(else_)
        }
        ArithAst::Assign { rhs, .. } => rewrite_arith(rhs),
        ArithAst::Sizeof(_) => false,
        ArithAst::Cast { arg, .. } => rewrite_arith(arg),
    }
}

fn rewrite_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Ext(n) => { let mut c = false; for s in crate::shir_nodes::ExtNode::children_mut(&mut **n) { c |= rewrite_stmt(s); } c }
        IrStmt::Expr(e) => rewrite_expr(e),
        IrStmt::Assign { targets, expr, .. } => {
            let mut changed = rewrite_expr(expr);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    changed |= rewrite_expr(i);
                }
            }
            changed
        }
        IrStmt::Declare { init, .. } => init.as_mut().map(rewrite_expr).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => {
            let mut changed = false;
            for e in elements.iter_mut() {
                changed |= rewrite_expr(e);
            }
            changed
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut changed = rewrite_expr(cond);
            changed |= rewrite_stmts(then);
            for (c, b) in elsifs.iter_mut() {
                changed |= rewrite_expr(c);
                changed |= rewrite_stmts(b);
            }
            changed |= rewrite_stmts(else_);
            changed
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut changed = rewrite_stmts(body);
            for exc in excepts.iter_mut() {
                changed |= rewrite_stmts(&mut exc.body);
            }
            changed |= rewrite_stmts(else_body);
            changed |= rewrite_stmts(finally_body);
            changed
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
            rewrite_expr(cond) | rewrite_stmts(body)
        }
        IrStmt::For { iter, body, .. } => rewrite_expr(iter) | rewrite_stmts(body),
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let mut changed = rewrite_stmts(init);
            changed |= rewrite_expr(cond);
            changed |= rewrite_stmts(step);
            changed |= rewrite_stmts(body);
            changed
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut changed = rewrite_expr(discriminant);
            for c in clauses.iter_mut() {
                changed |= rewrite_stmts(&mut c.body);
            }
            changed
        }
        IrStmt::Function { body, .. } => rewrite_stmts(body),
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            rewrite_stmts(body)
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                for st in stage.iter_mut() {
                    changed |= rewrite_stmt(st);
                }
            }
            changed
        }
        IrStmt::Redirect { inner, .. } => rewrite_stmts(inner),
        IrStmt::Exec { cmd, args, .. } => {
            // The RICH Exec node (env/redirects/capture) keeps its shape —
            // rewriting the func would lose those fields. Only recurse the
            // arg expressions (harmless: an exec arg that is itself an
            // exec-call shape is unusual).
            let mut changed = rewrite_expr(cmd);
            for a in args.iter_mut() {
                changed |= rewrite_expr(a);
            }
            changed
        }
        IrStmt::Output { value, .. } => rewrite_expr(value),
        IrStmt::WriteFile { content, .. } => rewrite_expr(content),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e)
        | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => rewrite_expr(e),
        IrStmt::Return(None) | IrStmt::Exit(None) | IrStmt::Continue | IrStmt::Break
        | IrStmt::Require(_) | IrStmt::RawText(_) | IrStmt::Label(_) | IrStmt::Goto(_) => false,
        IrStmt::Select { clauses } => {
            let mut changed = false;
            for c in clauses.iter_mut() {
                changed |= rewrite_stmts(&mut c.body);
            }
            changed
        }
        IrStmt::Asm { inputs, outputs, .. } => {
            let mut changed = false;
            for (_, e) in inputs.iter_mut() {
                changed |= rewrite_expr(e);
            }
            for (_, e) in outputs.iter_mut() {
                changed |= rewrite_expr(e);
            }
            changed
        }
    }
}

fn rewrite_stmts(stmts: &mut [IrStmt]) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= rewrite_stmt(st);
    }
    changed
}

/// Transform channel entry: `fn(&mut Vec<IrStmt>) -> bool`.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    rewrite_stmts(stmts)
}

/// Shared fallback arm (PLAN.md §11): a backend that has NOT accepted the
/// `builtin` op renders it as the exec it came from. Applied at the
/// non-accepting renderers' entries (perl/sh/c/go/python/java/rust/zig/
/// js/glsl). Accepting renderers (estree) drop this call.
pub fn fallback_builtin_to_exec(prog: &mut crate::ir::IrProgram) {
    for st in &mut prog.stmts {
        erase(st);
    }
}

fn erase(st: &mut IrStmt) -> bool {
    fn erase_expr(e: &mut IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                let mut changed = if func == "builtin" {
                    *func = "exec".to_string();
                    true
                } else {
                    false
                };
                for a in args.iter_mut() {
                    changed |= erase_expr(a);
                }
                changed
            }
            IrExpr::MethodCall { obj, args, .. } => {
                let mut changed = erase_expr(obj);
                for a in args.iter_mut() {
                    changed |= erase_expr(a);
                }
                changed
            }
            IrExpr::Interpolate(parts) => {
                let mut changed = false;
                for p in parts.iter_mut() {
                    if let crate::ir::InterpPart::Expr(e) = p {
                        changed |= erase_expr(e);
                    }
                }
                changed
            }
            IrExpr::Capture { expr, .. } => erase_expr(expr),
            IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => erase_stmts(stmts),
            IrExpr::Array(items) => {
                let mut changed = false;
                for i in items.iter_mut() {
                    changed |= erase_expr(i);
                }
                changed
            }
            IrExpr::ArrayComp { iter, elem, cond, .. } => {
                let mut changed = erase_expr(iter);
                changed |= erase_expr(elem);
                if let Some(c) = cond {
                    changed |= erase_expr(c);
                }
                changed
            }
            IrExpr::Splice(inner) => erase_expr(inner),
            IrExpr::BinOp { lhs, rhs, .. } => erase_expr(lhs) | erase_expr(rhs),
            IrExpr::Ext(_) => unreachable!("Ext nodes lowered before rendering"),
            IrExpr::Ternary { cond, then, else_ } => {
                erase_expr(cond) | erase_expr(then) | erase_expr(else_)
            }
            IrExpr::DefinedOr { expr, default } => erase_expr(expr) | erase_expr(default),
            _ => false,
        }
    }
    fn erase_stmts(stmts: &mut [IrStmt]) -> bool {
        let mut changed = false;
        for st in stmts.iter_mut() {
            changed |= erase(st);
        }
        changed
    }
    match st {
        IrStmt::Ext(n) => { let mut c = false; for s in crate::shir_nodes::ExtNode::children_mut(&mut **n) { c |= erase(s); } c }
        IrStmt::Expr(expr) => erase_expr(expr),
        IrStmt::Assign { targets, expr, .. } => {
            let mut changed = erase_expr(expr);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    changed |= erase_expr(i);
                }
            }
            changed
        }
        IrStmt::Declare { init, .. } => init.as_mut().map(erase_expr).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => {
            let mut changed = false;
            for e in elements.iter_mut() {
                changed |= erase_expr(e);
            }
            changed
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut changed = erase_expr(cond);
            changed |= erase_stmts(then);
            for (c, b) in elsifs.iter_mut() {
                changed |= erase_expr(c);
                changed |= erase_stmts(b);
            }
            changed |= erase_stmts(else_);
            changed
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut changed = erase_stmts(body);
            for exc in excepts.iter_mut() {
                changed |= erase_stmts(&mut exc.body);
            }
            changed |= erase_stmts(else_body);
            changed |= erase_stmts(finally_body);
            changed
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
            erase_expr(cond) | erase_stmts(body)
        }
        IrStmt::For { iter, body, .. } => erase_expr(iter) | erase_stmts(body),
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let mut changed = erase_stmts(init);
            changed |= erase_expr(cond);
            changed |= erase_stmts(step);
            changed |= erase_stmts(body);
            changed
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut changed = erase_expr(discriminant);
            for c in clauses.iter_mut() {
                changed |= erase_stmts(&mut c.body);
            }
            changed
        }
        IrStmt::Function { body, .. } => erase_stmts(body),
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            erase_stmts(body)
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                for st in stage.iter_mut() {
                    changed |= erase(st);
                }
            }
            changed
        }
        IrStmt::Redirect { inner, .. } => erase_stmts(inner),
        IrStmt::Exec { cmd, args, .. } => {
            let mut changed = erase_expr(cmd);
            for a in args.iter_mut() {
                changed |= erase_expr(a);
            }
            changed
        }
        IrStmt::Output { value, .. } => erase_expr(value),
        IrStmt::WriteFile { content, .. } => erase_expr(content),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e)
        | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => erase_expr(e),
        IrStmt::Return(None) | IrStmt::Exit(None) | IrStmt::Continue | IrStmt::Break
        | IrStmt::Require(_) | IrStmt::RawText(_) | IrStmt::Label(_) | IrStmt::Goto(_) => false,
        IrStmt::Select { clauses } => {
            let mut changed = false;
            for c in clauses.iter_mut() {
                changed |= erase_stmts(&mut c.body);
            }
            changed
        }
        IrStmt::Asm { inputs, outputs, .. } => {
            let mut changed = false;
            for (_, e) in inputs.iter_mut() {
                changed |= erase_expr(e);
            }
            for (_, e) in outputs.iter_mut() {
                changed |= erase_expr(e);
            }
            changed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn exec_call(cmd: &str, args: &[&str]) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str(cmd.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(
                    args.iter()
                        .map(|a| IrExpr::Str(a.to_string(), StrStyle::DoubleQuoted))
                        .collect(),
                ),
            ],
        })
    }

    #[test]
    fn rewrites_builtins_not_externals() {
        let mut stmts = vec![
            exec_call("ls", &["-la", "/tmp"]),
            exec_call("custom_prog", &["x"]),
        ];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Expr(IrExpr::Call { func, .. }) => assert_eq!(func, "builtin"),
            _ => panic!(),
        }
        match &stmts[1] {
            IrStmt::Expr(IrExpr::Call { func, .. }) => assert_eq!(func, "exec"),
            _ => panic!(),
        }
    }

    #[test]
    fn recurses_into_bodies() {
        let mut stmts = vec![IrStmt::Block(vec![exec_call("echo", &["hi"])])];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Block(b) => match &b[0] {
                IrStmt::Expr(IrExpr::Call { func, .. }) => assert_eq!(func, "builtin"),
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    #[test]
    fn is_idempotent() {
        let mut stmts = vec![exec_call("echo", &["hi"])];
        assert!(transform(&mut stmts));
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn fallback_erases_builtin() {
        let mut prog = crate::ir::IrProgram {
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::Expr(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![
                    IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![]),
                ],
            })],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        fallback_builtin_to_exec(&mut prog);
        match &prog.stmts[0] {
            IrStmt::Expr(IrExpr::Call { func, .. }) => assert_eq!(func, "exec"),
            _ => panic!(),
        }
    }
}