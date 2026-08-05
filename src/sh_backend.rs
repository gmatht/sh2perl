//! sh_backend — PURE sh renderer (worktree-local, branch `backend/sh`).
//!
//! Consumes the ShIR (the A1 contract) in-process and emits idiomatic
//! `sh` source. The source language is bash, so constructs with no POSIX
//! equivalent (arrays, `(( ))`, `[[ ]]`, `${x^^}`, `<<<`, `shopt`) render
//! in their native bash form; everything else renders POSIX.
//!
//! The corpus gate (`setup_backends.sh --backend-gate sh`) runs
//! `sh_backend <file.sh>` per example and requires exit 0 + valid sh on
//! stdout with no sh2.*/TODO stub markers — so every construct lowers to
//! REAL shell syntax (no stubs).

use crate::ir::{ArithAst, BinOpKind, InterpPart, IrExpr, IrProgram, IrRedirect, IrStmt, StrStyle};

/// Marker prefixes the core's lowering tags unquoted glob / process-
/// substitution words with (see shir.rs). The native shell performs both
/// natively, so the renderer strips the markers and emits the raw text.
const GLOB_MAGIC: &str = "\u{1}SH2GLOB\u{1}";
const PS_MAGIC: &str = "\u{1}SH2PS\u{1}";

/// Render a ShIR program to `sh` source. `Err` on a construct outside the
/// renderable subset (the gate reports it as a FAIL).
pub fn shir_to_sh(prog: &IrProgram) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("#!/bin/sh\n");
    for st in &prog.stmts {
        stmt_to_sh(st, 0, &mut out)?;
    }
    for sub in &prog.subs {
        out.push('\n');
        out.push_str(&sub.name);
        out.push_str("() {\n");
        for st in &sub.body {
            stmt_to_sh(st, 1, &mut out)?;
        }
        out.push_str("}\n");
    }
    Ok(out)
}

fn indent(out: &mut String, d: usize) {
    for _ in 0..d {
        out.push_str("    ");
    }
}

// ── statements (block form, newline-terminated) ──────────────────────

fn stmt_to_sh(st: &IrStmt, d: usize, out: &mut String) -> Result<(), String> {
    match st {
        IrStmt::Expr(e) => {
            indent(out, d);
            out.push_str(&cmd_to_sh(e)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::Assign { targets, expr } => {
            indent(out, d);
            out.push_str(&assign_to_sh(targets, expr)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            indent(out, d);
            out.push_str("if ");
            out.push_str(&cmd_to_sh(cond)?);
            out.push_str("; then\n");
            for b in then {
                stmt_to_sh(b, d + 1, out)?;
            }
            for (econd, ebody) in elsifs {
                indent(out, d);
                out.push_str("elif ");
                out.push_str(&cmd_to_sh(econd)?);
                out.push_str("; then\n");
                for b in ebody {
                    stmt_to_sh(b, d + 1, out)?;
                }
            }
            if !else_.is_empty() {
                indent(out, d);
                out.push_str("else\n");
                for b in else_ {
                    stmt_to_sh(b, d + 1, out)?;
                }
            }
            indent(out, d);
            out.push_str("fi\n");
            Ok(())
        }
        IrStmt::For { var, iter, body } => {
            indent(out, d);
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            out.push_str(&for_items_to_sh(iter)?);
            out.push_str("; do\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("done\n");
            Ok(())
        }
        IrStmt::While { cond, body } => {
            indent(out, d);
            out.push_str("while ");
            out.push_str(&cmd_to_sh(cond)?);
            out.push_str("; do\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("done\n");
            Ok(())
        }
        IrStmt::DoWhile { body, cond, until } => {
            indent(out, d);
            out.push_str("while :; do\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d + 1);
            if *until {
                out.push_str("if ");
            } else {
                out.push_str("if ! ");
            }
            out.push_str(&cmd_to_sh(cond)?);
            out.push_str("; then break; fi\n");
            indent(out, d);
            out.push_str("done\n");
            Ok(())
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            indent(out, d);
            out.push_str("case ");
            out.push_str(&word_to_sh(discriminant)?);
            out.push_str(" in\n");
            for cl in clauses {
                for pat in &cl.patterns {
                    indent(out, d + 1);
                    out.push_str(pat);
                    out.push_str(")\n");
                }
                for b in &cl.body {
                    stmt_to_sh(b, d + 2, out)?;
                }
                indent(out, d + 1);
                out.push_str(";;\n");
            }
            indent(out, d);
            out.push_str("esac\n");
            Ok(())
        }
        IrStmt::Function { name, body } => {
            indent(out, d);
            out.push_str(name);
            out.push_str("() {\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        IrStmt::Redirect { inner, redirects } => {
            let suffix = redirects_to_sh(redirects)?;
            if inner.len() == 1 {
                if let IrStmt::Expr(e) = &inner[0] {
                    indent(out, d);
                    out.push_str(&cmd_to_sh(e)?);
                    out.push_str(&suffix);
                    out.push('\n');
                    return Ok(());
                }
            }
            // compound inner: inline it, then the redirects apply to the group
            indent(out, d);
            out.push_str(&stmts_inline(inner)?);
            out.push_str(&suffix);
            out.push('\n');
            Ok(())
        }
        IrStmt::Subshell(body) => {
            indent(out, d);
            out.push_str("(\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str(")\n");
            Ok(())
        }
        IrStmt::Background(body) => {
            if body.len() == 1 {
                if let IrStmt::Expr(e) = &body[0] {
                    indent(out, d);
                    out.push_str(&cmd_to_sh(e)?);
                    out.push_str(" &\n");
                    return Ok(());
                }
            }
            indent(out, d);
            out.push_str("(\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str(") &\n");
            Ok(())
        }
        IrStmt::Block(body) => {
            indent(out, d);
            out.push_str("{\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("}\n");
            Ok(())
        }
        IrStmt::Return(e) => {
            indent(out, d);
            out.push_str("return");
            if let Some(x) = e {
                out.push(' ');
                out.push_str(&word_to_sh(x)?);
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::Exit(e) => {
            indent(out, d);
            out.push_str("exit");
            if let Some(x) = e {
                out.push(' ');
                out.push_str(&word_to_sh(x)?);
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::Die { expr, .. } => {
            indent(out, d);
            out.push_str("echo ");
            out.push_str(&word_to_sh(expr)?);
            out.push_str(" >&2\n");
            indent(out, d);
            out.push_str("exit 1\n");
            Ok(())
        }
        IrStmt::Warn { expr, .. } => {
            indent(out, d);
            out.push_str("echo ");
            out.push_str(&word_to_sh(expr)?);
            out.push_str(" >&2\n");
            Ok(())
        }
        IrStmt::Declare { vars, init, local } => {
            indent(out, d);
            if *local {
                out.push_str("local ");
            }
            for (i, v) in vars.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&v.name);
                if let Some(init) = init {
                    out.push('=');
                    out.push_str(&word_to_sh(init)?);
                }
            }
            out.push('\n');
            Ok(())
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            indent(out, d);
            out.push_str(var);
            out.push('=');
            out.push_str(&array_literal_to_sh(elements)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::Output { value, newline, .. } => {
            indent(out, d);
            if *newline {
                out.push_str("printf '%s\\n' ");
            } else {
                out.push_str("printf '%s' ");
            }
            out.push_str(&word_to_sh(value)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::Exec {
            cmd,
            args,
            capture,
            redirects,
            env,
        } => {
            let mut line = exec_line_to_sh(cmd, args, Some(env))?;
            if !redirects.is_empty() {
                line.push_str(&redirect_objs_to_sh(redirects)?);
            }
            if let Some(var) = capture {
                indent(out, d);
                out.push_str(var);
                out.push_str("=$(");
                out.push_str(&line);
                out.push_str(")\n");
            } else {
                indent(out, d);
                out.push_str(&line);
                out.push('\n');
            }
            Ok(())
        }
        IrStmt::Pipeline {
            stages,
            capture,
            ..
        } => {
            let mut line = String::new();
            for (i, stg) in stages.iter().enumerate() {
                if i > 0 {
                    line.push_str(" | ");
                }
                line.push_str(&stmts_inline(stg)?);
            }
            if let Some(var) = capture {
                indent(out, d);
                out.push_str(var);
                out.push_str("=$(");
                out.push_str(&line);
                out.push_str(")\n");
            } else {
                indent(out, d);
                out.push_str(&line);
                out.push('\n');
            }
            Ok(())
        }
        IrStmt::WriteFile {
            path,
            content,
            append,
        } => {
            indent(out, d);
            if *append {
                out.push_str("printf '%s' ");
            } else {
                out.push_str("printf '%s' ");
            }
            out.push_str(&word_to_sh(content)?);
            out.push_str(if *append { " >> " } else { " > " });
            out.push_str(&word_to_sh(path)?);
            out.push('\n');
            Ok(())
        }
        IrStmt::SetChildError(_) | IrStmt::Require(_) | IrStmt::RawText(_) => Ok(()),
    }
}

/// `var=value` for a statement-level assignment. Handles the sh2.* RHS
/// forms (capture, pipeline, arith, setArray, assign) natively.
fn assign_to_sh(targets: &[crate::ir::AssignTarget], expr: &IrExpr) -> Result<String, String> {
    let mut out = String::new();
    for (i, t) in targets.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&t.var);
        for idx in &t.indices {
            out.push('[');
            out.push_str(&word_to_sh(idx)?);
            out.push(']');
        }
        out.push('=');
        out.push_str(&assign_rhs_to_sh(expr)?);
    }
    Ok(out)
}

fn assign_rhs_to_sh(expr: &IrExpr) -> Result<String, String> {
    match expr {
        IrExpr::Call { func, args } => match func.as_str() {
            "capture" | "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),
            "pipeline" => {
                let stages = pipeline_stages(args)?;
                let mut line = String::new();
                for (i, stg) in stages.iter().enumerate() {
                    if i > 0 {
                        line.push_str(" | ");
                    }
                    line.push_str(&stmts_inline(stg)?);
                }
                Ok(format!("$({line})"))
            }
            "arith" => Ok(format!("$(({}))", raw_arg(args, 0)?)),
            "setArray" => {
                let name = raw_arg(args, 0)?;
                let items = array_items(args, 1)?;
                Ok(format!("{name}=({items})"))
            }
            "setArrayAppend" => {
                let name = raw_arg(args, 0)?;
                let items = array_items(args, 1)?;
                Ok(format!("{name}+=({items})"))
            }
            "assign" => {
                let name = raw_arg(args, 0)?;
                let op = raw_arg(args, 1)?;
                let value = word_to_sh(arg(args, 2)?)?;
                Ok(format!("{name}{op}{value}"))
            }
            _ => word_to_sh(expr),
        },
        _ => word_to_sh(expr),
    }
}

// ── command-position expressions ─────────────────────────────────────

fn cmd_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "exec" => {
                let words = match args.get(1) {
                    Some(IrExpr::Array(items)) => items.as_slice(),
                    _ => &[],
                };
                let env = match args.get(2) {
                    Some(IrExpr::Object(envs)) => Some(envs.as_slice()),
                    _ => None,
                };
                exec_line_to_sh(arg(args, 0)?, words, env)
            }
            "test" => {
                let t = raw_arg(args, 0)?;
                let t = t.trim();
                // `[[ ... ]]`-style compound tests survive as raw text; `[ ]`
                // cannot express &&/||, so keep those in [[ ]] form.
                if t.contains("&&") || t.contains("||") {
                    Ok(format!("[[ {t} ]]"))
                } else {
                    Ok(format!("[ {t} ]"))
                }
            }
            "pipeline" => {
                let stages = pipeline_stages(args)?;
                let mut line = String::new();
                for (i, stg) in stages.iter().enumerate() {
                    if i > 0 {
                        line.push_str(" | ");
                    }
                    line.push_str(&stmts_inline(stg)?);
                }
                Ok(line)
            }
            "redirect" => {
                let inner = arrow_to_sh(args)?;
                let specs = redirect_specs(args, 1)?;
                Ok(format!("{inner}{specs}"))
            }
            "subshell" => Ok(format!("( {} )", arrow_to_sh(args)?)),
            "block" => {
                let body = arrow_to_sh(args)?;
                if body.is_empty() {
                    Ok("{ :; }".into())
                } else {
                    Ok(format!("{{ {body}; }}"))
                }
            }
            "whileLoop" => {
                let cond = arrow_at(args, 0)?;
                let body = arrow_at(args, 1)?;
                Ok(format!("while {cond}; do {body}; done"))
            }
            "cstyleFor" => {
                let arith = raw_arg(args, 0)?;
                let body = arrow_at(args, 1)?;
                Ok(format!("for (( {arith} )); do {body}; done"))
            }
            "shopt" => {
                let opt = raw_arg(args, 0)?;
                let enable = match args.get(1) {
                    Some(IrExpr::Bool(b)) => *b,
                    _ => true,
                };
                Ok(if enable {
                    format!("shopt -s {opt}")
                } else {
                    format!("shopt -u {opt}")
                })
            }
            "arith" => Ok(format!("(( {} ))", raw_arg(args, 0)?)),
            "break" => Ok("break".into()),
            "continue" => Ok("continue".into()),
            "return" => {
                if args.is_empty() {
                    Ok("return".into())
                } else {
                    Ok(format!("return {}", word_to_sh(arg(args, 0)?)?))
                }
            }
            "assign" => {
                let name = raw_arg(args, 0)?;
                let op = raw_arg(args, 1)?;
                let value = word_to_sh(arg(args, 2)?)?;
                Ok(format!("{name}{op}{value}"))
            }
            "setArray" => {
                let name = raw_arg(args, 0)?;
                let items = array_items(args, 1)?;
                Ok(format!("{name}=({items})"))
            }
            "setArrayAppend" => {
                let name = raw_arg(args, 0)?;
                let items = array_items(args, 1)?;
                Ok(format!("{name}+=({items})"))
            }
            "getVar" => Ok(var_ref_to_sh(&raw_arg(args, 0)?, false)),
            "capture" | "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),
            "contains" => {
                let arg = word_to_sh(arg(args, 0)?)?;
                let pat = raw_arg(args, 1)?;
                Ok(format!("printf '%s\\n' {arg} | grep {pat} >/dev/null 2>&1"))
            }
            "fnCall" => {
                let name = raw_arg(args, 0)?;
                let mut line = name;
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        line.push(' ');
                        line.push_str(&word_to_sh(w)?);
                    }
                }
                Ok(line)
            }
            other => Err(format!("command call not renderable: {other:?}")),
        },
        IrExpr::BinOp {
            op: BinOpKind::And,
            lhs,
            rhs,
        } => Ok(format!("{} && {}", cmd_to_sh(lhs)?, cmd_to_sh(rhs)?)),
        IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs,
            rhs,
        } => Ok(format!("{} || {}", cmd_to_sh(lhs)?, cmd_to_sh(rhs)?)),
        IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs,
            ..
        } => Ok(format!("! {}", cmd_to_sh(lhs)?)),
        other => Err(format!("command expression not renderable: {other:?}")),
    }
}

fn exec_line_to_sh(cmd: &IrExpr, args: &[IrExpr], env: Option<&[(String, IrExpr)]>) -> Result<String, String> {
    let mut out = String::new();
    if let Some(envs) = env {
        for (k, v) in envs {
            out.push_str(k);
            out.push('=');
            out.push_str(&word_to_sh(v)?);
            out.push(' ');
        }
    }
    let cmd_name = match cmd {
        IrExpr::Str(s, _) => Some(s.as_str()),
        _ => None,
    };
    // bash-only `set` options (pipefail etc.) — dash would reject them;
    // drop the option word and the trailing `o` of a combined flag (the
    // corpus's `set -euo pipefail` must run under /bin/sh for the
    // equivalence gate).
    if cmd_name == Some("set") {
        const DASH_SET_O: &[&str] = &[
            "allexport", "errexit", "noglob", "noclobber", "nolog", "notify", "ignoreeof",
            "monitor", "nounset", "verbose", "vi", "xtrace",
        ];
        let mut kept: Vec<String> = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let lit = match &args[i] {
                IrExpr::Str(s, _) => s.clone(),
                _ => {
                    kept.push(word_to_sh(&args[i])?);
                    i += 1;
                    continue;
                }
            };
            if lit == "-o" || (lit.starts_with('-') && lit.len() > 1 && lit.ends_with('o')) {
                let opt = args.get(i + 1).and_then(|x| match x {
                    IrExpr::Str(s, _) => Some(s.as_str()),
                    _ => None,
                });
                if let Some(opt) = opt {
                    if DASH_SET_O.contains(&opt) {
                        // supported: keep `-o opt` (or the split flag)
                        if lit == "-o" {
                            kept.push("-o".into());
                            kept.push(opt.into());
                        } else {
                            kept.push(format!("-{}", &lit[1..lit.len() - 1]));
                            kept.push("-o".into());
                            kept.push(opt.into());
                        }
                        i += 2;
                        continue;
                    }
                    // unsupported (pipefail…): drop the option word, keep
                    // the OTHER flags of a combined form (`-euo` → `-eu`)
                    if lit != "-o" {
                        let rest = &lit[1..lit.len() - 1];
                        if !rest.is_empty() {
                            kept.push(format!("-{rest}"));
                        }
                    }
                    i += 2;
                    continue;
                }
                // `-o` with no option word — keep verbatim
            }
            kept.push(lit);
            i += 1;
        }
        if kept.is_empty() {
            out.push(':');
        } else {
            out.push_str("set ");
            out.push_str(&kept.join(" "));
        }
        return Ok(out);
    }
    // `declare`/`typeset` are bash-only builtins (dash rejects them);
    // translate the POSIX-representable forms: `declare x=1` -> `x=1`,
    // `-r` -> readonly, `-x` -> export, `-i`/`-a`/`-A`/`-u`/`-l`/`-n` flags
    // are dropped (dash has no variable attributes — the assignment still
    // happens, which keeps stdout identical for the equivalence gate).
    if matches!(cmd_name, Some("declare" | "typeset")) {
        let mut words: Vec<String> = Vec::new();
        let mut flags: Vec<String> = Vec::new();
        for a in args {
            match a {
                IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                    flags.push(s.clone());
                }
                other => words.push(word_to_sh(other)?),
            }
        }
        let prefix = if flags.iter().any(|f| f.contains('r')) {
            "readonly "
        } else if flags.iter().any(|f| f.contains('x')) {
            "export "
        } else {
            ""
        };
        if words.is_empty() {
            out.push(':');
        } else {
            out.push_str(prefix);
            out.push_str(&words.join(" "));
        }
        return Ok(out);
    }
    // `local`/`export`/`readonly` with bash-only type flags: drop the flags
    // (`local -i x=5` -> `local x=5`), keep the builtin.
    if matches!(cmd_name, Some("local" | "export" | "readonly")) {
        let mut words: Vec<String> = Vec::new();
        for a in args {
            match a {
                IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {}
                other => words.push(word_to_sh(other)?),
            }
        }
        if words.is_empty() {
            out.push_str(cmd_name.unwrap());
        } else {
            out.push_str(cmd_name.unwrap());
            out.push(' ');
            out.push_str(&words.join(" "));
        }
        return Ok(out);
    }
    // `echo -n` / `echo -e` / `echo -E` — dash's echo has no flags; render
    // the equivalent printf so stdout matches bash.
    if cmd_name == Some("echo") {
        if let Some(first) = args.first() {
            if let IrExpr::Str(s, _) = first {
                let rest = &args[1..];
                let mut words = Vec::new();
                for a in rest {
                    words.push(word_to_sh(a)?);
                }
                let joined = words.join(" ");
                match s.as_str() {
                    "-n" => {
                        // bash: args joined by a single space, no newline
                        if rest.is_empty() {
                            return Ok("printf '%s'".into());
                        }
                        let fmt = rest
                            .iter()
                            .map(|_| "%s")
                            .collect::<Vec<_>>()
                            .join(" ");
                        return Ok(format!("printf '{fmt}' {joined}"));
                    }
                    "-e" => {
                        // bash: backslash escapes interpreted, trailing newline
                        if rest.is_empty() {
                            return Ok("printf '\n'".into());
                        }
                        let fmt = rest
                            .iter()
                            .map(|_| "%b")
                            .collect::<Vec<_>>()
                            .join(" ");
                        return Ok(format!("printf '{fmt}\n' {joined}"));
                    }
                    "-E" => {
                        // bash: escapes NOT interpreted, trailing newline
                        if rest.is_empty() {
                            return Ok("printf '\n'".into());
                        }
                        let fmt = rest
                            .iter()
                            .map(|_| "%s")
                            .collect::<Vec<_>>()
                            .join(" ");
                        return Ok(format!("printf '{fmt}\n' {joined}"));
                    }
                    _ => {}
                }
            }
        }
    }
    out.push_str(&word_to_sh(cmd)?);
    for w in args {
        out.push(' ');
        out.push_str(&word_to_sh(w)?);
    }
    Ok(out)
}

// ── redirects ────────────────────────────────────────────────────────

fn redirects_to_sh(redirects: &[IrRedirect]) -> Result<String, String> {
    let mut out = String::new();
    for r in redirects {
        out.push_str(&redirect_to_sh(r)?);
    }
    Ok(out)
}

fn redirect_to_sh(r: &IrRedirect) -> Result<String, String> {
    let fd = r.fd.unwrap_or(0);
    let op = match r.mode.as_str() {
        "w" => ">",
        "a" => ">>",
        "r" => "<",
        "r+" => "<>",
        "herestring" => "<<<",
        "heredoc" | "heredoc-tabs" => {
            // The heredoc BODY is carried in the target string.
            let body = match &r.target {
                IrExpr::Str(s, _) => s.clone(),
                other => word_to_sh(other)?,
            };
            let delim = pick_delimiter(&body);
            let q = if r.interpolate { "" } else { "'" };
            let tab = if r.mode == "heredoc-tabs" { "-" } else { "" };
            let fdpre = fd_prefix(fd);
            return Ok(format!(
                " {fdpre}<<{tab}{q}{delim}{q}\n{body}{delim}\n"
            ));
        }
        "unsupported" => return Ok(String::new()), // process substitution — dropped in the IR
        other => return Err(format!("redirect mode not renderable: {other:?}")),
    };
    let fdpre = fd_prefix(fd);
    let target = match &r.target {
        // `2>&1` dup forms — the target arrives as a literal `&N`
        IrExpr::Str(s, _) if s.starts_with('&') => s.clone(),
        other => word_to_sh(other)?,
    };
    Ok(format!(" {fdpre}{op}{target}"))
}

/// Redirect specs as Object([(fd,Int),(mode,Str),(target,word)]) — the
/// command-position `redirect` call form.
fn redirect_specs(args: &[IrExpr], idx: usize) -> Result<String, String> {
    let Some(IrExpr::Array(specs)) = args.get(idx) else {
        return Ok(String::new());
    };
    redirect_objs_to_sh(specs)
}

/// Redirect spec objects — `Object([(fd,Int),(mode,Str),(target,word)])`
/// (the `IrStmt::Exec` redirects field and the call-form spec list).
fn redirect_objs_to_sh(specs: &[IrExpr]) -> Result<String, String> {
    let mut out = String::new();
    for spec in specs {
        let IrExpr::Object(props) = spec else {
            return Err(format!("redirect spec not an Object: {spec:?}"));
        };
        let mut fd: Option<i64> = None;
        let mut mode = String::new();
        let mut target: Option<&IrExpr> = None;
        let mut interpolate = true;
        for (k, v) in props {
            match (k.as_str(), v) {
                ("fd", IrExpr::Int(n)) => fd = Some(*n),
                ("mode", IrExpr::Str(m, _)) => mode = m.clone(),
                ("target", t) => target = Some(t),
                ("interpolate", IrExpr::Bool(b)) => interpolate = *b,
                _ => {}
            }
        }
        let r = IrRedirect {
            fd: fd.map(|n| n as i32),
            mode,
            target: target
                .cloned()
                .unwrap_or(IrExpr::Str(String::new(), StrStyle::DoubleQuoted)),
            interpolate,
        };
        out.push_str(&redirect_to_sh(&r)?);
    }
    Ok(out)
}

fn fd_prefix(fd: i32) -> String {
    match fd {
        0 | 1 => String::new(),
        n => n.to_string(),
    }
}

fn pick_delimiter(body: &str) -> &'static str {
    for d in ["EOF", "_EOF", "SH2_EOF", "SH2EOF", "END"] {
        if !body.lines().any(|l| l.trim_end() == d) {
            return d;
        }
    }
    "SH2EOF"
}

// ── words ────────────────────────────────────────────────────────────

fn word_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Str(s, _) => Ok(str_word(s)),
        IrExpr::Int(i) => Ok(i.to_string()),
        IrExpr::Var(name, _) => Ok(format!("${name}")),
        IrExpr::Ident(name) => Ok(name.clone()),
        IrExpr::Bool(b) => Ok(if *b { "1".into() } else { "0".into() }),
        IrExpr::Interpolate(parts) => interp_to_sh(parts),
        IrExpr::Arith(a) => Ok(format!("$(({}))", arith_to_sh(a))),
        IrExpr::Call { func, args } => call_word_to_sh(func, args),
        IrExpr::Json(v) => Ok(json_str(v)),
        other => Err(format!("word not renderable: {other:?}")),
    }
}

fn call_word_to_sh(func: &str, args: &[IrExpr]) -> Result<String, String> {
    match func {
        "getVar" => Ok(var_ref_to_sh(&raw_arg(args, 0)?, false)),
        "param" => param_to_sh(args, false),
        "listVar" => {
            let n = raw_arg(args, 0)?;
            Ok(if n == "*" { "\"$*\"".into() } else { "\"$@\"".into() })
        }
        "arrayIndex" => Ok(format!(
            "${{{}{}}}",
            raw_arg(args, 0)?,
            word_to_sh(arg(args, 1)?)?
        )),
        "arrayItems" => Ok(format!("${{!{}[@]}}", raw_arg(args, 0)?)),
        "arrayLen" => Ok(format!("${{#{}[@]}}", raw_arg(args, 0)?)),
        "capture" | "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),
        "arith" => Ok(format!("$(({}))", raw_arg(args, 0)?)),
        "brace" => brace_to_sh(args),
        "join" => join_to_sh(arg(args, 0)?, false),
        "setArray" => {
            let name = raw_arg(args, 0)?;
            let items = array_items(args, 1)?;
            Ok(format!("{name}=({items})"))
        }
        "setArrayAppend" => {
            let name = raw_arg(args, 0)?;
            let items = array_items(args, 1)?;
            Ok(format!("{name}+=({items})"))
        }
        "assign" => {
            let name = raw_arg(args, 0)?;
            let op = raw_arg(args, 1)?;
            let value = word_to_sh(arg(args, 2)?)?;
            Ok(format!("{name}{op}{value}"))
        }
        other => Err(format!("word call not renderable: {other:?}")),
    }
}

/// `${name}`-family rendering. `list` selects the list form (join
/// context, `"${arr[@]}"`).
fn var_ref_to_sh(name: &str, list: bool) -> String {
    if list {
        if name == "@" || name == "*" {
            return format!("${{{name}}}");
        }
        return format!("${{{name}[@]}}");
    }
    // special parameters / positionals: `$?` `$!` `$@` `$1` …
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "?!@*#$-_".contains(c))
        && !name.starts_with('[')
    {
        format!("${name}")
    } else {
        // array elements / indexed names: `${x[i]}`
        format!("${{{name}}}")
    }
}

/// Parameter-expansion call: `param(op, name, extras...)` → `${...}`.
fn param_to_sh(args: &[IrExpr], list: bool) -> Result<String, String> {
    let op = raw_arg(args, 0)?;
    let name = raw_arg(args, 1)?;
    match op.as_str() {
        "" => {
            if list {
                // `"${x[@]}"` — array elements joined
                Ok(var_ref_to_sh(&name, true))
            } else {
                Ok(format!("${{{name}}}"))
            }
        }
        "len" => Ok(format!("${{#{name}}}")),
        "slice" => {
            let off = raw_arg(args, 2)?;
            let len = args
                .get(3)
                .map(str_arg)
                .transpose()?
                .unwrap_or_default();
            // `${#arr[@]}` — the parser keeps the `#` in the name
            if name.starts_with('#') {
                return Ok(format!("${{#{}[@]}}", &name[1..]));
            }
            if len.is_empty() {
                Ok(format!("${{{name}:{off}}}"))
            } else {
                Ok(format!("${{{name}:{off}:{len}}}"))
            }
        }
        "^^" | ",," | "^" => Ok(format!("${{{name}{op}}}")),
        "#" | "##" | "%" | "%%" => {
            let pat = raw_arg(args, 2)?;
            Ok(format!("${{{name}{op}{pat}}}"))
        }
        "//" => {
            let pat = raw_arg(args, 2)?;
            let rep = raw_arg(args, 3)?;
            Ok(format!("${{{name}//{pat}/{rep}}}"))
        }
        ":-" | ":=" | ":?" => {
            let default = raw_arg(args, 2)?;
            Ok(format!("${{{name}{op}{default}}}"))
        }
        "basename" => Ok(format!("${{{name}##*/}}")),
        "dirname" => Ok(format!("${{{name}%/*}}")),
        other => Err(format!("param op not renderable: {other:?}")),
    }
}

/// `join(x)` — the LIST form of an expansion (bash joins array elements
/// with spaces when quoted).
fn join_to_sh(inner: &IrExpr, quoted: bool) -> Result<String, String> {
    let s = match inner {
        IrExpr::Call { func, args } if func == "param" => param_to_sh(args, true)?,
        IrExpr::Call { func, args } if func == "arrayIndex" => {
            let name = raw_arg(args, 0)?;
            let key = raw_arg(args, 1)?;
            if key == "@" || key == "*" {
                var_ref_to_sh(&name, true)
            } else {
                format!("${{{name}[{key}]}}")
            }
        }
        IrExpr::Call { func, args } if func == "arrayItems" => {
            format!("${{!{}[@]}}", raw_arg(args, 0)?)
        }
        IrExpr::Call { func, args } if func == "arrayLen" => {
            format!("${{#{}[@]}}", raw_arg(args, 0)?)
        }
        _ => word_to_sh(inner)?,
    };
    Ok(if quoted { format!("\"{s}\"") } else { s })
}

fn interp_to_sh(parts: &[InterpPart]) -> Result<String, String> {
    // pure-literal interpolation → plain literal word
    if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) {
        let mut s = String::new();
        for p in parts {
            if let InterpPart::Lit(t) = p {
                s.push_str(t);
            }
        }
        return Ok(str_word(&s));
    }
    let mut out = String::from("\"");
    for p in parts {
        match p {
            InterpPart::Lit(t) => {
                for c in t.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '$' => out.push_str("\\$"),
                        '`' => out.push_str("\\`"),
                        '\\' => out.push_str("\\\\"),
                        c => out.push(c),
                    }
                }
            }
            InterpPart::Expr(x) => out.push_str(&interp_expr_to_sh(x)?),
        }
    }
    out.push('"');
    Ok(out)
}

/// An expansion inside a double-quoted template.
fn interp_expr_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => Ok(var_ref_to_sh(&raw_arg(args, 0)?, false)),
            "param" => param_to_sh(args, false),
            "listVar" => {
                let n = raw_arg(args, 0)?;
                Ok(if n == "*" { "$*".into() } else { "$@".into() })
            }
            "arrayIndex" => Ok(format!(
                "${{{}{}}}",
                raw_arg(args, 0)?,
                raw_arg(args, 1)?
            )),
            "arrayItems" => Ok(format!("${{!{}[@]}}", raw_arg(args, 0)?)),
            "arrayLen" => Ok(format!("${{#{}[@]}}", raw_arg(args, 0)?)),
            "capture" | "captureWords" => Ok(format!("$({})", arrow_to_sh(args)?)),
            "arith" => Ok(format!("$(({}))", raw_arg(args, 0)?)),
            "join" => join_to_sh(arg(args, 0)?, false),
            "brace" => brace_to_sh(args),
            other => Err(format!("interp call not renderable: {other:?}")),
        },
        IrExpr::Arith(a) => Ok(format!("$(({}))", arith_to_sh(a))),
        IrExpr::Int(i) => Ok(i.to_string()),
        IrExpr::Bool(b) => Ok(if *b { "1".into() } else { "0".into() }),
        IrExpr::Var(name, _) => Ok(format!("${name}")),
        IrExpr::Str(s, _) => Ok(s.clone()),
        other => Err(format!("interp expr not renderable: {other:?}")),
    }
}

/// Brace expansion: `brace(prefix, groups, middles, suffix)` — expand the
/// cross product at render time (POSIX sh has no brace expansion).
fn brace_to_sh(args: &[IrExpr]) -> Result<String, String> {
    let prefix = raw_arg(args, 0)?;
    let groups = brace_groups(args, 1)?;
    let middles = brace_middles(args, 2)?;
    let suffix = raw_arg(args, 3)?;

    // word = prefix g1 m1 g2 m2 … suffix; each group holds alternatives
    let mut results: Vec<String> = vec![String::new()];
    for (gi, group) in groups.iter().enumerate() {
        let mut next = Vec::new();
        for alt in group {
            for base in &results {
                next.push(format!("{base}{alt}"));
            }
        }
        results = next;
        if let Some(mid) = middles.get(gi) {
            for r in &mut results {
                r.push_str(mid);
            }
        }
    }
    let mut out = String::new();
    for (i, r) in results.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&str_word(&format!("{prefix}{r}{suffix}")));
    }
    Ok(out)
}

/// The groups JSON: `[[{range:[s,e,null,null]} | String, ...], ...]` —
/// each group is a list of ALTERNATIVES (a range expands to several).
fn brace_groups(args: &[IrExpr], idx: usize) -> Result<Vec<Vec<String>>, String> {
    let Some(IrExpr::Json(serde_json::Value::Array(groups))) = args.get(idx) else {
        return Err("brace groups not Json".into());
    };
    let mut out = Vec::new();
    for g in groups {
        let serde_json::Value::Array(items) = g else {
            return Err("brace group not Array".into());
        };
        let mut expanded = Vec::new();
        for it in items {
            expanded.extend(brace_item(it)?);
        }
        out.push(expanded);
    }
    Ok(out)
}

fn brace_middles(args: &[IrExpr], idx: usize) -> Result<Vec<String>, String> {
    let Some(IrExpr::Json(serde_json::Value::Array(mids))) = args.get(idx) else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for m in mids {
        match m {
            serde_json::Value::String(s) => out.push(s.clone()),
            other => return Err(format!("brace middle not String: {other:?}")),
        }
    }
    Ok(out)
}

/// A brace item: a literal string, a `{range:[start,end,step,?]}` — or a
/// `{nested: [...]}` group (its elements are alternatives) — returns the
/// alternatives (a range expands to each number).
fn brace_item(it: &serde_json::Value) -> Result<Vec<String>, String> {
    match it {
        serde_json::Value::String(s) => Ok(vec![s.clone()]),
        serde_json::Value::Object(o) => {
            if let Some(serde_json::Value::Array(range)) = o.get("range") {
                let start: i64 = range[0].as_str().unwrap_or("0").parse().unwrap_or(0);
                let end: i64 = range[1].as_str().unwrap_or("0").parse().unwrap_or(0);
                let step: i64 = match range.get(2) {
                    Some(serde_json::Value::String(s)) if !s.is_empty() => s.parse().unwrap_or(0),
                    _ => 0,
                };
                let mut out = Vec::new();
                if step > 0 {
                    let mut n = start;
                    while n <= end {
                        out.push(n.to_string());
                        n += step;
                    }
                } else if step < 0 {
                    let mut n = start;
                    while n >= end {
                        out.push(n.to_string());
                        n += step;
                    }
                } else if start <= end {
                    for n in start..=end {
                        out.push(n.to_string());
                    }
                } else {
                    for n in (end..=start).rev() {
                        out.push(n.to_string());
                    }
                }
                Ok(out)
            } else if let Some(serde_json::Value::Array(nested)) = o.get("nested") {
                let mut out = Vec::new();
                for el in nested {
                    out.extend(brace_item(el)?);
                }
                Ok(out)
            } else {
                Err(format!("brace item not understood: {it:?}"))
            }
        }
        other => Err(format!("brace item not understood: {other:?}")),
    }
}

/// A literal word with shell-aware quoting. Str values are RAW source
/// text: unquoted globs arrive GLOB_MAGIC-tagged (emit bare → the shell
/// globs natively), backtick text must execute, everything else is
/// single-quoted when shell-active.
fn str_word(s: &str) -> String {
    if let Some(rest) = s.strip_prefix(GLOB_MAGIC) {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix(PS_MAGIC) {
        return rest.to_string();
    }
    if s.is_empty() {
        return "''".into();
    }
    // backticks must execute (command substitution captured as literal)
    if s.contains('`') {
        return s.to_string();
    }
    // words with `$`/`\` (regex anchors, escape sequences) or shell
    // metacharacters → single-quote so the text stays literal
    if s.contains(['$', '"', '\'', '\\', ';', '&', '|', '(', ')', '<', '>', ' ', '\t', '\n', '='])
        || s.starts_with('#')
    {
        let mut q = String::from("'");
        for c in s.chars() {
            if c == '\'' {
                q.push_str("'\\''");
            } else {
                q.push(c);
            }
        }
        q.push('\'');
        return q;
    }
    // quoted-in-source glob chars (no GLOB_MAGIC) → literal
    if s.contains(['*', '?', '[']) {
        return format!("'{s}'");
    }
    s.to_string()
}

fn json_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => {
            if *b { "1".into() } else { "0".into() }
        }
        _ => v.to_string(),
    }
}

// ── arithmetic ───────────────────────────────────────────────────────

fn arith_to_sh(a: &ArithAst) -> String {
    match a {
        ArithAst::Num(n) => n.to_string(),
        ArithAst::Var(name) => name.clone(),
        ArithAst::Index { var, key } => format!("{var}[{}]", arith_to_sh(key)),
        ArithAst::Bin { op, lhs, rhs } => {
            format!("({} {op} {})", arith_to_sh(lhs), arith_to_sh(rhs))
        }
        ArithAst::Un { op, arg } => format!("({op}{})", arith_to_sh(arg)),
        ArithAst::Cond { test, then, else_ } => format!(
            "({} ? {} : {})",
            arith_to_sh(test),
            arith_to_sh(then),
            arith_to_sh(else_)
        ),
        ArithAst::Assign { var, op, rhs } => {
            format!("{var} {op}= {}", arith_to_sh(rhs))
        }
        ArithAst::IncDec {
            var,
            delta,
            prefix,
        } => {
            let d = if *delta > 0 { "++" } else { "--" };
            if *prefix {
                format!("{d}{var}")
            } else {
                format!("{var}{d}")
            }
        }
    }
}

// ── arrows / inline statement sequences ──────────────────────────────

fn arrow_to_sh(args: &[IrExpr]) -> Result<String, String> {
    arrow_at(args, 0)
}

fn arrow_at(args: &[IrExpr], idx: usize) -> Result<String, String> {
    match args.get(idx) {
        Some(IrExpr::Arrow(stmts)) => stmts_inline(stmts),
        other => Err(format!("arrow not found at {idx}: {other:?}")),
    }
}

/// The stages of a pipeline call: `Array([Arrow, Arrow, ...])`.
fn pipeline_stages(args: &[IrExpr]) -> Result<Vec<Vec<IrStmt>>, String> {
    let Some(IrExpr::Array(stages)) = args.first() else {
        return Err("pipeline stages not Array".into());
    };
    let mut out = Vec::new();
    for s in stages {
        match s {
            IrExpr::Arrow(stmts) => out.push(stmts.clone()),
            other => return Err(format!("pipeline stage not Arrow: {other:?}")),
        }
    }
    Ok(out)
}

/// Compact single-line rendering of a statement sequence (capture bodies,
/// pipeline stages, inline compounds).
fn stmts_inline(stmts: &[IrStmt]) -> Result<String, String> {
    let mut out = Vec::new();
    for s in stmts {
        out.push(stmt_inline(s)?);
    }
    Ok(out.join("; "))
}

fn stmt_inline(st: &IrStmt) -> Result<String, String> {
    match st {
        IrStmt::Expr(e) => cmd_to_sh(e),
        IrStmt::Assign { targets, expr } => assign_to_sh(targets, expr),
        IrStmt::If { cond, then, elsifs, else_ } => {
            let mut out = format!("if {}; then {}", cmd_to_sh(cond)?, stmts_inline(then)?);
            for (ec, body) in elsifs {
                out.push_str(&format!("; elif {}; then {}", cmd_to_sh(ec)?, stmts_inline(body)?));
            }
            if !else_.is_empty() {
                out.push_str(&format!("; else {}", stmts_inline(else_)?));
            }
            out.push_str("; fi");
            Ok(out)
        }
        IrStmt::For { var, iter, body } => Ok(format!(
            "for {var} in {}; do {}; done",
            for_items_to_sh(iter)?,
            stmts_inline(body)?
        )),
        IrStmt::While { cond, body } => Ok(format!(
            "while {}; do {}; done",
            cmd_to_sh(cond)?,
            stmts_inline(body)?
        )),
        IrStmt::DoWhile { body, cond, until } => {
            let neg = if *until { "" } else { "! " };
            Ok(format!(
                "while :; do {}; if {neg}{}; then break; fi; done",
                stmts_inline(body)?,
                cmd_to_sh(cond)?
            ))
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut out = format!("case {} in", word_to_sh(discriminant)?);
            for cl in clauses {
                out.push_str(&format!(" {}) {};;", cl.patterns.join("|"), stmts_inline(&cl.body)?));
            }
            out.push_str(" esac");
            Ok(out)
        }
        IrStmt::Function { name, body } => {
            Ok(format!("{name}() {{ {}; }}", stmts_inline(body)?))
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut out = stmts_inline(inner)?;
            out.push_str(&redirects_to_sh(redirects)?);
            Ok(out)
        }
        IrStmt::Subshell(body) => Ok(format!("( {} )", stmts_inline(body)?)),
        IrStmt::Background(body) => Ok(format!("( {} ) &", stmts_inline(body)?)),
        IrStmt::Block(body) => Ok(format!("{{ {}; }}", stmts_inline(body)?)),
        IrStmt::Return(e) => match e {
            Some(x) => Ok(format!("return {}", word_to_sh(x)?)),
            None => Ok("return".into()),
        },
        IrStmt::Exit(e) => match e {
            Some(x) => Ok(format!("exit {}", word_to_sh(x)?)),
            None => Ok("exit".into()),
        },
        IrStmt::Die { expr, .. } => Ok(format!("echo {} >&2; exit 1", word_to_sh(expr)?)),
        IrStmt::Warn { expr, .. } => Ok(format!("echo {} >&2", word_to_sh(expr)?)),
        IrStmt::Declare { vars, init, local } => {
            let mut out = String::new();
            if *local {
                out.push_str("local ");
            }
            for (i, v) in vars.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&v.name);
                if let Some(init) = init {
                    out.push('=');
                    out.push_str(&word_to_sh(init)?);
                }
            }
            Ok(out)
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            Ok(format!("{var}=({})", array_literal_to_sh(elements)?))
        }
        IrStmt::Output { value, newline, .. } => {
            let w = word_to_sh(value)?;
            if *newline {
                Ok(format!("printf '%s\\n' {w}"))
            } else {
                Ok(format!("printf '%s' {w}"))
            }
        }
        IrStmt::Exec {
            cmd,
            args,
            capture,
            redirects,
            env,
        } => {
            let mut line = exec_line_to_sh(cmd, args, Some(env))?;
            if !redirects.is_empty() {
                line.push_str(&redirect_objs_to_sh(redirects)?);
            }
            if let Some(var) = capture {
                Ok(format!("{var}=$({line})"))
            } else {
                Ok(line)
            }
        }
        IrStmt::Pipeline {
            stages,
            capture,
            ..
        } => {
            let mut line = String::new();
            for (i, stg) in stages.iter().enumerate() {
                if i > 0 {
                    line.push_str(" | ");
                }
                line.push_str(&stmts_inline(stg)?);
            }
            if let Some(var) = capture {
                Ok(format!("{var}=$({line})"))
            } else {
                Ok(line)
            }
        }
        IrStmt::WriteFile {
            path,
            content,
            append,
        } => Ok(format!(
            "printf '%s' {} {} {}",
            word_to_sh(content)?,
            if *append { ">>" } else { ">" },
            word_to_sh(path)?
        )),
        IrStmt::SetChildError(_) | IrStmt::Require(_) | IrStmt::RawText(_) => Ok(String::new()),
    }
}

// ── for-loop item lists ──────────────────────────────────────────────

fn for_items_to_sh(iter: &IrExpr) -> Result<String, String> {
    match iter {
        IrExpr::Array(items) => {
            if items.is_empty() {
                // `for x; do` — iterate the positional parameters
                return Ok("\"$@\"".into());
            }
            let mut out = Vec::new();
            for it in items {
                out.push(for_item_to_sh(it)?);
            }
            Ok(out.join(" "))
        }
        other => Ok(word_to_sh(other)?),
    }
}

fn for_item_to_sh(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Call { func, args } if func == "getVar" => Ok(format!("${}", raw_arg(args, 0)?)),
        IrExpr::Call { func, args } if func == "listVar" => {
            let n = raw_arg(args, 0)?;
            Ok(if n == "*" { "\"$*\"".into() } else { "\"$@\"".into() })
        }
        IrExpr::Call { func, args } if func == "join" => join_to_sh(arg(args, 0)?, true),
        IrExpr::Call { func, args } if func == "param" => param_to_sh(args, false),
        IrExpr::Call { func, args } if func == "arrayIndex" => Ok(format!(
            "${{{}{}}}",
            raw_arg(args, 0)?,
            raw_arg(args, 1)?
        )),
        IrExpr::Call { func, args } if func == "arrayItems" => {
            Ok(format!("${{!{}[@]}}", raw_arg(args, 0)?))
        }
        IrExpr::Call { func, args } if func == "arrayLen" => {
            Ok(format!("${{#{}[@]}}", raw_arg(args, 0)?))
        }
        _ => word_to_sh(e),
    }
}

fn array_literal_to_sh(elements: &[IrExpr]) -> Result<String, String> {
    let mut out = Vec::new();
    for e in elements {
        out.push(word_to_sh(e)?);
    }
    Ok(format!("({})", out.join(" ")))
}

fn array_items(args: &[IrExpr], idx: usize) -> Result<String, String> {
    let Some(IrExpr::Array(items)) = args.get(idx) else {
        return Ok(String::new());
    };
    let mut out = Vec::new();
    for it in items {
        out.push(word_to_sh(it)?);
    }
    Ok(out.join(" "))
}

// ── argument helpers ─────────────────────────────────────────────────

fn arg<'a>(args: &'a [IrExpr], idx: usize) -> Result<&'a IrExpr, String> {
    args.get(idx)
        .ok_or_else(|| format!("missing argument {idx} in {args:?}"))
}

/// The raw text of a Str argument.
fn raw_arg(args: &[IrExpr], idx: usize) -> Result<String, String> {
    str_arg(arg(args, idx)?)
}

/// The raw text of a Str expression.
fn str_arg(e: &IrExpr) -> Result<String, String> {
    match e {
        IrExpr::Str(s, _) => Ok(s.clone()),
        IrExpr::Int(i) => Ok(i.to_string()),
        IrExpr::Ident(s) => Ok(s.clone()),
        other => Err(format!("expected Str argument, got {other:?}")),
    }
}

