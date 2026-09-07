//! copy-propagation — fold single-def (SSA-style) constant and copy
//! variables into their reads, the shIR equivalent of the stalled
//! proposal estree-20260813-183713 (a1-ssa-const-copy-prop). The GLSL
//! backend's own fragment output shows the shape every backend has:
//!
//!     g_fx = g_frag_x;                    // copy, never re-assigned
//!     g_hash = ((g_fx) * (7)) + ((g_fy) * (13));
//!     g_corrupt = ... g_hash ...;         // hash is a dead intermediate
//!
//! ## Need
//! The renderers are literal statement→code translators with zero value
//! analysis; copy lines, constant re-assignments and never-read
//! intermediates survive into the emitted code. The shIR already
//! computes the metadata this needs (`analyze_var_const` / `var_lifetimes`
//! in shir.rs — a `Const` var = exactly one static assignment, not in a
//! loop/function body, no runtime-store/arith/index/eval writes) but no
//! backend consumes it. This pass is a consumer, not new analysis — one
//! A1 fold fixes all nine renderers.
//!
//! ## Scope — the sound (dominance) version
//! A variable is folded into its reads only when:
//!   - it is ASSIGNED EXACTLY ONCE in the whole program (a string
//!     read-before-write is the shell empty-string default — only a
//!     single static def makes the value unambiguous), AND
//!   - that def is a TOP-LEVEL `Assign`/`Declare` to a literal
//!     (`Int`/`Str`/`Bool`) or a copy (`Var` of an equally-foldable
//!     var) — no arith/index/capture, which the const check already
//!     rejects, AND
//!   - the def textually precedes every read in the same statement
//!     list (a read before the def observes the unset empty-string
//!     default, never the literal), with NO intervening
//!     `Capture`/`Subshell`/`Background` (those may execute before the
//!     def, out of program order).
//! Ordering within a straight-line block is preserved (def before read
//! in the same `Vec<IrStmt>`), so an early read refuses the fold.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Composes
//! with dead-store-elim (a folded-away variable's def becomes a dead
//! store) and i32-provable (a folded literal becomes a constant leaf).

use std::collections::HashMap;
use std::collections::HashSet;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// A foldable single-def literal value (`Int`/`Str`/`Bool`).
#[derive(Clone)]
enum Def {
    Literal(IrExpr),
}

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // single-def table: var → its single def (if qualifies)
    let defs = collect_defs(stmts);
    if defs.is_empty() {
        return false;
    }
    // resolve the fold set: every single-def literal var. A copy def
    // (x=y) isn't itself foldable, but its RHS read-of-y still folds in
    // place when y is a literal (handled by fold_expr's Var arm).
    let mut resolved: HashMap<String, IrExpr> = HashMap::new();
    for (v, d) in defs.iter() {
        let Def::Literal(e) = d;
        resolved.insert(v.clone(), e.clone());
    }
    // def statement index per var — a read folds only AFTER its def has
    // executed (a read before the def sees the unset empty-string
    // default, never the literal).
    let def_at: HashMap<String, usize> = defs
        .keys()
        .filter_map(|v| def_index(stmts, v).map(|i| (v.clone(), i)))
        .collect();
    let mut changed = false;
    for (i, st) in stmts.iter_mut().enumerate() {
        let foldable: HashSet<String> = def_at
            .iter()
            .filter(|(_, &di)| di < i)
            .map(|(v, _)| v.clone())
            .collect();
        // fold reads within each top-level statement; the def-before-read
        // ordering is enforced per statement list
        changed |= fold_stmt(st, &defs, &resolved, &foldable);
    }
    changed
}

/// Single-def scan — a var qualifies iff written exactly once anywhere
/// in the program (nested bodies, builtin writes, arith writes and index
/// writes included: any second write makes the value ambiguous) and that
/// single write is a top-level literal/copy `Assign`/`Declare`. A
/// dynamic `eval`/`source`/`.` anywhere disqualifies everything.
fn collect_defs(stmts: &[IrStmt]) -> HashMap<String, Def> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut values: HashMap<String, (IrExpr, bool)> = HashMap::new();
    let mut dynamic = false;
    count_assigns(stmts, &mut counts, &mut values, &mut dynamic);
    if dynamic {
        return HashMap::new();
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c == 1)
        .filter_map(|(v, _)| {
            let (e, top) = values.get(&v)?;
            if !*top {
                return None; // nested/conditional/builtin write — not foldable
            }
            match e {
                IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Bool(_) => {
                    Some((v, Def::Literal(e.clone())))
                }
                // a copy def (x=y) isn't itself a literal — its reads stay
                // unfolded (conservative; the RHS read-of-y still folds in
                // place when y is a literal)
                _ => None,
            }
        })
        .collect()
}

fn count_assigns(
    stmts: &[IrStmt],
    counts: &mut HashMap<String, usize>,
    values: &mut HashMap<String, (IrExpr, bool)>,
    dynamic: &mut bool,
) {
    for st in stmts {
        count_stmt_assigns(st, counts, values, true, dynamic);
    }
}

fn site(
    counts: &mut HashMap<String, usize>,
    values: &mut HashMap<String, (IrExpr, bool)>,
    var: &str,
    expr: &IrExpr,
    top: bool,
) {
    *counts.entry(var.to_string()).or_insert(0) += 1;
    values.insert(var.to_string(), (expr.clone(), top));
}

fn count_stmt_assigns(
    st: &IrStmt,
    counts: &mut HashMap<String, usize>,
    values: &mut HashMap<String, (IrExpr, bool)>,
    top: bool,
    dynamic: &mut bool,
) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                if t.indices.is_empty() && !t.var.contains('[') {
                    site(counts, values, &t.var, expr, top);
                } else {
                    // array-element write — the base name is written
                    let base = t.var.split('[').next().unwrap_or(&t.var).to_string();
                    site(counts, values, &base, expr, false);
                }
            }
            if let IrExpr::Arith(a) = expr {
                for w in arith_written_vars(a) {
                    site(counts, values, &w, expr, false);
                }
            }
        }
        IrStmt::Declare { vars, init, .. } => {
            if let Some(i) = init {
                for v in vars {
                    site(counts, values, &v.name, i, top);
                }
            } else {
                for v in vars {
                    site(
                        counts,
                        values,
                        &v.name,
                        &IrExpr::Str(String::new(), crate::ir::StrStyle::DoubleQuoted),
                        top,
                    );
                }
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            site(counts, values, var, &IrExpr::Array(vec![]), top);
            for el in elements {
                count_expr_assigns(el, counts, values, top, dynamic);
            }
        }
        IrStmt::Expr(e) => count_expr_assigns(e, counts, values, top, dynamic),
        IrStmt::Exec { cmd, args, env, redirects, .. } => {
            classify_builtin_writes(cmd, args, counts, values, dynamic);
            count_expr_assigns(cmd, counts, values, top, dynamic);
            for a in args {
                count_expr_assigns(a, counts, values, top, dynamic);
            }
            for (_, e) in env {
                count_expr_assigns(e, counts, values, top, dynamic);
            }
            // Exec redirects are bare exprs (Vec<IrExpr>)
            for r in redirects {
                count_expr_assigns(r, counts, values, top, dynamic);
            }
        }
        IrStmt::If { then, elsifs, else_, .. } => {
            for s in then {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
            for (_, b) in elsifs {
                for s in b {
                    count_stmt_assigns(s, counts, values, false, dynamic);
                }
            }
            for s in else_ {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            for s in body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        IrStmt::For { var, iter, body, .. } => {
            // the loop header WRITES var every iteration
            site(counts, values, var, &IrExpr::Bool(false), false);
            count_expr_assigns(iter, counts, values, false, dynamic);
            for s in body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                for s in &c.body {
                    count_stmt_assigns(s, counts, values, false, dynamic);
                }
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
            for s in b {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            for s in inner {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
            for r in redirects {
                count_expr_assigns(&r.target, counts, values, top, dynamic);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                for st in s {
                    count_stmt_assigns(st, counts, values, false, dynamic);
                }
            }
        }
        IrStmt::ForInit { init, cond, step, body, .. } => {
            // init runs once, in order; cond/step/body repeat or branch
            for s in init {
                count_stmt_assigns(s, counts, values, top, dynamic);
            }
            count_expr_assigns(cond, counts, values, false, dynamic);
            for s in step {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
            for s in body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        IrStmt::Select { clauses, .. } => {
            for c in clauses {
                // a recv target receives a value — a conditional write
                if let Some(t) = &c.target {
                    site(counts, values, t, &IrExpr::Bool(false), false);
                }
                if let Some(ch) = &c.ch {
                    count_expr_assigns(ch, counts, values, false, dynamic);
                }
                if let Some(v) = &c.value {
                    count_expr_assigns(v, counts, values, false, dynamic);
                }
                for s in &c.body {
                    count_stmt_assigns(s, counts, values, false, dynamic);
                }
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
            for s in body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
            for e in excepts {
                // the exception binding is a conditional write
                if let Some(n) = &e.as_name {
                    site(counts, values, n, &IrExpr::Bool(false), false);
                }
                if let Some(m) = &e.match_expr {
                    count_expr_assigns(m, counts, values, false, dynamic);
                }
                for s in &e.body {
                    count_stmt_assigns(s, counts, values, false, dynamic);
                }
            }
            for s in else_body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
            for s in finally_body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        // leaf statements with inline value exprs — scan for nested
        // writes (setVar calls, Arrow/Lambda bodies).
        IrStmt::Output { value, .. } => {
            count_expr_assigns(value, counts, values, top, dynamic);
        }
        IrStmt::WriteFile { path, content, .. } => {
            count_expr_assigns(path, counts, values, top, dynamic);
            count_expr_assigns(content, counts, values, top, dynamic);
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            if let Some(x) = e {
                count_expr_assigns(x, counts, values, top, dynamic);
            }
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
            count_expr_assigns(expr, counts, values, top, dynamic);
        }
        IrStmt::SetChildError(e) => {
            count_expr_assigns(e, counts, values, top, dynamic);
        }
        _ => {}
    }
}

/// Write builtins: declaration-with-assignment (`typeset -i n=42`,
/// `declare -a arr=(…)`, `local x=5`, `export FOO=bar`, `readonly r=…`),
/// store writes (`read`/`readarray`/`mapfile`/`unset`), `let` arith
/// strings, and the dynamic `eval`/`source`/`.` (writes everything).
fn classify_builtin_writes(
    cmd: &IrExpr,
    args: &[IrExpr],
    counts: &mut HashMap<String, usize>,
    values: &mut HashMap<String, (IrExpr, bool)>,
    dynamic: &mut bool,
) {
    let cname = match cmd {
        IrExpr::Str(s, _) | IrExpr::Ident(s) => s.as_str(),
        _ => return,
    };
    if matches!(cname, "eval" | "source" | ".") {
        *dynamic = true;
        return;
    }
    if matches!(
        cname,
        "typeset" | "declare" | "local" | "export" | "readonly" | "read" | "readarray"
            | "mapfile" | "unset"
    ) {
        for a in args {
            if let IrExpr::Str(s, _) = a {
                let s = s.trim_start_matches('-');
                if s.is_empty() || (s.chars().all(|c| c.is_ascii_alphabetic()) && s.len() <= 2) {
                    continue; // a flag like -i/-r/-x/-a/-A/-l/-u/-n/-g/-t/-p/-f/-F
                }
                let name = s.split('=').next().unwrap_or(s);
                if !name.is_empty() {
                    site(
                        counts,
                        values,
                        name,
                        &IrExpr::Str(s.to_string(), crate::ir::StrStyle::DoubleQuoted),
                        false,
                    );
                }
            }
        }
        return;
    }
    if cname == "let" {
        for a in args {
            if let IrExpr::Str(s, _) = a {
                for v in str_maybe_read_vars(s) {
                    site(
                        counts,
                        values,
                        &v,
                        &IrExpr::Str(s.clone(), crate::ir::StrStyle::DoubleQuoted),
                        false,
                    );
                }
            }
        }
    }
}

/// Variables written by a native-arith expression (`x++`, `((x=1))`,
/// `$((x+=1))`).
fn arith_written_vars(a: &ArithAst) -> Vec<String> {
    let mut out = Vec::new();
    arith_written_vars_into(a, &mut out);
    out
}

fn arith_written_vars_into(a: &ArithAst, out: &mut Vec<String>) {
    match a {
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => out.push(var.clone()),
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_written_vars_into(lhs, out);
            arith_written_vars_into(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_written_vars_into(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_written_vars_into(test, out);
            arith_written_vars_into(then, out);
            arith_written_vars_into(else_, out);
        }
        ArithAst::Index { key, .. } => arith_written_vars_into(key, out),
        ArithAst::Cast { arg, .. } => arith_written_vars_into(arg, out),
        _ => {}
    }
}

fn count_expr_assigns(
    e: &IrExpr,
    counts: &mut HashMap<String, usize>,
    values: &mut HashMap<String, (IrExpr, bool)>,
    top: bool,
    dynamic: &mut bool,
) {
    match e {
        IrExpr::Lambda { body, .. } => {
            // a lambda body may run zero or many times — never top-level
            for s in body {
                count_stmt_assigns(s, counts, values, false, dynamic);
            }
        }
        IrExpr::ArrayComp { var, iter, elem, cond, .. } => {
            // the comprehension var is written every iteration
            site(counts, values, var, &IrExpr::Bool(false), false);
            count_expr_assigns(iter, counts, values, false, dynamic);
            count_expr_assigns(elem, counts, values, false, dynamic);
            if let Some(c) = cond {
                count_expr_assigns(c, counts, values, false, dynamic);
            }
        }
        IrExpr::Splice(e) => count_expr_assigns(e, counts, values, top, dynamic),
        IrExpr::Arith(a) => {
            // native-arith writes (`x++`, `((x=1))`) nested in an
            // expression are real writes
            for w in arith_written_vars(a) {
                site(counts, values, &w, e, false);
            }
        }
        IrExpr::Arrow(body) => {
            for s in body {
                count_stmt_assigns(s, counts, values, top, dynamic);
            }
        }
        IrExpr::Call { func, args, .. } => {
            if matches!(func.as_str(), "setVar" | "setArray" | "setArrayAppend" | "assign") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    site(counts, values, n, &IrExpr::Bool(false), false);
                }
            }
            if matches!(func.as_str(), "exec" | "builtin") {
                // shape: [cmd, Array(arg_list)]
                if let Some(IrExpr::Array(arg_list)) = args.get(1) {
                    classify_builtin_writes(&args[0], arg_list, counts, values, dynamic);
                }
            }
            for a in args {
                count_expr_assigns(a, counts, values, top, dynamic);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            count_expr_assigns(obj, counts, values, top, dynamic);
            for a in args {
                count_expr_assigns(a, counts, values, top, dynamic);
            }
        }
        IrExpr::Array(elems) => {
            for el in elems {
                count_expr_assigns(el, counts, values, top, dynamic);
            }
        }
        IrExpr::Object(fields) => {
            for (_, v) in fields {
                count_expr_assigns(v, counts, values, top, dynamic);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            count_expr_assigns(lhs, counts, values, top, dynamic);
            count_expr_assigns(rhs, counts, values, top, dynamic);
        }
        IrExpr::Ternary { cond, then, else_, .. } => {
            count_expr_assigns(cond, counts, values, top, dynamic);
            count_expr_assigns(then, counts, values, top, dynamic);
            count_expr_assigns(else_, counts, values, top, dynamic);
        }
        IrExpr::DefinedOr { expr, default, .. } => {
            count_expr_assigns(expr, counts, values, top, dynamic);
            count_expr_assigns(default, counts, values, top, dynamic);
        }
        IrExpr::Index { key, .. } | IrExpr::Capture { expr: key, .. } => {
            count_expr_assigns(key, counts, values, top, dynamic);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    count_expr_assigns(x, counts, values, top, dynamic);
                }
            }
        }
        _ => {}
    }
}

/// Does the string reference `var` as a standalone token (bash arith/test
/// strings reference vars by bare name, `$i` or `i`)?
fn str_maybe_read_vars(s: &str) -> Vec<String> {
    // conservative: every `$name` / bare alphanumeric token is a
    // potential var read (the runtime evaluates the string)
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(pos) = rest.find('$') {
        let after = &rest[pos + 1..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
        rest = &rest[pos + 1..];
    }
    out
}

/// Index of the top-level statement that defines `var` (its single def).
fn def_index(stmts: &[IrStmt], var: &str) -> Option<usize> {
    stmts.iter().position(|st| match st {
        IrStmt::Assign { targets, .. } => {
            targets.iter().any(|t| t.var == var && t.indices.is_empty())
        }
        IrStmt::Declare { vars, .. } => vars.iter().any(|v| v.name == var),
        _ => false,
    })
}

fn fold_stmt(
    st: &mut IrStmt,
    defs: &HashMap<String, Def>,
    resolved: &HashMap<String, IrExpr>,
    foldable: &HashSet<String>,
) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => fold_expr(expr, defs, resolved, foldable),
        IrStmt::Output { value, .. } => fold_expr(value, defs, resolved, foldable),
        IrStmt::WriteFile { path, content, .. } => {
            fold_expr(path, defs, resolved, foldable)
                | fold_expr(content, defs, resolved, foldable)
        }
        IrStmt::Declare { init, .. } => {
            init.as_mut().map(|i| fold_expr(i, defs, resolved, foldable)).unwrap_or(false)
        }
        IrStmt::DeclareArray { elements, .. } => {
            elements.iter_mut().any(|e| fold_expr(e, defs, resolved, foldable))
        }
        IrStmt::Expr(e) => fold_expr(e, defs, resolved, foldable),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = fold_expr(cond, defs, resolved, foldable);
            for s in then.iter_mut() {
                c |= fold_stmt(s, defs, resolved, foldable);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= fold_expr(ec, defs, resolved, foldable);
                for s in eb.iter_mut() {
                    c |= fold_stmt(s, defs, resolved, foldable);
                }
            }
            for s in else_.iter_mut() {
                c |= fold_stmt(s, defs, resolved, foldable);
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = fold_expr(cond, defs, resolved, foldable);
            for s in body.iter_mut() {
                c |= fold_stmt(s, defs, resolved, foldable);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= fold_stmt(s, defs, resolved, foldable);
            }
            c |= fold_expr(cond, defs, resolved, foldable);
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = fold_expr(iter, defs, resolved, foldable);
            for s in body.iter_mut() {
                c |= fold_stmt(s, defs, resolved, foldable);
            }
            c
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            let mut c = fold_expr(cmd, defs, resolved, foldable);
            for a in args.iter_mut() {
                c |= fold_expr(a, defs, resolved, foldable);
            }
            for r in redirects.iter_mut() {
                c |= fold_expr(r, defs, resolved, foldable);
            }
            for (_, e) in env.iter_mut() {
                c |= fold_expr(e, defs, resolved, foldable);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            stages.iter_mut().any(|s| s.iter_mut().any(|st| fold_stmt(st, defs, resolved, foldable)))
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            e.as_mut().map(|e| fold_expr(e, defs, resolved, foldable)).unwrap_or(false)
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } | IrStmt::SetChildError(expr) => {
            fold_expr(expr, defs, resolved, foldable)
        }
        IrStmt::Case { discriminant, clauses, .. } => {
            let mut c = fold_expr(discriminant, defs, resolved, foldable);
            for cl in clauses.iter_mut() {
                for s in cl.body.iter_mut() {
                    c |= fold_stmt(s, defs, resolved, foldable);
                }
            }
            c
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            let mut c = inner.iter_mut().any(|s| fold_stmt(s, defs, resolved, foldable));
            for r in redirects.iter_mut() {
                c |= fold_expr(&mut r.target, defs, resolved, foldable);
            }
            c
        }
        IrStmt::Function { body, .. } => {
            body.iter_mut().any(|s| fold_stmt(s, defs, resolved, foldable))
        }
        // Subshell/Background may execute out of program order (a
        // background job can run before the def) — never fold inside.
        IrStmt::Subshell(_) | IrStmt::Background(_) => false,
        IrStmt::Block(b) => b.iter_mut().any(|s| fold_stmt(s, defs, resolved, foldable)),
        _ => false,
    }
}

fn fold_expr(
    e: &mut IrExpr,
    defs: &HashMap<String, Def>,
    resolved: &HashMap<String, IrExpr>,
    foldable: &HashSet<String>,
) -> bool {
    let mut c = false;
    match e {
        IrExpr::Var(_, _) => {
            // peek the name without holding a borrow across the write
            let name = if let IrExpr::Var(v, _) = e { v.clone() } else { unreachable!() };
            if foldable.contains(&name) {
                if let Some(rep) = resolved.get(&name) {
                    *e = rep.clone();
                    c = true;
                }
            }
            let _ = defs;
        }
        IrExpr::Ident(_) => {}
        IrExpr::Index { var, key } => {
            c |= fold_expr(key, defs, resolved, foldable);
            let _ = var;
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            c |= fold_expr(lhs, defs, resolved, foldable);
            c |= fold_expr(rhs, defs, resolved, foldable);
        }
        IrExpr::Arith(a) => c |= fold_arith(a, defs, resolved, foldable),
        IrExpr::Call { func, args } => {
            if matches!(func.as_str(), "getVar" | "arrayIndex") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if foldable.contains(n) {
                        if let Some(rep) = resolved.get(n) {
                            // replace getVar("x") with the literal — the name
                            // string goes away entirely
                            *e = rep.clone();
                            c = true;
                            return c;
                        }
                    }
                }
            }
            for a in args.iter_mut() {
                c |= fold_expr(a, defs, resolved, foldable);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            c |= fold_expr(obj, defs, resolved, foldable);
            for a in args.iter_mut() {
                c |= fold_expr(a, defs, resolved, foldable);
            }
        }
        IrExpr::Ternary { cond, then, else_, .. } => {
            c |= fold_expr(cond, defs, resolved, foldable);
            c |= fold_expr(then, defs, resolved, foldable);
            c |= fold_expr(else_, defs, resolved, foldable);
        }
        IrExpr::DefinedOr { expr, default, .. } => {
            c |= fold_expr(expr, defs, resolved, foldable);
            c |= fold_expr(default, defs, resolved, foldable);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    c |= fold_expr(x, defs, resolved, foldable);
                }
            }
        }
        IrExpr::Capture { expr, .. } => {
            // a capture may run before the def — refuse to fold inside
            c |= false;
            let _ = expr;
        }
        IrExpr::Arrow(body) => {
            for s in body.iter_mut() {
                c |= fold_stmt(s, defs, resolved, foldable);
            }
        }
        IrExpr::Array(elems) => {
            for e in elems.iter_mut() {
                c |= fold_expr(e, defs, resolved, foldable);
            }
        }
        IrExpr::Object(fields) => {
            for (_, e) in fields.iter_mut() {
                c |= fold_expr(e, defs, resolved, foldable);
            }
        }
        _ => {}
    }
    c
}

fn fold_arith(
    a: &mut ArithAst,
    defs: &HashMap<String, Def>,
    resolved: &HashMap<String, IrExpr>,
    foldable: &HashSet<String>,
) -> bool {
    match a {
        ArithAst::Var(v) => {
            if foldable.contains(v) {
                if let Some(rep) = resolved.get(v) {
                    if let IrExpr::Int(n) = rep {
                        *a = ArithAst::Num(*n);
                        return true;
                    }
                }
            }
            let _ = (defs, foldable);
            false
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            fold_arith(lhs, defs, resolved, foldable) | fold_arith(rhs, defs, resolved, foldable)
        }
        ArithAst::Un { arg, .. } => fold_arith(arg, defs, resolved, foldable),
        ArithAst::Cond { test, then, else_, .. } => {
            fold_arith(test, defs, resolved, foldable)
                | fold_arith(then, defs, resolved, foldable)
                | fold_arith(else_, defs, resolved, foldable)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn getvar(var: &str) -> IrExpr {
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![IrExpr::Str(var.to_string(), StrStyle::DoubleQuoted)],
        }
    }

    fn str_lit(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn int_lit(n: i64) -> IrExpr {
        IrExpr::Int(n)
    }

    fn expr_stmt(e: IrExpr) -> IrStmt {
        IrStmt::Expr(e)
    }

    /// `x=5; echo $x` → the read folds to the literal.
    #[test]
    fn folds_read_after_def() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            expr_stmt(getvar("x")),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts[1], expr_stmt(int_lit(5)));
    }

    /// `echo $x; x=5` — the read BEFORE the def must NOT fold (it sees
    /// the unset empty-string default).
    #[test]
    fn refuses_read_before_def() {
        let mut stmts = vec![
            expr_stmt(getvar("x")),
            assign("x", int_lit(5)),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], expr_stmt(getvar("x")));
    }

    /// `x=5; for i in …; do a=$x; done` — the loop-body read folds (x is
    /// never reassigned anywhere, nested bodies included).
    #[test]
    fn folds_loop_body_read() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            IrStmt::For {
                var: "i".to_string(),
                iter: IrExpr::Array(vec![]),
                body: vec![assign("a", getvar("x"))],
            },
        ];
        assert!(transform(&mut stmts));
        if let IrStmt::For { body, .. } = &stmts[1] {
            assert_eq!(body[0], assign("a", int_lit(5)));
        } else {
            panic!("expected For");
        }
    }

    /// `x=5; for i in …; do x=$i; done` — the loop-body WRITE makes x
    /// multi-def; the read must NOT fold.
    #[test]
    fn refuses_loop_body_write() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            IrStmt::For {
                var: "i".to_string(),
                iter: IrExpr::Array(vec![]),
                body: vec![assign("x", getvar("i"))],
            },
        ];
        assert!(!transform(&mut stmts));
        if let IrStmt::For { body, .. } = &stmts[1] {
            assert_eq!(body[0], assign("x", getvar("i")));
        } else {
            panic!("expected For");
        }
    }

    /// `x=5; for x in a b; do echo $x; done` — the loop-HEADER write
    /// makes x multi-def; the body read must NOT fold (it observes a,
    /// b — never the pre-loop literal).
    #[test]
    fn refuses_loop_header_write() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            IrStmt::For {
                var: "x".to_string(),
                iter: IrExpr::Array(vec![]),
                body: vec![expr_stmt(getvar("x"))],
            },
        ];
        assert!(!transform(&mut stmts));
        if let IrStmt::For { body, .. } = &stmts[1] {
            assert_eq!(body[0], expr_stmt(getvar("x")));
        } else {
            panic!("expected For");
        }
    }

    /// `x=5; echo $x $x` — both reads fold (the foldable set is not a
    /// one-shot seen set).
    #[test]
    fn folds_both_reads_in_one_stmt() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            expr_stmt(IrExpr::Call {
                func: "echo".to_string(),
                args: vec![getvar("x"), getvar("x")],
            }),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(
            stmts[1],
            expr_stmt(IrExpr::Call {
                func: "echo".to_string(),
                args: vec![int_lit(5), int_lit(5)],
            })
        );
    }

    /// `x=5; (echo $x) &` — a background job may run before the def;
    /// reads inside it must NOT fold.
    #[test]
    fn refuses_background_read() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            IrStmt::Background(vec![expr_stmt(getvar("x"))]),
        ];
        assert!(!transform(&mut stmts));
        if let IrStmt::Background(b) = &stmts[1] {
            assert_eq!(b[0], expr_stmt(getvar("x")));
        } else {
            panic!("expected Background");
        }
    }

    /// `x=5; y=$x; echo $y` — the copy `y=$x` folds to `y=5`; `echo $y`
    /// stays (y's def is a copy, not a literal — conservative).
    #[test]
    fn folds_copy_rhs() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign("y", IrExpr::Var("x".to_string(), None)),
            expr_stmt(getvar("y")),
        ];
        assert!(transform(&mut stmts));
        // the copy `y = x` folds to `y = 5` (its RHS read of literal x)
        assert_eq!(stmts[1], assign("y", int_lit(5)));
        // y is a copy def — not a foldable literal — so its read stays
        assert_eq!(stmts[2], expr_stmt(getvar("y")));
    }

    /// `x=5; x=6; echo $x` — two defs → not foldable.
    #[test]
    fn refuses_multi_def() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign("x", int_lit(6)),
            expr_stmt(getvar("x")),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[2], expr_stmt(getvar("x")));
    }

    /// `x=5; if c; then x=6; fi; echo $x` — the nested write makes x
    /// multi-def → not foldable.
    #[test]
    fn refuses_nested_write() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            IrStmt::If {
                cond: getvar("c"),
                then: vec![assign("x", int_lit(6))],
                elsifs: vec![],
                else_: vec![],
            },
            expr_stmt(getvar("x")),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[2], expr_stmt(getvar("x")));
    }

    /// `x=5; echo $x` with a string literal def — folds.
    #[test]
    fn folds_string_def() {
        let mut stmts = vec![
            assign("x", str_lit("hello")),
            expr_stmt(getvar("x")),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts[1], expr_stmt(str_lit("hello")));
    }

    /// `typeset -l lc="HELLO WORLD"; lc="ANOTHER TEST"; echo $lc` — the
    /// typeset builtin WRITES lc (with a transformation), so lc is
    /// multi-write; the read must NOT fold to the literal (the actual
    /// value is lowercased by the attribute).
    #[test]
    fn refuses_builtin_write() {
        let mut stmts = vec![
            expr_stmt(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![
                    str_lit("typeset"),
                    IrExpr::Array(vec![str_lit("-l"), str_lit("lc=HELLO WORLD")]),
                ],
            }),
            assign("lc", str_lit("ANOTHER TEST")),
            expr_stmt(getvar("lc")),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[2], expr_stmt(getvar("lc")));
    }

    /// `count=0; pipeline(Arrow([count=$((count+1))])); echo $count` — a
    /// write inside a pipeline Arrow body is a real assignment site; the
    /// read must NOT fold (091_while_pipe_var shape).
    #[test]
    fn refuses_arrow_body_write() {
        let mut stmts = vec![
            assign("count", int_lit(0)),
            expr_stmt(IrExpr::Call {
                func: "pipeline".to_string(),
                args: vec![IrExpr::Array(vec![IrExpr::Arrow(vec![assign(
                    "count",
                    IrExpr::Arith(Box::new(ArithAst::Bin {
                        op: "+".to_string(),
                        lhs: Box::new(ArithAst::Var("count".to_string())),
                        rhs: Box::new(ArithAst::Num(1)),
                    })),
                )])])],
            }),
            expr_stmt(getvar("count")),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[2], expr_stmt(getvar("count")));
    }

    /// `x=5; eval "y=1"; echo $x` — a dynamic eval disqualifies
    /// everything.
    #[test]
    fn refuses_dynamic_eval() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            expr_stmt(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![str_lit("eval"), IrExpr::Array(vec![str_lit("y=1")])],
            }),
            expr_stmt(getvar("x")),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[2], expr_stmt(getvar("x")));
    }

    /// `x=5; x++` (arith IncDec write) — the second write disqualifies.
    #[test]
    fn refuses_arith_incdec_write() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign("x", IrExpr::Arith(Box::new(ArithAst::IncDec {
                var: "x".to_string(),
                delta: 1,
                prefix: true,
            }))),
            expr_stmt(getvar("x")),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[2], expr_stmt(getvar("x")));
    }
}
