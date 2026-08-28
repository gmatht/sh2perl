//! text_ops: Recognize common shell commands and lower to semantic IR nodes.
//!
//! `echo X | cut -d',' -f2`  → FieldExtract
//! `echo X | tr 'a-z' 'A-Z'` → CaseTransform / CharTranslate
//! `echo X | sed 's/p/r/'`   → RegSub
//! `echo X | head -n 5`      → TakeLines
//! `echo X | tail -n 5`      → TakeLines
//! `echo X | wc -l`          → RegCount (newline count)
//! `echo X | wc -w`          → ArrayLen(Split)
//! `${#var}`                 → StrLen
//! `echo X | xargs`          → StringTrim (bare xargs, clean literal only)
//!
//! Each transform walks the statement list, recognizes a pattern,
//! and replaces the pipeline/exec with an IrStmt::Expr(IrExpr::Ext(...)).

use crate::ir::*;
use crate::shir_nodes::*;
use crate::shir_nodes::ExtExpr;
use std::sync::atomic::{AtomicUsize, Ordering};

static LIFT_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // text-ops is an EXPERIMENTAL lowering that changes the shIR shape
    // (pipelines/commands → ExtExpr nodes). It is opt-in ONLY: run when
    // DEBASHC_TRANSFORMS explicitly lists "text-ops". This keeps the
    // default corpus gate and unit tests green (the analyses and renderers
    // have conservative Ext-node defaults, but the byte-equal round-trip
    // and corpus tests still pin the UN-lowered shape).
    let enabled = std::env::var("DEBASHC_TRANSFORMS").unwrap_or_default();
    if !enabled.split(',').any(|s| s.trim() == "text-ops") {
        return false;
    }
    let before = LIFT_COUNT.load(Ordering::Relaxed);
    for stmt in stmts.iter_mut() {
        lower_stmt(stmt, true);
    }
    let after = LIFT_COUNT.load(Ordering::Relaxed);
    if after > before {
    }
    after > before
}

fn lower_stmt(stmt: &mut IrStmt, emit: bool) {
    match stmt {
        // ShIR pipeline: IrExpr::Call { func: "pipeline", args: [Array(stages)] }
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "pipeline" => {
            if let [IrExpr::Array(stages)] = args.as_slice() {
                if stages.len() == 2 {
                    if emit {
                        if let Some((replacement, already_nl)) = try_lower_pipeline(stages, true) {
                            // A statement-level pipeline PRINTS its result.
                            *stmt = IrStmt::Output { value: replacement, newline: !already_nl, target: None };
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                }
            }
        }
        // Here-string/here-doc: `cmd <<< "text"` — the text is the fd-0
        // redirect target, the command is the inner stage.
        IrStmt::Redirect { inner, redirects } => {
            // Find a here-string / heredoc on fd 0 → the input text
            if let Some(text_ir) = redirects.iter().find_map(|r| {
                if r.fd == Some(0) && (r.mode == "herestring" || r.mode == "heredoc") {
                    Some(r.target.clone())
                } else { None }
            }) {
                // Try to lower the inner command against the here-text
                if let [IrStmt::Expr(IrExpr::Call { func, args })] = inner.as_slice() {
                    if func == "exec" || func == "builtin" {
                        if let [IrExpr::Str(name, _), IrExpr::Array(cmd_args)] = args.as_slice() {
                            if emit {
                                if let Some(replacement) = try_lower_command(text_ir, name, cmd_args) {
                                    *stmt = IrStmt::Output { value: replacement, newline: true, target: None };
                                    LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            // Recurse into the inner body
            for s in inner.iter_mut() {
                lower_stmt(s, emit);
            }
        }
        // Plain builtin command: basename X / dirname X (no pipeline)
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" || func == "builtin" => {
            // Recurse into args first (to reach nested $(...) / param calls)
            for a in args.iter_mut() { lower_expr(a); }
            if let [IrExpr::Str(cmd, _), IrExpr::Array(cmd_args)] = args.as_slice() {
                if emit && (cmd == "basename" || cmd == "dirname") && !cmd_args.is_empty() {
                    let which = if cmd == "dirname" { "dirname" } else { "basename" };
                    if let Some(text) = arg_to_expr(&cmd_args[0]) {
                        *stmt = IrStmt::Output {
                            value: IrExpr::Ext(Box::new(PathName { text, which: which.to_string() })),
                            newline: true,
                            target: None,
                        };
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
        }
        IrStmt::Expr(expr) => {
            lower_expr(expr);
        }
        // Recurse into nested statement bodies (if/while/for/function/...)
        IrStmt::If { then, elsifs, else_, .. } => {
            for s in then.iter_mut() { lower_stmt(s, emit); }
            for (_, b) in elsifs.iter_mut() { for s in b.iter_mut() { lower_stmt(s, emit); } }
            for s in else_.iter_mut() { lower_stmt(s, emit); }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            for s in body.iter_mut() { lower_stmt(s, emit); }
        }
        IrStmt::For { body, .. } => { for s in body.iter_mut() { lower_stmt(s, emit); } }
        IrStmt::ForInit { init, body, .. } => {
            for s in init.iter_mut() { lower_stmt(s, emit); }
            for s in body.iter_mut() { lower_stmt(s, emit); }
        }
        IrStmt::Function { body, .. } => { for s in body.iter_mut() { lower_stmt(s, emit); } }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            for s in body.iter_mut() { lower_stmt(s, emit); }
        }
        IrStmt::Case { discriminant, clauses } => {
            lower_expr(discriminant);
            for c in clauses.iter_mut() { for s in c.body.iter_mut() { lower_stmt(s, emit); } }
        }
        IrStmt::Assign { expr, .. } => lower_expr(expr),
        IrStmt::Declare { init, .. } => { if let Some(e) = init { lower_expr(e); } }
        IrStmt::WriteFile { path, content, .. } => {
            lower_expr(path); lower_expr(content);
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => lower_expr(e),
        _ => {}
    }
}

fn lower_expr(expr: &mut IrExpr) {
    match expr {
        // ${#var} → StrLen
        IrExpr::Call { func, args } if func == "param" => {
            if let Some(replacement) = try_lower_param_len(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            // ${p##*/} → PathName(basename), ${p%/*} → PathName(dirname)
            if let Some(replacement) = try_lower_param_path(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            // ${var,,} → Case(lower), ${var^^} → Case(upper),
            // ${var:2:3} → SubStr(var, 2, 3)
            if let Some(replacement) = try_lower_param_op(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            for a in args.iter_mut() { lower_expr(a); }
        }
        // Nested pipeline in expression position (&& chains, command
        // substitution, ternary): `echo X | cmd` inside `... && ...`.
        IrExpr::Call { func, args } if func == "pipeline" => {
            if let [IrExpr::Array(stages)] = args.as_slice() {
                if stages.len() == 2 {
                    if let Some((replacement, _)) = try_lower_pipeline(stages, false) {
                        *expr = replacement;
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
            for a in args.iter_mut() { lower_expr(a); }
        }
        IrExpr::Arrow(body) => {
            for s in body.iter_mut() { lower_stmt(s, false); }
        }
        IrExpr::Capture { expr: inner, .. } => lower_expr(inner),
        IrExpr::Array(items) => { for i in items.iter_mut() { lower_expr(i); } }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(e) = p { lower_expr(e); }
            }
        }
        IrExpr::Index { key, .. } => lower_expr(key),
        IrExpr::BinOp { lhs, rhs, .. } => { lower_expr(lhs); lower_expr(rhs); }
        IrExpr::Ternary { cond, then, else_, .. } => { lower_expr(cond); lower_expr(then); lower_expr(else_); }
        // `${#s}` outside a string lowers to getVar("##s") — the raw length
        // marker. Reduce to StrLen(read(s)).
        IrExpr::Call { func, args } if func == "getVar" => {
            if let Some(replacement) = try_lower_getvar_len(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            for a in args.iter_mut() { lower_expr(a); }
        }
        // Nested builtin/exec command in expression position: basename/dirname
        // inside $(...) — e.g. `dirname "$(pwd)"`.
        IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
            // Recursively lower nested expressions in args first
            for a in args.iter_mut() { lower_expr(a); }
            // Then check if this is a reducible single command (basename/dirname)
            if let [IrExpr::Str(cmd, _), IrExpr::Array(cmd_args)] = args.as_slice() {
                if (cmd == "basename" || cmd == "dirname") && !cmd_args.is_empty() {
                    let which = if cmd == "dirname" { "dirname" } else { "basename" };
                    if let Some(text) = arg_to_expr(&cmd_args[0]) {
                        *expr = IrExpr::Ext(Box::new(PathName {
                            text,
                            which: which.to_string(),
                        }));
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
        }
        IrExpr::Call { args, .. } => { for a in args.iter_mut() { lower_expr(a); } }
        _ => {}
    }
}

// ── Pipeline lowering ────────────────────────────────────────────────

/// Try to lower a two-stage pipeline `stage1 | stage2` to a semantic node.
/// Stages are IrExpr::Arrow(body) from the `Call { func: "pipeline", args: [Array(stages)] }` form.
///
/// `stmt_level` marks statement position (the caller wraps the value in an
/// Output that PRINTS it): status-only commands (grep -q) must not reduce
/// there — bash prints nothing for them — so they fall back to the
/// original command.
fn try_lower_pipeline(stages: &[IrExpr], stmt_level: bool) -> Option<(IrExpr, bool)> {
    // Each stage is an Arrow function: Arrow([Stmt])
    let stage1_body = match &stages[0] {
        IrExpr::Arrow(body) => body.as_slice(),
        _ => return None,
    };
    let stage2_body = match &stages[1] {
        IrExpr::Arrow(body) => body.as_slice(),
        _ => return None,
    };

    // stage2 must be an exec/builtin call
    let (cmd_name, cmd_args) = match stage2_body {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "exec" || func == "builtin" => {
            if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                (name.as_str(), a.as_slice())
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // `yes X | head -n K` → RepeatStr(X+"\n", K) — a clean repeat idiom.
    if cmd_name == "head" {
        if let Some(replacement) = try_lower_yes_head(stage1_body, cmd_args) {
            return Some((replacement, true)); // RepeatStr already ends in \n
        }
    }

    // stage1 produces text (echo, a literal, …)
    let source = extract_text_from_stage(stage1_body)?;

    lower_text_cmd(source, cmd_name, cmd_args, stmt_level).map(|e| (e, false))
}

/// `yes "X" | head -n K` → RepeatStr("X\n", K). `yes` repeats "X\n";
/// head -n K keeps K lines.
fn try_lower_yes_head(stage1: &[IrStmt], head_args: &[IrExpr]) -> Option<IrExpr> {
    // stage1: exec/builtin yes X
    let [IrStmt::Expr(IrExpr::Call { func, args })] = stage1 else { return None };
    if !(func == "exec" || func == "builtin") { return None; }
    let [IrExpr::Str(cmd, _), IrExpr::Array(yes_args)] = args.as_slice() else { return None };
    if cmd != "yes" { return None; }
    let text = match yes_args.first() {
        Some(IrExpr::Str(s, _)) => s.clone(),
        Some(IrExpr::Interpolate(p)) if p.len() == 1 => {
            if let InterpPart::Lit(s) = &p[0] { s.clone() } else { return None }
        }
        _ => return None,
    };
    // head -n K → K
    let k = head_count(head_args)?;
    Some(IrExpr::Ext(Box::new(RepeatStr {
        text: IrExpr::Str(format!("{}\n", text), StrStyle::DoubleQuoted),
        count: IrExpr::Int(k),
    })))
}

/// Extract the -n K count from `head -n K` / `head -K`.
fn head_count(head_args: &[IrExpr]) -> Option<i64> {
    let strs: Vec<&str> = head_args.iter().filter_map(|a| match a {
        IrExpr::Str(s, _) => Some(s.as_str()),
        _ => None,
    }).collect();
    let mut i = 0;
    while i < strs.len() {
        if strs[i] == "-n" || strs[i] == "-c" {
            if let Some(c) = strs.get(i + 1) { return c.parse::<i64>().ok(); }
        } else if let Some(rest) = strs[i].strip_prefix('-') {
            if rest.len() >= 1 && !rest.chars().all(|c| !c.is_ascii_digit()) {
                if let Ok(n) = rest.parse::<i64>() { return Some(n); }
            }
        }
        i += 1;
    }
    None
}

/// Dispatch a single command against input text (used by both the pipeline
/// stage-2 and the here-string inner command).
///
/// Trailing-newline fidelity: head/tail, tr and sed reproduce the input's
/// final newline in their output byte-for-byte, but the reduced statement
/// prints through `Output { newline: true }` — so those reductions REFUSE
/// a source without a trailing newline (`echo -n`) rather than guess.
/// cut and xargs terminate their own output lines, and wc consumes the
/// text, so they accept either source.
///
/// `stmt_level`: grep -q is status-only — at statement position bash
/// prints NOTHING, so reducing it to a printed value is wrong; it only
/// reduces in expression/condition position.
fn lower_text_cmd(source: TextSource, cmd_name: &str, cmd_args: &[IrExpr], stmt_level: bool) -> Option<IrExpr> {
    let TextSource { text, trailing_newline, is_literal } = source;
    // Single-line proof: a literal with no embedded newline. The as-built
    // FieldExtract and RegSub nodes are SINGLE-LINE (split-and-index /
    // one regex application, no per-line loop), so they only fire when
    // the input is provably one line. cut/sed on a variable (or a
    // multi-line literal) fall back to the original command.
    let single_line_literal = is_literal
        && matches!(&text, IrExpr::Str(t, _) if !t.contains('\n'));
    match cmd_name {
        "cut" if single_line_literal => try_lower_cut(text, cmd_args),
        // tr / wc / head / tail operate on the WHOLE text (char maps,
        // counts, line slices) — multi-line variable sources are safe.
        "tr" if trailing_newline => try_lower_tr(text, cmd_args),
        "head" if trailing_newline => try_lower_head_tail(text, cmd_args, false),
        "tail" if trailing_newline => try_lower_head_tail(text, cmd_args, true),
        "wc" => try_lower_wc(text, trailing_newline, cmd_args),
        "sed" if single_line_literal => try_lower_sed(text, cmd_args),
        "grep" if !stmt_level => try_lower_grep(text, cmd_args),
        "xargs" => try_lower_xargs(text, cmd_args, is_literal),
        _ => None,
    }
}

/// A pipeline stage-1 recognized as a pure text producer: the text it
/// writes, and whether that text is followed by a trailing newline
/// (`echo` adds one; `echo -n` does not). Reductions whose byte-exactness
/// depends on the trailing newline (wc -l, head/tail, tr, sed) consult it.
struct TextSource {
    text: IrExpr,
    trailing_newline: bool,
    /// True when `text` is a compile-time literal (its exact bytes are
    /// known). Line-structured reductions whose as-built nodes are
    /// single-line (FieldExtract, RegSub) and the whitespace-sensitive
    /// xargs trim only fire on provably-safe literals.
    is_literal: bool,
}

/// Extract the text expression from a pipeline stage body (echo, etc.).
///
/// Accepted producers, per REFUSE > GUESS:
/// - `echo ARGS` (with optional `-n` / `-E` flags). `-e` is REFUSED: its
///   escape interpretation would need decoding here to stay byte-exact.
/// - a literal-only expression statement (a string / all-literal
///   interpolation) — NOT an arbitrary command; `paste | head` must not
///   reduce as if paste produced a literal string.
/// - `printf` is REFUSED entirely: it adds NO trailing newline and its
///   format string interprets `%` directives and backslash escapes, so
///   treating the args as literal text mis-counts `printf x | wc -l`
///   (bash: 0 — no newline; the naive lowering said 1).
///
/// echo args may be literals OR variable reads (Var / param() / an
/// interpolation) — `echo "$s" | cut` reduces with the read as the text.
fn extract_text_from_stage(stmts: &[IrStmt]) -> Option<TextSource> {
    match stmts {
        // echo ARGS → join args (with " ") as the text
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec" || func == "builtin" =>
        {
            if let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args.as_slice() {
                if name == "echo" {
                    let mut trailing_newline = true;
                    let mut rest: &[IrExpr] = echo_args.as_slice();
                    // Leading flags: -n (no trailing newline), -E (default
                    // escape behavior — a no-op), -e (REFUSE: would need
                    // escape decoding to stay byte-exact).
                    while let Some(IrExpr::Str(flag, _)) = rest.first() {
                        match flag.as_str() {
                            "-n" => { trailing_newline = false; rest = &rest[1..]; }
                            "-E" => { rest = &rest[1..]; }
                            "-e" => return None,
                            _ => break,
                        }
                    }
                    let text = join_echo_args(rest)?;
                    let is_literal = matches!(text, IrExpr::Str(..));
                    return Some(TextSource { text, trailing_newline, is_literal });
                }
            }
            None
        }
        // A literal-only expression (a string or all-literal interpolation),
        // NOT an arbitrary command.
        [IrStmt::Expr(e)] => match e {
            IrExpr::Str(..) => Some(TextSource { text: e.clone(), trailing_newline: true, is_literal: true }),
            IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
                Some(TextSource { text: e.clone(), trailing_newline: true, is_literal: true })
            }
            _ => None,
        },
        _ => None,
    }
}

/// Join echo's args with single spaces into one text expression. Args may
/// be literals or variable reads; anything else (a nested capture with
/// side effects, an array splat, …) refuses the reduction.
fn join_echo_args(args: &[IrExpr]) -> Option<IrExpr> {
    // Each arg → either a literal string or an expression part.
    enum Part { Lit(String), Expr(IrExpr) }
    let mut parts: Vec<Part> = Vec::new();
    for a in args {
        match a {
            IrExpr::Str(s, _) => parts.push(Part::Lit(s.clone())),
            IrExpr::Var(..) => parts.push(Part::Expr(a.clone())),
            IrExpr::Call { func, .. } if func == "param" || func == "getVar" => {
                parts.push(Part::Expr(a.clone()))
            }
            IrExpr::Interpolate(interp) => {
                // Accept interpolations built from literals and variable reads.
                for p in interp {
                    match p {
                        InterpPart::Lit(s) => parts.push(Part::Lit(s.clone())),
                        InterpPart::Expr(e) => match e.as_ref() {
                            IrExpr::Var(..) => parts.push(Part::Expr((**e).clone())),
                            IrExpr::Call { func, .. } if func == "param" || func == "getVar" => {
                                parts.push(Part::Expr((**e).clone()))
                            }
                            _ => return None,
                        },
                    }
                }
            }
            _ => return None,
        }
        parts.push(Part::Lit(" ".to_string()));
    }
    // Drop the trailing separator.
    parts.pop();
    if parts.is_empty() {
        return Some(IrExpr::Str(String::new(), StrStyle::DoubleQuoted));
    }
    // All-literal → a plain Str; otherwise an Interpolate.
    if parts.iter().all(|p| matches!(p, Part::Lit(_))) {
        let joined: String = parts
            .iter()
            .map(|p| match p { Part::Lit(s) => s.as_str(), _ => unreachable!() })
            .collect();
        return Some(IrExpr::Str(joined, StrStyle::DoubleQuoted));
    }
    // Single bare expression (echo "$s") → the read itself.
    if parts.len() == 1 {
        if let Part::Expr(e) = &parts[0] {
            return Some(e.clone());
        }
    }
    let interp: Vec<InterpPart> = parts
        .into_iter()
        .map(|p| match p {
            Part::Lit(s) => InterpPart::Lit(s),
            Part::Expr(e) => InterpPart::Expr(Box::new(e)),
        })
        .collect();
    Some(IrExpr::Interpolate(interp))
}

// ── cut ──────────────────────────────────────────────────────────────

fn try_lower_cut(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        if let IrExpr::Str(s, _) = a { Some(s.as_str()) } else { None }
    }).collect();

    let mut delimiter = ",".to_string();
    let mut fields_str = "";
    let mut suppress = false;
    let mut i = 0;
    while i < args_str.len() {
        let arg = args_str[i];
        if let Some(d) = arg.strip_prefix("-d") {
            if !d.is_empty() {
                delimiter = d.to_string();
            } else if let Some(next) = args_str.get(i + 1) {
                delimiter = next.to_string();
                i += 1;
            }
        } else if let Some(f) = arg.strip_prefix("-f") {
            if !f.is_empty() {
                fields_str = f;
            } else if let Some(next) = args_str.get(i + 1) {
                fields_str = next;
                i += 1;
            }
        } else if arg == "-s" {
            suppress = true;
        }
        i += 1;
    }

    if fields_str.is_empty() {
        return None;
    }

    // Parse field spec: "1", "1,3", "1-3", "1-3,5"
    let fields = parse_field_spec(fields_str);

    let mut node = FieldExtract {
        text: text,
        delimiter,
        fields,
        suppress_no_delim: suppress,
        output_delimiter: None,
    };

    // Check for -o (output delimiter) — last arg that starts with -o
    for arg in &args_str {
        if let Some(d) = arg.strip_prefix("-o") {
            node.output_delimiter = Some(d.to_string());
        }
    }

    Some(IrExpr::Ext(Box::new(node)))
}

fn parse_field_spec(spec: &str) -> Vec<FieldRange> {
    spec.split(',').filter_map(|part| {
        if let Some((start, end)) = part.split_once('-') {
            let s: u32 = start.parse().ok()?;
            let e: u32 = end.parse().ok()?;
            Some(FieldRange::Range { start: s, end: e })
        } else {
            let n: u32 = part.parse().ok()?;
            Some(FieldRange::Single(n))
        }
    }).collect()
}

// ── tr ───────────────────────────────────────────────────────────────

fn try_lower_tr(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        match a {
            IrExpr::Str(s, _) => Some(s.as_str()),
            IrExpr::Interpolate(parts) if parts.len() == 1 => {
                match &parts[0] {
                    InterpPart::Lit(s) => Some(s.as_str()),
                    _ => None,
                }
            }
            _ => None,
        }
    }).collect();

    let mut delete = false;
    let mut squeeze = false;
    let mut from = "";
    let mut to = "";

    for arg in &args_str {
        if arg == &"-d" { delete = true; }
        else if arg == &"-s" { squeeze = true; }
        else if from.is_empty() { from = arg; }
        else if to.is_empty() { to = arg; }
    }

    if from.is_empty() {
        return None;
    }

    // POSIX character classes: tr '[:upper:]' '[:lower:]' is a CASE
    // transform, NOT a literal char map ("[:upper:]" is a class, not chars).
    // Other classes ([:digit:], [:space:], ...) can't be a literal CharTranslate
    // — leave them to the runtime.
    if from.contains("[:") || to.contains("[:") {
        if !delete && !squeeze && from == "[:upper:]" && to == "[:lower:]" {
            return Some(IrExpr::Ext(Box::new(CaseTransform { text, upper: false })));
        }
        if !delete && !squeeze && from == "[:lower:]" && to == "[:upper:]" {
            return Some(IrExpr::Ext(Box::new(CaseTransform { text, upper: true })));
        }
        return None; // other class translations → runtime
    }

    // Special case: tr 'a-z' 'A-Z' (case transform)
    if !delete && !squeeze && from == "a-z" && to == "A-Z" {
        return Some(IrExpr::Ext(Box::new(CaseTransform {
            text: text,
            upper: true,
        })));
    }
    if !delete && !squeeze && from == "A-Z" && to == "a-z" {
        return Some(IrExpr::Ext(Box::new(CaseTransform {
            text: text,
            upper: false,
        })));
    }

    Some(IrExpr::Ext(Box::new(CharTranslate {
        text: text,
        from: from.to_string(),
        to: to.to_string(),
        delete,
        squeeze,
    })))
}

// ── head / tail ──────────────────────────────────────────────────────

fn try_lower_head_tail(text: IrExpr, args: &[IrExpr], from_end: bool) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        if let IrExpr::Str(s, _) = a { Some(s.as_str()) } else { None }
    }).collect();

    let mut count_str = "10"; // default
    let mut bytes = false;

    let mut i = 0;
    while i < args_str.len() {
        if args_str[i] == "-n" || args_str[i] == "-c" {
            if args_str[i] == "-c" { bytes = true; }
            if let Some(c) = args_str.get(i + 1) {
                count_str = c;
                i += 2;
                continue;
            }
        } else if args_str[i].starts_with('-') && args_str[i].len() > 1 {
            // -5 or -c5
            let rest = &args_str[i][1..];
            if rest.starts_with('c') { bytes = true; count_str = &rest[1..]; }
            else { count_str = rest; }
        } else {
            count_str = args_str[i];
        }
        i += 1;
    }

    // `head -c` / `tail -c` take BYTES; the as-built TakeLines renderers
    // slice LINES (split('\n')…join) — a byte count would mis-render, so
    // it falls back to the original command.
    if bytes {
        return None;
    }

    let count = count_str.parse::<i64>().ok().map(|n| IrExpr::Int(n))
        .unwrap_or_else(|| IrExpr::Str(count_str.to_string(), StrStyle::DoubleQuoted));

    Some(IrExpr::Ext(Box::new(TakeLines {
        text: text,
        count: count,
        from_end,
        bytes,
    })))
}

// ── wc ───────────────────────────────────────────────────────────────

fn try_lower_wc(text: IrExpr, trailing_newline: bool, args: &[IrExpr]) -> Option<IrExpr> {
    // Lower `wc` to a PRIMITIVE count node so each renderer implements it
    // once, trivially — no per-mode branching in the renderers:
    //   wc -c / wc -m  → StrLen (text.length)
    //   wc -l          → LineCount (split('\n').length)
    //   wc -w          → WordCount (split(/\s+/).length)
    let flags: Vec<&str> = args.iter().filter_map(|a| {
        match a {
            IrExpr::Str(s, _) => Some(s.as_str()),
            IrExpr::Interpolate(parts) if parts.len() == 1 => {
                if let InterpPart::Lit(s) = &parts[0] { Some(s.as_str()) } else { None }
            }
            _ => None,
        }
    }).filter(|s| s.starts_with('-')).collect();

    let mut lower_c = false;
    let mut lower_l = false;
    let mut lower_w = false;
    for f in &flags {
        for c in f.chars().skip(1) {
            match c {
                'c' | 'm' => lower_c = true,
                'l' => lower_l = true,
                'w' => lower_w = true,
                _ => {}
            }
        }
    }
    // Multiple modes (e.g. `wc -lc`) output multiple counts — too complex
    // for a single primitive; don't lower.
    let set_count = [lower_c, lower_l, lower_w].iter().filter(|b| **b).count();
    if set_count != 1 {
        return None;
    }
    if lower_c {
        // wc -c → StrLen (text.length)
        Some(IrExpr::Ext(Box::new(StrLen { text })))
    } else {
        // wc -l / wc -w → ArrayLen(Split(text, delim)) — a COMPOSITION of
        // primitives. Backends implement Split + ArrayLen once; no bespoke
        // LineCount/WordCount nodes.
        if lower_l {
            // wc -l is a NEWLINE COUNT (each line ends in \n) — NOT
            // split('\n').length (off by one on trailing newline).
            // echo / here-string sources end with a trailing newline, so the
            // input is text + "\n"; the count includes that trailing
            // newline. An `echo -n` source has NO trailing newline —
            // bash: `echo -n x | wc -l` = 0 — so nothing is appended.
            let text = if trailing_newline { append_trailing_newline(text) } else { text };
            Some(IrExpr::Ext(Box::new(RegCount {
                text,
                pattern: "\\n".to_string(),
            })))
        } else {
            // wc -w → ArrayLen(Split(text, /\s+/))
            Some(IrExpr::Ext(Box::new(ArrayLen {
                array: IrExpr::Ext(Box::new(Split { text, delim: "\\s+".to_string(), is_regex: true })),
            })))
        }
    }
}

// ── sed ──────────────────────────────────────────────────────────────

fn try_lower_sed(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        match a {
            IrExpr::Str(s, _) => Some(s.as_str()),
            IrExpr::Interpolate(parts) if parts.len() == 1 => {
                match &parts[0] {
                    InterpPart::Lit(s) => Some(s.as_str()),
                    _ => None,
                }
            }
            _ => None,
        }
    }).collect();

    // Look for 's/pattern/replacement/flags'
    for arg in &args_str {
        if let Some(rest) = arg.strip_prefix("s/") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 2 {
                let pattern = parts[0].to_string();
                let replacement = parts[1].to_string();
                let global = parts.get(2).map(|f| f.contains('g')).unwrap_or(false);

                return Some(IrExpr::Ext(Box::new(RegSub {
                    text: text,
                    pattern,
                    replacement,
                    global,
                    line_mode: true,
                })));
            }
        }
    }
    None
}

// ── xargs (trim) ─────────────────────────────────────────────────────

/// Bare `| xargs` (the default `echo` over stdin words) prints the
/// words single-space-joined with leading/trailing whitespace dropped.
/// That equals a plain Trim ONLY when the text provably has no internal
/// whitespace runs (no doubled spaces, tabs, or newlines) — so this
/// fires only on such literals. `xargs` WITH arguments (-0, -n, an
/// explicit command, …) is a different program entirely and always
/// falls back to the original command.
fn try_lower_xargs(text: IrExpr, args: &[IrExpr], is_literal: bool) -> Option<IrExpr> {
    if !args.is_empty() {
        return None;
    }
    if !is_literal {
        return None;
    }
    if let IrExpr::Str(t, _) = &text {
        let inner = t.trim();
        if inner.contains('\n') || inner.contains('\t') || inner.contains("  ") {
            return None;
        }
    } else {
        return None;
    }
    Some(IrExpr::Ext(Box::new(StringTrim {
        text,
        leading: true,
        trailing: true,
    })))
}

// ── ${#var} → StrLen ────────────────────────────────────────────────

/// Convert a `param` op's var-name arg (a Str of the variable NAME) into a
/// real variable READ (`getVar("name")`), not the literal name string.
/// `${#var}` must read the variable's value, not count the chars of "var".
fn param_var_read(name: &IrExpr) -> Option<IrExpr> {
    match name {
        IrExpr::Str(s, _) => Some(IrExpr::Call { func: "param".to_string(),
            args: vec![IrExpr::Str(String::new(), StrStyle::DoubleQuoted), IrExpr::Str(s.clone(), StrStyle::DoubleQuoted)] }),
        _ => None,
    }
}

fn try_lower_param_len(args: &[IrExpr]) -> Option<IrExpr> {
    // param("length", var_name) → StrLen(read(var))
    if args.len() >= 2 {
        if let IrExpr::Str(op, _) = &args[0] {
            if op == "length" || op == "len" {
                let var = param_var_read(&args[1])?;
                return Some(IrExpr::Ext(Box::new(StrLen { text: var })));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_field_spec() {
        let fields = parse_field_spec("2");
        assert_eq!(fields, vec![FieldRange::Single(2)]);
    }

    #[test]
    fn parse_multi_field_spec() {
        let fields = parse_field_spec("1,3");
        assert_eq!(fields, vec![FieldRange::Single(1), FieldRange::Single(3)]);
    }

    #[test]
    fn parse_range_field_spec() {
        let fields = parse_field_spec("1-3");
        assert_eq!(fields, vec![FieldRange::Range { start: 1, end: 3 }]);
    }

    #[test]
    fn parse_mixed_field_spec() {
        let fields = parse_field_spec("1-3,5");
        assert_eq!(fields, vec![
            FieldRange::Range { start: 1, end: 3 },
            FieldRange::Single(5),
        ]);
    }

    // ── param op reductions ──────────────────────────────────────────
    fn st(s: &str) -> IrExpr { IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted) }

    #[test]
    fn param_case_upper() {
        let args = vec![st("^^"), st("var")];
        let r = try_lower_param_op(&args).expect("^^ lowers");
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        let n = n.as_any().downcast_ref::<CaseTransform>().unwrap();
        assert!(n.upper, "^^ should be upper");
    }

    #[test]
    fn param_case_lower() {
        let args = vec![st(",,"), st("var")];
        let r = try_lower_param_op(&args).expect(",, lowers");
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        let n = n.as_any().downcast_ref::<CaseTransform>().unwrap();
        assert!(!n.upper, ",, should be lower");
    }

    // ── wc reductions ────────────────────────────────────────────────

    fn arg(text: IrExpr) -> IrExpr {
        let t = IrExpr::Str("hello".to_string(), StrStyle::DoubleQuoted);
        let _ = text;
        t
    }

    #[test]
    fn wc_c_is_strlen() {
        let r = try_lower_wc(st("hello"), true, &[st("-c")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        assert!(n.as_any().downcast_ref::<StrLen>().is_some(), "wc -c → StrLen");
    }

    #[test]
    fn wc_l_is_regcount() {
        let r = try_lower_wc(st("hello"), true, &[st("-l")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        assert!(n.as_any().downcast_ref::<RegCount>().is_some(), "wc -l → RegCount");
    }

    #[test]
    fn wc_w_is_split_plus_arraylen() {
        let r = try_lower_wc(st("hello"), true, &[st("-w")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        assert!(n.as_any().downcast_ref::<ArrayLen>().is_some(), "wc -w → ArrayLen(Split)");
    }

    // ── grep reduction ──────────────────────────────────────────────

    #[test]
    fn grep_q_is_stringcontains() {
        let r = try_lower_grep(st("hello world"), &[st("-q"), st("wor")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        assert!(n.as_any().downcast_ref::<StringContains>().is_some());
    }

    #[test]
    fn grep_plain_not_reduced() {
        // grep without -q isn't a substring test → no reduction
        assert!(try_lower_grep(st("hello world"), &[st("wor")]).is_none());
    }

    #[test]
    fn getvar_hash_is_strlen() {
        // ${#s} raw form: getVar("#s") → StrLen
        let r = try_lower_getvar_len(&[st("#s")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        assert!(n.as_any().downcast_ref::<StrLen>().is_some(), "#s → StrLen");
    }

    #[test]
    fn getvar_plain_not_reduced() {
        // a normal var read getVar("s") is NOT a length → no reduction
        assert!(try_lower_getvar_len(&[st("s")]).is_none());
    }

    #[test]
    fn yes_head_is_repeat() {
        // yes "X" | head -n 3 → RepeatStr("X\n", 3)
        let yes = [IrStmt::Expr(IrExpr::Call { func: "exec".to_string(), args: vec![
            st("yes"), IrExpr::Array(vec![st("Hi")]),
        ]})];
        let r = try_lower_yes_head(&yes, &[st("-n"), st("3")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        let rep = n.as_any().downcast_ref::<RepeatStr>().unwrap();
        assert_eq!(rep.count, IrExpr::Int(3));
        assert!(matches!(&rep.text, IrExpr::Str(s, _) if s == "Hi\n"));
    }

    // ── source helpers ───────────────────────────────────────────────

    fn lit_src(s_: &str) -> TextSource {
        TextSource { text: st(s_), trailing_newline: true, is_literal: true }
    }
    fn var_src() -> TextSource {
        TextSource {
            text: param_var_read(&st("v")).unwrap(),
            trailing_newline: true,
            is_literal: false,
        }
    }
    fn echo_stage(args: Vec<IrExpr>) -> Vec<IrStmt> {
        vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![st("echo"), IrExpr::Array(args)],
        })]
    }

    // ── tr compositions ──────────────────────────────────────────────

    #[test]
    fn tr_range_upper_is_case_transform() {
        let r = try_lower_tr(st("hi"), &[st("a-z"), st("A-Z")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<CaseTransform>().unwrap().upper);
    }

    #[test]
    fn tr_posix_class_case_transform() {
        let r = try_lower_tr(st("hi"), &[st("[:lower:]"), st("[:upper:]")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<CaseTransform>().unwrap().upper);
    }

    #[test]
    fn tr_other_posix_class_not_reduced() {
        // [:digit:] is a class, not literal chars — falls back to the runtime.
        assert!(try_lower_tr(st("hi"), &[st("[:digit:]"), st("x")]).is_none());
    }

    #[test]
    fn tr_chars_is_char_translate() {
        let r = try_lower_tr(st("abc"), &[st("abc"), st("xyz")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let ct = n.as_any().downcast_ref::<CharTranslate>().unwrap();
        assert_eq!(ct.from, "abc");
        assert_eq!(ct.to, "xyz");
        assert!(!ct.delete && !ct.squeeze);
    }

    #[test]
    fn tr_delete_and_squeeze_flags() {
        let r = try_lower_tr(st("abc"), &[st("-d"), st("b")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<CharTranslate>().unwrap().delete);
        let r = try_lower_tr(st("a  b"), &[st("-s"), st(" ")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<CharTranslate>().unwrap().squeeze);
    }

    // ── sed composition ──────────────────────────────────────────────

    #[test]
    fn sed_subst_is_regsub() {
        let r = try_lower_sed(st("aaa"), &[st("s/a/b/g")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let rs = n.as_any().downcast_ref::<RegSub>().unwrap();
        assert_eq!(rs.pattern, "a");
        assert_eq!(rs.replacement, "b");
        assert!(rs.global);
        let r = try_lower_sed(st("aaa"), &[st("s/a/b/")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(!n.as_any().downcast_ref::<RegSub>().unwrap().global);
    }

    // ── head/tail compositions ───────────────────────────────────────

    #[test]
    fn head_n_is_take_lines() {
        let r = try_lower_head_tail(st("a\nb\nc"), &[st("-n"), st("2")], false).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let tl = n.as_any().downcast_ref::<TakeLines>().unwrap();
        assert_eq!(tl.count, IrExpr::Int(2));
        assert!(!tl.from_end && !tl.bytes);
    }

    #[test]
    fn tail_n_is_take_lines_from_end() {
        let r = try_lower_head_tail(st("a\nb"), &[st("-n"), st("1")], true).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<TakeLines>().unwrap().from_end);
    }

    #[test]
    fn head_bytes_not_reduced() {
        // head -c takes BYTES; the as-built TakeLines renderers slice
        // LINES — a byte count must fall back (070_gnuisms: printf|head -c).
        assert!(try_lower_head_tail(st("abcdef"), &[st("-c"), st("3")], false).is_none());
        assert!(try_lower_head_tail(st("abcdef"), &[st("-c3")], true).is_none());
    }

    // ── wc trailing-newline semantics ────────────────────────────────

    #[test]
    fn wc_l_no_trailing_newline_counts_raw() {
        // `echo -n x | wc -l` = 0 in bash: no newline is appended.
        let r = try_lower_wc(st("x"), false, &[st("-l")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let rc = n.as_any().downcast_ref::<RegCount>().unwrap();
        assert!(matches!(&rc.text, IrExpr::Str(s, _) if s == "x"), "no \n appended");
    }

    #[test]
    fn wc_l_trailing_newline_appends() {
        let r = try_lower_wc(st("x"), true, &[st("-l")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let rc = n.as_any().downcast_ref::<RegCount>().unwrap();
        assert!(matches!(&rc.text, IrExpr::Str(s, _) if s == "x\n"));
    }

    #[test]
    fn wc_multi_mode_not_reduced() {
        assert!(try_lower_wc(st("x"), true, &[st("-lc")]).is_none());
    }

    // ── xargs policy ─────────────────────────────────────────────────

    #[test]
    fn xargs_bare_clean_literal_is_trim() {
        let r = try_lower_xargs(st("  x y  "), &[], true).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<StringTrim>().is_some());
    }

    #[test]
    fn xargs_with_args_not_reduced() {
        // `xargs -0 … echo chown …` runs a command — not a trim
        // (chown-through-xargs.sh mis-rendered before this refusal).
        assert!(try_lower_xargs(st("p"), &[st("-0"), st("echo")], true).is_none());
    }

    #[test]
    fn xargs_whitespace_run_not_reduced() {
        // xargs single-space-joins words: `echo "a  b" | xargs` prints
        // "a b", which a plain Trim cannot produce.
        assert!(try_lower_xargs(st("a  b"), &[], true).is_none());
        assert!(try_lower_xargs(st("a\tb"), &[], true).is_none());
    }

    #[test]
    fn xargs_variable_source_not_reduced() {
        let TextSource { text, .. } = var_src();
        assert!(try_lower_xargs(text, &[], false).is_none());
    }

    // ── source-policy matrix (lower_text_cmd) ────────────────────────

    #[test]
    fn cut_single_line_literal_reduces() {
        let r = lower_text_cmd(lit_src("a,b,c"), "cut", &[st("-d,"), st("-f2")], true).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<FieldExtract>().is_some());
    }

    #[test]
    fn cut_variable_source_not_reduced() {
        // FieldExtract is SINGLE-LINE as-built; a variable may hold
        // multiple lines → fall back to the original command.
        assert!(lower_text_cmd(var_src(), "cut", &[st("-d,"), st("-f2")], true).is_none());
    }

    #[test]
    fn cut_multiline_literal_not_reduced() {
        assert!(lower_text_cmd(lit_src("a,b\nc,d"), "cut", &[st("-d,"), st("-f1")], true).is_none());
    }

    #[test]
    fn sed_variable_source_not_reduced() {
        assert!(lower_text_cmd(var_src(), "sed", &[st("s/a/b/")], true).is_none());
    }

    #[test]
    fn wc_variable_source_reduces() {
        // wc consumes the whole text — variable sources are safe.
        let r = lower_text_cmd(var_src(), "wc", &[st("-l")], true).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<RegCount>().is_some());
    }

    #[test]
    fn tr_variable_source_reduces() {
        let r = lower_text_cmd(var_src(), "tr", &[st("a-z"), st("A-Z")], true).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<CaseTransform>().is_some());
    }

    #[test]
    fn grep_q_statement_level_not_reduced() {
        // grep -q prints NOTHING at statement level (status only) — the
        // Output wrap would print the contains value (parse-herestring.sh
        // mis-rendered before this refusal).
        assert!(lower_text_cmd(lit_src("hello"), "grep", &[st("-q"), st("ell")], true).is_none());
    }

    #[test]
    fn grep_q_expression_level_reduces() {
        let r = lower_text_cmd(lit_src("hello"), "grep", &[st("-q"), st("ell")], false).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<StringContains>().is_some());
    }

    #[test]
    fn unknown_command_falls_back() {
        // The fallback rule: anything the core cannot reduce returns None
        // (the caller keeps the original command).
        assert!(lower_text_cmd(lit_src("x"), "sort", &[], true).is_none());
        assert!(lower_text_cmd(lit_src("x"), "awk", &[st("{print}")], true).is_none());
    }

    // ── echo/printf source extraction policy ─────────────────────────

    #[test]
    fn echo_n_source_has_no_trailing_newline() {
        let src = extract_text_from_stage(&echo_stage(vec![st("-n"), st("x")])).unwrap();
        assert!(!src.trailing_newline);
        assert!(matches!(&src.text, IrExpr::Str(s, _) if s == "x"), "-n is a flag, not text");
    }

    #[test]
    fn echo_e_source_refused() {
        // -e interprets escapes; treating the args as literal text would
        // not be byte-exact.
        assert!(extract_text_from_stage(&echo_stage(vec![st("-e"), st("a\\nb")])).is_none());
    }

    #[test]
    fn printf_source_refused() {
        // printf adds no trailing newline and interprets % directives:
        // `printf x | wc -l` is 0 in bash — the old lowering said 1.
        let stage = vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![st("printf"), IrExpr::Array(vec![st("x")])],
        })];
        assert!(extract_text_from_stage(&stage).is_none());
    }

    #[test]
    fn echo_variable_arg_is_text_source() {
        let src = extract_text_from_stage(&echo_stage(vec![param_var_read(&st("s")).unwrap()])).unwrap();
        assert!(src.trailing_newline);
        assert!(!src.is_literal);
        assert!(matches!(&src.text, IrExpr::Call { func, .. } if func == "param"));
    }

    #[test]
    fn echo_arbitrary_expr_refused() {
        // A nested capture as an echo arg could carry side effects —
        // refuse rather than guess.
        let cap = IrExpr::Capture { expr: Box::new(st("date")), native: false };
        assert!(extract_text_from_stage(&echo_stage(vec![cap])).is_none());
    }

    // ── param-op boundaries ──────────────────────────────────────────

    #[test]
    fn param_slice_not_reduced() {
        // ${s:N:M} (scalar) and ${arr[@]:N:M} (array) share the same
        // param("slice", …) shape — reducing to a string SubStr would be
        // wrong for arrays, so slice stays with the runtime.
        assert!(try_lower_param_op(&[st("slice"), st("v"), st("1"), st("2")]).is_none());
    }

    #[test]
    fn param_path_ops_are_pathname() {
        let r = try_lower_param_path(&[st("basename"), st("p")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert_eq!(n.as_any().downcast_ref::<PathName>().unwrap().which, "basename");
        let r = try_lower_param_path(&[st("dirname"), st("p")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert_eq!(n.as_any().downcast_ref::<PathName>().unwrap().which, "dirname");
    }

    // ── statement/capture scope boundary ─────────────────────────────

    fn stmt_pipeline(stage1: Vec<IrStmt>, stage2: Vec<IrStmt>) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "pipeline".to_string(),
            args: vec![IrExpr::Array(vec![
                IrExpr::Arrow(stage1),
                IrExpr::Arrow(stage2),
            ])],
        })
    }

    fn wc_stage() -> Vec<IrStmt> {
        vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![st("wc"), IrExpr::Array(vec![st("-l")])],
        })]
    }

    #[test]
    fn statement_pipeline_reduces_to_output() {
        let mut stmt = stmt_pipeline(echo_stage(vec![st("x")]), wc_stage());
        lower_stmt(&mut stmt, true);
        let IrStmt::Output { value: IrExpr::Ext(n), .. } = &stmt else {
            panic!("statement-level pipeline should reduce to Output, got {stmt:?}");
        };
        assert!(n.as_any().downcast_ref::<RegCount>().is_some());
    }

    #[test]
    fn capture_body_pipeline_not_reduced() {
        // Inside $(...) the body must remain the original COMMAND: the
        // capture collects its stdout; reducing to a bare value breaks it.
        // Arrow bodies lower with emit=false — the statement-level
        // reduction must not fire.
        let inner = stmt_pipeline(echo_stage(vec![st("x")]), wc_stage());
        let mut cap = IrExpr::Capture {
            expr: Box::new(IrExpr::Arrow(vec![inner.clone()])),
            native: false,
        };
        lower_expr(&mut cap);
        let IrExpr::Capture { expr, .. } = &cap else { panic!() };
        let IrExpr::Arrow(body) = expr.as_ref() else { panic!() };
        assert_eq!(&body[0], &inner, "capture body must keep the original pipeline");
    }
}

/// Lower a single command `cmd_name(args)` against input text `text`.
/// Used by the here-string/here-doc Redirect path.
fn try_lower_command(text: IrExpr, cmd_name: &str, cmd_args: &[IrExpr]) -> Option<IrExpr> {
    // Handle basename/dirname as top-level commands too
    match cmd_name {
        "basename" => Some(IrExpr::Ext(Box::new(PathName {
            text: text.clone(),
            which: "basename".to_string(),
        }))),
        "dirname" => Some(IrExpr::Ext(Box::new(PathName {
            text: text.clone(),
            which: "dirname".to_string(),
        }))),
        // A here-string supplies "text\n" (bash appends the newline), and
        // this path is statement position (the caller Output-wraps).
        _ => {
            let is_literal = matches!(text, IrExpr::Str(..));
            lower_text_cmd(
                TextSource { text, trailing_newline: true, is_literal },
                cmd_name,
                cmd_args,
                true,
            )
        }
    }
}

/// `${p##*/}` → PathName(basename), `${p%/*}` → PathName(dirname)
///
/// The `param` call carries the operator name and the variable.
fn try_lower_param_path(args: &[IrExpr]) -> Option<IrExpr> {
    // Shape: param(op, name) — the shIR already lowers ${p##*/} → param("basename", p)
    // and ${p%/*} → param("dirname", p).
    if args.len() >= 2 {
        if let IrExpr::Str(op, _) = &args[0] {
            let var = param_var_read(&args[1])?;
            match op.as_str() {
                "basename" => {
                    return Some(IrExpr::Ext(Box::new(PathName {
                        text: var,
                        which: "basename".to_string(),
                    })));
                }
                "dirname" => {
                    return Some(IrExpr::Ext(Box::new(PathName {
                        text: var,
                        which: "dirname".to_string(),
                    })));
                }
                _ => {}
            }
        }
    }
    None
}

fn arg_to_expr(arg: &IrExpr) -> Option<IrExpr> {
    match arg {
        IrExpr::Str(s, _) => Some(IrExpr::Str(s.clone(), StrStyle::DoubleQuoted)),
        IrExpr::Interpolate(parts) if parts.len() == 1 => {
            match &parts[0] {
                InterpPart::Lit(s) => Some(IrExpr::Str(s.clone(), StrStyle::DoubleQuoted)),
                _ => Some(arg.clone()),
            }
        }
        IrExpr::Var(..) | IrExpr::Capture { .. } | IrExpr::Call { .. } => Some(arg.clone()),
        _ => None,
    }
}

/// Reduce `param` expansion ops to primitives:
///   ${var^^} → Case(var, upper), ${var,,} → Case(var, lower)
///   ${var^} / ${var,} → CaseFirst (first char only)
///   ${var:2:3} → SubStr(var, 2, 3)
fn try_lower_param_op(args: &[IrExpr]) -> Option<IrExpr> {
    if args.len() < 2 { return None; }
    let op = match &args[0] { IrExpr::Str(s, _) => s.as_str(), _ => return None };
    let var = param_var_read(&args[1])?;
    match op {
        ",," => Some(IrExpr::Ext(Box::new(CaseTransform { text: var, upper: false }))),
        "^^" => Some(IrExpr::Ext(Box::new(CaseTransform { text: var, upper: true }))),
        // "slice" is NOT reduced: ${s:N:M} (scalar) and ${arr[@]:N:M} (array)
        // produce IDENTICAL param("slice", name, off, len) — they can't be
        // told apart from the args, and reducing an array slice to a string
        // SubStrExtract is WRONG. The runtime handles both correctly, so
        // leave param("slice") untouched.
        _ => None,
    }
}

/// `echo X | grep -q P` → StringContains(X, P) — the substring test.
fn try_lower_grep(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    // Only the grep -q P (quiet substring test) shape.
    let strs: Vec<&str> = args.iter().filter_map(|a| match a {
        IrExpr::Str(s, _) => Some(s.as_str()),
        IrExpr::Interpolate(p) if p.len() == 1 => {
            if let InterpPart::Lit(s) = &p[0] { Some(s.as_str()) } else { None }
        }
        _ => None,
    }).collect();
    // args like ["-q", "wor"] → quiet + literal pattern
    if strs.len() == 2 && strs[0] == "-q" {
        let pattern = IrExpr::Str(strs[1].to_string(), StrStyle::DoubleQuoted);
        return Some(IrExpr::Ext(Box::new(StringContains {
            text: text.clone(),
            pattern,
        })));
    }
    None
}

/// `${#name}` raw form: getVar("##name") → StrLen(read(name)).
/// The "##" prefix marks a length read in the shIR.
fn try_lower_getvar_len(args: &[IrExpr]) -> Option<IrExpr> {
    match args {
        [IrExpr::Str(name, _)] => {
            if name.starts_with('#') && name.len() > 1 {
                let var_name = &name[1..];
                let var = param_var_read(&IrExpr::Str(var_name.to_string(), StrStyle::DoubleQuoted))?;
                return Some(IrExpr::Ext(Box::new(StrLen { text: var })));
            }
            None
        }
        _ => None,
    }
}

/// Read a variable by name: param("", name) — the shIR's plain-read form.
fn param_var(name: &IrExpr) -> Option<IrExpr> {
    match name {
        IrExpr::Str(s, _) => Some(IrExpr::Call { func: "param".to_string(),
            args: vec![IrExpr::Str(String::new(), StrStyle::DoubleQuoted), IrExpr::Str(s.clone(), StrStyle::DoubleQuoted)] }),
        _ => None,
    }
}

/// Append a trailing "\n" to a literal text (echo / here-string sources
/// produce a trailing newline that newline-count reductions must see).
fn append_trailing_newline(text: IrExpr) -> IrExpr {
    match text {
        IrExpr::Str(s, style) => IrExpr::Str(format!("{}\n", s), style),
        _ => IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(text)), InterpPart::Lit("\n".to_string())]),
    }
}
