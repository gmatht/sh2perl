//! dead-store-elim — drop assignments, declarations and store syncs for
//! variables that are never READ anywhere in the program, the shIR
//! equivalent of the stalled proposal estree-20260813-182435
//! (dce-dead-vars) and one arm of 20260813-183713 (a1-ssa-const-copy-prop).
//!
//! ## Need
//! The mimecroft contract declares 238 top-level vars; a large fraction
//! are written-never-read scratch (the per-function `*_tmp` names, the
//! `g_*` timing accumulators — 96 `sh2.setVar` + 469 `sh2.vars.` writes
//! counted statically, many in the hot path). Every backend emits a
//! narrowing declaration + a store write for each. The shIR carries no
//! read set, so each backend re-derives it (or not at all).
//!
//! ## Scope
//! Only variables with ZERO reads anywhere in the program (module +
//! function bodies) are eliminated — the sound version:
//!   - a `Var`/`Ident` operand, `getVar`/`arrayIndex` call, an `Index`
//!     read of an array, an interpolated reference, a test/arith
//!     operand, a name inside a capture/subshell/arrow — ALL count as
//!     reads, so a var read in ANY scope is kept;
//!   - a var the program EXPOSES (a name read inside a capture/subshell/
//!     arrow that may outlive its writer, or an env/export-visible name)
//!     is kept — we only drop provably-private scratch;
//!   - arrays with no element reads drop their `DeclareArray`,
//!     `DeclareArray`-init and the `setArray`/store syncs;
//!   - the `$?`/lastExit/status machinery is never touched (that is
//!     lastexit-liveness's domain, not DCE's).
//! Anything ambiguous is left untouched — refuse > guess.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Runs inside
//! `ast_to_ir`. Consumes the escape-classes verdict (optional, sharpens
//! the escapes set) and feeds function-purity (a removed dead var cannot
//! be a purity input).

use std::collections::HashSet;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// Is a Call a dynamic write (eval/source/.)? The A1 carries these as
/// `builtin("eval", …)` / `exec("eval", …)` — the command name is
/// args[0], not the func.
fn is_dynamic_write_call(func: &str, args: &[IrExpr]) -> bool {
    if matches!(func, "eval" | "source" | ".") {
        return true;
    }
    if matches!(func, "builtin" | "exec") {
        if let Some(IrExpr::Str(cmd, _)) = args.first() {
            if matches!(cmd.as_str(), "eval" | "source" | ".") {
                return true;
            }
        }
    }
    false
}

/// Does the program contain a dynamic write (eval/source/.)? Such a
/// call can write ANY variable, so the const analysis classifies every
/// var Var — removing any store would change the verdicts. Skip the
/// whole transform.
fn has_dynamic_write(stmts: &[IrStmt]) -> bool {
    fn walk_stmt(st: &IrStmt) -> bool {
        match st {
            IrStmt::Expr(IrExpr::Call { func, args }) => {
                if is_dynamic_write_call(func, args) {
                    return true;
                }
                args.iter().any(walk_expr)
            }
            IrStmt::Assign { targets, expr, .. } => {
                targets.iter().any(|t| t.indices.iter().any(walk_expr)) || walk_expr(expr)
            }
            IrStmt::Declare { init, .. } => init.as_ref().map(walk_expr).unwrap_or(false),
            IrStmt::DeclareArray { elements, .. } => elements.iter().any(walk_expr),
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                walk_expr(cond)
                    || then.iter().any(walk_stmt)
                    || elsifs.iter().any(|(_, b)| b.iter().any(walk_stmt))
                    || else_.iter().any(walk_stmt)
            }
            IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
                walk_expr(cond) || body.iter().any(walk_stmt)
            }
            IrStmt::For { iter, body, .. } => walk_expr(iter) || body.iter().any(walk_stmt),
            IrStmt::ForInit { init, cond, step, body, .. } => {
                init.iter().any(walk_stmt)
                    || walk_expr(cond)
                    || step.iter().any(walk_stmt)
                    || body.iter().any(walk_stmt)
            }
            IrStmt::Case { discriminant, clauses, .. } => {
                walk_expr(discriminant) || clauses.iter().any(|c| c.body.iter().any(walk_stmt))
            }
            IrStmt::Function { body, .. } | IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                body.iter().any(walk_stmt)
            }
            IrStmt::Pipeline { stages, .. } => stages.iter().any(|st| st.iter().any(walk_stmt)),
            IrStmt::Redirect { inner, redirects } => {
                // redirect targets are reads (`<<< "$var"`, `> "$f"`) —
                // a var read ONLY there must not be judged dead
                redirects.iter().any(|r| walk_expr(&r.target))
                    || inner.iter().any(walk_stmt)
            }
            IrStmt::Expr(e) => walk_expr(e),
            IrStmt::Output { value, .. } => walk_expr(value),
            IrStmt::WriteFile { path, content, .. } => walk_expr(path) || walk_expr(content),
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => walk_expr(e),
            _ => false,
        }
    }
    fn walk_expr(e: &IrExpr) -> bool {
        match e {
            IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => stmts.iter().any(walk_stmt),
            IrExpr::Call { func, args } => {
                if is_dynamic_write_call(func, args) {
                    return true;
                }
                args.iter().any(walk_expr)
            }
            IrExpr::Array(items) => items.iter().any(walk_expr),
            IrExpr::Interpolate(parts) => parts.iter().any(|p| matches!(p, crate::ir::InterpPart::Expr(x) if walk_expr(x))),
            IrExpr::BinOp { lhs, rhs, .. } => walk_expr(lhs) || walk_expr(rhs),
            IrExpr::Ternary { cond, then, else_, .. } => walk_expr(cond) || walk_expr(then) || walk_expr(else_),
            IrExpr::DefinedOr { expr, default } => walk_expr(expr) || walk_expr(default),
            IrExpr::Capture { expr, .. } => walk_expr(expr),
            IrExpr::Index { key, .. } => walk_expr(key),
            IrExpr::MethodCall { obj, args, .. } => walk_expr(obj) || args.iter().any(walk_expr),
            IrExpr::Object(props) => props.iter().any(|(_, v)| walk_expr(v)),
            IrExpr::ArrayComp { iter, elem, cond, .. } => walk_expr(iter) || walk_expr(elem) || cond.as_ref().map(|c| walk_expr(c)).unwrap_or(false),
            IrExpr::Splice(inner) => walk_expr(inner),
            IrExpr::Arith(ast) => walk_arith(ast),
            IrExpr::Ext(n) => crate::shir_nodes::ExtExpr::children(&**n)
                .into_iter()
                .any(walk_expr),
            _ => false,
        }
    }
    fn walk_arith(ast: &ArithAst) -> bool {
        match ast {
            ArithAst::Bin { lhs, rhs, .. } => walk_arith(lhs) || walk_arith(rhs),
            ArithAst::Un { arg, .. } => walk_arith(arg),
            ArithAst::Cond { test, then, else_, .. } => walk_arith(test) || walk_arith(then) || walk_arith(else_),
            ArithAst::Assign { rhs, .. } => walk_arith(rhs),
            ArithAst::Index { key, .. } => walk_arith(key),
            ArithAst::Cast { arg, .. } => walk_arith(arg),
            _ => false,
        }
    }
    stmts.iter().any(walk_stmt)
}

/// Shell variables the SHELL ITSELF reads implicitly. No syntactic
/// reference is needed for the value to change behaviour, so a store to one
/// of these is never dead:
///   - `IFS` drives word splitting in every later unquoted expansion and
///     every `$*` / `${arr[*]}` join, including the expansion of the SAME
///     command (`IFS=$'\n' a=($(cmd))` — the capture splits on the new
///     value);
///   - `PATH` / `CDPATH` / `GLOBIGNORE` drive command lookup, `cd` and
///     globbing for the `exec` calls that follow;
///   - `HOME` / `PWD` / `OLDPWD` / `TMPDIR` are consulted by `cd`,
///     `mktemp` and `~` expansion;
///   - `ENV` / `BASH_ENV` are sourced by child shells;
///   - `OPTIND` is advanced by `getopts`.
/// The census only sees EXPLICIT reads, so `IFS=x x=5` (no `[*]` anywhere)
/// judged the IFS store dead and dropped it — core request
/// fish-20261016-core-ir-drops. Keeping the store is the faithful IR; a
/// backend that never consults the value simply emits an inert assignment.
const SHELL_IMPLICIT_VARS: &[&str] = &[
    "IFS", "PATH", "CDPATH", "GLOBIGNORE", "HOME", "PWD", "OLDPWD", "TMPDIR", "ENV",
    "BASH_ENV", "OPTIND",
];

fn is_shell_implicit(name: &str) -> bool {
    SHELL_IMPLICIT_VARS.contains(&name)
}

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    if has_dynamic_write(stmts) {
        return false;
    }
    let mut reads: HashSet<String> = HashSet::new();
    let mut writes: HashSet<String> = HashSet::new();
    let mut escapes: HashSet<String> = HashSet::new();
    census(stmts, &mut reads, &mut writes, &mut escapes);

    // Only remove stores for CONST variables (single assignment site,
    // never changed) that are never read. The const analysis
    // (analyze_var_const) classifies a var Var when it is multi-assigned,
    // written by a runtime store, in a function body, or under eval —
    // those stores MUST survive (the analysis needs to see them to
    // produce the Var verdict). Removing a Const var's single dead store
    // is the mimecroft win (the per-function *_tmp / g_* scratch).
    let consts: HashSet<String> = crate::shir::analyze_var_const(&crate::ir::IrProgram {
        imports: vec![],
        requires: vec![],
        stmts: stmts.clone(),
        subs: vec![],
        var_types: vec![],
        stmt_lines: vec![],
        var_lengths: vec![],
        var_const: vec![],
        var_lifetimes: vec![],
        var_nospace: vec![],
        var_storage: vec![],
        var_bash_env: vec![],
    })
    .into_iter()
    .filter(|(_, k)| matches!(k, crate::ir::VarKind::Const))
    .map(|(n, _)| n)
    .collect();

    let dead: Vec<String> = writes
        .iter()
        .filter(|v| {
            if reads.contains(*v) || escapes.contains(*v) {
                return false;
            }
            if is_shell_implicit(v) {
                return false;
            }
            if !consts.contains(*v) {
                return false;
            }
            if let Some(pos) = v.find('[') {
                let base = &v[..pos];
                if reads.contains(base) || escapes.contains(base) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();
    if dead.is_empty() {
        return false;
    }
    let before = stmts.len();
    *stmts = stmts.drain(..).filter_map(|s| purge(s, &dead)).collect();
    stmts.len() != before
}

/// Count how many times each variable is WRITTEN (Assign targets,
/// Declare vars, DeclareArray var, setVar calls). The const analysis
/// needs the multiplicity — a var assigned twice is Var, once is Const.
fn count_writes(stmts: &[IrStmt]) -> std::collections::HashMap<String, usize> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    fn bump(counts: &mut std::collections::HashMap<String, usize>, name: &str) {
        *counts.entry(name.to_string()).or_insert(0) += 1;
    }
    fn walk_stmt(st: &IrStmt, counts: &mut std::collections::HashMap<String, usize>) {
        match st {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    bump(counts, &t.var);
                }
                walk_expr(expr, counts);
            }
            IrStmt::Declare { vars, init, .. } => {
                for v in vars {
                    bump(counts, &v.name);
                }
                if let Some(i) = init {
                    walk_expr(i, counts);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                bump(counts, var);
                for e in elements {
                    walk_expr(e, counts);
                }
            }
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "setVar" => {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    bump(counts, n);
                }
                for a in args {
                    walk_expr(a, counts);
                }
            }
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                walk_expr(cond, counts);
                for b in then { walk_stmt(b, counts); }
                for (_, b) in elsifs { for s in b { walk_stmt(s, counts); } }
                for b in else_ { walk_stmt(b, counts); }
            }
            IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
                walk_expr(cond, counts);
                for b in body { walk_stmt(b, counts); }
            }
            IrStmt::For { var, iter, body, .. } => {
                bump(counts, var);
                walk_expr(iter, counts);
                for b in body { walk_stmt(b, counts); }
            }
            IrStmt::ForInit { init, cond, step, body, .. } => {
                for b in init { walk_stmt(b, counts); }
                walk_expr(cond, counts);
                for b in step { walk_stmt(b, counts); }
                for b in body { walk_stmt(b, counts); }
            }
            IrStmt::Case { discriminant, clauses, .. } => {
                walk_expr(discriminant, counts);
                for c in clauses { for b in &c.body { walk_stmt(b, counts); } }
            }
            IrStmt::Function { body, .. } | IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                for b in body { walk_stmt(b, counts); }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages { for b in stage { walk_stmt(b, counts); } }
            }
            IrStmt::Redirect { inner, redirects } => {
                for r in redirects { walk_expr(&r.target, counts); }
                for b in inner { walk_stmt(b, counts); }
            }
            IrStmt::Expr(e) => walk_expr(e, counts),
            IrStmt::Output { value, .. } => walk_expr(value, counts),
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, counts);
                walk_expr(content, counts);
            }
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => walk_expr(e, counts),
            _ => {}
        }
    }
    fn walk_expr(e: &IrExpr, counts: &mut std::collections::HashMap<String, usize>) {
        match e {
            IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => {
                for s in stmts { walk_stmt(s, counts); }
            }
            IrExpr::Call { args, .. } => {
                for a in args { walk_expr(a, counts); }
            }
            IrExpr::Array(items) => { for i in items { walk_expr(i, counts); } }
            IrExpr::Interpolate(parts) => {
                for p in parts { if let crate::ir::InterpPart::Expr(x) = p { walk_expr(x, counts); } }
            }
            IrExpr::BinOp { lhs, rhs, .. } => { walk_expr(lhs, counts); walk_expr(rhs, counts); }
            IrExpr::Ternary { cond, then, else_, .. } => { walk_expr(cond, counts); walk_expr(then, counts); walk_expr(else_, counts); }
            IrExpr::DefinedOr { expr, default } => { walk_expr(expr, counts); walk_expr(default, counts); }
            IrExpr::Capture { expr, .. } => walk_expr(expr, counts),
            IrExpr::Index { key, .. } => walk_expr(key, counts),
            IrExpr::MethodCall { obj, args, .. } => { walk_expr(obj, counts); for a in args { walk_expr(a, counts); } }
            IrExpr::Object(props) => { for (_, v) in props { walk_expr(v, counts); } }
            IrExpr::ArrayComp { iter, elem, cond, .. } => { walk_expr(iter, counts); walk_expr(elem, counts); if let Some(c) = cond { walk_expr(c, counts); } }
            IrExpr::Splice(inner) => walk_expr(inner, counts),
            IrExpr::Arith(ast) => walk_arith(ast, counts),
            IrExpr::Ext(n) => {
                for c in crate::shir_nodes::ExtExpr::children(&**n) {
                    walk_expr(c, counts);
                }
            }
            _ => {}
        }
    }
    fn walk_arith(ast: &ArithAst, counts: &mut std::collections::HashMap<String, usize>) {
        match ast {
            ArithAst::Bin { lhs, rhs, .. } => { walk_arith(lhs, counts); walk_arith(rhs, counts); }
            ArithAst::Un { arg, .. } => walk_arith(arg, counts),
            ArithAst::Cond { test, then, else_, .. } => { walk_arith(test, counts); walk_arith(then, counts); walk_arith(else_, counts); }
            ArithAst::Assign { rhs, .. } => walk_arith(rhs, counts),
            ArithAst::Index { key, .. } => walk_arith(key, counts),
            ArithAst::Cast { arg, .. } => walk_arith(arg, counts),
            _ => {}
        }
    }
    for st in stmts {
        walk_stmt(st, &mut counts);
    }
    counts
}

/// `$name` / `${name}` references inside a case PATTERN's source text.
///
/// Patterns arrive as TEXT (see the census arm) and bash expands them before
/// matching, so each `$name` is a read of that variable. Mirrors
/// `shir::lift_walk_stmt`'s `lift_mark_string_refs` call on the same field —
/// that walker already treats a pattern as a read, which is why a var used
/// only in a pattern is correctly EXCLUDED from the lift verdicts; the DCE
/// census must agree or it deletes the store the pattern depends on.
fn dollar_names_in(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
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
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
            i += 1;
        }
        if start < i && (b[start].is_ascii_alphabetic() || b[start] == b'_') {
            out.push(text[start..i].to_string());
        }
    }
    out
}

fn census(stmts: &[IrStmt], reads: &mut HashSet<String>, writes: &mut HashSet<String>, escapes: &mut HashSet<String>) {
    for st in stmts {
        census_stmt(st, reads, writes, escapes, false);
    }
}

#[allow(clippy::too_many_arguments)]
fn census_stmt(
    st: &IrStmt,
    reads: &mut HashSet<String>,
    writes: &mut HashSet<String>,
    escapes: &mut HashSet<String>,
    escaping: bool,
) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                writes.insert(t.var.clone());
                for k in &t.indices {
                    census_expr(k, reads, writes, escapes, escaping);
                }
            }
            census_expr(expr, reads, writes, escapes, escaping);
        }
        IrStmt::Declare { vars, init, .. } => {
            for v in vars {
                writes.insert(v.name.clone());
            }
            if let Some(i) = init {
                census_expr(i, reads, writes, escapes, escaping);
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            writes.insert(var.clone());
            for e in elements {
                census_expr(e, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Output { value, .. } => census_expr(value, reads, writes, escapes, escaping),
        IrStmt::WriteFile { path, content, .. } => {
            census_expr(path, reads, writes, escapes, escaping);
            census_expr(content, reads, writes, escapes, escaping);
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            census_expr(cond, reads, writes, escapes, escaping);
            for s in then {
                census_stmt(s, reads, writes, escapes, escaping);
            }
            for (c, b) in elsifs {
                census_expr(c, reads, writes, escapes, escaping);
                for s in b {
                    census_stmt(s, reads, writes, escapes, escaping);
                }
            }
            for s in else_ {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::For { iter, body, .. } => {
            census_expr(iter, reads, writes, escapes, escaping);
            for s in body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::While { cond, body } => {
            census_expr(cond, reads, writes, escapes, escaping);
            for s in body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            for s in body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
            census_expr(cond, reads, writes, escapes, escaping);
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
            census_expr(expr, reads, writes, escapes, escaping);
        }
        IrStmt::Exec { cmd, args, .. } => {
            census_expr(cmd, reads, writes, escapes, escaping);
            for a in args {
                census_expr(a, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => {
            census_expr(e, reads, writes, escapes, escaping);
        }
        IrStmt::SetChildError(e) => census_expr(e, reads, writes, escapes, escaping),
        IrStmt::Case { discriminant, clauses } => {
            census_expr(discriminant, reads, writes, escapes, escaping);
            for cl in clauses {
                // A case PATTERN is EXPANDED by bash before matching, and the
                // frontends hand it over as SOURCE TEXT (a plain `String` on
                // the clause, not an `IrExpr`), so a `$name` inside one is a
                // READ of that variable. Without this the census saw no read
                // at all for `x=bar; case bar in $x) …`: this transform
                // dropped the ONLY store of `x` (and `analyze_var_types` then
                // listed nothing, so no backend declared it either) and the
                // renderers fell through to the environment variable —
                // bash printed the clause body, every translation printed
                // nothing. See core-requests/core-20261015-145805-case-pattern-reads.md.
                for p in &cl.patterns {
                    for v in dollar_names_in(p) {
                        reads.insert(v);
                    }
                }
                for s in &cl.body {
                    census_stmt(s, reads, writes, escapes, escaping);
                }
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            for r in redirects {
                census_expr(&r.target, reads, writes, escapes, escaping);
            }
            for s in inner {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Function { body, .. } => {
            // function writes are multi-run (the const analysis marks
            // them Var) — treat them as escaping so they are never
            // dead-eliminated
            for s in body {
                census_stmt(s, reads, writes, escapes, true);
            }
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) => {
            for s in v {
                census_stmt(s, reads, writes, escapes, true);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            // stages run as separate processes, but their READS see
            // the parent's values (a var read only here must keep its
            // store — BASHC_FIX t03: `name` read only inside a pipeline
            // was dropped as dead). Ambient escaping (like Block):
            // stage-local writes don't outlive the stage.
            for st in stages {
                for s in st {
                    census_stmt(s, reads, writes, escapes, escaping);
                }
            }
        }
        IrStmt::ForInit { init, cond, step, body, .. } => {
            for s in init {
                census_stmt(s, reads, writes, escapes, escaping);
            }
            census_expr(cond, reads, writes, escapes, escaping);
            for s in step {
                census_stmt(s, reads, writes, escapes, escaping);
            }
            for s in body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
            for s in body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
            for e in excepts {
                if let Some(m) = &e.match_expr {
                    census_expr(m, reads, writes, escapes, escaping);
                }
                for s in &e.body {
                    census_stmt(s, reads, writes, escapes, escaping);
                }
            }
            for s in else_body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
            for s in finally_body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Block(v) => {
            for s in v {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Expr(e) => census_expr(e, reads, writes, escapes, escaping),
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn census_expr(
    e: &IrExpr,
    reads: &mut HashSet<String>,
    writes: &mut HashSet<String>,
    escapes: &mut HashSet<String>,
    escaping: bool,
) {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            reads.insert(v.clone());
            if escaping {
                escapes.insert(v.clone());
            }
        }
        IrExpr::Index { var, key } => {
            reads.insert(var.clone());
            census_expr(key, reads, writes, escapes, escaping);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            census_expr(lhs, reads, writes, escapes, escaping);
            census_expr(rhs, reads, writes, escapes, escaping);
        }
        IrExpr::Call { func, args } => {
            // getVar / arrayIndex / param read their name args;
            // setVar / setArray / SetChildError-style writes are
            // recorded by their targets. The go-sh self-hosting helpers
            // (byteAt / jsonGet / jsonSet / jsonArrGet / jsonArrSet /
            // jsonArrAppend) read their name arg (args[0]) — byteAt also
            // reads the index vars in args[1] — and the json* writers
            // WRITE the resolved var, so their stores stay live.
            if matches!(
                func.as_str(),
                "getVar" | "arrayIndex" | "param" | "byteAt" | "jsonGet" | "jsonSet"
                    | "jsonArrGet" | "jsonArrSet" | "jsonArrAppend"
                    | "memLoad" | "memAdvance" | "memFree"
            ) {
                let idx = if func == "param" { 1 } else { 0 };
                if let Some(IrExpr::Str(n, _)) = args.get(idx) {
                    reads.insert(n.clone());
                    // An ELEMENT read (`arr[1]` — the index baked into
                    // the name) also reads the array NAME (`arr`): the
                    // `arr=(...)` setArray/DeclareArray write is live
                    // (array-subscript.sh).
                    if let Some(base) = n.split('[').next() {
                        reads.insert(base.to_string());
                        if escaping {
                            escapes.insert(base.to_string());
                        }
                    }
                    // A `name[*]` expansion reads IFS at RUNTIME: the `[*]`
                    // join is performed by the backend runtime, which joins
                    // the elements with the FIRST IFS character (bash uses
                    // IFS for `*` even when quoted). Without this the
                    // `IFS=$'\n'` store is culled and the join falls back to
                    // a space — 064_07_complex_array_operations printed
                    // "admin localhost" where bash printed the
                    // newline-joined, sort-able string. `[@]` joins with a
                    // hard space and does not read IFS.
                    if n.ends_with("[*]") {
                        reads.insert("IFS".to_string());
                    }
                }
                // byteAt's index arg is arithmetic text naming vars
                // (`byteAt("src", "i+1")`) — read those too.
                if func == "byteAt" {
                    if let Some(IrExpr::Str(t, _)) = args.get(1) {
                        for w in t.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
                            if !w.is_empty() && w.chars().next().unwrap().is_ascii_alphabetic() {
                                reads.insert(w.to_string());
                            }
                        }
                    }
                }
            }
            // the json* writers write the resolved var (the receiver
            // name arg) — the store must stay live.
            if matches!(func.as_str(), "jsonSet" | "jsonArrSet" | "jsonArrAppend") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    writes.insert(n.clone());
                }
            }
            // memStore/arrayStore write THROUGH their first-arg handle:
            // the receiver name is both read (handle resolution) and
            // written — its initializing stores stay live
            if matches!(func.as_str(), "memStore" | "arrayStore") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    reads.insert(n.clone());
                    writes.insert(n.clone());
                }
            }
            // fnCall/fnValue/callDirect dispatch passes its string args
            // as the callee's $1.. — and callees routinely use those as
            // VARIABLE NAMES (array-by-name in-place ops: the arg "a"
            // becomes getVar/param reads inside the body, invisible here).
            // Mark every Str arg as a read: over-marking only shrinks
            // elimination, never drops live stores.
            if matches!(func.as_str(), "fnCall" | "fnValue" | "callDirect") {
                for a in args {
                    match a {
                        IrExpr::Str(n, _) => {
                            reads.insert(n.clone());
                        }
                        IrExpr::Array(items) => {
                            for it in items {
                                if let IrExpr::Str(n, _) = it {
                                    reads.insert(n.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // a $name reference inside ANY string argument is a read —
            // the `test` operands (`[ "$x" -gt "$y" ]`) and the slice
            // start (`${s:$i:1}` → Str("$i")) carry $vars in Str nodes,
            // invisible to the walk above. Without this, DSE judged
            // `RD_VR=16000` / `sm_tex_total=15` (constants read ONLY
            // inside test brackets) dead and dropped them — the culling
            // compared against 0 (blocks culled), the texture menu loops
            // ran zero iterations (no preload, no thumbnails).
            for a in args {
                if let IrExpr::Str(s, _) = a {
                    string_read_names(s, reads);
                }
            }
            for a in args {
                census_expr(a, reads, writes, escapes, escaping);
            }
        }
        IrExpr::Arith(a) => arith_census(a, reads, writes, escapes, escaping),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    census_expr(x, reads, writes, escapes, escaping);
                }
            }
        }
        IrExpr::Capture { expr, .. } => {
            census_expr(expr, reads, writes, escapes, true);
        }
        IrExpr::Arrow(body) => {
            for s in body {
                census_stmt(s, reads, writes, escapes, true);
            }
        }
        IrExpr::Array(v) => {
            for x in v {
                census_expr(x, reads, writes, escapes, escaping);
            }
        }
        IrExpr::Object(v) => {
            for (_, x) in v {
                census_expr(x, reads, writes, escapes, escaping);
            }
        }
        // A bare Str node can carry `$name` references too — the heredoc
        // target (`cat << EOF` with `Hello $name` → Str("Hello $name\n")
        // with interpolate:true) is a Redirect target, not a Call arg, so
        // the Call-arg scan above never sees it. Without this, DSE judged
        // `name` never-read and dropped `name="world"` — the heredoc
        // interpolated the empty store (079_heredoc_interpolation).
        IrExpr::Str(s, _) => {
            string_read_names(s, reads);
        }
        _ => {}
    }
}

/// Insert every bare `$name` / `${name}` reference in `s` into `reads`.
/// (The `test` operands `[ "$x" -gt "$y" ]` and the slice start
/// `${s:$i:1}` carry $vars inside Str nodes — a Str is opaque to the
/// census walk, so they must be decoded here.)
fn string_read_names(s: &str, reads: &mut HashSet<String>) {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '$' && i + 1 < bytes.len() {
            let start = i + 1;
            let mut j = start;
            if bytes[j] == b'{' {
                j += 1;
                let w = j;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
                {
                    j += 1;
                }
                // a name was scanned — record it even when the `}` is
                // missing or delayed by expansion OPERATORS
                // (`${MAXWAIT% *}`, `${x:-d}`: the core may also drop a
                // trailing `}` when the pattern holds a `#`) — failing
                // to count it judged the var never-read and DSE dropped
                // its only store (param-expand-hash)
                if j > w { reads.insert(String::from_utf8_lossy(&bytes[w..j]).into_owned()); }
                i = j + 1;
                continue;
            }
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > start { reads.insert(String::from_utf8_lossy(&bytes[start..j]).into_owned()); }
            i = j;
        } else {
            i += 1;
        }
    }
}

fn arith_census(
    a: &ArithAst,
    reads: &mut HashSet<String>,
    writes: &mut HashSet<String>,
    escapes: &mut HashSet<String>,
    escaping: bool,
) {
    match a {
        ArithAst::Var(v) => {
            reads.insert(v.clone());
            if escaping {
                escapes.insert(v.clone());
            }
        }
        ArithAst::Index { var, key, .. } => {
            reads.insert(var.clone());
            // the index expression is a read too (`arr[i]` — the key
            // var `i` must not be judged never-read, or its store is
            // DSE-dropped and the index renders as the empty-string
            // never-written read → arr[0] (arith-array-index-expr).
            arith_census(key, reads, writes, escapes, escaping);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_census(lhs, reads, writes, escapes, escaping);
            arith_census(rhs, reads, writes, escapes, escaping);
        }
        ArithAst::Un { arg, .. } => arith_census(arg, reads, writes, escapes, escaping),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_census(test, reads, writes, escapes, escaping);
            arith_census(then, reads, writes, escapes, escaping);
            arith_census(else_, reads, writes, escapes, escaping);
        }
        ArithAst::Assign { var, rhs, .. } => {
            writes.insert(var.clone());
            arith_census(rhs, reads, writes, escapes, escaping);
        }
        _ => {}
    }
}

/// Whether evaluating an expression is free of observable work. This is
/// deliberately narrower than the renderer's "pure function" analysis:
/// DCE must not erase a command substitution, a runtime call, an arithmetic
/// assignment/incdec, or any opaque/raw expression merely because its result
/// flows into a dead variable.
fn expr_pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_)
        | IrExpr::Str(_, _)
        | IrExpr::Var(_, _)
        | IrExpr::Regex { .. }
        | IrExpr::Range { .. }
        | IrExpr::Bool(_)
        | IrExpr::Json(_) => true,
        IrExpr::Index { key, .. } => expr_pure(key),
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs) && expr_pure(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_pure(cond) && expr_pure(then) && expr_pure(else_)
        }
        IrExpr::DefinedOr { expr, default } => expr_pure(expr) && expr_pure(default),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            crate::ir::InterpPart::Lit(_) => true,
            crate::ir::InterpPart::Expr(x) => expr_pure(x),
        }),
        IrExpr::Arith(a) => arith_pure(a),
        IrExpr::Array(items) => items.iter().all(expr_pure),
        IrExpr::Object(items) => items.iter().all(|(_, x)| expr_pure(x)),
        IrExpr::Splice(x) => expr_pure(x),
        // Calls, captures, method calls, arrows/lambdas and comprehensions
        // can execute work or observe shell state. Keep their enclosing
        // assignment rather than trying to prove more here.
        IrExpr::Call { .. }
        | IrExpr::MethodCall { .. }
        | IrExpr::Capture { .. }
        | IrExpr::RawExpr(_)
        | IrExpr::Arrow(_)
        | IrExpr::ArrayComp { .. }
        | IrExpr::Lambda { .. }
        | IrExpr::Ident(_) => false,
        IrExpr::Ext(_) => false,
    }
}

fn arith_pure(a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(_)
        | ArithAst::Var(_)
        | ArithAst::Ident(_)
        | ArithAst::Sizeof(_) => true,
        ArithAst::Index { key, .. } => arith_pure(key),
        ArithAst::Bin { lhs, rhs, .. } => arith_pure(lhs) && arith_pure(rhs),
        ArithAst::Un { arg, .. } => arith_pure(arg),
        ArithAst::Cond { test, then, else_ } => {
            arith_pure(test) && arith_pure(then) && arith_pure(else_)
        }
        ArithAst::Cast { arg, .. } => arith_pure(arg),
        ArithAst::Assign { .. } | ArithAst::IncDec { .. } => false,
    }
}

/// Recursively remove statements that only touch dead vars. Returns
/// `None` when the statement is dropped, `Some` (possibly with trimmed
/// nested bodies) when kept.
fn purge(st: IrStmt, dead: &[String]) -> Option<IrStmt> {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            // An assignment's RHS is evaluated even when its destination is
            // dead. In particular, `dead=$(cmd)` is a side-effecting command
            // substitution (and can contain redirects, writes, etc.). Drop
            // only a dead store whose RHS is provably pure; refuse > guess
            // for calls/captures/raw expressions. Indexed writes could also
            // matter to the array's lifetime, so those remain kept as before.
            let all_plain_dead = !targets.is_empty()
                && targets
                    .iter()
                    .all(|t| t.indices.is_empty() && !t.var.contains('[') && dead.contains(&t.var));
            if all_plain_dead && expr_pure(&expr) {
                return None;
            }
            Some(IrStmt::Assign { targets, expr, asm: None })
        }
        IrStmt::Declare { vars, init, local } => {
            // A declaration initializer has the same RHS evaluation
            // semantics as an assignment (`dead=$(cmd)` can occur here
            // through a frontend). A bare declaration is pure; an impure
            // initializer must survive even when every name is dead.
            if vars.iter().all(|v| dead.contains(&v.name))
                && init.as_ref().map_or(true, expr_pure)
            {
                return None;
            }
            Some(IrStmt::Declare { vars, init, local })
        }
        IrStmt::DeclareArray { var, sigil, elements } => {
            // Array elements may themselves be captures or calls. Keep the
            // declaration unless evaluating every element is side-effect free.
            if dead.contains(&var) && elements.iter().all(expr_pure) {
                return None;
            }
            Some(IrStmt::DeclareArray { var, sigil, elements })
        }
        IrStmt::Expr(ex) => {
            // setVar / sh2.setVar store writes for a dead var, and
            // setArray of a dead array, drop the whole call
            let drop = match &ex {
                IrExpr::Call { func, args }
                    if func.ends_with("setVar") || func == "setArray" =>
                {
                    matches!(args.first(), Some(IrExpr::Str(n, _)) if dead.contains(n) && !n.contains('['))
                        && args.iter().skip(1).all(expr_pure)
                }
                _ => false,
            };
            if drop {
                return None;
            }
            Some(IrStmt::Expr(ex))
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            Some(IrStmt::If {
                cond,
                then: then.into_iter().filter_map(|s| purge(s, dead)).collect(),
                elsifs: elsifs
                    .into_iter()
                    .map(|(c, b)| (c, b.into_iter().filter_map(|s| purge(s, dead)).collect()))
                    .collect(),
                else_: else_.into_iter().filter_map(|s| purge(s, dead)).collect(),
            })
        }
        IrStmt::For { var, iter, body } => Some(IrStmt::For {
            var,
            iter,
            body: body.into_iter().filter_map(|s| purge(s, dead)).collect(),
        }),
        IrStmt::While { cond, body } => Some(IrStmt::While {
            cond,
            body: body.into_iter().filter_map(|s| purge(s, dead)).collect(),
        }),
        IrStmt::DoWhile { body, cond, until } => Some(IrStmt::DoWhile {
            body: body.into_iter().filter_map(|s| purge(s, dead)).collect(),
            cond,
            until,
        }),
        IrStmt::Case { discriminant, clauses } => Some(IrStmt::Case {
            discriminant,
            clauses: clauses
                .into_iter()
                .map(|mut c| {
                    c.body = c.body.into_iter().filter_map(|s| purge(s, dead)).collect();
                    c
                })
                .collect(),
        }),
        IrStmt::Block(v) => Some(IrStmt::Block(
            v.into_iter().filter_map(|s| purge(s, dead)).collect(),
        )),
        IrStmt::Subshell(v) => Some(IrStmt::Subshell(
            v.into_iter().filter_map(|s| purge(s, dead)).collect(),
        )),
        IrStmt::Background(v) => Some(IrStmt::Background(
            v.into_iter().filter_map(|s| purge(s, dead)).collect(),
        )),
        IrStmt::Function { name, params, body, .. } => Some(IrStmt::Function {
            name,
            params,
            body: body.into_iter().filter_map(|s| purge(s, dead)).collect(),
            named_blocks: Vec::new(),
        }),
        other => Some(other),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn str_(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    /// Regression pin (core-request core-20261015-145805, rust backend):
    /// a var read ONLY inside a `case` PATTERN must keep its store.
    ///
    /// `x=bar; case bar in $x) echo MATCH;; esac` — the pattern is SOURCE
    /// TEXT bash expands before matching, so `$x` is a read. The census
    /// walked only the clause BODY, judged `x` never-read, and deleted its
    /// only store; every backend then read `$x` from the environment and
    /// printed nothing where bash prints MATCH.
    #[test]
    fn case_pattern_read_keeps_store() {
        let case = IrStmt::Case {
            discriminant: str_("bar"),
            clauses: vec![crate::ir::IrCaseClause {
                patterns: vec!["$x".to_string()],
                body: vec![IrStmt::Expr(IrExpr::Call {
                    func: "builtin".to_string(),
                    args: vec![
                        str_("echo"),
                        IrExpr::Array(vec![str_("MATCH")]),
                    ],
                })],
            }],
        };
        let mut prog = vec![
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "x".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: str_("bar"),
                asm: None,
            },
            case,
        ];
        assert!(
            !transform(&mut prog),
            "the store feeding the case pattern must not be dropped"
        );
        assert_eq!(prog.len(), 2, "the `x=bar` assignment must survive");

        // The control: a pattern with NO expansion is not a read, so a store
        // nothing else uses is still dead and IS dropped.
        let mut prog2 = vec![
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "y".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: str_("bar"),
                asm: None,
            },
            IrStmt::Case {
                discriminant: str_("bar"),
                clauses: vec![crate::ir::IrCaseClause {
                    patterns: vec!["bar".to_string()],
                    body: vec![],
                }],
            },
        ];
        assert!(transform(&mut prog2), "a literal pattern reads nothing");
        assert_eq!(prog2.len(), 1);

        // `${x}` and a pattern with surrounding text are reads too.
        assert_eq!(dollar_names_in("${x}"), vec!["x".to_string()]);
        assert_eq!(dollar_names_in("pre$x-suf"), vec!["x".to_string()]);
        assert!(dollar_names_in("*.txt").is_empty());
    }

    /// BASHC_FIX t03: a var read ONLY inside a pipeline stage must keep
    /// its store (`name="world"; echo $name | tr` dropped the assign).
    #[test]
    fn pipeline_read_keeps_store() {
        let prog = vec![
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "name".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: str_("world"),
                asm: None,
            },
            IrStmt::Pipeline {
                stages: vec![vec![IrStmt::Expr(IrExpr::Call {
                    func: "builtin".to_string(),
                    args: vec![
                        str_("echo"),
                        IrExpr::Array(vec![IrExpr::Call {
                            func: "getVar".to_string(),
                            args: vec![str_("name")],
                        }]),
                    ],
                })]],
                last_output: None,
                capture: None,
                cmd_str: None,
            },
        ];
        let mut stmts = prog;
        transform(&mut stmts);
        assert_eq!(stmts.len(), 2, "pipeline-read store dropped: {stmts:?}");
    }

    /// `IFS=$'\n'; echo "${config[*]}"` must keep the IFS store.
    ///
    /// The `[*]` join runs in the backend runtime and uses the FIRST IFS
    /// character, so a static census that only records the array name
    /// culls the IFS store and the join silently degrades to a space
    /// (064_07_complex_array_operations). `[@]` is the control: it joins
    /// with a hard space, so its store is still dead.
    #[test]
    fn star_join_keeps_ifs_store() {
        let read_of = |n: &str| {
            IrStmt::Expr(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![
                    str_("echo"),
                    IrExpr::Array(vec![IrExpr::Call {
                        func: "param".to_string(),
                        args: vec![str_(""), str_(n)],
                    }]),
                ],
            })
        };
        let ifs_store = IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "IFS".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: str_("\n"),
            asm: None,
        };

        let mut star = vec![ifs_store.clone(), read_of("config[*]")];
        transform(&mut star);
        assert_eq!(star.len(), 2, "[*] join must keep the IFS store: {star:?}");

        // `[@]` does not join on IFS, but the store must STILL survive:
        // IFS is a shell-implicit variable, and the same script can split
        // on it later (a `$(cmd)` capture, an unquoted `$v`). This control
        // asserted the old dead-store verdict before the shell-implicit
        // guard landed (fish-20261016-core-ir-drops).
        let mut at = vec![ifs_store, read_of("config[@]")];
        transform(&mut at);
        assert_eq!(at.len(), 2, "IFS is shell-implicit; keep its store: {at:?}");
    }

    /// A store to a shell-IMPLICIT variable survives even with no syntactic
    /// read anywhere: the shell consults the value itself (word splitting,
    /// command lookup, `cd`, globbing), so it can never be proven dead.
    ///
    /// Repro (core request fish-20261016-core-ir-drops case 2):
    /// `IFS=x x=5` emitted an EMPTY statement list — both assignment-only
    /// words vanished — and a later `$var` expansion or `$(cmd)` capture
    /// then split on the default IFS instead of the script's value.
    #[test]
    fn implicit_shell_var_store_is_never_dead() {
        let assign = |name: &str, value: &str| IrStmt::Assign {
            targets: vec![AssignTarget {
                var: name.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: str_(value),
            asm: None,
        };
        let mut stmts = vec![assign("IFS", "\n"), assign("x", "5")];
        transform(&mut stmts);
        let kept: Vec<&str> = stmts
            .iter()
            .filter_map(|s| match s {
                IrStmt::Assign { targets, .. } => targets.first().map(|t| t.var.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(kept, vec!["IFS"], "IFS store must survive; x is genuinely dead: {stmts:?}");
    }
}
