//! grep-o — `grep -o PAT` → the generic `grepMatches(text, pattern, flags)` op.
//!
//! The `-o` (only-matching) grep is a substring-extraction idiom: for
//! each line of the input it prints each match of PAT, one per line.
//! The backends with a native regex engine (estree/js, c, go, py, java,
//! rs, zig) lower `grepMatches` to their native match-all; the sh (and
//! perl) backends keep emitting the shell's `grep -o` (native for them).
//!
//! The lift rewrites the two statically-resolvable input shapes:
//!
//!   • `echo ARGS | grep -o PAT` — a two-stage pipeline whose text is
//!     the echo output (single-argument echo — the joined bytes),
//!   • `grep -o PAT <<< text` — a here-string redirect.
//!
//! `grepMatches(text, pattern, flags)` returns the ARRAY of matched
//! substrings; the backends print one match per line (grep -o's output).
//! The pattern is a BRE by default (translated to ERE for the native
//! engines); `-E` keeps it as-is, `-F` makes it a literal, `-i` adds
//! case-insensitivity. Conservative: any dynamic arg, multiple patterns,
//! or an unsupported flag keeps the grep exec (correct, unoptimized).

use crate::ir::{InterpPart, IrExpr, IrStmt};

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= lift_stmt(st);
    }
    changed
}

fn lift_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Expr(e) => {
            if let Some(new) = lift_expr(e) {
                *e = new;
                true
            } else {
                false
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            // `grep -o PAT <<< text` — the inner is the grep exec, the
            // here-string redirect carries the input text
            if let Some((pat, flags)) = inner_grep_o(inner) {
                if let Some(text) = herestring_text(redirects) {
                    *st = IrStmt::Expr(grep_matches(text, pat, flags));
                    return true;
                }
            }
            false
        }
        IrStmt::Assign { expr, .. } => {
            if let Some(new) = lift_expr(expr) {
                *expr = new;
                true
            } else {
                false
            }
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::If { then: body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        _ => false,
    }
}

fn lift_expr(e: &IrExpr) -> Option<IrExpr> {
    // `$(echo ARGS | grep -o PAT)` — the capture wraps the pipeline
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if func == "capture" {
        if let Some(IrExpr::Arrow(stmts)) = args.first() {
            let mut lifted = stmts.clone();
            for s in lifted.iter_mut() {
                if let IrStmt::Expr(e2) = s {
                    if let Some(new) = lift_expr(e2) {
                        *e2 = new;
                        return Some(IrExpr::Call {
                            func: "capture".to_string(),
                            args: vec![IrExpr::Arrow(lifted)],
                        });
                    }
                }
            }
        }
        return None;
    }
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else {
        return None;
    };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    // stage 1: exec("echo", [single static arg]) — the input text
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = &s1[..] else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if name1 != "echo" || echo_args.len() != 1 {
        return None;
    }
    let text = echo_args[0].clone();
    // stage 2: exec("grep", [flags..., PAT])
    let (pat, flags) = grep_o_of_stage(s2)?;
    Some(grep_matches(text, pat, flags))
}

/// `exec("grep", [argv])` where argv contains `-o` — returns (pattern, flags).
fn inner_grep_o(stmts: &[IrStmt]) -> Option<(String, String)> {
    let [IrStmt::Expr(IrExpr::Call { func, args })] = &stmts[..] else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let [IrExpr::Str(name, _), IrExpr::Array(argv)] = args.as_slice() else {
        return None;
    };
    if name != "grep" {
        return None;
    }
    parse_grep_o(argv)
}

fn grep_o_of_stage(stmts: &[IrStmt]) -> Option<(String, String)> {
    inner_grep_o(stmts)
}

/// Parse the grep argv: `-o` plus the supported flags (`-E -i -F`, combined
/// shorts ok) and exactly one static pattern.
fn parse_grep_o(argv: &[IrExpr]) -> Option<(String, String)> {
    let mut flags = String::new();
    let mut pat: Option<String> = None;
    let mut after_dd = false;
    let mut seen_o = false;
    for a in argv {
        let Some(s) = static_str(a) else {
            return None; // dynamic arg — keep the grep
        };
        if !after_dd && s == "--" {
            after_dd = true;
            continue;
        }
        if !after_dd && s.starts_with('-') && s.len() > 1 {
            for c in s[1..].chars() {
                match c {
                    'o' => seen_o = true,
                    'E' | 'i' | 'F' => {
                        if !flags.contains(c) {
                            flags.push(c);
                        }
                    }
                    _ => return None, // unsupported flag — keep the grep
                }
            }
            continue;
        }
        if pat.is_some() {
            return None; // multiple patterns — no
        }
        pat = Some(s);
    }
    if !seen_o {
        return None; // not `grep -o`
    }
    Some((pat?, flags))
}

/// The here-string redirect's input text.
fn herestring_text(redirects: &[crate::ir::IrRedirect]) -> Option<IrExpr> {
    for r in redirects {
        if r.mode == "herestring" {
            return Some(r.target.clone());
        }
    }
    None
}

/// A statically-known string: a plain Str or an Interpolate of literals.
fn static_str(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    InterpPart::Expr(_) => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn grep_matches(text: IrExpr, pattern: String, flags: String) -> IrExpr {
    IrExpr::Call {
        func: "grepMatches".to_string(),
        args: vec![
            text,
            IrExpr::Str(pattern, crate::ir::StrStyle::SingleQuoted),
            IrExpr::Str(flags, crate::ir::StrStyle::SingleQuoted),
        ],
    }
}
