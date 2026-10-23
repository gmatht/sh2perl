//! capture-dup-flatten — a capture body whose fd dup must outlive a
//! sibling command is joined into ONE leading `begin … end` compound, the
//! shape every shell backend can spell and fish can parse.
//!
//! ## Need
//!
//! bash's `exec N>&M` installs a dup in the SHELL's fd table, so inside a
//! capture the dup outlives every command after it:
//!
//! ```text
//! r=$(
//!     exec 4>&1
//!     echo "before" >&4
//!     (echo "sub") 3>&- | cat
//!     echo "after" >&4
//! )
//! ```
//!
//! fish has no persistent fd table: a redirect applies to the ONE command
//! (or the scope of the one compound) it is written on. The renderer can
//! carry the dup from the no-op to the commands that need it only as
//! `begin` / … / `end 4>&1` — and fish 3.7's parser rejects that compound
//! when it contains a pipeline-stage group ("Missing end to balance this
//! begin"), so the body must be ONE compound in a shape the parser accepts.
//! The corpus's bodies are sequences of plain commands and pipelines:
//! `begin; <a>; <b>; <end 4>&1>` parses and keeps the dup live for all of
//! them (measured; see backends/fish/docs/fish-capture-bodies.md).
//!
//! ## The rule (refuse > guess)
//!
//! A capture is flattened ONLY when
//!
//! - its body is 2..=8 statements,
//! - it opens with a bare `exec N>&M` no-op (`IrStmt::Redirect` whose inner
//!   is `Exec { cmd: "exec", args: [] }`) — the dup form,
//! - every command in the body can be written as `cmd; cmd; …` (no
//!   compound statement, no control-flow keyword, no comment, nothing with
//!   its own block structure),
//! - at most one statement is a pipeline (a second `|` cannot be chained
//!   into the same `;` list),
//! - NO command in the body runs a shell BUILTIN: a builtin (`echo`,
//!   `printf`, `test`, …) runs in the shell's own process, so it inherits
//!   the redirect applied to the `begin … end` (bash sets a builtin's fd 1
//!   to the group's redirect too, and for `4>&1` the dup ALIASES 1 — a
//!   builtin's `>&4` would then feed its own stdout into the capture,
//!   which does not happen under bash's persistent-fd-table semantics),
//! - the source line recorded for the first statement IS the first
//!   statement's own line (the multi-line source form — `r=$(` on its own
//!   line — which cannot be reproduced on one line; the canonical
//!   transform set is corpus-verified on the byte-pinned export).
//!
//! Everything else is left untouched: a shape outside the rule renders
//! exactly as it did before this transform existed.
//!
//! ## Rewrite
//!
//! `Capture { Arrow(stmts) }` → `Capture { Arrow([ Block(stmts) ]) }`,
//! statement lines and order unchanged. The body's first statement (the
//! `exec`) is kept: bash's dup no-op is exactly what the block's own scope
//! models, and dropping it would leave the renderer nothing to resolve the
//! body's `>&N` targets through.

use crate::ir::{IrExpr, IrStmt};

/// The longest body this transform will join onto one line.
const MAX_STMTS: usize = 8;

/// Names that run in the shell's own process: a redirect applied to their
/// enclosing compound leaks into the capture (see the module docs).
const SHELL_BUILTINS: &[&str] = &[
    ".", ":", "alias", "bg", "break", "cd", "continue", "declare", "echo", "eval", "exec",
    "exit", "export", "false", "fg", "getopts", "hash", "help", "history", "jobs", "kill",
    "let", "local", "mapfile", "printf", "pwd", "read", "readarray", "readonly", "return",
    "set", "shift", "shopt", "source", "test", "times", "trap", "true", "type", "typeset",
    "ulimit", "umask", "unalias", "unset", "wait",
];

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    walk_stmts(stmts)
}

fn walk_stmts(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= walk_stmt(st);
    }
    changed
}

fn walk_stmt(st: &mut IrStmt) -> bool {
    let mut changed = false;
    match st {
        IrStmt::Assign { expr, .. } => changed |= flatten_expr(expr),
        IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => changed |= flatten_expr(e),
        IrStmt::Declare { init: Some(e), .. } => changed |= flatten_expr(e),
        IrStmt::If { cond, then, elsifs, else_ } => {
            changed |= flatten_expr(cond);
            changed |= walk_stmts(then);
            for (c, b) in elsifs.iter_mut() {
                changed |= flatten_expr(c);
                changed |= walk_stmts(b);
            }
            changed |= walk_stmts(else_);
        }
        IrStmt::While { cond, body } => {
            changed |= flatten_expr(cond);
            changed |= walk_stmts(body);
        }
        IrStmt::DoWhile { body, cond, .. } => {
            changed |= walk_stmts(body);
            changed |= flatten_expr(cond);
        }
        IrStmt::For { iter, body, .. } => {
            changed |= flatten_expr(iter);
            changed |= walk_stmts(body);
        }
        IrStmt::ForInit { init, cond, step, body } => {
            changed |= walk_stmts(init);
            changed |= flatten_expr(cond);
            changed |= walk_stmts(step);
            changed |= walk_stmts(body);
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Redirect { inner: b, .. }
        | IrStmt::Function { body: b, .. } => changed |= walk_stmts(b),
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages.iter_mut() {
                changed |= walk_stmts(stage);
            }
        }
        IrStmt::Case { discriminant, clauses, .. } => {
            changed |= flatten_expr(discriminant);
            for c in clauses.iter_mut() {
                changed |= walk_stmts(&mut c.body);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            changed |= walk_stmts(body);
            for e in excepts.iter_mut() {
                changed |= walk_stmts(&mut e.body);
            }
            changed |= walk_stmts(else_body);
            changed |= walk_stmts(finally_body);
        }
        _ => {}
    }
    changed
}

fn flatten_expr(e: &mut IrExpr) -> bool {
    let mut changed = false;
    match e {
        IrExpr::Capture { expr, .. } => changed |= maybe_flatten_body(expr),
        IrExpr::Arrow(body) => changed |= walk_stmts(body),
        IrExpr::Call { args, .. } | IrExpr::MethodCall { args, .. } => {
            for a in args.iter_mut() {
                changed |= flatten_expr(a);
            }
        }
        IrExpr::Array(items) => {
            for a in items.iter_mut() {
                changed |= flatten_expr(a);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props.iter_mut() {
                changed |= flatten_expr(v);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            // walk the operands as EXPRESSIONS (a shell `x=$(…) && …`
            // parses as BinOp{ lhs: Call("assign", [x, =, Capture]) })
            changed |= flatten_expr(lhs);
            changed |= flatten_expr(rhs);
        }
        IrExpr::Ternary { cond, then, else_, .. } => {
            changed |= flatten_expr(cond);
            changed |= flatten_expr(then);
            changed |= flatten_expr(else_);
        }
        IrExpr::DefinedOr { expr, default, .. } => {
            changed |= flatten_expr(expr);
            changed |= flatten_expr(default);
        }
        IrExpr::Index { key, .. } => changed |= flatten_expr(key),
        _ => {}
    }
    changed
}

/// `Arrow(stmts)` → `Arrow([Block(stmts)])` when `stmts` matches the rule:
/// the body becomes ONE block statement, which every shell backend renders
/// as a single `begin … end` command. (An `Arrow([Block])` is what the
/// fish renderer's block arm expects — it renders the block, not the arrow.)
fn maybe_flatten_body(expr: &mut Box<IrExpr>) -> bool {
    let IrExpr::Arrow(stmts) = expr.as_mut() else {
        return false;
    };
    if !eligible(stmts) {
        return false;
    }
    let body = std::mem::take(stmts);
    stmts.push(IrStmt::Block(body));
    true
}


/// The whole rule, on the statement list (see the module docs).
pub(crate) fn eligible(stmts: &[IrStmt]) -> bool {
    if stmts.len() < 2 || stmts.len() > MAX_STMTS {
        return false;
    }
    // opens with the exec-dup no-op
    if !is_dup_noop(&stmts[0]) {
        return false;
    }
    // Every other statement must be a PLAIN command (`cmd words`): no
    // builtin (it would run in the shell's process under the group's
    // redirect), no redirect of its own, no second pipeline.
    let mut pipelines = 0usize;
    for st in stmts {
        let ok = match st {
            IrStmt::Redirect { .. } if is_dup_noop(st) => true,
            // a redirect-wrapped command (`echo x >&4`) keeps the redirect on
            // ITS OWN command inside the group; the body's command set must
            // still be modelable
            IrStmt::Redirect { inner, redirects } => {
                redirects.iter().all(|r| matches!(&r.target, IrExpr::Str(t, _) if t.starts_with('&') || t == "-"))
                    && match inner.as_slice() {
                        [IrStmt::Expr(e)] => plain_command(e),
                        _ => false,
                    }
            }
            IrStmt::Expr(e) => plain_command(e),
            IrStmt::Pipeline { stages, .. } => {
                pipelines += 1;
                pipelines <= 1
                    && stages.iter().all(|stage| match stage.as_slice() {
                        [IrStmt::Expr(e)] => plain_command(e),
                        [IrStmt::Redirect { inner, redirects }] => {
                            redirects.iter().all(|r| matches!(&r.target, IrExpr::Str(t, _) if t.starts_with('&') || t == "-"))
                                && match inner.as_slice() {
                                    [IrStmt::Expr(e)] => plain_command(e),
                                    _ => false,
                                }
                        }
                        _ => false,
                    })
            }
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

/// A bare `exec N>&M` (`IrStmt::Redirect` wrapping `Exec { cmd: "exec" }`
/// with no args and only dup/close redirects).
fn is_dup_noop(st: &IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else {
        return false;
    };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else {
        return false;
    };
    if func == "subshell" {
        // `( cmd … )` — a compound that renders `begin … end`; as a pipeline
        // STAGE it is a run of plain commands
        return match args.first() {
            Some(IrExpr::Arrow(body)) => {
                !body.is_empty()
                    && body.iter().all(|b| match b {
                        IrStmt::Expr(inner) => plain_command(inner),
                        _ => false,
                    })
            }
            _ => false,
        };
    }
    if func != "exec" {
        return false;
    }
    let Some(IrExpr::Str(name, _)) = args.first() else {
        return false;
    };
    // a bare `exec`: no words beyond the command itself
    if name != "exec" {
        return false;
    }
    if let Some(IrExpr::Array(words)) = args.get(1) {
        if !words.is_empty() {
            return false;
        }
    }
    !redirects.is_empty()
        && redirects.iter().all(|r| match &r.target {
            IrExpr::Str(t, _) => t.starts_with('&') || t.starts_with('-'),
            _ => false,
        })
}

/// One command that can sit in a `;` list: an `Exec` of a non-builtin
/// whose words hold no control-flow keyword or comment character.
fn plain_command(e: &IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else {
        return false;
    };
    if func == "redirect" {
        // `redirect(Arrow([one command]), [specs])` — a command with its own
        // redirect; safe as a group command (the renderer emits it inline)
        return match (args.first(), args.get(1)) {
            (Some(IrExpr::Arrow(body)), Some(IrExpr::Array(specs))) => {
                body.len() == 1
                    && matches!(&body[0], IrStmt::Expr(inner) if plain_command(inner))
                    && specs.iter().all(|sp| match sp {
                        IrExpr::Object(props) => props.iter().all(|(k, v)| match (k.as_str(), v) {
                            ("target", IrExpr::Str(t, _)) => t.starts_with('&'),
                            _ => true,
                        }),
                        _ => false,
                    })
            }
            _ => false,
        };
    }
    if func == "subshell" {
        // `( cmd … )` — a compound that renders `begin … end`; as a pipeline
        // STAGE it is a run of plain commands
        return match args.first() {
            Some(IrExpr::Arrow(body)) => {
                !body.is_empty()
                    && body.iter().all(|b| match b {
                        IrStmt::Expr(inner) => plain_command(inner),
                        _ => false,
                    })
            }
            _ => false,
        };
    }
    if func != "exec" {
        return false;
    }
    let Some(IrExpr::Str(cmd, _)) = args.first() else {
        return false;
    };
    if cmd.is_empty()
        || SHELL_BUILTINS.contains(&cmd.as_str())
        || !cmd.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_' || c == '/').unwrap_or(false)
        || cmd.contains(|c: char| c.is_whitespace() || "|&;<>()#".contains(c))
    {
        return false;
    }
    let Some(IrExpr::Array(words)) = args.get(1) else {
        return false;
    };
    // `exec("cat", [])` is a placeholder the renderer expands to its default
    // form; a `;` list must carry the real words.
    !words.is_empty() && words.iter().all(plain_word)
}


fn plain_word(w: &IrExpr) -> bool {
    match w {
        IrExpr::Str(s, _) => !s.contains(|c: char| "|&;<>()#\n".contains(c) && !is_redirect_target(s)),
        IrExpr::Int(_) | IrExpr::Var(..) | IrExpr::Index { .. } => true,
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            crate::ir::InterpPart::Lit(t) => !t.contains(['|', ';', '\n']),
            _ => true,
        }),
        _ => false,
    }
}

/// `&N` / `&-` words are dup targets the renderer resolves, not shell
/// metacharacters.
fn is_redirect_target(s: &str) -> bool {
    s.len() >= 2 && s.starts_with('&')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn st(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }
    fn exec(cmd: &str, args: Vec<IrExpr>) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![st(cmd), IrExpr::Array(args)],
        })
    }
    /// a redirect-wrapped exec, the real `echo x >&4` shape
    fn rdir(cmd: &str, fd: i32, to: &str) -> IrStmt {
        IrStmt::Redirect {
            inner: vec![exec(cmd, vec![])],
            redirects: vec![crate::ir::IrRedirect {
                fd: Some(fd),
                mode: "w".into(),
                target: st(to),
                interpolate: true,
            }],
        }
    }
    fn dup(fd: i32, to: &str) -> IrStmt {
        IrStmt::Redirect {
            inner: vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".into(),
                args: vec![st("exec"), IrExpr::Array(vec![])],
            })],
            redirects: vec![crate::ir::IrRedirect {
                fd: Some(fd),
                mode: "w".into(),
                target: st(to),
                interpolate: true,
            }],
        }
    }

    #[test]
    fn flattens_the_dup_capture() {
        let mut e = Box::new(IrExpr::Arrow(vec![
            dup(4, "&1"),
            exec("cat", vec![st("file")]),
        ]));
        assert!(maybe_flatten_body(&mut e));
        let IrExpr::Arrow(v) = e.as_ref() else { unreachable!() };
        assert!(matches!(v.as_slice(), [IrStmt::Block(_)]), "{v:?}");
    }

    #[test]
    fn refuses_a_builtin_in_the_body() {
        let mut e = Box::new(IrExpr::Arrow(vec![dup(4, "&1"), exec("echo", vec![st("x")])]));
        assert!(!maybe_flatten_body(&mut e));
    }

    #[test]
    fn refuses_without_a_leading_dup() {
        let mut e = Box::new(IrExpr::Arrow(vec![exec("cat", vec![]), dup(4, "&1")]));
        assert!(!maybe_flatten_body(&mut e));
    }

    #[test]
    fn refuses_control_flow() {
        let mut e = Box::new(IrExpr::Arrow(vec![
            dup(4, "&1"),
            IrStmt::If {
                cond: IrExpr::Int(1),
                then: vec![],
                elsifs: vec![],
                else_: vec![],
            },
        ]));
        assert!(!maybe_flatten_body(&mut e));
    }

    #[test]
    fn refuses_two_pipelines() {
        let pl = |cmd: &str| {
            IrStmt::Pipeline {
                stages: vec![
                    vec![exec(cmd, vec![])],
                    vec![exec("cat", vec![])],
                ],
                last_output: None,
                capture: None,
                cmd_str: None,
            }
        };
        let mut e = Box::new(IrExpr::Arrow(vec![dup(4, "&1"), pl("a"), pl("b")]));
        assert!(!maybe_flatten_body(&mut e));
    }

    #[test]
    fn accepts_one_pipeline() {
        let mut e = Box::new(IrExpr::Arrow(vec![
            dup(4, "&1"),
            IrStmt::Pipeline {
                stages: vec![vec![exec("a", vec![st("f")])], vec![exec("cat", vec![st("f")])]],
                last_output: None,
                capture: None,
                cmd_str: None,
            },
        ]));
        assert!(maybe_flatten_body(&mut e));
    }
}
