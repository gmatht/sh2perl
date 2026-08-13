//! process-subst — materialize process substitution (`<(...)` / `>(...)`)
//! redirects into POSIX temp-file form, at the IR level.
//!
//! ## Need
//!
//! POSIX sh has no process substitution: bash's `<(...)` passes the
//! producer's output via a `/dev/fd/N` path *argument*; a POSIX-targeting
//! renderer cannot express it as a redirect-string patch (the path must
//! exist as a file before the command runs and be cleaned up after — a
//! statement-level lowering). This transform lowers every
//! `process-in`/`process-out` redirect on an `IrStmt::Redirect` into the
//! canonical temp-file materialization:
//!
//! ```text
//! tmp=$(mktemp)
//! { producer; } > tmp          # process-in: capture the producer's stdout
//! cmd … "$tmp" …               # the path stands where bash put /dev/fd/N
//! { producer; } < tmp          # process-out: the producer consumes the file
//! rm -f "$tmp"
//! ```
//!
//! (represented as `Assign` + `capture`-call, `Redirect`-wrapped producer
//! stmts, appended exec args, and a trailing `rm` exec — every node is in
//! the A1 contract, so all backends inherit the lowering).
//!
//! ## Placement
//!
//! Registered in `transforms.rs` (gated by `DEBASHC_TRANSFORMS` like the
//! rest of the registry) — it runs in `ast_to_ir`, i.e. the ESTree file
//! pipeline and the `--shir` export — and is ALSO run at the A1 ingress
//! (cli `--shir-in-estree` / `--shir-in-perl`), so frontend A1 JSON
//! carrying process-in/out is materialized the same way the file pipeline
//! is. The ESTree corpus never sees process-in modes (estree.rs
//! `transform_cmd` rewrites `<(...)` to here-string + materialized-path
//! args BEFORE `ast_to_ir`), so the transform is a no-op there.
//!
//! ## Semantics / limits
//!
//! - The producer's shell text (the redirect target, reconstructed by
//!   `command_to_shell_text`) is re-parsed into IR stmts; a producer that
//!   fails to parse (or an exec shape that cannot take appended args)
//!   leaves the whole redirect untouched — the renderer's existing
//!   behavior stays in charge (refuse > guess).
//! - Nested process substitution inside a producer is materialized
//!   recursively.
//! - `mapfile`/`readarray` (no path-argument form) get the temp file via
//!   a stdin `r` redirect instead of an appended arg; everything else
//!   gets the path arg, mirroring bash's `/dev/fd/N` argument passing.
//! - The path arg is appended at the END of the arg list (the parser
//!   drops the original token positions; the estree pre-pass has the same
//!   convention). Trailing-position process substitutions are the
//!   corpus-proven shape.
//! - The trailing `rm -f` clobbers the command's exit status for a
//!   following `$?` — the same accepted limitation as the sh renderer's
//!   single-PS lowering; the multi-PS / `$?`-sensitive renderers do their
//!   own rc preservation.

use crate::ast::Command;
use crate::ir::{AssignTarget, IrExpr, IrProgram, IrRedirect, IrStmt, StrStyle};
use crate::parser::commands::parse_commands_from_text;

/// Apply the transform to a statement list. Returns whether anything
/// changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut n = 0usize;
    materialize(stmts, &mut n)
}

/// Program-level entry for the A1 ingress: walks top-level stmts AND the
/// `subs` (frontend-emitted function bodies live there).
pub fn transform_program(prog: &mut IrProgram) -> bool {
    let mut n = 0usize;
    let mut c = materialize(&mut prog.stmts, &mut n);
    for sub in &mut prog.subs {
        c |= materialize(&mut sub.body, &mut n);
    }
    c
}

// ── helpers ─────────────────────────────────────────────────────────

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}
fn var(name: &str) -> IrExpr {
    IrExpr::Var(name.to_string(), None)
}

/// Commands with no path-argument form (they read stdin when given no
/// file args, and error on a stray path operand). Everything else accepts
/// a path argument, which is what bash actually passes for `<(...)`.
fn stdin_only_command(name: &str) -> bool {
    matches!(name, "mapfile" | "readarray")
}

/// `tmp=$(mktemp)` — the Assign+capture form so every renderer sees the
/// capture (the sh renderer: `tmp="$(mktemp)"`; the ESTree renderer:
/// `sh2.setVar(..., sh2.capture(...))` — the runtime strips the trailing
/// newline like bash).
fn mktemp_assign(tmp: &str) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: tmp.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: IrExpr::Call {
            func: "capture".to_string(),
            args: vec![IrExpr::Arrow(vec![IrStmt::Exec {
                cmd: st("mktemp"),
                args: vec![],
                capture: None,
                redirects: vec![],
                env: vec![],
            }])],
        },
        asm: None,
    }
}

/// `{ producer…; } <op> tmp` — run the producer IR with the temp file as
/// the redirect target (stdout for process-in, stdin for process-out).
fn producer_redirect(producer: Vec<IrStmt>, tmp: &IrExpr, mode: &str, fd: Option<i32>) -> IrStmt {
    IrStmt::Redirect {
        inner: producer,
        redirects: vec![IrRedirect {
            fd,
            mode: mode.to_string(),
            target: tmp.clone(),
            interpolate: false,
        }],
    }
}

/// `rm -f tmp…` — the cleanup exec.
fn rm_exec(tmps: &[IrExpr]) -> IrStmt {
    let mut args = vec![st("-f")];
    args.extend(tmps.iter().cloned());
    IrStmt::Exec {
        cmd: st("rm"),
        args,
        capture: None,
        redirects: vec![],
        env: vec![],
    }
}

/// Parse the producer's shell text (reconstructed by the core's
/// `command_to_shell_text`) back into IR stmts. `None` on any parse or
/// lowering failure — the caller leaves the redirect untouched then.
/// A control byte / Debug-string artifact in the text means the
/// reconstruction fell into a Debug leak (a producer command kind
/// `command_to_shell_text` cannot express): real shell text never
/// contains these, so refuse rather than emit a garbage producer.
fn parse_producer(text: &str) -> Option<Vec<IrStmt>> {
    if text.contains('\u{1}')
        || text.contains("Literal(")
        || text.contains("None)")
        || text.contains("commands: [")
        || text.contains("SimpleCommand { ")
    {
        return None;
    }
    let commands: Vec<Command> = parse_commands_from_text(text).ok()?;
    let mut out = Vec::new();
    for c in &commands {
        out.extend(crate::shir::stmt_for_command(c));
    }
    Some(out)
}

// ── walkers ─────────────────────────────────────────────────────────

/// Recursively materialize every process substitution in the statement
/// list. `n` is the fresh-name counter (deterministic: names are assigned
/// in scan order, unique within the program).
fn materialize(stmts: &mut Vec<IrStmt>, n: &mut usize) -> bool {
    let mut changed = false;
    for s in stmts.iter_mut() {
        changed |= materialize_stmt(s, n);
    }
    changed
}

fn materialize_stmt(st: &mut IrStmt, n: &mut usize) -> bool {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = materialize_expr(cond, n);
            c |= materialize(then, n);
            for (ec, eb) in elsifs.iter_mut() {
                c |= materialize_expr(ec, n);
                c |= materialize(eb, n);
            }
            c |= materialize(else_, n);
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = materialize_expr(iter, n);
            c |= materialize(body, n);
            c
        }
        IrStmt::While { cond, body, .. } => {
            let mut c = materialize_expr(cond, n);
            c |= materialize(body, n);
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut c = materialize(body, n);
            c |= materialize_expr(cond, n);
            c
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = materialize_expr(discriminant, n);
            for cl in clauses.iter_mut() {
                c |= materialize(&mut cl.body, n);
            }
            c
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => materialize(b, n),
        IrStmt::Function { body, .. } => materialize(body, n),
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stg in stages.iter_mut() {
                c |= materialize(stg, n);
            }
            c
        }
        IrStmt::Expr(e) => materialize_expr(e, n),
        IrStmt::Assign { expr, .. } => materialize_expr(expr, n),
        IrStmt::Declare { init, .. } => init
            .as_mut()
            .map(|i| materialize_expr(i, n))
            .unwrap_or(false),
        IrStmt::Output { value, .. } => materialize_expr(value, n),
        IrStmt::WriteFile { path, content, .. } => {
            materialize_expr(path, n) | materialize_expr(content, n)
        }
        IrStmt::Exec { cmd, args, env, .. } => {
            let mut c = materialize_expr(cmd, n);
            for a in args.iter_mut() {
                c |= materialize_expr(a, n);
            }
            for (_, v) in env.iter_mut() {
                c |= materialize_expr(v, n);
            }
            c
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut c = materialize(inner, n);
            if redirects
                .iter()
                .any(|r| r.mode == "process-in" || r.mode == "process-out")
            {
                if let Some(replacement) = materialize_redirect(inner, redirects, n) {
                    *st = replacement;
                    c = true;
                }
            }
            c
        }
        _ => false,
    }
}

fn materialize_expr(e: &mut IrExpr, n: &mut usize) -> bool {
    match e {
        IrExpr::Arrow(stmts) => materialize(stmts, n),
        IrExpr::Call { func, args } if func == "redirect" => {
            // Expression-context process substitution: the call-form
            // `redirect(Arrow([exec …]), [spec …])` (the A1 shape for
            // command-substitution / pipeline contexts; see
            // `command_arrow_stmts`). Materialize the process specs the
            // same way as the statement form.
            let mut c = false;
            for a in args.iter_mut() {
                c |= materialize_expr(a, n);
            }
            c |= materialize_redirect_call(args, n);
            c
        }
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= materialize_expr(a, n);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => materialize_expr(lhs, n) | materialize_expr(rhs, n),
        IrExpr::Array(els) => {
            let mut c = false;
            for a in els.iter_mut() {
                c |= materialize_expr(a, n);
            }
            c
        }
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, v) in pairs.iter_mut() {
                c |= materialize_expr(v, n);
            }
            c
        }
        IrExpr::Index { key, .. } => materialize_expr(key, n),
        _ => false,
    }
}

// ── the rewrite ─────────────────────────────────────────────────────

/// Materialize one redirect wrapper's process substitutions. Returns the
/// replacement statement (a `Block`: mktemp(s) + producer write(s), the
/// original exec with tmp-path args appended / stdin redirects, the
/// process-out producer read(s), the cleanup `rm`), or `None` when the
/// shape is not materializable — the redirect list is left untouched
/// then.
fn materialize_redirect(
    inner: &mut Vec<IrStmt>,
    redirects: &mut Vec<IrRedirect>,
    n: &mut usize,
) -> Option<IrStmt> {
    // The inner must be a single appendable exec (an exec call or an
    // `IrStmt::Exec`); anything else is left for the renderer.
    let appendable = matches!(
        inner.as_slice(),
        [IrStmt::Expr(IrExpr::Call { func, .. })] if func == "exec"
    ) || matches!(inner.as_slice(), [IrStmt::Exec { .. }]);
    if !appendable {
        return None;
    }
    let cmd_name = exec_cmd_name(inner);
    let stdin_only = cmd_name.as_deref().map(stdin_only_command).unwrap_or(false);

    // Build everything BEFORE mutating the original stmt; any failure
    // leaves the redirect list intact.
    let mut pre: Vec<IrStmt> = Vec::new();
    let mut post: Vec<IrStmt> = Vec::new();
    let mut arg_paths: Vec<IrExpr> = Vec::new();
    let mut in_redirects: Vec<IrRedirect> = Vec::new();
    let mut cleanup: Vec<IrExpr> = Vec::new();
    for r in redirects.iter() {
        if r.mode != "process-in" && r.mode != "process-out" {
            continue;
        }
        let text = match &r.target {
            IrExpr::Str(s, _) => s.clone(),
            _ => return None, // non-string producer — leave untouched
        };
        let mut producer = parse_producer(&text)?;
        // nested process substitution inside the producer
        materialize(&mut producer, n);
        let tmp = format!("__ps_tmp{n}");
        *n += 1;
        let tmp_v = var(&tmp);
        pre.push(mktemp_assign(&tmp));
        cleanup.push(tmp_v.clone());
        if r.mode == "process-in" {
            // capture the producer's stdout into the temp file
            pre.push(producer_redirect(producer, &tmp_v, "w", Some(1)));
            if stdin_only {
                // mapfile/readarray cannot take a path arg — feed stdin
                in_redirects.push(IrRedirect {
                    fd: Some(0),
                    mode: "r".to_string(),
                    target: tmp_v,
                    interpolate: false,
                });
            } else {
                // bash passes the /dev/fd/N path as an argument
                arg_paths.push(tmp_v);
            }
        } else {
            // process-out: the exec writes the temp file; the producer
            // consumes it afterwards
            arg_paths.push(tmp_v.clone());
            post.push(producer_redirect(producer, &tmp_v, "r", Some(0)));
        }
    }
    if arg_paths.is_empty() && in_redirects.is_empty() {
        return None;
    }
    if !append_exec_args(inner, &arg_paths) {
        return None;
    }

    let mut exec_redirects: Vec<IrRedirect> = Vec::new();
    for r in redirects.drain(..) {
        if r.mode != "process-in" && r.mode != "process-out" {
            exec_redirects.push(r);
        }
    }
    exec_redirects.extend(in_redirects);
    let exec_stmt = if exec_redirects.is_empty() {
        let mut i = Vec::new();
        std::mem::swap(inner, &mut i);
        match i.len() {
            1 => i.pop().unwrap(),
            _ => IrStmt::Block(i),
        }
    } else {
        IrStmt::Redirect {
            inner: std::mem::take(inner),
            redirects: exec_redirects,
        }
    };

    let has_post = !post.is_empty();
    let mut out = pre;
    out.push(exec_stmt);
    out.extend(post);
    if cleanup.is_empty() {
        return Some(IrStmt::Block(out));
    }
    let rm = rm_exec(&cleanup);
    if !has_post {
        // process-in only: the trailing `rm` must not clobber the exec's
        // status (`diff <(a) <(b) || echo differ` — the `||` branches on
        // lastExit). Wrap as `and(Arrow([mktemp, writes, exec]),
        // Arrow([rm]))`: the rm runs ONLY when the exec succeeded, and
        // the stmt's status is the exec's in both branches (the runtime
        // and/or dispatch reads the arrow's last stmt — the exec — and
        // the rm's success is 0 = the exec's success). Renders in sh as
        // `{ …; exec; } && { rm …; }` — portable POSIX.
        let mut exec_part = out;
        Some(IrStmt::Expr(IrExpr::Call {
            func: "and".to_string(),
            args: vec![IrExpr::Arrow(exec_part), IrExpr::Arrow(vec![rm])],
        }))
    } else {
        // process-out present: the producer-read must run regardless of
        // the exec's status (it CONSUMES the file), so a plain sequence
        // stays; the trailing rm is the last status (accepted caveat).
        out.push(rm);
        Some(IrStmt::Block(out))
    }
}

/// Materialize process substitutions in the call-form redirect
/// (`redirect(Arrow([exec …]), [spec …])` — command-substitution and
/// pipeline contexts, see `command_arrow_stmts`). Rewrites the arrow in
/// place: mktemp + producer-write stmts are PREPENDED, the exec gains the
/// tmp-path args, process specs are dropped (or turned into a stdin `r`
/// spec for mapfile/readarray), process-out producer-reads and the `rm`
/// cleanup are APPENDED. Returns false (leaves everything untouched)
/// when any producer fails to parse or the exec shape is not found.
fn materialize_redirect_call(call_args: &mut [IrExpr], n: &mut usize) -> bool {
    let [IrExpr::Arrow(stmts), IrExpr::Array(specs)] = call_args else {
        return false;
    };
    // the redirect wraps a single exec call inside the arrow
    let exec_ok = matches!(
        stmts.as_slice(),
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec"
                && matches!(args.get(1), Some(IrExpr::Array(_)))
    );
    if !exec_ok {
        return false;
    }
    let cmd_name = match stmts.as_slice() {
        [IrStmt::Expr(IrExpr::Call { args, .. })] => match args.first() {
            Some(IrExpr::Str(s, _)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    };
    let stdin_only = cmd_name.as_deref().map(stdin_only_command).unwrap_or(false);

    // build everything BEFORE mutating; any failure leaves it untouched
    let mut pre: Vec<IrStmt> = Vec::new();
    let mut post: Vec<IrStmt> = Vec::new();
    let mut arg_paths: Vec<IrExpr> = Vec::new();
    let mut kept_specs: Vec<IrExpr> = Vec::new();
    let mut cleanup: Vec<IrExpr> = Vec::new();
    let mut materialized = false;
    for spec in specs.iter() {
        let IrExpr::Object(pairs) = spec else {
            kept_specs.push(spec.clone());
            continue;
        };
        let mode = pairs
            .iter()
            .find(|(k, _)| k == "mode")
            .and_then(|(_, v)| match v {
                IrExpr::Str(s, _) => Some(s.clone()),
                _ => None,
            });
        if !matches!(mode.as_deref(), Some("process-in" | "process-out")) {
            kept_specs.push(spec.clone());
            continue;
        }
        materialized = true;
        let text = pairs
            .iter()
            .find(|(k, _)| k == "target")
            .and_then(|(_, v)| match v {
                IrExpr::Str(s, _) => Some(s.clone()),
                _ => None,
            });
        let text = match text {
            Some(t) => t,
            None => return false,
        };
        let mut producer = match parse_producer(&text) {
            Some(p) => p,
            None => return false,
        };
        materialize(&mut producer, n);
        let tmp = format!("__ps_tmp{n}");
        *n += 1;
        let tmp_v = var(&tmp);
        pre.push(mktemp_assign(&tmp));
        cleanup.push(tmp_v.clone());
        if mode.as_deref() == Some("process-in") {
            pre.push(producer_redirect(producer, &tmp_v, "w", Some(1)));
            if stdin_only {
                // mapfile/readarray cannot take a path arg — feed stdin
                let mut pairs2: Vec<(String, IrExpr)> = Vec::new();
                for (k, v) in pairs {
                    if k == "mode" {
                        pairs2.push((k.clone(), st("r")));
                    } else if k == "target" {
                        pairs2.push((k.clone(), tmp_v.clone()));
                    } else {
                        pairs2.push((k.clone(), v.clone()));
                    }
                }
                kept_specs.push(IrExpr::Object(pairs2));
            } else {
                arg_paths.push(tmp_v);
            }
        } else {
            // process-out: the exec writes tmp; the producer reads it after
            arg_paths.push(tmp_v.clone());
            post.push(producer_redirect(producer, &tmp_v, "r", Some(0)));
        }
    }
    if !materialized {
        return false;
    }
    // commit: exec args gain the tmp paths
    if let [IrStmt::Expr(IrExpr::Call {
        args: call_args2, ..
    })] = stmts.as_mut_slice()
    {
        if let Some(IrExpr::Array(els)) = call_args2.get_mut(1) {
            els.extend(arg_paths.iter().cloned());
        }
    }
    *specs = kept_specs;
    let has_post = !post.is_empty();
    let mut seq = pre;
    seq.extend(stmts.drain(..));
    seq.extend(post);
    if cleanup.is_empty() {
        *stmts = seq;
        return true;
    }
    let rm = rm_exec(&cleanup);
    if has_post {
        // process-out present: the producer-read runs regardless of the
        // exec's status — plain sequence, rm last (status caveat).
        seq.push(rm);
        *stmts = seq;
    } else {
        // process-in only: `and(Arrow([mktemp, writes, exec]),
        // Arrow([rm]))` — the rm runs only on the exec's success, so the
        // enclosing status dispatch sees the exec's code either way.
        *stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "and".to_string(),
            args: vec![IrExpr::Arrow(seq), IrExpr::Arrow(vec![rm])],
        })];
    }
    true
}

/// The command name of the wrapped exec (for the stdin-only decision).
fn exec_cmd_name(inner: &[IrStmt]) -> Option<String> {
    match inner {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "exec" => match args.first() {
            Some(IrExpr::Str(s, _)) => Some(s.clone()),
            _ => None,
        },
        [IrStmt::Exec { cmd, .. }] => match cmd {
            IrExpr::Str(s, _) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Append the materialized temp-file paths to the exec's argument list
/// (bash's `/dev/fd/N` argument position; trailing position — the same
/// convention as the estree pre-pass). Returns false when the arg list
/// cannot be found (aborts the rewrite).
fn append_exec_args(inner: &mut [IrStmt], args: &[IrExpr]) -> bool {
    match inner {
        [IrStmt::Expr(IrExpr::Call {
            func,
            args: call_args,
        })] if func == "exec" => match call_args.get_mut(1) {
            Some(IrExpr::Array(els)) => {
                els.extend(args.iter().cloned());
                true
            }
            _ => false,
        },
        [IrStmt::Exec {
            args: exec_args, ..
        }] => {
            exec_args.extend(args.iter().cloned());
            true
        }
        _ => false,
    }
}

// ── tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    fn lower(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "transform changed nothing");
        crate::shir_json::shir_to_shir_json(&prog)
    }

    fn lower_untouched(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(!transform(&mut prog.stmts), "transform should be a no-op");
        crate::shir_json::shir_to_shir_json(&prog)
    }

    #[test]
    fn process_in_materializes_to_temp_file() {
        let json = lower("grep -f <(echo hi)");
        // mktemp capture assign
        assert!(json.contains("\"__ps_tmp0\""), "{json}");
        assert!(json.contains("\"mktemp\""), "{json}");
        // producer write: `echo hi` with stdout -> tmp
        assert!(json.contains("\"echo\""), "{json}");
        assert!(json.contains("\"w\""), "{json}");
        // the exec keeps its args and gains the tmp path arg
        assert!(json.contains("\"grep\""), "{json}");
        assert!(json.contains("\"-f\""), "{json}");
        // cleanup
        assert!(json.contains("\"rm\""), "{json}");
        // no process-in mode survives
        assert!(!json.contains("process-in"), "{json}");
    }

    #[test]
    fn two_process_ins_get_two_temps() {
        let json = lower("diff <(a) <(b)");
        assert!(json.contains("\"__ps_tmp0\""), "{json}");
        assert!(json.contains("\"__ps_tmp1\""), "{json}");
        assert!(!json.contains("process-in"), "{json}");
    }

    #[test]
    fn process_out_runs_producer_after() {
        let json = lower("tee > >(cat)");
        assert!(json.contains("\"__ps_tmp0\""), "{json}");
        // the producer reads the temp file after the exec
        assert!(json.contains("\"cat\""), "{json}");
        assert!(json.contains("\"mode\":\"r\""), "{json}");
        assert!(!json.contains("process-out"), "{json}");
    }

    #[test]
    fn mapfile_gets_stdin_redirect_not_arg() {
        let json = lower("mapfile -t arr < <(echo x)");
        // stdin redirect form: mode "r" on the temp file, NO appended arg
        assert!(json.contains("\"mode\":\"r\""), "{json}");
        assert!(!json.contains("process-in"), "{json}");
    }

    #[test]
    fn nested_producer_is_materialized() {
        let json = lower("diff <(sort <(echo y)) z");
        assert!(json.contains("\"__ps_tmp0\""), "{json}");
        assert!(json.contains("\"__ps_tmp1\""), "{json}");
        assert!(!json.contains("process-in"), "{json}");
    }

    #[test]
    fn no_process_substitution_is_noop() {
        lower_untouched("echo hello world");
        lower_untouched("grep -f file < input");
        lower_untouched("for i in 1 2 3; do echo $i; done");
    }

    #[test]
    fn deterministic_across_runs() {
        let src = "diff <(a) <(b)";
        let j1 = lower(src);
        let j2 = lower(src);
        assert_eq!(j1, j2);
    }

    #[test]
    fn producer_debug_leak_leaves_redirect() {
        // a producer command kind `command_to_shell_text` cannot express
        // (a For loop) leaks a Debug string with the SH2GLOB magic byte —
        // the transform refuses it and the redirect survives untouched
        let commands =
            parse_commands_from_text("grep -f <(for i in 1; do echo $i; done)").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        let before = crate::shir_json::shir_to_shir_json(&prog);
        let changed = transform(&mut prog.stmts);
        let after = crate::shir_json::shir_to_shir_json(&prog);
        assert_eq!(
            before, after,
            "unmaterializable producer must not be touched"
        );
        assert!(!changed);
    }

    #[test]
    fn capture_context_is_materialized() {
        // `x=$(diff <(a) <(b))` — the capture arrow's stmts
        let json = lower("x=$(diff <(a) <(b))");
        assert!(json.contains("\"__ps_tmp0\""), "{json}");
        assert!(!json.contains("process-in"), "{json}");
    }
}
