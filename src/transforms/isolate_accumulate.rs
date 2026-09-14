//! isolate-accumulate — inline single-use loop-private temporaries into
//! a self-referential tail, isolating a single accumulate statement.
//!
//! Pattern (one loop body, straight-line scalar Assigns):
//! ```text
//! for (...); do t1 = F1(..); t2 = F2(..); acc = G(acc, t1, t2); done
//! ```
//! becomes (when every temp is defined once, read once later, pure):
//! ```text
//! for (...); do acc = G(acc, F1(..), F2(..)); done
//! ```
//! Every backend whose loop candidacy matches single-accumulate bodies
//! (CUDA reduce/seq, C versioning, Vulkan) benefits from the same
//! rewrite; the Perl/JS outputs compute identical scalars.
//!
//! REFUSE > GUESS — vetoes (never misfires):
//! - body isn't straight-line scalar Assigns (any control flow, IO,
//!   multi-targets veto the whole body);
//! - a temp is redefined, read before its def (loop-carried), read
//!   0 or 2+ times later, or its RHS isn't pure (calls/captures,
//!   Index, Assign/IncDec nodes, `$?`);
//! - any body stmt writes the counter/bound (trip/order stability is
//!   then unprovable — pathological shapes miss);
//! - the tail isn't self-referential (no accumulate to isolate) or the
//!   result isn't a single stmt (partial progress is discarded —
//!   all-or-nothing, never half-inlined churn).
//!
//! Soundness notes: headers/control flow untouched (trips identical);
//! inlining moves pure evaluations within one iteration (same values:
//! counter/bound never written, invariants stable, temps
//! dominance-ordered); single-use means evaluation counts never change;
//! loop exit status unchanged (Assign tail in both, 0-trip identical).

use crate::ir::{ArithAst, IrExpr, IrStmt};
use std::collections::BTreeSet;

/// Apply to every loop body in the statement list (top-level and
/// nested). Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= recurse(st);
    }
    changed
}

fn recurse(st: &mut IrStmt) -> bool {
    let mut changed = false;
    match st {
        IrStmt::ForInit { body, .. } => {
            changed |= isolate_body(body);
            for s in body.iter_mut() {
                changed |= recurse(s);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            changed |= isolate_body(body);
            for s in body.iter_mut() {
                changed |= recurse(s);
            }
        }
        IrStmt::For { body, .. } => {
            changed |= isolate_body(body);
            for s in body.iter_mut() {
                changed |= recurse(s);
            }
        }
        IrStmt::If { then, elsifs, else_, .. } => {
            for s in then.iter_mut() {
                changed |= recurse(s);
            }
            for (_, b) in elsifs.iter_mut() {
                for s in b.iter_mut() {
                    changed |= recurse(s);
                }
            }
            for s in else_.iter_mut() {
                changed |= recurse(s);
            }
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            for s in b.iter_mut() {
                changed |= recurse(s);
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body.iter_mut() {
                changed |= recurse(s);
            }
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                for s in c.body.iter_mut() {
                    changed |= recurse(s);
                }
            }
        }
        _ => {}
    }
    changed
}

/// Try to isolate a single accumulate in one loop body. Works on a
/// clone — commits only on full isolation (never partial churn).
///
/// Single static assignment: every body target defined at most once
/// (redefinition = loop-carried state, veto whole body).
fn isolate_body(body: &mut Vec<IrStmt>) -> bool {
    if body.len() < 2 {
        return false;
    }
    // All stmts must be scalar single-target Assigns (no control flow).
    for s in body.iter() {
        let IrStmt::Assign { targets, .. } = s else {
            return false;
        };
        let [t] = targets.as_slice() else {
            return false;
        };
        if !t.indices.is_empty() || t.var.contains('[') {
            return false;
        }
    }
    // Single static assignment across the body (redefinition means
    // loop-carried state — not isolateable).
    {
        let mut seen = BTreeSet::new();
        for x in body.iter() {
            let IrStmt::Assign { targets, .. } = x else {
                return false;
            };
            for t in targets {
                if !seen.insert(t.var.clone()) {
                    return false;
                }
            }
        }
    }
    let mut work: Vec<IrStmt> = body.clone();
    // Forward pass: inline each single-use pure temp into its unique
    // later use (defs vanish as absorbed).
    let mut i = 0;
    while i < work.len() {
        if !try_inline_temp(&mut work, i) {
            // Not inlineable (tail? multi-use? carried?) — advance.
            // (Refusals leave work untouched; everything is discarded
            // below unless isolation completes, so no partial churn
            // escapes.)
            i += 1;
        }
        // (On success the def at i is deleted; the next stmt shifts
        // into i — re-examine the same index.)
    }
    // Commit only on single self-referential stmt.
    if work.len() == 1 {
        if let IrStmt::Assign { targets, expr, .. } = &work[0] {
            if let [t] = targets.as_slice() {
                if expr_reads_var(expr, &t.var) {
                    *body = work;
                    return true;
                }
            }
        }
    }
    false
}

/// Try to inline (or drop, if dead) the temp defined at `work[i]`.
/// Returns true when the def was absorbed (caller re-examines index i).
/// Refusals leave `work` untouched.
fn try_inline_temp(work: &mut Vec<IrStmt>, i: usize) -> bool {
    let IrStmt::Assign { targets, expr, .. } = work[i].clone() else {
        return false;
    };
    let [t] = targets.as_slice() else {
        return false;
    };
    let name = t.var.clone();
    // Tail position never inlines (it's the accumulate keeper).
    if i + 1 == work.len() {
        return false;
    }
    // Temp RHS must be pure and movable: Arith without Index/
    // Assign/IncDec/`?`, or a literal. (Calls/captures can't appear in
    // ArithAst; non-Arith RHS must be literal.)
    if !movable_rhs(&expr) {
        return false;
    }
    // Ordered temp RHS: free vars must be never-written in the body
    // or uniquely defined BEFORE this def (same value at def and at
    // the later use — single static assignment makes it stable).
    {
        let mut def_before: BTreeSet<String> = BTreeSet::new();
        for x in work[..i].iter() {
            if let IrStmt::Assign { targets, .. } = x {
                for t in targets {
                    def_before.insert(t.var.clone());
                }
            }
        }
        let mut written: BTreeSet<String> = BTreeSet::new();
        for x in work.iter() {
            if let IrStmt::Assign { targets, .. } = x {
                for t in targets {
                    written.insert(t.var.clone());
                }
            }
        }
        let mut free = BTreeSet::new();
        expr_free_vars(&expr, &mut free);
        if free.iter().any(|v| written.contains(v) && !def_before.contains(v)) {
            return false;
        }
    }
    // Single ordered use: exactly one later read, no redefinition, no
    // earlier read (earlier read = loop-carried value, not this def).
    let mut uses = 0usize;
    let mut use_idx = 0usize;
    for (j, s) in work.iter().enumerate() {
        let IrStmt::Assign { targets: tj, expr: ej, .. } = s else {
            continue;
        };
        if j < i {
            if expr_reads_var(ej, &name) {
                return false;
            }
            continue;
        }
        if j == i {
            continue;
        }
        if tj.iter().any(|x| x.var == name) {
            return false;
        }
        if expr_reads_var(ej, &name) {
            uses += 1;
            use_idx = j;
        }
    }
    if uses == 0 {
        // Dead temp (pure def, never read) — drop the def.
        work.remove(i);
        return true;
    }
    if uses != 1 {
        return false;
    }
    // Substitute into the unique use.
    let IrStmt::Assign { expr: uexpr, .. } = work[use_idx].clone() else {
        return false;
    };
    let Some(new_expr) = subst_var(&uexpr, &name, &expr) else {
        return false;
    };
    if let IrStmt::Assign { expr: e, .. } = &mut work[use_idx] {
        *e = new_expr;
    }
    work.remove(i);
    true
}

/// A temp RHS is movable iff pure: Arith without Index/Assign/IncDec
/// and without `$?`, or a side-effect-free literal. (Str literals are
/// inert text; Int/Bool likewise.)
fn movable_rhs(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Int(_) | IrExpr::Bool(_) => true,
        IrExpr::Str(_, _) => true,
        IrExpr::Arith(a) => arith_movable(a),
        _ => false,
    }
}

fn arith_movable(a: &ArithAst) -> bool {
    match a {
        ArithAst::Index { .. } | ArithAst::Assign { .. } | ArithAst::IncDec { .. } => false,
        ArithAst::Var(v) | ArithAst::Ident(v) => v != "?",
        ArithAst::Bin { lhs, rhs, .. } => arith_movable(lhs) && arith_movable(rhs),
        ArithAst::Un { arg, .. } => arith_movable(arg),
        ArithAst::Cond { test, then, else_ } => {
            arith_movable(test) && arith_movable(then) && arith_movable(else_)
        }
        ArithAst::Cast { arg, .. } => arith_movable(arg),
        _ => true,
    }
}

/// Substitute `rhs` for `Var/Ident(name)` reads in `expr` (both Arith
/// and word-level positions). Names in nested statement bodies
/// (Arrow/Lambda) are NOT substituted (deferred execution observes a
/// different store) — any such mention vetoes (None).
fn subst_var(expr: &IrExpr, name: &str, rhs: &IrExpr) -> Option<IrExpr> {
    match expr {
        IrExpr::Var(v, _) | IrExpr::Ident(v) if v == name => Some(rhs.clone()),
        IrExpr::Arith(a) => Some(IrExpr::Arith(Box::new(subst_arith(a, name, rhs)?))),
        IrExpr::BinOp { lhs, op, rhs: r } => Some(IrExpr::BinOp {
            lhs: Box::new(subst_var(lhs, name, rhs)?),
            op: op.clone(),
            rhs: Box::new(subst_var(r, name, rhs)?),
        }),
        IrExpr::Ternary { cond, then, else_ } => Some(IrExpr::Ternary {
            cond: Box::new(subst_var(cond, name, rhs)?),
            then: Box::new(subst_var(then, name, rhs)?),
            else_: Box::new(subst_var(else_, name, rhs)?),
        }),
        IrExpr::DefinedOr { expr, default } => Some(IrExpr::DefinedOr {
            expr: Box::new(subst_var(expr, name, rhs)?),
            default: Box::new(subst_var(default, name, rhs)?),
        }),
        IrExpr::Interpolate(parts) => {
            let mut np = Vec::with_capacity(parts.len());
            for p in parts {
                match p {
                    crate::ir::InterpPart::Lit(s) => {
                        np.push(crate::ir::InterpPart::Lit(s.clone()))
                    }
                    crate::ir::InterpPart::Expr(x) => {
                        np.push(crate::ir::InterpPart::Expr(Box::new(subst_var(x, name, rhs)?)))
                    }
                }
            }
            Some(IrExpr::Interpolate(np))
        }
        IrExpr::Index { var, key } => {
            if var == name {
                return None;
            }
            Some(IrExpr::Index { var: var.clone(), key: Box::new(subst_var(key, name, rhs)?) })
        }
        IrExpr::Call { func, args } => {
            let mut na = Vec::with_capacity(args.len());
            for x in args {
                na.push(subst_var(x, name, rhs)?);
            }
            Some(IrExpr::Call { func: func.clone(), args: na })
        }
        IrExpr::MethodCall { obj, method, args } => {
            let mut na = Vec::with_capacity(args.len());
            for x in args {
                na.push(subst_var(x, name, rhs)?);
            }
            Some(IrExpr::MethodCall {
                obj: Box::new(subst_var(obj, name, rhs)?),
                method: method.clone(),
                args: na,
            })
        }
        IrExpr::Array(els) => {
            let mut ne = Vec::with_capacity(els.len());
            for x in els {
                ne.push(subst_var(x, name, rhs)?);
            }
            Some(IrExpr::Array(ne))
        }
        IrExpr::Capture { expr, native } => Some(IrExpr::Capture {
            expr: Box::new(subst_var(expr, name, rhs)?),
            native: *native,
        }),
        IrExpr::ArrayComp { var, iter, elem, cond } => Some(IrExpr::ArrayComp {
            var: var.clone(),
            iter: Box::new(subst_var(iter, name, rhs)?),
            elem: Box::new(subst_var(elem, name, rhs)?),
            cond: match cond {
                Some(c) => Some(Box::new(subst_var(c, name, rhs)?)),
                None => None,
            },
        }),
        IrExpr::Object(pairs) => {
            let mut no = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                no.push((k.clone(), subst_var(v, name, rhs)?));
            }
            Some(IrExpr::Object(no))
        }
        IrExpr::Splice(x) => Some(IrExpr::Splice(Box::new(subst_var(x, name, rhs)?))),
        // Deferred bodies observe a different store — mention vetoes.
        IrExpr::Lambda { .. } | IrExpr::Arrow(_) => {
            if lambda_mentions(expr, name) {
                None
            } else {
                Some(expr.clone())
            }
        }
        IrExpr::Ext(x) => {
            // Opaque children: substitute only when name-free.
            if ext_mentions(x, name) {
                None
            } else {
                Some(expr.clone())
            }
        }
        _ => Some(expr.clone()),
    }
}

fn subst_arith(a: &ArithAst, name: &str, rhs: &IrExpr) -> Option<ArithAst> {
    // The temp RHS is Arith or literal; substituting a NON-Arith rhs
    // into arithmetic position is only sound for literals (Num-shaped).
    let rhs_arith: Option<ArithAst> = match rhs {
        IrExpr::Arith(x) => Some((**x).clone()),
        IrExpr::Int(n) => Some(ArithAst::Num(*n)),
        IrExpr::Str(s, _) => s.trim().parse::<i64>().ok().map(ArithAst::Num),
        _ => None,
    };
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) if v == name => rhs_arith,
        ArithAst::Bin { op, lhs, rhs: r } => Some(ArithAst::Bin {
            op: op.clone(),
            lhs: Box::new(subst_arith(lhs, name, rhs)?),
            rhs: Box::new(subst_arith(r, name, rhs)?),
        }),
        ArithAst::Un { op, arg } => Some(ArithAst::Un {
            op: op.clone(),
            arg: Box::new(subst_arith(arg, name, rhs)?),
        }),
        ArithAst::Cond { test, then, else_ } => Some(ArithAst::Cond {
            test: Box::new(subst_arith(test, name, rhs)?),
            then: Box::new(subst_arith(then, name, rhs)?),
            else_: Box::new(subst_arith(else_, name, rhs)?),
        }),
        ArithAst::Assign { var, .. } if var == name => None,
        ArithAst::Assign { var, op, rhs: r } => Some(ArithAst::Assign {
            var: var.clone(),
            op: op.clone(),
            rhs: Box::new(subst_arith(r, name, rhs)?),
        }),
        ArithAst::IncDec { var, .. } if var == name => None,
        ArithAst::Cast { ty, arg } => Some(ArithAst::Cast {
            ty: ty.clone(),
            arg: Box::new(subst_arith(arg, name, rhs)?),
        }),
        ArithAst::Index { var, .. } if var == name => None,
        ArithAst::Index { var, key } => Some(ArithAst::Index {
            var: var.clone(),
            key: Box::new(subst_arith(key, name, rhs)?),
        }),
        _ => Some(a.clone()),
    }
}

/// Every Var/Ident name read in an expr (over-approx for the veto
/// direction; targets-as-reads included).
fn expr_free_vars(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            out.insert(v.clone());
        }
        IrExpr::Arith(a) => arith_free_vars(a, out),
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_free_vars(lhs, out);
            expr_free_vars(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_free_vars(cond, out);
            expr_free_vars(then, out);
            expr_free_vars(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_free_vars(expr, out);
            expr_free_vars(default, out);
        }
        IrExpr::Interpolate(parts) => {
            for q in parts {
                if let crate::ir::InterpPart::Expr(x) = q {
                    expr_free_vars(x, out);
                }
            }
        }
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            expr_free_vars(key, out);
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                expr_free_vars(a, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_free_vars(obj, out);
            for a in args {
                expr_free_vars(a, out);
            }
        }
        IrExpr::Array(els) => {
            for a in els {
                expr_free_vars(a, out);
            }
        }
        IrExpr::Capture { expr, .. } => expr_free_vars(expr, out),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_free_vars(iter, out);
            expr_free_vars(elem, out);
            if let Some(c) = cond {
                expr_free_vars(c, out);
            }
        }
        IrExpr::Lambda { body, .. } => {
            for x in body {
                stmt_free_vars(x, out);
            }
        }
        IrExpr::Arrow(b) => {
            for x in b {
                stmt_free_vars(x, out);
            }
        }
        IrExpr::Object(pairs) => {
            for (_, v) in pairs {
                expr_free_vars(v, out);
            }
        }
        IrExpr::Splice(x) => expr_free_vars(x, out),
        IrExpr::Ext(x) => {
            for c in x.children() {
                expr_free_vars(c, out);
            }
        }
        _ => {}
    }
}

fn stmt_free_vars(s: &IrStmt, out: &mut BTreeSet<String>) {
    match s {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                out.insert(t.var.clone());
            }
            expr_free_vars(expr, out);
        }
        _ => nest_free_vars(s, out),
    }
}

fn nest_free_vars(s: &IrStmt, out: &mut BTreeSet<String>) {
    // Over-approx: binds + all reachable names (veto direction safe).
    match s {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                out.insert(t.var.clone());
            }
            expr_free_vars(expr, out);
        }
        IrStmt::Declare { vars, init, .. } => {
            for d in vars {
                out.insert(d.name.clone());
            }
            if let Some(e) = init {
                expr_free_vars(e, out);
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            out.insert(var.clone());
            for e in elements {
                expr_free_vars(e, out);
            }
        }
        IrStmt::ForInit { init, cond, step, body, .. } => {
            for x in init.iter().chain(step.iter()).chain(body.iter()) {
                nest_free_vars(x, out);
            }
            expr_free_vars(cond, out);
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
            expr_free_vars(cond, out);
            for x in body {
                nest_free_vars(x, out);
            }
        }
        IrStmt::For { var, iter, body, .. } => {
            out.insert(var.clone());
            expr_free_vars(iter, out);
            for x in body {
                nest_free_vars(x, out);
            }
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_free_vars(cond, out);
            for x in then.iter().chain(else_.iter()) {
                nest_free_vars(x, out);
            }
            for (c, b) in elsifs {
                expr_free_vars(c, out);
                for x in b {
                    nest_free_vars(x, out);
                }
            }
        }
        IrStmt::Case { discriminant, clauses, .. } => {
            expr_free_vars(discriminant, out);
            for c in clauses {
                for x in &c.body {
                    nest_free_vars(x, out);
                }
            }
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            for x in b {
                nest_free_vars(x, out);
            }
        }
        IrStmt::Function { body, .. } => {
            for x in body {
                nest_free_vars(x, out);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for st in stages {
                for x in st {
                    nest_free_vars(x, out);
                }
            }
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            for x in inner {
                nest_free_vars(x, out);
            }
            for r in redirects {
                expr_free_vars(&r.target, out);
            }
        }
        IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => expr_free_vars(e, out),
        IrStmt::WriteFile { path, content, .. } => {
            expr_free_vars(path, out);
            expr_free_vars(content, out);
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            if let Some(x) = e {
                expr_free_vars(x, out);
            }
        }
        IrStmt::SetChildError(e) => expr_free_vars(e, out),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_free_vars(expr, out),
        IrStmt::Exec { cmd, args, redirects, capture, .. } => {
            expr_free_vars(cmd, out);
            for a in args.iter().chain(redirects.iter()) {
                expr_free_vars(a, out);
            }
            if let Some(c) = capture {
                out.insert(c.clone());
            }
        }
        IrStmt::Select { clauses, .. } => {
            for c in clauses {
                if let Some(t) = &c.target {
                    out.insert(t.clone());
                }
                if let Some(x) = &c.ch {
                    expr_free_vars(x, out);
                }
                if let Some(x) = &c.value {
                    expr_free_vars(x, out);
                }
                for x in &c.body {
                    nest_free_vars(x, out);
                }
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
            for x in body.iter().chain(else_body.iter()).chain(finally_body.iter()) {
                nest_free_vars(x, out);
            }
            for e in excepts {
                if let Some(m) = &e.match_expr {
                    expr_free_vars(m, out);
                }
                if let Some(n) = &e.as_name {
                    out.insert(n.clone());
                }
                for x in &e.body {
                    nest_free_vars(x, out);
                }
            }
        }
        IrStmt::Asm { outputs, inputs, .. } => {
            for (n, e) in outputs.iter().chain(inputs.iter()) {
                out.insert(n.clone());
                expr_free_vars(e, out);
            }
        }
        IrStmt::Shaderize { var, body, .. } => {
            out.insert(var.clone());
            for x in body {
                nest_free_vars(x, out);
            }
        }
        IrStmt::Ext(x) => {
            for c in x.children() {
                nest_free_vars(c, out);
            }
        }
        IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Require(_)
        | IrStmt::Label(_)
        | IrStmt::Goto(_) => {}
        IrStmt::RawText(_) => {}
    }
}

fn arith_free_vars(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => {
            out.insert(v.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_free_vars(lhs, out);
            arith_free_vars(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_free_vars(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            arith_free_vars(test, out);
            arith_free_vars(then, out);
            arith_free_vars(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.insert(var.clone());
            arith_free_vars(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Cast { arg, .. } => arith_free_vars(arg, out),
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            arith_free_vars(key, out);
        }
        _ => {}
    }
}

/// Any Var/Ident read of `name` in an expr (all positions, incl.
/// deferred bodies — for use-counting AND refusal).
fn expr_reads_var(e: &IrExpr, name: &str) -> bool {
    // (Flattened `t[` reads count — a use the substitutor vetoes, so
    // counting them can only block, never unsoundly allow.)
    let flat = |v: &String| v == name || v.starts_with(&format!("{name}["));
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => flat(v),
        IrExpr::Arith(a) => arith_reads_var(a, name),
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_reads_var(lhs, name) || expr_reads_var(rhs, name)
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads_var(cond, name) || expr_reads_var(then, name) || expr_reads_var(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_reads_var(expr, name) || expr_reads_var(default, name)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Lit(_) => false,
            crate::ir::InterpPart::Expr(x) => expr_reads_var(x, name),
        }),
        IrExpr::Index { var, key } => var == name || expr_reads_var(key, name),
        IrExpr::Call { args, .. } => args.iter().any(|a| expr_reads_var(a, name)),
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads_var(obj, name) || args.iter().any(|a| expr_reads_var(a, name))
        }
        IrExpr::Array(els) => els.iter().any(|a| expr_reads_var(a, name)),
        IrExpr::Capture { expr, .. } => expr_reads_var(expr, name),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_reads_var(iter, name)
                || expr_reads_var(elem, name)
                || cond.as_ref().is_some_and(|c| expr_reads_var(c, name))
        }
        IrExpr::Lambda { body, .. } => body.iter().any(|s| stmt_reads_var(s, name)),
        IrExpr::Arrow(b) => b.iter().any(|s| stmt_reads_var(s, name)),
        IrExpr::Object(pairs) => pairs.iter().any(|(_, v)| expr_reads_var(v, name)),
        IrExpr::Splice(x) => expr_reads_var(x, name),
        IrExpr::Ext(x) => x.children().iter().any(|c| expr_reads_var(c, name)),
        _ => false,
    }
}

/// Name mention anywhere in a stmt (exhaustive — the compiler forces
/// a decision on new IR variants; missing a mention is unsound, so any
/// residual arm assumes mention). Used for use-counting (over-counting
/// only blocks) and deferred-body refusal.
fn stmt_reads_var(s: &IrStmt, name: &str) -> bool {
    let flat = |v: &String| v == name || v.starts_with(&format!("{name}["));
    match s {
        IrStmt::Assign { targets, expr, .. } => {
            targets.iter().any(|t| flat(&t.var) || t.indices.iter().any(|k| expr_reads_var(k, name)))
                || expr_reads_var(expr, name)
        }
        IrStmt::Declare { vars, init, .. } => {
            vars.iter().any(|d| flat(&d.name))
                || init.as_ref().is_some_and(|e| expr_reads_var(e, name))
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            flat(var) || elements.iter().any(|e| expr_reads_var(e, name))
        }
        IrStmt::ForInit { init, cond, step, body, .. } => {
            init.iter().chain(step.iter()).chain(body.iter()).any(|x| stmt_reads_var(x, name))
                || expr_reads_var(cond, name)
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
            expr_reads_var(cond, name) || body.iter().any(|x| stmt_reads_var(x, name))
        }
        IrStmt::For { var, iter, body, .. } => {
            flat(var) || expr_reads_var(iter, name) || body.iter().any(|x| stmt_reads_var(x, name))
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_reads_var(cond, name)
                || then.iter().any(|x| stmt_reads_var(x, name))
                || elsifs.iter().any(|(c, b)| {
                    expr_reads_var(c, name) || b.iter().any(|x| stmt_reads_var(x, name))
                })
                || else_.iter().any(|x| stmt_reads_var(x, name))
        }
        IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
            body.iter().chain(else_body.iter()).chain(finally_body.iter()).any(|x| stmt_reads_var(x, name))
                || excepts.iter().any(|e| {
                    e.match_expr.as_ref().is_some_and(|m| expr_reads_var(m, name))
                        || e.as_name.as_ref().is_some_and(|n| flat(n))
                        || e.body.iter().any(|x| stmt_reads_var(x, name))
                })
        }
        IrStmt::Case { discriminant, clauses, .. } => {
            expr_reads_var(discriminant, name)
                || clauses.iter().any(|c| c.body.iter().any(|x| stmt_reads_var(x, name)))
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            b.iter().any(|x| stmt_reads_var(x, name))
        }
        IrStmt::Function { body, .. } => body.iter().any(|x| stmt_reads_var(x, name)),
        IrStmt::Pipeline { stages, .. } => {
            stages.iter().any(|st| st.iter().any(|x| stmt_reads_var(x, name)))
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            inner.iter().any(|x| stmt_reads_var(x, name))
                || redirects.iter().any(|r| expr_reads_var(&r.target, name))
        }
        IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => expr_reads_var(e, name),
        IrStmt::WriteFile { path, content, .. } => {
            expr_reads_var(path, name) || expr_reads_var(content, name)
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            e.as_ref().is_some_and(|x| expr_reads_var(x, name))
        }
        IrStmt::SetChildError(e) => expr_reads_var(e, name),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_reads_var(expr, name),
        IrStmt::Exec { cmd, args, redirects, capture, .. } => {
            expr_reads_var(cmd, name)
                || args.iter().chain(redirects.iter()).any(|a| expr_reads_var(a, name))
                || capture.as_ref().is_some_and(|c| flat(c))
        }
        IrStmt::Select { clauses } => clauses.iter().any(|c| {
            c.target.as_ref().is_some_and(|t| flat(t))
                || c.ch.as_ref().is_some_and(|x| expr_reads_var(x, name))
                || c.value.as_ref().is_some_and(|x| expr_reads_var(x, name))
                || c.body.iter().any(|x| stmt_reads_var(x, name))
        }),
        IrStmt::Asm { outputs, inputs, .. } => outputs
            .iter()
            .chain(inputs.iter())
            .any(|(n, e)| flat(n) || expr_reads_var(e, name)),
        IrStmt::Shaderize { var, body, .. } => {
            flat(var) || body.iter().any(|x| stmt_reads_var(x, name))
        }
        IrStmt::Ext(x) => x.children().iter().any(|c| stmt_reads_var(c, name)),
        // Nameless control (no names to mention).
        IrStmt::Continue | IrStmt::Break | IrStmt::Require(_) | IrStmt::Label(_) | IrStmt::Goto(_) => false,
        // Opaque bridge text (may name anything — assume mention).
        IrStmt::RawText(_) => true,
    }
}

fn arith_reads_var(a: &ArithAst, name: &str) -> bool {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => {
            v == name || v.starts_with(&format!("{name}["))
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_reads_var(lhs, name) || arith_reads_var(rhs, name)
        }
        ArithAst::Un { arg, .. } => arith_reads_var(arg, name),
        ArithAst::Cond { test, then, else_ } => {
            arith_reads_var(test, name) || arith_reads_var(then, name) || arith_reads_var(else_, name)
        }
        ArithAst::Assign { var, rhs, .. } => var == name || arith_reads_var(rhs, name),
        ArithAst::IncDec { var, .. } => var == name,
        ArithAst::Cast { arg, .. } => arith_reads_var(arg, name),
        ArithAst::Index { var, key } => var == name || arith_reads_var(key, name),
        _ => false,
    }
}

fn lambda_mentions(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Lambda { body, .. } => body.iter().any(|s| stmt_reads_var(s, name)),
        IrExpr::Arrow(b) => b.iter().any(|s| stmt_reads_var(s, name)),
        _ => false,
    }
}

fn ext_mentions(x: &Box<dyn crate::shir_nodes::ExtExpr>, name: &str) -> bool {
    x.children().iter().any(|c| expr_reads_var(c, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn scalar(var: &str) -> crate::ir::AssignTarget {
        crate::ir::AssignTarget { var: var.to_string(), sigil: None, indices: vec![] }
    }

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign { targets: vec![scalar(var)], expr, asm: None }
    }

    fn str_lit(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn arith(a: ArithAst) -> IrExpr {
        IrExpr::Arith(Box::new(a))
    }

    fn v(name: &str) -> ArithAst {
        ArithAst::Var(name.to_string())
    }

    fn num(n: i64) -> ArithAst {
        ArithAst::Num(n)
    }

    fn bin(op: &str, l: ArithAst, r: ArithAst) -> ArithAst {
        ArithAst::Bin { op: op.to_string(), lhs: Box::new(l), rhs: Box::new(r) }
    }

    /// Hash body: a=(i*53)%256; b=(i*89)%256; s=((s+a*31+b*17)%256).
    fn hash_body() -> Vec<IrStmt> {
        vec![
            assign("a", arith(bin("%", bin("*", v("i"), num(53)), num(256)))),
            assign("b", arith(bin("%", bin("*", v("i"), num(89)), num(256)))),
            assign("s", arith(bin("%",
                bin("+", bin("+", v("s"),
                    bin("*", v("a"), num(31))),
                    bin("*", v("b"), num(17))),
                num(256)))),
        ]
    }

    fn wrap(body: Vec<IrStmt>) -> Vec<IrStmt> {
        // (isolate_body operates on the body vec directly.)
        body
    }

    #[test]
    fn hash_isolates() {
        let mut body = wrap(hash_body());
        assert!(isolate_body(&mut body));
        assert_eq!(body.len(), 1);
        let IrStmt::Assign { targets, expr, .. } = &body[0] else {
            panic!("single assign");
        };
        assert_eq!(targets[0].var, "s");
        // No temp mentions survive.
        assert!(!expr_reads_var(expr, "a"));
        assert!(!expr_reads_var(expr, "b"));
        assert!(expr_reads_var(expr, "s"));
        assert!(expr_reads_var(expr, "i"));
    }

    #[test]
    fn multi_use_temp_blocks() {
        // `a` read twice (tail + extra) — not single-use.
        let mut body = hash_body();
        body.push(assign("z", arith(v("a"))));
        assert!(!isolate_body(&mut body));
    }

    #[test]
    fn carried_temp_blocks() {
        // Read-before-def: `s` accumulator aside, `a` read in first stmt
        // before its def (previous-iteration value).
        let mut body = vec![
            assign("z", arith(v("a"))),
            assign("a", arith(num(1))),
            assign("s", arith(bin("+", v("s"), v("a")))),
        ];
        assert!(!isolate_body(&mut body));
    }

    #[test]
    fn counter_write_blocks() {
        // Body writes the counter mid-body with a counter-reading temp.
        let mut body = vec![
            assign("a", arith(bin("+", v("i"), num(1)))),
            assign("i", arith(bin("+", v("i"), num(1)))),
            assign("s", arith(bin("+", v("s"), v("a")))),
        ];
        assert!(!isolate_body(&mut body));
    }

    #[test]
    fn nonacc_tail_blocks() {
        // Tail not self-referential (`z = a+b`).
        let mut body = vec![
            assign("a", arith(num(1))),
            assign("b", arith(num(2))),
            assign("z", arith(bin("+", v("a"), v("b")))),
        ];
        assert!(!isolate_body(&mut body));
    }

    #[test]
    fn status_temp_blocks() {
        // `$?` in temp RHS observes shifting status.
        let mut body = vec![
            assign("a", arith(v("?"))),
            assign("s", arith(bin("+", v("s"), v("a")))),
        ];
        assert!(!isolate_body(&mut body));
    }

    #[test]
    fn temp_chain_collapses() {
        // t1 -> t2 -> acc chain inlines in one forward pass.
        let mut body = vec![
            assign("t1", arith(bin("*", v("i"), num(2)))),
            assign("t2", arith(bin("+", v("t1"), num(1)))),
            assign("s", arith(bin("+", v("s"), v("t2")))),
        ];
        assert!(isolate_body(&mut body));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn control_flow_blocks() {
        // Non-assign stmt in body vetoes the whole body.
        let mut body = hash_body();
        body.insert(1, IrStmt::Expr(arith(num(0))));
        assert!(!isolate_body(&mut body));
    }
}
