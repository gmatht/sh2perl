//! append-to-store — normalize a counted loop's growable append
//! `a = append(a, v)` into the affine indexed store `a[i] = v`.
//!
//! Python/C frontends lower `a.append(v)` (the CPython-valid way to grow
//! a list) to a `setArrayAppend` runtime call. Every backend renders the
//! indexed store more cheaply than a growable-array call, and the
//! shared analyses key on stores: the CUDA map candidacy needs an
//! `A[<i>]` target, and `fuse-fill-consume` needs one too. This pass is
//! the frontend-safe normalisation that gives all of them the shape.
//!
//! ```text
//! a = setArray(a, [])
//! for i in 0..N:            for i in 0..N:
//!     a = append(a, F(i))  →     a[i] = F(i)
//! ```
//!
//! REFUSE > GUESS — all must hold:
//! - the array has EXACTLY ONE `setArrayAppend` site, an EMPTY
//!   `setArray`/array-literal init, and NO other write anywhere in the
//!   program (so the append count is exactly the iteration counter);
//! - the append is 1:1 (`[v]`, not `[a, b]` — a multi-element append
//!   does not advance the index by one);
//! - the loop is counted from 0 stepping +1 (`ForInit` init `i=0` /
//!   step `i++`, or a `While` whose body ends in `i++` after a `i=0`);
//! - no `break`/`continue`/`return`/`goto` in the body (an early exit
//!   desynchronises the counter from the append count).
//!
//! Reads of the array elsewhere are allowed: after the fill the contents
//! at every index are identical to the append built them, and the length
//! is the same (one append per trip).

use crate::ir::{ArithAst, IrExpr, IrStmt};
use std::collections::{BTreeMap, BTreeSet};

/// Apply to one statement list (recursively). Returns whether changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // Recurse first (nested blocks may hold their own append loops).
    let mut changed = false;
    for s in stmts.iter_mut() {
        changed |= recurse(s);
    }
    let mut info: BTreeMap<String, AppendInfo> = BTreeMap::new();
    collect(stmts, &mut info);
    let ok: BTreeSet<String> = info
        .iter()
        .filter(|(_, i)| i.appends == 1 && !i.other && i.empty_init)
        .map(|(k, _)| k.clone())
        .collect();
    if ok.is_empty() {
        return changed;
    }
    let consts = const_int_vars(stmts);
    changed |= rewrite_list(stmts, &ok, &consts);
    changed
}

#[derive(Default)]
struct AppendInfo {
    appends: usize,
    other: bool,
    empty_init: bool,
}

fn recurse(s: &mut IrStmt) -> bool {
    match s {
        IrStmt::If { then, elsifs, else_, .. } => {
            let mut c = transform(then) | transform(else_);
            for (_, b) in elsifs {
                c |= transform(b);
            }
            c
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. } => transform(body),
        IrStmt::ForInit { init, step, body, .. } => {
            transform(init) | transform(step) | transform(body)
        }
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = transform(body);
            for (_, b) in named_blocks {
                c |= transform(b);
            }
            c
        }
        IrStmt::Try { body, else_body, finally_body, excepts } => {
            let mut c = transform(body) | transform(else_body) | transform(finally_body);
            for e in excepts {
                c |= transform(&mut e.body);
            }
            c
        }
        IrStmt::Case { clauses, .. } => clauses.iter_mut().map(|c| transform(&mut c.body)).fold(false, |a, b| a | b),
        IrStmt::Select { clauses } => clauses.iter_mut().map(|c| transform(&mut c.body)).fold(false, |a, b| a | b),
        _ => false,
    }
}

/// The counted loop's bound is statically known (compares against a
/// literal). With an unknown bound we keep the growable append instead of
/// rewriting it to `a[i]=v`: the C backend's list profile then routes the
/// array to a growable vec (the fixed ARR_CAP would exit 127 on a long
/// enough input). REFUSE > GUESS — the rewrite is only an optimization.
fn literal_bound(c: &IrExpr, consts: &BTreeSet<String>) -> bool {
    fn lit(e: &IrExpr, consts: &BTreeSet<String>) -> bool {
        match e {
            IrExpr::Int(_) => true,
            IrExpr::Str(s, _) => {
                let t = s.trim();
                !t.is_empty()
                    && t.trim_start_matches('-').chars().all(|ch| ch.is_ascii_digit())
            }
            IrExpr::Var(n, _) | IrExpr::Ident(n) => consts.contains(n),
            _ => false,
        }
    }
    match c {
        IrExpr::BinOp { lhs, rhs, .. } => lit(lhs, consts) || lit(rhs, consts),
        IrExpr::Arith(a) => match a.as_ref() {
            ArithAst::Bin { lhs, rhs, .. } => {
                matches!(lhs.as_ref(), ArithAst::Num(_))
                    || matches!(rhs.as_ref(), ArithAst::Num(_))
                    || matches!(lhs.as_ref(), ArithAst::Var(v) if consts.contains(v))
                    || matches!(rhs.as_ref(), ArithAst::Var(v) if consts.contains(v))
            }
            _ => false,
        },
        IrExpr::Bool(_) => true,
        other => lit(other, consts),
    }
}

/// Variables assigned a numeric literal exactly once: a counted bound that
/// reads such a var (`N=10; for i in range(N)`) is statically known.
fn const_int_vars(stmts: &[IrStmt]) -> BTreeSet<String> {
    fn numeric(e: &IrExpr) -> bool {
        match e {
            IrExpr::Int(_) => true,
            IrExpr::Str(s, _) => {
                let t = s.trim();
                !t.is_empty()
                    && t.trim_start_matches('-').chars().all(|ch| ch.is_ascii_digit())
            }
            IrExpr::Arith(a) => matches!(a.as_ref(), ArithAst::Num(_)),
            _ => false,
        }
    }
    fn scan(list: &[IrStmt], count: &mut BTreeMap<String, usize>, numeric_vars: &mut BTreeSet<String>) {
        for s in list {
            if let IrStmt::Assign { targets, expr, .. } = s {
                if let Some(t) = targets.first() {
                    if t.indices.is_empty() && !t.var.contains('[') && targets.len() == 1 {
                        *count.entry(t.var.clone()).or_insert(0) += 1;
                        if numeric(expr) {
                            numeric_vars.insert(t.var.clone());
                        }
                    }
                }
            }
            // detect reassignments anywhere
            match s {
                IrStmt::If { then, elsifs, else_, .. } => {
                    scan(then, count, numeric_vars);
                    for (_, b) in elsifs {
                        scan(b, count, numeric_vars);
                    }
                    scan(else_, count, numeric_vars);
                }
                IrStmt::For { body, .. }
                | IrStmt::While { body, .. }
                | IrStmt::DoWhile { body, .. }
                | IrStmt::Block(body)
                | IrStmt::Subshell(body)
                | IrStmt::Background(body)
                | IrStmt::Redirect { inner: body, .. } => scan(body, count, numeric_vars),
                IrStmt::ForInit { init, step, body, .. } => {
                    scan(init, count, numeric_vars);
                    scan(step, count, numeric_vars);
                    scan(body, count, numeric_vars);
                }
                IrStmt::Function { body, named_blocks, .. } => {
                    scan(body, count, numeric_vars);
                    for (_, b) in named_blocks {
                        scan(b, count, numeric_vars);
                    }
                }
                IrStmt::Try { body, excepts, else_body, finally_body } => {
                    scan(body, count, numeric_vars);
                    for e in excepts {
                        scan(&e.body, count, numeric_vars);
                    }
                    scan(else_body, count, numeric_vars);
                    scan(finally_body, count, numeric_vars);
                }
                IrStmt::Case { clauses, .. } => clauses.iter().for_each(|c| scan(&c.body, count, numeric_vars)),
                IrStmt::Select { clauses } => clauses.iter().for_each(|c| scan(&c.body, count, numeric_vars)),
                _ => {}
            }
        }
    }
    let mut count = BTreeMap::new();
    let mut numeric_vars = BTreeSet::new();
    scan(stmts, &mut count, &mut numeric_vars);
    numeric_vars.into_iter().filter(|n| count.get(n) == Some(&1)).collect()
}

fn base_of(var: &str) -> String {
    var.split('[').next().unwrap_or(var).to_string()
}

fn is_append_call(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Call { func, .. } if func == "setArrayAppend")
}

fn is_empty_array_init(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } if func == "setArray" => match args.get(1) {
            Some(IrExpr::Array(items)) => items.is_empty(),
            _ => false,
        },
        IrExpr::Array(items) => items.is_empty(),
        _ => false,
    }
}

fn appended_value(e: &IrExpr) -> Option<IrExpr> {
    let IrExpr::Call { args, .. } = e else { return None };
    match args.get(1) {
        Some(IrExpr::Array(items)) if items.len() == 1 => Some(items[0].clone()),
        _ => None,
    }
}

fn collect(stmts: &[IrStmt], info: &mut BTreeMap<String, AppendInfo>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                if let Some(t) = targets.first() {
                    if t.indices.is_empty() {
                        let base = base_of(&t.var);
                        if !base.is_empty() {
                            let e = info.entry(base).or_default();
                            if is_append_call(expr) {
                                e.appends += 1;
                            } else if is_empty_array_init(expr) {
                                e.empty_init = true;
                            } else {
                                e.other = true;
                            }
                        }
                    } else {
                        // A structured-index store at a non-append site.
                        let base = base_of(&t.var);
                        info.entry(base).or_default().other = true;
                    }
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                info.entry(var.clone()).or_default().other = true;
            }
            other => collect_kids(other, info),
        }
    }
}

fn collect_kids(s: &IrStmt, info: &mut BTreeMap<String, AppendInfo>) {
    match s {
        IrStmt::If { then, elsifs, else_, .. } => {
            collect(then, info);
            for (_, b) in elsifs {
                collect(b, info);
            }
            collect(else_, info);
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. } => collect(body, info),
        IrStmt::ForInit { init, step, body, .. } => {
            collect(init, info);
            collect(step, info);
            collect(body, info);
        }
        IrStmt::Function { body, named_blocks, .. } => {
            collect(body, info);
            for (_, b) in named_blocks {
                collect(b, info);
            }
        }
        IrStmt::Try { body, else_body, finally_body, excepts } => {
            collect(body, info);
            collect(else_body, info);
            collect(finally_body, info);
            for e in excepts {
                collect(&e.body, info);
            }
        }
        IrStmt::Case { clauses, .. } => clauses.iter().for_each(|c| collect(&c.body, info)),
        IrStmt::Select { clauses } => clauses.iter().for_each(|c| collect(&c.body, info)),
        _ => {}
    }
}

fn rewrite_list(stmts: &mut Vec<IrStmt>, ok: &BTreeSet<String>, consts: &BTreeSet<String>) -> bool {
    let mut changed = false;
    for i in 0..stmts.len() {
        // `ForInit` counted loop: counter from init, +1 from step.
        let header = match &stmts[i] {
            IrStmt::ForInit { init, cond, step, body } if literal_bound(cond, consts) => {
                counted_forinit(init, step).map(|c| (c, body.clone()))
            }
            _ => None,
        };
        if let Some((counter, body)) = header {
            let Some(newbody) = rewrite_body(&body, &counter, ok) else {
                continue;
            };
            if let IrStmt::ForInit { body, .. } = &mut stmts[i] {
                *body = newbody;
                changed = true;
            }
            continue;
        }
        // `While` counted loop with a preceding `i = 0`.
        if let IrStmt::While { cond, body } = &stmts[i] {
            if !literal_bound(cond, consts) {
                continue;
            }
            if i > 0 {
                if let Some(counter) = zero_init_of(&stmts[i - 1]) {
                    let Some(newbody) = rewrite_body(body, &counter, ok) else {
                        continue;
                    };
                    if let IrStmt::While { body, .. } = &mut stmts[i] {
                        *body = newbody;
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

fn rewrite_body(body: &[IrStmt], counter: &str, ok: &BTreeSet<String>) -> Option<Vec<IrStmt>> {
    if body.iter().any(escapes) {
        return None;
    }
    let mut out = body.to_vec();
    let mut hits = 0;
    for s in out.iter_mut() {
        let IrStmt::Assign { targets, expr, .. } = s else { continue };
        let Some(t) = targets.first() else { continue };
        if !t.indices.is_empty() {
            continue;
        }
        let base = base_of(&t.var);
        if !ok.contains(&base) {
            continue;
        }
        if let Some(value) = appended_value(expr) {
            *s = IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: format!("{base}[{counter}]"),
                    sigil: None,
                    indices: Vec::new(),
                }],
                expr: value,
                asm: None,
            };
            hits += 1;
        }
    }
    // Exactly one append per trip (the info scan already proved one site
    // program-wide, but a body could contain it under a nested if — the
    // scan counted it; here require exactly one rewritten site).
    (hits == 1).then_some(out)
}

/// (counter, +1 step) for a ForInit header, or None.
fn counted_forinit(init: &[IrStmt], step: &[IrStmt]) -> Option<String> {
    let counter = zero_init_of(init.last()?)?;
    let [IrStmt::Assign { targets, expr, .. }] = step else {
        return None;
    };
    if targets.len() != 1 || targets[0].var != counter || !targets[0].indices.is_empty() {
        return None;
    }
    let IrExpr::Arith(a) = expr else { return None };
    let step_ok = match a.as_ref() {
        ArithAst::IncDec { var, delta, .. } => var == &counter && *delta == 1,
        ArithAst::Assign { var, op, rhs } if var == &counter && op == "+=" => {
            num_of(rhs) == Some(1)
        }
        ArithAst::Bin { op, lhs, rhs } if op == "+" => {
            matches!(lhs.as_ref(), ArithAst::Var(v) | ArithAst::Ident(v) if v == &counter)
                && num_of(rhs) == Some(1)
        }
        _ => false,
    };
    step_ok.then_some(counter)
}

/// `v = <0-literal>` (with any Cast wrapping) → the counter name.
fn zero_init_of(s: &IrStmt) -> Option<String> {
    let IrStmt::Assign { targets, expr, .. } = s else {
        return None;
    };
    let [t] = targets.as_slice() else { return None };
    if !t.indices.is_empty() || t.var.contains('[') {
        return None;
    }
    let zero = match expr {
        IrExpr::Int(n) => *n == 0,
        IrExpr::Str(s, _) => s.trim() == "0",
        IrExpr::Arith(a) => num_of(a) == Some(0),
        _ => false,
    };
    zero.then(|| t.var.clone())
}

/// Numeric literal under Cast/Assign(=) wrapping.
fn num_of(a: &ArithAst) -> Option<i64> {
    match a {
        ArithAst::Num(n) => Some(*n),
        ArithAst::Cast { arg, .. } => num_of(arg),
        ArithAst::Assign { op, rhs, .. } if op == "=" => num_of(rhs),
        _ => None,
    }
}

/// Body-level escapes desynchronise the counter from the append count.
fn escapes(s: &IrStmt) -> bool {
    match s {
        IrStmt::Break | IrStmt::Continue | IrStmt::Return(_) | IrStmt::Exit(_)
        | IrStmt::Goto(_) | IrStmt::Label(_) => true,
        IrStmt::If { then, elsifs, else_, .. } => {
            then.iter().any(escapes)
                || elsifs.iter().any(|(_, b)| b.iter().any(escapes))
                || else_.iter().any(escapes)
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Redirect { inner: body, .. } => body.iter().any(escapes),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn scalar(var: &str) -> AssignTarget {
        AssignTarget { var: var.to_string(), sigil: None, indices: vec![] }
    }
    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign { targets: vec![scalar(var)], expr, asm: None }
    }
    fn v(name: &str) -> ArithAst {
        ArithAst::Var(name.to_string())
    }
    fn arith(a: ArithAst) -> IrExpr {
        IrExpr::Arith(Box::new(a))
    }
    fn append(a: &str, value: ArithAst) -> IrStmt {
        assign(
            a,
            IrExpr::Call {
                func: "setArrayAppend".to_string(),
                args: vec![
                    IrExpr::Str(a.to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![arith(value)]),
                ],
            },
        )
    }
    fn empty_init(a: &str) -> IrStmt {
        assign(
            a,
            IrExpr::Call {
                func: "setArray".to_string(),
                args: vec![
                    IrExpr::Str(a.to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![]),
                ],
            },
        )
    }
    /// `for i in range(N): a.append(i*i)` with the frontend's structured
    /// Arith header.
    fn append_loop(a: &str) -> IrStmt {
        append_loop_bound(a, ArithAst::Num(100))
    }

    /// A counted append loop with a runtime (unknown) bound: the rewrite is
    /// only an optimization, so it must be refused and the growable append
    /// kept (the C backend then routes the array to a growable vec).
    fn append_loop_unbounded(a: &str) -> IrStmt {
        append_loop_bound(a, ArithAst::Var("N".to_string()))
    }

    fn append_loop_bound(a: &str, bound: ArithAst) -> IrStmt {
        IrStmt::ForInit {
            init: vec![assign("i", arith(ArithAst::Num(0)))],
            cond: arith(ArithAst::Bin {
                op: "<".to_string(),
                lhs: Box::new(v("i")),
                rhs: Box::new(bound),
            }),
            step: vec![assign(
                "i",
                arith(ArithAst::IncDec { var: "i".to_string(), delta: 1, prefix: false }),
            )],
            body: vec![append(a, ArithAst::Bin {
                op: "*".to_string(),
                lhs: Box::new(v("i")),
                rhs: Box::new(v("i")),
            })],
        }
    }

    #[test]
    fn append_in_counted_loop_becomes_store() {
        let mut prog = vec![empty_init("a"), append_loop("a")];
        assert!(transform(&mut prog), "must rewrite");
        let IrStmt::ForInit { body, .. } = &prog[1] else { panic!("loop") };
        let IrStmt::Assign { targets, .. } = &body[0] else { panic!("store") };
        assert_eq!(targets[0].var, "a[i]");
    }

    #[test]
    fn append_with_unknown_bound_keeps_growable() {
        // REFUSE > GUESS: the rewrite assumes the append count equals the
        // iteration count. With an unknown bound keep the growable append —
        // the C backend's list profile routes it to a growable vec, while a
        // rewrite would leave the array on the fixed ARR_CAP (exit 127).
        let mut prog = vec![empty_init("a"), append_loop_unbounded("a")];
        assert!(!transform(&mut prog), "unknown bound must keep the append");
    }

    #[test]
    fn two_appends_veto() {
        let mut prog = vec![empty_init("a"), append_loop("a")];
        let IrStmt::ForInit { body, .. } = &mut prog[1] else { panic!() };
        body.push(append("a", v("i")));
        assert!(!transform(&mut prog));
    }

    #[test]
    fn nonempty_init_vetoes() {
        let mut prog = vec![assign("a", IrExpr::Array(vec![IrExpr::Int(0)])), append_loop("a")];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn other_write_vetoes() {
        let mut prog = vec![empty_init("a"), append_loop("a"), assign("a", IrExpr::Int(0))];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn break_vetoes() {
        let mut prog = vec![empty_init("a"), append_loop("a")];
        let IrStmt::ForInit { body, .. } = &mut prog[1] else { panic!() };
        body.push(IrStmt::Break);
        assert!(!transform(&mut prog));
    }

    #[test]
    fn step2_vetoes() {
        let mut prog = vec![empty_init("a"), append_loop("a")];
        let IrStmt::ForInit { step, .. } = &mut prog[1] else { panic!() };
        step[0] = assign(
            "i",
            arith(ArithAst::IncDec { var: "i".to_string(), delta: 2, prefix: false }),
        );
        assert!(!transform(&mut prog));
    }
}
