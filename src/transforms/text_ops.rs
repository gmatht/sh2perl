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
    // text-ops is often exercised ALONE (DEBASHC_TRANSFORMS=text-ops) — the
    // worker's A/B gate. But arithmetic statements (`let "v = e"`, `((v=e))`)
    // are lowered by arith-forms, which the opt-in gate then EXCLUDES: a
    // script byte-exact at baseline would regress purely because another
    // transform went missing. Compose arith-forms FIRST (the same relative
    // order the registry runs it when both are listed) so text-ops-only
    // keeps baseline-exact arithmetic semantics.
    crate::transforms::arith_forms::transform(stmts);
    // Census of array variables — scalar `${v:N:M}` slices reduce to
    // SubStrExtract; array slices must NOT (they are index subsets).
    let mut arrays: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_array_names(stmts, &mut arrays);
    let before = LIFT_COUNT.load(Ordering::Relaxed);
    for stmt in stmts.iter_mut() {
        lower_stmt(stmt, true, &arrays);
    }
    let after = LIFT_COUNT.load(Ordering::Relaxed);
    if after > before {
    }
    after > before
}

fn lower_stmt(stmt: &mut IrStmt, emit: bool, arrays: &std::collections::HashSet<String>) {
    match stmt {
        // ShIR pipeline: IrExpr::Call { func: "pipeline", args: [Array(stages)] }
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "pipeline" => {
            if let [IrExpr::Array(stages)] = args.as_slice() {
                if stages.len() == 2 {
                    // `cat F | <reducible>` — cat contributes a FILE source;
                    // rewrite stage1 body to the file-arg command shape so
                    // the file-source reductions below apply uniformly.
                    if let [IrExpr::Arrow(b1), IrExpr::Arrow(b2)] = stages.as_slice() {
                        if let Some(repl) = try_lower_cat_pipe(b1, b2) {
                            *stmt = repl;
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                    // `find P [-maxdepth N] [-type f|d] | wc -l` →
                    // STREAMING directory-walk count (no entry list).
                    if emit {
                        if let [IrExpr::Arrow(b1), IrExpr::Arrow(b2)] = stages.as_slice() {
                            if let Some(repl) = try_lower_find_wc(b1, b2) {
                                *stmt = repl;
                                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                        }
                    }
                    // `grep P F | wc -l` → STREAMING filtered line count.
                    // Statement-level Block replacement (counter loop).
                    if emit {
                        if let [IrExpr::Arrow(b1), IrExpr::Arrow(b2)] = stages.as_slice() {
                            if let Some(repl) = try_lower_grep_cut(b1, b2) {
                                *stmt = repl;
                                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                            if let Some(repl) = try_lower_grep_text_cmd(b1, b2) {
                                *stmt = repl;
                                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                            if let Some(repl) = try_lower_grep_count(b1, b2) {
                                *stmt = repl;
                                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                            if let Some(repl) = try_lower_grep_wc(b1, b2) {
                                *stmt = repl;
                                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                return;
                            }
                        }
                    }
                    if emit {
                        if let Some((replacement, already_nl)) = try_lower_pipeline(stages) {
                            // A statement-level pipeline PRINTS its result.
                            *stmt = with_status_zero(IrStmt::Output { value: replacement, newline: !already_nl, target: None });
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
                                    *stmt = with_status_zero(IrStmt::Output { value: replacement, newline: true, target: None });
                                    LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            // `wc -l < F` → STREAMING count: O(1) memory line loop, never
            // a whole-file read (docs shir-primitives.md §ForEachLine).
            if emit {
                // `tr SET1 SET2 < F` → ForEachLine(Output(tr(l)))
                let tr_inner = match inner.as_slice() {
                    [IrStmt::Expr(IrExpr::Call { func, args })]
                        if (func == "exec" || func == "builtin")
                            && matches!(args.as_slice(),
                                [IrExpr::Str(n, _), IrExpr::Array(_)] if n == "tr") =>
                    {
                        match args.as_slice() {
                            [IrExpr::Str(_, _), IrExpr::Array(a)] => Some(a.as_slice()),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                let red_path = redirects.iter().find_map(|r| {
                    if r.fd == Some(0) && r.mode == "r" { Some(r.target.clone()) } else { None }
                });
                if let (Some(targs), Some(path)) = (tr_inner, red_path) {
                    if let Some(val) = try_lower_tr(loop_var_read("__l"), targs) {
                        *stmt = with_status_zero(IrStmt::Ext(Box::new(ForEachLine {
                            source: path,
                            var: "__l".to_string(),
                            limit: None,
                            body: vec![IrStmt::Output { value: val, newline: true, target: None }],
                        })));
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
                let is_wc_l = matches!(inner.as_slice(),
                    [IrStmt::Expr(IrExpr::Call { func, args })]
                        if (func == "exec" || func == "builtin")
                            && matches!(args.as_slice(),
                                [IrExpr::Str(n, _), IrExpr::Array(a)]
                                    if n == "wc" && a.len() == 1
                                        && matches!(&a[0], IrExpr::Str(f, _) if f == "-l")));
                let path = redirects.iter().find_map(|r| {
                    if r.fd == Some(0) && r.mode == "r" { Some(r.target.clone()) } else { None }
                });
                if is_wc_l {
                    if let Some(path) = path {
                        *stmt = streaming_line_count(path, None);
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
            // Recurse into the inner body
            for s in inner.iter_mut() {
                lower_stmt(s, emit, arrays);
            }
        }
        // Plain builtin command: basename X / dirname X (no pipeline)
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" || func == "builtin" => {
            // Recurse into args first (to reach nested $(...) / param calls)
            for a in args.iter_mut() { lower_expr(a, arrays); }
            if let [IrExpr::Str(cmd, _), IrExpr::Array(cmd_args)] = args.as_slice() {
                // `cut -dD -fN F` (single FILE source) → STREAMING per-line
                // FieldExtract — never slurped.
                if emit && cmd == "cut" {
                    // Pairwise parse: -d/-f/-o consume the NEXT arg when
                    // detached (`-d :`). Everything else non-flag is a file.
                    let mut flags: Vec<IrExpr> = Vec::new();
                    let mut files: Vec<IrExpr> = Vec::new();
                    let mut i = 0;
                    while i < cmd_args.len() {
                        match &cmd_args[i] {
                            IrExpr::Str(s, _) if matches!(s.as_str(), "-d" | "-f" | "-o" | "--output-delimiter") => {
                                flags.push(cmd_args[i].clone());
                                i += 1;
                                if i < cmd_args.len() { flags.push(cmd_args[i].clone()); }
                            }
                            IrExpr::Str(s, _) if s.starts_with('-') => flags.push(cmd_args[i].clone()),
                            other => files.push(other.clone()),
                        }
                        i += 1;
                    }
                    if files.len() == 1 {
                        if let Some(field_val) =
                            try_lower_cut(loop_var_read("__l"), &flags)
                        {
                            *stmt = with_status_zero(IrStmt::Ext(Box::new(ForEachLine {
                                source: files.remove(0),
                                var: "__l".to_string(),
                                limit: None,
                                body: vec![IrStmt::Output {
                                    value: field_val,
                                    newline: true,
                                    target: None,
                                }],
                            })));
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                }
                // `sed s/// F` (single FILE source) → STREAMING per-line RegSub.
                if emit && cmd == "sed" {
                    let mut flags: Vec<IrExpr> = Vec::new();
                    let mut files: Vec<IrExpr> = Vec::new();
                    let mut script: Option<IrExpr> = None;
                    let mut i = 0;
                    while i < cmd_args.len() {
                        match &cmd_args[i] {
                            IrExpr::Str(s, _) if matches!(s.as_str(), "-i" | "-n" | "-E") => {
                                flags.push(cmd_args[i].clone()); // unsupported modifiers → won't reduce below
                            }
                            IrExpr::Str(s, _) if s.starts_with('-') => { flags.push(cmd_args[i].clone()); }
                            other => {
                                if script.is_none() { script = Some(other.clone()); }
                                else { files.push(other.clone()); }
                            }
                        }
                        i += 1;
                    }
                    if flags.is_empty() && files.len() == 1 {
                        if let Some(val) = try_lower_sed(loop_var_read("__l"), &flags.iter().chain(script.iter()).cloned().collect::<Vec<_>>()) {
                            *stmt = with_status_zero(IrStmt::Ext(Box::new(ForEachLine {
                                source: files.remove(0),
                                var: "__l".to_string(),
                                limit: None,
                                body: vec![IrStmt::Output { value: val, newline: true, target: None }],
                            })));
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                }
                // `tr X Y < F` → STREAMING per-line CharTranslate/Case.
                // (handled in the Redirect arm below via try_lower_tr_fd)
                // `grep -c P F` (single FILE source) → STREAMING count.
                if emit && cmd == "grep" && cmd_args.len() == 3 {
                    let is_c = matches!(&cmd_args[0], IrExpr::Str(s, _) if s.as_str() == "-c");
                    if is_c {
                        let mut strs: Vec<&str> = Vec::new();
                        let mut ok = true;
                        for x in &cmd_args[1..] {
                            match x {
                                IrExpr::Str(s, _) => strs.push(s.as_str()),
                                IrExpr::Interpolate(p) if p.len() == 1 => {
                                    match &p[0] {
                                        InterpPart::Lit(s) => strs.push(s.as_str()),
                                        _ => { ok = false; }
                                    }
                                }
                                _ => { ok = false; }
                            }
                        }
                        if ok && strs.len() == 2 && !strs[1].starts_with('-') {
                            let pat = IrExpr::Str(strs[0].to_string(), StrStyle::DoubleQuoted);
                            let path = IrExpr::Str(strs[1].to_string(), StrStyle::DoubleQuoted);
                            *stmt = streaming_line_count(path, Some(pat));
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                }
                // `head -n K F` (single FILE source) → STREAMING head with
                // early exit — O(K) memory, reader closed after K lines.
                // (tail CANNOT stream — falls back.)
                if emit && cmd == "head" {
                    let mut files: Vec<IrExpr> = Vec::new();
                    let mut k: Option<i64> = None;
                    let mut bad = false;
                    let mut i = 0;
                    while i < cmd_args.len() {
                        match &cmd_args[i] {
                            IrExpr::Str(s, _) if s == "-n" => {
                                match cmd_args.get(i + 1) {
                                    Some(IrExpr::Str(v, _)) => { k = v.parse().ok(); i += 1; }
                                    _ => { bad = true; }
                                }
                            }
                            IrExpr::Str(s, _) if s == "-c" => { bad = true; } // byte-head: skip v1
                            IrExpr::Str(s, _) if s.len() > 1 && s.starts_with("-n") => {
                                k = s[2..].parse().ok();
                            }
                            IrExpr::Str(s, _) if s.starts_with('-') && s != "--" => {
                                if let Ok(n) = s[1..].parse::<i64>() { k = Some(n); }
                                else { bad = true; }
                            }
                            other => files.push(other.clone()),
                        }
                        i += 1;
                    }
                    if !bad && k.filter(|n| *n > 0).is_some() && files.len() == 1 {
                        *stmt = with_status_zero(IrStmt::Ext(Box::new(ForEachLine {
                            source: files.remove(0),
                            var: "__l".to_string(),
                            limit: Some(Box::new(IrExpr::Int(k.unwrap()))),
                            body: vec![IrStmt::Output {
                                value: loop_var_read("__l"),
                                newline: true,
                                target: None,
                            }],
                        })));
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
                if emit && cmd == "find" {
                    // bare `find P [-maxdepth N] [-type f|d]` → WalkDir
                    // printing each entry path (streaming, no entry list).
                    if let Some(repl) = try_lower_find_stmt(cmd_args) {
                        *stmt = with_status_zero(repl);
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
                if emit && cmd == "printf" {
                    if let Some(val) = try_lower_printf_repeat(cmd_args) {
                        // printf emits NO trailing newline.
                        *stmt = with_status_zero(IrStmt::Output { value: val, newline: false, target: None });
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
                if emit && (cmd == "basename" || cmd == "dirname") && !cmd_args.is_empty() {
                    let which = if cmd == "dirname" { "dirname" } else { "basename" };
                    if let Some(text) = arg_to_expr(&cmd_args[0]) {
                        *stmt = with_status_zero(IrStmt::Output {
                            value: IrExpr::Ext(Box::new(PathName { text, which: which.to_string() })),
                            newline: true,
                            target: None,
                        });
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
        }
        IrStmt::Expr(expr) => {
            lower_expr(expr, arrays);
        }
        // Recurse into nested statement bodies (if/while/for/function/...)
        IrStmt::If { then, elsifs, else_, .. } => {
            for s in then.iter_mut() { lower_stmt(s, emit, arrays); }
            for (_, b) in elsifs.iter_mut() { for s in b.iter_mut() { lower_stmt(s, emit, arrays); } }
            for s in else_.iter_mut() { lower_stmt(s, emit, arrays); }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            for s in body.iter_mut() { lower_stmt(s, emit, arrays); }
        }
        IrStmt::For { body, .. } => { for s in body.iter_mut() { lower_stmt(s, emit, arrays); } }
        IrStmt::ForInit { init, body, .. } => {
            for s in init.iter_mut() { lower_stmt(s, emit, arrays); }
            for s in body.iter_mut() { lower_stmt(s, emit, arrays); }
        }
        IrStmt::Function { body, .. } => { for s in body.iter_mut() { lower_stmt(s, emit, arrays); } }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            for s in body.iter_mut() { lower_stmt(s, emit, arrays); }
        }
        IrStmt::Case { discriminant, clauses } => {
            lower_expr(discriminant, arrays);
            for c in clauses.iter_mut() { for s in c.body.iter_mut() { lower_stmt(s, emit, arrays); } }
        }
        IrStmt::Assign { .. } if emit => {
            // CAPTURE-INTERNAL REDUCTION: x=$(echo X | cut …) — reduce the
            // ASSIGN'S EXPRESSION to the composed primitive value (docs
            // shir-primitives.md §"Capture-internal reduction"), preserving
            // $? via SetChildError(0) for the provably-successful allowlist.
            if let Some(repl) = try_reduce_capture_assign(&*stmt, arrays) {
                *stmt = repl;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            if let IrStmt::Assign { expr, .. } = stmt { lower_expr(expr, arrays); }
        }
        IrStmt::Declare { init, .. } => { if let Some(e) = init { lower_expr(e, arrays); } }
        IrStmt::WriteFile { path, content, .. } => {
            lower_expr(path, arrays); lower_expr(content, arrays);
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => lower_expr(e, arrays),
        _ => {}
    }
}

fn lower_expr(expr: &mut IrExpr, arrays: &std::collections::HashSet<String>) {
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
            if let Some(replacement) = try_lower_param_op(args, arrays) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            for a in args.iter_mut() { lower_expr(a, arrays); }
        }
        // Nested pipeline in expression position (&& chains, if-conds,
        // command substitution): `echo X | cmd` inside `... && ...`.
        IrExpr::Call { func, args } if func == "pipeline" => {
            if let [IrExpr::Array(stages)] = args.as_slice() {
                if stages.len() == 2 {
                    // CONDITION-context grep: `echo X | grep -q P` reduces to
                    // StringContains here (status semantics preserved by the
                    // enclosing If/&& lowering). Statement level skips it.
                    if let [b1, b2] = stages.as_slice() {
                        if let Some(replacement) = try_lower_grep_cond(b1, b2) {
                            *expr = replacement;
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                    if let Some((replacement, _)) = try_lower_pipeline(stages) {
                        *expr = replacement;
                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
            for a in args.iter_mut() { lower_expr(a, arrays); }
        }
        IrExpr::Arrow(body) => {
            for s in body.iter_mut() { lower_stmt(s, false, arrays); }
        }
        IrExpr::Capture { expr: inner, .. } => lower_expr(inner, arrays),
        IrExpr::Array(items) => { for i in items.iter_mut() { lower_expr(i, arrays); } }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(e) = p { lower_expr(e, arrays); }
            }
        }
        IrExpr::Index { key, .. } => lower_expr(key, arrays),
        IrExpr::BinOp { lhs, rhs, .. } => { lower_expr(lhs, arrays); lower_expr(rhs, arrays); }
        IrExpr::Ternary { cond, then, else_, .. } => { lower_expr(cond, arrays); lower_expr(then, arrays); lower_expr(else_, arrays); }
        // `${#s}` outside a string lowers to getVar("##s") — the raw length
        // marker. Reduce to StrLen(read(s)).
        IrExpr::Call { func, args } if func == "getVar" => {
            if let Some(replacement) = try_lower_getvar_len(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            for a in args.iter_mut() { lower_expr(a, arrays); }
        }
        // Nested builtin/exec command in expression position: basename/dirname
        // inside $(...) — e.g. `dirname "$(pwd)"`.
        IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
            // Recursively lower nested expressions in args first
            for a in args.iter_mut() { lower_expr(a, arrays); }
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
        IrExpr::Call { args, .. } => { for a in args.iter_mut() { lower_expr(a, arrays); } }
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

    // stage1 produces text (echo, capture, etc.) — plus whether bash's
    // stage-1 output ENDS with a newline (echo appends one; printf only
    // when its last literal does). The distinction decides whether the
    // statement-level Output wrapper may re-append the stripped newline.
    let (text_expr, implicit_nl) = extract_stage_text(stage1_body)?;

    // Byte-takes slice the RAW text INCLUDING its trailing-newline state,
    // so the reduced value ends "with or without \n" exactly as bash's
    // bytes do. Reducible only when text AND count are static — then the
    // slice folds and `already_nl` is computed exactly; otherwise fall
    // back (an eager wrong-newline render is worse than the runtime).
    if cmd_name == "head" || cmd_name == "tail" {
        if let Some(k) = head_byte_count(cmd_args) {
            if let (IrExpr::Str(ts, _), Some(k)) = (&text_expr, Some(k)) {
                let mut raw = ts.clone();
                if implicit_nl {
                    raw.push('\n');
                }
                let n = raw.len() as i64;
                let (start, end) = if cmd_name == "head" {
                    (0, k.min(n))
                } else {
                    (n.saturating_sub(k).max(0), n)
                };
                if start >= 0 && end <= raw.len() as i64 && start <= end {
                    // floor the start / ceiling the end to UTF-8 boundaries
                    let b = raw.as_bytes();
                    let mut s = start.max(0) as usize;
                    while s > 0 && (b[s] & 0xC0) == 0x80 { s -= 1; }
                    let mut e2 = (end as usize).min(raw.len());
                    while e2 < raw.len() && (b[e2] & 0xC0) == 0x80 { e2 += 1; }
                    let sliced = raw[s..e2].to_string();
                    // The fold produces the EXACT output bytes — never let
                    // the Output wrapper re-append a newline (whether or
                    // not the slice happens to end in one).
                    return Some((IrExpr::Str(sliced, StrStyle::DoubleQuoted), true));
                }
            }
            return None;
        }
    }

    let replacement = lower_text_cmd(text_expr, cmd_name, cmd_args)?;
    // Line-oriented transforms pass the (stripped) terminal newline
    // through, so the Output wrapper re-appends it ONLY for an echo-like
    // source. wc PRINTS its own terminating newline after the number —
    // always append. A printf source without a trailing newline stays
    // without one.
    let already_nl = if cmd_name == "wc" { false } else { !implicit_nl };
    Some((replacement, already_nl))
}

/// The `-c N` byte-count form of head/tail (plain positive N; GNU's signed
/// `head -c -N` / `tail -c +K` forms are NOT covered → None → fallback).
fn head_byte_count(args: &[IrExpr]) -> Option<i64> {
    let strs: Vec<&str> = args.iter().filter_map(|a| match a {
        IrExpr::Str(s, _) => Some(s.as_str()),
        _ => None,
    }).collect();
    let mut i = 0;
    let mut found: Option<i64> = None;
    while i < strs.len() {
        if strs[i] == "-c" {
            match strs.get(i + 1).and_then(|c| c.parse::<i64>().ok()) {
                Some(k) if k > 0 => { found = Some(k); i += 2; continue; }
                _ => return None,
            }
        } else if let Some(rest) = strs[i].strip_prefix("-c") {
            match rest.parse::<i64>() {
                Ok(k) if k > 0 => found = Some(k),
                _ => return None,
            }
        }
        i += 1;
    }
    found
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
        // NOTE: grep deliberately absent — `grep -q` is a STATUS idiom
        // (no stdout); reducing it to StringContains is only valid in
        // CONDITION contexts (handled separately in lower_expr).
        "xargs" => try_lower_xargs(text, cmd_args),
        _ => None,
    }
}

/// Extract the text expression from a pipeline stage body, plus whether
/// bash's stage-1 output ends with a terminal newline:
///   echo ARGS  → true  (echo appends \n; the extraction drops it)
///   printf L…  → last literal ends with \n ?
///   a bare literal → same test on its text
fn extract_stage_text(stmts: &[IrStmt]) -> Option<(IrExpr, bool)> {
    match stmts {
        // echo/printf with string/literal args → concatenate
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
                        // Backslash escapes (printf '\n', echo -e) are NOT
                        // interpreted by this literal extraction — bail so
                        // such sources keep the original (correct) command.
                        if strs.iter().any(|s| s.contains('\\')) {
                            return None;
                        }
                        // The echo ARGS joined (echo's trailing newline is
                        // added by the statement Output wrapper, and by the
                        // wc -l newline-count case below).
                        let joined = strs.join(" ");
                        let nl = if name == "echo" { true } else { joined.ends_with('\n') };
                        return Some((IrExpr::Str(joined, StrStyle::DoubleQuoted), nl));
                    }
                    // VARIABLE-bearing source: a single interpolated arg
                    // (`echo "$s"`) is the variable's value verbatim — the
                    // interpolation IS the text, no interpretation needed.
                    // This is the corpus's dominant pipeline source shape
                    // (`echo "$s" | cut …`); refusing it would leave every
                    // real-world idiom at the sh2.* runtime fallback.
                    // A bare `$s` lowers to Call(getVar) rather than a
                    // one-part interpolation — same value, same treatment.
                    // ECHO ONLY: echo's terminal \n is unconditional, so the
                    // Output wrapper reproduces it exactly. A dynamic printf
                    // format may or may not end in \n — unknowable here, so
                    // printf keeps its literal-only path.
                    if name == "echo" && echo_args.len() == 1 {
                        let var_text: Option<IrExpr> = match &echo_args[0] {
                            IrExpr::Interpolate(parts) => Some(IrExpr::Interpolate(parts.clone())),
                            IrExpr::Call { func, args }
                                if func == "getVar" && args.len() == 1 =>
                            {
                                Some(echo_args[0].clone())
                            }
                            _ => None,
                        };
                        if let Some(text) = var_text {
                            return Some((text, true));
                        }
                    }
                }
            }
            None
        }
        // A literal-only expression (a string or all-literal interpolation),
        // NOT an arbitrary command — `paste | head` must not reduce as if
        // paste produced a literal string.
        [IrStmt::Expr(e)] => match e {
            IrExpr::Str(s, _) => Some((e.clone(), s.ends_with('\n'))),
            IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => {
                let txt: String = parts.iter().filter_map(|p| match p {
                    InterpPart::Lit(s) => Some(s.as_str()),
                    _ => None,
                }).collect();
                Some((e.clone(), txt.ends_with('\n')))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Extract the text expression from a pipeline stage body (echo, capture, etc.)
fn extract_text_from_stage(stmts: &[IrStmt]) -> Option<IrExpr> {
    extract_stage_text(stmts).map(|(e, _)| e)
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
    // Squeeze needs a well-defined output set: `tr -sd` (delete+squeeze of
    // DIFFERENT sets) and range/class squeeze sets aren't expressible as a
    // literal per-char run collapse — leave those to the runtime.
    if delete && squeeze {
        return None;
    }
    if squeeze && (from.contains('-') || to.contains('-')) {
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

fn try_lower_xargs(text: IrExpr, cmd_args: &[IrExpr]) -> Option<IrExpr> {
    // ONLY bare `| xargs` trims. `xargs -n1 echo "Number:"` EXECUTES a
    // command per word — reducing that to Trim is wrong.
    if !cmd_args.is_empty() { return None; }
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
        let r = try_lower_param_op(&args, &std::collections::HashSet::new()).expect("^^ lowers");
        let IrExpr::Ext(n) = &r else { panic!("expected Ext") };
        let n = n.as_any().downcast_ref::<CaseTransform>().unwrap();
        assert!(n.upper, "^^ should be upper");
    }

    #[test]
    fn param_case_lower() {
        let args = vec![st(",,"), st("var")];
        let r = try_lower_param_op(&args, &std::collections::HashSet::new()).expect(",, lowers");
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
        // A variable read reduces fine; a CAPTURE or command CALL does NOT —
        // embedding one inside an Ext node makes the A1 exporter punt
        // ("Other") and ingress refuse. Those sources keep the original
        // command (correct fallback).
        IrExpr::Var(..) => Some(arg.clone()),
        IrExpr::Call { func, .. } if func == "getVar" || func == "param" => Some(arg.clone()),
        _ => None,
    }
}

/// Reduce `param` expansion ops to primitives:
///   ${var^^} → Case(var, upper), ${var,,} → Case(var, lower)
///   ${var^} / ${var,} → CaseFirst (first char only)
///   ${var:2:3} → SubStr(var, 2, 3)
fn try_lower_param_op(args: &[IrExpr], arrays: &std::collections::HashSet<String>) -> Option<IrExpr> {
    if args.len() < 2 { return None; }
    let op = match &args[0] { IrExpr::Str(s, _) => s.as_str(), _ => return None };
    let var = param_var_read(&args[1])?;
    match op {
        ",," => Some(IrExpr::Ext(Box::new(CaseTransform { text: var, upper: false }))),
        "^^" => Some(IrExpr::Ext(Box::new(CaseTransform { text: var, upper: true }))),
        "slice" if args.len() >= 4 => {
            // ${v:N:M} on a SCALAR is SubStr; on an ARRAY it's an index
            // subset. The two produce identical param calls, so consult the
            // array-variable census: only reduce when v was never declared/
            // written as an array anywhere in the program.
            let raw_name = match &args[1] { IrExpr::Str(s, _) => s.as_str(), _ => return None };
            // "p", "p[@]", "p[*]", "p[i]" all refer to array p — compare on
            // the BASE name against the census.
            let base = raw_name.split('[').next().unwrap_or(raw_name);
            if arrays.contains(base) { return None; }
            let off = match &args[2] { IrExpr::Str(s, _) => s.parse::<i64>().ok()?, _ => return None };
            let len = match &args[3] { IrExpr::Str(s, _) => s.parse::<i64>().ok()?, _ => return None };
            // Negative offset (counting from end) or negative length (up-to-M
            // from end) are bash-only semantics SubStrExtract can't express —
            // leave them to the runtime.
            if off < 0 || len < 0 { return None; }
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

// ── Array-variable census (for safe scalar-slice reduction) ─────────

/// Collect variables that are ARRAYS anywhere in the program: declared via
/// DeclareArray / setArray / setArrayAppend, written with an index
/// (`a[i]=…`), or filled by readarray/mapfile. `${v:N:M}` on such a var is
/// an ARRAY slice (index subset), which must NOT reduce to SubStrExtract.
fn collect_array_names(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
    for s in stmts {
        match s {
            IrStmt::DeclareArray { var, .. } => { out.insert(var.clone()); }
            IrStmt::Assign { targets, expr, .. } => {
                // Array markers: indexed target (`a[i]=…`), array-literal
                // RHS (`p=(1 2 3)`), or a setArray/setArrayAppend call as the
                // assigned value — any of these make the var an ARRAY.
                let arr_rhs = match expr {
                    IrExpr::Array(..) => true,
                    IrExpr::Call { func, .. } => matches!(func.as_str(), "setArray" | "setArrayAppend"),
                    _ => false,
                };
                for t in targets {
                    if !t.indices.is_empty() || arr_rhs {
                        out.insert(t.var.clone());
                    }
                }
            }
            IrStmt::Expr(IrExpr::Call { func, args }) => {
                if matches!(func.as_str(), "setArray" | "setArrayAppend"
                    | "readarray" | "mapfile") {
                    if let Some(IrExpr::Str(n, _)) = args.first() {
                        out.insert(n.trim_start_matches('-').to_string());
                    }
                }
            }
            _ => {}
        }
        // recurse into nested bodies
        let mut sub: Vec<&Vec<IrStmt>> = Vec::new();
        match s {
            IrStmt::If { then, elsifs, else_, .. } => {
                sub.push(then);
                for (_, b) in elsifs.iter() { sub.push(b); }
                sub.push(else_);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. } | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body)
            | IrStmt::Try { body, .. } => sub.push(body),
            IrStmt::Redirect { inner, .. } => sub.push(inner),
            IrStmt::ForInit { init, body, .. } => { sub.push(init); sub.push(body); }
            IrStmt::Case { clauses, .. } => {
                for cl in clauses.iter() { sub.push(&cl.body); }
            }
            _ => {}
        }
        for b in sub { collect_array_names(b, out); }
    }
}


#[cfg(test)]
mod census_tests {
    use super::*;
    fn st(s: &str) -> IrExpr { IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted) }
    #[test]
    fn collector_finds_setarray() {
        // p=(1 2 3) lowers to exec/builtin("setArray", ["p", [...]])
        let stmt = IrStmt::Expr(IrExpr::Call {
            func: "setArray".to_string(),
            args: vec![st("p"), IrExpr::Array(vec![st("1"), st("2")])],
        });
        let mut out = std::collections::HashSet::new();
        collect_array_names(&[stmt], &mut out);
        assert!(out.contains("p"), "collector found: {:?}", out);
    }
}

/// `printf 'X%.0s' ARGS...` → RepeatStr("X", N) — the classic repeat idiom.
/// The format must be exactly `<unit>%.0s` with no other conversions; N is
/// the static arg count, where a brace(...) expansion contributes its range
/// size (`{1..200}` → 200). Anything non-static → no reduction.
fn try_lower_printf_repeat(args: &[IrExpr]) -> Option<IrExpr> {
    let fmt = match args.first()? {
        IrExpr::Str(s, _) => s.as_str(),
        IrExpr::Interpolate(parts) if parts.len() == 1 => match &parts[0] {
            InterpPart::Lit(s) => s.as_str(),
            _ => return None,
        },
        _ => return None,
    };
    let suffix = "%.0s";
    if !fmt.ends_with(suffix) { return None; }
    let unit = &fmt[..fmt.len() - suffix.len()];
    if unit.contains('%') { return None; } // other conversions — bail

    let mut total: i64 = 0;
    for a in &args[1..] {
        match a {
            IrExpr::Str(..) => total += 1,
            IrExpr::Interpolate(parts) if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) => total += 1,
            IrExpr::Call { func, args: ba } if func == "brace" => {
                // brace(prefix, Json(groups), Json(middles?), suffix)
                let groups = ba.get(1)?;
                let gv = match groups { IrExpr::Json(v) => v, _ => return None };
                let outer = gv.as_array()?;
                if outer.len() != 1 { return None; }
                let group = outer[0].as_array()?;
                if group.len() != 1 { return None; }
                let spec = group[0].get("range")?.as_array()?;
                if spec.len() < 2 { return None; }
                let s: i64 = spec[0].as_str()?.parse().ok()?;
                let e: i64 = spec[1].as_str()?.parse().ok()?;
                if !spec.get(2)?.is_null() { return None; } // step unsupported
                if e >= s { total += e - s + 1; }
            }
            _ => return None,
        }
    }
    if total < 1 { return None; }
    Some(IrExpr::Ext(Box::new(RepeatStr {
        text: IrExpr::Str(unit.to_string(), StrStyle::DoubleQuoted),
        count: IrExpr::Int(total),
    })))
}

/// Reduce `x=$(echo X | <reducible>)`: the Assign's Capture-wrapped pipeline
/// becomes the composed Ext VALUE. Allowlist only (cut/tr/head/tail/wc/xargs/
/// simple sed) — these exit 0 on static input, so `SetChildError(0)` preserves
/// $?. grep is excluded (status idiom). Counts append the echo trailing \n.
fn try_reduce_capture_assign(stmt: &IrStmt, arrays: &std::collections::HashSet<String>) -> Option<IrStmt> {
    // Peek (immutable), build replacement from cloned pieces.
    let (targets, asm, stages) = match stmt {
        IrStmt::Assign { targets, asm, expr, .. } => {
            let inner = match expr {
                IrExpr::Capture { expr: ci, native: false } => ci.as_ref(),
                _ => return None,
            };
            let stages = match inner {
                IrExpr::Arrow(body) => match body.as_slice() {
                    [IrStmt::Expr(pe)] => match pe {
                        IrExpr::Call { func, args } if func == "pipeline" => {
                            match args.as_slice() {
                                [IrExpr::Array(st)] if st.len() == 2 => st.as_slice(),
                                _ => return None,
                            }
                        }
                        _ => return None,
                    },
                    _ => return None,
                },
                _ => return None,
            };
            (targets.clone(), asm.clone(), stages.to_vec())
        }
        _ => return None,
    };

    let stage_bodies: Vec<&[IrStmt]> = stages.iter().map(|s| match s {
        IrExpr::Arrow(b) => b.as_slice(), _ => unreachable!("checked above"),
    }).collect();
    if stage_bodies.len() != 2 { return None; }

    // `x=$(find ARGS | wc -l)` → streaming directory-walk count assigned
    // to the target (find's exit status is 0 on success; allowlisted).
    {
        let find_wc = stage_bodies.len() == 2
            && matches!(stage_bodies[0], [IrStmt::Expr(IrExpr::Call { ref func, ref args })]
                if (func == "exec" || func == "builtin")
                    && matches!(args.as_slice(),
                        [IrExpr::Str(n, _), IrExpr::Array(_)] if n == "find"));
        if find_wc {
            if let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = stage_bodies[1] {
                if f2 == "exec" || f2 == "builtin" {
                    if let [IrExpr::Str(n2, _), IrExpr::Array(wa)] = a2.as_slice() {
                        if n2 == "wc"
                            && wa.len() == 1
                            && matches!(&wa[0], IrExpr::Str(f, _) if f.as_str() == "-l")
                        {
                            if let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] =
                                stage_bodies[0]
                            {
                                if let [IrExpr::Str(_, _), IrExpr::Array(fa)] = a1.as_slice() {
                                    if let Some((path, tf, md)) = parse_find_args(fa) {
                                        let mut block = walk_dir_count(
                                            IrExpr::Str(path, StrStyle::DoubleQuoted),
                                            tf,
                                            md.map(IrExpr::Int),
                                        );
                                        if let IrStmt::Block(ref mut stmts) = block {
                                            // the counter is the capture VALUE:
                                            // swap the trailing Output for the Assign
                                            if let Some(IrStmt::Output { value, .. }) = stmts.last_mut() {
                                                let value = value.clone();
                                                *stmts.last_mut().unwrap() = IrStmt::Assign {
                                                    targets,
                                                    expr: value,
                                                    asm,
                                                };
                                                return Some(block);
                                            }
                                        }
                                        return None;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Source must be a clean literal echo/printf (no flags/backslash escapes).
    let text = extract_text_from_stage(stage_bodies[0])?;

    // Last stage must be in the status-0 allowlist.
    let (cmd_name, cmd_args) = match stage_bodies[1] {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "exec" || func == "builtin" => {
            match args.as_slice() {
                [IrExpr::Str(n, _), IrExpr::Array(a)] => (n.as_str(), a.as_slice()),
                _ => return None,
            }
        }
        _ => return None,
    };
    const STATUS0: [&str; 7] = ["cut", "tr", "head", "tail", "wc", "xargs", "sed"];
    if !STATUS0.contains(&cmd_name) { return None; }

    let value = lower_text_cmd(text, cmd_name, cmd_args)?;

    // Preserve $?: the allowlisted pipeline exits 0.
    Some(IrStmt::Block(vec![
        IrStmt::Assign { targets, expr: value, asm },
        IrStmt::SetChildError(IrExpr::Int(0)),
    ]))
}

/// Read the ForEachLine loop variable inside composed Ext children:
/// a plain Var — compiled backends render it natively, and the ESTree
/// ForEachLine arm registers the name in LIFTED_STRING so it renders as
/// the bare callback identifier there too.
fn loop_var_read(lv: &str) -> IrExpr {
    IrExpr::Var(lv.to_string(), None)
}

/// Build a STREAMING line counter: `n=0; ForEachLine(src, guard? n+=1)` —
/// value = n. guard=None counts every line (`wc -l < F`); Some(pattern)
/// counts lines containing it (`grep P F | wc -l`). O(1) memory.
fn streaming_line_count(source: IrExpr, guard: Option<IrExpr>) -> IrStmt {
    let k = LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
    let cnt = format!("__lc{}", k);
    let lv = format!("__l{}", k);
    let mut body: Vec<IrStmt> = Vec::new();
    let incr = IrStmt::Assign {
        targets: vec![AssignTarget { var: cnt.clone(), sigil: None, indices: vec![] }],
        expr: IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var(cnt.clone(), None)),
            op: BinOpKind::Add,
            rhs: Box::new(IrExpr::Int(1)),
        },
        asm: None,
    };
    match guard {
        Some(pat) => {
            let cond = IrExpr::Ext(Box::new(StringContains {
                text: loop_var_read(&lv),
                pattern: pat,
            }));
            body.push(IrStmt::If { cond, then: vec![incr], elsifs: vec![], else_: vec![] });
        }
        None => body.push(incr),
    }
    IrStmt::Block(vec![
        IrStmt::Assign {
            targets: vec![AssignTarget { var: cnt.clone(), sigil: None, indices: vec![] }],
            expr: IrExpr::Int(0),
            asm: None,
        },
        IrStmt::Ext(Box::new(ForEachLine { source, var: lv, limit: None, body })),
        IrStmt::Output {
            value: IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str(cnt, StrStyle::DoubleQuoted)],
            },
            newline: true,
            target: None,
        },
    ])
}

/// GNU find subset: [PATH] [-maxdepth N] [-type f|d]. Anything else
/// (-name, -perm, multi-path, expressions) is not lowered → None.
fn parse_find_args(args: &[IrExpr]) -> Option<(String, Option<String>, Option<i64>)> {
    let mut strs: Vec<&str> = Vec::new();
    for x in args {
        match x {
            IrExpr::Str(s, _) => strs.push(s.as_str()),
            IrExpr::Interpolate(p) if p.len() == 1 => match &p[0] {
                InterpPart::Lit(s) => strs.push(s.as_str()),
                _ => return None,
            },
            _ => return None,
        }
    }
    let mut path: Option<String> = None;
    let mut tf: Option<String> = None;
    let mut md: Option<i64> = None;
    let mut i = 0;
    while i < strs.len() {
        match strs[i] {
            "-maxdepth" => {
                let v = strs.get(i + 1)?.parse::<i64>().ok()?;
                md = Some(v);
                i += 2;
            }
            "-type" => {
                let v = strs.get(i + 1)?;
                if !matches!(*v, "f" | "d") { return None; }
                tf = Some(v.to_string());
                i += 2;
            }
            s if s.starts_with('-') => return None,
            p => {
                if path.is_some() { return None; } // multi-path → fallback
                path = Some(p.to_string());
                i += 1;
            }
        }
    }
    Some((path?, tf, md))
}

/// Streaming directory-entry counter: `n=0; WalkDir(src, l, n+=1)` — the
/// tree is walked entry-by-entry; contents are never opened.
fn walk_dir_count(source: IrExpr, type_filter: Option<String>, maxdepth: Option<IrExpr>) -> IrStmt {
    let k = LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
    let cnt = format!("__lc{}", k);
    let lv = format!("__l{}", k);
    let incr = IrStmt::Assign {
        targets: vec![AssignTarget { var: cnt.clone(), sigil: None, indices: vec![] }],
        expr: IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var(cnt.clone(), None)),
            op: BinOpKind::Add,
            rhs: Box::new(IrExpr::Int(1)),
        },
        asm: None,
    };
    IrStmt::Block(vec![
        IrStmt::Assign {
            targets: vec![AssignTarget { var: cnt.clone(), sigil: None, indices: vec![] }],
            expr: IrExpr::Int(0),
            asm: None,
        },
        IrStmt::Ext(Box::new(WalkDir {
            source,
            var: lv,
            body: vec![incr],
            type_filter,
            maxdepth: maxdepth.map(Box::new),
        })),
        IrStmt::Output {
            value: IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str(cnt, StrStyle::DoubleQuoted)],
            },
            newline: true,
            target: None,
        },
    ])
}

/// `find ARGS | wc -l` — stage1 find subset, stage2 exactly wc -l.
fn try_lower_find_wc(stage1: &[IrStmt], stage2: &[IrStmt]) -> Option<IrStmt> {
    let [IrStmt::Expr(IrExpr::Call { func, args })] = stage1 else { return None };
    if !(func == "exec" || func == "builtin") { return None; }
    let [IrExpr::Str(n, _), IrExpr::Array(a)] = args.as_slice() else { return None };
    if n != "find" { return None; }
    let wc_ok = matches!(stage2,
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if (func == "exec" || func == "builtin")
                && matches!(args.as_slice(),
                    [IrExpr::Str(n, _), IrExpr::Array(wa)] if n == "wc" && wa.len() == 1
                        && matches!(&wa[0], IrExpr::Str(f, _) if f.as_str() == "-l")));
    if !wc_ok { return None; }
    let (path, tf, md) = parse_find_args(a)?;
    Some(walk_dir_count(
        IrExpr::Str(path, StrStyle::DoubleQuoted),
        tf,
        md.map(IrExpr::Int),
    ))
}

/// A statement-level `find ARGS` (no pipeline): print each entry path.
fn try_lower_find_stmt(cmd_args: &[IrExpr]) -> Option<IrStmt> {
    let (path, tf, md) = parse_find_args(cmd_args)?;
    Some(IrStmt::Ext(Box::new(WalkDir {
        source: IrExpr::Str(path, StrStyle::DoubleQuoted),
        var: "__l".to_string(),
        body: vec![IrStmt::Output {
            value: loop_var_read("__l"),
            newline: true,
            target: None,
        }],
        type_filter: tf,
        maxdepth: md.map(|v| Box::new(IrExpr::Int(v))),
    })))
}

/// `grep P F | wc -l` → streaming filtered count. Static args only:
/// grep takes exactly [P, F] (no flags), both literals, F a path.
fn try_lower_grep_wc(stage1: &[IrStmt], stage2: &[IrStmt]) -> Option<IrStmt> {
    // wc must be exactly -l
    let wc_args_ok = matches!(stage2,
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if (func == "exec" || func == "builtin")
                && matches!(args.as_slice(),
                    [IrExpr::Str(n, _), IrExpr::Array(wa)] if n == "wc" && wa.len() == 1
                        && matches!(&wa[0], IrExpr::Str(f, _) if f.as_str() == "-l")));
    if !wc_args_ok { return None; }
    let [IrStmt::Expr(IrExpr::Call { func, args })] = stage1 else { return None };
    if !(func == "exec" || func == "builtin") { return None; }
    let [IrExpr::Str(n, _), IrExpr::Array(a)] = args.as_slice() else { return None };
    if n != "grep" || a.len() != 2 { return None; }
    // no flags: both args literal strings, second is a path
    let mut strs: Vec<&str> = Vec::new();
    for x in a.iter() {
        match x {
            IrExpr::Str(s, _) => strs.push(s.as_str()),
            IrExpr::Interpolate(p) if p.len() == 1 => {
                match &p[0] { InterpPart::Lit(s) => strs.push(s.as_str()), _ => return None }
            }
            _ => return None,
        }
    }
    if strs.len() != 2 || strs[1].starts_with('-') { return None; }
    let pat = IrExpr::Str(strs[0].to_string(), StrStyle::DoubleQuoted);
    let path = IrExpr::Str(strs[1].to_string(), StrStyle::DoubleQuoted);
    Some(streaming_line_count(path, Some(pat)))
}

/// `grep P F | cut -dD -fN` → ForEachLine(F, if Contains(l,P) then
/// Output(FieldExtract(l,D,N))). Static args only.
fn try_lower_grep_cut(stage1: &[IrStmt], stage2: &[IrStmt]) -> Option<IrStmt> {
    // stage1: grep P F (no flags, two literals)
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = stage1 else { return None };
    if !(f1 == "exec" || f1 == "builtin") { return None; }
    let [IrExpr::Str(n1, _), IrExpr::Array(ga)] = a1.as_slice() else { return None };
    if n1 != "grep" || ga.len() != 2 { return None; }
    let mut gs: Vec<&str> = Vec::new();
    for x in ga.iter() {
        match x {
            IrExpr::Str(s, _) => gs.push(s.as_str()),
            IrExpr::Interpolate(p) if p.len() == 1 => {
                match &p[0] { InterpPart::Lit(s) => gs.push(s.as_str()), _ => return None }
            }
            _ => return None,
        }
    }
    if gs.len() != 2 || gs[1].starts_with('-') { return None; }
    let pat = IrExpr::Str(gs[0].to_string(), StrStyle::DoubleQuoted);
    let path = IrExpr::Str(gs[1].to_string(), StrStyle::DoubleQuoted);

    // stage2: cut with flag-only args
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = stage2 else { return None };
    if !(f2 == "exec" || f2 == "builtin") { return None; }
    let [IrExpr::Str(n2, _), IrExpr::Array(ca)] = a2.as_slice() else { return None };
    if n2 != "cut" { return None; }
    let field = try_lower_cut(loop_var_read("__l"), ca)?;
    let cond = IrExpr::Ext(Box::new(StringContains {
        text: loop_var_read("__l"),
        pattern: pat,
    }));
    Some(IrStmt::Ext(Box::new(ForEachLine {
        source: path,
        var: "__l".to_string(),
        limit: None,
        body: vec![IrStmt::If {
            cond,
            then: vec![IrStmt::Output { value: field, newline: true, target: None }],
            elsifs: vec![],
            else_: vec![],
        }],
    })))
}

/// `grep P F | tr/sed ARGS…` → ForEachLine(F, if Contains(l,P) then
/// Output(<primitive>(l))). Same shape as try_lower_grep_cut but for the
/// per-line TEXT transforms (cut has its own arm — flag-only static).
fn try_lower_grep_text_cmd(stage1: &[IrStmt], stage2: &[IrStmt]) -> Option<IrStmt> {
    // stage1: grep P F (no flags, two literals)
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = stage1 else { return None };
    if !(f1 == "exec" || f1 == "builtin") { return None; }
    let [IrExpr::Str(n1, _), IrExpr::Array(ga)] = a1.as_slice() else { return None };
    if n1 != "grep" || ga.len() != 2 { return None; }
    let mut gs: Vec<&str> = Vec::new();
    for x in ga.iter() {
        match x {
            IrExpr::Str(s, _) => gs.push(s.as_str()),
            IrExpr::Interpolate(p) if p.len() == 1 => {
                match &p[0] { InterpPart::Lit(s) => gs.push(s.as_str()), _ => return None }
            }
            _ => return None,
        }
    }
    if gs.len() != 2 || gs[1].starts_with('-') { return None; }
    let pat = IrExpr::Str(gs[0].to_string(), StrStyle::DoubleQuoted);
    let path = IrExpr::Str(gs[1].to_string(), StrStyle::DoubleQuoted);

    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = stage2 else { return None };
    if !(f2 == "exec" || f2 == "builtin") { return None; }
    let [IrExpr::Str(n2, _), IrExpr::Array(ca)] = a2.as_slice() else { return None };
    if !matches!(n2.as_str(), "tr" | "sed") { return None; }

    let val = lower_text_cmd(loop_var_read("__l"), n2, ca)?;
    let cond = IrExpr::Ext(Box::new(StringContains {
        text: loop_var_read("__l"),
        pattern: pat,
    }));
    Some(IrStmt::Ext(Box::new(ForEachLine {
        source: path,
        var: "__l".to_string(),
        limit: None,
        body: vec![IrStmt::If {
            cond,
            then: vec![IrStmt::Output { value: val, newline: true, target: None }],
            elsifs: vec![],
            else_: vec![],
        }],
    })))
}

/// `cat F | wc -l` / `cat F | cut flags` → ForEachLine forms. cat's file
/// becomes the ForEachLine source; stage2 must be flag-only static.
fn try_lower_cat_pipe(stage1: &[IrStmt], stage2: &[IrStmt]) -> Option<IrStmt> {
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = stage1 else { return None };
    if !(f1 == "exec" || f1 == "builtin") { return None; }
    let [IrExpr::Str(n1, _), IrExpr::Array(a1a)] = a1.as_slice() else { return None };
    if n1 != "cat" || a1a.len() != 1 { return None; }
    let path = match &a1a[0] {
        IrExpr::Str(s, _) if !s.starts_with('-') =>
            IrExpr::Str(s.clone(), StrStyle::DoubleQuoted),
        _ => return None,
    };

    // stage2: wc -l → counter
    if let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = stage2 {
        if f2 == "exec" || f2 == "builtin" {
            if let [IrExpr::Str(n2, _), IrExpr::Array(wa)] = a2.as_slice() {
                if n2 == "wc" && wa.len() == 1
                    && matches!(&wa[0], IrExpr::Str(f, _) if f.as_str() == "-l") {
                    return Some(streaming_line_count(path, None));
                }
            }
        }
    }

    // stage2: cut flags → per-line FieldExtract output
    if let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = stage2 {
        if f2 == "exec" || f2 == "builtin" {
            if let [IrExpr::Str(n2, _), IrExpr::Array(ca)] = a2.as_slice() {
                if n2 == "cut" {
                    if let Some(field) = try_lower_cut(loop_var_read("__l"), ca) {
                        return Some(IrStmt::Ext(Box::new(ForEachLine {
                            source: path,
                            var: "__l".to_string(),
                            limit: None,
                            body: vec![IrStmt::Output {
                                value: field, newline: true, target: None,
                            }],
                        })));
                    }
                }
            }
        }
    }
    None
}

/// `grep -c P F` → STREAMING guarded count (grep prints the count itself).
fn try_lower_grep_count(stage1: &[IrStmt], stage2: &[IrStmt]) -> Option<IrStmt> {
    // stage2 must be a bare `grep` passthrough? No — stage2 is `wc -l`;
    // grep -c prints its own count, so the PIPELINE is grep -c P F | wc -l?
    // No: `grep -c P F` alone prints the count. Handle the PIPELINE form
    // `… | wc -l` where stage1 is grep -c: bash would print count AND wc
    // counts lines of it — rare; skip. Instead: bare `grep -c P F` is a
    // single command (handled in the plain-command arm via
    // streaming_line_count). This fn handles `grep -c P F` as stage1 of a
    // 2-stage pipeline whose stage2 is `cat`/nothing — not a corpus shape;
    // kept for symmetry: only accept stage2 == cat passthrough.
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = stage2 else { return None };
    if !(f2 == "exec" || f2 == "builtin") { return None; }
    let [IrExpr::Str(n2, _), IrExpr::Array(ca)] = a2.as_slice() else { return None };
    if !(n2 == "cat" && ca.is_empty()) { return None; }
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = stage1 else { return None };
    if !(f1 == "exec" || f1 == "builtin") { return None; }
    let [IrExpr::Str(n1, _), IrExpr::Array(ga)] = a1.as_slice() else { return None };
    if n1 != "grep" || ga.len() != 3 { return None; }
    // shape: [-c, P, F]
    let is_c = matches!(&ga[0], IrExpr::Str(s, _) if s.as_str() == "-c");
    if !is_c { return None; }
    let pat = match &ga[1] {
        IrExpr::Str(s, _) => IrExpr::Str(s.clone(), StrStyle::DoubleQuoted),
        IrExpr::Interpolate(p) if p.len() == 1 => match &p[0] {
            InterpPart::Lit(s) => IrExpr::Str(s.clone(), StrStyle::DoubleQuoted),
            _ => return None,
        },
        _ => return None,
    };
    let path = match &ga[2] {
        IrExpr::Str(s, _) if !s.starts_with('-') =>
            IrExpr::Str(s.clone(), StrStyle::DoubleQuoted),
        _ => return None,
    };
    Some(streaming_line_count(path, Some(pat)))
}

/// `echo X | grep -q P` in EXPRESSION/CONDITION position → StringContains.
/// Only fires here (conditions), never at statement level where the status
/// semantics would be lost.
fn try_lower_grep_cond(stage1: &IrExpr, stage2: &IrExpr) -> Option<IrExpr> {
    let b1 = match stage1 { IrExpr::Arrow(b) => b.as_slice(), _ => return None };
    let b2 = match stage2 { IrExpr::Arrow(b) => b.as_slice(), _ => return None };
    let text = extract_text_from_stage(b1)?;
    let [IrStmt::Expr(IrExpr::Call { func, args })] = b2 else { return None };
    if !(func == "exec" || func == "builtin") { return None; }
    let [IrExpr::Str(n, _), IrExpr::Array(ga)] = args.as_slice() else { return None };
    if n != "grep" { return None; }
    try_lower_grep(text, ga)
}

/// A statement-level reduction replaces a COMMAND with a pure value — the
/// original command would have set $? = 0 (allowlisted, static-good input).
/// Wrap so the observable status is preserved: Block([stmt, SetChildError(0)]).
fn with_status_zero(s: IrStmt) -> IrStmt {
    IrStmt::Block(vec![s, IrStmt::SetChildError(IrExpr::Int(0))])
}
