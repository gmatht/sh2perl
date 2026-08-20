//! text_ops: Recognize common shell commands and lower to semantic IR nodes.
//!
//! `echo X | cut -d',' -f2`  → FieldExtract
//! `echo X | tr 'a-z' 'A-Z'` → CaseTransform / CharTranslate
//! `echo X | sed 's/p/r/'`   → RegSub
//! `echo X | head -n 5`      → TakeLines
//! `echo X | tail -n 5`      → TakeLines
//! `echo X | wc -l`          → WordCount
//! `${#var}`                 → StrLen
//! `expr substr "$x" 1 5`   → SubStrExtract
//! `echo X | xargs`          → StringTrim
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
                        if let Some((replacement, already_nl)) = try_lower_pipeline(stages) {
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
                            if let Some(replacement) = try_lower_command(text_ir, name, cmd_args) {
                                *stmt = IrStmt::Expr(replacement);
                                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                return;
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
                    if let Some((replacement, _)) = try_lower_pipeline(stages) {
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
fn try_lower_pipeline(stages: &[IrExpr]) -> Option<(IrExpr, bool)> {
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

    // stage1 produces text (echo, capture, etc.)
    let text_expr = match extract_text_from_stage(stage1_body) {
        Some(e) => {
            e
        }
        None => {
            return None;
        }
    };

    lower_text_cmd(text_expr, cmd_name, cmd_args).map(|e| (e, false))
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
fn lower_text_cmd(text: IrExpr, cmd_name: &str, cmd_args: &[IrExpr]) -> Option<IrExpr> {
    match cmd_name {
        "cut" => try_lower_cut(text, cmd_args),
        "tr" => try_lower_tr(text, cmd_args),
        "head" => try_lower_head_tail(text, cmd_args, false),
        "tail" => try_lower_head_tail(text, cmd_args, true),
        "wc" => try_lower_wc(text, cmd_args),
        "sed" => try_lower_sed(text, cmd_args),
        "grep" => try_lower_grep(text, cmd_args),
        "xargs" => try_lower_xargs(text),
        _ => None,
    }
}

/// Extract the text expression from a pipeline stage body (echo, capture, etc.)
fn extract_text_from_stage(stmts: &[IrStmt]) -> Option<IrExpr> {
    match stmts {
        // echo ARGS → join args as string
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec" || func == "builtin" =>
        {
            if let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args.as_slice() {
                if name == "echo" || name == "printf" {
                    // Simple case: echo with string/literal args → concatenate
                    let all_strs: Option<Vec<&str>> = echo_args.iter().map(|a| {
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
                    if let Some(strs) = all_strs {
                        // The echo ARGS joined (echo's trailing newline is
                        // added by the statement Output wrapper, and by the
                        // wc -l newline-count case below).
                        let joined = strs.join(" ");
                        return Some(IrExpr::Str(joined, StrStyle::DoubleQuoted));
                    }
                }
            }
            None
        }
        // Simple expression
        [IrStmt::Expr(e)] => Some(e.clone()),
        _ => None,
    }
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

fn try_lower_wc(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
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
            // input is text + "\n"; the count includes that trailing newline.
            let text = append_trailing_newline(text);
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

fn try_lower_xargs(text: IrExpr) -> Option<IrExpr> {
    Some(IrExpr::Ext(Box::new(StringTrim {
        text: text,
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

    #[test]
    fn param_slice() {
        let args = vec![st("slice"), st("var"), st("2"), st("3")];
        let r = try_lower_param_op(&args).expect("slice lowers");
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        let n = n.as_any().downcast_ref::<SubStrExtract>().unwrap();
        assert_eq!(n.offset, IrExpr::Int(2));
        assert_eq!(**n.length.as_ref().unwrap(), IrExpr::Int(3));
    }

    // ── wc reductions ────────────────────────────────────────────────

    fn arg(text: IrExpr) -> IrExpr {
        let t = IrExpr::Str("hello".to_string(), StrStyle::DoubleQuoted);
        let _ = text;
        t
    }

    #[test]
    fn wc_c_is_strlen() {
        let r = try_lower_wc(st("hello"), &[st("-c")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        assert!(n.as_any().downcast_ref::<StrLen>().is_some(), "wc -c → StrLen");
    }

    #[test]
    fn wc_l_is_regcount() {
        let r = try_lower_wc(st("hello"), &[st("-l")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        assert!(n.as_any().downcast_ref::<RegCount>().is_some(), "wc -l → RegCount");
    }

    #[test]
    fn wc_w_is_split_plus_arraylen() {
        let r = try_lower_wc(st("hello"), &[st("-w")]).unwrap();
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
        _ => lower_text_cmd(text, cmd_name, cmd_args),
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
        "slice" if args.len() >= 4 => {
            let off = match &args[2] { IrExpr::Str(s, _) => s.parse::<i64>().ok()?, _ => return None };
            let len = match &args[3] { IrExpr::Str(s, _) => s.parse::<i64>().ok()?, _ => return None };
            Some(IrExpr::Ext(Box::new(SubStrExtract {
                text: var,
                offset: IrExpr::Int(off),
                length: Some(Box::new(IrExpr::Int(len))),
            })))
        }
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
