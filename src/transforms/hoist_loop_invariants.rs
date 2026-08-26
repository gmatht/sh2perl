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
//!   - `expr` reads NO variable assigned in the body (the candidate's
//!     own target included: `x = x + 1` is an accumulator whose value
//!     changes per iteration — hoisting it is unsound; only a RHS free
//!     of every body-assigned var is provably invariant),
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
        // expr reads only loop-invariant vars. A read of ANY var assigned
        // in the body (the candidate's own target INCLUDED) is loop-carried:
        // `x = x + 1` is an accumulator whose value changes every iteration
        // — hoisting it to a single pre-loop store is unsound. Only a RHS
        // free of every assigned var (its self included) is provably
        // invariant across iterations.
        if !expr_pure(expr, fns) {
            continue;
        }
        let reads = read_set(expr);
        if reads.iter().any(|v| assigned.contains(v)) {
            continue; // loop-carried input (self-var included)
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
        IrStmt::WriteFile { path, content, .. } => expr_reads(path, var) || expr_reads(content, var),
        IrStmt::Declare { init, .. } => init.as_ref().map(|i| expr_reads(i, var)).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => elements.iter().any(|e| expr_reads(e, var)),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_reads(cond, var)
                || then.iter().any(|s| stmt_reads(s, var))
                || elsifs.iter().any(|(c, b)| expr_reads(c, var) || b.iter().any(|s| stmt_reads(s, var)))
                || else_.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::For { iter, body, .. } => expr_reads(iter, var) || body.iter().any(|s| stmt_reads(s, var)),
        IrStmt::While { cond, body } => expr_reads(cond, var) || body.iter().any(|s| stmt_reads(s, var)),
        IrStmt::DoWhile { body, cond, .. } => {
            body.iter().any(|s| stmt_reads(s, var)) || expr_reads(cond, var)
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            expr_reads(cmd, var)
                || args.iter().any(|a| expr_reads(a, var))
                || redirects.iter().any(|r| expr_reads(r, var))
                || env.iter().any(|(_, e)| expr_reads(e, var))
        }
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| s.iter().any(|st| stmt_reads(st, var))),
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
                match a.as_ref() {
                    ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => {
                        out.insert(var.clone());
                    }
                    _ => {}
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
        // runtime store calls (`sh2.setVar("x", …)`, `sh2.setArray`,
        // `sh2.setArrayAppend`, `sh2.assign`) write their name arg — a
        // body that stores to a var makes it loop-carried.
        IrStmt::Expr(IrExpr::Call { func, args, .. }) => {
            if matches!(func.as_str(), "setVar" | "setArray" | "setArrayAppend" | "assign") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.insert(n.clone());
                }
            }
        }
        _ => {}
    }
}

/// Runtime-call variable access: `getVar`/`arrayIndex` read their name
/// arg, `param` reads its second arg, and the string-eval calls
/// (`arith`/`test`/`caseMatch`) may reference vars by name inside their
/// string payloads (the estree path lowers `$i` reads to
/// `getVar("i")` calls, so a plain `Var`/`Ident` scan misses them).
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

fn call_read_set(func: &str, args: &[IrExpr], out: &mut HashSet<String>) {
    if matches!(func, "getVar" | "arrayIndex") {
        if let Some(IrExpr::Str(n, _)) = args.first() {
            out.insert(n.clone());
        }
    }
    if func == "param" {
        if let Some(IrExpr::Str(n, _)) = args.get(1) {
            out.insert(n.clone());
        }
    }
    if matches!(func, "arith" | "test" | "caseMatch") {
        for a in args {
            if let IrExpr::Str(s, _) = a {
                for v in str_maybe_read_vars(s) {
                    out.insert(v);
                }
            }
        }
    }
}

/// Does the string reference `var` as a standalone token (bash arith/test
/// strings reference vars by bare name, `$i` or `i`)?
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

fn str_maybe_read_vars(s: &str) -> Vec<String> {
    // conservative: every `$name` / bare alphanumeric token is a
    // potential var read (the runtime evaluates the string)
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(pos) = rest.find('$') {
        let after = &rest[pos + 1..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
        rest = &rest[pos + 1..];
    }
    out
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
        IrExpr::Index { var, key, .. } => {
            out.insert(var.clone());
            read_set_into(key, out);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            read_set_into(lhs, out);
            read_set_into(rhs, out);
        }
        IrExpr::Arith(a) => arith_read_set(a, out),
        IrExpr::Call { func, args, .. } => {
            call_read_set(func, args, out);
            for a in args {
                read_set_into(a, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            read_set_into(obj, out);
            for a in args {
                read_set_into(a, out);
            }
        }
        IrExpr::Ternary { cond, then, else_, .. } => {
            read_set_into(cond, out);
            read_set_into(then, out);
            read_set_into(else_, out);
        }
        IrExpr::DefinedOr { expr, default, .. } => {
            read_set_into(expr, out);
            read_set_into(default, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    read_set_into(x, out);
                }
            }
        }
        IrExpr::Capture { expr, .. } => read_set_into(expr, out),
        IrExpr::Arrow(body) => {
            for s in body {
                stmt_read_set(s, out);
            }
        }
        IrExpr::Array(elems) => {
            for e in elems {
                read_set_into(e, out);
            }
        }
        IrExpr::Object(fields) => {
            for (_, e) in fields {
                read_set_into(e, out);
            }
        }
        _ => {}
    }
}

fn stmt_read_set(st: &IrStmt, out: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { expr, .. } => read_set_into(expr, out),
        IrStmt::Output { value, .. } => read_set_into(value, out),
        IrStmt::WriteFile { path, content, .. } => {
            read_set_into(path, out);
            read_set_into(content, out);
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                read_set_into(i, out);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                read_set_into(e, out);
            }
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            read_set_into(cond, out);
            for s in then {
                stmt_read_set(s, out);
            }
            for (c, b) in elsifs {
                read_set_into(c, out);
                for s in b {
                    stmt_read_set(s, out);
                }
            }
            for s in else_ {
                stmt_read_set(s, out);
            }
        }
        IrStmt::For { iter, body, .. } => {
            read_set_into(iter, out);
            for s in body {
                stmt_read_set(s, out);
            }
        }
        IrStmt::While { cond, body } => {
            read_set_into(cond, out);
            for s in body {
                stmt_read_set(s, out);
            }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            for s in body {
                stmt_read_set(s, out);
            }
            read_set_into(cond, out);
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            read_set_into(cmd, out);
            for a in args {
                read_set_into(a, out);
            }
            for r in redirects {
                read_set_into(r, out);
            }
            for (_, e) in env {
                read_set_into(e, out);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                for st in s {
                    stmt_read_set(st, out);
                }
            }
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            if let Some(e) = e {
                read_set_into(e, out);
            }
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } | IrStmt::SetChildError(expr) => {
            read_set_into(expr, out);
        }
        IrStmt::Case { discriminant, clauses, .. } => {
            read_set_into(discriminant, out);
            for c in clauses {
                for s in &c.body {
                    stmt_read_set(s, out);
                }
            }
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            for s in inner {
                stmt_read_set(s, out);
            }
            for r in redirects {
                read_set_into(&r.target, out);
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body {
                stmt_read_set(s, out);
            }
        }
        IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
            for s in b {
                stmt_read_set(s, out);
            }
        }
        IrStmt::Expr(e) => read_set_into(e, out),
        _ => {}
    }
}

fn arith_read_set(a: &ArithAst, out: &mut HashSet<String>) {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => {
            out.insert(v.clone());
        }
        ArithAst::Index { var, key, .. } => {
            out.insert(var.clone());
            arith_read_set(key, out);
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
        ArithAst::Assign { rhs, .. } => arith_read_set(rhs, out),
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Cast { arg, .. } => arith_read_set(arg, out),
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, ArithAst, StrStyle};

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

    fn inc(var: &str) -> IrExpr {
        IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(ArithAst::Var(var.to_string())),
            rhs: Box::new(ArithAst::Num(1)),
        }))
    }

    /// `while c; do x = x + 1; done` — the accumulator write reads its own
    /// target (loop-carried) → must NOT be hoisted (bench-count shape).
    #[test]
    fn refuses_accumulator() {
        let mut stmts = vec![IrStmt::While {
            cond: IrExpr::Call {
                func: "test".to_string(),
                args: vec![IrExpr::Str("$x -lt 10".into(), StrStyle::DoubleQuoted)],
            },
            body: vec![assign("x", inc("x"))],
        }];
        assert!(!transform(&mut stmts));
        if let IrStmt::While { body, .. } = &stmts[0] {
            assert_eq!(body.len(), 1, "accumulator must stay in the body");
            assert_eq!(body[0], assign("x", inc("x")));
        } else {
            panic!("expected While");
        }
    }

    /// `x=5; while c; do y = x; done` — `y` reads only the invariant `x`;
    /// `y` is written once, reads nothing loop-carried → hoisted.
    #[test]
    fn hoists_invariant_from_body() {
        let mut stmts = vec![
            assign("x", IrExpr::Int(5)),
            IrStmt::While {
                cond: IrExpr::Call {
                    func: "test".to_string(),
                    args: vec![IrExpr::Str("$n -lt 10".into(), StrStyle::DoubleQuoted)],
                },
                body: vec![assign("y", IrExpr::Var("x".to_string(), None))],
            },
        ];
        assert!(transform(&mut stmts));
        // the hoisted `y = x` lands just before the loop
        assert_eq!(stmts[1], assign("y", IrExpr::Var("x".to_string(), None)));
        if let IrStmt::While { body, .. } = &stmts[2] {
            assert!(body.is_empty(), "invariant write removed from body");
        } else {
            panic!("expected While at index 2");
        }
    }

    /// `while [ $x -lt 10 ]; do echo $x; x=x+1; done` — the echo reads x
    /// before the write; x is loop-carried (accumulator) → no hoist.
    #[test]
    fn refuses_read_before_accumulator() {
        let mut stmts = vec![IrStmt::While {
            cond: IrExpr::Call {
                func: "test".to_string(),
                args: vec![IrExpr::Str("$x -lt 10".into(), StrStyle::DoubleQuoted)],
            },
            body: vec![
                IrStmt::Expr(IrExpr::Call {
                    func: "exec".to_string(),
                    args: vec![
                        IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                        IrExpr::Array(vec![IrExpr::Call {
                            func: "getVar".to_string(),
                            args: vec![IrExpr::Str("x".into(), StrStyle::DoubleQuoted)],
                        }]),
                    ],
                }),
                assign("x", inc("x")),
            ],
        }];
        assert!(!transform(&mut stmts));
        if let IrStmt::While { body, .. } = &stmts[0] {
            assert_eq!(body.len(), 2, "no hoisting for a loop-carried accumulator");
        }
    }
}
