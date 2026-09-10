//! split-set — `set[int]` → small ∪ big partitions (runtime routing).
//!
//! ## Need
//! The py-sh-go frontend lowers a Python `set[int]` to ONE homogeneous
//! shell array, and its `setElemDom` verdict latches the whole set to
//! "big" the moment ANY element is unproven within ±2^53. So a set that
//! is *mostly* small with an occasional big element (the t89/t90
//! `small_factors` shape) stores and sorts the common small elements as
//! BigInts too — correct, but the hot path pays bigint cost for values
//! that are provably small.
//!
//! ## The rewrite
//! A set `s` built via `setArray(s, [])` + `setArrayAppend(s, [v])` is
//! split into two arrays — `s` (the small partition, i53-safe) and
//! `s__big` (the big partition) — with a per-`add()` runtime routing:
//!
//! ```text
//! add(s, v):  if isSmallInt(v) → setArrayAppend(s, [v])
//!             else             → setArrayAppend(s__big, [v])
//! ```
//!
//! `sortedIntJoin(s)` / `sortedBigintJoin(s)` become a merge of the two
//! partitions (`sortedJoinMerge(s, s__big)`): the small partition sorts
//! with fast Number comparison, the big partition with BigInt, then a
//! linear merge. The output is byte-identical to the unsplit sort (the
//! firewall: levels change bytes/cycles, never behavior) — the win is
//! that the common small bulk avoids BigInt.
//!
//! ## Soundness (fail-closed)
//! - The set must be **append-only**: an empty `setArray(s, [])` init and
//!   `setArrayAppend(s, [v])` appends only. Indexed writes, deletes,
//!   assoc, dynamic, non-empty inits, and any other reference to `s`
//!   (scalar reads, iteration) refuse — the split is skipped.
//! - All refs lie in ONE scope (the statement list being transformed).
//! - The routing check runs in the WIDE domain: `sh2.isSmallInt(v)`
//!   parses the element as BigInt and compares against ±(2^53−1) — exact,
//!   no lossy Number conversion.
//! - **Dedup stays correct** because the partition is a function of the
//!   value, not the expression: a given value always routes to the same
//!   partition, so dedup-within-partition preserves global set semantics.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Speed-class:
//! gated by `SH2_SPLIT_SETS` (set at `-O3`/`-O4` by the CLI, mirroring
//! `set_dual_loops`). The routing/merge are generic `sh2.*` calls each
//! backend renders with its own narrow bound (i53 for JS; i64/u64 for
//! C-family — see core-requests/py-sh-go-20260909-split-set-union.md).
//!
//! The two-loop iteration (`for x in s` → narrow loop over `s` + wide
//! loop over `s__big`) is the follow-up; iterated sets are currently
//! refused (deferred).

use crate::ir::{ArithAst, AssignTarget, IrExpr, IrStmt};

/// Apply the transform to a statement list. Returns whether anything
/// changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    if !crate::shir::split_sets_enabled() {
        return false;
    }
    let mut changed = false;
    changed |= split_sets_in_scope(stmts);
    for s in stmts.iter_mut() {
        changed |= transform_stmt(s);
    }
    changed
}

/// Recurse into nested scopes (function bodies, if/loop bodies, arrows)
/// to split sets defined there. Does NOT rewrite joins — that happens in
/// `rewrite_scope` for the set being split.
fn transform_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::For { body, .. } => {
            let mut changed = split_sets_in_scope(body);
            for b in body.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed
        }
        IrStmt::While { body, .. } => {
            let mut changed = split_sets_in_scope(body);
            for b in body.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed
        }
        IrStmt::DoWhile { body, .. } => {
            let mut changed = split_sets_in_scope(body);
            for b in body.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Function { body, .. } => {
            let mut changed = split_sets_in_scope(body);
            for b in body.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed
        }
        IrStmt::If {
            then, elsifs, else_, ..
        } => {
            let mut changed = split_sets_in_scope(then);
            for b in then.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed |= split_sets_in_scope(else_);
            for b in else_.iter_mut() {
                changed |= transform_stmt(b);
            }
            for (_, b) in elsifs.iter_mut() {
                changed |= split_sets_in_scope(b);
                for s in b.iter_mut() {
                    changed |= transform_stmt(s);
                }
            }
            changed
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                changed |= split_sets_in_scope(stage);
                for b in stage.iter_mut() {
                    changed |= transform_stmt(b);
                }
            }
            changed
        }
        IrStmt::Redirect { inner, .. } => {
            let mut changed = split_sets_in_scope(inner);
            for b in inner.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed
        }
        IrStmt::Case { clauses, .. } => {
            let mut changed = false;
            for c in clauses.iter_mut() {
                changed |= split_sets_in_scope(&mut c.body);
                for b in c.body.iter_mut() {
                    changed |= transform_stmt(b);
                }
            }
            changed
        }
        IrStmt::Expr(e) => expr_rewrite(e, ""),
        IrStmt::Output { value, .. } => expr_rewrite(value, ""),
        IrStmt::Assign { expr, .. } => expr_rewrite(expr, ""),
        _ => false,
    }
}

/// Rewrite `sorted*Join(name)` → `sortedJoinMerge(name, name__big)` in an
/// expression tree (post-order). When `name` is empty, rewrites nothing
/// (the no-op used by `transform_stmt`'s expression arms — join rewriting
/// is scoped to the set being split in `rewrite_scope`). Returns whether
/// changed.
fn expr_rewrite(e: &mut IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            let mut changed = false;
            for a in args.iter_mut() {
                changed |= expr_rewrite(a, name);
            }
            if !name.is_empty()
                && (func == "sortedIntJoin" || func == "sortedBigintJoin")
                && matches!(args.as_slice(), [IrExpr::Str(n, _)] if n == name)
            {
                *func = "sortedJoinMerge".to_string();
                args.push(IrExpr::Str(
                    format!("{name}__big"),
                    crate::ir::StrStyle::DoubleQuoted,
                ));
                changed = true;
            }
            changed
        }
        IrExpr::BinOp { lhs, rhs, .. } => expr_rewrite(lhs, name) | expr_rewrite(rhs, name),
        IrExpr::Index { key, .. } => expr_rewrite(key, name),
        IrExpr::MethodCall { obj, args, .. } => {
            let mut changed = expr_rewrite(obj, name);
            for a in args.iter_mut() {
                changed |= expr_rewrite(a, name);
            }
            changed
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_rewrite(cond, name) | expr_rewrite(then, name) | expr_rewrite(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_rewrite(expr, name) | expr_rewrite(default, name)
        }
        IrExpr::Interpolate(parts) => {
            let mut changed = false;
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(e) = p {
                    changed |= expr_rewrite(e, name);
                }
            }
            changed
        }
        IrExpr::Capture { expr, .. } => expr_rewrite(expr, name),
        IrExpr::Array(elems) => {
            let mut changed = false;
            for el in elems.iter_mut() {
                changed |= expr_rewrite(el, name);
            }
            changed
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            let mut changed = expr_rewrite(iter, name) | expr_rewrite(elem, name);
            if let Some(c) = cond {
                changed |= expr_rewrite(c, name);
            }
            changed
        }
        IrExpr::Arrow(stmts) => {
            let mut changed = false;
            for s in stmts.iter_mut() {
                changed |= stmt_rewrite(s, name);
            }
            changed
        }
        IrExpr::Arith(a) => arith_rewrite(a, name),
        IrExpr::Splice(inner) => expr_rewrite(inner, name),
        IrExpr::Lambda { body, .. } => {
            let mut changed = false;
            for s in body.iter_mut() {
                changed |= stmt_rewrite(s, name);
            }
            changed
        }
        _ => false,
    }
}

/// Rewrite a `sorted*Join` call inside an arith tree (rare, but walk it).
fn arith_rewrite(a: &mut ArithAst, name: &str) -> bool {
    match a {
        ArithAst::Bin { lhs, rhs, .. } => arith_rewrite(lhs, name) | arith_rewrite(rhs, name),
        ArithAst::Un { arg, .. } => arith_rewrite(arg, name),
        ArithAst::Cond { test, then, else_ } => {
            arith_rewrite(test, name) | arith_rewrite(then, name) | arith_rewrite(else_, name)
        }
        ArithAst::Assign { rhs, .. } => arith_rewrite(rhs, name),
        ArithAst::Index { key, .. } => arith_rewrite(key, name),
        ArithAst::Cast { arg, .. } => arith_rewrite(arg, name),
        _ => false,
    }
}

/// Rewrite `sorted*Join(name)` in all expressions of a statement,
/// recursing into nested statements.
fn stmt_rewrite(s: &mut IrStmt, name: &str) -> bool {
    match s {
        IrStmt::Expr(e) => expr_rewrite(e, name),
        IrStmt::Output { value, .. } => expr_rewrite(value, name),
        IrStmt::Return(Some(e)) => expr_rewrite(e, name),
        IrStmt::Exit(Some(e)) => expr_rewrite(e, name),
        IrStmt::Die { expr, .. } => expr_rewrite(expr, name),
        IrStmt::Warn { expr, .. } => expr_rewrite(expr, name),
        IrStmt::SetChildError(e) => expr_rewrite(e, name),
        IrStmt::WriteFile { path, content, .. } => {
            expr_rewrite(path, name) | expr_rewrite(content, name)
        }
        IrStmt::Assign { expr, .. } => expr_rewrite(expr, name),
        IrStmt::If { cond, then, elsifs, else_ } => {
            let mut changed = expr_rewrite(cond, name);
            for b in then.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            for b in else_.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            for (c, b) in elsifs.iter_mut() {
                changed |= expr_rewrite(c, name);
                for s in b.iter_mut() {
                    changed |= stmt_rewrite(s, name);
                }
            }
            changed
        }
        IrStmt::For { iter, body, .. } => {
            let mut changed = expr_rewrite(iter, name);
            for b in body.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            changed
        }
        IrStmt::While { cond, body } => {
            let mut changed = expr_rewrite(cond, name);
            for b in body.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            changed
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut changed = expr_rewrite(cond, name);
            for b in body.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            changed
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Function { body, .. } => {
            let mut changed = false;
            for b in body.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            changed
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                for b in stage.iter_mut() {
                    changed |= stmt_rewrite(b, name);
                }
            }
            changed
        }
        IrStmt::Redirect { inner, .. } => {
            let mut changed = false;
            for b in inner.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            changed
        }
        IrStmt::Case { clauses, .. } => {
            let mut changed = false;
            for c in clauses.iter_mut() {
                for b in c.body.iter_mut() {
                    changed |= stmt_rewrite(b, name);
                }
            }
            changed
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut changed = false;
            for b in body.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            for b in else_body.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            for b in finally_body.iter_mut() {
                changed |= stmt_rewrite(b, name);
            }
            for ex in excepts.iter_mut() {
                for b in ex.body.iter_mut() {
                    changed |= stmt_rewrite(b, name);
                }
            }
            changed
        }
        _ => false,
    }
}

/// Identify set variables in a scope and split them. Returns whether
/// anything changed.
fn split_sets_in_scope(stmts: &mut Vec<IrStmt>) -> bool {
    // Candidate set variables: those with an empty setArray init.
    let mut candidates: Vec<String> = Vec::new();
    for s in stmts.iter() {
        if let Some(name) = set_init_name(s) {
            candidates.push(name);
        }
    }
    let mut changed = false;
    for name in candidates {
        // The big-partition name must be free.
        let big = format!("{name}__big");
        if scope_uses_name(stmts, &big) {
            continue;
        }
        // All refs to `name` must be whitelisted (init/append/join).
        if !scope_refs_ok(stmts, &name) {
            continue;
        }
        changed |= rewrite_scope(stmts, &name, &big);
    }
    changed
}

/// If `s` is `Assign { var: name, expr: Call("setArray", [Str(name),
/// Array([])]) }` (an EMPTY set init), return the name.
fn set_init_name(s: &IrStmt) -> Option<String> {
    if let IrStmt::Assign { targets, expr, .. } = s {
        if let [t] = targets.as_slice() {
            if let IrExpr::Call { func, args } = expr {
                if func == "setArray"
                    && matches!(args.as_slice(), [IrExpr::Str(n, _), IrExpr::Array(elems)]
                        if n == &t.var && elems.is_empty())
                {
                    return Some(t.var.clone());
                }
            }
        }
    }
    None
}

/// Does any expression in the scope reference `name`? (Used to check the
/// big-partition name is free.)
fn scope_uses_name(stmts: &[IrStmt], name: &str) -> bool {
    stmts.iter().any(|s| stmt_uses_name(s, name))
}

fn stmt_uses_name(s: &IrStmt, name: &str) -> bool {
    match s {
        IrStmt::Assign { targets, expr, .. } => {
            targets.iter().any(|t| t.var == name) || expr_uses_name(expr, name)
        }
        IrStmt::Expr(e) => expr_uses_name(e, name),
        IrStmt::Output { value, .. } => expr_uses_name(value, name),
        IrStmt::If { cond, then, elsifs, else_ } => {
            expr_uses_name(cond, name)
                || then.iter().any(|s| stmt_uses_name(s, name))
                || else_.iter().any(|s| stmt_uses_name(s, name))
                || elsifs
                    .iter()
                    .any(|(c, b)| expr_uses_name(c, name) || b.iter().any(|s| stmt_uses_name(s, name)))
        }
        IrStmt::For { iter, body, .. } => {
            expr_uses_name(iter, name) || body.iter().any(|s| stmt_uses_name(s, name))
        }
        IrStmt::While { cond, body } => {
            expr_uses_name(cond, name) || body.iter().any(|s| stmt_uses_name(s, name))
        }
        IrStmt::DoWhile { body, cond, .. } => {
            body.iter().any(|s| stmt_uses_name(s, name)) || expr_uses_name(cond, name)
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Function { body, .. } => body.iter().any(|s| stmt_uses_name(s, name)),
        IrStmt::Pipeline { stages, .. } => {
            stages.iter().any(|st| st.iter().any(|s| stmt_uses_name(s, name)))
        }
        IrStmt::Redirect { inner, .. } => inner.iter().any(|s| stmt_uses_name(s, name)),
        IrStmt::Case { clauses, .. } => {
            clauses.iter().any(|c| c.body.iter().any(|s| stmt_uses_name(s, name)))
        }
        IrStmt::Return(Some(e)) => expr_uses_name(e, name),
        IrStmt::Exit(Some(e)) => expr_uses_name(e, name),
        IrStmt::Die { expr, .. } => expr_uses_name(expr, name),
        IrStmt::Warn { expr, .. } => expr_uses_name(expr, name),
        IrStmt::SetChildError(e) => expr_uses_name(e, name),
        IrStmt::WriteFile { path, content, .. } => {
            expr_uses_name(path, name) || expr_uses_name(content, name)
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            var == name || elements.iter().any(|e| expr_uses_name(e, name))
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            body.iter().any(|s| stmt_uses_name(s, name))
                || else_body.iter().any(|s| stmt_uses_name(s, name))
                || finally_body.iter().any(|s| stmt_uses_name(s, name))
                || excepts.iter().any(|ex| ex.body.iter().any(|s| stmt_uses_name(s, name)))
        }
        _ => false,
    }
}

fn expr_uses_name(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Var(n, _) => n == name,
        IrExpr::Str(n, _) => n == name,
        IrExpr::Call { args, .. } => args.iter().any(|a| expr_uses_name(a, name)),
        IrExpr::MethodCall { obj, args, .. } => {
            expr_uses_name(obj, name) || args.iter().any(|a| expr_uses_name(a, name))
        }
        IrExpr::BinOp { lhs, rhs, .. } => expr_uses_name(lhs, name) || expr_uses_name(rhs, name),
        IrExpr::Index { var, key } => var == name || expr_uses_name(key, name),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_uses_name(cond, name) || expr_uses_name(then, name) || expr_uses_name(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_uses_name(expr, name) || expr_uses_name(default, name)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Expr(e) => expr_uses_name(e, name),
            _ => false,
        }),
        IrExpr::Capture { expr, .. } => expr_uses_name(expr, name),
        IrExpr::Array(elems) => elems.iter().any(|e| expr_uses_name(e, name)),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_uses_name(iter, name)
                || expr_uses_name(elem, name)
                || cond.as_ref().map(|c| expr_uses_name(c, name)).unwrap_or(false)
        }
        IrExpr::Arrow(stmts) => stmts.iter().any(|s| stmt_uses_name(s, name)),
        IrExpr::Arith(a) => arith_uses_name(a, name),
        IrExpr::Splice(inner) => expr_uses_name(inner, name),
        IrExpr::Lambda { body, .. } => body.iter().any(|s| stmt_uses_name(s, name)),
        _ => false,
    }
}

fn arith_uses_name(a: &ArithAst, name: &str) -> bool {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) => n == name,
        ArithAst::Index { var, key } => var == name || arith_uses_name(key, name),
        ArithAst::Bin { lhs, rhs, .. } => arith_uses_name(lhs, name) || arith_uses_name(rhs, name),
        ArithAst::Un { arg, .. } => arith_uses_name(arg, name),
        ArithAst::Cond { test, then, else_ } => {
            arith_uses_name(test, name) || arith_uses_name(then, name) || arith_uses_name(else_, name)
        }
        ArithAst::Assign { var, rhs, .. } => var == name || arith_uses_name(rhs, name),
        ArithAst::IncDec { var, .. } => var == name,
        ArithAst::Cast { arg, .. } => arith_uses_name(arg, name),
        _ => false,
    }
}

/// Are ALL references to `name` in the scope whitelisted (empty setArray
/// init, setArrayAppend append, sortedIntJoin/sortedBigintJoin consumer)?
fn scope_refs_ok(stmts: &[IrStmt], name: &str) -> bool {
    stmts.iter().all(|s| stmt_refs_ok(s, name))
}

fn stmt_refs_ok(s: &IrStmt, name: &str) -> bool {
    match s {
        IrStmt::Assign { targets, expr, .. } => {
            let writes_name = targets.iter().any(|t| t.var == name);
            if writes_name {
                // A write to `name` is whitelisted only as setArray (empty
                // init) or setArrayAppend. The element args must not
                // reference `name` (recursion).
                match expr {
                    IrExpr::Call { func, args } if func == "setArray" || func == "setArrayAppend" => {
                        matches!(args.first(), Some(IrExpr::Str(n, _)) if n == name)
                            && args.iter().skip(1).all(|a| !expr_has_other_ref(a, name))
                    }
                    _ => false,
                }
            } else {
                !expr_has_other_ref(expr, name)
            }
        }
        IrStmt::Expr(e) => !expr_has_other_ref(e, name),
        IrStmt::Output { value, .. } => !expr_has_other_ref(value, name),
        IrStmt::If { cond, then, elsifs, else_ } => {
            !expr_has_other_ref(cond, name)
                && then.iter().all(|s| stmt_refs_ok(s, name))
                && else_.iter().all(|s| stmt_refs_ok(s, name))
                && elsifs.iter().all(|(c, b)| {
                    !expr_has_other_ref(c, name) && b.iter().all(|s| stmt_refs_ok(s, name))
                })
        }
        IrStmt::For { iter, body, .. } => {
            !expr_has_other_ref(iter, name) && body.iter().all(|s| stmt_refs_ok(s, name))
        }
        IrStmt::While { cond, body } => {
            !expr_has_other_ref(cond, name) && body.iter().all(|s| stmt_refs_ok(s, name))
        }
        IrStmt::DoWhile { body, cond, .. } => {
            body.iter().all(|s| stmt_refs_ok(s, name)) && !expr_has_other_ref(cond, name)
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Function { body, .. } => body.iter().all(|s| stmt_refs_ok(s, name)),
        IrStmt::Pipeline { stages, .. } => {
            stages.iter().all(|st| st.iter().all(|s| stmt_refs_ok(s, name)))
        }
        IrStmt::Redirect { inner, .. } => inner.iter().all(|s| stmt_refs_ok(s, name)),
        IrStmt::Case { clauses, .. } => {
            clauses.iter().all(|c| c.body.iter().all(|s| stmt_refs_ok(s, name)))
        }
        IrStmt::Return(Some(e)) => !expr_has_other_ref(e, name),
        IrStmt::Exit(Some(e)) => !expr_has_other_ref(e, name),
        IrStmt::Die { expr, .. } => !expr_has_other_ref(expr, name),
        IrStmt::Warn { expr, .. } => !expr_has_other_ref(expr, name),
        IrStmt::SetChildError(e) => !expr_has_other_ref(e, name),
        IrStmt::WriteFile { path, content, .. } => {
            !expr_has_other_ref(path, name) && !expr_has_other_ref(content, name)
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            var != name && elements.iter().all(|e| !expr_has_other_ref(e, name))
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            body.iter().all(|s| stmt_refs_ok(s, name))
                && else_body.iter().all(|s| stmt_refs_ok(s, name))
                && finally_body.iter().all(|s| stmt_refs_ok(s, name))
                && excepts.iter().all(|ex| ex.body.iter().all(|s| stmt_refs_ok(s, name)))
        }
        _ => true,
    }
}

/// Does the expression contain a reference to `name` that is NOT a
/// whitelisted `sortedIntJoin`/`sortedBigintJoin` consumer?
fn expr_has_other_ref(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            // Whitelisted consumer: sortedIntJoin/sortedBigintJoin(name).
            if (func == "sortedIntJoin" || func == "sortedBigintJoin")
                && matches!(args.as_slice(), [IrExpr::Str(n, _)] if n == name)
            {
                return false;
            }
            // setArray/setArrayAppend(name, ...) in expression position:
            // a write outside an Assign — refuse.
            if (func == "setArray" || func == "setArrayAppend")
                && matches!(args.first(), Some(IrExpr::Str(n, _)) if n == name)
            {
                return true;
            }
            args.iter().any(|a| expr_has_other_ref(a, name))
        }
        IrExpr::Var(n, _) => n == name,
        IrExpr::Str(n, _) => n == name,
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_has_other_ref(lhs, name) || expr_has_other_ref(rhs, name)
        }
        IrExpr::Index { var, key } => var == name || expr_has_other_ref(key, name),
        IrExpr::MethodCall { obj, args, .. } => {
            expr_has_other_ref(obj, name) || args.iter().any(|a| expr_has_other_ref(a, name))
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_has_other_ref(cond, name)
                || expr_has_other_ref(then, name)
                || expr_has_other_ref(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_has_other_ref(expr, name) || expr_has_other_ref(default, name)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Expr(e) => expr_has_other_ref(e, name),
            _ => false,
        }),
        IrExpr::Capture { expr, .. } => expr_has_other_ref(expr, name),
        IrExpr::Array(elems) => elems.iter().any(|e| expr_has_other_ref(e, name)),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_has_other_ref(iter, name)
                || expr_has_other_ref(elem, name)
                || cond.as_ref().map(|c| expr_has_other_ref(c, name)).unwrap_or(false)
        }
        IrExpr::Arrow(stmts) => !stmts.iter().all(|s| stmt_refs_ok(s, name)),
        IrExpr::Arith(a) => arith_has_other_ref(a, name),
        IrExpr::Splice(inner) => expr_has_other_ref(inner, name),
        IrExpr::Lambda { body, .. } => !body.iter().all(|s| stmt_refs_ok(s, name)),
        _ => false,
    }
}

fn arith_has_other_ref(a: &ArithAst, name: &str) -> bool {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) => n == name,
        ArithAst::Index { var, key } => var == name || arith_has_other_ref(key, name),
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_has_other_ref(lhs, name) || arith_has_other_ref(rhs, name)
        }
        ArithAst::Un { arg, .. } => arith_has_other_ref(arg, name),
        ArithAst::Cond { test, then, else_ } => {
            arith_has_other_ref(test, name)
                || arith_has_other_ref(then, name)
                || arith_has_other_ref(else_, name)
        }
        ArithAst::Assign { var, rhs, .. } => var == name || arith_has_other_ref(rhs, name),
        ArithAst::IncDec { var, .. } => var == name,
        ArithAst::Cast { arg, .. } => arith_has_other_ref(arg, name),
        _ => false,
    }
}

/// Rewrite the scope for a verified set `name`: add the big-partition
/// init, route appends, and merge the joins.
fn rewrite_scope(stmts: &mut Vec<IrStmt>, name: &str, big: &str) -> bool {
    let mut changed = false;
    let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len() + 1);
    for s in stmts.drain(..) {
        match s {
            IrStmt::Assign { targets, expr, asm } => {
                let writes_name = targets.iter().any(|t| t.var == name);
                if writes_name {
                    match &expr {
                        IrExpr::Call { func, .. } if func == "setArray" => {
                            // Empty init: keep it, then add the big init.
                            out.push(IrStmt::Assign { targets, expr, asm });
                            out.push(IrStmt::Assign {
                                targets: vec![AssignTarget {
                                    var: big.to_string(),
                                    sigil: None,
                                    indices: vec![],
                                }],
                                expr: IrExpr::Call {
                                    func: "setArray".to_string(),
                                    args: vec![
                                        IrExpr::Str(big.to_string(), crate::ir::StrStyle::DoubleQuoted),
                                        IrExpr::Array(vec![]),
                                    ],
                                },
                                asm: None,
                            });
                            changed = true;
                        }
                        IrExpr::Call { func, args } if func == "setArrayAppend" => {
                            // Route the append: if isSmallInt(v) → s, else → big.
                            let v = match args.get(1) {
                                Some(IrExpr::Array(elems)) if elems.len() == 1 => elems[0].clone(),
                                _ => {
                                    // unexpected shape — keep as-is
                                    out.push(IrStmt::Assign { targets, expr, asm });
                                    continue;
                                }
                            };
                            let small_assign = IrStmt::Assign {
                                targets: targets.clone(),
                                expr: IrExpr::Call {
                                    func: "setArrayAppend".to_string(),
                                    args: vec![
                                        IrExpr::Str(name.to_string(), crate::ir::StrStyle::DoubleQuoted),
                                        IrExpr::Array(vec![v.clone()]),
                                    ],
                                },
                                asm: None,
                            };
                            let big_assign = IrStmt::Assign {
                                targets: vec![AssignTarget {
                                    var: big.to_string(),
                                    sigil: None,
                                    indices: vec![],
                                }],
                                expr: IrExpr::Call {
                                    func: "setArrayAppend".to_string(),
                                    args: vec![
                                        IrExpr::Str(big.to_string(), crate::ir::StrStyle::DoubleQuoted),
                                        IrExpr::Array(vec![v.clone()]),
                                    ],
                                },
                                asm: None,
                            };
                            out.push(IrStmt::If {
                                cond: IrExpr::Call {
                                    func: "isSmallInt".to_string(),
                                    args: vec![v],
                                },
                                then: vec![small_assign],
                                elsifs: vec![],
                                else_: vec![big_assign],
                            });
                            changed = true;
                        }
                        _ => {
                            // A write to `name` that isn't setArray/setArrayAppend
                            // — should have been refused by scan; keep as-is.
                            out.push(IrStmt::Assign { targets, expr, asm });
                        }
                    }
                } else {
                    // Not a write to `name`; rewrite any join for `name`.
                    let mut expr = expr;
                    let c = expr_rewrite(&mut expr, name);
                    out.push(IrStmt::Assign { targets, expr, asm });
                    changed |= c;
                }
            }
            other => {
                // Rewrite joins for `name` in nested expressions.
                let mut o = other;
                changed |= stmt_rewrite(&mut o, name);
                out.push(o);
            }
        }
    }
    *stmts = out;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;
    use std::sync::{Mutex, OnceLock};

    // The split-set tests all mutate the shared `SPLIT_SETS_OVERRIDE`
    // static (`run` arms it ON, `disabled_by_default` asserts the OFF
    // default). Cargo runs tests in parallel threads, so without a lock a
    // sibling `set_split_sets(Some(true))` can land between
    // `disabled_by_default`'s disable and its `transform` call — making the
    // "refuses by default" assertion spuriously fail. Serialize them.
    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn str_lit(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn set_init(name: &str) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: name.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Call {
                func: "setArray".to_string(),
                args: vec![str_lit(name), IrExpr::Array(vec![])],
            },
            asm: None,
        }
    }

    fn set_append(name: &str, v: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: name.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Call {
                func: "setArrayAppend".to_string(),
                args: vec![str_lit(name), IrExpr::Array(vec![v])],
            },
            asm: None,
        }
    }

    fn sorted_join(name: &str, bigint: bool) -> IrExpr {
        IrExpr::Call {
            func: if bigint {
                "sortedBigintJoin".to_string()
            } else {
                "sortedIntJoin".to_string()
            },
            args: vec![str_lit(name)],
        }
    }

    fn run(stmts: &mut Vec<IrStmt>) -> bool {
        crate::shir::set_split_sets(Some(true));
        let r = transform(stmts);
        crate::shir::set_split_sets(None);
        r
    }

    #[test]
    fn splits_set_into_two_partitions() {
        let _g = lock();
        let mut stmts = vec![
            set_init("s"),
            set_append("s", str_lit("5")),
            set_append("s", str_lit("18446744073709551617")),
            IrStmt::Return(Some(sorted_join("s", false))),
        ];
        assert!(run(&mut stmts));

        // 1. the big-partition init follows the small init
        assert!(matches!(
            &stmts[1],
            IrStmt::Assign {
                expr: IrExpr::Call { func, args },
                ..
            } if func == "setArray"
                && matches!(args.as_slice(), [IrExpr::Str(n, _), IrExpr::Array(e)] if n == "s__big" && e.is_empty())
        ));

        // 2. each append is routed by isSmallInt
        for st in &stmts[2..4] {
            match st {
                IrStmt::If { cond, then, else_, .. } => {
                    assert!(matches!(
                        cond,
                        IrExpr::Call { func, .. } if func == "isSmallInt"
                    ));
                    assert_eq!(then.len(), 1);
                    assert_eq!(else_.len(), 1);
                }
                _ => panic!("expected routed If, got {st:?}"),
            }
        }

        // 3. the join is a merge of both partitions
        match &stmts[4] {
            IrStmt::Return(Some(IrExpr::Call { func, args })) => {
                assert_eq!(func, "sortedJoinMerge");
                assert!(matches!(
                    args.as_slice(),
                    [IrExpr::Str(a, _), IrExpr::Str(b, _)] if a == "s" && b == "s__big"
                ));
            }
            _ => panic!("expected merged join"),
        }
    }

    #[test]
    fn refuses_iterated_set() {
        let _g = lock();
        // `for x in s` (param slice) is not whitelisted → no split.
        let mut stmts = vec![
            set_init("s"),
            set_append("s", str_lit("5")),
            IrStmt::For {
                var: "x".to_string(),
                iter: IrExpr::Array(vec![IrExpr::Call {
                    func: "param".to_string(),
                    args: vec![
                        str_lit("slice"),
                        str_lit("s"),
                        str_lit("@"),
                        str_lit(""),
                    ],
                }]),
                body: vec![],
            },
        ];
        assert!(!run(&mut stmts));
        // unchanged: no s__big init, no isSmallInt routing
        assert!(!stmts.iter().any(|s| matches!(
            s,
            IrStmt::Assign {
                expr: IrExpr::Call { func, .. },
                ..
            } if func == "setArray"
        ) && matches!(s, IrStmt::Assign { expr: IrExpr::Call { args, .. }, .. }
            if matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "s__big"))));
    }

    #[test]
    fn refuses_nonempty_init() {
        let _g = lock();
        // A non-empty setArray init is not the empty-set pattern → refuse.
        let mut stmts = vec![IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "s".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Call {
                func: "setArray".to_string(),
                args: vec![str_lit("s"), IrExpr::Array(vec![str_lit("1")])],
            },
            asm: None,
        }];
        assert!(!run(&mut stmts));
    }

    #[test]
    fn disabled_by_default() {
        let _g = lock();
        crate::shir::set_split_sets(Some(false));
        let mut stmts = vec![set_init("s"), set_append("s", str_lit("5"))];
        assert!(!transform(&mut stmts));
        crate::shir::set_split_sets(None);
    }
}

