//! Capabilities: file-grep over a single literal file, in condition
//! position (`grep -q PAT FILE` → boolean) and in the `(grep && echo A) ||
//! echo B` control chain (→ native if/else; -c/-l/-L print their output
//! first). Only output-quiet / literal-safe forms lift — regex patterns,
//! globs, multiple files and bare grep (which prints matching lines) refuse.

use crate::ir::BinOpKind;
use crate::ir::IrExpr;
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    match ctx {
        NativeCtx::Exec { call, cond: true } => grep_boolean(call).map(NativeEmit::Cond),
        NativeCtx::Chain(e) => grep_chain(e).map(NativeEmit::Stmt),
        _ => None,
    }
}

/// A verified `grep [-q|c|l|L] [-i] [-s] PAT FILE` (single literal file).
struct GrepSpec {
    flags: String, // the flag characters actually present (q/c/l/L/i/s)
    pat: String,
    file: String,
    ci: bool,
}

fn grep_spec(call: &IrExpr) -> Option<GrepSpec> {
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
    let mut flags = String::new();
    let mut nonflag: Vec<&IrExpr> = Vec::new();
    let mut ci = false;
    for w in words {
        match w {
            IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                let f: String = s[1..].chars().filter(|c| "qclLis".contains(*c)).collect();
                if f.len() != s.len() - 1 {
                    return None; // an unsupported flag char
                }
                flags.push_str(&f);
                ci |= f.contains('i');
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
    Some(GrepSpec { flags, pat, file, ci })
}

/// A perl expression counting the lines of FILE that contain PAT (0/1 is
/// the -q boolean; -c/-l/-L use the count).
fn count_expr(spec: &GrepSpec) -> String {
    let file = crate::ir::safe_perl_q_string(&spec.file);
    let (pat, line) = if spec.ci {
        (
            format!("lc({})", crate::ir::safe_perl_q_string(&spec.pat)),
            "lc($__gl)".to_string(),
        )
    } else {
        (crate::ir::safe_perl_q_string(&spec.pat), "$__gl".to_string())
    };
    format!(
        "(sub {{ open(my $__gh, '<', {file}) or return 0; local $/; my $__gc = <$__gh>; close $__gh; my $__gn = 0; for my $__gl (split(/\\n/, $__gc)) {{ $__gn++ if index({line}, {pat}) >= 0; }} $__gn }})->()"
    )
}

/// `grep -q PAT FILE` in condition position → the files-equal boolean.
fn grep_boolean(call: &IrExpr) -> Option<String> {
    let spec = grep_spec(call)?;
    if spec.flags.chars().any(|c| matches!(c, 'c' | 'l' | 'L')) {
        return None; // these print output, not a plain boolean
    }
    Some(format!("({} > 0)", count_expr(&spec)))
}

/// `(grep [-q|-c|-l|-L] PAT FILE && echo A) || echo B` → native statement:
/// -q prints nothing, -c prints the count, -l prints FILE when matched,
/// -L prints FILE when NOT matched; the exit status drives the if/else.
fn grep_chain(e: &IrExpr) -> Option<String> {
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
    let spec = grep_spec(cond)?;
    let cnt = count_expr(&spec);
    let then_perl = literal_echo(then_body)?;
    let else_perl = literal_echo(rhs)?;
    let file_perl = crate::ir::safe_perl_q_string(&spec.file);
    // NOTE: grep's EXIT STATUS is always "did any line match" (0) — the
    // -c/-l/-L flags only change the OUTPUT (count / matched filename /
    // non-matched filename). So the if/else status is `cnt > 0` for every
    // flag; only the printed payload differs.
    let output = match spec.flags.as_str() {
        f if f.contains('q') => String::new(),
        f if f.contains('c') => format!("print({cnt}, \"\\n\");"),
        f if f.contains('l') => format!("print({file_perl}, \"\\n\") if {cnt} > 0;"),
        f if f.contains('L') => format!("print({file_perl}, \"\\n\") if {cnt} == 0;"),
        _ => return None,
    };
    let hit_test = format!("({cnt} > 0)");
    Some(format!(
        "{output} if ({hit_test}) {{ {then_perl} }} else {{ {else_perl} }}"
    ))
}

/// A bare `exec echo LITERAL` renders as native `print` (always exit 0),
/// so it can be a `&&`/`||` chain body without a shell-out.
pub(crate) fn literal_echo(e: &IrExpr) -> Option<String> {
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
