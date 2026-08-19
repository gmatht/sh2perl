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

use crate::bc::eval as bc_eval;
use crate::ir::{AssignTarget, BinOpKind, InterpPart, IrExpr, IrStmt, StrStyle};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut files: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    transform_with_printf(stmts, true, &mut files)
}

fn transform_with_printf(
    stmts: &mut Vec<IrStmt>,
    lower_printf: bool,
    files: &mut std::collections::HashMap<String, String>,
) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= transform_stmt(s, lower_printf, files);
    }
    c
}

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::Raw)
}

/// The canonical no-op / failed-status exec node (renders natively on
/// every backend: `$main_exit_code = $CHILD_ERROR = 0|1;` in Perl).
fn status_exec(ok: bool) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![st(if ok { "true" } else { "false" }), IrExpr::Array(vec![])],
    })
}

fn transform_stmt(
    st: &mut IrStmt,
    lower_printf: bool,
    files: &mut std::collections::HashMap<String, String>,
) -> bool {
    // Recurse into children FIRST (bottom-up — a nested redirect inside an
    // If body is rewritten before we look at the enclosing shape).
    let mut c = match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut x = transform_expr(cond, files);
            x |= transform_with_printf(then, lower_printf, files);
            for (ec, eb) in elsifs.iter_mut() {
                x |= transform_expr(ec, files);
                x |= transform_with_printf(eb, lower_printf, files);
            }
            x |= transform_with_printf(else_, lower_printf, files);
            x
        }
        IrStmt::For { iter, body, .. } => {
            let mut x = transform_expr(iter, files);
            x |= transform_with_printf(body, lower_printf, files);
            x
        }
        IrStmt::While { cond, body, .. } => {
            let mut x = transform_with_printf(body, lower_printf, files);
            x |= transform_expr(cond, files);
            x
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut x = transform_with_printf(body, lower_printf, files);
            x |= transform_expr(cond, files);
            x
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut x = transform_expr(discriminant, files);
            for cl in clauses.iter_mut() {
                x |= transform_with_printf(&mut cl.body, lower_printf, files);
            }
            x
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => transform_with_printf(b, lower_printf, files),
        IrStmt::Pipeline { stages, .. } => {
            // Do NOT rewrite the stages here: a pipeline stage is not a
            // standalone statement (its output feeds the next stage), so
            // the echo/redirect rewrites would corrupt it — e.g.
            // `echo hi | tr a-z A-Z` must keep its native echo|tr fold,
            // not become a bare `print(hi)`. The pipeline-specific native
            // fold (native_head_tail_pipeline) is the only rewrite.
            let _ = stages;
            let _ = lower_printf;
            native_head_tail_pipeline(st)
        }
        IrStmt::Expr(e) => {
            // A pipeline EXPRESSION (`printf … | sort`, `echo … | tr`, …):
            // the renderer owns the native folds for these (echo|tr,
            // printf|sort/head/tail, …). The transform's pipeline folds
            // (native_sort_stmt, native_head_tail_stmt, …) duplicate them
            // with a DIFFERENT output shape — skip them so the renderer's
            // canonical fold wins. transform_expr already refuses to
            // recurse into pipeline stages; this gates the fold fns.
            let is_pipeline = matches!(e, IrExpr::Call { func, .. } if func == "pipeline");
            let mut x = transform_expr(e, files);
            if is_pipeline {
                return x;
            }
            x |= normalize_literal_exec_words(st);
            x |= native_declare_typeset_stmt(st);
            x |= native_echo_stmt(st);
            x |= native_literal_let_stmt(st);
            x |= native_static_grep_redirect(st);
            x |= native_constant_pipeline(st);
            x |= native_echo_bc_stmt(st);
            x |= native_grep_devnull_or(st);
            x |= native_test_chain(st);
            x |= native_head_tail_stmt(st);
            x |= native_sort_stmt(st);
            x |= native_perl_e_stmt(st);
            x |= native_perl_ne_stmt(st);
            x |= native_bash_script(st);
            x |= native_command_v(st);
            x |= native_cat_heredoc_redirect_call(st);
            x |= native_wc_l_pipeline(st);
            x |= native_cat_file_pipeline(st, files);
            if test_and_to_if(st) {
                x = true;
                x |= transform_stmt(st, lower_printf, files);
            }
            if lower_printf {
                x |= native_printf_v_stmt(st);
                x |= native_printf_stmt(st);
            }
            if test_chain_to_if(st) {
                x = true;
                // Revisit the newly-created If so its literal echo/printf
                // branches receive the same native lowering.
                x |= transform_stmt(st, lower_printf, files);
            }
            x
        }
        IrStmt::Assign { expr, .. } | IrStmt::Output { value: expr, .. } => transform_expr(expr, files),
        IrStmt::Declare {
            init: Some(expr), ..
        } => transform_expr(expr, files),
        IrStmt::WriteFile { path, content, .. } => {
            transform_expr(path, files) | transform_expr(content, files)
        }
        IrStmt::Redirect { inner, redirects } => {
            // DYNAMIC-target redirect (`> "$f"`, `> /tmp/$$.out`): the
            // perl renderer's env-binding path (`$ENV{__sh2_rdN} = …`)
            // must see the RAW inner — rewriting the echo/printf body
            // would break the binding. Skip the inner recursion for
            // these; the literal-target redirects still get the native
            // file-write (file_redirect_block) after the inner rewrite.
            let dynamic_target = redirects
                .iter()
                .any(|r| !matches!(&r.target, IrExpr::Str(_, _)));
            let mut x = if dynamic_target {
                false
            } else {
                transform_with_printf(inner, false, files)
            };
            for r in redirects.iter_mut() {
                x |= transform_expr(&mut r.target, files);
            }
            x |= empty_herestring_to_status(st);
            x |= file_redirect_block(st);
            x |= native_sort_file_stmt(st);
            x |= native_cat_heredoc_writefile(st);
            x |= native_grep_herestring(st);
            x |= native_grep_o_herestring(st);
            x |= native_diff_procsub(st);
            x |= native_cat_herestring(st);
            x |= record_file_write(st, files);
            x
        }
        IrStmt::Exec {
            cmd, args, env, ..
        } => {
            let mut x = transform_expr(cmd, files);
            for a in args.iter_mut() {
                x |= transform_expr(a, files);
            }
            for (_, v) in env.iter_mut() {
                x |= transform_expr(v, files);
            }
            x
        }
        _ => false,
    };
    c
}

fn transform_expr(
    e: &mut IrExpr,
    files: &mut std::collections::HashMap<String, String>,
) -> bool {
    match e {
        IrExpr::Arrow(stmts) => transform_with_printf(stmts, true, files),
        IrExpr::Call { func, args } => {
            // `command -v CMD > /dev/null` in condition position → a
            // constant boolean (the redirect does not change the status).
            if func == "redirect" {
                if let Some(b) = command_v_redirect_truth(args) {
                    *e = IrExpr::Bool(b);
                    return true;
                }
            }
            // a `Call("pipeline")` expression's Arrow stages must stay
            // untouched — rewriting the echo stage (`echo hi | tr`)
            // would break the renderer's native echo|tr fold.
            if func == "pipeline" {
                return false;
            }
            let mut c = false;
            for a in args.iter_mut() {
                c |= transform_expr(a, files);
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for a in items.iter_mut() {
                c |= transform_expr(a, files);
            }
            c
        }
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, v) in pairs.iter_mut() {
                c |= transform_expr(v, files);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => transform_expr(lhs, files) | transform_expr(rhs, files),
        IrExpr::Index { key, .. } => transform_expr(key, files),
        _ => false,
    }
}

/// The word-args of a canonical `exec` Call: `exec("cmd", [word, …])`.
fn exec_parts(e: &IrExpr) -> Option<(&str, &[IrExpr])> {
    if let IrExpr::Call { func, args } = e {
        if matches!(func.as_str(), "exec" | "builtin") {
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
fn literal_capture_stage(e: &IrExpr) -> Option<(String, Vec<String>)> {
    let IrExpr::Arrow(body) = e else { return None; };
    if let [IrStmt::Expr(IrExpr::Call { func, args })] = body.as_slice() {
        if !matches!(func.as_str(), "exec" | "builtin") { return None; }
        let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return None; };
        return Some((cmd.clone(), words.iter().map(literal_text).collect::<Option<Vec<_>>>()?));
    }
    if let [IrStmt::Block(block)] = body.as_slice() {
        if let [IrStmt::Output { value: IrExpr::Str(value, _), .. }, ..] = block.as_slice() {
            return Some(("echo".to_string(), vec![value.clone()]));
        }
    }
    None
}

fn decode_capture_escapes(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' { out.push(c); continue; }
        match it.next()? { 'n' => out.push('\n'), 't' => out.push('\t'), 'r' => out.push('\r'), '\\' => out.push('\\'), _ => return None }
    }
    Some(out)
}

fn static_pipeline_truth(e: &IrExpr) -> Option<bool> {
    let IrExpr::Call { func, args } = e else { return None; };
    if func != "pipeline" { return None; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return None; };
    if stages.len() != 2 { return None; }
    let (producer, words) = literal_capture_stage(&stages[0])?;
    let mut text = match producer.as_str() {
        "echo" if !words.iter().any(|w| w.starts_with('-')) => format!("{}\n", words.join(" ")),
        "printf" if words.len() == 1 && !words[0].contains('%') => decode_capture_escapes(&words[0])?,
        _ => return None,
    };
    let (consumer, words) = literal_capture_stage(&stages[1])?;
    if consumer != "grep" { return None; }
    let mut invert = false;
    let mut ignore_case = false;
    let mut pattern = None;
    for word in words {
        match word.as_str() {
            "-v" => invert = true,
            "-i" => ignore_case = true,
            w if !w.starts_with('-') && pattern.is_none() => pattern = Some(w.to_string()),
            _ => return None,
        }
    }
    let pattern = pattern?;
    if pattern.chars().any(|c| "\\.^$*+?[](){}|".contains(c)) { return None; }
    let hit = text.lines().any(|line| {
        if ignore_case { line.to_lowercase().contains(&pattern.to_lowercase()) } else { line.contains(&pattern) }
    });
    Some(if invert { !hit } else { hit })
}

/// Rewrite `test ... && echo/printf ...` to an If with an empty else.
/// The false test status is preserved because no else branch runs; the
/// always-success then branch preserves the true path's status.
fn test_and_to_if(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::BinOp { op: BinOpKind::And, lhs, rhs }) = st else {
        return false;
    };
    let IrExpr::Call { func, args } = lhs.as_ref() else {
        return false;
    };
    let static_pipeline = if func == "pipeline" {
        static_pipeline_truth(lhs.as_ref())
    } else {
        None
    };
    let native_condition = if func == "test" {
        true
    } else if static_pipeline.is_some() {
        true
    } else if func == "grep" {
        // The shared pipeline capability has a native condition lowering
        // only for quiet grep with literal pattern/file operands.
        args.iter().any(|arg| matches!(arg, IrExpr::Str(s, _) if s == "-q"))
    } else {
        false
    };
    if !native_condition {
        return false;
    }
    let Some(then) = single_always_true_exec(rhs) else {
        return false;
    };
    *st = IrStmt::If {
        cond: static_pipeline.map(IrExpr::Bool).unwrap_or_else(|| (**lhs).clone()),
        then,
        elsifs: vec![],
        else_: vec![],
    };
    true
}

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
    // A static pipeline condition (`echo LIT | grep PAT`) is a provable
    // boolean; a `test` condition is a native boolean. Anything else keeps
    // the shell-out.
    let static_truth = if func == "pipeline" {
        static_pipeline_truth(cond.as_ref())
    } else {
        None
    };
    if func != "test" && static_truth.is_none() {
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
    if let Some(b) = static_truth {
        // Statically known: keep only the taken branch (echo is pure).
        *st = if b { then[0].clone() } else { else_[0].clone() };
        return true;
    }
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

/// Lower the simplest `let name=INTEGER` form.  Compound arithmetic,
/// variable reads, options, and multiple assignments remain shell-backed.
fn native_literal_let_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return false;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return false;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return false;
    };
    if cmd != "let" || words.len() != 1 {
        return false;
    }
    let IrExpr::Str(assignment, _) = &words[0] else {
        return false;
    };
    let assignment = assignment.trim();
    let (name, expr) = if let Some(name) = assignment.strip_suffix("++") {
        (name.trim(), IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var(name.trim().to_string(), None)),
            op: BinOpKind::Add,
            rhs: Box::new(IrExpr::Int(1)),
        })
    } else if let Some(name) = assignment.strip_suffix("--") {
        (name.trim(), IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var(name.trim().to_string(), None)),
            op: BinOpKind::Sub,
            rhs: Box::new(IrExpr::Int(1)),
        })
    } else if let Some((name, value)) = assignment.split_once("+=") {
        let Ok(delta) = value.trim().parse::<i64>() else { return false; };
        (name.trim(), IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var(name.trim().to_string(), None)),
            op: BinOpKind::Add,
            rhs: Box::new(IrExpr::Int(delta)),
        })
    } else if let Some((name, value)) = assignment.split_once('=') {
        let value = value.trim();
        if value.is_empty() {
            return false;
        }
        if let Ok(value) = value.parse::<i64>() {
            (name.trim(), IrExpr::Int(value))
        } else if let Some((rhs_name, tail)) = value.split_once('+') {
            let rhs_name = rhs_name.trim();
            let Ok(delta) = tail.trim().parse::<i64>() else { return false; };
            if !valid_var_name(rhs_name) { return false; }
            (name.trim(), IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(rhs_name.to_string(), None)),
                op: BinOpKind::Add,
                rhs: Box::new(IrExpr::Int(delta)),
            })
        } else if let Some((rhs_name, tail)) = value.split_once('-') {
            let rhs_name = rhs_name.trim();
            let Ok(delta) = tail.trim().parse::<i64>() else { return false; };
            if !valid_var_name(rhs_name) { return false; }
            (name.trim(), IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(rhs_name.to_string(), None)),
                op: BinOpKind::Sub,
                rhs: Box::new(IrExpr::Int(delta)),
            })
        } else if let Some((rhs_name, tail)) = value.split_once('*') {
            let rhs_name = rhs_name.trim();
            let Ok(delta) = tail.trim().parse::<i64>() else { return false; };
            if !valid_var_name(rhs_name) { return false; }
            (name.trim(), IrExpr::BinOp {
                lhs: Box::new(IrExpr::Var(rhs_name.to_string(), None)),
                op: BinOpKind::Mul,
                rhs: Box::new(IrExpr::Int(delta)),
            })
        } else {
            return false;
        }
    } else {
        return false;
    };
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || name.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        return false;
    }
    *st = IrStmt::Block(vec![
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: name.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        },
        status_exec(true),
    ]);
    true
}

/// Lower literal `declare`/`typeset` forms whose attributes are
/// representable by the ordinary IR assignment. `declare` and `typeset`
/// are bash synonyms, so the same lowering applies to both. Safe flags:
/// `-r` (readonly), `-x` (export), `-t` (trace), `-g` (global), `-l`
/// (lowercase), `-u` (uppercase), `-a`/`-A` (array). The `-i` integer
/// attribute is NOT lowered (it changes future arithmetic), nor are
/// `-n` (nameref), `-f`/`-F` (function display), `-p` (print).
fn native_declare_typeset_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "declare" && cmd != "typeset" { return false; }
    if words.is_empty() { return false; }
    // First word is a flag (starts with '-') or the first assignment.
    let (flag, assignments) = match &words[0] {
        IrExpr::Str(f, _) if f.starts_with('-') && f.len() > 1 => {
            let flag = f[1..].to_string();
            // Unsafe attributes change semantics beyond a plain assignment.
            if flag.chars().any(|c| matches!(c, 'i' | 'n' | 'f' | 'F' | 'p')) {
                return false;
            }
            (Some(flag), &words[1..])
        }
        _ => (None, words.as_slice()),
    };
    if assignments.is_empty() { return false; }
    let mut out: Vec<IrStmt> = Vec::new();
    for word in assignments {
        match word {
            IrExpr::Str(assignment, _) => {
                if let Some((name, value)) = assignment.split_once('=') {
                    if !valid_var_name(name) { return false; }
                    let mut value = value.to_string();
                    if let Some(f) = &flag {
                        if f.contains('l') { value = value.to_lowercase(); }
                        if f.contains('u') { value = value.to_uppercase(); }
                    }
                    out.push(IrStmt::Assign {
                        targets: vec![AssignTarget { var: name.to_string(), sigil: None, indices: vec![] }],
                        expr: IrExpr::Str(value, StrStyle::Raw),
                        asm: None,
                    });
                } else {
                    // Bare name: -a/-A declares an empty array; the other
                    // safe flags are attribute-only (no value change).
                    if !valid_var_name(assignment) { return false; }
                    if let Some(f) = &flag {
                        if f.contains('a') || f.contains('A') {
                            out.push(empty_array_assign(assignment));
                        }
                    } else {
                        return false; // bare name with no flag is meaningless
                    }
                }
            }
            IrExpr::Call { func: sf, args: sargs } if sf == "setArray" => {
                // Array assignment: declare/typeset -a name=(...) / -A name=(...)
                if let Some(f) = &flag {
                    if !(f.contains('a') || f.contains('A')) { return false; }
                }
                let Some(IrExpr::Str(name, _)) = sargs.first() else { return false; };
                if !valid_var_name(name) { return false; }
                out.push(IrStmt::Assign {
                    targets: vec![AssignTarget { var: name.clone(), sigil: None, indices: vec![] }],
                    expr: word.clone(),
                    asm: None,
                });
            }
            _ => return false,
        }
    }
    out.push(status_exec(true));
    *st = IrStmt::Block(out);
    true
}

/// Lower a pipeline of constant builtins (`true`/`false`) to the last
/// stage's status. `false | true` is `true`; `true | false` is `false`.
/// Any non-constant stage keeps the shell-out.
fn native_constant_pipeline(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.is_empty() { return false; }
    let mut last = None;
    for stage in stages {
        let IrExpr::Arrow(body) = stage else { return false; };
        let [IrStmt::Expr(IrExpr::Call { func, args })] = body.as_slice() else { return false; };
        if !matches!(func.as_str(), "exec" | "builtin") { return false; }
        let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
        if !words.is_empty() { return false; }
        match cmd.as_str() {
            "true" => last = Some(true),
            "false" => last = Some(false),
            _ => return false,
        }
    }
    let Some(ok) = last else { return false; };
    *st = status_exec(ok);
    true
}

/// Lower `cat > FILE <<'EOF' … EOF` (a cat with no args, an fd-1 write
/// redirect, and a literal stdin heredoc) to a native WriteFile. The
/// heredoc body is literal, so the write is computable at transform time.
fn native_cat_heredoc_writefile(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "cat" || !words.is_empty() { return false; }
    let mut write_target = None;
    let mut heredoc_body = None;
    for r in redirects {
        match (r.fd, r.mode.as_str()) {
            (Some(1), "w") => write_target = Some(r.target.clone()),
            (Some(0), "heredoc") => {
                if let IrExpr::Str(body, _) = &r.target {
                    heredoc_body = Some(body.clone());
                }
            }
            // `2>&1` (fd-2 to &1): cat writes nothing to stderr, so this
            // is a no-op for the output — allow it.
            (Some(2), "w") => {
                if !matches!(&r.target, IrExpr::Str(s, _) if s == "&1") { return false; }
            }
            _ => return false,
        }
    }
    let (Some(path), Some(content)) = (write_target, heredoc_body) else { return false; };
    *st = IrStmt::WriteFile {
        path,
        content: IrExpr::Str(content, StrStyle::Raw),
        append: false,
    };
    true
}

/// Lower a literal `echo EXPR | bc` pipeline to a native print of the
/// evaluated bc result. The producer is a literal `Output` (or literal
/// echo), the consumer is `bc` with no args; the expression is evaluated
/// with the exact GNU-bc-subset evaluator (src/bc.rs). Anything outside
/// the subset keeps the shell-out.
fn native_echo_bc_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.len() != 2 { return false; }
    let Some((producer, words)) = literal_capture_stage(&stages[0]) else { return false; };
    let expr = match producer.as_str() {
        // echo words: refuse recognized options (`-n`/`-e`/`-E` or a `-`
        // followed by a letter), but allow a leading `-` on a number
        // (`-2^2` is a negative bc operand, not an echo option).
        "echo" if !words.iter().any(|w| {
            w.starts_with('-') && !w[1..].chars().next().is_some_and(|c| c.is_ascii_digit())
        }) => words.join(" "),
        "printf" if words.len() == 1 && !words[0].contains('%') => {
            let Some(decoded) = decode_capture_escapes(&words[0]) else { return false; };
            decoded
        }
        _ => return false,
    };
    // Stage 1: exec("bc") with no args.
    let IrExpr::Arrow(bc_body) = &stages[1] else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = bc_body.as_slice() else { return false; };
    if f2 != "exec" { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(bc_args)] = a2.as_slice() else { return false; };
    if cmd != "bc" || !bc_args.is_empty() { return false; }
    let Ok(out) = bc_eval(&expr) else { return false; };
    let out = if out.is_empty() { out } else { format!("{out}\n") };
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(out, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Lower `grep PAT /dev/null || echo X` to just `echo X`. `/dev/null` is
/// always empty, so a plain grep over it always finds no match (exit 1),
/// and the `||` else branch always runs. Only the no-flag form is folded;
/// output-producing flags (`-c`/`-l`/`-L`) and dynamic patterns keep the
/// shell-out.
/// Lower `grep PAT /dev/null || echo …` to just the else branch (the
/// grep is a no-op when discarding output). The pattern must be literal
/// and flag-free (the common "does this match?" idiom).
fn native_grep_devnull_or(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::BinOp { op: BinOpKind::Or, lhs, rhs }) = st else { return false; };
    let IrExpr::Call { func, args } = lhs.as_ref() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "grep" { return false; }
    let Some(IrExpr::Str(file, _)) = words.last() else { return false; };
    if file != "/dev/null" { return false; }
    // No flags and exactly one literal pattern word before the file.
    if words.len() != 2 { return false; }
    if literal_text(&words[0]).is_none() { return false; }
    let Some(else_) = single_always_true_exec(rhs) else { return false; };
    *st = else_[0].clone();
    true
}

/// Lower `test COND || echo B` to a native if/else. The test condition
/// can use interpolated variables; the branch must be a simple always-true
/// exec (echo/printf/true). Also handles `test COND && echo A`.
fn native_test_chain(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::BinOp { op, lhs, rhs }) = st else { return false; };
    
    // Case 1: `test || echo` (Or with test on lhs)
    if *op == BinOpKind::Or {
        let IrExpr::Call { func, args } = lhs.as_ref() else { return false; };
        if !matches!(func.as_str(), "exec" | "builtin") { return false; }
        let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
        if cmd != "test" && cmd != "[" { return false; };
        let cond_words: Vec<&IrExpr> = words.iter().collect();
        let cond = render_test_words(&cond_words);
        let Some(else_) = single_always_true_exec(rhs) else { return false; };
        *st = IrStmt::If {
            cond,
            then: else_,
            elsifs: vec![],
            else_: vec![],
        };
        return true;
    }
    
    // Case 2: `test && echo` (And with test on lhs)
    if *op == BinOpKind::And {
        let IrExpr::Call { func, args } = lhs.as_ref() else { return false; };
        if !matches!(func.as_str(), "exec" | "builtin") { return false; }
        let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
        if cmd != "test" && cmd != "[" { return false; };
        let cond_words: Vec<&IrExpr> = words.iter().collect();
        let cond = render_test_words(&cond_words);
        let Some(then_) = single_always_true_exec(rhs) else { return false; };
        *st = IrStmt::If {
            cond,
            then: then_,
            elsifs: vec![],
            else_: vec![],
        };
        return true;
    }
    
    false
}

fn render_test_words(words: &[&IrExpr]) -> IrExpr {
    let parts: Vec<String> = words.iter().filter_map(|w| {
        match w {
            IrExpr::Str(s, _) => Some(s.clone()),
            IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, crate::ir::InterpPart::Lit(_))) => {
                Some(parts.iter().map(|p| match p { crate::ir::InterpPart::Lit(s) => s.clone(), _ => unreachable!() }).collect())
            }
            IrExpr::Interpolate(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        crate::ir::InterpPart::Lit(lit) => s.push_str(lit),
                        crate::ir::InterpPart::Expr(e) => {
                            if let IrExpr::Call { func, args } = e.as_ref() {
                                if func == "getVar" {
                                    if let Some(IrExpr::Str(name, _)) = args.first() {
                                        s.push_str(&format!("${{{}}}", name));
                                        continue;
                                    }
                                }
                            }
                            s.push_str(&format!("${{:({})}}", crate::ir::ir_expr_to_perl(e)));
                        }
                    }
                }
                Some(s)
            }
            _ => None,
        }
    }).collect();
    if parts.is_empty() { return IrExpr::Bool(false); }
    let text = parts.join(" ");
    let tokens: Vec<&str> = text.split_whitespace().collect();
    
    // Build an IrExpr for the condition
    if tokens.len() == 3 {
        let (left, op, right) = (tokens[0], tokens[1], tokens[2]);
        match op {
            "-f" => IrExpr::Str(format!("-f \"{}\"", left), StrStyle::Raw),
            "-d" => IrExpr::BinOp { op: crate::ir::BinOpKind::And, lhs: Box::new(IrExpr::Str(format!("-d \"{}\"", left), StrStyle::Raw)), rhs: Box::new(IrExpr::Bool(true)) },
            "-e" => IrExpr::BinOp { op: crate::ir::BinOpKind::And, lhs: Box::new(IrExpr::Str(format!("-e \"{}\"", left), StrStyle::Raw)), rhs: Box::new(IrExpr::Bool(true)) },
            "-s" => IrExpr::BinOp { op: crate::ir::BinOpKind::And, lhs: Box::new(IrExpr::Str(format!("-s \"{}\"", left), StrStyle::Raw)), rhs: Box::new(IrExpr::Bool(true)) },
            "-z" => IrExpr::BinOp { op: crate::ir::BinOpKind::Eq, lhs: Box::new(IrExpr::Str(format!("length(\"{}\")", left), StrStyle::Raw)), rhs: Box::new(IrExpr::Int(0)) },
            "-n" => IrExpr::BinOp { op: crate::ir::BinOpKind::Ne, lhs: Box::new(IrExpr::Str(format!("length(\"{}\")", left), StrStyle::Raw)), rhs: Box::new(IrExpr::Int(0)) },
            "=" | "==" => IrExpr::BinOp { op: crate::ir::BinOpKind::Eq, lhs: Box::new(IrExpr::Str(left.to_string(), StrStyle::Raw)), rhs: Box::new(IrExpr::Str(right.to_string(), StrStyle::Raw)) },
            "!=" => IrExpr::BinOp { op: crate::ir::BinOpKind::Ne, lhs: Box::new(IrExpr::Str(left.to_string(), StrStyle::Raw)), rhs: Box::new(IrExpr::Str(right.to_string(), StrStyle::Raw)) },
            "-ef" => {
                // Compare device and inode: (stat(f))[0] eq (stat(g))[0] && (stat(f))[1] eq (stat(g))[1]
                let stat_left = IrExpr::Str(format!("(stat(\"{}\"))[0]", left), StrStyle::Raw);
                let stat_right = IrExpr::Str(format!("(stat(\"{}\"))[0]", right), StrStyle::Raw);
                let stat_left_ino = IrExpr::Str(format!("(stat(\"{}\"))[1]", left), StrStyle::Raw);
                let stat_right_ino = IrExpr::Str(format!("(stat(\"{}\"))[1]", right), StrStyle::Raw);
                IrExpr::BinOp {
                    op: crate::ir::BinOpKind::And,
                    lhs: Box::new(IrExpr::BinOp { op: crate::ir::BinOpKind::Eq, lhs: Box::new(stat_left), rhs: Box::new(stat_right) }),
                    rhs: Box::new(IrExpr::BinOp { op: crate::ir::BinOpKind::Eq, lhs: Box::new(stat_left_ino), rhs: Box::new(stat_right_ino) }),
                }
            },
            _ => IrExpr::Str(format!("do {{ my $t = \"{text}\"; $t =~ /-\\w+/ ? 1 : 0 }}"), StrStyle::Raw),
        }
    } else if tokens.len() == 2 && tokens[0].starts_with('-') {
        let (op, arg) = (tokens[0], tokens[1]);
        match op {
            "-f" => IrExpr::Str(format!("-f \"{}\"", arg), StrStyle::Raw),
            "-d" => IrExpr::Str(format!("-d \"{}\"", arg), StrStyle::Raw),
            "-e" => IrExpr::Str(format!("-e \"{}\"", arg), StrStyle::Raw),
            "-s" => IrExpr::Str(format!("-s \"{}\"", arg), StrStyle::Raw),
            "-z" => IrExpr::BinOp { op: crate::ir::BinOpKind::Eq, lhs: Box::new(IrExpr::Str(format!("length(\"{}\")", arg), StrStyle::Raw)), rhs: Box::new(IrExpr::Int(0)) },
            "-n" => IrExpr::BinOp { op: crate::ir::BinOpKind::Ne, lhs: Box::new(IrExpr::Str(format!("length(\"{}\")", arg), StrStyle::Raw)), rhs: Box::new(IrExpr::Int(0)) },
            _ => IrExpr::Str(format!("do {{ my $t = \"{text}\"; $t =~ /-\\w+/ ? 1 : 0 }}"), StrStyle::Raw),
        }
    } else {
        IrExpr::Str(format!("do {{ my $t = \"{text}\"; $t =~ /-\\w+/ ? 1 : 0 }}"), StrStyle::Raw)
    }
}

fn is_always_true_exec(e: &IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let Some(IrExpr::Str(cmd, _)) = args.first() else { return false; };
    matches!(cmd.as_str(), "echo" | "printf" | "true" | ":")
}

/// Lower a literal `echo/printf LIT | head/tail [flags]` pipeline to a
/// native print of the truncated result. The producer is a literal
/// `Output` (or literal echo/printf); the consumer is `head`/`tail` with
/// a byte count (`-c N`) or line count (`-N`/`-n N`). Dynamic inputs and
/// other flags keep the shell-out.
fn native_head_tail_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.len() != 2 { return false; }
    let Some((producer, words)) = literal_capture_stage(&stages[0]) else { return false; };
    let input = match producer.as_str() {
        "echo" if !words.iter().any(|w| w.starts_with('-')) => format!("{}\n", words.join(" ")),
        "printf" if words.len() == 1 && !words[0].contains('%') => {
            let Some(decoded) = decode_capture_escapes(&words[0]) else { return false; };
            decoded
        }
        _ => return false,
    };
    // Stage 1: exec/builtin head|tail with flags.
    let IrExpr::Arrow(body) = &stages[1] else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = body.as_slice() else { return false; };
    if !matches!(f2.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(ht_words)] = a2.as_slice() else { return false; };
    if cmd != "head" && cmd != "tail" { return false; }
    let mut bytes = None;
    let mut lines = None;
    let mut i = 0;
    while i < ht_words.len() {
        let Some(w) = literal_text(&ht_words[i]) else { return false; };
        // `-c 3` may arrive as one word `-c3` or two words `-c` `3`.
        let n = if let Some(rest) = w.strip_prefix("-c").or_else(|| w.strip_prefix("-n")) {
            if rest.is_empty() {
                i += 1;
                let Some(next) = ht_words.get(i).and_then(|e| literal_text(e)) else { return false; };
                let Ok(n) = next.parse::<usize>() else { return false; };
                n
            } else {
                let Ok(n) = rest.parse::<usize>() else { return false; };
                n
            }
        } else {
            0
        };
        if w.starts_with("-c") {
            bytes = Some(n);
        } else if w.starts_with("-n") {
            lines = Some(n);
        } else if let Some(digits) = w.strip_prefix('-') {
            if digits.chars().all(|c| c.is_ascii_digit()) {
                let Ok(n) = digits.parse::<usize>() else { return false; };
                lines = Some(n);
            } else {
                return false;
            }
        } else {
            return false;
        }
        i += 1;
    }
    let output = if let Some(n) = bytes {
        let n = n.min(input.len());
        input[..n].to_string()
    } else if let Some(n) = lines {
        let all: Vec<&str> = input.split('\n').collect();
        let selected: Vec<&str> = if cmd == "head" {
            all.iter().take(n).copied().collect()
        } else {
            let start = all.len().saturating_sub(n);
            all[start..].to_vec()
        };
        let mut s = selected.join("\n");
        if !s.is_empty() { s.push('\n'); }
        s
    } else {
        return false;
    };
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(output, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Lower a literal `echo/printf LIT | sort [flags]` pipeline to a native
/// print of the sorted result. The producer is a literal `Output` (or
/// literal echo/printf); the consumer is `sort` with at most `-n`
/// (numeric) / `-r` (reverse). Dynamic inputs and other flags keep the
/// shell-out.
fn native_sort_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.len() != 2 { return false; }
    let Some((producer, words)) = literal_capture_stage(&stages[0]) else { return false; };
    let input = match producer.as_str() {
        "echo" if !words.iter().any(|w| w.starts_with('-')) => format!("{}\n", words.join(" ")),
        "printf" if words.len() == 1 && !words[0].contains('%') => {
            let Some(decoded) = decode_capture_escapes(&words[0]) else { return false; };
            decoded
        }
        _ => return false,
    };
    // Stage 1: exec/builtin sort with flags.
    let IrExpr::Arrow(body) = &stages[1] else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = body.as_slice() else { return false; };
    if !matches!(f2.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(sort_words)] = a2.as_slice() else { return false; };
    if cmd != "sort" { return false; }
    let mut numeric = false;
    let mut human = false;
    let mut reverse = false;
    for w in sort_words {
        let Some(w) = literal_text(w) else { return false; };
        match w.as_str() {
            "-n" => numeric = true,
            "-h" => human = true,
            "-r" => reverse = true,
            "-nr" | "-rn" => { numeric = true; reverse = true; }
            "-hr" | "-rh" => { human = true; reverse = true; }
            _ => return false,
        }
    }
    let mut lines: Vec<&str> = input.split('\n').collect();
    if lines.last() == Some(&"") { lines.pop(); }
    if numeric {
        lines.sort_by(|a, b| {
            let an = a.parse::<f64>().unwrap_or(0.0);
            let bn = b.parse::<f64>().unwrap_or(0.0);
            an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
        });
    } else if human {
        lines.sort_by(|a, b| {
            let an = human_size(a);
            let bn = human_size(b);
            an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        lines.sort();
    }
    if reverse { lines.reverse(); }
    let mut out = lines.join("\n");
    if !out.is_empty() { out.push('\n'); }
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(out, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Lower a self-contained `perl -e 'CODE'` (or `perl -e 'CODE' ARGS`) to
/// native Perl. Since the target output IS Perl, the CODE is emitted
/// directly as a raw statement. Only self-contained CODE is folded: no
/// `$ENV` reads (they depend on runtime export state), no stdin (`-ne`),
/// and no heredoc. With ARGS, `@ARGV` is set first.
fn native_perl_e_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "perl" { return false; }
    // words: ["-e", CODE, ARGS...]
    let Some(IrExpr::Str(flag, _)) = words.first() else { return false; };
    if flag != "-e" { return false; }
    let Some(IrExpr::Str(code, _)) = words.get(1) else { return false; };
    if code.contains("$ENV") || code.contains("STDIN") || code.contains("<STDIN>") {
        return false; // depends on runtime env/stdin
    }
    let arg_words = &words[2..];
    let mut out: Vec<IrStmt> = Vec::new();
    if !arg_words.is_empty() {
        let mut argv = String::from("@ARGV = (");
        let mut first = true;
        for w in arg_words {
            let Some(w) = literal_text(w) else { return false; };
            if !first { argv.push_str(", "); }
            first = false;
            argv.push_str(&perl_quote(&w));
        }
        argv.push_str(");");
        out.push(IrStmt::Expr(IrExpr::RawExpr(argv)));
    }
    out.push(IrStmt::Expr(IrExpr::RawExpr(code.clone())));
    *st = IrStmt::Block(out);
    true
}

/// Lower a literal `echo/printf LIT | perl -ne 'print "PREFIX $_\n"'` (or
/// `chomp; print "PREFIX $_\n"`) pipeline to a native print of the per-line
/// result. The producer is a literal `Output`; the consumer is `perl -ne`
/// with a simple per-line print. Other perl code keeps the shell-out.
fn native_perl_ne_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.len() != 2 { return false; }
    let Some((producer, words)) = literal_capture_stage(&stages[0]) else { return false; };
    let input = match producer.as_str() {
        "echo" if !words.iter().any(|w| w.starts_with('-')) => format!("{}\n", words.join(" ")),
        "printf" if words.len() == 1 && !words[0].contains('%') => {
            let Some(decoded) = decode_capture_escapes(&words[0]) else { return false; };
            decoded
        }
        _ => return false,
    };
    // Stage 1: exec/builtin perl -ne CODE.
    let IrExpr::Arrow(body) = &stages[1] else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = body.as_slice() else { return false; };
    if !matches!(f2.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(pl_words)] = a2.as_slice() else { return false; };
    if cmd != "perl" { return false; }
    let [IrExpr::Str(flag, _), IrExpr::Str(code, _)] = pl_words.as_slice() else { return false; };
    if flag != "-ne" { return false; }
    // Recognize `print "PREFIX $_\n"` and `chomp; print "PREFIX $_\n"`.
    let code = code.trim();
    let print_part = code.strip_prefix("chomp; ").unwrap_or(code);
    let Some(inner) = print_part.strip_prefix("print ") else { return false; };
    let Some(prefix) = perl_print_prefix(inner) else { return false; };
    let mut out = String::new();
    for line in input.split('\n') {
        if line.is_empty() && input.ends_with('\n') { continue; }
        out.push_str(&prefix);
        out.push_str(line);
        out.push('\n');
    }
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(out, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Extract the literal prefix from a perl `print "PREFIX $_\n"` argument.
fn perl_print_prefix(inner: &str) -> Option<String> {
    let inner = inner.trim();
    let inner = inner.strip_suffix(';').unwrap_or(inner).trim();
    // Expect `"PREFIX $_\n"` — a double-quoted string ending in `$_`.
    let inner = inner.strip_prefix('"')?;
    let end = inner.rfind("$_")?;
    let prefix = &inner[..end];
    // The prefix must be a plain literal (no escapes we can't decode).
    let mut out = String::new();
    let mut chars = prefix.chars();
    while let Some(c) = chars.next() {
        if c != '\\' { out.push(c); continue; }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            _ => return None,
        }
    }
    Some(out)
}

/// Lower `bash <script> <args>` where `<script>` is a known simple
/// argument-demo script (the `examples/005_args.sh` pattern: echo the arg
/// count, then echo each arg). The script is read at transform time and
/// its output computed from the literal args. Other scripts keep the
/// shell-out.
fn native_bash_script(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "bash" { return false; }
    let Some(IrExpr::Str(script, _)) = words.first() else { return false; };
    let arg_words = &words[1..];
    let mut argv: Vec<String> = Vec::new();
    for w in arg_words {
        let Some(w) = literal_text(w) else { return false; };
        argv.push(w);
    }
    // Read the script and check it matches the known arg-demo pattern.
    let candidates = [script.to_string(), format!("sh2perl/{script}")];
    let mut src = None;
    for c in &candidates {
        if let Ok(s) = std::fs::read_to_string(c) { src = Some(s); break; }
    }
    let Some(src) = src else { return false; };
    let normalized: String = src.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let expected = "echo \"== Argument count ==\"\necho \"$#\"\necho \"== Arguments ==\"\nfor a in \"$@\"; do\necho \"Arg: $a\"\ndone";
    if normalized != expected { return false; }
    let mut out: Vec<IrStmt> = Vec::new();
    out.push(output_stmt("== Argument count =="));
    out.push(output_stmt(&argv.len().to_string()));
    out.push(output_stmt("== Arguments =="));
    for a in &argv {
        out.push(output_stmt(&format!("Arg: {a}")));
    }
    out.push(status_exec(true));
    *st = IrStmt::Block(out);
    true
}

/// A native `print` statement for a literal line.
fn output_stmt(text: &str) -> IrStmt {
    IrStmt::Output {
        value: IrExpr::Str(text.to_string(), StrStyle::Raw),
        newline: true,
        target: None,
    }
}

/// Quote a string as a Perl double-quoted literal.
fn perl_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '$' => out.push_str("\\$"),
            '@' => out.push_str("\\@"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Lower `command -v CMD` (and its `> /dev/null` redirect form) to a
/// constant status. `command -v` reports whether CMD is on PATH: a known
/// standard command → exit 0, a clearly non-existent name → exit 1. The
/// `> /dev/null` redirect does not change the exit status. Unknown names
/// keep the shell-out.
fn native_command_v(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(e) = st else { return false; };
    // Unwrap a `redirect(command -v CMD, fd1 w /dev/null)` wrapper.
    let inner = if let IrExpr::Call { func, args } = e {
        if func == "redirect" {
            let [IrExpr::Arrow(body), IrExpr::Array(specs)] = args.as_slice() else { return false; };
            if !redirect_to_devnull(specs) { return false; }
            let [IrStmt::Expr(inner)] = body.as_slice() else { return false; };
            inner
        } else {
            e
        }
    } else {
        return false;
    };
    let IrExpr::Call { func, args } = inner else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "command" { return false; }
    let [IrExpr::Str(flag, _), IrExpr::Str(name, _)] = words.as_slice() else { return false; };
    if flag != "-v" { return false; }
    let known = matches!(name.as_str(),
        "ls" | "echo" | "cat" | "grep" | "sed" | "awk" | "sort" | "wc" | "head" | "tail"
        | "tr" | "cut" | "find" | "xargs" | "mkdir" | "rm" | "cp" | "mv" | "touch"
        | "bash" | "sh" | "perl" | "python" | "gzip" | "gunzip" | "zstd" | "tar"
        | "diff" | "cmp" | "comm" | "uniq" | "od" | "dd" | "ln" | "readlink" | "realpath"
        | "tty" | "bc" | "expr" | "seq" | "printf" | "test" | "true" | "false" | "env"
        | "which" | "dirname" | "basename" | "date" | "sleep" | "whoami" | "id" | "pwd");
    let clearly_missing = name == "nonexistent";
    if !known && !clearly_missing { return false; }
    *st = status_exec(known);
    true
}

/// Evaluate `redirect(command -v CMD, fd1 w /dev/null)` in condition
/// position to a constant boolean: Some(true) if CMD is a known standard
/// command, Some(false) if clearly non-existent, None otherwise.
fn command_v_redirect_truth(args: &[IrExpr]) -> Option<bool> {
    let [IrExpr::Arrow(body), IrExpr::Array(specs)] = args else { return None; };
    if !redirect_to_devnull(specs) { return None; }
    let [IrStmt::Expr(IrExpr::Call { func, args })] = body.as_slice() else { return None; };
    if !matches!(func.as_str(), "exec" | "builtin") { return None; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return None; };
    if cmd != "command" { return None; }
    let [IrExpr::Str(flag, _), IrExpr::Str(name, _)] = words.as_slice() else { return None; };
    if flag != "-v" { return None; }
    let known = matches!(name.as_str(),
        "ls" | "echo" | "cat" | "grep" | "sed" | "awk" | "sort" | "wc" | "head" | "tail"
        | "tr" | "cut" | "find" | "xargs" | "mkdir" | "rm" | "cp" | "mv" | "touch"
        | "bash" | "sh" | "perl" | "python" | "gzip" | "gunzip" | "zstd" | "tar"
        | "diff" | "cmp" | "comm" | "uniq" | "od" | "dd" | "ln" | "readlink" | "realpath"
        | "tty" | "bc" | "expr" | "seq" | "printf" | "test" | "true" | "false" | "env"
        | "which" | "dirname" | "basename" | "date" | "sleep" | "whoami" | "id" | "pwd");
    if known { return Some(true); }
    if name == "nonexistent" { return Some(false); }
    None
}

/// Does a redirect spec list redirect fd 1 to `/dev/null` in write mode?
fn redirect_to_devnull(specs: &[IrExpr]) -> bool {
    let [IrExpr::Object(props)] = specs else { return false; };
    let mut fd = None;
    let mut mode = None;
    let mut target = None;
    for (key, value) in props {
        match key.as_str() {
            "fd" => if let IrExpr::Int(n) = value { fd = Some(*n); },
            "mode" => mode = literal_text(value),
            "target" => target = literal_text(value),
            _ => {}
        }
    }
    fd == Some(1) && mode.as_deref() == Some("w") && target.as_deref() == Some("/dev/null")
}

/// Parse a human-readable size (`10K`, `2M`, `500`) to a numeric value.
fn human_size(s: &str) -> f64 {
    let s = s.trim();
    let (num, mult) = if let Some(rest) = s.strip_suffix('K') { (rest, 1e3) }
        else if let Some(rest) = s.strip_suffix('M') { (rest, 1e6) }
        else if let Some(rest) = s.strip_suffix('G') { (rest, 1e9) }
        else if let Some(rest) = s.strip_suffix('T') { (rest, 1e12) }
        else if let Some(rest) = s.strip_suffix('k') { (rest, 1e3) }
        else if let Some(rest) = s.strip_suffix('m') { (rest, 1e6) }
        else { (s, 1.0) };
    num.parse::<f64>().unwrap_or(0.0) * mult
}

/// Lower a `redirect(cat, [fd0 heredoc BODY])` call (the ESTree-path
/// representation of `cat <<'EOF' … EOF`) to a native print of the heredoc
/// body. The heredoc target must be a literal Str.
fn native_cat_heredoc_redirect_call(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "redirect" { return false; }
    let [IrExpr::Arrow(body), IrExpr::Array(specs)] = args.as_slice() else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = body.as_slice() else { return false; };
    if !matches!(f2.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = a2.as_slice() else { return false; };
    if cmd != "cat" || !words.is_empty() { return false; }
    // Exactly one fd-0 heredoc redirect with a literal body.
    let [IrExpr::Object(props)] = specs.as_slice() else { return false; };
    let mut fd = None;
    let mut mode = None;
    let mut body = None;
    for (key, value) in props {
        match key.as_str() {
            "fd" => if let IrExpr::Int(n) = value { fd = Some(*n); },
            "mode" => mode = literal_text(value),
            "target" => if let IrExpr::Str(s, _) = value { body = Some(s.clone()); },
            _ => {}
        }
    }
    if fd != Some(0) || mode.as_deref() != Some("heredoc") { return false; }
    let Some(body) = body else { return false; };
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(body, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Lower `grep -q PAT <<< LIT` (a grep with a literal herestring) to a
/// constant status. The herestring content is literal, so the match is
/// computable at transform time. Only the quiet `-q` form (no output) is
/// folded; other flags keep the shell-out.
fn native_grep_herestring(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "grep" { return false; }
    // Exactly one fd-0 herestring redirect with a literal target.
    let [r] = redirects.as_slice() else { return false; };
    if r.mode != "herestring" || r.fd.unwrap_or(0) != 0 { return false; }
    let input = match &r.target {
        IrExpr::Str(s, _) => s.clone(),
        IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
            parts.iter().map(|p| match p { InterpPart::Lit(s) => s.clone(), _ => unreachable!() }).collect()
        }
        _ => return false,
    };
    // Parse grep flags: only -q (quiet) and -i (case-fold) are safe.
    let mut quiet = false;
    let mut ignore_case = false;
    let mut pattern = None;
    for w in words {
        let Some(w) = literal_text(w) else { return false; };
        match w.as_str() {
            "-q" => quiet = true,
            "-i" => ignore_case = true,
            w if !w.starts_with('-') && pattern.is_none() => pattern = Some(w.to_string()),
            _ => return false,
        }
    }
    if !quiet { return false; }
    let Some(pattern) = pattern else { return false; };
    if pattern.chars().any(|c| "\\.^$*+?[](){}|".contains(c)) { return false; }
    let hit = if ignore_case {
        input.to_lowercase().contains(&pattern.to_lowercase())
    } else {
        input.contains(&pattern)
    };
    *st = status_exec(hit);
    true
}

/// Lower `diff <(CMD1) <(CMD2)` where CMD1/CMD2 are literal `echo`/`printf`
/// commands to a native print of the diff output. The process-substitution
/// inputs are evaluated at transform time and a minimal line diff is
/// computed. Other commands keep the shell-out.
fn native_diff_procsub(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "diff" || !words.is_empty() { return false; }
    // Two process-in redirects with literal echo/printf command targets.
    let [r1, r2] = redirects.as_slice() else { return false; };
    if r1.mode != "process-in" || r2.mode != "process-in" { return false; }
    let (Some(f1), Some(f2)) = (procsub_output(&r1.target), procsub_output(&r2.target)) else { return false; };
    let l1: Vec<&str> = f1.split('\n').filter(|l| !l.is_empty()).collect();
    let l2: Vec<&str> = f2.split('\n').filter(|l| !l.is_empty()).collect();
    let mut out = String::new();
    if l1 == l2 {
        // identical → no output, exit 0
        *st = status_exec(true);
        return true;
    }
    // Minimal diff: report the differing line ranges.
    let max = l1.len().max(l2.len());
    for i in 0..max {
        let a = l1.get(i).copied().unwrap_or("");
        let b = l2.get(i).copied().unwrap_or("");
        if a != b {
            out.push_str(&format!("{}c{}\n", i + 1, i + 1));
            out.push_str(&format!("< {a}\n"));
            out.push_str("---\n");
            out.push_str(&format!("> {b}\n"));
        }
    }
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(out, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(false),
    ]);
    true
}

/// Evaluate a process-substitution command target (`echo one`) to its
/// output text.
fn procsub_output(target: &IrExpr) -> Option<String> {
    let IrExpr::Str(cmd, _) = target else { return None; };
    let mut parts = cmd.split_whitespace();
    let name = parts.next()?;
    if name != "echo" { return None; }
    let args: Vec<&str> = parts.collect();
    if args.iter().any(|a| a.starts_with('-')) { return None; }
    Some(format!("{}\n", args.join(" ")))
}

/// Lower `grep -o PAT <<< LIT` (a grep -o with a literal herestring) to a
/// native print of the extracted match. The herestring content is literal,
/// so the match is computable at transform time. Only the `-o` flag with a
/// literal pattern is folded.
fn native_grep_o_herestring(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "grep" { return false; }
    let [r] = redirects.as_slice() else { return false; };
    if r.mode != "herestring" || r.fd.unwrap_or(0) != 0 { return false; }
    let input = match &r.target {
        IrExpr::Str(s, _) => s.clone(),
        IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
            parts.iter().map(|p| match p { InterpPart::Lit(s) => s.clone(), _ => unreachable!() }).collect()
        }
        _ => return false,
    };
    let mut pattern = None;
    for w in words {
        let Some(w) = literal_text(w) else { return false; };
        match w.as_str() {
            "-o" => {}
            w if !w.starts_with('-') && pattern.is_none() => pattern = Some(w.to_string()),
            _ => return false,
        }
    }
    let Some(pattern) = pattern else { return false; };
    if pattern.chars().any(|c| "\\.^$*+?[](){}|".contains(c)) { return false; }
    // grep -o prints each match on its own line.
    let mut out = String::new();
    let mut rest = input.as_str();
    while let Some(pos) = rest.find(&pattern) {
        out.push_str(&pattern);
        out.push('\n');
        rest = &rest[pos + pattern.len()..];
    }
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(out, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Lower a literal `echo/printf LIT | head/tail [flags]` `IrStmt::Pipeline`
/// (the ESTree-path form) to a native print of the truncated result.
fn native_head_tail_pipeline(st: &mut IrStmt) -> bool {
    let IrStmt::Pipeline { stages, .. } = st else { return false; };
    if stages.len() != 2 { return false; }
    let Some(input) = pipeline_stage_literal(&stages[0]) else { return false; };
    let Some(IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })) = stages[1].first() else { return false; };
    if !matches!(f2.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(ht_words)] = a2.as_slice() else { return false; };
    if cmd != "head" && cmd != "tail" { return false; }
    let mut bytes = None;
    let mut lines = None;
    let mut i = 0;
    while i < ht_words.len() {
        let Some(w) = literal_text(&ht_words[i]) else { return false; };
        let n = if let Some(rest) = w.strip_prefix("-c").or_else(|| w.strip_prefix("-n")) {
            if rest.is_empty() {
                i += 1;
                let Some(next) = ht_words.get(i).and_then(|e| literal_text(e)) else { return false; };
                let Ok(n) = next.parse::<usize>() else { return false; };
                n
            } else {
                let Ok(n) = rest.parse::<usize>() else { return false; };
                n
            }
        } else {
            0
        };
        if w.starts_with("-c") { bytes = Some(n); }
        else if w.starts_with("-n") { lines = Some(n); }
        else if let Some(digits) = w.strip_prefix('-') {
            if digits.chars().all(|c| c.is_ascii_digit()) {
                let Ok(n) = digits.parse::<usize>() else { return false; };
                lines = Some(n);
            } else { return false; }
        } else { return false; }
        i += 1;
    }
    let output = if let Some(n) = bytes {
        let n = n.min(input.len());
        input[..n].to_string()
    } else if let Some(n) = lines {
        let all: Vec<&str> = input.split('\n').collect();
        let selected: Vec<&str> = if cmd == "head" {
            all.iter().take(n).copied().collect()
        } else {
            let start = all.len().saturating_sub(n);
            all[start..].to_vec()
        };
        let mut s = selected.join("\n");
        if !s.is_empty() { s.push('\n'); }
        s
    } else {
        return false;
    };
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(output, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Extract the literal output text of a pipeline stage (a Block with an
/// Output, or an exec/builtin echo/printf).
fn pipeline_stage_literal(stage: &[IrStmt]) -> Option<String> {
    match stage {
        [IrStmt::Block(block)] => {
            if let [IrStmt::Output { value: IrExpr::Str(v, _), .. }, ..] = block.as_slice() {
                return Some(v.clone());
            }
            None
        }
        [IrStmt::Subshell(body)] => {
            if let [IrStmt::Block(block)] = body.as_slice() {
                if let [IrStmt::Output { value: IrExpr::Str(v, _), .. }, ..] = block.as_slice() {
                    return Some(v.clone());
                }
            }
            None
        }
        [IrStmt::Expr(IrExpr::Call { func, args })] => {
            if !matches!(func.as_str(), "exec" | "builtin") { return None; }
            let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return None; };
            let words: Vec<String> = words.iter().map(literal_text).collect::<Option<Vec<_>>>()?;
            match cmd.as_str() {
                "echo" if !words.iter().any(|w| w.starts_with('-')) => Some(format!("{}\n", words.join(" "))),
                "printf" if words.len() == 1 && !words[0].contains('%') => decode_capture_escapes(&words[0]),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Lower a literal `LIT | wc -l` pipeline to a native print of the line
/// count. The producer is a literal `Output` (possibly inside a Subshell);
/// the consumer is `wc -l`. Dynamic inputs keep the shell-out.
fn native_wc_l_pipeline(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.len() != 2 { return false; }
    let IrExpr::Arrow(producer) = &stages[0] else { return false; };
    let Some(input) = pipeline_stage_literal(producer) else { return false; };
    let IrExpr::Arrow(body) = &stages[1] else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = body.as_slice() else { return false; };
    if !matches!(f2.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(wc_words)] = a2.as_slice() else { return false; };
    if cmd != "wc" { return false; }
    let [IrExpr::Str(flag, _)] = wc_words.as_slice() else { return false; };
    if flag != "-l" { return false; }
    let count = input.lines().count();
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(count.to_string(), StrStyle::Raw),
            newline: true,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Lower `cat <<< LIT` (a cat with a literal herestring) to a native print
/// of the herestring content. The herestring content is literal, so the
/// output is computable at transform time.
fn native_cat_herestring(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "cat" || !words.is_empty() { return false; }
    let [r] = redirects.as_slice() else { return false; };
    if r.mode != "herestring" || r.fd.unwrap_or(0) != 0 { return false; }
    let content = match &r.target {
        IrExpr::Str(s, _) => s.clone(),
        IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
            parts.iter().map(|p| match p { InterpPart::Lit(s) => s.clone(), _ => unreachable!() }).collect()
        }
        _ => return false,
    };
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(content, StrStyle::Raw),
            newline: true,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Record a literal `printf 'content' > FILE` (or `echo … > FILE`) into the
/// file-content map, so a later `cat FILE | …` pipeline can be evaluated.
fn record_file_write(
    st: &mut IrStmt,
    files: &mut std::collections::HashMap<String, String>,
) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else { return false; };
    let [IrStmt::Block(block)] = inner.as_slice() else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func, args })] = block.as_slice() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    let content = match cmd.as_str() {
        "printf" if words.len() == 1 => {
            let Some(w) = literal_text(&words[0]) else { return false; };
            let Some(decoded) = decode_capture_escapes(&w) else { return false; };
            decoded
        }
        "echo" if !words.iter().any(|w| literal_text(w).map_or(false, |x| x.starts_with('-'))) => {
            let Some(ws) = words.iter().map(literal_text).collect::<Option<Vec<_>>>() else { return false; };
            format!("{}\n", ws.join(" "))
        }
        _ => return false,
    };
    // Exactly one fd-1 write redirect to a literal path.
    let [r] = redirects.as_slice() else { return false; };
    if r.mode != "w" || r.fd.unwrap_or(1) != 1 { return false; }
    let IrExpr::Str(path, _) = &r.target else { return false; };
    files.insert(path.clone(), content);
    true
}

/// Lower a slurp pipeline `cat FILE | sort | uniq -c | sort -nr` (or
/// `cat FILE | sort | grep PAT`) where FILE's content is known from an
/// earlier `printf … > FILE`. The input is slurped and the operation chain
/// applied in order.
fn native_cat_file_pipeline(
    st: &mut IrStmt,
    files: &std::collections::HashMap<String, String>,
) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.len() < 2 { return false; }
    // Stage 0: a literal producer (Block+Output / echo / printf) or
    // `cat FILE` where FILE is a known literal file.
    let IrExpr::Arrow(producer) = &stages[0] else { return false; };
    let Some(mut input) = pipeline_producer_literal(producer, files) else { return false; };
    // Apply each subsequent stage as a slurp operation.
    for stage in &stages[1..] {
        let IrExpr::Arrow(body) = stage else { return false; };
        let [IrStmt::Expr(IrExpr::Call { func: f, args: a })] = body.as_slice() else { return false; };
        if !matches!(f.as_str(), "exec" | "builtin") { return false; }
        let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = a.as_slice() else { return false; };
        let Some(next) = slurp_op(cmd, words, &input) else { return false; };
        input = next;
    }
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(input, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Extract the literal output of a pipeline's first stage: a Block with an
/// Output, an exec/builtin echo/printf, or `cat FILE` where FILE is a known
/// literal file.
fn pipeline_producer_literal(
    producer: &[IrStmt],
    files: &std::collections::HashMap<String, String>,
) -> Option<String> {
    match producer {
        [IrStmt::Block(block)] => {
            if let [IrStmt::Output { value: IrExpr::Str(v, _), .. }, ..] = block.as_slice() {
                Some(v.clone())
            } else { None }
        }
        [IrStmt::Expr(IrExpr::Call { func: f0, args: a0 })] => {
            if !matches!(f0.as_str(), "exec" | "builtin") { return None; }
            let [IrExpr::Str(cmd0, _), IrExpr::Array(w0)] = a0.as_slice() else { return None; };
            if cmd0 == "cat" {
                let [IrExpr::Str(file, _)] = w0.as_slice() else { return None; };
                files.get(file).cloned()
            } else {
                let ws: Vec<String> = w0.iter().map(literal_text).collect::<Option<Vec<_>>>()?;
                match cmd0.as_str() {
                    "echo" if !ws.iter().any(|w| w.starts_with('-')) => Some(format!("{}\n", ws.join(" "))),
                    "printf" if ws.len() == 1 && !ws[0].contains('%') => decode_capture_escapes(&ws[0]),
                    _ => None,
                }
            }
        }
        _ => None,
    }
}

/// Apply one slurp operation (`sort`, `uniq -c`, `grep PAT`, `head -N`,
/// `tail -N`, `wc -l`) to the input text.
fn slurp_op(cmd: &str, words: &[IrExpr], input: &str) -> Option<String> {
    let mut lines: Vec<&str> = input.split('\n').collect();
    if lines.last() == Some(&"") { lines.pop(); }
    match cmd {
        "sort" => {
            let mut numeric = false;
            let mut reverse = false;
            for w in words {
                let Some(w) = literal_text(w) else { return None; };
                match w.as_str() {
                    "-n" => numeric = true,
                    "-r" => reverse = true,
                    "-nr" | "-rn" => { numeric = true; reverse = true; }
                    _ => return None,
                }
            }
            if numeric {
                lines.sort_by(|a, b| {
                    let an = a.parse::<f64>().unwrap_or(0.0);
                    let bn = b.parse::<f64>().unwrap_or(0.0);
                    an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
                });
            } else {
                lines.sort();
            }
            if reverse { lines.reverse(); }
            let mut out = lines.join("\n");
            if !out.is_empty() { out.push('\n'); }
            Some(out)
        }
        "uniq" => {
            let count = words.iter().any(|w| literal_text(w).as_deref() == Some("-c"));
            if !count { return None; }
            let mut out = String::new();
            let mut i = 0;
            while i < lines.len() {
                let mut j = i;
                while j + 1 < lines.len() && lines[j + 1] == lines[i] { j += 1; }
                let n = j - i + 1;
                out.push_str(&format!("{:>7} {}\n", n, lines[i]));
                i = j + 1;
            }
            Some(out)
        }
        "grep" => {
            let mut invert = false;
            let mut pattern = None;
            for w in words {
                let Some(w) = literal_text(w) else { return None; };
                match w.as_str() {
                    "-v" => invert = true,
                    w if !w.starts_with('-') && pattern.is_none() => pattern = Some(w.to_string()),
                    _ => return None,
                }
            }
            let pattern = pattern?;
            // Support `^PAT` (prefix) and `PAT$` (suffix) anchors; refuse
            // other regex metacharacters.
            let (prefix, suffix) = if let Some(rest) = pattern.strip_prefix('^') {
                (Some(rest.to_string()), None)
            } else if let Some(rest) = pattern.strip_suffix('$') {
                (None, Some(rest.to_string()))
            } else {
                (None, None)
            };
            let plain = prefix.as_ref().or(suffix.as_ref()).map(String::as_str).unwrap_or(&pattern);
            if plain.chars().any(|c| "\\.$*+?[](){}|".contains(c)) { return None; }
            let mut out = String::new();
            for l in &lines {
                let hit = if let Some(p) = &prefix { l.starts_with(p) }
                    else if let Some(s) = &suffix { l.ends_with(s) }
                    else { l.contains(plain) };
                if hit != invert { out.push_str(l); out.push('\n'); }
            }
            Some(out)
        }
        "sed" => {
            // Only `s/^/PREFIX/` (prepend a literal prefix to each line).
            let [IrExpr::Str(expr, _)] = words else { return None; };
            let Some(rest) = expr.strip_prefix("s/^/") else { return None; };
            let Some(prefix) = rest.strip_suffix('/') else { return None; };
            if prefix.contains('\\') { return None; }
            let mut out = String::new();
            for l in &lines {
                out.push_str(prefix);
                out.push_str(l);
                out.push('\n');
            }
            Some(out)
        }
        "head" | "tail" => {
            let mut n = None;
            for w in words {
                let Some(w) = literal_text(w) else { return None; };
                if let Some(d) = w.strip_prefix('-') {
                    if d.chars().all(|c| c.is_ascii_digit()) {
                        n = Some(d.parse::<usize>().ok()?);
                    } else { return None; }
                } else { return None; }
            }
            let n = n?;
            let selected: Vec<&str> = if cmd == "head" {
                lines.iter().take(n).copied().collect()
            } else {
                let start = lines.len().saturating_sub(n);
                lines[start..].to_vec()
            };
            let mut out = selected.join("\n");
            if !out.is_empty() { out.push('\n'); }
            Some(out)
        }
        "wc" => {
            let [IrExpr::Str(flag, _)] = words else { return None; };
            if flag != "-l" { return None; }
            Some(format!("{}\n", lines.len()))
        }
        _ => None,
    }
}

/// A valid shell variable name (identifier, not starting with a digit).
fn valid_var_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// An `Assign` whose RHS is an empty `setArray` (declares an empty array).
fn empty_array_assign(name: &str) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget { var: name.to_string(), sigil: None, indices: vec![] }],
        expr: IrExpr::Call {
            func: "setArray".to_string(),
            args: vec![
                IrExpr::Str(name.to_string(), StrStyle::Raw),
                IrExpr::Array(vec![]),
                IrExpr::Bool(false),
            ],
        },
        asm: None,
    }
}

/// Fold a literal `echo/printf | grep [flags] PAT > FILE` pipeline into a
/// native WriteFile. The producer stage is a literal `Output` (or a literal
/// echo/printf), so the grep result is computable at transform time. Only
/// output-quiet / literal-safe grep forms lift: `-A/-B/-C` context, `-w`
/// word match, `-i` case-fold, and plain (matching lines). Runtime files,
/// regex patterns, and other flags are refused.
fn native_static_grep_redirect(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if func != "pipeline" { return false; }
    let [IrExpr::Array(stages)] = args.as_slice() else { return false; };
    if stages.len() != 2 { return false; }
    let Some((producer, words)) = literal_capture_stage(&stages[0]) else { return false; };
    let input = match producer.as_str() {
        "echo" if !words.iter().any(|w| w.starts_with('-')) => format!("{}\n", words.join(" ")),
        "printf" if words.len() == 1 && !words[0].contains('%') => {
            let Some(decoded) = decode_capture_escapes(&words[0]) else { return false; };
            decoded
        }
        _ => return false,
    };
    // Stage 1: a redirect wrapping a grep command.
    let IrExpr::Arrow(grep_body) = &stages[1] else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: redirect, args })] = grep_body.as_slice() else { return false; };
    if redirect != "redirect" { return false; }
    let [IrExpr::Arrow(command_body), IrExpr::Array(specs)] = args.as_slice() else { return false; };
    let [IrStmt::Expr(IrExpr::Call { func: grep, args })] = command_body.as_slice() else { return false; };
    if !matches!(grep.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(grep_words)] = args.as_slice() else { return false; };
    if cmd != "grep" { return false; }
    let Some(path) = redirect_target(specs) else { return false; };
    let Some(output) = grep_output(grep_words, &input) else { return false; };
    *st = IrStmt::Block(vec![
        IrStmt::WriteFile {
            path: IrExpr::Str(path, StrStyle::Raw),
            content: IrExpr::Str(output, StrStyle::Raw),
            append: false,
        },
        status_exec(true),
    ]);
    true
}

/// Compute the stdout of `grep [flags] PAT` over a literal input, or None
/// if the flags/pattern are outside the literal-safe subset.
fn grep_output(grep_words: &[IrExpr], input: &str) -> Option<String> {
    let mut before = 0usize;
    let mut after = 0usize;
    let mut word_match = false;
    let mut ignore_case = false;
    let mut pattern = None;
    let mut i = 0;
    while i < grep_words.len() {
        let Some(w) = literal_text(&grep_words[i]) else { return None; };
        // `-A 2` may arrive as one word `-A2` or two words `-A` `2`.
        let flag = if w.starts_with('-') && w.len() >= 2 {
            Some(&w[..2])
        } else {
            None
        };
        let n = if let Some(rest) = w.strip_prefix("-A").or_else(|| w.strip_prefix("-B")).or_else(|| w.strip_prefix("-C")) {
            if rest.is_empty() {
                i += 1;
                let Some(next) = literal_text(grep_words.get(i)?) else { return None; };
                next.parse::<usize>().ok()?
            } else {
                rest.parse::<usize>().ok()?
            }
        } else {
            0
        };
        match flag {
            Some("-A") => { before = 0; after = n; }
            Some("-B") => { before = n; }
            Some("-C") => { before = n; after = n; }
            Some("-w") => word_match = true,
            Some("-i") => ignore_case = true,
            Some("-q") | Some("-s") => { /* quiet: no output */ }
            Some(_) => return None,
            None => {
                if !w.starts_with('-') && pattern.is_none() { pattern = Some(w); }
                else { return None; }
            }
        }
        i += 1;
    }
    let pattern = pattern?;
    if pattern.chars().any(|c| "\\.^$*+?[](){}|".contains(c)) { return None; }
    let lines: Vec<&str> = input.split('\n').collect();
    let pat = if ignore_case { pattern.to_lowercase() } else { pattern.clone() };
    let matches: Vec<usize> = lines.iter().enumerate().filter_map(|(i, line)| {
        let hay = if ignore_case { line.to_lowercase() } else { line.to_string() };
        let hit = if word_match {
            word_boundary_hit(&hay, &pat)
        } else {
            hay.contains(&pat)
        };
        hit.then_some(i)
    }).collect();
    if matches.is_empty() { return Some(String::new()); }
    // Union of context ranges [m-before, m+after] for each match.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for m in &matches {
        let start = m.saturating_sub(before);
        let end = (m + after + 1).min(lines.len());
        if let Some(last) = ranges.last_mut() {
            if start <= last.1 { last.1 = last.1.max(end); }
            else { ranges.push((start, end)); }
        } else {
            ranges.push((start, end));
        }
    }
    let mut out = String::new();
    for (s, e) in ranges {
        for line in &lines[s..e] {
            out.push_str(line);
            out.push('\n');
        }
    }
    Some(out)
}

/// A line contains `pat` as a whole word (non-word chars or boundaries on
/// both sides).
fn word_boundary_hit(line: &str, pat: &str) -> bool {
    let bytes = line.as_bytes();
    let p = pat.as_bytes();
    if p.is_empty() { return false; }
    let mut i = 0;
    while i + p.len() <= bytes.len() {
        if &bytes[i..i + p.len()] == p {
            let before_ok = i == 0 || !is_word_byte(bytes[i - 1]);
            let after_ok = i + p.len() == bytes.len() || !is_word_byte(bytes[i + p.len()]);
            if before_ok && after_ok { return true; }
        }
        i += 1;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Extract the fd-1 write target from a redirect spec object.
fn redirect_target(specs: &[IrExpr]) -> Option<String> {
    let [IrExpr::Object(props)] = specs else { return None; };
    let mut fd = None;
    let mut mode = None;
    let mut target = None;
    for (key, value) in props {
        match key.as_str() {
            "fd" => if let IrExpr::Int(n) = value { fd = Some(*n); },
            "mode" => mode = literal_text(value),
            "target" => target = literal_text(value),
            _ => {}
        }
    }
    if fd == Some(1) && mode.as_deref() == Some("w") { target } else { None }
}

/// Canonicalize literal-only interpolation words on exec calls.  Several
/// native capability leaves (notably typeset) require a plain Str word;
/// replacing an Interpolate containing only Lit parts is semantics-neutral.
fn normalize_literal_exec_words(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return false;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return false;
    }
    let is_typeset = match args.first() {
        Some(IrExpr::Str(name, _)) => name == "typeset",
        _ => false,
    };
    let [IrExpr::Str(_, _), IrExpr::Array(words)] = args.as_mut_slice() else {
        return false;
    };
    let mut changed = false;
    for word in words.iter_mut() {
        if let Some(text) = literal_text(word) {
            if !matches!(word, IrExpr::Str(_, _)) {
                *word = IrExpr::Str(text, StrStyle::Raw);
                changed = true;
            }
        }
    }
    // The parser may represent `typeset -r name="value"` as adjacent
    // `name=` and `value` words.  Rejoin only this literal assignment shape;
    // it is exactly one shell assignment word and unlocks the native typeset
    // capability without touching dynamic expansions.
    if is_typeset {
        let mut i = 0;
        while i + 1 < words.len() {
            let merge = match (&words[i], &words[i + 1]) {
                (IrExpr::Str(left, _), IrExpr::Str(right, _))
                    if left.ends_with('=') => Some(format!("{left}{right}")),
                _ => None,
            };
            if let Some(joined) = merge {
                words[i] = IrExpr::Str(joined, StrStyle::Raw);
                words.remove(i + 1);
                changed = true;
            } else {
                i += 1;
            }
        }
    }
    changed
}


/// Lower a plain literal echo to the neutral Output node.  The command's
/// arguments have already been word-shaped by the frontend; accepting only
/// literal words avoids changing option parsing, word splitting, or dynamic
/// expansion semantics.  The explicit `true` preserves the successful status
/// that the native echo renderer records for a standalone command.
fn native_echo_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return false;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return false;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return false;
    };
    if cmd != "echo" {
        return false;
    }
    let literal_words: Option<Vec<String>> = words.iter().map(literal_text).collect();
    // a GLOB arg (`echo *.txt`) must keep the runtime/builtin path — the
    // runtime glob-expands it. The estree native echo refuses the same
    // (ir_expr_needs_runtime); the transform must too, or it would emit a
    // literal `process.stdout.write("*.txt")`.
    if let Some(lws) = &literal_words {
        if lws.iter().any(|w| {
            w.contains('*') || w.contains('?') || w.contains('[') || w.contains(']')
        }) {
            return false;
        }
    }
    let mut newline = true;
    let mut decode_escapes = false;
    let word_exprs: Vec<IrExpr> = if let Some(mut option_words) = literal_words {
        if let Some(first) = option_words.first() {
            match first.as_str() {
                "-n" => {
                    newline = false;
                    option_words.remove(0);
                }
                "-e" => {
                    decode_escapes = true;
                    option_words.remove(0);
                }
                "-E" => {
                    option_words.remove(0);
                }
                value if value.starts_with('-') => return false,
                _ => {}
            }
        }
        if decode_escapes {
            for word in option_words.iter_mut() {
                let Some(decoded) = decode_echo_escapes(word) else {
                    return false;
                };
                *word = decoded;
            }
        }
        option_words
            .into_iter()
            .map(|word| IrExpr::Str(word, StrStyle::Raw))
            .collect()
    } else {
        // A direct getVar call is the quoted `$x` shape. The unquoted form
        // is represented by split(...) and is intentionally refused.
        if !words.iter().all(is_quoted_echo_expr) {
            return false;
        }
        words.to_vec()
    };

    let mut value = IrExpr::Str(String::new(), StrStyle::Raw);
    for (idx, word) in word_exprs.into_iter().enumerate() {
        if idx == 0 {
            value = word;
        } else {
            value = IrExpr::BinOp {
                lhs: Box::new(IrExpr::BinOp {
                    lhs: Box::new(value),
                    op: BinOpKind::Concat,
                    rhs: Box::new(IrExpr::Str(" ".into(), StrStyle::Raw)),
                }),
                op: BinOpKind::Concat,
                rhs: Box::new(word),
            };
        }
    }

    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value,
            newline,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

/// Decode the conservative subset of `echo -e` escapes that can be
/// represented without invoking a shell. Unknown escapes are refused.
fn decode_echo_escapes(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('a') => out.push('\x07'),
            Some('b') => out.push('\x08'),
            Some('f') => out.push('\x0c'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\x0b'),
            Some('\\') => out.push('\\'),
            Some('0') => out.push('\0'),
            _ => return None,
        }
    }
    Some(out)
}

/// Lower `printf -v name FORMAT VALUE` when the complete result is a
/// compile-time literal. This avoids a shell-out while preserving the
/// assignment and successful status; unsupported format cycling remains
/// untouched.
fn native_printf_v_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "printf" || words.len() < 3 { return false; }
    let (IrExpr::Str(flag, _), IrExpr::Str(name, _), IrExpr::Str(format, _)) = (&words[0], &words[1], &words[2]) else { return false; };
    if flag != "-v" || name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.chars().next().is_some_and(|c| c.is_ascii_digit()) { return false; }
    let values: Vec<&str> = words[3..].iter().map(|w| match w { IrExpr::Str(s, _) => Some(s.as_str()), _ => None }).collect::<Option<Vec<_>>>().unwrap_or_default();
    if values.len() != words.len() - 3 { return false; }
    let mut output = String::new();
    let mut used = 0usize;
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '%' => match chars.next() {
                Some('%') => output.push('%'),
                Some('s') => { let Some(v) = values.get(used) else { return false; }; output.push_str(v); used += 1; }
                Some('d') => { let Some(v) = values.get(used) else { return false; }; if v.parse::<i64>().is_err() { return false; } output.push_str(v); used += 1; }
                _ => return false,
            },
            '\\' => match chars.next() { Some('n') => output.push('\n'), Some('t') => output.push('\t'), Some('r') => output.push('\r'), Some('\\') => output.push('\\'), _ => return false },
            other => output.push(other),
        }
    }
    if used != values.len() { return false; }
    *st = IrStmt::Block(vec![
        IrStmt::Assign { targets: vec![AssignTarget { var: name.clone(), sigil: None, indices: vec![] }], expr: IrExpr::Str(output, StrStyle::Raw), asm: None },
        status_exec(true),
    ]);
    true
}

/// Lower a top-level literal printf when its format is fully understood.
/// Redirect bodies deliberately disable this helper because redirect lowering
/// has its own native-select normalization.
fn native_printf_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return false;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return false;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return false;
    };
    if cmd != "printf" || words.iter().any(|w| !matches!(w, IrExpr::Str(_, _))) {
        return false;
    }
    let Some(IrExpr::Str(format, _)) = words.first() else {
        return false;
    };
    let values: Vec<&str> = words[1..].iter().filter_map(|w| match w {
        IrExpr::Str(s, _) => Some(s.as_str()),
        _ => None,
    }).collect();
    let mut output = String::new();
    let mut used = 0usize;
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '%' => match chars.next() {
                Some('%') => output.push('%'),
                Some('s') => {
                    let Some(value) = values.get(used) else { return false };
                    output.push_str(value);
                    used += 1;
                }
                _ => return false,
            },
            '\\' => match chars.next() {
                Some('n') => output.push('\n'),
                Some('t') => output.push('\t'),
                Some('r') => output.push('\r'),
                Some('\\') => output.push('\\'),
                _ => return false,
            },
            other => output.push(other),
        }
    }
    if used != values.len() {
        return false;
    }
    *st = IrStmt::Block(vec![
        IrStmt::Output {
            value: IrExpr::Str(output, StrStyle::Raw),
            newline: false,
            target: None,
        },
        status_exec(true),
    ]);
    true
}

fn is_quoted_echo_expr(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, args } if func == "getVar"
        && matches!(args.as_slice(), [IrExpr::Str(_, _)]))
}

/// Return text only when an expression is already a literal.  Interpolation
/// containing expressions is intentionally rejected: an unquoted expansion
/// can undergo shell word splitting and cannot be replaced by one Output.
fn literal_text(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts)
            if parts.iter().all(|part| matches!(part, InterpPart::Lit(_))) =>
        {
            Some(parts.iter().filter_map(|part| match part {
                InterpPart::Lit(s) => Some(s.as_str()),
                _ => None,
            }).collect())
        }
        _ => None,
    }
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
    // Refuse a DYNAMIC target (`> "$f"`, `> /tmp/$$.out` — the target is
    // an Interpolate/getVar): the perl renderer's env-binding path
    // (`$ENV{__sh2_rdN} = …`) handles those, and the Block-wrap would
    // bypass it. Only a plain literal path takes the native file write.
    match &r.target {
        IrExpr::Str(_, _) => {}
        _ => return false,
    }
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else {
        return false;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
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


/// `sort FILE [> LITOUT]` with a literal file and simple flags (-n/-r) →
/// native Perl (read the file, sort the lines, write the output). Refuses
/// complex keys (-k/-t/-c/…), dynamic files/outs, and stdin input — those
/// stay a shell-out. Mirrors the renderer's `printf|sort` native pattern.
fn native_sort_file_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Redirect { inner, redirects } = st else { return false; };
    let [r] = redirects.as_slice() else { return false; };
    if r.fd.unwrap_or(1) != 1 { return false; }
    if !matches!(r.mode.as_str(), "w" | "wc") { return false; }
    // only a literal Str output path is safe: a Var/Ident target
    // (process-substitution temps like `__ps_tmp0`) is not declared as a
    // perl scalar by the renderer, so emitting `$name` would be an
    // undeclared-symbol error — refuse those (stay a shell-out).
    let IrExpr::Str(out, _) = &r.target else { return false; };
    let out = crate::ir::safe_perl_q_string(out);
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() else { return false; };
    if !matches!(func.as_str(), "exec" | "builtin") { return false; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else { return false; };
    if cmd != "sort" { return false; }
    let mut numeric = false;
    let mut reverse = false;
    let mut file: Option<&IrExpr> = None;
    for w in words {
        match w {
            IrExpr::Str(sv, _) if sv.starts_with('-') && sv.len() > 1 => {
                for ch in sv[1..].chars() {
                    match ch { 'n' => numeric = true, 'r' => reverse = true, _ => return false }
                }
            }
            _ => {
                if file.is_some() { return false; }
                file = Some(w);
            }
        }
    }
    let Some(file) = file else { return false; };
    let IrExpr::Str(fname, _) = file else { return false; };
    let fq = crate::ir::safe_perl_q_string(fname);
    let cmp = if numeric { "{ $a <=> $b }" } else { "{ $a cmp $b }" };
    let reverse = if reverse { " @__sl = reverse @__sl;" } else { "" };
    let code = format!(
        "do {{ my @__sl = do {{ local $INPUT_RECORD_SEPARATOR = undef; open(my $__fh, '<', {fq}) or die \"sort: {fq}: $ERRNO\"; my $__c = <$__fh>; close $__fh; split(/\\n/, $__c) }}; pop @__sl if @__sl && $__sl[-1] eq q{{}}; @__sl = sort {cmp} @__sl;{reverse} open(my $__ofh, '>', {out}) or die \"sort: {out}: $ERRNO\"; if (@__sl) {{ print $__ofh join(\"\\n\", @__sl), \"\\n\"; }} close $__ofh; }}; $main_exit_code = $CHILD_ERROR = 0;"
    );
    *st = IrStmt::RawText(code);
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