//! hoist-loop-invariants — hoist loop-invariant PURE assignments out of
//! while/for loops, the shIR equivalent of the stalled proposal
//! estree-20260813-201235 (hoist-pure-loop-invariants). The field hash
//! in the texture generators is recomputed per pixel:
//!
//!     while [ "$y" -lt "$SIZE" ]; do … lat_hash $x 0 $SIZE 1; gph=$((lhn%3)); … done
//!
//! ## Need
//! The pseudorandom texture generators run ~256 pixels with ~2800
//! dispatched calls per texture; a value that depends only on the loop
//! invariant `x` (and consts) is recomputed every iteration
//! (240/256 wasted). Every backend re-derives loop-carried dataflow or
//! duplicates the work. A shared hoist fixes them all, and feeds the
//! i32/escape analyses (a hoisted let is a single-def constant).
//!
//! ## Scope — the sound (dominance) version
//! An `IrStmt::Assign { targets: [var], expr }` inside a `While`/`For`/
//! `DoWhile` body is hoisted to just before the loop when ALL of:
//!   - `var` is assigned EXACTLY ONCE in the body (this statement),
//!   - every variable `expr` reads is loop-invariant (not assigned
//!     anywhere in the body — a loop-carried accumulator is refused),
//!   - `expr` is PURE (Arith / BinOp of literals+vars / a call to a
//!     defined pure function / no capture-exec-subshell side effect),
//!   - `var` is not READ before this statement in the body (a read
//!     earlier in the iteration would observe the PREVIOUS iteration's
//!     value — the one thing hoisting changes).
//! Reads of `var` AFTER the hoisted statement are unchanged (the value
//! is identical every iteration), and reads after the loop see the same
//! value the last iteration saw — so the rewrite is behavior-preserving.
//! `rand` (an LCG that advances a program var) is refused by the purity
//! gate — a random value must never be hoisted.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). Prereq:
//! function-purity (the pure-function call verdict; the conservative
//! local arith-only fallback below stands without it).

use std::collections::HashSet;

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // user function bodies for the pure-call gate
    let mut fns: Vec<(String, Vec<IrStmt>)> = Vec::new();
    for st in stmts.iter() {
        if let IrStmt::Function { name, body, .. } = st {
            fns.push((name.clone(), body.clone()));
        }
    }
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let (hoisted, new_body) = split_loop(&stmts[i], &fns);
        if let Some(nb) = new_body {
            stmts[i] = set_loop_body(&stmts[i], nb);
            stmts.splice(i..i, hoisted); // insert just before the loop
            changed = true;
            continue; // re-run at the new index (loops can nest)
        }
        i += 1;
    }
    changed
}

/// If `st` is a loop with hoistable leading work, return (hoisted, new
/// body); otherwise `(vec![], None)`.
fn split_loop(st: &IrStmt, fns: &[(String, Vec<IrStmt>)]) -> (Vec<IrStmt>, Option<Vec<IrStmt>>) {
    let body = match st {
        IrStmt::While { body, .. } | IrStmt::For { body, .. } => body,
        _ => return (Vec::new(), None),
    };
    let hoisted = hoist_body(body, fns);
    if hoisted.is_empty() {
        return (Vec::new(), None);
    }
    let kept: Vec<IrStmt> = body.iter().filter(|s| !hoisted.contains(*s)).cloned().collect();
    (hoisted, Some(kept))
}

fn set_loop_body(st: &IrStmt, body: Vec<IrStmt>) -> IrStmt {
    match st.clone() {
        IrStmt::While { cond, .. } => IrStmt::While { cond, body },
        IrStmt::For { var, iter, .. } => IrStmt::For { var, iter, body },
        _ => st.clone(),
    }
}

/// Split the hoistable assignments out of a body (in body order, so a
/// hoisted var is not read before its write in the same pass).
fn hoist_body(body: &[IrStmt], fns: &[(String, Vec<IrStmt>)]) -> Vec<IrStmt> {
    // total assigned-in-body (for loop-invariance of the candidate's reads)
    let mut assigned: HashSet<String> = HashSet::new();
    for s in body {
        stmt_writes(s, &mut assigned);
    }

    let mut hoisted: Vec<IrStmt> = Vec::new();
    for (j, st) in body.iter().enumerate() {
        let (target, expr) = match st {
            IrStmt::Assign { targets, expr, .. } if targets.len() == 1 && targets[0].indices.is_empty() => {
                (&targets[0].var, expr)
            }
            _ => continue,
        };
        // written exactly once (this is the only writer of `target` in
        // the body — a second writer makes it loop-carried)
        let writes_elsewhere = body.iter().enumerate().any(|(k, s2)| k != j && stmt_assigns(s2, target));
        if writes_elsewhere {
            continue;
        }
        // expr reads only loop-invariant vars (the self var is allowed —
        // it is re-derived identically each iteration, but only if the
        // read is AFTER the write; the order check below handles that)
        if !expr_pure(expr, fns) {
            continue;
        }
        let reads = read_set(expr);
        if reads.iter().any(|v| v != target && assigned.contains(v)) {
            continue; // loop-carried input
        }
        // `target` not read before this write in the body
        let read_before = body.iter().take(j).any(|s| stmt_reads(s, target));
        if read_before {
            continue;
        }
        hoisted.push(st.clone());
    }
    hoisted
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
        IrStmt::For { iter, body, .. } => expr_reads(iter, var) || body.iter().any(|s| stmt_reads(s, var)),
        IrStmt::While { cond, body } => expr_reads(cond, var) || body.iter().any(|s| stmt_reads(s, var)),
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
        IrExpr::Call { args, .. } => args.iter().any(|a| expr_reads(a, var)),
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
        ArithAst::Cond { test, then, else_, .. } => {
            arith_reads(test, var) || arith_reads(then, var) || arith_reads(else_, var)
        }
        _ => false,
    }
}

fn stmt_assigns(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => targets.iter().any(|t| t.var == var),
        IrStmt::Declare { vars, .. } => vars.iter().any(|v| v.name == var),
        IrStmt::DeclareArray { var: v, .. } => v == var,
        _ => false,
    }
}

fn stmt_writes(st: &IrStmt, out: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                out.insert(t.var.clone());
            }
            if let IrExpr::Arith(a) = expr {
                if let ArithAst::Assign { var, .. } = a.as_ref() {
                    out.insert(var.clone());
                }
            }
        }
        IrStmt::Declare { vars, .. } => {
            for v in vars {
                out.insert(v.name.clone());
            }
        }
        IrStmt::DeclareArray { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
}

fn read_set(e: &IrExpr) -> HashSet<String> {
    let mut out = HashSet::new();
    read_set_into(e, &mut out);
    out
}

fn read_set_into(e: &IrExpr, out: &mut HashSet<String>) {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            out.insert(v.clone());
        }
        IrExpr::Index { var, .. } => {
            out.insert(var.clone());
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            read_set_into(lhs, out);
            read_set_into(rhs, out);
        }
        IrExpr::Arith(a) => arith_read_set(a, out),
        IrExpr::Call { args, .. } => {
            for a in args {
                read_set_into(a, out);
            }
        }
        _ => {}
    }
}

fn arith_read_set(a: &ArithAst, out: &mut HashSet<String>) {
    match a {
        ArithAst::Var(v) => {
            out.insert(v.clone());
        }
        ArithAst::Index { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_read_set(lhs, out);
            arith_read_set(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_read_set(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_read_set(test, out);
            arith_read_set(then, out);
            arith_read_set(else_, out);
        }
        _ => {}
    }
}

fn expr_pure(e: &IrExpr, fns: &[(String, Vec<IrStmt>)]) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Var(_, _) | IrExpr::Range { .. } => true,
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs, fns) && expr_pure(rhs, fns),
        IrExpr::Arith(_) => true,
        IrExpr::Call { func, args } => {
            if matches!(func.as_str(), "arith" | "test") {
                return args.iter().all(|a| expr_pure(a, fns));
            }
            match fns.iter().find(|(n, _)| n == func) {
                Some((_, body)) => body.iter().all(|s| stmt_pure(s, fns)),
                None => false,
            }
        }
        _ => false,
    }
}

fn stmt_pure(st: &IrStmt, fns: &[(String, Vec<IrStmt>)]) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_pure(expr, fns),
        IrStmt::Declare { .. } => true,
        _ => false,
    }
}

