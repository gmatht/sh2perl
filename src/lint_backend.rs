//! `.lint` backend — a linter, not a code generator.
//!
//! Unlike the other backends (which render `IrProgram` to `<lang>`
//! source), this one renders the **analysis verdicts** to a diagnostic
//! report. It is the natural home for the lints our existing passes make
//! cheap:
//!
//! - **use-before-declared** — [`crate::shir_passes::used_before_decl`]
//!   (the forward must-defined dataflow; the check behind PowerShell's
//!   `Set-StrictMode` undeclared-variable error).
//! - **unused variable** — assigned/declared but never read anywhere. A
//!   single set-difference over the read/write classification the
//!   use-before-decl walk already maintains (no dataflow needed).
//! - **unused function** — defined but never called.
//! - **const-candidate** — a variable `ConstVar` proved is assigned once
//!   and never reassigned (suggest `readonly`/`const`).
//! - **informational** — `set -e` may be enabled (`errexit_may_enable`),
//!   and mutually-recursive clusters (`FunctionScc`).
//!
//! The renderer runs ONLY the canonical *analyses* (not the transforms —
//! it has no code to generate, so mutating the IR is pointless) and then
//! aggregates. Output is deterministic and sorted so it is diff-stable.
//!
//! Surface it via `shir_render --target lint -` (or `otranspilerl-cli x.sh
//! --target lint`). The report is plain text on stdout; pipe through
//! `jq` if you want machine form (a future `--lint-json` flag can emit
//! `ctx`'s serialized verdicts directly).

use std::collections::{HashMap, HashSet};

use crate::ir::{IrExpr, IrProgram, IrStmt, VarKind};
use crate::shir_passes::{PassContext, Pipeline};
use crate::shir_passes::used_before_decl::collect_var_defs_reads;

/// Render the analysis verdicts as a lint report. Empty string ⇒ clean.
pub fn shir_to_lint(prog: &IrProgram) -> String {
    // Run every registered analysis (including use_before_decl) on a
    // clone-free basis: analyses are pure and never mutate `prog`.
    let mut ctx = PassContext::default();
    for a in &Pipeline::canonical().analyses {
        a.run(prog, &mut ctx);
    }

    let mut lines: Vec<String> = Vec::new();

    // ── use-before-declared ────────────────────────────────────────────
    for f in &ctx.use_before_decl {
        lines.push(format!(
            "use-before-decl: `${}` read at statement {} before any definition on the reaching path",
            f.var, f.stmt_pos
        ));
    }

    // ── unused variable (assigned, never read) ────────────────────────
    let (defs, reads) = collect_var_defs_reads(prog);
    let mut unused: Vec<&String> = defs.difference(&reads).collect();
    unused.sort();
    for v in unused {
        lines.push(format!("unused-var: `${v}` is assigned/declared but never read"));
    }

    // ── unused function (defined, never called) ───────────────────────
    let (defined_funcs, called_funcs) = collect_function_defs_calls(prog);
    let mut unused_fns: Vec<&String> = defined_funcs.difference(&called_funcs).collect();
    unused_fns.sort();
    for f in unused_fns {
        lines.push(format!("unused-function: `{f}()` is defined but never called"));
    }

    // ── const-candidate (single static assignment) ────────────────────
    let mut consts: Vec<&String> = ctx
        .const_vars
        .iter()
        .filter(|(_, k)| **k == VarKind::Const)
        .map(|(n, _)| n)
        .collect();
    consts.sort();
    for v in consts {
        lines.push(format!(
            "const-candidate: `${v}` is assigned exactly once; mark `readonly`/`const`"
        ));
    }

    // ── dead store (assigned, then overwritten before any read) ──────
    for (v, p) in collect_dead_stores(prog) {
        lines.push(format!(
            "dead-store: `${v}` assigned at statement {p} is never read before being overwritten"
        ));
    }

    // ── unreachable code (after return/exit/die/break/continue) ─────
    for p in collect_unreachable_code(prog) {
        lines.push(format!(
            "unreachable-code: statement {p} is unreachable (after return/exit/die/break/continue)"
        ));
    }

    // ── discarded command substitution (computed, result unused) ────
    for p in collect_discarded_captures(prog) {
        lines.push(format!(
            "discarded-capture: command-substitution result at statement {p} is computed but not used"
        ));
    }

    // ── informational verdicts ─────────────────────────────────────────
    for scc in &ctx.function_sccs {
        if scc.len() > 1 {
            let mut names: Vec<&String> = scc.iter().collect();
            names.sort();
            lines.push(format!(
                "info: mutually-recursive cluster: {}",
                names
                    .iter()
                    .map(|n| format!("`{n}()`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    lines.sort();
    lines.join("\n") + if lines.is_empty() { "" } else { "\n" }
}

/// Collect function definitions (`IrStmt::Function{name}`) and call sites
/// (`fnCall("name", …)` in the ShIR runtime-call convention). Returns
/// (defined, called) name sets. A function is "unused" when it is defined
/// but its name never appears as a `fnCall` target.
pub fn collect_function_defs_calls(prog: &IrProgram) -> (HashSet<String>, HashSet<String>) {
    let mut defined = HashSet::new();
    let mut called = HashSet::new();
    let mut stmts: Vec<&[IrStmt]> = vec![&prog.stmts];
    for sub in &prog.subs {
        stmts.push(&sub.body);
    }
    for s in &stmts {
        walk_func_defs_calls_stmts(s, &mut defined, &mut called);
    }
    (defined, called)
}

fn walk_func_defs_calls_stmts(stmts: &[IrStmt], defined: &mut HashSet<String>, called: &mut HashSet<String>) {
    for st in stmts {
        walk_func_defs_calls_stmt(st, defined, called);
    }
}

fn walk_func_defs_calls_stmt(st: &IrStmt, defined: &mut HashSet<String>, called: &mut HashSet<String>) {
    match st {
        IrStmt::Function { name, body, named_blocks } => {
            defined.insert(name.clone());
            walk_func_defs_calls_stmts(body, defined, called);
            for (_, nb) in named_blocks {
                walk_func_defs_calls_stmts(nb, defined, called);
            }
        }
        IrStmt::Expr(e) => walk_func_calls_expr(e, defined, called),
        // a few statement shapes carry a call expression worth scanning
        IrStmt::Output { value, .. } => walk_func_calls_expr(value, defined, called),
        IrStmt::WriteFile { path, content, .. } => {
            walk_func_calls_expr(path, defined, called);
            walk_func_calls_expr(content, defined, called);
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            walk_func_calls_expr(cmd, defined, called);
            for a in args {
                walk_func_calls_expr(a, defined, called);
            }
            for r in redirects {
                walk_func_calls_expr(r, defined, called);
            }
            for (_, v) in env {
                walk_func_calls_expr(v, defined, called);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            walk_func_calls_expr(cond, defined, called);
            walk_func_defs_calls_stmts(then, defined, called);
            for (ec, eb) in elsifs {
                walk_func_calls_expr(ec, defined, called);
                walk_func_defs_calls_stmts(eb, defined, called);
            }
            walk_func_defs_calls_stmts(else_, defined, called);
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            walk_func_calls_expr(discriminant, defined, called);
            for cl in clauses {
                walk_func_defs_calls_stmts(&cl.body, defined, called);
            }
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            walk_func_calls_expr(cond, defined, called);
            walk_func_defs_calls_stmts(body, defined, called);
        }
        IrStmt::For { iter, body, .. } => {
            walk_func_calls_expr(iter, defined, called);
            walk_func_defs_calls_stmts(body, defined, called);
        }
        IrStmt::ForInit { init, cond, step, body } => {
            walk_func_defs_calls_stmts(init, defined, called);
            walk_func_calls_expr(cond, defined, called);
            walk_func_defs_calls_stmts(step, defined, called);
            walk_func_defs_calls_stmts(body, defined, called);
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            walk_func_defs_calls_stmts(body, defined, called);
            for ex in excepts {
                if let Some(m) = &ex.match_expr {
                    walk_func_calls_expr(m, defined, called);
                }
                walk_func_defs_calls_stmts(&ex.body, defined, called);
            }
            walk_func_defs_calls_stmts(else_body, defined, called);
            walk_func_defs_calls_stmts(finally_body, defined, called);
        }
        IrStmt::Select { clauses } => {
            for cl in clauses {
                if let Some(ch) = &cl.ch {
                    walk_func_calls_expr(ch, defined, called);
                }
                if let Some(v) = &cl.value {
                    walk_func_calls_expr(v, defined, called);
                }
                walk_func_defs_calls_stmts(&cl.body, defined, called);
            }
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            walk_func_defs_calls_stmts(b, defined, called);
        }
        IrStmt::Pipeline { stages, .. } => {
            for stg in stages {
                walk_func_defs_calls_stmts(stg, defined, called);
            }
        }
        IrStmt::Assign { expr, .. }
        | IrStmt::Declare { init: Some(expr), .. }
        | IrStmt::Die { expr, .. }
        | IrStmt::Warn { expr, .. }
        | IrStmt::SetChildError(expr)
        | IrStmt::Return(Some(expr))
        | IrStmt::Exit(Some(expr)) => walk_func_calls_expr(expr, defined, called),
        _ => {}
    }
}

fn walk_func_calls_expr(e: &IrExpr, defined: &mut HashSet<String>, called: &mut HashSet<String>) {
    match e {
        IrExpr::Call { func, args } => {
            if func == "fnCall" {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    called.insert(name.clone());
                }
            }
            for a in args {
                walk_func_calls_expr(a, defined, called);
            }
        }
        IrExpr::Arrow(body) => walk_func_defs_calls_stmts(body, defined, called),
        IrExpr::Array(items) => {
            for i in items {
                walk_func_calls_expr(i, defined, called);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_func_calls_expr(lhs, defined, called);
            walk_func_calls_expr(rhs, defined, called);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_func_calls_expr(cond, defined, called);
            walk_func_calls_expr(then, defined, called);
            walk_func_calls_expr(else_, defined, called);
        }
        IrExpr::MethodCall { obj, args, .. } => {
            walk_func_calls_expr(obj, defined, called);
            for a in args {
                walk_func_calls_expr(a, defined, called);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    walk_func_calls_expr(x, defined, called);
                }
            }
        }
        IrExpr::Index { var: _, key } => walk_func_calls_expr(key, defined, called),
        IrExpr::DefinedOr { expr, default } => {
            walk_func_calls_expr(expr, defined, called);
            walk_func_calls_expr(default, defined, called);
        }
        IrExpr::Capture { expr, .. } => walk_func_calls_expr(expr, defined, called),
        IrExpr::Arith(a) => walk_func_calls_arith(a, called),
        IrExpr::Lambda { params, body } => {
            for p in params {
                defined.insert(p.clone());
            }
            walk_func_defs_calls_stmts(body, defined, called);
        }
        IrExpr::Object(props) => {
            for (_, v) in props {
                walk_func_calls_expr(v, defined, called);
            }
        }
        IrExpr::Splice(x) => walk_func_calls_expr(x, defined, called),
        _ => {}
    }
}

fn walk_func_calls_arith(a: &crate::ir::ArithAst, called: &mut HashSet<String>) {
    use crate::ir::ArithAst;
    match a {
        ArithAst::Bin { lhs, rhs, .. } => {
            walk_func_calls_arith(lhs, called);
            walk_func_calls_arith(rhs, called);
        }
        ArithAst::Un { arg, .. } => walk_func_calls_arith(arg, called),
        ArithAst::Cond { test, then, else_, .. } => {
            walk_func_calls_arith(test, called);
            walk_func_calls_arith(then, called);
            walk_func_calls_arith(else_, called);
        }
        ArithAst::Assign { rhs, .. } => walk_func_calls_arith(rhs, called),
        ArithAst::IncDec { .. } => {}
        ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
        ArithAst::Index { key, .. } => walk_func_calls_arith(key, called),
        ArithAst::Cast { arg, .. } => walk_func_calls_arith(arg, called),
    }
}

// ── unreachable code ───────────────────────────────────────────────────
// A statement is unreachable when it follows a terminator (`return` /
// `exit` / `die` / `break` / `continue` / `goto`) in the same block. The
// walk uses the same global pre-order statement numbering as
// `use_before_decl`, so positions are comparable across lints.

/// Statement positions that are unreachable (after a terminator in the
/// same block). Sorted ascending.
pub fn collect_unreachable_code(prog: &IrProgram) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let mut roots: Vec<&[IrStmt]> = vec![&prog.stmts];
    for sub in &prog.subs {
        roots.push(&sub.body);
    }
    for stmts in roots {
        walk_unreachable(stmts, &mut pos, &mut out);
    }
    out.sort();
    out
}

fn walk_unreachable(stmts: &[IrStmt], pos: &mut usize, out: &mut Vec<usize>) {
    let mut dead = false;
    for st in stmts {
        *pos += 1;
        if dead {
            out.push(*pos);
        }
        let terminated = is_terminator(st);
        walk_unreachable_stmt(st, pos, out);
        if terminated {
            dead = true;
        }
    }
}

fn is_terminator(st: &IrStmt) -> bool {
    matches!(
        st,
        IrStmt::Return(_)
            | IrStmt::Exit(_)
            | IrStmt::Die { .. }
            | IrStmt::Continue
            | IrStmt::Break
            | IrStmt::Goto(_)
    )
}

fn walk_unreachable_stmt(st: &IrStmt, pos: &mut usize, out: &mut Vec<usize>) {
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            walk_unreachable(then, pos, out);
            for (_, eb) in elsifs {
                walk_unreachable(eb, pos, out);
            }
            walk_unreachable(else_, pos, out);
        }
        IrStmt::Case { clauses, .. } => {
            for cl in clauses {
                walk_unreachable(&cl.body, pos, out);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => walk_unreachable(body, pos, out),
        IrStmt::For { body, .. } => walk_unreachable(body, pos, out),
        IrStmt::ForInit { init, step, body, .. } => {
            walk_unreachable(init, pos, out);
            walk_unreachable(step, pos, out);
            walk_unreachable(body, pos, out);
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            walk_unreachable(b, pos, out);
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                walk_unreachable(s, pos, out);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            walk_unreachable(body, pos, out);
            for ex in excepts {
                walk_unreachable(&ex.body, pos, out);
            }
            walk_unreachable(else_body, pos, out);
            walk_unreachable(finally_body, pos, out);
        }
        IrStmt::Select { clauses } => {
            for cl in clauses {
                walk_unreachable(&cl.body, pos, out);
            }
        }
        IrStmt::Function { body, named_blocks, .. } => {
            walk_unreachable(body, pos, out);
            for (_, nb) in named_blocks {
                walk_unreachable(nb, pos, out);
            }
        }
        _ => {}
    }
}

// ── dead store ──────────────────────────────────────────────────────────
// A write to `v` is a dead store when, in the same straight-line block, it
// is overwritten by a later write to `v` with NO read of `v` in between
// (including reads inside nested sub-blocks). Conservative: writes inside
// nested scopes never make an outer write dead, so this under-reports but
// never false-positives (the safe direction for a warning).

/// `(var, stmt_pos)` of writes whose value is provably never read before
/// being overwritten. Sorted by (pos, var).
pub fn collect_dead_stores(prog: &IrProgram) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let mut roots: Vec<&[IrStmt]> = vec![&prog.stmts];
    for sub in &prog.subs {
        roots.push(&sub.body);
    }
    for stmts in roots {
        walk_dead_stores(stmts, &mut pos, &mut out);
    }
    out.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    out
}

fn walk_dead_stores(stmts: &[IrStmt], pos: &mut usize, out: &mut Vec<(String, usize)>) {
    let mut last_write: HashMap<String, usize> = HashMap::new();
    let mut read_since: HashSet<String> = HashSet::new();
    for st in stmts {
        *pos += 1;
        // reads anywhere in this statement (incl. nested) mark the var read
        for r in stmt_reads(st) {
            read_since.insert(r);
        }
        // writes at THIS level only (nested writes are handled by recursion)
        for w in stmt_writes(st) {
            if !read_since.contains(&w) && last_write.contains_key(&w) {
                out.push((w.clone(), last_write[&w]));
            }
            last_write.insert(w, *pos);
        }
        walk_dead_stores_stmt(st, pos, out);
    }
}

fn walk_dead_stores_stmt(st: &IrStmt, pos: &mut usize, out: &mut Vec<(String, usize)>) {
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            walk_dead_stores(then, pos, out);
            for (_, eb) in elsifs {
                walk_dead_stores(eb, pos, out);
            }
            walk_dead_stores(else_, pos, out);
        }
        IrStmt::Case { clauses, .. } => {
            for cl in clauses {
                walk_dead_stores(&cl.body, pos, out);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => walk_dead_stores(body, pos, out),
        IrStmt::For { body, .. } => walk_dead_stores(body, pos, out),
        IrStmt::ForInit { init, step, body, .. } => {
            walk_dead_stores(init, pos, out);
            walk_dead_stores(step, pos, out);
            walk_dead_stores(body, pos, out);
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            walk_dead_stores(b, pos, out);
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                walk_dead_stores(s, pos, out);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            walk_dead_stores(body, pos, out);
            for ex in excepts {
                walk_dead_stores(&ex.body, pos, out);
            }
            walk_dead_stores(else_body, pos, out);
            walk_dead_stores(finally_body, pos, out);
        }
        IrStmt::Select { clauses } => {
            for cl in clauses {
                walk_dead_stores(&cl.body, pos, out);
            }
        }
        IrStmt::Function { body, named_blocks, .. } => {
            walk_dead_stores(body, pos, out);
            for (_, nb) in named_blocks {
                walk_dead_stores(nb, pos, out);
            }
        }
        _ => {}
    }
}

/// All variable READS anywhere in a statement subtree (used by the
/// dead-store lint to detect "written, then overwritten before any read").
fn stmt_reads(st: &IrStmt) -> HashSet<String> {
    let mut reads = HashSet::new();
    stmt_reads_walk_stmt(st, &mut reads);
    reads
}

fn stmt_reads_walk_stmt(st: &IrStmt, reads: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { expr, .. }
        | IrStmt::Declare { init: Some(expr), .. }
        | IrStmt::Die { expr, .. }
        | IrStmt::Warn { expr, .. }
        | IrStmt::SetChildError(expr)
        | IrStmt::Return(Some(expr))
        | IrStmt::Exit(Some(expr)) => expr_reads_walk(expr, reads),
        IrStmt::Output { value, .. } => expr_reads_walk(value, reads),
        IrStmt::WriteFile { path, content, .. } => {
            expr_reads_walk(path, reads);
            expr_reads_walk(content, reads);
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            expr_reads_walk(cmd, reads);
            for a in args {
                expr_reads_walk(a, reads);
            }
            for r in redirects {
                expr_reads_walk(r, reads);
            }
            for (_, v) in env {
                expr_reads_walk(v, reads);
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            expr_reads_walk(cond, reads);
            stmt_reads_walk_stmts(then, reads);
            for (ec, eb) in elsifs {
                expr_reads_walk(ec, reads);
                stmt_reads_walk_stmts(eb, reads);
            }
            stmt_reads_walk_stmts(else_, reads);
        }
        IrStmt::Case { discriminant, clauses } => {
            expr_reads_walk(discriminant, reads);
            for cl in clauses {
                stmt_reads_walk_stmts(&cl.body, reads);
            }
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            expr_reads_walk(cond, reads);
            stmt_reads_walk_stmts(body, reads);
        }
        IrStmt::For { iter, body, .. } => {
            expr_reads_walk(iter, reads);
            stmt_reads_walk_stmts(body, reads);
        }
        IrStmt::ForInit { init, cond, step, body } => {
            stmt_reads_walk_stmts(init, reads);
            expr_reads_walk(cond, reads);
            stmt_reads_walk_stmts(step, reads);
            stmt_reads_walk_stmts(body, reads);
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            stmt_reads_walk_stmts(body, reads);
            for ex in excepts {
                if let Some(m) = &ex.match_expr {
                    expr_reads_walk(m, reads);
                }
                stmt_reads_walk_stmts(&ex.body, reads);
            }
            stmt_reads_walk_stmts(else_body, reads);
            stmt_reads_walk_stmts(finally_body, reads);
        }
        IrStmt::Select { clauses } => {
            for cl in clauses {
                if let Some(ch) = &cl.ch {
                    expr_reads_walk(ch, reads);
                }
                if let Some(v) = &cl.value {
                    expr_reads_walk(v, reads);
                }
                stmt_reads_walk_stmts(&cl.body, reads);
            }
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            stmt_reads_walk_stmts(b, reads);
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                stmt_reads_walk_stmts(s, reads);
            }
        }
        IrStmt::Function { body, named_blocks, .. } => {
            stmt_reads_walk_stmts(body, reads);
            for (_, nb) in named_blocks {
                stmt_reads_walk_stmts(nb, reads);
            }
        }
        IrStmt::Expr(e) => expr_reads_walk(e, reads),
        IrStmt::Asm { outputs, inputs, .. } => {
            for (_, t) in outputs {
                expr_reads_walk(t, reads);
            }
            for (_, e) in inputs {
                expr_reads_walk(e, reads);
            }
        }
        IrStmt::Ext(n) => {
            for c in crate::shir_nodes::ExtNode::children(&**n) {
                stmt_reads_walk_stmt(c, reads);
            }
        }
        _ => {}
    }
}

fn stmt_reads_walk_stmts(stmts: &[IrStmt], reads: &mut HashSet<String>) {
    for st in stmts {
        stmt_reads_walk_stmt(st, reads);
    }
}

fn expr_reads_walk(e: &IrExpr, reads: &mut HashSet<String>) {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => {
            reads.insert(n.clone());
        }
        IrExpr::Index { var, key } => {
            reads.insert(var.clone());
            expr_reads_walk(key, reads);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_reads_walk(lhs, reads);
            expr_reads_walk(rhs, reads);
        }
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    reads.insert(n.clone());
                }
            }
            _ => {
                for a in args {
                    expr_reads_walk(a, reads);
                }
            }
        },
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads_walk(obj, reads);
            for a in args {
                expr_reads_walk(a, reads);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads_walk(cond, reads);
            expr_reads_walk(then, reads);
            expr_reads_walk(else_, reads);
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_reads_walk(expr, reads);
            expr_reads_walk(default, reads);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    expr_reads_walk(x, reads);
                }
            }
        }
        IrExpr::Capture { expr, .. } => expr_reads_walk(expr, reads),
        IrExpr::Arrow(body) => stmt_reads_walk_stmts(body, reads),
        IrExpr::Array(items) => {
            for i in items {
                expr_reads_walk(i, reads);
            }
        }
        IrExpr::Arith(a) => arith_reads_walk(a, reads),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_reads_walk(iter, reads);
            expr_reads_walk(elem, reads);
            if let Some(c) = cond {
                expr_reads_walk(c, reads);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props {
                expr_reads_walk(v, reads);
            }
        }
        IrExpr::Splice(x) => expr_reads_walk(x, reads),
        IrExpr::Ext(n) => {
            for c in n.children() {
                expr_reads_walk(c, reads);
            }
        }
        _ => {}
    }
}

fn arith_reads_walk(a: &crate::ir::ArithAst, reads: &mut HashSet<String>) {
    use crate::ir::ArithAst::*;
    match a {
        Var(n) | Ident(n) => {
            reads.insert(n.clone());
        }
        Index { var, key } => {
            reads.insert(var.clone());
            arith_reads_walk(key, reads);
        }
        Bin { lhs, rhs, .. } => {
            arith_reads_walk(lhs, reads);
            arith_reads_walk(rhs, reads);
        }
        Un { arg, .. } => arith_reads_walk(arg, reads),
        Cond { test, then, else_, .. } => {
            arith_reads_walk(test, reads);
            arith_reads_walk(then, reads);
            arith_reads_walk(else_, reads);
        }
        Assign { rhs, .. } => arith_reads_walk(rhs, reads),
        IncDec { var, .. } => {
            reads.insert(var.clone());
        }
        Num(_) | Sizeof(_) => {}
        Cast { arg, .. } => arith_reads_walk(arg, reads),
    }
}

/// Variables written DIRECTLY by a statement (not counting nested
/// sub-blocks). Used by the dead-store lint.
fn stmt_writes(st: &IrStmt) -> Vec<String> {
    match st {
        IrStmt::Assign { targets, .. } => targets.iter().map(|t| t.var.clone()).collect(),
        IrStmt::Declare { vars, .. } => vars.iter().map(|d| d.name.clone()).collect(),
        IrStmt::DeclareArray { var, .. } => vec![var.clone()],
        IrStmt::For { var, .. } => vec![var.clone()],
        IrStmt::Expr(e) => expr_writes(e),
        _ => vec![],
    }
}

fn expr_writes(e: &IrExpr) -> Vec<String> {
    match e {
        IrExpr::Arith(a) => match a.as_ref() {
            crate::ir::ArithAst::Assign { var, .. } => vec![var.clone()],
            crate::ir::ArithAst::IncDec { var, .. } => vec![var.clone()],
            _ => vec![],
        },
        IrExpr::Call { func, args } => match func.as_str() {
            "setVar" | "setArray" | "setArrayAppend" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    vec![n.clone()]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        },
        _ => vec![],
    }
}

// ── discarded command substitution ─────────────────────────────────────
// `$(cmd)` used as a bare statement computes a value that is thrown away
// (the author almost certainly meant `cmd` or `var=$(cmd)`).

/// Statement positions where a `Capture` (command substitution) is the
/// whole statement — its result is computed and discarded. Sorted.
pub fn collect_discarded_captures(prog: &IrProgram) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let mut roots: Vec<&[IrStmt]> = vec![&prog.stmts];
    for sub in &prog.subs {
        roots.push(&sub.body);
    }
    for stmts in roots {
        walk_discarded_stmts(stmts, &mut pos, &mut out);
    }
    out.sort();
    out
}

fn walk_discarded_stmts(stmts: &[IrStmt], pos: &mut usize, out: &mut Vec<usize>) {
    for st in stmts {
        *pos += 1;
        if let IrStmt::Expr(IrExpr::Capture { .. }) = st {
            out.push(*pos);
        }
        walk_discarded_stmt(st, pos, out);
    }
}

fn walk_discarded_stmt(st: &IrStmt, pos: &mut usize, out: &mut Vec<usize>) {
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            walk_discarded_stmts(then, pos, out);
            for (_, eb) in elsifs {
                walk_discarded_stmts(eb, pos, out);
            }
            walk_discarded_stmts(else_, pos, out);
        }
        IrStmt::Case { clauses, .. } => {
            for cl in clauses {
                walk_discarded_stmts(&cl.body, pos, out);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => walk_discarded_stmts(body, pos, out),
        IrStmt::For { body, .. } => walk_discarded_stmts(body, pos, out),
        IrStmt::ForInit { init, step, body, .. } => {
            walk_discarded_stmts(init, pos, out);
            walk_discarded_stmts(step, pos, out);
            walk_discarded_stmts(body, pos, out);
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            walk_discarded_stmts(b, pos, out);
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                walk_discarded_stmts(s, pos, out);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            walk_discarded_stmts(body, pos, out);
            for ex in excepts {
                walk_discarded_stmts(&ex.body, pos, out);
            }
            walk_discarded_stmts(else_body, pos, out);
            walk_discarded_stmts(finally_body, pos, out);
        }
        IrStmt::Select { clauses } => {
            for cl in clauses {
                walk_discarded_stmts(&cl.body, pos, out);
            }
        }
        IrStmt::Function { body, named_blocks, .. } => {
            walk_discarded_stmts(body, pos, out);
            for (_, nb) in named_blocks {
                walk_discarded_stmts(nb, pos, out);
            }
        }
        _ => {}
    }
}
