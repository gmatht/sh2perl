//! cat-heredoc — `cat <<'EOF' … EOF` (statement position, literal body) →
//! a native `Output` print.
//!
//! `cat` with no operands and a literal heredoc on fd 0 is a pure print:
//! the body bytes, exactly once, with no substitution. Every backend can
//! emit that without a fork/exec — and once the shape is an `Output`, the
//! existing `output-line-fold` moves the body's trailing newline into the
//! `newline` flag, so the renderer's LINE-print arm fires (`println!`,
//! `puts`, `fmt.Println`, …) instead of a raw multi-line `print!`.
//!
//! ## Why this belongs in the shared channel
//! Measured on the 563-example corpus: 32 `__sh_spawn("'cat'")` sites in
//! the Rust render alone, concentrated in the heredoc examples
//! (`007_cat_EOF`, `062_03_complex_heredocs`, `079_heredoc_interpolation`,
//! `063_hard_to_lex`, …). Each is a `bash -c` per heredoc. The shape is
//! already IR: `Redirect { inner: [Expr(exec("cat", []))], redirects:
//! [{fd: 0, mode: "heredoc", target: Str(body)}] }`, so one rewrite
//! retires the spawn for EVERY backend at once (§11 marketplace shape).
//!
//! ## Scope — the sound rewrite
//! Statement-position `Redirect` only, and ALL of:
//!   * inner is exactly one `exec`/`builtin` call of `cat` with NO operands
//!     (`cat FILE` is `cat-read`'s business; `cat > f <<EOF` is
//!     `shir-native-stmt`'s `WriteFile` lift);
//!   * exactly ONE redirect: fd 0, mode `heredoc` (not `heredoc-tabs` —
//!     `<<-` tab stripping is the parser's job and is not verified here);
//!   * the target is a literal `Str`;
//!   * if the heredoc is INTERPOLATING, the body carries no `$`, no
//!     backtick and no backslash — otherwise the expansion is real work and
//!     the exec is kept (REFUSE > GUESS).
//!
//! A quoted body (`<<'EOF'`) is always literal, so it always lifts.
//!
//! ## Placement
//! Registered in `transforms.rs` immediately after `cat-read` and BEFORE
//! `output-line-fold`, so the `Output` this emits is line-folded in the
//! same pass (trailing `\n` → `newline: true`).

use crate::ir::{IrExpr, IrStmt, StrStyle};

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= lift_stmt(st);
    }
    changed
}

fn lift_stmt(st: &mut IrStmt) -> bool {
    if let Some(body) = literal_cat_heredoc(st) {
        *st = IrStmt::Output {
            value: IrExpr::Str(body, StrStyle::DoubleQuoted),
            // The body carries its own trailing newline (bash's heredoc
            // terminator is not part of it), so print the bytes as-is;
            // output-line-fold turns that into `newline: true`.
            newline: false,
            target: None,
        };
        return true;
    }
    match st {
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b) => transform(b),
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            let mut c = transform(body);
            for (_, nb) in named_blocks.iter_mut() {
                c |= transform(nb);
            }
            c
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. } => transform(body),
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            let mut c = transform(then);
            for (_, eb) in elsifs.iter_mut() {
                c |= transform(eb);
            }
            c |= transform(else_);
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stage in stages.iter_mut() {
                c |= transform(stage);
            }
            c
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = transform(body);
            for e in excepts.iter_mut() {
                c |= transform(&mut e.body);
            }
            c |= transform(else_body);
            c |= transform(finally_body);
            c
        }
        IrStmt::Case { clauses, .. } => {
            let mut c = false;
            for cl in clauses.iter_mut() {
                c |= transform(&mut cl.body);
            }
            c
        }
        // any other redirect shape: its inner statements may still hold one
        IrStmt::Redirect { inner, .. } => transform(inner),
        _ => false,
    }
}

/// The literal body of a statement-position `cat <<EOF`, if this statement
/// is that shape (see the module doc for the exact guards).
fn literal_cat_heredoc(st: &IrStmt) -> Option<String> {
    let IrStmt::Redirect { inner, redirects } = st else {
        return None;
    };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if cmd != "cat" || !words.is_empty() {
        return None;
    }
    if redirects.len() != 1 {
        return None;
    }
    let r = &redirects[0];
    if r.fd != Some(0) || r.mode != "heredoc" {
        return None;
    }
    let IrExpr::Str(body, _) = &r.target else {
        return None;
    };
    if r.interpolate && body_has_expansion(body) {
        return None;
    }
    Some(body.clone())
}

/// Would an interpolating heredoc need real work? `$` (variable/command),
/// a backtick, or a backslash (which escapes `$`, a backtick, `\`, or a
/// newline). Anything else is literal text.
fn body_has_expansion(s: &str) -> bool {
    s.contains('$') || s.contains('`') || s.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::AssignTarget;

    fn cat_heredoc(body: &str, interpolate: bool, mode: &str, fd: i32) -> IrStmt {
        IrStmt::Redirect {
            inner: vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![
                    IrExpr::Str("cat".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![]),
                ],
            })],
            redirects: vec![crate::ir::IrRedirect {
                fd: Some(fd),
                mode: mode.to_string(),
                target: IrExpr::Str(body.to_string(), StrStyle::DoubleQuoted),
                interpolate,
            }],
        }
    }

    fn output_of(st: &IrStmt) -> Option<(&str, bool)> {
        match st {
            IrStmt::Output {
                value: IrExpr::Str(s, _),
                newline,
                target: None,
            } => Some((s.as_str(), *newline)),
            _ => None,
        }
    }

    #[test]
    fn quoted_literal_heredoc_lifts_to_output() {
        for body in ["hello\nworld\n", "no trailing newline", "\n\n"] {
            let mut stmts = vec![cat_heredoc(body, false, "heredoc", 0)];
            assert!(transform(&mut stmts), "{body:?} must lift");
            assert_eq!(
                output_of(&stmts[0]),
                Some((body, false)),
                "{body:?}: bytes preserved verbatim, newline flag left to output-line-fold"
            );
        }
    }

    #[test]
    fn unquoted_body_without_expansion_lifts() {
        // interpolation is a no-op when the body names nothing
        let mut stmts = vec![cat_heredoc("plain text\n", true, "heredoc", 0)];
        assert!(transform(&mut stmts));
        assert_eq!(output_of(&stmts[0]), Some(("plain text\n", false)));
    }

    #[test]
    fn interpolating_body_with_expansion_refuses() {
        for body in ["say $v\n", "`date`\n", "a\\tb\n", "${x}\n", "$(pwd)\n"] {
            let stmts0 = vec![cat_heredoc(body, true, "heredoc", 0)];
            let mut stmts = stmts0.clone();
            assert!(!transform(&mut stmts), "{body:?} needs runtime expansion");
            assert_eq!(stmts, stmts0, "{body:?}: shape untouched");
        }
    }

    #[test]
    fn a_quoted_body_keeps_dollar_text() {
        // `<<'EOF'` is literal even when it looks like an expansion
        let mut stmts = vec![cat_heredoc("cost: $5\n", false, "heredoc", 0)];
        assert!(transform(&mut stmts));
        assert_eq!(output_of(&stmts[0]), Some(("cost: $5\n", false)));
    }

    #[test]
    fn heredoc_tabs_and_other_fds_refuse() {
        for (mode, fd) in [("heredoc-tabs", 0), ("heredoc", 1), ("herestring", 0)] {
            let mut stmts = vec![cat_heredoc("x\n", false, mode, fd)];
            let before = stmts.clone();
            assert!(!transform(&mut stmts), "{mode}/{fd} must refuse");
            assert_eq!(stmts, before);
        }
    }

    #[test]
    fn cat_with_operands_and_extra_redirects_refuse() {
        // `cat FILE` belongs to cat-read; `cat <<EOF > f` to shir-native-stmt
        let mut with_arg = cat_heredoc("x\n", false, "heredoc", 0);
        if let IrStmt::Redirect { inner, .. } = &mut with_arg {
            if let IrStmt::Expr(IrExpr::Call { args, .. }) = &mut inner[0] {
                args[1] = IrExpr::Array(vec![IrExpr::Str(
                    "file.txt".to_string(),
                    StrStyle::DoubleQuoted,
                )]);
            }
        }
        let before = with_arg.clone();
        let mut stmts = vec![with_arg];
        assert!(!transform(&mut stmts), "an operand is not a pure print");
        assert_eq!(stmts[0], before);

        let mut two = cat_heredoc("x\n", false, "heredoc", 0);
        if let IrStmt::Redirect { redirects, .. } = &mut two {
            let extra = redirects[0].clone();
            redirects.push(extra);
        }
        let before = two.clone();
        let mut stmts = vec![two];
        assert!(!transform(&mut stmts), "two redirects: not this shape");
        assert_eq!(stmts[0], before);
    }

    #[test]
    fn nested_bodies_are_reached() {
        let mut stmts = vec![IrStmt::While {
            cond: IrExpr::Str("1".to_string(), StrStyle::DoubleQuoted),
            body: vec![cat_heredoc("inside\n", false, "heredoc", 0), {
                IrStmt::Assign {
                    targets: vec![AssignTarget {
                        var: "i".to_string(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Int(1),
                    asm: None,
                }
            }],
        }];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::While { body, .. } => {
                assert_eq!(output_of(&body[0]), Some(("inside\n", false)))
            }
            other => panic!("not a While: {other:?}"),
        }
    }

    #[test]
    fn the_fold_is_idempotent() {
        let mut stmts = vec![cat_heredoc("x\n", false, "heredoc", 0)];
        assert!(transform(&mut stmts));
        assert!(!transform(&mut stmts), "an Output has no heredoc to lift");
    }
}
