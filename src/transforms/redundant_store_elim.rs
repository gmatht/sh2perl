//! redundant-store-elim — drop an assignment whose value is overwritten
//! by a LATER store to the same scalar variable before anything can read
//! it (`x=A; …; x=B` with no read of x in between). The complement of
//! dead-store-elim (which needs ZERO reads anywhere) — here the var IS
//! read later, but an intermediate store is dead.
//!
//! ## Need
//! Real and generated code reassigns a scalar (`x="$a"; …; x="$b"`); each
//! write emits a store. Only the last write before a read matters.
//!
//! ## Scope — the sound rule
//! A scalar store `x=A` at index `i` is dropped iff ALL of:
//!   - `A` is PURE — no `Capture`, `Arrow`, `Call`, `MethodCall` (an
//!     impure store's side effect must not be skipped),
//!   - there is a LATER scalar store to the same `x` in the same block,
//!   - NO read of `x` and NO "indirect observer" (a function `Call`,
//!     `Capture`, `Subshell`, `Background`, `Exec`, `WriteFile`,
//!     `setVar`) occurs strictly between `i` and that later store.
//! Dropping the earlier store is then unconditionally safe: its value is
//! replaced before any possible observation, whether `x` is a local or a
//! shared-store global (the later store is what any caller observes).
//! The LAST store in a block is never dropped (it is the surviving
//! value). Trailing-stores-before-end-of-function are likewise kept — a
//! later store is REQUIRED.
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest). Registered in
//! `transforms.rs` (DEBASHC_TRANSFORMS gated). Prereq: function-purity
//! (the pure-expression gate; a local conservative fallback is used here).

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    // top-level list
    c |= drop_redundant(stmts);
    // function bodies (recurse)
    let mut i = 0;
    while i < stmts.len() {
        if let IrStmt::Function { name: _, body, .. } = &mut stmts[i] {
            c |= drop_redundant(body);
        }
        i += 1;
    }
    c
}

fn drop_redundant(stmts: &mut Vec<IrStmt>) -> bool {
    let n = stmts.len();
    let mut remove = vec![false; n];
    for i in 0..n {
        let var = match scalar_pure_assign(&stmts[i]) {
            Some(v) => v,
            None => continue,
        };
        // scan forward: any read/observer stops it; a later scalar store
        // to the same var makes store-i dead
        let mut j = i + 1;
        while j < n {
            if stmt_reads(&stmts[j], &var) || indirect_observer(&stmts[j]) {
                break;
            }
            if scalar_assign_to(&stmts[j], &var) {
                remove[i] = true;
                break;
            }
            j += 1;
        }
    }
    let changed = remove.iter().any(|&r| r);
    *stmts = stmts
        .drain(..)
        .enumerate()
        .filter_map(|(i, s)| if remove[i] { None } else { Some(s) })
        .collect();
    changed
}

/// A single scalar-target Assign whose value is pure; returns the var.
fn scalar_pure_assign(st: &IrStmt) -> Option<String> {
    match st {
        IrStmt::Assign { targets, expr, .. } if targets.len() == 1 && targets[0].indices.is_empty() => {
            if expr_pure(expr) {
                Some(targets[0].var.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// A scalar Assign to the given var (purity not required — a later
/// impure store still overwrites the value; we drop the EARLIER pure one).
fn scalar_assign_to(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => {
            targets.len() == 1 && targets[0].indices.is_empty() && targets[0].var == var
        }
        _ => false,
    }
}

fn expr_pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Var(_, _) | IrExpr::Str(_, _) | IrExpr::Range { .. } | IrExpr::Bool(_) => true,
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs) && expr_pure(rhs),
        IrExpr::Arith(_) => true,
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            crate::ir::InterpPart::Lit(_) => true,
            crate::ir::InterpPart::Expr(x) => expr_pure(x),
        }),
        _ => false, // Call / Capture / Arrow / MethodCall / array — refuse
    }
}

fn stmt_reads(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_reads(expr, var),
        IrStmt::Output { value, .. } => expr_reads(value, var),
        IrStmt::Declare { init, .. } => init.as_ref().map(|i| expr_reads(i, var)).unwrap_or(false),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_reads(cond, var)
                || then.iter().any(|s| stmt_reads(s, var))
                || elsifs.iter().any(|(c, b)| expr_reads(c, var) || b.iter().any(|s| stmt_reads(s, var)))
                || else_.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, var: _ } => {
            expr_reads(cond, var) || body.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::Expr(e) => expr_reads(e, var),
        _ => false,
    }
}

fn expr_reads(e: &IrExpr, var: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == var,
        IrExpr::Index { var: v, .. } => v == var,
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads(lhs, var) || expr_reads(rhs, var),
        IrExpr::Arith(a) => arith_reads(a, var),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Expr(x) => expr_reads(x, var),
            _ => false,
        }),
        _ => false,
    }
}

fn arith_reads(a: &ArithAst, var: &str) -> bool {
    match a {
        ArithAst::Var(v) => v == var,
        ArithAst::Index { var: v, .. } => v == var,
        ArithAst::Bin { lhs, rhs, .. } => arith_reads(lhs, var) || arith_reads(rhs, var),
        ArithAst::Un { arg, .. } => arith_reads(arg, var),
        _ => false,
    }
}

/// A statement that could read `x` indirectly (a call/capture/subshell/
/// exec/write) — conservatively stops the drop-scan.
fn indirect_observer(st: &IrStmt) -> bool {
    matches!(
        st,
        IrStmt::Exec { .. }
            | IrStmt::Subshell(_)
            | IrStmt::Background(_)
            | IrStmt::WriteFile { .. }
            | IrStmt::Expr(
                IrExpr::Call { .. }
                    | IrExpr::Capture { .. }
                    | IrExpr::MethodCall { .. }
                    | IrExpr::Arrow(_)
            )
    ) || (matches!(st, IrStmt::Return(_) | IrStmt::Exit(_)))
}
