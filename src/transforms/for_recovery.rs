//! for-recovery — recover a native `For` from a counter `while` loop
//! (`i=0; while [ $i -lt N ]; do …; i=$((i+1)); done`), the shIR
//! equivalent of the estree pass `nativeForLoops` in sh2runtime's
//! lower.js (which recovers `for (…)` from counter whiles on the JS
//! side today — the C/Go/Rust/Perl backends still emit `while`, so the
//! win is currently JS-only).
//!
//! ## Need
//! The C backend renders the textbook counter loop as
//! `while (_sh_site_0()) { i = (i + 1); }` — a native `for` is never
//! recovered, because the recovery lives in the JS estree pipeline
//! (`estree.js` → `nativeForLoops`). A shIR-level recovery lets EVERY
//! backend emit the native loop: C `for (i = a; i < b; i++)`, Go
//! `for i := a; i < b; i++`, Rust `for i in a..b`, JS
//! `for (let i = a; i < b; i++)`.
//!
//! ## Scope
//! Only `IrStmt::While` loops where ALL of:
//!   - the cond is a comparison of ONE counter variable against an
//!     integer bound (`test "$i -lt 10"` string form, or the lowered
//!     `BinOp { Lt/Le/Gt/Ge, Var(counter), Int(bound) }` form),
//!   - the body's LAST statement is the counter increment
//!     (`i=$((i+1))` — `Assign` whose expr is `Arith(Bin{+, Var(i),
//!     Num(1)})`, or the `BinOp{Add/Sub, Var(i), Int(±1)}` form),
//!   - the counter is not read or written ANYWHERE ELSE in the body
//!     (a second writer, or a read in a capture/subshell = refuse),
//!   - the counter's value is not read after the loop in the enclosing
//!     statement list (shell's post-loop value = the bound; the IR
//!     `For` leaves it at the last iteration value — the difference is
//!     the classic off-by-one, so refuse when the value escapes).
//! Anything else is left untouched — refuse > guess.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated, like the
//! rest of the registry). Runs inside `ast_to_ir` (NOT `ast_to_ir_raw`
//! — raw = unoptimized). The estree worker mediates the contract bit:
//! `IrExpr::Range` is already INCLUSIVE (`start..end`), so the bound is
//! adjusted (`i < 10` step +1 → `Range { start, end: bound - 1 }`) and
//! the renderers' For arms (estree.rs, cfront.rs, the C/Go/Perl
//! generators) emit the native loop from `For { var, iter: Range }`.

use crate::ir::{ArithAst, BinOpKind, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for i in 0..stmts.len() {
        if let Some(for_stmt) = try_convert_at(stmts.as_slice(), i) {
            // replace the While with the For (the increment is dropped
            // in the new body)
            stmts[i] = for_stmt;
            c = true;
        }
    }
    c
}

/// Pure decision function: does `stmts[i]` hold a convertible counter
/// While, and if so what For does it become? (No borrows escape — the
/// caller swaps the statement.)
fn try_convert_at(stmts: &[IrStmt], i: usize) -> Option<IrStmt> {
    let (counter, cmp, bound) = loop_cond(&stmts[i])?;
    let body = loop_body(&stmts[i])?;

    // the increment must be the LAST statement of the body
    let (inc, rest) = body.split_last()?;
    let step = increment_of(inc, &counter)?;

    // counter must not appear anywhere else in the body
    for st in rest {
        if stmt_mentions(st, &counter) {
            return None;
        }
    }
    // counter must not be read after the loop (post-loop value)
    for st in &stmts[i + 1..] {
        if stmt_mentions(st, &counter) {
            return None;
        }
    }

    // the initial value: the immediately-preceding Assign of the
    // counter to a literal. If absent, refuse.
    let start = match i.checked_sub(1).and_then(|j| init_of(&stmts[j], &counter)) {
        Some(v) => v,
        None => return None,
    };

    // inclusive Range bounds for step ±1 (the only steps accepted —
    // matching the native for's step)
    let end = match (cmp, step) {
        (BinOpKind::Lt, 1) => bound - 1,
        (BinOpKind::Le, 1) => bound,
        (BinOpKind::Gt, -1) => bound + 1,
        (BinOpKind::Ge, -1) => bound,
        _ => return None,
    };
    let (lo, hi) = if step > 0 { (start, end) } else { (end, start) };
    if lo > hi {
        return None; // empty loop — leave the while as-is
    }

    Some(IrStmt::For {
        var: counter,
        iter: IrExpr::Range { start: lo, end: hi },
        body: rest.to_vec(),
    })
}

/// The cond forms: `test "$i -lt 10"` (Call) or the lowered comparison.
fn loop_cond(st: &IrStmt) -> Option<(String, BinOpKind, i64)> {
    let cond = match st {
        IrStmt::While { cond, .. } => cond,
        _ => return None,
    };
    match cond {
        // `while [ $i -lt 10 ]` → Call { func: "test", args: ["$i -lt 10"] }
        IrExpr::Call { func, args } if func == "test" => {
            let s = match args.first()? {
                IrExpr::Str(s, _) => s,
                _ => return None,
            };
            parse_test_str(s)
        }
        // the lowered form: BinOp { Lt/Le/Gt/Ge, Var(counter), Int(bound) }
        IrExpr::BinOp { op, lhs, rhs }
            if matches!(
                op,
                BinOpKind::Lt | BinOpKind::Le | BinOpKind::Gt | BinOpKind::Ge
            ) =>
        {
            let (var, bound) = match (lhs.as_ref(), rhs.as_ref()) {
                (IrExpr::Var(v, _), IrExpr::Int(b)) => (v.clone(), *b),
                (IrExpr::Int(b), IrExpr::Var(v, _)) => (v.clone(), *b),
                _ => return None,
            };
            Some((var, op.clone(), bound))
        }
        _ => None,
    }
}

/// `"$i -lt 10"` / `"$i -le 10"` / mirror forms (`"10 -ge $i"` — the
/// bound first, comparison inverted).
fn parse_test_str(s: &str) -> Option<(String, BinOpKind, i64)> {
    // "$i -lt 10" — counter first
    if let Some(rest) = s.strip_prefix('$') {
        let var_end = rest.find(' ').unwrap_or(rest.len());
        let var = rest[..var_end].to_string();
        let tail = rest[var_end..].trim();
        let mut it = tail.splitn(2, ' ');
        let op = it.next()?;
        let num: i64 = it.next()?.trim().parse().ok()?;
        let kind = match op {
            "-lt" => BinOpKind::Lt,
            "-le" => BinOpKind::Le,
            "-gt" => BinOpKind::Gt,
            "-ge" => BinOpKind::Ge,
            _ => return None,
        };
        return Some((var, kind, num));
    }
    // "10 -lt $i" — bound first: 10 < i ≡ i > 10, etc.
    let mut it = s.splitn(3, ' ');
    let num: i64 = it.next()?.parse().ok()?;
    let op = it.next()?;
    let var = it.next()?.strip_prefix('$')?.to_string();
    let kind = match op {
        "-lt" => BinOpKind::Gt,
        "-le" => BinOpKind::Ge,
        "-gt" => BinOpKind::Lt,
        "-ge" => BinOpKind::Le,
        _ => return None,
    };
    Some((var, kind, num))
}

fn loop_body(st: &IrStmt) -> Option<&Vec<IrStmt>> {
    match st {
        IrStmt::While { body, .. } => Some(body),
        _ => None,
    }
}

/// `i=$((i+1))` / `i=$((i-1))` → ±1; matches Arith and plain BinOp.
fn increment_of(st: &IrStmt, counter: &str) -> Option<i64> {
    let expr = match st {
        IrStmt::Assign { targets, expr, .. } if targets.len() == 1 => {
            let t = &targets[0];
            if t.var != counter || !t.indices.is_empty() {
                return None;
            }
            expr
        }
        _ => return None,
    };
    match expr {
        IrExpr::Arith(a) => match a.as_ref() {
            ArithAst::Bin { op, lhs, rhs } => {
                let is_counter = |e: &ArithAst| matches!(e, ArithAst::Var(n) if n == counter);
                let lit = |e: &ArithAst| match e {
                    ArithAst::Num(n) => *n,
                    _ => 0,
                };
                if is_counter(lhs) {
                    match op.as_str() {
                        "+" => (lit(rhs) == 1).then_some(1),
                        "-" => (lit(rhs) == 1).then_some(-1),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        },
        IrExpr::BinOp { op, lhs, rhs } => match (lhs.as_ref(), rhs.as_ref()) {
            (IrExpr::Var(v, _), IrExpr::Int(n)) if v == counter => match op {
                BinOpKind::Add => (*n == 1).then_some(1),
                BinOpKind::Sub => (*n == 1).then_some(-1),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// the initial value: `i=0` immediately before the loop.
fn init_of(st: &IrStmt, counter: &str) -> Option<i64> {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            let t = targets.first()?;
            if t.var != counter || !t.indices.is_empty() {
                return None;
            }
            match expr {
                IrExpr::Int(v) => Some(*v),
                _ => None,
            }
        }
        _ => None,
    }
}

fn stmt_mentions(st: &IrStmt, name: &str) -> bool {
    let mut found = false;
    walk_stmt(st, &mut |e| {
        if expr_mentions(e, name) {
            found = true;
        }
    });
    found
}

fn expr_mentions(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == name,
        IrExpr::Index { var, key } => var == name || expr_mentions(key, name),
        IrExpr::BinOp { lhs, rhs, .. } => expr_mentions(lhs, name) || expr_mentions(rhs, name),
        IrExpr::Call { args, .. } => args.iter().any(|a| expr_mentions(a, name)),
        IrExpr::Arith(a) => arith_mentions(a, name),
        IrExpr::Capture { expr, .. } => expr_mentions(expr, name),
        IrExpr::Arrow(body) => body.iter().any(|s| stmt_mentions(s, name)),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Expr(x) => expr_mentions(x, name),
            _ => false,
        }),
        _ => false,
    }
}

fn arith_mentions(a: &ArithAst, name: &str) -> bool {
    match a {
        ArithAst::Var(v) => v == name,
        ArithAst::Index { var, key } => var == name || arith_mentions(key, name),
        ArithAst::Bin { lhs, rhs, .. } => arith_mentions(lhs, name) || arith_mentions(rhs, name),
        ArithAst::Un { arg, .. } => arith_mentions(arg, name),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_mentions(test, name) || arith_mentions(then, name) || arith_mentions(else_, name)
        }
        ArithAst::Assign { var, rhs, .. } => var == name || arith_mentions(rhs, name),
        _ => false,
    }
}

fn walk_stmt<F: FnMut(&IrExpr)>(st: &IrStmt, f: &mut F) {
    match st {
        IrStmt::Output { value, .. } => f(value),
        IrStmt::WriteFile { path, content, .. } => {
            f(path);
            f(content);
        }
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                for k in &t.indices {
                    f(k);
                }
            }
            f(expr);
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                f(i);
            }
        }
        IrStmt::If {
            cond, then, elsifs, else_, ..
        } => {
            f(cond);
            for s in then {
                walk_stmt(s, f);
            }
            for (c, b) in elsifs {
                f(c);
                for s in b {
                    walk_stmt(s, f);
                }
            }
            for s in else_ {
                walk_stmt(s, f);
            }
        }
        IrStmt::For { iter, body, .. } => {
            f(iter);
            for s in body {
                walk_stmt(s, f);
            }
        }
        IrStmt::While { cond, body } => {
            f(cond);
            for s in body {
                walk_stmt(s, f);
            }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            for s in body {
                walk_stmt(s, f);
            }
            f(cond);
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => f(expr),
        IrStmt::Exec { cmd, args, .. } => {
            f(cmd);
            for a in args {
                f(a);
            }
        }
        IrStmt::Return(Some(e)) => f(e),
        IrStmt::Exit(Some(e)) => f(e),
        IrStmt::SetChildError(e) => f(e),
        IrStmt::Case { discriminant, clauses } => {
            f(discriminant);
            for cl in clauses {
                for s in &cl.body {
                    walk_stmt(s, f);
                }
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            for s in inner {
                walk_stmt(s, f);
            }
            for r in redirects {
                f(&r.target);
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body {
                walk_stmt(s, f);
            }
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) | IrStmt::Block(v) => {
            for s in v {
                walk_stmt(s, f);
            }
        }
        IrStmt::Expr(e) => f(e),
        _ => {}
    }
}

// name: for-recovery
// prereqs: [none — a self-contained IR pattern match]
// invariant: only `While { cond: <counter <op> bound>, body: [*, inc] }`
//   loops with no other counter use (in body or after) become `For`;
//   every other loop and every other statement is untouched.
// scope: offered to c, go, rust, perl, sh, glsl (the estree owner — the
//   JS `nativeForLoops` pass is superseded once this lands)
// updates: none (first offer)
