//! shir-native-stmt — rewrite statement-position shapes that the shIR
//! Perl renderer lowers to `system('bash', '-c', …)` shell-outs into the
//! canonical NATIVE statement shapes every shIR renderer already renders
//! in-process (no bash at runtime).
//!
//! ## Why
//!
//! `./fail-shir` counts `system('bash', …)` call sites in the rendered
//! Perl. For the verified-emulable command set (harness/shir-whitelist.txt
//! — the whitelist the renderers' native emulations cover) the shell-out
//! is never necessary: it is an artefact of the STATEMENT SHAPE the
//! renderer saw, not of the command. Three shape families account for
//! almost all normalisable sites:
//!
//! 1. **File redirect of a native exec** — `echo args > file` /
//!    `printf fmt args > file` is `Redirect{inner:[Expr(exec …)], fd1 w/a}`.
//!    The Perl redirect arm rebuilds the shell text of a bare-exec inner
//!    (`stmts_to_shell_cmd`) and shells out; the native select()-based
//!    file-redirect fallback is only reached when the inner is NOT
//!    rebuildable. Wrapping the exec in a `Block` (a neutral container
//!    every renderer supports) makes the inner non-rebuildable, so the
//!    native fallback fires — the exec renders as a plain print/printf
//!    into the selected handle. The ESTree backend lowers the SAME
//!    runtime `sh2.redirect(body, specs)` call with or without the Block
//!    (the fold it currently uses for literal echo/printf emits
//!    byte-identical output).
//!
//! 2. **Empty-input herestrings** — `cat <<< ''` / `tr … <<< ''` /
//!    `grep -q … <<< ''`: empty stdin produces NO output and a PROVABLE
//!    status (cat/tr → 0; grep-no-match → 1), so the whole redirect
//!    collapses to the `true`/`false` exec.
//!
//! 3. **`test && always-true-cmd || cmd` chains** —
//!    `[[ -f f ]] && echo exists || echo missing`: the chain's decision
//!    is the test's; the always-success then-arm (echo/printf — status 0
//!    on every path) short-circuits the `||` exactly like an if/else, so
//!    the chain becomes a native `IrStmt::If` whose arms render as plain
//!    prints.
//!
//! ## Soundness (REFUSE > GUESS)
//!
//! - A rewrite only fires when the SAME statement, rendered natively,
//!   provably matches bash's stdout + status for the shape (the redirect
//!   target mechanism changes; the exec's rendered words are identical to
//!   the already-native plain statement form of the same command).
//! - The then-arm of family 3 must be an always-success builtin
//!   (echo/printf) — a fallible command (`cat`, `grep`, …) must NOT be
//!   moved into a plain if-branch: bash would run the `||`-else when it
//!   fails, the if-form would not.
//! - Non-empty herestrings, heredocs, fd-2 redirects, multi-spec
//!   redirects, pipelines, captures and dynamic command names are all
//!   left untouched.
//! - Every emitted node (`Expr`, `Call exec`, `If`, `Block`, `Redirect`)
//!   is in the A1 contract and rendered natively by BOTH the Perl and the
//!   ESTree renderers — no new statement kinds.

use crate::ir::{BinOpKind, IrExpr, IrStmt, StrStyle};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= transform_stmt(s);
    }
    c
}

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}

/// The canonical no-op / failed-status exec node (renders natively on
/// every backend: `$main_exit_code = $CHILD_ERROR = 0|1;` in Perl).
fn status_exec(ok: bool) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![st(if ok { "true" } else { "false" }), IrExpr::Array(vec![])],
    })
}

fn transform_stmt(st: &mut IrStmt) -> bool {
    // Recurse into children FIRST (bottom-up — a nested redirect inside an
    // If body is rewritten before we look at the enclosing shape).
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
        IrStmt::Expr(e) => {
            let mut x = transform_expr(e);
            x |= test_chain_to_if(st);
            x
        }
        IrStmt::Assign { expr, .. } | IrStmt::Output { value: expr, .. } => transform_expr(expr),
        IrStmt::Declare {
            init: Some(expr), ..
        } => transform_expr(expr),
        IrStmt::WriteFile { path, content, .. } => {
            transform_expr(path) | transform_expr(content)
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut x = transform(inner);
            for r in redirects.iter_mut() {
                x |= transform_expr(&mut r.target);
            }
            x |= empty_herestring_to_status(st);
            x |= file_redirect_block(st);
            x
        }
        IrStmt::Exec {
            cmd, args, env, ..
        } => {
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

/// The word-args of a canonical `exec` Call: `exec("cmd", [word, …])`.
fn exec_parts(e: &IrExpr) -> Option<(&str, &[IrExpr])> {
    if let IrExpr::Call { func, args } = e {
        if func == "exec" {
            if let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() {
                return Some((cmd, words));
            }
        }
    }
    None
}

// ── Family 3: `test && echo/printf || echo/printf` → native If ────────

/// `[[ -f f ]] && echo A || echo B` (a single-test chain whose then-arm
/// is an always-success builtin) → `If{cond: test, then:[echo A],
/// else:[echo B]}`. Refused for any other chain shape (fallible then-arm,
/// multi-link chains needing mid-chain status, `test && cmd` without the
/// `||` — its false-path status is the test's, an if/else cannot express
/// it).
fn test_chain_to_if(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::BinOp {
        op: BinOpKind::Or,
        lhs,
        rhs,
    }) = st
    else {
        return false;
    };
    let IrExpr::BinOp {
        op: BinOpKind::And,
        lhs: cond,
        rhs: then_expr,
    } = &**lhs
    else {
        return false;
    };
    let IrExpr::Call { func, .. } = cond.as_ref() else {
        return false;
    };
    if func != "test" {
        return false;
    }
    // then-arm must be ALWAYS-SUCCESS (echo/printf — status 0 on every
    // path, so bash's `|| else` short-circuits exactly like the if-form).
    let Some(then) = single_always_true_exec(then_expr) else {
        return false;
    };
    let Some(else_) = single_always_true_exec(rhs) else {
        return false;
    };
    *st = IrStmt::If {
        cond: (**cond).clone(),
        then,
        elsifs: vec![],
        else_,
    };
    true
}

/// A single exec of an always-success builtin (echo/printf — the builtins
/// whose statement rendering also records `$main_exit_code = 0`).
fn single_always_true_exec(e: &IrExpr) -> Option<Vec<IrStmt>> {
    if let Some((cmd, _)) = exec_parts(e) {
        if matches!(cmd, "echo" | "printf") {
            return Some(vec![IrStmt::Expr(e.clone())]);
        }
    }
    None
}

// ── Family 1: `echo/printf … > file` → native select-based redirect ──

/// `Redirect{inner:[Expr(exec echo|printf …)], [fd1 w|wc|a file]}`: wrap
/// the exec in a plain Block. The Perl redirect arm's shell-text rebuild
/// only covers bare calls (Expr/For/Subshell); a Block inner is NOT
/// rebuildable, so the native file-redirect fallback (open + select —
/// the exec renders as print/printf into the handle) fires instead of
/// `system('bash', '-c', …)`. The ESTree backend renders the identical
/// `sh2.redirect(body, specs)` runtime call for both inner shapes.
///
/// Refused when the redirect carries fd-0/-2 specs, `2>&1`-style dups,
/// heredocs/herestrings, or a non-file mode — the native fallback cannot
/// express those (REFUSE > GUESS).
fn file_redirect_block(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else {
        return false;
    };
    let [r] = redirects.as_slice() else {
        return false;
    };
    if r.fd.unwrap_or(1) != 1 {
        return false;
    }
    if !matches!(r.mode.as_str(), "w" | "wc" | "a") {
        return false;
    }
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else {
        return false;
    };
    if func != "exec" {
        return false;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(_)] = args.as_slice() else {
        return false;
    };
    if !matches!(cmd.as_str(), "echo" | "printf") {
        return false;
    }
    // Wrap the single exec in a Block (inner has exactly one statement).
    inner[0] = IrStmt::Block(vec![IrStmt::Expr(IrExpr::Call {
        func: func.clone(),
        args: args.clone(),
    })]);
    true
}

// ── Family 2: empty-input herestrings → provable-status exec ──────────

/// `cmd … <<< ''` on fd 0 with an EMPTY string: the command reads no
/// input, so it produces no output and its status is provable —
/// cat (no file args) / tr (any args) exit 0 printing nothing;
/// grep (pattern, no file args, no `-c`) exits 1 printing nothing
/// (no match). The whole redirect collapses to the `true`/`false` exec —
/// native everywhere. Any other shape (non-empty herestring, grep with
/// file args / `-c`, dynamIC words) keeps the shell-out.
fn empty_herestring_to_status(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else {
        return false;
    };
    let [r] = redirects.as_slice() else {
        return false;
    };
    if r.mode != "herestring" || r.fd.unwrap_or(0) != 0 {
        return false;
    }
    let empty = match &r.target {
        IrExpr::Str(s, _) => s.is_empty(),
        _ => false,
    };
    if !empty {
        return false;
    }
    let [IrStmt::Expr(inner)] = inner.as_slice() else {
        return false;
    };
    let Some((cmd, words)) = exec_parts(inner) else {
        return false;
    };
    match cmd {
        // empty stdin → no output, exit 0 (cat with no file operands).
        "cat" => {
            let arg_ok = words.is_empty()
                || matches!(words, [IrExpr::Str(s, _)] if s == "-");
            if !arg_ok {
                return false;
            }
            *st = status_exec(true);
            true
        }
        // any SET1/SET2 args are fine — the transform never runs on empty
        // input, so the sets are irrelevant; bare `tr` (no args) errors
        // (exit 1) and must keep the shell-out.
        "tr" => {
            if words.is_empty() {
                return false;
            }
            *st = status_exec(true);
            true
        }
        // grep NEEDS a pattern; no file args (exactly one non-flag word),
        // no `-c` (it would print `0`); an empty pattern is refused (the
        // runtime's empty-input grep shapes diverge at the fold boundary).
        "grep" => {
            let mut flags: Vec<&IrExpr> = Vec::new();
            let mut plain: Vec<&IrExpr> = Vec::new();
            for w in words {
                match w {
                    IrExpr::Str(s, _) if s.starts_with('-') => flags.push(w),
                    IrExpr::Str(..) | IrExpr::Interpolate(_) => plain.push(w),
                    _ => return false, // dynamic word — refuse
                }
            }
            if plain.len() != 1 {
                return false;
            }
            if flags.iter().any(|w| {
                matches!(w, IrExpr::Str(s, _) if s == "-c")
            }) {
                return false;
            }
            let pat_empty = match plain[0] {
                IrExpr::Str(s, _) => s.is_empty(),
                // an interpolated pattern may expand to empty/many words —
                // refuse (REFUSE > GUESS)
                _ => return false,
            };
            if pat_empty {
                return false;
            }
            *st = status_exec(false);
            true
        }
        _ => false,
    }
}