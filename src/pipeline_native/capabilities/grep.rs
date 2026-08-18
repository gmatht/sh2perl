//! Capabilities: quiet file-grep (`grep -q PAT FILE` → native "does the
//! file contain") in condition position, and the `(grep -q && echo A) ||
//! echo B` control chain → native if/else.

use crate::ir::BinOpKind;
use crate::ir::IrExpr;
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    match ctx {
        NativeCtx::Exec { call, cond: true } => grep_file_contains(call).map(NativeEmit::Cond),
        NativeCtx::Chain(e) => grep_echo_chain(e).map(NativeEmit::Stmt),
        _ => None,
    }
}

/// `grep [-q|-i|-s] PAT FILE` in condition position → a boolean perl
/// expression (read the file + `index`). Only the output-quiet forms lift
/// (grep -q discards stdout; the status is exactly "a line contains the
/// literal"). Regex patterns, globs, multiple files → refuse.
fn grep_file_contains(call: &IrExpr) -> Option<String> {
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "grep") {
        return None;
    }
    let words: Vec<&IrExpr> = crate::ir::exec_word_args(args);
    let mut ci = false;
    let mut nonflag: Vec<&IrExpr> = Vec::new();
    for w in words {
        match w {
            IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                let f: String = s[1..].chars().filter(|c| "qis".contains(*c)).collect();
                if f.len() != s.len() - 1 {
                    return None; // an unsupported flag char
                }
                ci |= s.contains('i');
            }
            _ => nonflag.push(w),
        }
    }
    if nonflag.len() != 2 {
        return None; // need exactly pattern + one file
    }
    let pat = crate::ir::grep_lit_str(nonflag[0])?;
    if !grep_literal_safe(&pat) {
        return None;
    }
    let file = crate::ir::grep_lit_str(nonflag[1])?;
    if file.contains('*') || file.contains('?') {
        return None; // a glob expends to many files — not a single read
    }
    let (hay, ndl) = if ci {
        (
            "lc($__grep_c)".to_string(),
            format!("lc({})", crate::ir::safe_perl_q_string(&pat)),
        )
    } else {
        ("$__grep_c".to_string(), crate::ir::safe_perl_q_string(&pat))
    };
    Some(format!(
        "(sub {{ open(my $__grep_h, '<', {}) or return 0; local $/; \
            my $__grep_c = <$__grep_h>; close $__grep_h; \
            return (index({hay}, {ndl}) >= 0 ? 1 : 0); }}->())",
        crate::ir::safe_perl_q_string(&file)
    ))
}

/// `(grep -q PAT FILE && echo A) || echo B` → native if/else: -q is quiet
/// and a literal echo body always succeeds, so `X && A || B ≡ if X {A}
/// else {B}`.
fn grep_echo_chain(e: &IrExpr) -> Option<String> {
    let IrExpr::BinOp {
        lhs,
        op: BinOpKind::Or,
        rhs,
    } = e
    else {
        return None;
    };
    let IrExpr::BinOp {
        lhs: cond,
        op: BinOpKind::And,
        rhs: then_body,
    } = lhs.as_ref()
    else {
        return None;
    };
    let cond_perl = grep_file_contains(cond)?;
    let then_perl = literal_echo(then_body)?;
    let else_perl = literal_echo(rhs)?;
    Some(format!(
        "if ({cond_perl}) {{ {then_perl} }} else {{ {else_perl} }}"
    ))
}

/// A bare `exec echo LITERAL` renders as native `print` (always exit 0),
/// so it can be a `&&`/`||` chain body without a shell-out.
fn literal_echo(e: &IrExpr) -> Option<String> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "echo") {
        return None;
    }
    let words: Vec<&IrExpr> = crate::ir::exec_word_args(args);
    if words.len() != 1 {
        return None; // native form is a single literal payload
    }
    let text = crate::ir::grep_lit_str(words[0])?;
    Some(format!(
        "print({}, \"\\n\"); $main_exit_code = $CHILD_ERROR = 0;",
        crate::ir::safe_perl_q_string(&text)
    ))
}

/// A pattern is a plain substring iff grep's BRE would treat it as a
/// literal: no *leading* `-` (parsed as an option), no BRE metacharacters
/// (`^ $ . [ ] * \`), and no real newline (grep is line-scoped).
fn grep_literal_safe(pat: &str) -> bool {
    !pat.starts_with('-')
        && !pat
            .chars()
            .any(|c| matches!(c, '^' | '$' | '.' | '[' | ']' | '*' | '\\' | '\n'))
}
