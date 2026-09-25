//! const-capture-fold — fold a command substitution that runs only a
//! constant `echo` into the string literal (`v=$(echo foo)` → `v="foo"`),
//! the constant half of capture lowering (direct-calls handles
//! function-of-constant; this handles the plain literal echo).
//!
//! ## Need
//! Scripts and generated code capture constants via `$(echo …)`; every
//! backend spawns a process (or a string-parse) for a value known at IR
//! time. Folding it to a `Str` literal removes the spawn in all backends.
//!
//! ## Scope — the sound rule
//! A `Capture` folds to a `Str` only when its arrow body is EXACTLY ONE
//! `Output` of a literal `Str` (no interpolation, no fd redirect, no
//! `target`), and the result strips the trailing newlines the way command
//! substitution does:
//!   - `Output { value: Str(s), newline: true, target: None }`
//!     → `Str(s)` with ALL trailing `\n` removed;
//!   - `Output { value: Str(s), newline: false, … }` → `Str(s)`.
//! Any capture that embeds a variable, a non-`echo` command, multiple
//! statements, or a redirect is left untouched (refuse > guess).
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest). Registered in
//! `transforms.rs` (SH2_TRANSFORMS gated). Complements direct-calls.

use crate::ir::{ArithAst, InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt_pass(st);
    }
    c
}

fn stmt_pass(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_pass(expr),
        IrStmt::Expr(e) => expr_pass(e),
        IrStmt::Output { value, .. } => expr_pass(value),
        IrStmt::Declare { init, .. } => init.as_mut().map(expr_pass).unwrap_or(false),
        IrStmt::WriteFile { path, content, .. } => {
            expr_pass(path) | expr_pass(content)
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = expr_pass(cond);
            for s in then.iter_mut() {
                c |= stmt_pass(s);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr_pass(ec);
                for s in eb.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            for s in else_.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, var: _ } => {
            let mut c = expr_pass(cond);
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        _ => false,
    }
}

fn expr_pass(e: &mut IrExpr) -> bool {
    match e {
        // the FRONTEND spelling of a command substitution: a `capture`
        // Call whose single argument is the body arrow. The py-sh-go
        // frontend emits this for `subprocess.run(…, capture_output=True)`
        // / `$(…)`, with the body an `exec` call rather than an `Output`.
        IrExpr::Call { func, args, .. } if func == "capture" => {
            let mut changed = false;
            if let Some(inner) = args.first() {
                if let Some(s) = fold_const_echo(inner) {
                    let style = crate::ir::StrStyle::DoubleQuoted;
                    *e = IrExpr::Str(s, style);
                    return true;
                }
            }
            for a in args.iter_mut() {
                changed |= expr_pass(a);
            }
            changed
        }
        IrExpr::Capture { expr, native } => {
            if *native {
                return false; // already a native value — leave it
            }
            let mut changed = false;
            if let Some(s) = fold_const_echo(expr) {
                *e = IrExpr::Str(s, crate::ir::StrStyle::DoubleQuoted);
                changed = true;
            } else {
                changed = expr_pass(expr);
            }
            changed
        }
        IrExpr::BinOp { lhs, rhs, .. } => expr_pass(lhs) | expr_pass(rhs),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr_pass(a);
            }
            c
        }
        IrExpr::Index { key, .. } => expr_pass(key),
        IrExpr::Arith(a) => arith_pass(a),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= expr_pass(x);
                }
            }
            c
        }
        IrExpr::Arrow(body) => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        _ => false,
    }
}

fn arith_pass(a: &mut ArithAst) -> bool {
    match a {
        ArithAst::Un { arg, .. } => arith_pass(arg),
        ArithAst::Bin { lhs, rhs, .. } => arith_pass(lhs) | arith_pass(rhs),
        _ => false,
    }
}

/// If `expr` (the capture's inner Arrow) is exactly one literal-echo
/// Output, return the folded string (trailing newlines stripped); else
/// None.
///
/// TWO spellings of "one literal echo" are recognised:
///   * `Output { value: Str, newline, target: None }` — the lowered form
///     (`echo foo` after echo-fold, or a shell-source capture);
///   * `Expr(Call { func: "exec", args: [Str("echo"), Array(literals)] })`
///     — the FRONTEND form (py-sh-go lowers `subprocess.run(["echo", …],
///     capture_output=True)` and command substitution this way, before any
///     echo lowering has run on the body).
fn fold_const_echo(expr: &IrExpr) -> Option<String> {
    let body = match expr {
        IrExpr::Arrow(body) => body,
        _ => return None,
    };
    match body.as_slice() {
        [IrStmt::Output { value, newline, target: None }] => match value {
            IrExpr::Str(s, _) => Some(if *newline {
                strip_all_trailing_newlines(s)
            } else {
                s.clone()
            }),
            // not a literal — maybe an Interpolate of consts? refuse
            _ => None,
        },
        // the frontend exec shape: `echo <literal words…>`
        [IrStmt::Expr(IrExpr::Call { func, args, .. })] if func == "exec" => {
            // echo joins its arguments with a space and ends the line
            Some(const_echo_words(args)?.join(" "))
        }
        _ => None,
    }
}

/// The literal words of an `exec` echo, when EVERY argument is a constant
/// (the command must be `echo`; a `-n`/`-e` flag disables the fold since
/// it changes the output). Returns None on anything with a variable, a
/// capture, an interpolation or a flag.
fn const_echo_words(args: &[IrExpr]) -> Option<Vec<String>> {
    let cmd = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    if cmd != "echo" {
        return None;
    }
    let items = match args.get(1) {
        Some(IrExpr::Array(items)) => items,
        _ => return None,
    };
    let mut words = Vec::with_capacity(items.len());
    for w in items {
        match w {
            IrExpr::Str(s, _) => {
                // an echo FLAG (`-n`, `-e`, `-E`) changes the output
                if s.starts_with('-') && s.len() > 1 && s[1..].chars().all(|c| "neE".contains(c)) {
                    return None;
                }
                words.push(s.clone());
            }
            _ => return None,
        }
    }
    Some(words)
}

fn strip_all_trailing_newlines(s: &str) -> String {
    s.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn call(func: &str, args: Vec<IrExpr>) -> IrExpr {
        IrExpr::Call { func: func.to_string(), args }
    }
    fn s(x: &str) -> IrExpr {
        IrExpr::Str(x.to_string(), StrStyle::DoubleQuoted)
    }
    /// The py-frontend capture shape:
    /// `capture(Arrow([Expr(exec(echo, [words]))]))`.
    fn frontend_capture(words: Vec<IrExpr>) -> IrStmt {
        let body = IrExpr::Arrow(vec![IrStmt::Expr(call(
            "exec",
            vec![s("echo"), IrExpr::Array(words)],
        ))]);
        IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: "x".into(),
                indices: vec![],
                sigil: None,
            }],
            expr: call("capture", vec![body]),
            asm: None,
        }
    }

    #[test]
    fn frontend_constant_echo_capture_folds() {
        let mut stmts = vec![frontend_capture(vec![s("hi")])];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Assign { expr: IrExpr::Str(v, _), .. } => assert_eq!(v, "hi"),
            other => panic!("not folded: {other:?}"),
        }
    }

    #[test]
    fn multi_word_echo_joins_with_a_space() {
        let mut stmts = vec![frontend_capture(vec![s("a"), s("b")])];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Assign { expr: IrExpr::Str(v, _), .. } => assert_eq!(v, "a b"),
            other => panic!("not folded: {other:?}"),
        }
    }

    #[test]
    fn variable_word_refuses() {
        let mut stmts = vec![frontend_capture(vec![call("getVar", vec![s("v")])])];
        assert!(!transform(&mut stmts), "a var word must not fold");
    }

    #[test]
    fn echo_flag_refuses() {
        // `echo -n hi`: the flag changes the output, so refuse
        let mut stmts = vec![frontend_capture(vec![s("-n"), s("hi")])];
        assert!(!transform(&mut stmts), "an echo flag must not fold");
    }

    #[test]
    fn non_echo_command_refuses() {
        let body = IrExpr::Arrow(vec![IrStmt::Expr(call(
            "exec",
            vec![s("printf"), IrExpr::Array(vec![s("hi")])],
        ))]);
        let mut stmts = vec![IrStmt::Expr(call("capture", vec![body]))];
        assert!(!transform(&mut stmts), "printf must not fold as echo");
    }

    #[test]
    fn multi_statement_body_refuses() {
        let body = IrExpr::Arrow(vec![
            IrStmt::Expr(call("exec", vec![s("echo"), IrExpr::Array(vec![s("a")])])),
            IrStmt::Expr(call("exec", vec![s("echo"), IrExpr::Array(vec![s("b")])])),
        ]);
        let mut stmts = vec![IrStmt::Expr(call("capture", vec![body]))];
        assert!(!transform(&mut stmts), "two statements must not fold");
    }
}
