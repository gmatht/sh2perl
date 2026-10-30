//! split-in-place — destructive-buffer-reuse for legacy `split` calls in
//! `for w in $var` iteration position.
//!
//! When a split's SOURCE is a plain mutable store var that is never read
//! or written around the loop statement, the C backend can tokenize the
//! var's own buffer in place (delimiters stamped with NUL) instead of
//! copying it into a scratch first. The transform proves safety and
//! rewrites the legacy `Call{"split", [Var X]}` iterator into
//! `Ext(Split{in_place: true})`; each backend decides what the flag
//! means (GC'd string languages ignore it — immutable strings make it
//! meaningless; C treats it as exclusive-ownership permission).
//!
//! Soundness rules (each conservative):
//! - the text operand must be a PLAIN variable read — literals,
//!   interpolations, env fallbacks and composite exprs are ineligible;
//! - the var must have ZERO reads after the loop statement anywhere in
//!   the program, including inside function bodies (call time unknown);
//! - exactly ONE read at the loop statement itself — a body that also
//!   reads the source would observe the mutated content, diverging from
//!   bash where `$y` keeps its value through `for w in $y`;
//! - ZERO writes to the var inside the loop statement: C's word array
//!   points INTO the source buffer, so a mid-loop reassignment frees it
//!   under the iterator (use-after-free), and any mid-loop store would
//!   destroy words bash still splits from the pristine value. Writes
//!   AFTER the loop stay eligible: the loop is over (the stale word
//!   pointers are never re-read — the next split overwrites them) and a
//!   later read is already refused above, so neither the free on
//!   reassign nor a re-export can observe the stamped bytes. The loop
//!   variable itself is a write, so `for x in $x` refuses;
//! - writes performed by FUNCTION bodies refuse the var outright
//!   (call time unknown), as do reads — both walked over every top-level
//!   function, nested definitions included;
//! - command shapes that store into a variable are census'd as writes
//!   (`unset`/`declare`/`typeset`/`local`/`readonly`/`export`/`read`/
//!   `mapfile`/`readarray`/`printf -v`/`cd`-family word arguments, plus
//!   `Assign`/`Declare`/capture/loop-var targets), and
//!   `eval`/`source`/`trap`/`getopts`/wrapper commands
//!   (`time`/`nohup`/`env`/`nice`/`command`) or a computed command name
//!   set a WILD flag that refuses the candidate — their variable effects
//!   cannot be proven from the IR. Unknown statement/expr kinds (Ext
//!   statements, inline asm, JSON/object payloads) are likewise wild
//!   (REFUSE > GUESS);
//! - literal text is scanned for `$name` / `${name…` references (the
//!   parser keeps source text with `$` in `Str` — Declare inits, glob
//!   patterns, exec words — and those expand at render);
//! - `set -o allexport` anywhere refuses every candidate (every later
//!   store would be env-visible to children);
//! - export-visible names are ineligible (the stamped bytes must never
//!   reach a child process's environment).
//!
//! Registered in transforms.rs ("split-in-place"), gated to the C target
//! (only C renders the flag today; the embed/Carp test pins fragment
//! output with the target unset, which skips the transform). Additive
//! contract: Split gains an optional_bool field defaulting to false.

use crate::ir::{ArithAst, InterpPart, IrExpr, IrStmt};
use std::collections::{BTreeSet, HashMap, HashSet};

fn is_ident(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Push every conservative READ candidate a piece of literal text
/// provides: the whole text (bare-equality arg), its `head` when the
/// text is `head=…`, and every `$name` / `${name…` / `${#name` /
/// `${!name` reference inside it (Str keeps source text with `$`, and
/// those expand at render).
fn push_text_refs(text: &str, r: &mut Vec<String>) {
    r.push(text.to_string());
    if let Some((head, _)) = text.split_once('=') {
        if is_ident(head) {
            r.push(head.to_string());
        }
    }
    push_dollar_refs(text, r);
}

/// Only the `$name` references inside a piece of text — for positions
/// that are WRITES of a whole name (an Assign target's baked subscript
/// `arr[$y]` reads `y`, but a bare `y=` target is a write, and pushing
/// it as a read would wrongly refuse the allowed "later write" case).
fn push_dollar_refs(text: &str, r: &mut Vec<String>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'$' {
            i += 1;
            continue;
        }
        i += 1;
        if i < b.len() && b[i] == b'{' {
            i += 1;
            if i < b.len() && (b[i] == b'#' || b[i] == b'!') {
                i += 1;
            }
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if i > start {
                r.push(text[start..i].to_string());
            }
        } else {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if i > start {
                r.push(text[start..i].to_string());
            }
        }
    }
}

fn word_write(word: &str, w: &mut Vec<String>) {
    if is_ident(word) {
        w.push(word.to_string());
    } else if let Some((head, _)) = word.split_once('=') {
        if is_ident(head) {
            w.push(head.to_string());
        }
    }
}

/// Commands whose word arguments are variable STORES: push the target
/// names. `printf` only writes through `-v`; the `cd` family updates
/// PWD/OLDPWD.
fn command_writes(cmd: &str, args: &[IrExpr], w: &mut Vec<String>, wild: &mut bool) {
    let words: Vec<String> = args
        .iter()
        .filter_map(|a| match a {
            IrExpr::Str(s, _) => Some(s.clone()),
            _ => None,
        })
        .collect();
    let any_dynamic = args.iter().any(|a| !matches!(a, IrExpr::Str(_, _)));
    match cmd {
        "eval" | "source" | "." | "trap" | "getopts" | "time" | "nohup" | "env" | "nice"
        | "command" => *wild = true,
        "unset" | "declare" | "typeset" | "local" | "readonly" | "export" | "mapfile"
        | "readarray" | "read" => {
            for wd in &words {
                word_write(wd, w);
            }
            // a dynamic argument to a storing builtin may name anything
            if any_dynamic {
                *wild = true;
            }
        }
        "printf" => {
            if let Some(p) = words.iter().position(|wd| wd == "-v") {
                if let Some(t) = words.get(p + 1) {
                    word_write(t, w);
                }
            }
        }
        "cd" | "pushd" | "popd" | "pwd" => {
            w.push("PWD".to_string());
            w.push("OLDPWD".to_string());
        }
        _ => {}
    }
}

/// The (command, arg) view of an expression-level call: `exec`/`builtin`
/// wrap the command word; anything else names it directly.
fn call_command<'a>(func: &'a str, args: &'a [IrExpr]) -> Option<(&'a str, &'a [IrExpr])> {
    match func {
        "exec" | "builtin" => match args.first() {
            Some(IrExpr::Str(c, _)) => Some((c.as_str(), &args[1..])),
            _ => None,
        },
        _ => Some((func, args)),
    }
}

fn expr_rw(e: &IrExpr, r: &mut Vec<String>, w: &mut Vec<String>, wild: &mut bool) {
    match e {
        IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Range { .. } => {}
        IrExpr::Str(s, _) | IrExpr::RawExpr(s) => push_text_refs(s, r),
        IrExpr::Var(n, _) | IrExpr::Ident(n) => r.push(n.clone()),
        IrExpr::Index { var, key } => {
            r.push(var.clone());
            expr_rw(key, r, w, wild);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_rw(lhs, r, w, wild);
            expr_rw(rhs, r, w, wild);
        }
        IrExpr::Call { func, args } => {
            if let Some((cmd, rest)) = call_command(func, args) {
                command_writes(cmd, rest, w, wild);
            }
            // the callee name may shadow a var name (conservative)
            r.push(func.clone());
            for a in args {
                expr_rw(a, r, w, wild);
            }
        }
        IrExpr::MethodCall { obj, method, args } => {
            r.push(method.clone());
            expr_rw(obj, r, w, wild);
            for a in args {
                expr_rw(a, r, w, wild);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_rw(cond, r, w, wild);
            expr_rw(then, r, w, wild);
            expr_rw(else_, r, w, wild);
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_rw(expr, r, w, wild);
            expr_rw(default, r, w, wild);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                match p {
                    InterpPart::Lit(s) => push_text_refs(s, r),
                    InterpPart::Expr(x) => expr_rw(x, r, w, wild),
                }
            }
        }
        IrExpr::Capture { expr, .. } => expr_rw(expr, r, w, wild),
        IrExpr::Regex { pattern, flags } => {
            push_text_refs(pattern, r);
            push_text_refs(flags, r);
        }
        IrExpr::Arrow(body) => {
            for b in body {
                stmt_rw(b, r, w, wild);
            }
        }
        IrExpr::Array(items) => {
            for it in items {
                expr_rw(it, r, w, wild);
            }
        }
        IrExpr::ArrayComp { var, iter, elem, cond } => {
            w.push(var.clone());
            expr_rw(iter, r, w, wild);
            expr_rw(elem, r, w, wild);
            if let Some(c) = cond {
                expr_rw(c, r, w, wild);
            }
        }
        IrExpr::Lambda { params, body } => {
            // parameter binding is a write at call time (unknown here)
            for p in params {
                w.push(p.clone());
            }
            for b in body {
                stmt_rw(b, r, w, wild);
            }
        }
        IrExpr::Arith(a) => arith_rw(a, r, w),
        IrExpr::Json(_) | IrExpr::Object(_) => {
            // estree-path payloads: uninspectable here → refuse
            *wild = true;
        }
        IrExpr::Splice(x) => expr_rw(x, r, w, wild),
        IrExpr::Ext(n) => {
            for c in crate::shir_nodes::ExtExpr::children(&**n) {
                expr_rw(c, r, w, wild);
            }
        }
    }
}

fn arith_rw(a: &ArithAst, r: &mut Vec<String>, w: &mut Vec<String>) {
    match a {
        ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
        ArithAst::Var(v) | ArithAst::Ident(v) => r.push(v.clone()),
        ArithAst::Index { var, key } => {
            r.push(var.clone());
            arith_rw(key, r, w);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_rw(lhs, r, w);
            arith_rw(rhs, r, w);
        }
        ArithAst::Un { arg, .. } => arith_rw(arg, r, w),
        ArithAst::Cond { test, then, else_ } => {
            arith_rw(test, r, w);
            arith_rw(then, r, w);
            arith_rw(else_, r, w);
        }
        ArithAst::Assign { var, rhs, .. } => {
            w.push(var.clone());
            r.push(var.clone());
            arith_rw(rhs, r, w);
        }
        ArithAst::IncDec { var, .. } => {
            w.push(var.clone());
            r.push(var.clone());
        }
        ArithAst::Cast { arg, .. } => arith_rw(arg, r, w),
    }
}

fn stmt_rw(s: &IrStmt, r: &mut Vec<String>, w: &mut Vec<String>, wild: &mut bool) {
    match s {
        IrStmt::Ext(_) => {
            // opaque transform node: assume any read/write
            *wild = true;
        }
        IrStmt::Output { value, target, .. } => {
            expr_rw(value, r, w, wild);
            if let Some(t) = target {
                r.push(t.clone());
            }
        }
        IrStmt::WriteFile { path, content, .. } => {
            expr_rw(path, r, w, wild);
            expr_rw(content, r, w, wild);
        }
        IrStmt::Assign { targets, expr, asm } => {
            for t in targets {
                let base = t.var.split('[').next().unwrap_or(&t.var);
                let base = base.split('{').next().unwrap_or(base);
                w.push(base.to_string());
                // a baked subscript may READ through `$y`
                push_dollar_refs(&t.var, r);
                for ix in &t.indices {
                    expr_rw(ix, r, w, wild);
                }
            }
            expr_rw(expr, r, w, wild);
            if asm.is_some() {
                *wild = true;
            }
        }
        IrStmt::Declare { vars, init, .. } => {
            for d in vars {
                w.push(d.name.clone());
            }
            if let Some(e) = init {
                expr_rw(e, r, w, wild);
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            w.push(var.clone());
            for e in elements {
                expr_rw(e, r, w, wild);
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            expr_rw(cond, r, w, wild);
            for b in then.iter().chain(else_) {
                stmt_rw(b, r, w, wild);
            }
            for (c, b) in elsifs {
                expr_rw(c, r, w, wild);
                for st in b {
                    stmt_rw(st, r, w, wild);
                }
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            for b in body.iter().chain(else_body).chain(finally_body) {
                stmt_rw(b, r, w, wild);
            }
            for e in excepts {
                if let Some(m) = &e.match_expr {
                    expr_rw(m, r, w, wild);
                }
                if let Some(n) = &e.as_name {
                    w.push(n.clone());
                }
                for b in &e.body {
                    stmt_rw(b, r, w, wild);
                }
            }
        }
        IrStmt::For { var, iter, body } => {
            w.push(var.clone());
            expr_rw(iter, r, w, wild);
            for b in body {
                stmt_rw(b, r, w, wild);
            }
        }
        IrStmt::ForInit { init, cond, step, body } => {
            for b in init.iter().chain(step).chain(body) {
                stmt_rw(b, r, w, wild);
            }
            expr_rw(cond, r, w, wild);
        }
        IrStmt::Continue | IrStmt::Break => {}
        IrStmt::While { cond, body } => {
            expr_rw(cond, r, w, wild);
            for b in body {
                stmt_rw(b, r, w, wild);
            }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            for b in body {
                stmt_rw(b, r, w, wild);
            }
            expr_rw(cond, r, w, wild);
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
            expr_rw(expr, r, w, wild);
        }
        IrStmt::Exec {
            cmd,
            args,
            capture,
            redirects,
            env,
        } => {
            if let IrExpr::Str(c, _) = cmd {
                command_writes(c, args, w, wild);
            } else {
                // a computed command name may be anything
                *wild = true;
            }
            expr_rw(cmd, r, w, wild);
            for a in args {
                expr_rw(a, r, w, wild);
            }
            if let Some(t) = capture {
                w.push(t.clone());
            }
            for rd in redirects {
                expr_rw(rd, r, w, wild);
            }
            for (v, e) in env {
                r.push(v.clone());
                expr_rw(e, r, w, wild);
            }
        }
        IrStmt::Pipeline {
            stages,
            last_output,
            capture,
            cmd_str,
        } => {
            for stage in stages {
                for b in stage {
                    stmt_rw(b, r, w, wild);
                }
            }
            if let Some(t) = last_output {
                w.push(t.clone());
            }
            if let Some(t) = capture {
                w.push(t.clone());
            }
            if let Some(s) = cmd_str {
                push_text_refs(s, r);
            }
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            if let Some(x) = e {
                expr_rw(x, r, w, wild);
            }
        }
        IrStmt::SetChildError(e) => expr_rw(e, r, w, wild),
        IrStmt::Require(s) | IrStmt::RawText(s) => push_text_refs(s, r),
        IrStmt::Case { discriminant, clauses } => {
            expr_rw(discriminant, r, w, wild);
            for c in clauses {
                for p in &c.patterns {
                    push_text_refs(p, r);
                }
                for b in &c.body {
                    stmt_rw(b, r, w, wild);
                }
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            for b in inner {
                stmt_rw(b, r, w, wild);
            }
            for rd in redirects {
                expr_rw(&rd.target, r, w, wild);
            }
        }
        IrStmt::Function {
            name: fname,
            body,
            named_blocks,
            ..
        } => {
            // definition site: the name may shadow a var, and the body is
            // walked here conservatively (call time unknown — over-count
            // only ever refuses)
            r.push(fname.clone());
            for b in body {
                stmt_rw(b, r, w, wild);
            }
            for (bn, b) in named_blocks {
                r.push(bn.clone());
                for st in b {
                    stmt_rw(st, r, w, wild);
                }
            }
        }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            for b in body {
                stmt_rw(b, r, w, wild);
            }
        }
        IrStmt::Select { clauses } => {
            for c in clauses {
                r.push(c.comm.clone());
                if let Some(t) = &c.target {
                    r.push(t.clone());
                }
                if let Some(ch) = &c.ch {
                    expr_rw(ch, r, w, wild);
                }
                if let Some(v) = &c.value {
                    expr_rw(v, r, w, wild);
                }
                for b in &c.body {
                    stmt_rw(b, r, w, wild);
                }
            }
        }
        IrStmt::Asm { .. } => {
            // inline asm may read/write anything reachable
            *wild = true;
        }
        IrStmt::Expr(e) => expr_rw(e, r, w, wild),
        IrStmt::Label(s) | IrStmt::Goto(s) => r.push(s.clone()),
        IrStmt::Shaderize { var, body, .. } => {
            w.push(var.clone());
            for b in body {
                stmt_rw(b, r, w, wild);
            }
        }
    }
}

/// Apply the transform (analysis-only — rewrites eligible iterators).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // per top-level statement: read counts, write names, wild flag
    let mut reads: Vec<HashMap<String, usize>> = Vec::new();
    let mut writes: Vec<HashSet<String>> = Vec::new();
    let mut wilds: Vec<bool> = Vec::new();
    for s in stmts.iter() {
        let (mut r, mut w) = (Vec::new(), Vec::new());
        let mut wild = false;
        stmt_rw(s, &mut r, &mut w, &mut wild);
        let mut m: HashMap<String, usize> = HashMap::new();
        for n in r {
            *m.entry(n).or_insert(0) += 1;
        }
        reads.push(m);
        writes.push(w.into_iter().collect());
        wilds.push(wild);
    }

    // reads AND writes inside FUNCTION definitions: call time unknown —
    // any var mentioned there is ineligible everywhere
    let mut fn_read_names: BTreeSet<String> = BTreeSet::new();
    let mut fn_write_names: BTreeSet<String> = BTreeSet::new();
    let mut fn_wild = false;
    for s in stmts.iter() {
        if let IrStmt::Function { name, body, .. } = s {
            fn_read_names.insert(name.clone());
            let (mut r, mut w) = (Vec::new(), Vec::new());
            let mut wild = false;
            for b in body {
                stmt_rw(b, &mut r, &mut w, &mut wild);
            }
            fn_read_names.extend(r);
            fn_write_names.extend(w);
            fn_wild |= wild;
        }
    }

    let exported = exported_names(stmts);
    // `set -o allexport`: every later store becomes env-visible
    let allexport = export_allexports(stmts);
    if allexport {
        return false;
    }

    // candidate shape: For{iter: Array[Call{"split", [Var X]}]}
    let n = stmts.len();
    let mut changed = false;
    for (i, s) in stmts.iter_mut().enumerate() {
        let IrStmt::For { iter, .. } = s else { continue };
        let IrExpr::Array(items) = &*iter else { continue };
        if items.len() != 1 {
            continue;
        }
        let IrExpr::Call { func, args } = &items[0] else { continue };
        if func != "split" {
            continue;
        }
        // the source arrives as getVar("y") OR Var("y")
        let xv: String = match args.first() {
            Some(IrExpr::Var(v, _)) | Some(IrExpr::Ident(v)) => v.clone(),
            Some(IrExpr::Call { func, args: ca })
                if func == "getVar" && matches!(ca.first(), Some(IrExpr::Str(_, _))) =>
            {
                match ca.first() {
                    Some(IrExpr::Str(nm, _)) => nm.clone(),
                    _ => continue,
                }
            }
            _ => continue,
        };
        if !is_plain_mutable_name(&xv) {
            continue;
        }
        let x = xv.clone();
        let cnt_i = reads.get(i).and_then(|m| m.get(&x)).copied().unwrap_or(0);
        let wrote_i = wilds[i] || writes[i].contains(&x);
        // later READS refuse (the stamped bytes must be unobservable);
        // later WRITES do not — the loop is over, and no later read
        // exists, so the free-on-reassign cannot dangle and a re-export
        // cannot observe the stamped value (exported names refuse above).
        let later = (i + 1..n).any(|j| wilds[j] || reads[j].contains_key(&x));
        let ok = cnt_i == 1
            && !wrote_i
            && !later
            && !fn_read_names.contains(&x)
            && !fn_write_names.contains(&x)
            && !fn_wild
            && !exported.contains(&x);
        if !ok {
            continue;
        }
        // eligible: swap the legacy split-call for the marked Ext(Split);
        // the For iter stays a word-list ARRAY (shape preserved)
        *iter = IrExpr::Array(vec![IrExpr::Ext(Box::new(crate::shir_nodes::Split {
            text: IrExpr::Var(x.clone(), None),
            delim: delim_from(args),
            is_regex: false,
            in_place: true,
        }))]);
        changed = true;
    }
    changed
}

fn delim_from(args: &[IrExpr]) -> String {
    match args.get(1) {
        Some(IrExpr::Str(d, _)) => d.clone(),
        _ => " ".to_string(),
    }
}

fn is_plain_mutable_name(x: &str) -> bool {
    is_ident(x)
}

/// Names given the EXPORT attribute or otherwise env-visible — never
/// eligible for in-place destruction.
pub fn exported_names(stmts: &[IrStmt]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let add_word = |ws: &str, out: &mut BTreeSet<String>| {
        if let Some((n, _)) = ws.split_once('=') {
            if !n.is_empty() {
                out.insert(n.to_string());
            }
        } else if !ws.is_empty() {
            out.insert(ws.to_string());
        }
    };
    for s in stmts.iter() {
        match s {
            IrStmt::Expr(IrExpr::Call { func, args })
                if (func == "exec" || func == "builtin")
                    && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "export") =>
            {
                for a in args.iter().skip(1) {
                    match a {
                        IrExpr::Str(ws, _) => add_word(ws, &mut out),
                        IrExpr::Array(items) => {
                            for it in items {
                                if let IrExpr::Str(ws, _) = it {
                                    add_word(ws, &mut out);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Exec-form `export x` / `export x=y`
            IrStmt::Exec {
                cmd: IrExpr::Str(c, _),
                args,
                ..
            } if c == "export" => {
                for a in args {
                    if let IrExpr::Str(ws, _) = a {
                        add_word(ws, &mut out);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Did the program enable `set -o allexport` (every store becomes
/// env-visible)? Checked in the `set` builtin's word args in either the
/// Call or Exec form.
fn export_allexports(stmts: &[IrStmt]) -> bool {
    fn words_have_allexport(args: &[IrExpr]) -> bool {
        args.iter()
            .any(|a| matches!(a, IrExpr::Str(w, _) if w.contains("allexport")))
    }
    for s in stmts.iter() {
        let hit = match s {
            IrStmt::Expr(IrExpr::Call { func, args })
                if (func == "exec" || func == "builtin")
                    && matches!(args.first(), Some(IrExpr::Str(c, _)) if c == "set") =>
            {
                words_have_allexport(&args[1..])
            }
            IrStmt::Exec {
                cmd: IrExpr::Str(c, _),
                args,
                ..
            } if c == "set" => words_have_allexport(args),
            _ => false,
        };
        if hit {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn assign(var: &str, val: &str) -> IrStmt {
        IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Str(val.to_string(), StrStyle::DoubleQuoted),
            asm: None,
        }
    }

    fn read(v: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Var(v.to_string(), None))
    }

    /// `for w in $src; do <body>; done` — the legacy Call{split} shape.
    fn split_for(src: &str, body: Vec<IrStmt>) -> IrStmt {
        IrStmt::For {
            var: "w".to_string(),
            iter: IrExpr::Array(vec![IrExpr::Call {
                func: "split".to_string(),
                args: vec![IrExpr::Var(src.to_string(), None)],
            }]),
            body,
        }
    }

    fn fndef(name: &str, body: Vec<IrStmt>) -> IrStmt {
        IrStmt::Function {
            name: name.to_string(),
            params: vec![],
            body,
            named_blocks: vec![],
        }
    }

    /// "ext" = rewritten to Ext(Split{in_place}); "call" = legacy kept.
    fn shape(stmts: &[IrStmt]) -> &'static str {
        let Some(IrStmt::For { iter, .. }) = stmts.iter().find(|s| matches!(s, IrStmt::For { .. }))
        else {
            return "no-for";
        };
        let IrExpr::Array(items) = iter else {
            return "no-array";
        };
        match &items[0] {
            IrExpr::Call { .. } => "call",
            IrExpr::Ext(n) if n.tag() == "Split" => "ext",
            _ => "other",
        }
    }

    #[test]
    fn eligible_iter_rewrites_to_in_place() {
        let mut stmts = vec![assign("y", "a b c"), split_for("y", vec![read("w")])];
        assert!(transform(&mut stmts));
        assert_eq!(shape(&stmts), "ext");
        // and the flag + source travel on the node
        let IrStmt::For { iter, .. } = &stmts[1] else { panic!() };
        let IrExpr::Array(items) = iter else { panic!() };
        let IrExpr::Ext(n) = &items[0] else { panic!() };
        let sp = n.as_any().downcast_ref::<crate::shir_nodes::Split>().unwrap();
        assert!(sp.in_place);
        assert!(matches!(&sp.text, IrExpr::Var(v, _) if v == "y"));
    }

    #[test]
    fn body_write_to_source_refuses() {
        let mut stmts = vec![
            assign("y", "a b c"),
            split_for("y", vec![read("w"), assign("y", "d e")]),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(shape(&stmts), "call");
    }

    #[test]
    fn body_read_of_source_refuses() {
        // `for w in $y; do echo $y; done` — bash keeps $y pristine
        let mut stmts = vec![
            assign("y", "a b c"),
            split_for("y", vec![read("w"), read("y")]),
        ];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn eval_in_body_refuses() {
        let ev = IrStmt::Expr(IrExpr::Call {
            func: "eval".to_string(),
            args: vec![IrExpr::Str("x=1".to_string(), StrStyle::DoubleQuoted)],
        });
        let mut stmts = vec![assign("y", "a b c"), split_for("y", vec![read("w"), ev])];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn unset_of_source_in_body_refuses() {
        let un = IrStmt::Expr(IrExpr::Call {
            func: "unset".to_string(),
            args: vec![IrExpr::Str("y".to_string(), StrStyle::DoubleQuoted)],
        });
        let mut stmts = vec![assign("y", "a b c"), split_for("y", vec![read("w"), un])];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn later_read_refuses() {
        let mut stmts = vec![
            assign("y", "a b c"),
            split_for("y", vec![read("w")]),
            read("y"),
        ];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn later_write_stays_eligible() {
        // the loop is over; no later read exists, so the stamped bytes
        // can never be observed (documented rule)
        let mut stmts = vec![
            assign("y", "a b c"),
            split_for("y", vec![read("w")]),
            assign("y", "z"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(shape(&stmts), "ext");
    }

    #[test]
    fn fn_write_of_source_refuses() {
        let mut stmts = vec![
            assign("y", "a b c"),
            fndef("f", vec![assign("y", "other")]),
            split_for("y", vec![read("w")]),
        ];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn fn_read_of_source_refuses() {
        let mut stmts = vec![
            assign("y", "a b c"),
            fndef("f", vec![read("y")]),
            split_for("y", vec![read("w")]),
        ];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn loop_var_equal_to_source_refuses() {
        // `for y in $y` writes y at the loop statement
        let mut stmts = vec![
            assign("y", "a b c"),
            IrStmt::For {
                var: "y".to_string(),
                iter: IrExpr::Array(vec![IrExpr::Call {
                    func: "split".to_string(),
                    args: vec![IrExpr::Var("y".to_string(), None)],
                }]),
                body: vec![read("w")],
            },
        ];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn dollar_ref_text_in_body_refuses() {
        // a Str carrying `$y` text expands at render — counted as a read
        let txt = IrStmt::Expr(IrExpr::Str(
            "v=$y".to_string(),
            StrStyle::DoubleQuoted,
        ));
        let mut stmts = vec![assign("y", "a b c"), split_for("y", vec![read("w"), txt])];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn non_plain_source_not_rewritten() {
        // composite/interpolated sources are ineligible by shape
        let mut stmts = vec![
            assign("y", "a b c"),
            IrStmt::For {
                var: "w".to_string(),
                iter: IrExpr::Array(vec![IrExpr::Call {
                    func: "split".to_string(),
                    args: vec![IrExpr::Interpolate(vec![crate::ir::InterpPart::Expr(
                        Box::new(IrExpr::Var("y".to_string(), None)),
                    )])],
                }]),
                body: vec![read("w")],
            },
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(shape(&stmts), "call");
    }
}
