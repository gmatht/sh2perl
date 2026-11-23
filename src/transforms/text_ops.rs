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
use std::sync::atomic::{AtomicUsize, Ordering};

static LIFT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// May the CURRENT target render this Ext node? The §11.12 offer: a tag in the
/// target's manifest means its renderer spells the node natively, so swapping
/// the exact command for the node cannot regress. An explicit
/// `SH2_TRANSFORMS=text-ops` keeps the historical emit-everything mode (the
/// A/B worker's baseline, and every other backend's path today).
fn offered(tag: &str) -> bool {
    let enabled = std::env::var("SH2_TRANSFORMS").unwrap_or_default();
    if enabled.split(',').any(|s| s.trim() == "text-ops") {
        return true;
    }
    match crate::transforms::target() {
        // An UNSET target is the historical "every transform applies" (unit
        // tests, the embed path): emit every node, as before.
        None => true,
        Some(t) => crate::transforms::planner::node_manifest(&t).contains(&tag),
    }
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // text-ops is an EXPERIMENTAL lowering that changes the shIR shape
    // (pipelines/commands → ExtExpr nodes). It is opt-in ONLY: run when
    // SH2_TRANSFORMS explicitly lists "text-ops". This keeps the
    // default corpus gate and unit tests green (the analyses and renderers
    // have conservative Ext-node defaults, but the byte-equal round-trip
    // and corpus tests still pin the UN-lowered shape).
    let enabled = std::env::var("SH2_TRANSFORMS").unwrap_or_default();
    let opted_in = enabled.split(',').any(|s| s.trim() == "text-ops");
    // Rust takes the manifest offer (PLAN v157): its renderer declares each
    // text node it spells in `std`, and `offered()` withholds every other tag,
    // so the transform fires only for the shapes that target consumes. The
    // other backends keep the explicit opt-in until their own gates pin the
    // new IR shape.
    // Rust takes the manifest offer (PLAN v157): its renderer declares each
    // text node it spells in `std`, and `offered()` withholds every other tag,
    // so the transform fires only for the shapes that target consumes. The
    // other backends keep the explicit opt-in until their own gates pin the
    // new IR shape.
    let manifest_offer = matches!(
        crate::transforms::target().as_deref(),
        Some("rs") | Some("rust")
    );
    if !opted_in && !manifest_offer {
        return false;
    }
    // text-ops is often exercised ALONE (SH2_TRANSFORMS=text-ops) — the
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
        // The OFFER, enforced: a rewrite may build a node this target cannot
        // render (every site is filtered by `offered()`, but a site can also
        // emit a node indirectly, and a new site must not be able to slip
        // one past). Keep a copy and restore it when the result holds an
        // undeclared node — the original command is exact everywhere.
        let keep = stmt.clone();
        lower_stmt(stmt, true, &arrays);
        if let Some(tag) = first_unoffered(stmt) {
            if std::env::var("SH2C_DEBUG").is_ok() {
                eprintln!("text-ops: restoring the command, {tag} is not offered to this target");
            }
            *stmt = keep;
        }
    }
    let after = LIFT_COUNT.load(Ordering::Relaxed);
    if after > before {
    }
    after > before
}

/// The EXACT bytes of a compile-time literal: a plain string, or an
/// interpolation whose every part is a literal (a here-string's target can
/// arrive as `"x:y:z"`, one Lit part). A read — even of a variable that holds
/// a constant — is not literal: its value is a runtime fact.
fn literal_text(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts)
            if !parts.is_empty() && parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) =>
        {
            Some(
                parts
                    .iter()
                    .map(|p| match p {
                        InterpPart::Lit(s) => s.as_str(),
                        _ => unreachable!(),
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// The first Ext-node tag in `stmt` that the current target does not
/// declare, if any. Scanned from the Debug form: the IR has no uniform child
/// accessor across `IrStmt`, and a false positive here costs exactly one
/// refused rewrite (the exact original command is kept), never a wrong value.
fn first_unoffered(stmt: &IrStmt) -> Option<String> {
    let dbg = format!("{:?}", stmt);
    for (i, _) in dbg.match_indices("Ext(") {
        let rest = dbg[i + "Ext(".len()..].trim_start();
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        if !offered(&name) {
            return Some(name);
        }
    }
    None
}

fn lower_stmt(stmt: &mut IrStmt, emit: bool, arrays: &std::collections::HashSet<String>) {
    // ── SUBSHELL SCOPE ISOLATION: `(a=2; …)` runs in a copy of the shell
    // — writes inside must NOT leak out. Our flat-variable backends render
    // Subshell as a plain block (writes leak). Normalise: snapshot every
    // scalar the body WRITES into fresh temps, run the body, restore.
    // Array/hash writes, function definitions and unset keep the original
    // shape (explicit fallback).
    if emit {
        if let IrStmt::Subshell(body) = stmt {
            let mut written: Vec<String> = Vec::new();
            let supported = collect_scalar_writes(body, &mut written)
                && !written.iter().any(|v| v.starts_with("__"));
            if supported {
                written.sort();
                written.dedup();
                let k = LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                let mut out_stmts: Vec<IrStmt> = Vec::new();
                for (i, v) in written.iter().enumerate() {
                    // save current value (" " model: never-assigned reads "")
                    out_stmts.push(IrStmt::Assign {
                        targets: vec![AssignTarget {
                            var: format!("__sv{}_{}", k, i),
                            sigil: None,
                            indices: vec![],
                        }],
                        expr: IrExpr::Var(v.clone(), None),
                        asm: None,
                    });
                }
                out_stmts.extend(body.iter().cloned());
                for (i, v) in written.iter().enumerate() {
                    // restore
                    out_stmts.push(IrStmt::Assign {
                        targets: vec![AssignTarget { var: v.clone(), sigil: None, indices: vec![] }],
                        expr: IrExpr::Var(format!("__sv{}_{}", k, i), None),
                        asm: None,
                    });
                }
                *stmt = IrStmt::Block(out_stmts);
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
    }

    // ── construct normalisations (docs/shir-reductions.md §Normalisation
    // transforms): let / local / read — re-expressed in vocabulary every
    // backend already renders. Runs FIRST, for every statement shape.
    if emit {
        if let Some(repl) = try_normalize_construct_stmt(stmt) {
            *stmt = repl;
            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // `(( x > 3 ))` / `let "x > 3"` in If/While CONDITION position →
        // native Arith comparison (the zsh-sh-go frontend lowers these to
        // exec("let", [text])).
        if let IrStmt::If { cond, .. } | IrStmt::While { cond, .. } = stmt {
            if let IrExpr::Call { func, args, .. } = cond {
                if (func == "exec" || func == "builtin")
                    && matches!(args.as_slice(),
                        [IrExpr::Str(n, _), IrExpr::Array(wa)]
                            if n == "let"
                                && matches!(wa.as_slice(), [IrExpr::Str(_, _)]))
                {
                    let text_opt = if let [_, IrExpr::Array(wa)] = args.as_slice() {
                        match wa.as_slice() {
                            [IrExpr::Str(tx, _)] => Some(tx.clone()),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(text) = text_opt {
                        if let Some(repl) = let_cond_from_text(&text) {
                            *cond = repl;
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }
    match stmt {
        // ShIR pipeline: IrExpr::Call { func: "pipeline", args: [Array(stages)] }
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "pipeline" => {
            if let [IrExpr::Array(stages)] = args.as_slice() {
                if stages.len() >= 2 {
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
                        if let Some((replacement, already_nl)) = try_lower_pipeline(stages, true) {
                            // A statement-level pipeline PRINTS its result.
                            *stmt = with_status_zero(IrStmt::Output { value: replacement, newline: !already_nl, target: None });
                            LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                            return;
                        }
                    }
                }
            }
        }
        // STRUCTURED pipeline statement — `IrStmt::Pipeline` is the shape the
        // parser builds for a FINAL pipeline (`echo x | head -n 5` as the last
        // line), and the arm above only knew the `Call{func:"pipeline"}` form.
        // So a program ENDING in `| head`/`| tail`/`| tr`/`| wc`/`| sort` kept
        // its bash spawn even though the SAME two stages reduce natively a few
        // lines earlier. Same reductions, same proof, same allowlist.
        //
        // `capture`/`last_output` must be None: with either set the pipeline's
        // stdout is consumed rather than printed, so it is not this arm's
        // "print the reduced value" contract.
        IrStmt::Pipeline { stages, capture: None, last_output: None, .. } if emit => {
            {
                if let Some((replacement, already_nl)) = try_lower_chain(stages, true) {
                    *stmt = with_status_zero(IrStmt::Output {
                        value: replacement,
                        newline: !already_nl,
                        target: None,
                    });
                    LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            }
        }
        // Here-string/here-doc: `cmd <<< "text"` — the text is the fd-0
        // redirect target, the command is the inner stage.
        IrStmt::Redirect { inner, redirects } => {
            // The reductions below replace the whole STATEMENT with an
            // Output (they print a value), so they only hold when the only
            // redirect is the fd-0 here-text. `cat > f <<EOF` writes a FILE:
            // printing its content instead lost the write.
            let only_here_input = redirects.iter().all(|r| {
                r.fd == Some(0) && (r.mode == "herestring" || r.mode == "heredoc")
            });
            // Find a here-string / heredoc on fd 0 → the input text (and
            // whether it is a here-string: a heredoc body is only expanded by
            // the shell, so a few folds must not treat it as literal bytes).
            let here_mode = redirects.iter().find_map(|r| {
                if r.fd == Some(0) && (r.mode == "herestring" || r.mode == "heredoc") {
                    Some(r.mode.clone())
                } else { None }
            });
            if only_here_input {
            if let Some((text_ir, here_string)) = here_mode.and_then(|m| {
                redirects.iter().find_map(|r| {
                    if r.fd == Some(0) && r.mode == m {
                        Some((r.target.clone(), m == "herestring"))
                    } else { None }
                })
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
                        if let Some(__s) = streaming_line_count(path, None) { *stmt = __s; }
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
                            if let Some(__s) = streaming_line_count(path, Some(pat)) { *stmt = __s; }
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
                    if let Some(text) = path_operand(cmd_args).and_then(arg_to_expr) {
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
        // `(( x > 3 ))` / `let "x > 3"` in CONDITION position: the zsh-sh-go
        // and posix frontends lower these to exec("let", [text]) — parse the
        // arithmetic text here so EVERY backend renders a native comparison
        // instead of a runtime let call (§Normalisation transforms).
        IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
            if let Some(replacement) = try_normalize_let_cond(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            for a in args.iter_mut() { lower_expr(a, arrays); }
        }
        // ${#var} → StrLen
        IrExpr::Call { func, args } if func == "param" => {
            if let Some(replacement) = try_lower_param_len(args) {
                *expr = replacement;
                LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                return;
            }
            // `${p##*/}` / `${p%/*}` deliberately do NOT become PathName:
            // the two are GLOB STRIPS (a trailing slash matches `*/`, so
            // `${p##*/}` of "a/b/" is empty) while PathName promises GNU
            // `basename`/`dirname` ("a/b/" → "b"). They stay `param`
            // reads, which every backend renders with its own strip path.
            // PathName is built only from REAL basename/dirname commands
            // below and in `lower_stmt`.
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
                    if let Some((replacement, _)) = try_lower_pipeline(stages, false) {
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
        // `$(echo … | cmd)` whose whole body is ONE pipeline: the node IS the
        // captured value. `$(…)` strips trailing newlines and the fold's
        // value already excludes the final one, so the subshell wrapper is
        // dropped with it. Before this, a capture in a WORD position
        // (`local y="$(…)"`, a case word, an echo operand) ran its body with
        // emit=false, the statement-level pipeline arm never fired, and the
        // whole capture stayed a `bash -c` spawn; a plain `x=$(…)` assignment
        // only folded because its capture arrives without the Arrow wrapper.
        IrExpr::Capture { expr: inner, .. } => {
            if let IrExpr::Arrow(body) = inner.as_ref() {
                if let [IrStmt::Expr(IrExpr::Call { func, args })] = body.as_slice() {
                    if func == "pipeline" {
                        if let [IrExpr::Array(stages)] = args.as_slice() {
                            if stages.len() >= 2 {
                                // `already_nl` marks a value that still ends in
                                // its OWN final newline (RepeatStr, `yes | head`);
                                // `$(…)` strips trailing newlines, so those keep
                                // the capture (and its command) rather than
                                // gaining a blank line at the consumer.
                                if let Some((replacement, already_nl)) =
                                    try_lower_pipeline(stages, false)
                                {
                                    if !already_nl {
                                        *expr = replacement;
                                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                        return;
                                    }
                                    // A value that ends in its own newline
                                    // (`yes X | head -n K` → RepeatStr) still
                                    // folds INTO a capture when the repeat is
                                    // literal: `$(…)` strips exactly one
                                    // trailing newline, so K copies of
                                    // "X\n" minus that newline is the
                                    // captured string. `$(yes Hi | head -n
                                    // 3)` is "Hi\nHi\nHi" — the same fold
                                    // the statement path already makes,
                                    // without the trailing newline it keeps.
                                    if let Some(trimmed) = capture_trim_one_newline(&replacement) {
                                        *expr = trimmed;
                                        LIFT_COUNT.fetch_add(1, Ordering::Relaxed);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            lower_expr(inner, arrays)
        }
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
                    if let Some(text) = path_operand(cmd_args).and_then(arg_to_expr) {
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
///
/// `stmt_level` marks statement position (the caller wraps the value in an
/// Output that PRINTS it): status-only commands (grep -q) must not reduce
/// there — bash prints nothing for them — so they fall back to the
/// original command.
fn try_lower_pipeline(stages: &[IrExpr], stmt_level: bool) -> Option<(IrExpr, bool)> {
    // Each stage is an Arrow function: Arrow([Stmt])
    let mut bodies: Vec<Vec<IrStmt>> = Vec::with_capacity(stages.len());
    for s in stages {
        match s {
            IrExpr::Arrow(body) => bodies.push(body.clone()),
            _ => return None,
        }
    }
    try_lower_chain(&bodies, stmt_level)
}

/// One pipeline stage's command, when the stage body is exactly a single
/// exec/builtin call (`tr X Y`, `sort`, `head -n 2`, …).
fn stage_cmd(body: &[IrStmt]) -> Option<(String, Vec<IrExpr>)> {
    match body {
        [IrStmt::Expr(IrExpr::Call { func, args })] if func == "exec" || func == "builtin" => {
            if let [IrExpr::Str(name, _), IrExpr::Array(a)] = args.as_slice() {
                Some((name.clone(), a.clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Is `cmd` an intermediate stage whose VALUE is still text (so the next
/// stage can take it as its input)? `wc` yields a COUNT and `grep -q` a
/// status — neither is a text value, so a pipeline that goes through one is
/// not chained (the original command stays, which is exact).
fn stage_keeps_text(cmd: &str) -> bool {
    matches!(cmd, "tr" | "sed" | "sort" | "uniq" | "head" | "tail" | "cut" | "xargs")
}

/// Fold a pipeline of ANY length left-to-right: stage 1 produces the text,
/// and every later stage reduces against the PREVIOUS stage's value, so
/// `echo | tr | sort | head` reaches the four nodes the two-stage rewrites
/// already build instead of shelling out for the whole pipeline.
///
/// The intermediate value EXCLUDES the final newline (the same convention
/// every two-stage reduction uses — the enclosing `Output` supplies it), and
/// each stage keeps its own `trailing_newline`/`is_literal` rule: a
/// line-structured command (sort/uniq/sed) gets the same forced-literal
/// source the two-stage path gives it, so its per-line rules are unchanged.
fn try_lower_chain(bodies: &[Vec<IrStmt>], stmt_level: bool) -> Option<(IrExpr, bool)> {
    if bodies.len() < 2 {
        return None;
    }
    if bodies.len() == 2 {
        return try_lower_pipeline_bodies(&bodies[0], &bodies[1], stmt_level);
    }
    let n = bodies.len();
    let mut src = extract_text_from_stage(&bodies[0])?;
    for (i, body) in bodies.iter().enumerate().skip(1) {
        let last = i + 1 == n;
        let (cmd_name, cmd_args) = stage_cmd(body)?;
        if !last && !stage_keeps_text(&cmd_name) {
            return None;
        }
        let line_structured = matches!(cmd_name.as_str(), "sort" | "uniq")
            || (cmd_name == "sed" && sed_is_line_structured(&cmd_args));
        let source = TextSource {
            text: src.text.clone(),
            trailing_newline: src.trailing_newline,
            is_literal: line_structured || src.is_literal,
        };
        let value = lower_text_cmd(source, &cmd_name, &cmd_args, last && stmt_level)?;
        if last {
            return Some((value, false));
        }
        src = TextSource {
            text: value,
            trailing_newline: src.trailing_newline,
            is_literal: false,
        };
    }
    None
}

/// The reductions themselves, over the two stages' STATEMENT LISTS — the
/// shared core of both pipeline shapes: the `Call { func: "pipeline" }`
/// expression (mid-program) and the structured `IrStmt::Pipeline` (what the
/// parser builds for a FINAL pipeline).
fn try_lower_pipeline_bodies(
    stage1_body: &[IrStmt],
    stage2_body: &[IrStmt],
    stmt_level: bool,
) -> Option<(IrExpr, bool)> {
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

    // Line-structured commands (sort/uniq, and sed's per-LINE substitution)
    // own their trailing-newline behaviour: they split the text into lines
    // themselves and re-join with the newline the source had. The other
    // reductions reproduce the input's final newline through
    // `Output { newline: true }`, which is why THEY need `trailing_newline`.
    let line_structured = matches!(cmd_name, "sort" | "uniq")
        || (cmd_name == "sed" && sed_is_line_structured(cmd_args));
    let source = if line_structured {
        TextSource { is_literal: true, ..source }
    } else {
        source
    };
    lower_text_cmd(source, cmd_name, cmd_args, stmt_level).map(|e| (e, false))
}

/// Does this `sed` invocation take the per-line path (no `-n`, no `p`)?
/// `-n` suppresses the default print and `p` prints explicitly, so the
/// output is no longer "the lines with each substitution applied" —
/// `substitute`'s denotation does not cover either.
fn sed_is_line_structured(args: &[IrExpr]) -> bool {
    for a in args {
        if let IrExpr::Str(s, _) = a {
            if s == "-n" || s == "--quiet" || s == "--silent" {
                return false;
            }
            if s.starts_with("s/") {
                let flags = s.rsplit('/').next().unwrap_or("");
                if flags.chars().any(|c| c == 'p') {
                    return false;
                }
            }
        }
    }
    true
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
    .filter(|_| offered("RepeatStr"))
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
    //
    // RECORDED REFUSALS — the census still lists these programs as spawning,
    // deliberately, because every alternative would change behaviour:
    //  * `cut` with a DYNAMIC source — `echo "$s" | cut -d: -f1`
    //    (examples/bench-cut.sh). `cut` splits EVERY line and extracts the
    //    field per line; FieldExtract is one split-and-index over ONE line and
    //    its spec proves only the single-line law. Rendering it for a
    //    multi-line value would return the first line's fields and drop the
    //    rest, so the dynamic shape stays refused instead of growing a
    //    per-line contract the node does not have.
    //  * `sed s///` — the Substitute node's `pattern` is a POSIX BRE (its spec
    //    is over a COMPILED matcher, and no field records literalness), so an
    //    arm cannot tell `s/=/:/` (examples/assign-in-args.sh) from
    //    `s/[0-9]+/N/g` (examples/070_gnuisms_thorough.sh) at render time.
    //    Rust has no regex engine in `std`, and an unrenderable node falls
    //    through to the generic Ext path rather than back to the original
    //    command, so Substitute stays UN-OFFERED for Rust: those programs keep
    //    the exact `sed` instead of a mis-rendered subset.
    //  * `head -c N` is a BYTE count while TakeLines is line-based (`bytes:
    //    true` returns None in the renderer). Its bytes ARE reason-able about
    //    when the source is a literal, so `try_lower_head_tail` folds the
    //    exact slice and refuses when the slice would not end in '\n' (the
    //    Output supplies that terminator): 070_gnuisms_thorough's
    //    `printf 'abcdef' | head -c 3` writes "abc" with NO terminator, so it
    //    keeps its command.
    //  * `wc -L` counts the longest line's DISPLAY WIDTH (GNU: character
    //    count under the locale), and no node models it — `try_lower_wc`
    //    lowers only -c/-m/-l/-w, so `wc -L` keeps its command.
    let single_line_literal = is_literal
        && matches!(&text, IrExpr::Str(t, _) if !t.contains('\n'));
    match cmd_name {
        "cut" if single_line_literal => try_lower_cut(text, cmd_args),
        // tr / wc / head / tail operate on the WHOLE text (char maps,
        // counts, line slices) — multi-line variable sources are safe.
        "tr" if trailing_newline => try_lower_tr(text, cmd_args),
        "head" if trailing_newline => try_lower_head_tail(text, trailing_newline, cmd_args, false),
        "tail" if trailing_newline => try_lower_head_tail(text, trailing_newline, cmd_args, true),
        "wc" => try_lower_wc(text, trailing_newline, cmd_args),
        "sed" if single_line_literal => try_lower_sed(text, cmd_args),
        "grep" if !stmt_level => try_lower_grep(text, cmd_args),
        "xargs" => try_lower_xargs(text, cmd_args, is_literal),
        // Ordering and run-collapsing are pure functions of the text and do
        // not care about the trailing newline (they split lines themselves).
        "sort" => try_lower_sort(text, cmd_args),
        "uniq" => try_lower_uniq(text, cmd_args),
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
/// - `printf FMT ARGS…` when the whole invocation FOLDS to its bytes at IR
///   time (`const_capture_fold::const_printf_output`: all-literal argv, no
///   `-v`, no width/precision/flags, no `%c`/float, no format cycling). The
///   escapes and conversions are already applied, so the consumer node sees
///   exactly what bash wrote and the format's own trailing newline is
///   recorded in `trailing_newline` — this is the guard the old blanket
///   `printf` refusal stood in for (treating the ARGS as the output would
///   mis-count `printf x | wc -l`, which is 0: no newline). Every `printf`
///   whose bytes are NOT known at IR time is still refused.
///
/// echo args may be literals OR variable reads (Var / param() / an
/// interpolation) — `echo "$s" | cut` reduces with the read as the text.
fn extract_text_from_stage(stmts: &[IrStmt]) -> Option<TextSource> {
    // `printf FMT ARGS…` folded to its exact bytes (see this function's
    // doc): a producer whose output is KNOWN, so the consumer's node
    // reduces over what bash wrote. Sits ahead of the `echo` arm because
    // `printf` is an exec call with the same shape but different output.
    if let [IrStmt::Expr(e)] = stmts {
        if let Some(raw) = crate::transforms::const_capture_fold::const_printf_output(e) {
            let (text, trailing_newline) = match raw.strip_suffix('\n') {
                Some(body) => (body.to_string(), true),
                None => (raw, false),
            };
            return Some(TextSource {
                text: IrExpr::Str(text, StrStyle::DoubleQuoted),
                trailing_newline,
                is_literal: true,
            });
        }
    }
    match stmts {
        // echo ARGS → join args (with " ") as the text
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec" || func == "builtin" =>
        {
            if let [IrExpr::Str(name, _), IrExpr::Array(echo_args)] = args.as_slice() {
                if name == "echo" {
                    let mut trailing_newline = true;
                    let mut escapes = false;
                    let mut rest: &[IrExpr] = echo_args.as_slice();
                    // Leading flags: -n (no trailing newline), -E (default
                    // escape behavior — a no-op), -e (INTERPRET escapes). The
                    // last of -e/-E wins, as in bash.
                    while let Some(IrExpr::Str(flag, _)) = rest.first() {
                        match flag.as_str() {
                            "-n" => { trailing_newline = false; rest = &rest[1..]; }
                            "-E" => { escapes = false; rest = &rest[1..]; }
                            "-e" => { escapes = true; rest = &rest[1..]; }
                            _ => break,
                        }
                    }
                    let mut text = join_echo_args(rest)?;
                    let mut is_literal = matches!(text, IrExpr::Str(..));
                    if escapes {
                        // `echo -e` writes the DECODED bytes, so the escape
                        // interpretation must happen here or the consumer
                        // would see backslashes bash never wrote. Only an
                        // all-literal argument list has known bytes; a
                        // variable's content cannot be decoded at IR time.
                        let raw = literal_text(&text)?;
                        let decoded = decode_echo_escapes(&raw)?;
                        text = IrExpr::Str(decoded, StrStyle::DoubleQuoted);
                        is_literal = true;
                    }
                    return Some(TextSource { text, trailing_newline, is_literal });
                }
                // `seq FIRST LAST [STEP]` (or `seq LAST`) with INTEGER
                // literal operands: the exact lines it writes are known, so a
                // downstream stage (`| head -3`, `| wc -l`, `| tr …`) reduces
                // over them instead of spawning the producer. An empty range
                // (`seq 1 0`) writes NOTHING — no trailing newline — which is
                // exactly what `wc -l` must see.
                if name == "seq" {
                    return seq_text(echo_args);
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

/// `seq` with integer literal operands → the exact text it writes.
///
/// `seq LAST`, `seq FIRST LAST`, `seq FIRST STEP LAST` (GNU's positional
/// forms; a STEP of 0 is an error, and a reversed range prints nothing).
/// Refuses anything else — a non-integer operand (`seq 0.5 2` prints 0.5, 1.5
/// with `%g` formatting this does not reproduce), a flag (`-s`, `-f`, `-w`,
/// `--equal-width`), or a range longer than the constant budget.
fn seq_text(args: &[IrExpr]) -> Option<TextSource> {
    let mut nums: Vec<i64> = Vec::with_capacity(3);
    for a in args {
        let s = literal_text(a)?;
        nums.push(s.trim().parse::<i64>().ok()?);
    }
    let (first, step, last) = match nums.as_slice() {
        [last] => (1, 1, *last),
        [first, last] => (*first, 1, *last),
        [first, step, last] => (*first, *step, *last),
        _ => return None,
    };
    if step == 0 {
        return None;
    }
    const MAX_LINES: i64 = 4096;
    let mut lines: Vec<String> = Vec::new();
    let mut v = first;
    while (step > 0 && v <= last) || (step < 0 && v >= last) {
        if lines.len() as i64 >= MAX_LINES {
            return None;
        }
        lines.push(v.to_string());
        v = v.checked_add(step)?;
    }
    // An EMPTY range writes no bytes at all — not even a newline.
    let trailing_newline = !lines.is_empty();
    Some(TextSource {
        text: IrExpr::Str(lines.join("\n"), StrStyle::DoubleQuoted),
        trailing_newline,
        is_literal: true,
    })
}

/// bash's `echo -e` escape interpretation, over an all-literal argument list.
///
/// Handles the escapes that produce fixed bytes: `\\a \\b \\e \\E \\f \\n
/// \\r \\t \\v \\\\`, octal `\\0nnn` (and `\\nnn`) and `\\xHH`. An UNKNOWN
/// escape is passed through as a backslash and the character, which is what
/// bash does (`echo -e '\\q'` prints `\q`).
///
/// REFUSED: `\\c` (it also SUPPRESSES the rest of the output and the trailing
/// newline — a control-flow effect, not a byte), `\\uXXXX`/`\\UXXXXXXXX`
/// (locale-dependent encoding), and NUL (`\\0` with no digits) which a Rust
/// `String` cannot hold.
fn decode_echo_escapes(s: &str) -> Option<String> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] != '\\' {
            out.push(cs[i]);
            i += 1;
            continue;
        }
        let c = *cs.get(i + 1)?;
        i += 2;
        match c {
            'a' => out.push('\u{7}'),
            'b' => out.push('\u{8}'),
            'e' | 'E' => out.push('\u{1b}'),
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\u{b}'),
            '\\' => out.push('\\'),
            '0'..='7' => {
                // Up to three octal digits, including this one.
                let mut val: u32 = c.to_digit(8)?;
                let mut n = 0;
                while n < 2 {
                    match cs.get(i).and_then(|d| d.to_digit(8)) {
                        Some(d) => {
                            val = val * 8 + d;
                            i += 1;
                            n += 1;
                        }
                        None => break,
                    }
                }
                if val == 0 {
                    // A bare `\0` is a NUL byte: not representable.
                    return None;
                }
                out.push(char::from_u32(val)?);
            }
            'x' => {
                let mut val: u32 = 0;
                let mut n = 0;
                while n < 2 {
                    match cs.get(i).and_then(|d| d.to_digit(16)) {
                        Some(d) => {
                            val = val * 16 + d;
                            i += 1;
                            n += 1;
                        }
                        None => break,
                    }
                }
                if n == 0 {
                    // `\\x` with no hex digit is a literal backslash-x.
                    out.push_str("\\x");
                } else {
                    // A NUL byte cannot live in a Rust `String`, and a shell
                    // variable cannot hold one either — refuse the shape.
                    if val == 0 {
                        return None;
                    }
                    out.push(char::from_u32(val)?);
                }
            }
            'c' | 'u' | 'U' => return None,
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    Some(out)
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
    // Refusals, kept where the ORIGINAL command stays exact:
    //  * `-c`/`-b` select CHARACTER/BYTE positions — a different node
    //    (SubStrExtract) and a different unit than a field (
    //    `cut -c` counts characters, so on multibyte input it is not a byte
    //    slice either);
    //  * `--complement` is modelled in the node's theory but the node carries
    //    no complement field, so it cannot be rendered faithfully.
    if args_str.iter().any(|a| *a == "-c" || *a == "-b" || a.starts_with("--")) {
        return None;
    }

    // Parse field spec: "1", "1,3", "1-3", "2-", "-3". The field COUNT is
    // known whenever the text is a literal (the only shape that reaches here),
    // which is what makes an open range (`-f2-`) and the empty-selection
    // refusal decidable.
    let field_count = literal_text(&text).map(|t| t.split(delimiter.as_str()).count());
    let fields = parse_field_spec(fields_str, field_count)?;
    // No field to print means cut prints NOTHING for this line — not a blank
    // line — and the Output wrapping the node always writes a newline.
    if fields.is_empty() {
        return None;
    }

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

/// `cut -f LIST` → the ascending, de-duplicated list of 1-based indices cut
/// selects (GNU: the output follows the INPUT order, so `-f3,1` and `-f1,3`
/// agree), clamped to the fields the line actually has.
///
/// `count` is the line's field count when the text is a literal. It turns an
/// open range (`-f2-`) into its members and drops indices past the end; both
/// are what GNU cut does (`cut -d: -f1-5` on `a:b` prints `a:b`).
///
/// Refuses (None) — the caller keeps the command — on anything whose meaning
/// the node does not cover: a non-numeric part, field 0 (`cut -f0` is an
/// error), or a reversed range (`-f4-2` is an error, not an empty set).
/// A repeat node whose value is a KNOWN string of repetitions → the same
/// value with ONE trailing newline removed (what `$(…)` does). None for any
/// other node (an unknown count, a non-literal unit, a unit not ending in
/// "\n").
fn capture_trim_one_newline(e: &IrExpr) -> Option<IrExpr> {
    let IrExpr::Ext(n) = e else { return None };
    if n.tag() != "RepeatStr" {
        return None;
    }
    let r = n.as_any().downcast_ref::<crate::shir_nodes::RepeatStr>()?;
    let unit = literal_text(&r.text)?;
    let body = unit.strip_suffix('\n')?;
    let k: i64 = match &r.count {
        IrExpr::Int(k) => *k,
        _ => return None,
    };
    if k < 0 {
        return None;
    }
    let mut out = String::new();
    for i in 0..k {
        if i + 1 < k {
            out.push_str(&unit);
        } else {
            out.push_str(body);
        }
    }
    Some(IrExpr::Str(out, StrStyle::DoubleQuoted))
}

fn parse_field_spec(spec: &str, count: Option<usize>) -> Option<Vec<FieldRange>> {
    let mut idx: Vec<u32> = Vec::new();
    for part in spec.split(',') {
        if part.is_empty() {
            return None;
        }
        let (start, end) = match part.split_once('-') {
            Some((a, b)) => {
                let start: u32 = if a.is_empty() { 1 } else { a.parse().ok()? };
                let end: u32 = if b.is_empty() {
                    match count {
                        Some(c) => c as u32,
                        // An open range over an unknown length is not a
                        // finite index list — leave it to the runtime.
                        None => return None,
                    }
                } else {
                    b.parse().ok()?
                };
                // `cut -f5-` over a 3-field line is VALID and selects
                // nothing (unlike the reversed `-f4-2`, which is an error).
                if b.is_empty() && start > end {
                    continue;
                }
                (start, end)
            }
            None => {
                let n: u32 = part.parse().ok()?;
                (n, n)
            }
        };
        if start == 0 || end == 0 || end < start {
            return None;
        }
        for n in start..=end {
            if !idx.contains(&n) {
                idx.push(n);
            }
        }
    }
    idx.sort_unstable();
    if let Some(c) = count {
        idx.retain(|n| (*n as usize) <= c);
    }
    Some(idx.into_iter().map(FieldRange::Single).collect())
}

// ── tr ───────────────────────────────────────────────────────────────

/// The ASCII members of a `tr` POSIX class (see `expand_tr_set`). `tr` classes are BYTE sets: glibc's
/// `tr` does not expand them multibyte, so `[:upper:]` is exactly A-Z even in a
/// UTF-8 locale (measured: `tr '[:lower:]' '[:upper:]'` leaves É untouched).
/// A class this table does not carry — `[:space:]` (its members include the
/// newline the fold cannot represent), `[:alpha:]`, `[:punct:]`, … — stays
/// refused rather than being approximated.
fn tr_class_members(s: &str) -> Option<&'static str> {
    match s {
        "[:lower:]" => Some("abcdefghijklmnopqrstuvwxyz"),
        "[:upper:]" => Some("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
        "[:digit:]" => Some("0123456789"),
        _ => None,
    }
}

/// Expand `tr`'s RANGE syntax into the literal member set the node's
/// `from`/`to` fields promise (`tr_expand` in the node's theory). `a-z` is
/// 26 members, `z-a` is the same range DESCENDING (GNU accepts it), and a
/// dash with no character on either side is a literal dash (`-abc`, `abc-`,
/// `a-c-e`). The C escapes this expansion owns are `\n \t \r \a \b \f \v`
/// and `\\`; an escape it does not own (`\012` octal, `\xHH`, `[=c=]`) or a
/// non-ASCII range endpoint refuses (None), so the original command is kept
/// (exact everywhere) rather than guessed.
fn expand_tr_set(s: &str) -> Option<String> {
    if s.contains("[:") {
        // Only a WHOLE-SET class is modelled; a class mixed into a larger set
        // (`a[:digit:]`) is still refused.
        return tr_class_members(s).map(|m| m.to_string());
    }
    let decoded = decode_tr_escapes(s)?;
    // A RANGE endpoint that arrived as an escape (`tr 'a-\n' x`) is not a
    // member-set question any more — which side of the range the escape
    // denotes, and whether a reversed range is even accepted, is the
    // runtime's call. Dash-free sets (the `\n` translation that matters)
    // still expand, so only that ambiguity stays refused.
    if s.contains('\\') && decoded.contains('-') {
        return None;
    }
    let s = decoded.as_str();
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if i + 2 < cs.len() && cs[i + 1] == '-' {
            let (a, b) = (cs[i], cs[i + 2]);
            if !(a.is_ascii() && b.is_ascii()) {
                return None;
            }
            let (lo, hi) = (a as u32, b as u32);
            if lo <= hi {
                for c in lo..=hi {
                    out.push(char::from_u32(c)?);
                }
            } else {
                let mut c = lo;
                loop {
                    out.push(char::from_u32(c)?);
                    if c == hi {
                        break;
                    }
                    c -= 1;
                }
            }
            i += 3;
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Decode the ESCAPE sequences POSIX `tr` defines for a set: `\n \t \r \a
/// \b \f \v \\`. Anything else (octal `\101`, `\0`, a trailing `\`) REFUSES:
/// the node's `from`/`to` are MEMBER CHARACTERS, so an escape this function
/// cannot decode exactly must keep the runtime path instead of guessing.
fn decode_tr_escapes(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match it.next()? {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'a' => '\u{7}',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'v' => '\u{b}',
            '\\' => '\\',
            _ => return None,
        });
    }
    Some(out)
}

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

    // CASE PAIRS ARE NOT `CaseTransform`. `tr` is a BYTE map, always: measured
    // against this box, `printf 'Éé Aa' | tr '[:lower:]' '[:upper:]'` leaves
    // both É and é untouched (byte map, in a UTF-8 locale and in LC_ALL=C),
    // while Rust's `to_uppercase()` maps the CODE POINT ("ÉÉ AA"). So `tr`
    // case pairs — the `a-z`/`A-Z` ranges and the `[:lower:]`/`[:upper:]`
    // classes alike — lower to the general CharTranslate, whose model IS the
    // per-byte map (`tr_expand` + `map_of`, TextOps#char_translate_sound), and
    // whose Rust arm spells `chars().map(..)`: for an ASCII from/to set every
    // non-ASCII char is a fixed point, exactly as `tr` behaves.
    // CaseTransform stays for the BASH PARAMETER ops `${v^^}`/`${v,,}`, which
    // DO case-map code points — that is what the node's renderer (and bash)
    // do, and why the two sources must not share a node.

    // The node's `from`/`to` are the EXPANDED member sets (ranges and
    // classes are the lowering's job, not the renderer's).
    let from = expand_tr_set(from)?;
    let to = if to.is_empty() { String::new() } else { expand_tr_set(to)? };

    // A from-set that contains the NEWLINE is a newline-MAPPING `tr`, and the
    // pipeline fold hands a stage its input WITHOUT the final newline (the
    // enclosing `Output` supplies exactly one). That convention is exact only
    // when the map leaves a newline a newline: `tr '\n' ' '` turns it into a
    // SPACE that the enclosing newline does not replace — measured: bash gives
    // `seq: 1 2 3 ` (trailing space, `$(\342\200\246)` strips newlines only)
    // while the dropped newline gives `seq: 1 2 3`. So a map to some other
    // char, and `-d`/`-s` (which change whether a newline survives at all),
    // keep the command — the case is rare and the command is exact.
    if from.contains('\n') {
        let nl_is_fixed_point = !delete && !squeeze
            && from.chars().position(|c| c == '\n')
                .and_then(|at| to.chars().nth(at)) == Some('\n');
        if !nl_is_fixed_point {
            return None;
        }
    }
    Some(IrExpr::Ext(Box::new(CharTranslate {
        text,
        from,
        to,
        delete,
        squeeze,
    })))
}

// ── head / tail ──────────────────────────────────────────────────────

fn try_lower_head_tail(
    text: IrExpr,
    source_trailing_newline: bool,
    args: &[IrExpr],
    from_end: bool,
) -> Option<IrExpr> {
    let args_str: Vec<&str> = args.iter().filter_map(|a| {
        if let IrExpr::Str(s, _) = a { Some(s.as_str()) } else { None }
    }).collect();
    // Every argv word must be a LITERAL: a dynamic count (`head -n "$max"`)
    // used to vanish from `args_str` and leave the default 10 in place, which
    // folds to a fixed take of 10 lines — a silently wrong program.
    if args_str.len() != args.len() {
        return None;
    }

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
            // A BARE word here is an OPERAND (a file), never a count: `-n N`
            // consumed its value above. `head -1 a b` is the multi-file header
            // form — and the old code took its LAST word as the count, so it
            // rendered `1` where bash prints a header per file plus errors.
            // The operand forms keep their command.
            return None;
        }
        i += 1;
    }
    // A count the node cannot express keeps the command. `TakeLines` counts
    // lines from one end; `tail -n +2` counts from the START and `head -n -2`
    // means all but the last two, so neither is that node. The check is
    // PLAIN DIGITS on purpose: `str::parse::<i64>` accepts a sign, so `+2`
    // parsed to 2 and folded `tail -n +2` to "last 2 lines" — right for a
    // 3-line input and wrong for a 5-line one.
    if count_str.is_empty() || !count_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    // `head -c` / `tail -c` take BYTES; TakeLines slices LINES, so a byte
    // count cannot ride the node (`bytes: true` has no render arm on any
    // backend). A LITERAL source's bytes are known, though, so the slice is a
    // constant — but only when the result can be spelled as a statement
    // VALUE: `head -c`/`tail -c` print raw bytes with no terminator, while the
    // Output wrapping the fold always appends '\n'. So the slice must end in
    // '\n' (strip it; the Output restores it) and must land on a UTF-8
    // boundary (the value is a `String`). `printf 'abcdef' | head -c 3`
    // therefore keeps its command: bash writes "abc" with no newline.
    if bytes {
        let n: i64 = count_str.parse().ok()?;
        if n <= 0 {
            return None;
        }
        let mut full = literal_text(&text)?;
        if source_trailing_newline {
            full.push('\n');
        }
        let all = full.as_bytes();
        let n = n as usize;
        let slice = if from_end {
            &all[all.len().saturating_sub(n)..]
        } else {
            &all[..n.min(all.len())]
        };
        let slice = std::str::from_utf8(slice).ok()?;
        let body = slice.strip_suffix('\n')?;
        return Some(IrExpr::Str(body.to_string(), StrStyle::DoubleQuoted));
    }

    let count = count_str.parse::<i64>().ok().map(|n| IrExpr::Int(n))
        .unwrap_or_else(|| IrExpr::Str(count_str.to_string(), StrStyle::DoubleQuoted));

    Some(IrExpr::Ext(Box::new(TakeLines {
        text: text,
        count: count,
        from_end,
        bytes,
    })))
    .filter(|_| offered("TakeLines"))
}

// ── wc ───────────────────────────────────────────────────────────────

fn try_lower_wc(text: IrExpr, trailing_newline: bool, args: &[IrExpr]) -> Option<IrExpr> {
    // Lower `wc` to a PRIMITIVE count node so each renderer implements it
    // once, trivially — no per-mode branching in the renderers:
    //   wc -c / wc -m  → StrLen (text.length)
    //   wc -l          → LineCount (split('\n').length)
    //   wc -w          → WordCount (split(/\s+/).length)
    // An OPERAND (`wc -l f`) makes wc read that FILE, not the text this lift
    // hands the node — and the old flag filter silently DROPPED it, folding a
    // per-file count to a count of the pipe (a silently wrong program). Every
    // argv word must be a literal FLAG; any other word, literal or dynamic,
    // keeps the command.
    for a in args {
        let word = match a {
            IrExpr::Str(s, _) => Some(s.as_str()),
            IrExpr::Interpolate(parts) if parts.len() == 1 => match &parts[0] {
                InterpPart::Lit(s) => Some(s.as_str()),
                _ => None,
            },
            _ => None,
        };
        match word {
            Some(w) if w.starts_with('-') => {}
            _ => return None,
        }
    }

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
        Some(IrExpr::Ext(Box::new(StrLen { text }))).filter(|_| offered("StrLen"))
    } else {
        // wc -l → RegCount(text, /\n/) (newline count, NOT split-length:
        // off by one on the trailing newline). wc -w → WordCount(text):
        // a dedicated node — C renders an inline transition-count scan
        // (no allocation), JS/Perl the split-length idiom.
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
            .filter(|_| offered("RegCount"))
        } else {
            // wc -w → WordCount(text) — a dedicated node: C renders an
            // inline transition-count scan (no allocation), JS/Perl the
            // split-length idiom (docs/shir-reductions.md, source-aware
            // literal-string row of the wc table)
            Some(IrExpr::Ext(Box::new(WordCount { text })))
                .filter(|_| offered("WordCount"))
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
                let flags = parts.get(2).copied().unwrap_or("");
                // Only the VALUE-PRESERVING flags are accepted. POSIX `sed`
                // also allows `p` (print, which changes what the command
                // prints), `w file` and `N`, and a leading number (`s///2`)
                // selects the Nth match. Each of those makes the command's
                // output differ from "every line with its substitution", so
                // the node's denotation does not cover them and the lowering
                // REFUSES rather than dropping the flag on the floor.
                if flags.chars().any(|c| !matches!(c, 'g' | 'i')) {
                    return None;
                }
                // (`s/x/y/2` is refused below with the digit check.)
                // A leading digit after the flags selects the Nth match
                // (`s/x/y/2`) — a different function, and one the node does
                // not model. Caught here because `flags` is the whole tail
                // (`"g2"`, `"2"`, …), not just a char set.
                if flags.chars().any(|c| c.is_ascii_digit()) {
                    return None;
                }
                return Some(IrExpr::Ext(Box::new(Substitute {
                    text,
                    pattern,
                    replacement,
                    global: flags.contains('g'),
                    ignore_case: flags.contains('i'),
                })));
            }
        }
    }
    None
}

// ── sort / uniq → SortLines / UniqLines ─────────────────────────────

/// `| sort [flags]` → `SortLines`.
///
/// The corpus pipes `sort` in 20 of 562 examples (`sort`, `sort -n`,
/// `sort -nr`, `sort -u`, `sort -t: -k2 -n`) and there was no node for it:
/// every backend shelled out, so the shell-out WAS the meaning. Ordering is
/// a pure function of the text, so it belongs in the shared layer.
///
/// Accepts exactly the flags whose meaning the node covers:
///   -n / -g   numeric key comparison (`-g` is `-n` without the leading-blank
///             rule; the corpus never distinguishes them and the node's
///             `numeric` flag does not either — a `-g` sort that would
///             differ is a locale/number-syntax question, so it is REFUSED)
///   -r        reverse             -u  de-duplicate (`sort -u`)
///   -f        case-insensitive    -t X  field separator   -kN  sort by field N
/// Anything else (`-k1,2`, `-V`, `-h`, `-M`, `-R`, `-o`, `-m`, `--parallel`)
/// REFUSES: each is a different ordering, and guessing one would silently
/// reorder a program's output.
fn try_lower_sort(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    // Only flags, and only the accepted ones; every operand must be a
    // literal (a non-literal `-k "$spec"` cannot be checked here).
    let mut strs: Vec<String> = Vec::new();
    for a in args {
        match a {
            IrExpr::Str(s, _) => strs.push(s.clone()),
            _ => return None,
        }
    }
    let mut numeric = false;
    let mut reverse = false;
    let mut unique = false;
    let mut ignore_case = false;
    let mut sep: Option<String> = None;
    let mut field: Option<u32> = None;
    let mut i = 0;
    while i < strs.len() {
        let s = strs[i].as_str();
        match s {
            "-n" => numeric = true,
            "-r" => reverse = true,
            "-u" => unique = true,
            "-f" => ignore_case = true,
            "-t" if i + 1 < strs.len() => {
                // POSIX: the separator is the FIRST character of the operand.
                let sepstr = &strs[i + 1];
                if sepstr.chars().count() != 1 {
                    return None;
                }
                sep = Some(sepstr.clone());
                i += 1;
            }
            _ if s.starts_with("-k") && s.len() > 2 => {
                // Only a bare field number. `-k1,2` / `-k1.2n` are ranges and
                // per-key options the node does not model.
                if s[2..].contains(|c: char| !c.is_ascii_digit()) {
                    return None;
                }
                field = s[2..].parse::<u32>().ok();
                if field == Some(0) {
                    return None; // sort fields are 1-based
                }
            }
            // Combined single-dash flags (`-nr`) — every char must be known.
            _ if s.starts_with('-') && s.len() > 1 => {
                for c in s[1..].chars() {
                    match c {
                        'n' => numeric = true,
                        'r' => reverse = true,
                        'u' => unique = true,
                        'f' => ignore_case = true,
                        _ => return None,
                    }
                }
            }
            _ => return None, // a file operand: not a pure text transform
        }
        i += 1;
    }
    // A field key without a separator sorts by the default (blank-run)
    // separator, which is a different, whitespace-collapsing rule — refuse.
    let key = match (field, sep) {
        (None, _) => String::new(), // whole line
        (Some(n), Some(sep)) => format!("{sep}{n}"),
        (Some(_), None) => return None,
    };
    Some(IrExpr::Ext(Box::new(SortLines {
        text,
        key,
        numeric,
        reverse,
        unique,
        ignore_case,
    })))
}

/// `| uniq [flags]` → `UniqLines`.
///
/// `-c` (prefix the run length), `-s N` (skip N chars when comparing).
/// `-d` / `-u` (only duplicated / only unique lines) are FILTERS whose
/// output depends on the run lengths of the WHOLE input; they need a
/// different node, so they refuse here rather than being approximated.
fn try_lower_uniq(text: IrExpr, args: &[IrExpr]) -> Option<IrExpr> {
    let mut count = false;
    let mut skip: i64 = 0;
    let mut i = 0;
    while i < args.len() {
        let IrExpr::Str(s, _) = &args[i] else { return None };
        match s.as_str() {
            "-c" => count = true,
            "-s" => {
                let IrExpr::Str(n, _) = args.get(i + 1)? else { return None };
                skip = n.parse::<i64>().ok()?;
                if skip < 0 {
                    return None;
                }
                i += 1;
            }
            _ if s.starts_with('-') && s.len() > 2 => {
                // a combined form such as `-cs3` is not accepted: parsing it
                // reliably needs getopt, and a mis-parse re-orders a program.
                return None;
            }
            _ => return None, // a file operand
        }
        i += 1;
    }
    Some(IrExpr::Ext(Box::new(UniqLines { text, skip, count })))
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
    .filter(|_| offered("StringTrim"))
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
                return Some(IrExpr::Ext(Box::new(StrLen { text: var })))
                    .filter(|_| offered("StrLen"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // The spec is now expanded to its FIELD INDICES (a range is the members the
    // line has, in input order) — the shape the renderers emit — instead of
    // keeping `Range` nodes. Same selection, one representation.
    #[test]
    fn parse_simple_field_spec() {
        let fields = parse_field_spec("2", None).unwrap();
        assert_eq!(fields, vec![FieldRange::Single(2)]);
    }

    #[test]
    fn parse_multi_field_spec() {
        let fields = parse_field_spec("1,3", None).unwrap();
        assert_eq!(fields, vec![FieldRange::Single(1), FieldRange::Single(3)]);
    }

    #[test]
    fn parse_range_field_spec() {
        let fields = parse_field_spec("1-3", None).unwrap();
        assert_eq!(fields, vec![
            FieldRange::Single(1),
            FieldRange::Single(2),
            FieldRange::Single(3),
        ]);
    }

    #[test]
    fn parse_mixed_field_spec() {
        let fields = parse_field_spec("1-3,5", None).unwrap();
        assert_eq!(fields, vec![
            FieldRange::Single(1),
            FieldRange::Single(2),
            FieldRange::Single(3),
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
        assert!(
            n.as_any().downcast_ref::<WordCount>().is_some(),
            "wc -w → WordCount (C renders an inline scan, no allocation)"
        );
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

    /// A program's FINAL pipeline is the structured `IrStmt::Pipeline`, not the
    /// `Call{func:"pipeline"}` the reductions knew: `echo abc | tail -n 1` as
    /// the last line never reduced and kept spawning bash, while the same
    /// pipeline mid-program did. Both shapes must reach the same node.
    #[test]
    fn final_pipeline_statement_reduces_like_a_mid_program_pipeline() {
        crate::transforms::set_target(Some("rs"));
        let commands = crate::parser::commands::parse_commands_from_text("echo abc | tail -n 1")
            .expect("parse");
        let mut prog = crate::shir::ast_to_ir_raw(&commands);
        // `shir-pipeline-native` is the pass that turns a statement-position
        // pipeline Call into the structured statement form (it precedes
        // text-ops in the registry).
        crate::transforms::shir_pipeline_native::transform(&mut prog.stmts);
        assert!(
            matches!(prog.stmts.as_slice(), [IrStmt::Pipeline { .. }]),
            "expected the structured Pipeline statement, got {:?}",
            prog.stmts
        );
        assert!(transform(&mut prog.stmts), "the final pipeline did not reduce");
        let dbg = format!("{:?}", prog.stmts);
        assert!(dbg.contains("TakeLines"), "no TakeLines node: {dbg}");
        assert!(!dbg.contains("Pipeline"), "the pipeline survived: {dbg}");
        crate::transforms::set_target(None);
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
    // `tr`'s case pairs are a BYTE map, not `CaseTransform`: measured on this
    // box, `printf 'Éé Aa' | tr '[:lower:]' '[:upper:]'` leaves É and é alone
    // (and Rust's `to_uppercase()` gives "ÉÉ AA") — see the comment in
    // `try_lower_tr`. Both sources lower to the general CharTranslate.
    #[test]
    fn tr_range_upper_is_a_byte_map() {
        let r = try_lower_tr(st("hi"), &[st("a-z"), st("A-Z")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<CaseTransform>().is_none(), "tr is not a case node");
        let x = n.as_any().downcast_ref::<CharTranslate>().unwrap();
        assert_eq!(x.from, "abcdefghijklmnopqrstuvwxyz");
        assert_eq!(x.to, "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    }

    #[test]
    fn tr_posix_class_expands_to_ascii_members() {
        let r = try_lower_tr(st("hi"), &[st("[:lower:]"), st("[:upper:]")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let x = n.as_any().downcast_ref::<CharTranslate>().unwrap();
        assert_eq!(x.from, "abcdefghijklmnopqrstuvwxyz");
        assert_eq!(x.to, "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    }

    #[test]
    fn tr_digit_class_is_a_byte_map() {
        let r = try_lower_tr(st("a1b"), &[st("[:digit:]"), st("x")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let x = n.as_any().downcast_ref::<CharTranslate>().unwrap();
        assert_eq!(x.from, "0123456789");
        assert_eq!(x.to, "x");
    }

    #[test]
    fn tr_unmodelled_posix_class_not_reduced() {
        // [:alpha:] / [:space:] are locale sets this table does not own (and
        // [:space:] contains the newline the fold cannot carry) — the command
        // stays.
        assert!(try_lower_tr(st("hi"), &[st("[:alpha:]"), st("x")]).is_none());
        assert!(try_lower_tr(st("hi"), &[st("[:space:]"), st("_x")]).is_none());
        // A class MIXED into a larger set is not modelled either.
        assert!(try_lower_tr(st("hi"), &[st("a[:digit:]"), st("x")]).is_none());
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
    fn tr_ranges_expand_before_the_node_is_built() {
        // The node promises the EXPANDED sets: `tr 'a-c' 'x'` is the map
        // a→x, b→x, c→x (SET2's last member repeats), not three literal
        // characters `a`, `-`, `c`.
        let r = try_lower_tr(st("abc"), &[st("a-c"), st("x")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let ct = n.as_any().downcast_ref::<CharTranslate>().unwrap();
        assert_eq!(ct.from, "abc");
        assert_eq!(ct.to, "x");
        // A descending range is the same member set, reversed.
        let r = try_lower_tr(st("abc"), &[st("c-a"), st("x")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert_eq!(n.as_any().downcast_ref::<CharTranslate>().unwrap().from, "cba");
        // A leading/trailing dash is literal, not an incomplete range.
        let r = try_lower_tr(st("abc"), &[st("a-c-e"), st("x")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert_eq!(n.as_any().downcast_ref::<CharTranslate>().unwrap().from, "abc-e");
        // An escape means the runtime path (`\n`, `\012` are not ours).
        assert!(try_lower_tr(st("a"), &[st("a-\\n"), st("x")]).is_none());
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
        // `RegSub` was replaced by the `Substitute` node (see
        // shir_nodes/substitute.node): one node, one domain — the lines of
        // a text — instead of two booleans over a string and a line list.
        let r = try_lower_sed(st("aaa"), &[st("s/a/b/g")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let rs = n.as_any().downcast_ref::<Substitute>().unwrap();
        assert_eq!(rs.pattern, "a");
        assert_eq!(rs.replacement, "b");
        assert!(rs.global);
        let r = try_lower_sed(st("aaa"), &[st("s/a/b/")]).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(!n.as_any().downcast_ref::<Substitute>().unwrap().global);
    }

    // ── head/tail compositions ───────────────────────────────────────

    #[test]
    fn head_n_is_take_lines() {
        let r = try_lower_head_tail(st("a\nb\nc"), true, &[st("-n"), st("2")], false).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        let tl = n.as_any().downcast_ref::<TakeLines>().unwrap();
        assert_eq!(tl.count, IrExpr::Int(2));
        assert!(!tl.from_end && !tl.bytes);
    }

    #[test]
    fn tail_n_is_take_lines_from_end() {
        let r = try_lower_head_tail(st("a\nb"), true, &[st("-n"), st("1")], true).unwrap();
        let IrExpr::Ext(n) = &r else { panic!() };
        assert!(n.as_any().downcast_ref::<TakeLines>().unwrap().from_end);
    }

    #[test]
    fn head_bytes_fold_only_when_the_slice_ends_in_a_newline() {
        // `head -c`/`tail -c` write RAW bytes with no terminator, and the
        // statement wrapper always appends '\n' — so only a slice that ends in
        // '\n' (the wrapper restores it) can fold. A literal source's bytes are
        // known, which is what makes the decision decidable.
        let s = |v: &IrExpr| match v {
            IrExpr::Str(t, _) => Some(t.clone()),
            _ => None,
        };
        // Source "abc\ndef" + newline: the first 4 bytes are "abc\n".
        let r = try_lower_head_tail(st("abc\ndef"), true, &[st("-c"), st("4")], false).unwrap();
        assert_eq!(s(&r), Some("abc".to_string()));
        // The last 2 bytes are "f\n".
        let r = try_lower_head_tail(st("abc\ndef"), true, &[st("-c2")], true).unwrap();
        assert_eq!(s(&r), Some("f".to_string()));
        // `printf 'abcdef' | head -c 3` writes "abc" with NO terminator:
        // refused (070_gnuisms keeps the command).
        assert!(try_lower_head_tail(st("abcdef"), true, &[st("-c"), st("3")], false).is_none());
        // The same, from a source that has no trailing newline at all.
        assert!(try_lower_head_tail(st("abcdef"), false, &[st("-c3")], true).is_none());
        // `-c 0` prints nothing; the empty slice has no '\n' to restore.
        assert!(try_lower_head_tail(st("abc"), true, &[st("-c"), st("0")], false).is_none());
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
        assert!(n.as_any().downcast_ref::<CharTranslate>().is_some());
    }

    #[test]
    fn tr_newline_map_to_other_char_keeps_the_command() {
        // The pipeline fold drops the source's final newline (the enclosing
        // Output supplies one). `tr '\n' ' '` turns that newline into a SPACE
        // which nothing replaces: bash `seq 1 3 | tr '\n' ' '` ends in a
        // space, a folded CharTranslate would not. Keep the command.
        assert!(try_lower_tr(st("hi"), &[st("\\n"), st(" ")]).is_none());
        // `-d`/`-s` change whether a newline survives at all — same reason.
        assert!(try_lower_tr(st("hi"), &[st("-d"), st("\\n")]).is_none());
        assert!(try_lower_tr(st("hi"), &[st("-s"), st("\\n"), st(" ")]).is_none());
        // Newline to newline IS a fixed point: the node is exact there.
        let r = try_lower_tr(st("hi"), &[st("\\n"), st("\\n")]).unwrap();
        assert!(matches!(&r, IrExpr::Ext(n) if n.as_any().downcast_ref::<CharTranslate>().is_some()));
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
        //
        // `sort` was the first example here; it now REDUCES (SortLines,
        // see try_lower_sort), so it is asserted as a positive case — the
        // rule itself is still pinned by the commands that stay refused.
        assert!(lower_text_cmd(lit_src("x"), "awk", &[st("{print}")], true).is_none());
        assert!(lower_text_cmd(lit_src("x"), "frobnicate", &[], true).is_none());
        assert!(lower_text_cmd(lit_src("x"), "sort", &[], true).is_some());
    }

    // ── echo/printf source extraction policy ─────────────────────────

    #[test]
    fn echo_n_source_has_no_trailing_newline() {
        let src = extract_text_from_stage(&echo_stage(vec![st("-n"), st("x")])).unwrap();
        assert!(!src.trailing_newline);
        assert!(matches!(&src.text, IrExpr::Str(s, _) if s == "x"), "-n is a flag, not text");
    }

    // `-e` is no longer refused outright: the escape interpretation happens at
    // IR time over an all-literal argument list, so the consumer stage sees the
    // bytes bash wrote. A non-literal list still refuses (see
    // `echo_e_with_a_variable_stays_a_command`).
    #[test]
    fn echo_e_source_is_decoded() {
        let src = extract_text_from_stage(&echo_stage(vec![st("-e"), st("a\\nb")])).unwrap();
        assert!(src.trailing_newline && src.is_literal);
        assert!(matches!(&src.text, IrExpr::Str(t, _) if t == "a\nb"));
    }

    #[test]
    fn echo_e_with_a_variable_stays_a_command() {
        // `echo -e "$v"` interprets escapes inside the VARIABLE's value, which
        // is a runtime fact — the args are not literal bytes any more.
        let stage = vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                st("echo"),
                IrExpr::Array(vec![st("-e"), IrExpr::Var("v".to_string(), None)]),
            ],
        })];
        assert!(extract_text_from_stage(&stage).is_none());
    }

    #[test]
    fn cut_specs_expand_to_ascending_field_indices() {
        // `-f3,1` is the SAME selection as `-f1,3` (cut prints the line's
        // fields in input order), `-f2-` runs to the last field the literal
        // line has, and duplicates collapse.
        assert_eq!(
            parse_field_spec("3,1", Some(4)).unwrap(),
            vec![FieldRange::Single(1), FieldRange::Single(3)]
        );
        assert_eq!(
            parse_field_spec("2-", Some(4)).unwrap(),
            vec![
                FieldRange::Single(2),
                FieldRange::Single(3),
                FieldRange::Single(4)
            ]
        );
        assert_eq!(
            parse_field_spec("2,2-3", Some(3)).unwrap(),
            vec![FieldRange::Single(2), FieldRange::Single(3)]
        );
        // `-f1-5` over a two-field line keeps the two it has.
        assert_eq!(
            parse_field_spec("1-5", Some(2)).unwrap(),
            vec![FieldRange::Single(1), FieldRange::Single(2)]
        );
    }

    #[test]
    fn cut_spec_refusals() {
        // A reversed range and field 0 are ERRORS in cut, so the command is
        // kept (a wrong-but-silent empty selection must never be offered).
        assert!(parse_field_spec("4-2", Some(4)).is_none());
        assert!(parse_field_spec("0", Some(4)).is_none());
        assert!(parse_field_spec("1,x", Some(4)).is_none());
        // An open range over an UNKNOWN length is not a finite index list.
        assert!(parse_field_spec("2-", None).is_none());
        // A selection that hits no field of the literal line: cut prints
        // NOTHING for the line, not a blank line — the caller must refuse.
        assert_eq!(parse_field_spec("4", Some(3)).unwrap(), Vec::new());
        assert_eq!(parse_field_spec("5-", Some(3)).unwrap(), Vec::new());
    }

    #[test]
    fn provided_producers_are_known_bytes() {
        // `seq` with integer operands: the exact lines it writes, with the
        // trailing newline a non-empty range has.
        let seq = |args: Vec<IrExpr>| {
            extract_text_from_stage(&[IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st("seq"), IrExpr::Array(args)],
            })])
        };
        let src = seq(vec![st("1"), st("3")]).expect("seq 1 3");
        assert!(src.is_literal && src.trailing_newline);
        assert!(matches!(&src.text, IrExpr::Str(t, _) if t == "1\n2\n3"));
        let src = seq(vec![st("5"), st("-1"), st("1")]).expect("seq 5 -1 1");
        assert!(matches!(&src.text, IrExpr::Str(t, _) if t == "5\n4\n3\n2\n1"));
        // An EMPTY range writes nothing at all — no trailing newline — which
        // is what `seq 1 0 | wc -l` (0) must see.
        let src = seq(vec![st("1"), st("0")]).expect("seq 1 0");
        assert!(!src.trailing_newline);
        assert!(matches!(&src.text, IrExpr::Str(t, _) if t.is_empty()));
        // Refusals: a fractional operand (GNU %g formatting), a step of 0,
        // and any extra operand.
        assert!(seq(vec![st("0.5"), st("2")]).is_none());
        assert!(seq(vec![st("1"), st("0"), st("5")]).is_none());
        assert!(seq(vec![st("1"), st("2"), st("3"), st("4")]).is_none());
    }

    #[test]
    fn echo_e_decodes_its_escapes() {
        assert_eq!(decode_echo_escapes("a\\nb").unwrap(), "a\nb");
        assert_eq!(decode_echo_escapes("x\\ty").unwrap(), "x\ty");
        assert_eq!(decode_echo_escapes("\\101\\x42").unwrap(), "AB");
        // An unknown escape keeps its backslash, as bash does.
        assert_eq!(decode_echo_escapes("a\\qb").unwrap(), "a\\qb");
        // `\c` also stops the output and drops the newline (control flow, not
        // a byte), and a NUL cannot live in a Rust String.
        assert!(decode_echo_escapes("a\\cb").is_none());
        assert!(decode_echo_escapes("a\\0").is_none());
    }

    #[test]
    fn printf_literal_formats_are_folded_bytes() {
        // `printf 'a\nb\n'`'s BYTES are known at IR time (const_printf), so
        // a consumer stage reduces over what bash actually wrote. This is
        // the deliberate change from the earlier blanket refusal, whose
        // objection was to treating the ARGS as the output (`printf x |\
        // wc -l` is 0 in bash, the naive lowering said 1).
        let stage = |args: Vec<IrExpr>| {
            vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st("printf"), IrExpr::Array(args)],
            })]
        };
        let src = extract_text_from_stage(&stage(vec![st("a\\nb\\n")])).unwrap();
        assert!(src.trailing_newline);
        assert!(src.is_literal);
        assert!(matches!(&src.text, IrExpr::Str(t, _) if t == "a\nb"));
        // The format's own newline is the only one: `printf x` writes none,
        // so `printf x | wc -l` still counts 0.
        let src = extract_text_from_stage(&stage(vec![st("x")])).unwrap();
        assert!(!src.trailing_newline);
        assert!(matches!(&src.text, IrExpr::Str(t, _) if t == "x"));
    }

    #[test]
    fn printf_unmodelled_shapes_stay_commands() {
        // Format cycling (`printf '%s\n' a b` prints a and b), `-v` (assigns
        // instead of writing) and a non-literal format are NOT known at IR
        // time — each keeps the original command rather than guessing.
        let stage = |args: Vec<IrExpr>| {
            vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st("printf"), IrExpr::Array(args)],
            })]
        };
        assert!(extract_text_from_stage(&stage(vec![st("%s\\n"), st("a"), st("b")])).is_none());
        assert!(extract_text_from_stage(&stage(vec![st("-v"), st("x"), st("hi")])).is_none());
        let var = param_var_read(&st("f")).unwrap();
        assert!(extract_text_from_stage(&stage(vec![var])).is_none());
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
        assert!(try_lower_param_op(&[st("slice"), st("v"), st("1"), st("2")], &std::collections::HashSet::new()).is_some());
        let arrays: std::collections::HashSet<String> = ["v".to_string()].into_iter().collect();
        assert!(try_lower_param_op(&[st("slice"), st("v"), st("1"), st("2")], &arrays).is_none());
    }

    /// `${p##*/}` is a GLOB strip, not `basename`: for "a/b/" the strip is
    /// empty and GNU basename is "b". Only a real `basename`/`dirname`
    /// command may become PathName, and a FLAGGED call (`-s`, `-z`) never
    /// does (the node has no flag fields).
    #[test]
    fn only_a_real_path_command_is_pathname() {
        let mut glob_strip = IrExpr::Call {
            func: "param".to_string(),
            args: vec![st("basename"), st("p")],
        };
        lower_expr(&mut glob_strip, &std::collections::HashSet::new());
        assert!(
            matches!(&glob_strip, IrExpr::Call { func, .. } if func == "param"),
            "`${{p##*/}}` became a PathName: {glob_strip:?}"
        );

        let base = |args: Vec<IrExpr>| {
            let mut s = IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st("basename"), IrExpr::Array(args)],
            });
            lower_stmt(&mut s, true, &std::collections::HashSet::new());
            s
        };
        let plain = base(vec![st("/a/b/")]);
        assert!(
            format!("{plain:?}").contains("PathName"),
            "basename command not lowered: {plain:?}"
        );
        let flagged = base(vec![st("-s"), st(".sh"), st("a.b.sh")]);
        assert!(
            !format!("{flagged:?}").contains("PathName"),
            "basename -s became a PathName: {flagged:?}"
        );
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
        lower_stmt(&mut stmt, true, &std::collections::HashSet::new());
        let IrStmt::Block(stmts) = &stmt else {
            panic!("statement-level pipeline should reduce to Block, got {stmt:?}");
        };
        let [IrStmt::Output { value: IrExpr::Ext(n), .. }, IrStmt::SetChildError(_)] = stmts.as_slice() else {
            panic!("unexpected statement block shape: {stmt:?}");
        };
        assert!(n.as_any().downcast_ref::<RegCount>().is_some());
    }

    /// A pipeline longer than two stages FOLDS left-to-right: each later
    /// stage takes the value the previous reduction produced, so
    /// `echo | tr | sort | head` reaches CharTranslate, SortLines and
    /// TakeLines instead of shelling the whole pipeline out.
    #[test]
    fn a_long_pipeline_folds_through_the_text_nodes() {
        let cmd = |name: &str, args: Vec<IrExpr>| {
            IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st(name), IrExpr::Array(args)],
            })
        };
        let stages = vec![
            IrExpr::Arrow(echo_stage(vec![st("b a")])),
            IrExpr::Arrow(vec![cmd("tr", vec![st(" "), st("\\n")])]),
            IrExpr::Arrow(vec![cmd("sort", vec![])]),
            IrExpr::Arrow(vec![cmd("head", vec![st("-2")])]),
        ];
        let (value, already_nl) = try_lower_pipeline(&stages, true).expect("chain");
        assert!(!already_nl);
        let dbg = format!("{value:?}");
        assert!(dbg.contains("CharTranslate"), "tr stage missing: {dbg}");
        assert!(dbg.contains("SortLines"), "sort stage missing: {dbg}");
        assert!(dbg.contains("TakeLines"), "head stage missing: {dbg}");
    }

    /// A COUNT in the middle of a pipeline is not text: `echo x | wc -l | tr`
    /// feeds `tr` the digits, so the chain refuses and the original pipeline
    /// (which is exact) stays.
    #[test]
    fn a_count_in_the_middle_breaks_the_chain() {
        let cmd = |name: &str, args: Vec<IrExpr>| {
            IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st(name), IrExpr::Array(args)],
            })
        };
        let stages = vec![
            IrExpr::Arrow(echo_stage(vec![st("x")])),
            IrExpr::Arrow(vec![cmd("wc", vec![st("-l")])]),
            IrExpr::Arrow(vec![cmd("tr", vec![st("0"), st("o")])]),
        ];
        assert!(try_lower_pipeline(&stages, true).is_none());
    }

    #[test]
    fn tr_escapes_decode_before_the_node_is_built() {
        assert_eq!(decode_tr_escapes(" \\n").as_deref(), Some(" \n"));
        assert_eq!(decode_tr_escapes("a\\tb").as_deref(), Some("a\tb"));
        assert_eq!(decode_tr_escapes("a\\\\b").as_deref(), Some("a\\b"));
        // octal / unknown escapes refuse: the node's sets are MEMBERS, so an
        // escape this cannot decode exactly must keep the runtime path.
        assert!(decode_tr_escapes("\\101").is_none());
        assert!(decode_tr_escapes("a\\").is_none());
    }

    #[test]
    fn capture_body_pipeline_folds_to_the_value() {
        // `$(echo … | tr …)` in WORD position (`local`, a case word, an echo
        // operand): the fold's value IS what the capture collects — `$(…)`
        // strips the trailing newline and the fold's value already excludes
        // it. Arrow bodies lower with emit=false, so before this the
        // statement-level reduction never fired and the capture kept
        // spawning `bash -c`. (This replaces `capture_body_pipeline_not_
        // reduced`, which pinned exactly that old behaviour.)
        crate::transforms::set_target(Some("rs"));
        let commands = crate::parser::commands::parse_commands_from_text(
            "f() { local y=\"$(echo hi | tr a-z A-Z)\"; echo \"$y\"; }",
        )
        .expect("parse");
        let mut prog = crate::shir::ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "the capture did not fold");
        let dbg = format!("{:?}", prog.stmts);
        crate::transforms::set_target(None);
        // `tr a-z A-Z` specializes to CaseTransform (the general map is
        // CharTranslate); either way the CAPTURE and its command are gone.
        assert!(
            dbg.contains("CaseTransform") || dbg.contains("CharTranslate"),
            "no case node: {dbg}"
        );
        assert!(!dbg.contains("Capture"), "the capture survived: {dbg}");
    }

    #[test]
    fn capture_strips_one_newline_from_a_literal_repeat() {
        // `yes Hi | head -n 3` folds to RepeatStr, whose value still ends in
        // "\n" (the statement path tracks that with `already_nl`). In a
        // CAPTURE that trailing newline does not exist: `$(…)` strips exactly
        // one, so the captured value is "Hi\nHi\nHi" — a literal, when the
        // count and the unit are literal. (This replaces
        // `capture_keeps_a_value_that_ends_in_its_own_newline`, which pinned
        // the refusal that made 000__04d/000__05 spawn `bash -c`.)
        crate::transforms::set_target(Some("rs"));
        let stages = vec![
            IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st("yes"), IrExpr::Array(vec![st("Hi")])],
            })]),
            IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![st("head"), IrExpr::Array(vec![st("-n"), st("3")])],
            })]),
        ];
        let inner = IrStmt::Expr(IrExpr::Call {
            func: "pipeline".to_string(),
            args: vec![IrExpr::Array(stages)],
        });
        let mut cap = IrExpr::Capture {
            expr: Box::new(IrExpr::Arrow(vec![inner.clone()])),
            native: false,
        };
        lower_expr(&mut cap, &std::collections::HashSet::new());
        crate::transforms::set_target(None);
        let _ = inner;
        assert!(
            matches!(cap, IrExpr::Str(ref t, _) if t == "Hi\nHi\nHi"),
            "the capture did not become the trimmed repeat: {cap:?}"
        );
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
            // The here-string's target can arrive as an all-literal
            // interpolation (`<<< "x:y:z"`) rather than a bare Str; its bytes
            // are still known, so normalize it and treat it as literal — this
            // is what lets `cut -d: -f2 <<< "x:y:z"` reduce instead of
            // spawning the command for text it already holds.
            let (text, is_literal) = match literal_text(&text) {
                Some(s) => (IrExpr::Str(s, StrStyle::DoubleQuoted), true),
                None => (text, false),
            };
            lower_text_cmd(
                TextSource { text, trailing_newline: true, is_literal },
                cmd_name,
                cmd_args,
                true,
            )
        }
    }
}

/// The operand of a `basename`/`dirname` call, or None when the first word
/// is a FLAG (`basename -s .sh p`, `dirname -z p`) or the call has more than
/// one operand: the node models the plain, SINGLE-operand GNU call only, so a
/// flagged call and `basename p .txt` / `dirname a b` keep their original
/// command (they print MORE than one word — the node prints one).
fn path_operand(cmd_args: &[IrExpr]) -> Option<&IrExpr> {
    if cmd_args.len() != 1 {
        return None;
    }
    match cmd_args.first()? {
        IrExpr::Str(s, _) if s.starts_with('-') => None,
        a => Some(a),
    }
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
        ",," => Some(IrExpr::Ext(Box::new(CaseTransform { text: var, upper: false })))
            .filter(|_| offered("CaseTransform")),
        "^^" => Some(IrExpr::Ext(Box::new(CaseTransform { text: var, upper: true })))
            .filter(|_| offered("CaseTransform")),

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
            .filter(|_| offered("SubStrExtract"))
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
        })))
        .filter(|_| offered("StringContains"));
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
                return Some(IrExpr::Ext(Box::new(StrLen { text: var })))
                    .filter(|_| offered("StrLen"));
            }
            None
        }
        _ => None,
    }
}

/// Read a variable by name: param("", name) — the shIR's plain-read form.
#[allow(dead_code)]
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

    let mut total = 0;
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
                let s = spec[0].as_str()?.parse().ok()?;
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
    .filter(|_| offered("RepeatStr"))
}

/// Reduce `x=$(echo X | <reducible>)`: the Assign's Capture-wrapped pipeline
/// becomes the composed Ext VALUE. Allowlist only (cut/tr/head/tail/wc/xargs/
/// simple sed) — these exit 0 on static input, so `SetChildError(0)` preserves
/// $?. grep is excluded (status idiom). Counts append the echo trailing \n.
fn try_reduce_capture_assign(stmt: &IrStmt, _arrays: &std::collections::HashSet<String>) -> Option<IrStmt> {
    if std::env::var("SH2C_DEBUG").is_ok() {
        if let IrStmt::Assign { expr, .. } = stmt {
            if matches!(expr, IrExpr::Capture { .. }) {
                eprintln!("REDUCE_CAP called");
            }
        }
    }
    // `x=$(find ARGS)` (bare command capture, NO pipeline) → accumulator
    // over WalkDir + command-substitution newline strip. Self-contained
    // peek: the shared extraction below demands a 2-stage pipeline.
    if let IrStmt::Assign { targets, asm, expr, .. } = stmt {
        if let IrExpr::Capture { expr: ci, native: false } = expr {
            if let IrExpr::Arrow(body) = ci.as_ref() {
                if let [IrStmt::Expr(IrExpr::Call { func, args })] = body.as_slice() {
                    if func == "exec" || func == "builtin" {
                        if let [IrExpr::Str(n, _), IrExpr::Array(fa)] = args.as_slice() {
                            if n == "find" {
                                if let Some((path, tf, md, nf)) = parse_find_args(fa) {
                                    let target0 = &targets[0].var;
                                    let acc = target0.clone();
                                    let append = IrStmt::Assign {
                                        targets: vec![AssignTarget { var: acc.clone(), sigil: None, indices: vec![] }],
                                        expr: IrExpr::BinOp {
                                            lhs: Box::new(IrExpr::Var(acc.clone(), None)),
                                            op: BinOpKind::Concat,
                                            rhs: Box::new(IrExpr::BinOp {
                                                lhs: Box::new(loop_var_read("__l")),
                                                op: BinOpKind::Concat,
                                                rhs: Box::new(IrExpr::Str("\n".to_string(), StrStyle::DoubleQuoted)),
                                            }),
                                        },
                                        asm: None,
                                    };
                                    return Some(IrStmt::Block(vec![
                                        IrStmt::Assign {
                                            targets: vec![AssignTarget { var: acc, sigil: None, indices: vec![] }],
                                            expr: IrExpr::Str(String::new(), StrStyle::DoubleQuoted),
                                            asm: None,
                                        },
                                        IrStmt::Ext(Box::new(WalkDir {
                                            source: IrExpr::Str(path, StrStyle::DoubleQuoted),
                                            var: "__l".to_string(),
                                            body: vec![append],
                                            type_filter: tf,
                                            name_filter: nf,
                                            maxdepth: md.map(|v| Box::new(IrExpr::Int(v))),
                                        })),
                                        IrStmt::Assign {
                                            targets: targets.clone(),
                                            // Command-substitution strips
                                            // trailing newlines. That is its
                                            // own node: it was spelled as a
                                            // RegSub with a magic pattern,
                                            // which is a node doing a job its
                                            // denotation does not cover.
                                            expr: IrExpr::Ext(Box::new(NewlineStrip {
                                                text: IrExpr::Var(target0.clone(), None),
                                            })),
                                            asm: asm.clone(),
                                        },
                                        IrStmt::SetChildError(IrExpr::Int(0)),
                                    ]));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // `x=$(wc -w <<< "text")` — a HERE-STRING on fd 0 feeding ONE
    // reducible command lowers to the composed Ext VALUE over the
    // here-text (WordCount/StrLen/RegCount/…). Same status contract as
    // the pipeline arm: these exit 0 on static input.
    //
    // Shape (A1): Capture → Arrow → [Expr(Call pipeline,
    //   [Arrow([Expr(exec NAME words)]),
    //    Array([{fd:0, mode:"herestring", target:text}])])]
    {
        let text_ir: Option<IrExpr> = None;
        let _ = text_ir;
        if let IrStmt::Assign { targets, asm, expr, .. } = stmt {
            if let IrExpr::Capture { expr: ci, native: false } = expr {
                if let IrExpr::Arrow(body) = ci.as_ref() {
                    if std::env::var("SH2C_DEBUG").is_ok() {
                        eprintln!("HEREDOC-ARM body={:?}", body.as_slice().iter().map(|s| format!("{:?}", std::mem::discriminant(s))).collect::<Vec<_>>());
                    }
                    if let [IrStmt::Expr(IrExpr::Call { func, args })] = body.as_slice() {
                        if func != "pipeline" {
                            return None;
                        }
                        let mut stage_cmd: Option<(&str, &[IrExpr])> = None;
                        let mut heredoc_text: Option<&IrExpr> = None;
                        let mut here_is_herestring = false;
                        if let Some(IrExpr::Arrow(stage)) = args.first() {
                            if let [IrStmt::Expr(IrExpr::Call { func: sf, args: sa })] =
                                stage.as_slice()
                            {
                                if sf == "exec" || sf == "builtin" {
                                    if let [IrExpr::Str(n, _), IrExpr::Array(fa)] =
                                        sa.as_slice()
                                    {
                                        stage_cmd = Some((n.as_str(), fa.as_slice()));
                                    }
                                }
                            }
                        }
                        if let Some(IrExpr::Array(specs)) = args.get(1) {
                            for sp in specs {
                                if let IrExpr::Object(fields) = sp {
                                    let mut fd0 = false;
                                    let mut is_herestring = false;
                                    let mut target: Option<&IrExpr> = None;
                                    for (k, v) in fields {
                                        match k.as_str() {
                                            "fd" => {
                                                if matches!(v, IrExpr::Int(f) if *f == 0)
                                                {
                                                    fd0 = true;
                                                }
                                            }
                                            "mode" => {
                                                if let IrExpr::Str(m, _) = v {
                                                    if m == "herestring" || m == "heredoc" {
                                                        is_herestring = true;
                                                        // A heredoc BODY is expanded
                                                        // by the shell; only a
                                                        // here-string's text is
                                                        // literal bytes.
                                                        here_is_herestring = m == "herestring";
                                                    }
                                                }
                                            }
                                            "target" => target = Some(v),
                                            _ => {}
                                        }
                                    }
                                    if fd0 && is_herestring {
                                        heredoc_text = target;
                                    }
                                }
                            }
                        }
                        if let (
                            Some((cmd_name, cmd_args)),
                            Some(text_ir),
                        ) = (stage_cmd, heredoc_text)
                        {
                            if let Some(value) =
                                try_lower_command(text_ir.clone(), cmd_name, cmd_args)
                            {
                                return Some(IrStmt::Block(vec![
                                    IrStmt::Assign {
                                        targets: targets.clone(),
                                        expr: value,
                                        asm: asm.clone(),
                                    },
                                    IrStmt::SetChildError(IrExpr::Int(0)),
                                ]));
                            }
                        }
                    }
                }
            }
        }
    }

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
                            if let [IrStmt::Expr(IrExpr::Call { func: _f1, args: a1 })] =
                                stage_bodies[0]
                            {
                                if let [IrExpr::Str(_, _), IrExpr::Array(fa)] = a1.as_slice() {
                                    if let Some((path, tf, md, nf)) = parse_find_args(fa) {
                                        let mut block = walk_dir_count(
                                            IrExpr::Str(path, StrStyle::DoubleQuoted),
                                            tf,
                                            md.map(IrExpr::Int),
                                            nf,
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

    let value = lower_text_cmd(text, cmd_name, cmd_args, true)?;

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
fn streaming_line_count(source: IrExpr, guard: Option<IrExpr>) -> Option<IrStmt> {
    // The streaming form is ForEachLine: a target without that node keeps the
    // original command (grep/cat | wc -l), which is exact everywhere.
    if !offered("ForEachLine") {
        return None;
    }
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
    Some(IrStmt::Block(vec![
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
    ]))
}

/// GNU find subset: [PATH] [-maxdepth N] [-type f|d]. Anything else
/// (-name, -perm, multi-path, expressions) is not lowered → None.
#[allow(clippy::type_complexity)]
fn parse_find_args(
    args: &[IrExpr],
) -> Option<(String, Option<String>, Option<i64>, Option<String>)> {
    // (path, -type, -maxdepth, -name glob)

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
    let mut nf: Option<String> = None;
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
            "-name" => {
                // a basename GLOB — carried verbatim; renderers implement
                // fnmatch-style matching. Multiple -name (implicit -o
                // semantics in GNU find) → not lowered.
                if nf.is_some() { return None; }
                nf = Some(strs.get(i + 1)?.to_string());
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
    Some((path?, tf, md, nf))
}

/// Streaming directory-entry counter: `n=0; WalkDir(src, l, n+=1)` — the
/// tree is walked entry-by-entry; contents are never opened.
fn walk_dir_count(
    source: IrExpr,
    type_filter: Option<String>,
    maxdepth: Option<IrExpr>,
    name_filter: Option<String>,
) -> IrStmt {
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
            name_filter,
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
    let (path, tf, md, nf) = parse_find_args(a)?;
    Some(walk_dir_count(
        IrExpr::Str(path, StrStyle::DoubleQuoted),
        tf,
        md.map(IrExpr::Int),
        nf,
    ))
}

/// Frontend-ingest normalisations: apply ONLY the let-condition and read
/// lifts to an already-ingested A1 (the fish/zsh frontends emit those
/// constructs as opaque exec calls). Runs when SH2_TRANSFORMS lists
/// text-ops; deliberately narrower than transform() — everything else in
/// the ingest keeps its baseline shape.
pub fn normalize_frontend_constructs(stmts: &mut Vec<IrStmt>) {
    fn normalize_read(s: &mut IrStmt) -> bool {
        if let IrStmt::Expr(IrExpr::Call { func, args }) = s {
            if func == "exec" || func == "builtin" {
                if let [IrExpr::Str(n, _), IrExpr::Array(wa)] = args.as_slice() {
                    if n == "read"
                        && wa.len() == 1
                        && matches!(&wa[0], IrExpr::Str(v, _) if !v.starts_with('-'))
                    {
                        let var = match &wa[0] {
                            IrExpr::Str(v, _) => v.clone(),
                            _ => unreachable!(),
                        };
                        *s = IrStmt::Assign {
                            targets: vec![AssignTarget { var, sigil: None, indices: vec![] }],
                            expr: IrExpr::Ext(Box::new(ReadLine {})),
                            asm: None,
                        };
                        return true;
                    }
                }
            }
        }
        false
    }
    fn cond_let(cond: &mut IrExpr) -> bool {
        if let IrExpr::Call { func, args } = cond {
            if func == "exec" || func == "builtin" {
                if let [IrExpr::Str(n, _), IrExpr::Array(wa)] = args.as_slice() {
                    if n == "let" {
                        if let [IrExpr::Str(tx, _)] = wa.as_slice() {
                            if let Some(repl) = let_cond_from_text(tx) {
                                *cond = repl;
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
    fn wrap_unquoted_vars(s: &mut IrStmt) -> bool {
        if std::env::var("SHIR_ECHO_DEBUG").is_ok() {
            if let IrStmt::Expr(IrExpr::Call { func, args }) = s {
                eprintln!("WUV stmt {} args {}", func, args.len());
            }
        }
        // Frontend-emitted exec/builtin ARG ARRAYS carry unquoted $var
        // expansions as BARE getVar elements. Bash word-splits those on
        // IFS and drops empties — the same contract the CORE parser
        // expresses with its split(getVar(…)) marker (which the renderers
        // already handle as array-valued splices). Wrap to match.
        let mut changed = false;
        if let IrStmt::Expr(IrExpr::Call { func, args }) = s {
            if func == "exec" || func == "builtin" {
                if let Some(IrExpr::Array(items)) = args.get_mut(1) {
                    for it in items.iter_mut() {
                        if let IrExpr::Call { func: f, .. } = it {
                            if f == "getVar" {
                                *it = IrExpr::Call {
                                    func: "split".to_string(),
                                    args: vec![it.clone()],
                                };
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        changed
    }
    fn walk(s: &mut Vec<IrStmt>) {
        let mut i = 0;
        while i < s.len() {
            if normalize_read(&mut s[i]) {
                i += 1;
                continue;
            }
            wrap_unquoted_vars(&mut s[i]);
            match &mut s[i] {
                IrStmt::If { cond, then, elsifs, else_, .. } => {
                    cond_let(cond);
                    walk(then);
                    for (_, b) in elsifs.iter_mut() { walk(b); }
                    walk(else_);
                }
                IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
                    cond_let(cond);
                    walk(body);
                }
                IrStmt::Block(b) | IrStmt::Subshell(b) => walk(b),
                IrStmt::Function { body, .. } => walk(body),
                _ => {}
            }
            i += 1;
        }
    }
    walk(stmts)
}

/// Collect every SCALAR variable name the statement list WRITES (plain
/// Assign targets, setVar stores, read-normalised assigns). Returns false
/// when the body uses constructs the subshell-isolation lowering cannot
/// snapshot: array/hash writes, declarations, function definitions, unset.
fn collect_scalar_writes(stmts: &[IrStmt], out: &mut Vec<String>) -> bool {
    for s in stmts {
        let ok = match s {
            IrStmt::Assign { targets, expr, .. } => {
                for tgt in targets {
                    if !tgt.indices.is_empty() || tgt.sigil.is_some() {
                        return false;
                    }
                    out.push(tgt.var.clone());
                }
                collect_scalar_writes_expr(expr, out)
            }
            IrStmt::Expr(e) => collect_scalar_writes_expr(e, out),
            IrStmt::Output { value, .. } => collect_scalar_writes_expr(value, out),
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                collect_scalar_writes_expr(cond, out)
                    && collect_scalar_writes(then, out)
                    && else_.iter().all(|_| true)
                    && collect_scalar_writes(else_, out)
                    && elsifs.iter().map(|(_, b)| b).all(|b| collect_scalar_writes(b, out))
            }
            IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
                collect_scalar_writes_expr(cond, out) && collect_scalar_writes(body, out)
            }
            IrStmt::For { var, iter, body } => {
                out.push(var.clone());
                collect_scalar_writes_expr(iter, out) && collect_scalar_writes(body, out)
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) => collect_scalar_writes(b, out),
            IrStmt::Pipeline { stages, .. } => stages.iter().all(|st| collect_scalar_writes(st, out)),
            IrStmt::Function { .. } | IrStmt::Declare { .. } | IrStmt::DeclareArray { .. } => false,
            // ForInit/Case/Try/Redirect/background etc.: conservative fallback
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn collect_scalar_writes_expr(e: &IrExpr, out: &mut Vec<String>) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            if func == "setVar" || func == "setArray" || func == "setArrayAppend" {
                return false; // dynamic/array stores — not snapshotted
            }
            // `assign(name, op, val)` writes `name`: snapshot it so a
            // subshell whose body is `x=2` (rendered as `assign`) gets the
            // copy semantics below. Without this, written stayed empty and
            // the subshell was lifted to a Block with NO save/restore —
            // `x=1; (x=2); echo $x` leaked 2 instead of 1.
            if func == "assign" {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    out.push(name.clone());
                }
            }
            args.iter().all(|a| collect_scalar_writes_expr(a, out))
        }
        IrExpr::Var(_, _) | IrExpr::Str(_, _) | IrExpr::Int(_) | IrExpr::Bool(_) => true,
        IrExpr::Arith(a) => {
            // arith assignments inside expressions write dynamically
            !matches!(a.as_ref(), ArithAst::Assign { .. } | ArithAst::IncDec { .. })
                || {
                    return false;
                }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_scalar_writes_expr(lhs, out) && collect_scalar_writes_expr(rhs, out)
        }
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(x) => collect_scalar_writes_expr(x, out),
        }),
        IrExpr::Ext(n) => n.children().iter().all(|c| collect_scalar_writes_expr(c, out)),
        IrExpr::Capture { .. } => false,
        _ => true,
    }
}

/// Construct normalisations for statement-level exec calls
/// (docs/shir-reductions.md §Normalisation transforms).
fn try_normalize_construct_stmt(stmt: &IrStmt) -> Option<IrStmt> {
    // The `local`→assign normalisation is NOT offered to Rust. The Declare
    // renderer binds an unsubstituted `$1` to the enclosing positional, types
    // the binding from `local`'s own metadata (`local -A` stays a map, an int
    // stays i64), and the pass-3 signature probe reads the declaration to
    // decide the function's arity; flattening it produced `fn f()` with
    // `a = "$1"`, a Vec where `declare -A` had built a map, and an int store
    // where a string was declared (061_test_local_names_preserved,
    // typeset-cmdsub, 064_22_function_returning_complex_data_structures).
    if matches!(crate::transforms::target().as_deref(), Some("rs") | Some("rust")) {
        if matches!(stmt, IrStmt::Declare { local: true, .. }) {
            return None;
        }
        if let IrStmt::Expr(IrExpr::Call { func, args }) = stmt {
            if matches!(func.as_str(), "exec" | "builtin") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if matches!(n.as_str(), "local" | "declare" | "typeset") {
                        return None;
                    }
                }
            }
        }
    }
    // `local v=…` / `local v` — the parser lowers these to Declare{local}
    // before transforms run. Our flat variable model has one scope per
    // program, so a local declaration IS an assignment (bash: the var is
    // created/overwritten for the function's lifetime; the corpus never
    // relies on shadow-restore).
    if let IrStmt::Declare { vars, init, local: true } = stmt {
        let mut stmts: Vec<IrStmt> = Vec::new();
        for v in vars {
            let expr = init.clone()
                .unwrap_or_else(|| IrExpr::Str(String::new(), StrStyle::DoubleQuoted));
            stmts.push(IrStmt::Assign {
                targets: vec![AssignTarget { var: v.name.clone(), sigil: None, indices: vec![] }],
                expr,
                asm: None,
            });
        }
        return Some(IrStmt::Block(stmts));
    }

    // only statement-level exec/builtin calls carry these constructs
    let (cmd, cmd_args) = match stmt {
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" || func == "builtin" => {
            match args.as_slice() {
                [IrExpr::Str(n, _), IrExpr::Array(a)] => (n.as_str(), a.as_slice()),
                _ => return None,
            }
        }
        _ => return None,
    };
    let strs: Option<Vec<&str>> = cmd_args.iter().map(|a| match a {
        IrExpr::Str(s, _) => Some(s.as_str()),
        IrExpr::Interpolate(p) if p.len() == 1 => match &p[0] {
            InterpPart::Lit(s) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }).collect();

    // `local v=val …` → plain assigns (flat variable model). The value may
    // carry DYNAMIC parts (`local sqrt_n=$(echo x | bc)`) — the word is an
    // Interpolate whose literal prefix names the variable and whose
    // remaining parts concatenate into the value (captures included).
    if cmd == "local" {
        let mut stmts: Vec<IrStmt> = Vec::new();
        let mut failed_words: Vec<IrExpr> = Vec::new();
        let mut wi = 0usize;
        while wi < cmd_args.len() {
            let mut skip_next = false;
            let (name, value): (String, Option<IrExpr>) = match &cmd_args[wi] {
                // positional form: [Str("v="), <value expr>]
                IrExpr::Str(s, _) if s.ends_with('=') && wi + 1 < cmd_args.len() => {
                    let n = s[..s.len() - 1].to_string();
                    if n.is_empty()
                        || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return None;
                    }
                    skip_next = true;
                    (n, Some(cmd_args[wi + 1].clone()))
                }
                IrExpr::Str(s, _) => {
                    skip_next = false;
                    match s.split_once('=') {
                        Some((n, v)) => (
                            n.to_string(),
                            Some(IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted)),
                        ),
                        None => (s.clone(), None),
                    }
                }
                IrExpr::Interpolate(parts) => {
                    // literal prefix names the variable; remaining parts
                    // concatenate into the value (captures included).
                    // Dynamic part BEFORE any '=' → ambiguous: per-word fail.
                    skip_next = false;
                    let mut name: Option<String> = None;
                    let mut rest: Vec<InterpPart> = Vec::new();
                    let mut ambiguous = false;
                    for p in parts.iter() {
                        if ambiguous {
                            break;
                        }
                        if name.is_some() {
                            rest.push(p.clone());
                            continue;
                        }
                        if let InterpPart::Lit(s) = p {
                            match s.find('=') {
                                Some(eq) => {
                                    name = Some(s[..eq].to_string());
                                    if eq + 1 < s.len() {
                                        rest.push(InterpPart::Lit(s[eq + 1..].to_string()));
                                    }
                                }
                                None => { ambiguous = true; }
                            }
                        } else {
                            ambiguous = true;
                        }
                    }
                    if ambiguous {
                        // keep this word for the residual runtime call
                        failed_words.push(cmd_args[wi].clone());
                        wi += 1;
                        continue;
                    }
                    match name {
                        Some(n) if !n.is_empty()
                            && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
                        {
                            let value: Option<IrExpr> = match rest.len() {
                                0 => None,
                                1 => match &rest[0] {
                                    InterpPart::Lit(s) => Some(IrExpr::Str(
                                        s.clone(),
                                        StrStyle::DoubleQuoted,
                                    )),
                                    InterpPart::Expr(x) => Some(x.as_ref().clone()),
                                },
                                _ => {
                                    let mut it = rest.into_iter();
                                    let first = it.next().unwrap();
                                    let mut acc = match first {
                                        InterpPart::Lit(s) => IrExpr::Str(
                                            s,
                                            StrStyle::DoubleQuoted,
                                        ),
                                        InterpPart::Expr(x) => x.as_ref().clone(),
                                    };
                                    for part in it {
                                        let piece = match part {
                                            InterpPart::Lit(s) => IrExpr::Str(
                                                s,
                                                StrStyle::DoubleQuoted,
                                            ),
                                            InterpPart::Expr(x) => x.as_ref().clone(),
                                        };
                                        acc = IrExpr::BinOp {
                                            lhs: Box::new(acc),
                                            op: crate::ir::BinOpKind::Concat,
                                            rhs: Box::new(piece),
                                        };
                                    }
                                    Some(acc)
                                }
                            };
                            (n, Some(value.unwrap_or_else(|| {
                                IrExpr::Str(String::new(), StrStyle::DoubleQuoted)
                            })))
                        }
                        _ => return None,
                    }
                }
                // array-valued / other complex words: per-word fallback —
                // they stay in a residual exec("local") so SIBLING words
                // still normalise instead of losing the whole statement
                _ => ("\u{0}FAILED".to_string(), None),
            };
            if name == "\u{0}FAILED" {
                failed_words.push(cmd_args[wi].clone());
                wi += 1;
                continue;
            }
            stmts.push(IrStmt::Assign {
                targets: vec![AssignTarget { var: name, sigil: None, indices: vec![] }],
                expr: value.unwrap_or_else(|| {
                    IrExpr::Str(String::new(), StrStyle::DoubleQuoted)
                }),
                asm: None,
            });
            if skip_next { wi += 1; }
            wi += 1;
        }
        if failed_words.is_empty() {
            return Some(IrStmt::Block(stmts));
        }
        stmts.push(IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("local".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(failed_words),
            ],
        }));
        return Some(IrStmt::Block(stmts));
    }

    // `read VAR` (single variable) → Assign{VAR, ReadLine}
    if cmd == "read" {
        let words = strs?;
        if words.len() == 1 && !words[0].starts_with('-') {
            return Some(IrStmt::Assign {
                targets: vec![AssignTarget { var: words[0].to_string(), sigil: None, indices: vec![] }],
                expr: IrExpr::Ext(Box::new(ReadLine {})),
                asm: None,
            });
        }
        return None; // multi-var / flag shapes fall back explicitly
    }

    // `let "i=…"` / `let "i<3"` (statement position): assignments become
    // native arith assigns; bare comparisons are status-only → no-op true.
    if cmd == "let" {
        let words = strs?;
        let mut stmts: Vec<IrStmt> = Vec::new();
        for w in &words {
            // an assignment word (`x=expr`, also x+=expr) lowers natively;
            // a pure comparison in statement position sets $? only
            if let Some(eq) = w.find('=').filter(|&i| i > 0
                && w[..i].chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !w[..i].ends_with('<') && !w[..i].ends_with('>')
                && !w[..i].ends_with('!'))
            {
                let (name, rhs) = (&w[..eq], &w[eq + 1..]);
                let rhs = rhs.strip_prefix('+').unwrap_or(rhs); // += form
                let ast = crate::shir::parse_arith(rhs)?;
                stmts.push(IrStmt::Assign {
                    targets: vec![AssignTarget { var: name.to_string(), sigil: None, indices: vec![] }],
                    expr: IrExpr::Arith(Box::new(ast)),
                    asm: None,
                });
            } else if crate::shir::parse_arith(w).is_some() {
                // comparison-only: bash sets $? by the truth value; our
                // callers use it in conditions (handled at expr level), a
                // bare statement is a no-op with status 0
                continue;
            } else {
                return None;
            }
        }
        return Some(IrStmt::Block(stmts));
    }

    None
}

/// exec("let", [TEXT]) in CONDITION position → native Arith comparison.
fn try_normalize_let_cond(args: &[IrExpr]) -> Option<IrExpr> {
    let [IrExpr::Str(n, _), IrExpr::Array(wa)] = args else { return None };
    if n != "let" { return None; }
    let [IrExpr::Str(text, _)] = wa.as_slice() else { return None };
    let_cond_from_text(text)
}

/// The let-condition core: parse the arithmetic TEXT into a native
/// comparison. Assignment-shaped text is not a condition.
fn let_cond_from_text(text: &str) -> Option<IrExpr> {
    // assignment-shaped text is not a condition
    if text.contains('=') && !matches!(text.chars().next(), Some(c) if c.is_ascii_digit()) {
        let body = text.trim_start_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
        if body.starts_with('=') { return None; }
    }
    let ast = crate::shir::parse_arith(text)?;
    Some(IrExpr::Arith(Box::new(ast)))
}

/// A statement-level `find ARGS` (no pipeline): print each entry path.
fn try_lower_find_stmt(cmd_args: &[IrExpr]) -> Option<IrStmt> {
    let (path, tf, md, nf) = parse_find_args(cmd_args)?;
    Some(IrStmt::Ext(Box::new(WalkDir {
        source: IrExpr::Str(path, StrStyle::DoubleQuoted),
        var: "__l".to_string(),
        body: vec![IrStmt::Output {
            value: loop_var_read("__l"),
            newline: true,
            target: None,
        }],
        type_filter: tf,
        name_filter: nf,
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
    streaming_line_count(path, Some(pat))
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

    let val = lower_text_cmd(
        TextSource { text: loop_var_read("__l"), trailing_newline: false, is_literal: false },
        n2,
        ca,
        true,
    )?;
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
                    return streaming_line_count(path, None);
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
    streaming_line_count(path, Some(pat))
}

/// `echo X | grep -q P` in EXPRESSION/CONDITION position → StringContains.
/// Only fires here (conditions), never at statement level where the status
/// semantics would be lost.
/// Peel command-substitution / redirect wrappers a pipeline stage may carry
/// (e.g. `echo X | grep P >/dev/null` parses as
/// Arrow([Call{func:"redirect", args:[Arrow([exec grep]), ...]})]), returning
/// the innermost command-bearing `Arrow` (whose single stmt is `Expr(Call
/// exec|builtin …)`). The grep-idiom lift needs this to recognize `grep`
/// regardless of how the stage is wrapped — without it, a redirected grep
/// never matches and the stage falls through to an unrenderable Arrow.
/// Mirrors the estree ref, which lowers the bare pipeline directly.
fn peel_to_cmd_arrow(s: &IrExpr) -> Option<&IrExpr> {
    match s {
        IrExpr::Arrow(b) => {
            if b.len() == 1 {
                if let IrStmt::Expr(e) = &b[0] {
                    if matches!(e, IrExpr::Call { func, .. } if func == "redirect")
                        || matches!(e, IrExpr::Capture { .. })
                    {
                        return peel_to_cmd_arrow(e);
                    }
                    return Some(s);
                }
            }
            None
        }
        IrExpr::Capture { expr, .. } => peel_to_cmd_arrow(expr),
        IrExpr::Call { func, args } if func == "redirect" => {
            args.first().and_then(|a| peel_to_cmd_arrow(a))
        }
        _ => None,
    }
}

fn try_lower_grep_cond(stage1: &IrExpr, stage2: &IrExpr) -> Option<IrExpr> {
    let s1 = peel_to_cmd_arrow(stage1)?;
    let s2 = peel_to_cmd_arrow(stage2)?;
    let b1 = match s1 { IrExpr::Arrow(b) => b.as_slice(), _ => return None };
    let b2 = match s2 { IrExpr::Arrow(b) => b.as_slice(), _ => return None };
    let TextSource { text, .. } = extract_text_from_stage(b1)?;
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
