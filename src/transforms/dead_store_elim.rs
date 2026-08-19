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
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). Runs inside
//! `ast_to_ir`. Consumes the escape-classes verdict (optional, sharpens
//! the escapes set) and feeds function-purity (a removed dead var cannot
//! be a purity input).

use std::collections::HashSet;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut reads: HashSet<String> = HashSet::new();
    let mut writes: HashSet<String> = HashSet::new();
    let mut escapes: HashSet<String> = HashSet::new();
    census(stmts, &mut reads, &mut writes, &mut escapes);

    let dead: Vec<String> = writes
        .iter()
        .filter(|v| !reads.contains(*v) && !escapes.contains(*v))
        .cloned()
        .collect();
    if dead.is_empty() {
        return false;
    }
    let before = stmts.len();
    *stmts = stmts.drain(..).filter_map(|s| purge(s, &dead)).collect();
    stmts.len() != before
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
                for s in &cl.body {
                    census_stmt(s, reads, writes, escapes, escaping);
                }
            }
        }
        IrStmt::Redirect { inner, .. } => {
            for s in inner {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body {
                census_stmt(s, reads, writes, escapes, escaping);
            }
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) => {
            for s in v {
                census_stmt(s, reads, writes, escapes, true);
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
            // recorded by their targets
            if matches!(func.as_str(), "getVar" | "arrayIndex" | "param") {
                let idx = if func == "param" { 1 } else { 0 };
                if let Some(IrExpr::Str(n, _)) = args.get(idx) {
                    reads.insert(n.clone());
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
        _ => {}
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
        ArithAst::Index { var, .. } => {
            reads.insert(var.clone());
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
        IrStmt::Function { name, body, .. } => Some(IrStmt::Function {
            name,
            body: body.into_iter().filter_map(|s| purge(s, dead)).collect(),
            named_blocks: Vec::new(),
        }),
        other => Some(other),
    }
}

