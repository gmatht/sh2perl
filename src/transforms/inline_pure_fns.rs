//! inline-pure-fns — inline pure, non-recursive LEAF functions at their
//! `fnCall` sites (stalled estree-20260813-182431-inline-pure-fns,
//! converted to a marketplace offer).
//!
//! name: inline-pure-fns
//! depends: []
//! prereqs: []
//! invariant: ONLY single-expression pure-leaf functions are inlined
//!   (body is one IrStmt::Expr of getVar/setVar/arith/arrayIndex — the
//!   get_cell → map_get chain class; ~1500–2300 sh2.fnCall dispatches per
//!   frame on MIMEcroft). The positional reads (getVar("N")) substitute
//!   the call's Nth arg; calls with IMPURE args (capture/exec/…) refuse.
//!   Every other function (multi-stmt, If, recursion, subprocess) is left
//!   untouched — refuse > guess. Pure leaves have no process/redirect/
//!   subshell, so inlining is behaviourally identical.
//! scope: [estree] (the sh2.* fnCall dispatch; other backends may adopt)
//! updates: inline-pure-fns (v1 in done/ had Array-shape compile errors — this re-offer fixes them)

use crate::ir::{IrExpr, IrStmt, InterpPart};

/// Pure = no capture/exec/process/pipeline — store/arith reads+writes only.
fn expr_pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(_, _) | IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Var(_, _) => true,
        IrExpr::Call { func, args } => match func.as_str() {
            "getVar" | "setVar" | "arith" | "arithEval" | "arrayIndex" | "arraySet" | "assign" => {
                args.iter().all(expr_pure)
            }
            _ => false,
        },
        IrExpr::BinOp { lhs, op, rhs } => {
            let _ = op;
            expr_pure(lhs) && expr_pure(rhs)
        }
        IrExpr::Array(elems) => elems.iter().all(expr_pure),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(x) => expr_pure(x),
        }),
        _ => false,
    }
}

/// A single-expression pure leaf: `IrStmt::Expr(pure)`.
fn leaf_value(s: &IrStmt) -> Option<&IrExpr> {
    match s {
        IrStmt::Expr(e) if expr_pure(e) => Some(e),
        _ => None,
    }
}

/// Substitute the function's positional reads (getVar("N")) with the call's
/// Nth arg; recurse into every other expression shape unchanged.
fn subst(e: &IrExpr, args: &[IrExpr]) -> IrExpr {
    match e {
        IrExpr::Call { func, args: a } => {
            if func == "getVar" {
                if let Some(IrExpr::Str(n, _)) = a.first() {
                    if let Ok(idx) = n.parse::<usize>() {
                        if idx >= 1 && idx <= args.len() {
                            return args[idx - 1].clone();
                        }
                    }
                }
            }
            IrExpr::Call {
                func: func.clone(),
                args: a.iter().map(|x| subst(x, args)).collect(),
            }
        }
        IrExpr::BinOp { lhs, op, rhs } => IrExpr::BinOp {
            lhs: Box::new(subst(lhs, args)),
            op: op.clone(),
            rhs: Box::new(subst(rhs, args)),
        },
        IrExpr::Array(elems) => IrExpr::Array(elems.iter().map(|x| subst(x, args)).collect()),
        IrExpr::Interpolate(parts) => IrExpr::Interpolate(
            parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(_) => p.clone(),
                    InterpPart::Expr(x) => InterpPart::Expr(Box::new(subst(x, args))),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Rewrite one statement's fnCall sites: `fnCall("name", pure-args…)` where
/// `name` is a pure leaf → the substituted leaf expression.
fn rewrite_stmt(s: &IrStmt, leaves: &[(String, IrExpr)], changed: &mut bool) -> IrStmt {
    match s {
        IrStmt::Expr(e) => IrStmt::Expr(rewrite_expr(e, leaves, changed)),
        IrStmt::Assign { targets, expr, asm, .. } => IrStmt::Assign {
            targets: targets.clone(),
            expr: rewrite_expr(expr, leaves, changed),
            asm: asm.clone(),
        },
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => IrStmt::If {
            cond: rewrite_expr(cond, leaves, changed),
            then: then.iter().map(|x| rewrite_stmt(x, leaves, changed)).collect(),
            elsifs: elsifs
                .iter()
                .map(|(c, b)| {
                    (
                        rewrite_expr(c, leaves, changed),
                        b.iter().map(|x| rewrite_stmt(x, leaves, changed)).collect(),
                    )
                })
                .collect(),
            else_: else_
                .iter()
                .map(|x| rewrite_stmt(x, leaves, changed))
                .collect(),
        },
        other => other.clone(),
    }
}

fn rewrite_expr(e: &IrExpr, leaves: &[(String, IrExpr)], changed: &mut bool) -> IrExpr {
    match e {
        IrExpr::Call { func, args } if func == "fnCall" => {
            if let Some(IrExpr::Str(name, _)) = args.first() {
                let call_args = &args[1..];
                if call_args.iter().all(expr_pure) {
                    if let Some((_, body)) = leaves.iter().find(|(n, _)| n == name) {
                        *changed = true;
                        return subst(body, call_args);
                    }
                }
            }
            IrExpr::Call {
                func: func.clone(),
                args: args.iter().map(|x| rewrite_expr(x, leaves, changed)).collect(),
            }
        }
        IrExpr::Call { func, args } => IrExpr::Call {
            func: func.clone(),
            args: args.iter().map(|x| rewrite_expr(x, leaves, changed)).collect(),
        },
        IrExpr::BinOp { lhs, op, rhs } => IrExpr::BinOp {
            lhs: Box::new(rewrite_expr(lhs, leaves, changed)),
            op: op.clone(),
            rhs: Box::new(rewrite_expr(rhs, leaves, changed)),
        },
        IrExpr::Array(elems) => IrExpr::Array(elems.iter().map(|x| rewrite_expr(x, leaves, changed)).collect()),
        IrExpr::Interpolate(parts) => IrExpr::Interpolate(
            parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(_) => p.clone(),
                    InterpPart::Expr(x) => {
                        InterpPart::Expr(Box::new(rewrite_expr(x, leaves, changed)))
                    }
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The transform entry (the §11 offer contract): returns true when any
/// call was inlined.
pub fn inline_pure_fns(stmts: &mut Vec<IrStmt>) -> bool {
    let mut leaves: Vec<(String, IrExpr)> = Vec::new();
    for s in stmts.iter() {
        if let IrStmt::Function { name, body, .. } = s {
            if body.len() == 1 {
                if let Some(v) = leaf_value(&body[0]) {
                    leaves.push((name.clone(), v.clone()));
                }
            }
        }
    }
    if leaves.is_empty() {
        return false;
    }
    let mut changed = false;
    let rewritten: Vec<IrStmt> = stmts
        .iter()
        .map(|s| rewrite_stmt(s, &leaves, &mut changed))
        .collect();
    *stmts = rewritten;
    changed
}
