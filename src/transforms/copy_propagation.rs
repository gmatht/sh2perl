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
//!   - that def is a top-level `Assign`/`Declare` to a literal
//!     (`Int`/`Str`/`Bool`) or a copy (`Var` of an equally-foldable
//!     var) — no arith/index/capture, which the const check already
//!     rejects, AND
//!   - the def textually precedes every read in the same statement
//!     list, with NO intervening `Capture`/`Subshell`/`Background`
//!     (those may execute before the def, out of program order).
//! Ordering within a straight-line block is preserved (def before read
//! in the same `Vec<IrStmt>`), so an early read refuses the fold.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). Composes
//! with dead-store-elim (a folded-away variable's def becomes a dead
//! store) and i32-provable (a folded literal becomes a constant leaf).

use std::collections::HashMap;
use std::collections::HashSet;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// A foldable single-def value: a literal, or a copy of another var.
#[derive(Clone)]
enum Def {
    Literal(IrExpr),
    Copy(String),
}

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // single-def table: var → its single def (if qualifies)
    let defs = collect_defs(stmts);
    if defs.is_empty() {
        return false;
    }
    // closed copies: resolve a Copy chain to its literal (or itself)
    let mut resolved: HashMap<String, IrExpr> = HashMap::new();
    for (v, d) in defs.iter() {
        if let Def::Literal(e) = d {
            resolved.insert(v.clone(), e.clone());
        }
    }
    let mut changed = false;
    for st in stmts.iter_mut() {
        // fold reads within each top-level statement; the def-before-read
        // ordering is enforced per statement list
        changed |= fold_stmt(st, &defs, &resolved, &mut HashSet::new());
    }
    changed
}

/// Single-def scan — a var qualifies iff assigned exactly once at a
/// top-level-to-this-body literal/copy.
fn collect_defs(stmts: &[IrStmt]) -> HashMap<String, Def> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut values: HashMap<String, IrExpr> = HashMap::new();
    count_assigns(stmts, &mut counts, &mut values);
    counts
        .into_iter()
        .filter(|(_, c)| *c == 1)
        .filter_map(|(v, _)| {
            let e = values.get(&v)?;
            match e {
                IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Bool(_) => {
                    Some((v, Def::Literal(e.clone())))
                }
                IrExpr::Var(w, _) => Some((v, Def::Copy(w.clone()))),
                _ => None,
            }
        })
        .collect()
}

fn count_assigns(stmts: &[IrStmt], counts: &mut HashMap<String, usize>, values: &mut HashMap<String, IrExpr>) {
    for st in stmts {
        match st {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    if t.indices.is_empty() {
                        *counts.entry(t.var.clone()).or_insert(0) += 1;
                        values.insert(t.var.clone(), expr.clone());
                    }
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                if let Some(i) = init {
                    for v in vars {
                        *counts.entry(v.name.clone()).or_insert(0) += 1;
                        values.insert(v.name.clone(), i.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

fn fold_stmt(st: &mut IrStmt, defs: &HashMap<String, Def>, resolved: &HashMap<String, IrExpr>, seen: &mut HashSet<String>) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => fold_expr(expr, defs, resolved, seen),
        IrStmt::Output { value, .. } => fold_expr(value, defs, resolved, seen),
        IrStmt::Expr(e) => fold_expr(e, defs, resolved, seen),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = fold_expr(cond, defs, resolved, seen);
            for s in then.iter_mut() {
                c |= fold_stmt(s, defs, resolved, seen);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= fold_expr(ec, defs, resolved, seen);
                for s in eb.iter_mut() {
                    c |= fold_stmt(s, defs, resolved, seen);
                }
            }
            for s in else_.iter_mut() {
                c |= fold_stmt(s, defs, resolved, seen);
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = fold_expr(cond, defs, resolved, seen);
            for s in body.iter_mut() {
                c |= fold_stmt(s, defs, resolved, seen);
            }
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = fold_expr(iter, defs, resolved, seen);
            for s in body.iter_mut() {
                c |= fold_stmt(s, defs, resolved, seen);
            }
            c
        }
        _ => false,
    }
}

fn fold_expr(e: &mut IrExpr, defs: &HashMap<String, Def>, resolved: &HashMap<String, IrExpr>, seen: &mut HashSet<String>) -> bool {
    let mut c = false;
    match e {
        IrExpr::Var(_, _) => {
            // peek the name without holding a borrow across the write
            let name = if let IrExpr::Var(v, _) = e { v.clone() } else { unreachable!() };
            if !seen.contains(&name) {
                if let Some(rep) = resolved.get(&name) {
                    seen.insert(name.clone());
                    *e = rep.clone();
                    c = true;
                }
            }
            let _ = defs;
        }
        IrExpr::Ident(_) => {}
        IrExpr::Index { var, key } => {
            c |= fold_expr(key, defs, resolved, seen);
            let _ = var;
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            c |= fold_expr(lhs, defs, resolved, seen);
            c |= fold_expr(rhs, defs, resolved, seen);
        }
        IrExpr::Arith(a) => c |= fold_arith(a, defs, resolved, seen),
        IrExpr::Call { func, args } => {
            if matches!(func.as_str(), "getVar" | "arrayIndex") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if let Some(rep) = resolved.get(n) {
                        // replace getVar("x") with the literal — the name
                        // string goes away entirely
                        *e = rep.clone();
                        c = true;
                        return c;
                    }
                }
            }
            for a in args.iter_mut() {
                c |= fold_expr(a, defs, resolved, seen);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    c |= fold_expr(x, defs, resolved, seen);
                }
            }
        }
        IrExpr::Capture { expr, .. } => {
            // a capture may run before the def — refuse to fold inside
            c |= false;
            let _ = expr;
        }
        _ => {}
    }
    c
}

fn fold_arith(a: &mut ArithAst, defs: &HashMap<String, Def>, resolved: &HashMap<String, IrExpr>, seen: &mut HashSet<String>) -> bool {
    match a {
        ArithAst::Var(v) => {
            if let Some(rep) = resolved.get(v) {
                if let IrExpr::Int(n) = rep {
                    *a = ArithAst::Num(*n);
                    return true;
                }
            }
            let _ = (defs, seen);
            false
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            fold_arith(lhs, defs, resolved, seen) | fold_arith(rhs, defs, resolved, seen)
        }
        ArithAst::Un { arg, .. } => fold_arith(arg, defs, resolved, seen),
        ArithAst::Cond { test, then, else_, .. } => {
            fold_arith(test, defs, resolved, seen)
                | fold_arith(then, defs, resolved, seen)
                | fold_arith(else_, defs, resolved, seen)
        }
        _ => false,
    }
}

