//! shir-pipeline-native — rewrite SIDE-EFFECT `pipeline(...)` calls whose
//! stages are all commands the renderers emulate NATIVELY (no bash at
//! runtime) into the structured `IrStmt::Pipeline` statement form.
//!
//! ## Why
//!
//! `./fail-shir` counts `system('bash', '-c', …)` call sites in the
//! rendered Perl. The Perl renderer has ONE statement-position lowering
//! for a `Call { func: "pipeline" }`: rebuild the shell text
//! (`pipeline_call_to_cmd`) and run it through `system('bash', '-c', …)` —
//! even when EVERY stage is a command it renders natively in-process
//! (echo/printf + the verified-emulable command set, the same whitelist
//! the renderers' native paths cover). The shell-out is an artefact of
//! the STATEMENT SHAPE, not of the commands.
//!
//! The structured `IrStmt::Pipeline` form is the canonical shape every
//! renderer already lowers:
//! - **Perl** (`emit_stmt` Pipeline, capture:None): emits the STAGES as
//!   native statements — `echo` → a print, `grep`/`tr`/`wc`/`ls`/`cat`/
//!   `tail`/… → the AST Generator's in-Perl emulation. No `bash -c`.
//! - **ESTree** (`stmt_to_estree` Pipeline): rebuilds the identical
//!   `pipeline` Call and lets `expr_to_estree` lower it through the SAME
//!   pipeline machinery/folds the `Expr(Call(pipeline))` form reaches —
//!   an `echo X | grep P` fold (grepText), `echo X | tr` fold (a native
//!   echo), etc. For the static echo/printf-produced pipelines this
//!   transform rewrites, the ESTree output is byte-identical (the folds
//!   fire identically), so fail-estree cannot move.
//!
//! ## Scope / guards (REFUSE > GUESS)
//!
//! - ONLY statement-position `Expr(Call { func: "pipeline" })` whose
//!   stages are each a SINGLE `Expr(exec|builtin)` of a natively-
//!   emulable command — i.e. rewriting the pipeline removes shell-outs
//!   instead of moving them.
//! - The first (producer) stage must also be a natively-emulable
//!   command. The estree renderer re-creates the identical `pipeline`
//!   Call from a `Pipeline` statement (`stmt_to_estree`), so any native
//!   producer renders byte-identically in both statement forms (verified
//!   empirically for echo/printf/cat/ls/grep/tr/wc/tail producers). A
//!   non-native producer (sed/diff/gzip/…) is refused: the perl renderer
//!   would only shell out that stage, so no reduction.
//! - NOT the whole story everywhere else: pipelines inside `&&`/`||`
//!   operand positions, `Redirect` inners, `capture` bodies, and
//!   dynamic-word stages are all left untouched (they render through
//!   different arms and cannot be proven bash-identical here).

use crate::ir::{IrExpr, IrStmt, StrStyle};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= transform_stmt(s);
    }
    c
}

fn transform_stmt(st: &mut IrStmt) -> bool {
    // Recurse into children FIRST (bottom-up — a nested pipeline inside
    // an If body is rewritten before we look at the enclosing shape).
    let mut c = match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut x = transform_expr(cond);
            x |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                x |= transform_expr(ec);
                x |= transform(eb);
            }
            x |= transform(else_);
            x
        }
        IrStmt::For { iter, body, .. } => {
            let mut x = transform_expr(iter);
            x |= transform(body);
            x
        }
        IrStmt::While { cond, body, .. } => {
            let mut x = transform(body);
            x |= transform_expr(cond);
            x
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut x = transform(body);
            x |= transform_expr(cond);
            x
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut x = transform_expr(discriminant);
            for cl in clauses.iter_mut() {
                x |= transform(&mut cl.body);
            }
            x
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => transform(b),
        IrStmt::Pipeline { stages, .. } => {
            let mut x = false;
            for stg in stages.iter_mut() {
                x |= transform(stg);
            }
            x
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut x = transform(inner);
            for r in redirects.iter_mut() {
                x |= transform_expr(&mut r.target);
            }
            x
        }
        IrStmt::Expr(e) => {
            let mut x = transform_expr(e);
            x |= lower_pipeline(st);
            x
        }
        IrStmt::Assign { expr, .. } | IrStmt::Output { value: expr, .. } => transform_expr(expr),
        IrStmt::Declare {
            init: Some(expr), ..
        } => transform_expr(expr),
        IrStmt::WriteFile { path, content, .. } => transform_expr(path) | transform_expr(content),
        IrStmt::Exec { cmd, args, env, .. } => {
            let mut x = transform_expr(cmd);
            for a in args.iter_mut() {
                x |= transform_expr(a);
            }
            for (_, v) in env.iter_mut() {
                x |= transform_expr(v);
            }
            x
        }
        _ => false,
    };
    c
}

fn transform_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arrow(stmts) => transform(stmts),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for a in items.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, v) in pairs.iter_mut() {
                c |= transform_expr(v);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => transform_expr(lhs) | transform_expr(rhs),
        IrExpr::Index { key, .. } => transform_expr(key),
        _ => false,
    }
}

/// The canonical exec/builtin word args: `func("cmd", [word, …])` or
/// `func(Ident("cmd"), [word, …])`. The command name is the first arg;
/// the words sit in the trailing Array (the plain statement shape both
/// renderers' native emulations match).
fn exec_parts<'a>(func: &str, args: &'a [IrExpr]) -> Option<(&'a str, &'a [IrExpr])> {
    if !matches!(func, "exec" | "builtin") {
        return None;
    }
    match args {
        [IrExpr::Str(cmd, _), IrExpr::Array(words)] => Some((cmd, words)),
        [IrExpr::Ident(cmd), IrExpr::Array(words)] => Some((cmd, words)),
        _ => None,
    }
}

/// The command names the Perl renderer emulates natively at statement
/// position: the modern-IR echo/printf/cd/… builtin arms of
/// `emit_exec_call` PLUS the verified-emulable set
/// (`harness/shir-whitelist.txt` / `EMULATED_COMMANDS`, via the AST
/// Generator's in-Perl emulations). A stage whose command is outside
/// this set would still shell out when rendered — such a pipeline is
/// NOT a shell-out reduction and is refused.
fn is_native_cmd(cmd: &str) -> bool {
    matches!(
        cmd,
        "echo"
            | "printf"
            | "cd"
            | "export"
            | "pwd"
            | "shopt"
            | "set"
            | "true"
            | ":"
            | "false"
            | "shift"
            | "exit"
            | "local"
            | "read"
            | "sleep"
            | "wait"
            | "seq"
            | "ls"
            | "wc"
            | "cat"
            | "tail"
            | "grep"
            | "tr"
            | "mkdir"
            | "rm"
            | "touch"
            | "basename"
            | "dirname"
            | "date"
            | "hostname"
            | "paste"
            | "tee"
            | "which"
            | "yes"
    )
}

/// A pipeline stage that the Perl renderer lowers natively (no
/// shell-out): a single statement `Expr(exec|builtin)` whose command is
/// natively emulable. Anything else (multi-statement stages, redirects,
/// dynamic commands, captures, `for` stages…) keeps the pipeline on the
/// shell-out path — refuse > guess.
fn stage_is_native(body: &[IrStmt]) -> Option<()> {
    let [IrStmt::Expr(IrExpr::Call { func, args })] = body else {
        return None;
    };
    let (cmd, _words) = exec_parts(func, args)?;
    if is_native_cmd(cmd) {
        Some(())
    } else {
        None
    }
}

/// `Expr(Call { func: "pipeline" })` in statement position →
/// `IrStmt::Pipeline`. Only when EVERY stage is native. The producer
/// (first stage) must also be a natively-emulable command — the estree
/// renderer re-creates the identical `pipeline` Call from a `Pipeline`
/// statement (stmt_to_estree), so any native producer renders
/// byte-identically in both statement forms (verified empirically for
/// echo/printf/cat/ls/grep/tr/wc/head/sort/tail producers). The stage
/// list is emptied — the renderer's `capture: None` arm re-emits each
/// stage natively.
fn lower_pipeline(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return false;
    };
    if func != "pipeline" {
        return false;
    }
    let [IrExpr::Array(stage_exprs)] = args.as_slice() else {
        return false;
    };
    if stage_exprs.is_empty() {
        return false;
    }
    let mut stages: Vec<Vec<IrStmt>> = Vec::with_capacity(stage_exprs.len());
    for (idx, s) in stage_exprs.iter().enumerate() {
        let IrExpr::Arrow(body) = s else {
            return false;
        };
        if stage_is_native(body).is_none() {
            return false;
        }
        // The producer (first stage) must be a natively-emulable command
        // too — a non-native producer would still shell out when rendered.
        if idx == 0 {
            let IrStmt::Expr(IrExpr::Call { func: f, args: a }) = &body[0] else {
                return false;
            };
            let (cmd, _) = exec_parts(f, a).unwrap();
            if !is_native_cmd(cmd) {
                return false;
            }
        }
        stages.push(body.clone());
    }
    *st = IrStmt::Pipeline {
        stages,
        last_output: None,
        capture: None,
        cmd_str: None,
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    /// Lower + run the transform + serialize to compact JSON.
    fn lower(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        let _ = transform(&mut prog.stmts);
        shir_to_shir_json(&prog)
    }

    /// Lower + run the transform + assert SOMETHING changed.
    fn assert_changes(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(
            transform(&mut prog.stmts),
            "transform was a no-op for {src}"
        );
        shir_to_shir_json(&prog)
    }

    #[test]
    fn echo_grep_pipeline_becomes_pipeline_stmt() {
        let json = assert_changes("echo \"alpha beta\" | grep beta");
        // the `pipeline` Call is gone — a structured Pipeline statement
        assert!(!json.contains("\"pipeline\""), "pipeline call should be gone: {json}");
        assert!(json.contains("\"Pipeline\""), "missing Pipeline stmt: {json}");
        // both stages survive, each as a native exec/builtin
        assert!(json.contains("\"echo\""));
        assert!(json.contains("\"grep\""));
    }

    #[test]
    fn non_native_stage_is_refused() {
        // `sed` (outside the emulable set) keeps the pipeline Call — the
        // perl renderer would only shell out that stage, so no reduction.
        let commands = parse_commands_from_text("echo foo | sed s/a/b/").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        let before = shir_to_shir_json(&prog);
        let changed = transform(&mut prog.stmts);
        let after = shir_to_shir_json(&prog);
        assert_eq!(before, after, "non-native stage must be untouched");
        assert!(!changed);
    }

    #[test]
    fn non_native_producer_is_refused() {
        // `sed | grep` — sed is outside the emulable set, so the
        // producer (and the stage) is not native; the perl renderer
        // would only shell out that stage, so no reduction. Refuse.
        let commands = parse_commands_from_text("sed s/a/b/ | grep -v a.txt").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        let before = shir_to_shir_json(&prog);
        let changed = transform(&mut prog.stmts);
        let after = shir_to_shir_json(&prog);
        assert_eq!(before, after, "non-native producer must be untouched");
        assert!(!changed);
    }

    #[test]
    fn echo_tr_pipeline() {
        let json = assert_changes("echo hello | tr a-z A-Z");
        assert!(json.contains("\"Pipeline\""));
        assert!(json.contains("\"tr\""));
    }

    #[test]
    fn multi_stage_echo_grep_wc() {
        // each stage is native; grep/native chain folds in estree as the
        // Expr form — rewrite all of it.
        let json = assert_changes("echo x | grep y | wc -l");
        assert!(json.contains("\"Pipeline\""));
        assert!(json.contains("\"wc\""));
    }

    #[test]
    fn pipeline_in_if_body_rewrites_recursively() {
        let json = assert_changes("if true; then echo x | grep y; fi");
        assert!(json.contains("\"Pipeline\""), "nested pipeline should be lowered: {json}");
    }

    #[test]
    fn non_pipeline_is_left_alone() {
        let commands = parse_commands_from_text("echo hello").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(!transform(&mut prog.stmts), "echo should be a no-op");
    }
}
