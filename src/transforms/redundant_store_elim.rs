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
//! `transforms.rs` (SH2_TRANSFORMS gated). Prereq: function-purity
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
        IrStmt::WriteFile { path, content, .. } => expr_reads(path, var) || expr_reads(content, var),
        IrStmt::Declare { init, .. } => init.as_ref().map(|i| expr_reads(i, var)).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => elements.iter().any(|e| expr_reads(e, var)),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_reads(cond, var)
                || then.iter().any(|s| stmt_reads(s, var))
                || elsifs.iter().any(|(c, b)| expr_reads(c, var) || b.iter().any(|s| stmt_reads(s, var)))
                || else_.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
            expr_reads(cond, var) || body.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::DoWhile { body, cond, .. } => {
            body.iter().any(|s| stmt_reads(s, var)) || expr_reads(cond, var)
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            expr_reads(cmd, var)
                || args.iter().any(|a| expr_reads(a, var))
                || redirects.iter().any(|r| expr_reads(r, var))
                || env.iter().any(|(_, e)| expr_reads(e, var))
        }
        IrStmt::Pipeline { stages, .. } => {
            stages.iter().any(|s| s.iter().any(|st| stmt_reads(st, var)))
        }
        IrStmt::Return(e) => e.as_ref().map(|e| expr_reads(e, var)).unwrap_or(false),
        IrStmt::Exit(e) => e.as_ref().map(|e| expr_reads(e, var)).unwrap_or(false),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_reads(expr, var),
        IrStmt::SetChildError(e) => expr_reads(e, var),
        IrStmt::Case { discriminant, clauses, .. } => {
            expr_reads(discriminant, var)
                || clauses.iter().any(|c| c.body.iter().any(|s| stmt_reads(s, var)))
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            inner.iter().any(|s| stmt_reads(s, var))
                || redirects.iter().any(|r| expr_reads(&r.target, var))
        }
        IrStmt::Function { body, .. } => body.iter().any(|s| stmt_reads(s, var)),
        IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
            b.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::Expr(e) => expr_reads(e, var),
        _ => false,
    }
}

fn expr_reads(e: &IrExpr, var: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == var,
        IrExpr::Index { var: v, key, .. } => v == var || expr_reads(key, var),
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads(lhs, var) || expr_reads(rhs, var),
        IrExpr::Arith(a) => arith_reads(a, var),
        IrExpr::Call { func, args, .. } => {
            call_reads_var(func, args, var) || args.iter().any(|a| expr_reads(a, var))
        }
        IrExpr::MethodCall { obj, args, .. } => expr_reads(obj, var) || args.iter().any(|a| expr_reads(a, var)),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_reads(cond, var) || expr_reads(then, var) || expr_reads(else_, var)
        }
        IrExpr::DefinedOr { expr, default, .. } => expr_reads(expr, var) || expr_reads(default, var),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Expr(x) => expr_reads(x, var),
            _ => false,
        }),
        IrExpr::Capture { expr, .. } => expr_reads(expr, var),
        IrExpr::Arrow(body) => body.iter().any(|s| stmt_reads(s, var)),
        IrExpr::Array(elems) => elems.iter().any(|e| expr_reads(e, var)),
        IrExpr::Object(fields) => fields.iter().any(|(_, e)| expr_reads(e, var)),
        _ => false,
    }
}

fn arith_reads(a: &ArithAst, var: &str) -> bool {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => v == var,
        ArithAst::Index { var: v, key, .. } => v == var || arith_reads(key, var),
        ArithAst::Bin { lhs, rhs, .. } => arith_reads(lhs, var) || arith_reads(rhs, var),
        ArithAst::Un { arg, .. } => arith_reads(arg, var),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_reads(test, var) || arith_reads(then, var) || arith_reads(else_, var)
        }
        ArithAst::Assign { rhs, .. } => arith_reads(rhs, var),
        ArithAst::IncDec { var: v, .. } => v == var,
        ArithAst::Cast { arg, .. } => arith_reads(arg, var),
        _ => false,
    }
}

/// Runtime-call variable access: `getVar`/`arrayIndex` read their name
/// arg, `param` reads its second arg, and the string-eval calls
/// (`arith`/`test`/`caseMatch`) may reference vars by name inside their
/// string payloads.
fn call_reads_var(func: &str, args: &[IrExpr], var: &str) -> bool {
    if matches!(func, "getVar" | "arrayIndex") {
        if let Some(IrExpr::Str(n, _)) = args.first() {
            return n == var;
        }
    }
    if func == "param" {
        if let Some(IrExpr::Str(n, _)) = args.get(1) {
            return n == var;
        }
    }
    if matches!(func, "arith" | "test" | "caseMatch") {
        return args.iter().any(|a| match a {
            IrExpr::Str(s, _) => str_maybe_reads(s, var),
            _ => false,
        });
    }
    false
}

/// Does the string reference `var` as a standalone token?
fn str_maybe_reads(s: &str, var: &str) -> bool {
    if var.is_empty() {
        return false;
    }
    let mut rest = s;
    while let Some(pos) = rest.find(var) {
        let before = pos == 0
            || !rest[..pos]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        let after = pos + var.len() >= rest.len()
            || !rest[pos + var.len()..]
                .chars()
                .next()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        if before && after {
            return true;
        }
        rest = &rest[pos + var.len()..];
    }
    false
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

    fn str_lit(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn int_lit(n: i64) -> IrExpr {
        IrExpr::Int(n)
    }

    fn getvar(var: &str) -> IrExpr {
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![str_lit(var)],
        }
    }

    fn expr_stmt(e: IrExpr) -> IrStmt {
        IrStmt::Expr(e)
    }

    /// `x=A; y="v$x"; x=B` — the intermediate `y="v$x"` READS x via a
    /// getVar call inside an interpolation; the scan must stop there and
    /// keep the `x=A` store (its value is observed).
    #[test]
    fn keeps_store_read_via_getvar_in_assign_rhs() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign(
                "y",
                IrExpr::Interpolate(vec![
                    crate::ir::InterpPart::Lit("v".to_string()),
                    crate::ir::InterpPart::Expr(Box::new(getvar("x"))),
                ]),
            ),
            assign("x", int_lit(6)),
        ];
        // the first store must survive: y observes x=5
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
        assert_eq!(stmts.len(), 3);
    }

    /// `x=A; x=B` with NO read between — the intermediate store drops.
    #[test]
    fn drops_store_with_no_read_between() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign("x", int_lit(6)),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
        if let IrStmt::Assign { expr, .. } = &stmts[0] {
            assert_eq!(expr, &int_lit(6));
        } else {
            panic!("expected Assign");
        }
    }

    /// `x=A; echo $y` where the echo reads x via getVar inside an exec
    /// arg — the store must survive.
    #[test]
    fn keeps_store_read_via_exec_arg() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            expr_stmt(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![
                    str_lit("echo"),
                    IrExpr::Array(vec![IrExpr::Interpolate(vec![
                        crate::ir::InterpPart::Lit("v".to_string()),
                        crate::ir::InterpPart::Expr(Box::new(getvar("x"))),
                    ])]),
                ],
            }),
            assign("x", int_lit(6)),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
    }

    /// `x=A; if cond; then echo $x; fi; x=B` — the conditional read of x
    /// must stop the scan.
    #[test]
    fn keeps_store_read_inside_if() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            IrStmt::If {
                cond: getvar("c"),
                then: vec![expr_stmt(getvar("x"))],
                elsifs: vec![],
                else_: vec![],
            },
            assign("x", int_lit(6)),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
    }

    /// `x=A; y=$((x+1)); x=B` — the RHS reads x via native arith.
    #[test]
    fn keeps_store_read_via_arith() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign(
                "y",
                IrExpr::Arith(Box::new(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(ArithAst::Var("x".to_string())),
                    rhs: Box::new(ArithAst::Num(1)),
                })),
            ),
            assign("x", int_lit(6)),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
    }
}
