//! sh_backend — PURE POSIX-sh renderer (worktree-local, branch `backend/sh`).
//!
//! Consumes the ShIR (the A1 contract) in-process and emits POSIX
//! `sh` source. INITIAL VERSION: the v1 subset — simple commands
//! (Expr(Call("exec", ...)) — echo/printf/etc.), Assign, If, For,
//! While, Block, Expr(Call("getVar", ...)) reads. Anything outside the
//! subset prints a marker to stderr and returns Err — the corpus gate
//! then reports FAIL on that example, and the worker's pi extends the
//! renderer (the same progression the c backend followed).
//!
//! The corpus gate (`setup_backends.sh --backend-gate sh`) runs
//! `sh_backend <file.sh>` per example and requires exit 0 + valid
//! POSIX-sh output.

use crate::ir::{IrExpr, IrProgram, IrStmt};

/// Render a ShIR program to POSIX `sh` source. `Err` on a construct
/// outside the v1 subset (the gate reports it as a FAIL).
pub fn shir_to_sh(prog: &IrProgram) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("#!/bin/sh\n");
    for st in &prog.stmts {
        stmt_to_sh(st, 0, &mut out)?;
    }
    Ok(out)
}

fn indent(out: &mut String, d: usize) {
    for _ in 0..d {
        out.push_str("    ");
    }
}

fn stmt_to_sh(st: &IrStmt, d: usize, out: &mut String) -> Result<(), String> {
    match st {
        IrStmt::Expr(e) => expr_stmt_to_sh(e, d, out),
        IrStmt::Assign { targets, expr } => {
            // v1: a single plain target (no indices).
            let t = targets.first().ok_or("assign: no target (v1)")?;
            if !t.indices.is_empty() {
                return Err("assign with indices not in the v1 subset".into());
            }
            indent(out, d);
            out.push_str(&t.var);
            out.push('=');
            expr_to_sh(expr, out)?;
            out.push('\n');
            Ok(())
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            indent(out, d);
            out.push_str("if ");
            cond_to_sh(cond, out)?;
            out.push_str("; then\n");
            for b in then {
                stmt_to_sh(b, d + 1, out)?;
            }
            for (econd, ebody) in elsifs {
                indent(out, d);
                out.push_str("elif ");
                cond_to_sh(econd, out)?;
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
            for_items_to_sh(iter, out)?;
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
            cond_to_sh(cond, out)?;
            out.push_str("; do\n");
            for b in body {
                stmt_to_sh(b, d + 1, out)?;
            }
            indent(out, d);
            out.push_str("done\n");
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
        other => Err(format!(
            "statement not in the v1 POSIX-sh subset: {other:?}"
        )),
    }
}

/// A condition is a command (the shell's if/while test). v1: an exec
/// call or a var read.
fn cond_to_sh(cond: &IrExpr, out: &mut String) -> Result<(), String> {
    let mut tmp = String::new();
    expr_stmt_to_sh(cond, 0, &mut tmp)?;
    out.push_str(tmp.trim_end());
    Ok(())
}

/// A simple command expression — `Expr(Call("exec"|"getVar", ...))`.
fn expr_stmt_to_sh(e: &IrExpr, d: usize, out: &mut String) -> Result<(), String> {
    match e {
        IrExpr::Call { func, args, .. } => {
            let cmd = match args.first() {
                Some(IrExpr::Str(name, _)) => name.clone(),
                _ => return Err("exec with non-literal command not in the v1 subset".into()),
            };
            indent(out, d);
            out.push_str(&cmd);
            for a in &args[1..] {
                out.push(' ');
                expr_word_to_sh(a, out)?;
            }
            out.push('\n');
            Ok(())
        }
        IrExpr::Var(name, _) => {
            indent(out, d);
            out.push_str(&format!("${name}\n"));
            Ok(())
        }
        other => Err(format!("expression not in the v1 POSIX-sh subset: {other:?}")),
    }
}

/// A word in a command/assign RHS.
fn expr_word_to_sh(e: &IrExpr, out: &mut String) -> Result<(), String> {
    match e {
        IrExpr::Str(s, _) => {
            // v1: quote words that contain shell metacharacters, else bare.
            if s.is_empty() || s.contains([' ', '\t', '$', '"', '\'', ';', '&', '|', '(', ')', '*', '?']) {
                out.push_str(&format!("'{}'", s.replace('\'', "'\\''")));
            } else {
                out.push_str(s);
            }
            Ok(())
        }
        IrExpr::Call { func, args, .. } if func == "getVar" => {
            if let Some(IrExpr::Str(name, _)) = args.first() {
                out.push_str(&format!("\"${name}\""));
                Ok(())
            } else {
                Err("getVar with non-literal name (v1)".into())
            }
        }
        IrExpr::Var(name, _) => {
            out.push_str(&format!("\"${name}\""));
            Ok(())
        }
        // a multi-word argument (`echo hi $x` -> Array([Str, getVar])) —
        // the shell word-splits the joined text exactly as bash does
        IrExpr::Array(items) => {
            let mut tmp = String::new();
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    tmp.push(' ');
                }
                expr_word_to_sh(it, &mut tmp)?;
            }
            out.push_str(&tmp);
            Ok(())
        }
        other => Err(format!("word not in the v1 POSIX-sh subset: {other:?}")),
    }
}

fn expr_to_sh(e: &IrExpr, out: &mut String) -> Result<(), String> {
    expr_word_to_sh(e, out)
}

/// For-iterable items: `Array` of literal words (v1).
fn for_items_to_sh(iter: &IrExpr, out: &mut String) -> Result<(), String> {
    match iter {
        IrExpr::Array(items) => {
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                expr_word_to_sh(it, out)?;
            }
            Ok(())
        }
        other => Err(format!("for-iterable not in the v1 POSIX-sh subset: {other:?}")),
    }
}
