//! seq-range-for — `for i in $(seq A B)` → native numeric range loop.
//!
//! The ESTree emitter renders a `Range`-iterable for-loop as a native JS
//! `for (let i = A; i <= B; i++)` — the hand-written ideal for
//! `sqrt1337.sh` (PLAN.md §9.1: `seq 1 N → native range`). The loop var
//! then becomes numeric-lifted by the EXISTING analysis
//! (`iter_numeric(Range) == Some(true)`), so `$((i*i))` lowers to native
//! `i * i` with no per-iteration coercion, and the 10k-word capture
//! buffer / array materialization disappears.
//!
//! ## Why a counter loop is not a word loop
//!
//! `for i in $(seq A B)` materializes the output, splits it into words,
//! and iterates the words; a native counter loop iterates the integers.
//! The two are byte-identical ONLY when:
//!
//! 1. the seq arguments are plain integers (floats pad/format via
//!    `%g`/locale — `seq 1 0.5` prints `1,0000000000000000`-style junk in
//!    some locales; `-w`/`-s` flags change the word text),
//! 2. no leading-zero args (GNU seq pads `01 02 …` — and bash
//!    arithmetic reads a leading-zero word as OCTAL: `$((010))` is 8, a
//!    counter loop would see 10),
//! 3. the body never WRITES the loop var (bash re-iterates the
//!    materialized list regardless of a body write; a counter loop's
//!    `i++` update would read the body-written value and derail the
//!    sequence),
//! 4. the range is finite and small enough for a JS number to step
//!    exactly (double precision: |v| ≤ 2^53; span ≤ 1M so the emitter's
//!    materialized-array FALLBACK — the runtime has no range helper — is
//!    bounded when the loop cannot go native).
//!
//! The transform is conservative on every point; anything doubtful keeps
//! the runtime `captureWords` path (correct, just unoptimized).
//!
//! Registered in `transforms.rs` (gated by `DEBASHC_TRANSFORMS` like the
//! rest of the registry). IR-shape changes only — the ESTree renderer
//! consumes the `Range` iterable; the Perl corpus path (AST generator)
//! never sees this IR.

use crate::ir::{IrExpr, IrStmt};

/// JS doubles step integers exactly only up to 2^53; a counter past that
/// silently loses the increment. seq itself prints beyond it fine, so the
/// transform just declines (bash's own `for i in $(seq …)` stays).
const MAX_SAFE_INT: i64 = 1 << 53;
/// The emitter's materialized-array fallback for a range iterable (used
/// only when the loop cannot take the native counter path — async
/// region / awaits in body / signals) must stay bounded: a 10^9-item
/// literal would OOM the compiler. sqrt1337's 10^4 is two orders under.
const MAX_SPAN: i64 = 1_000_000;

/// Apply the transform to a statement list. Returns whether anything
/// changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for s in stmts.iter_mut() {
        changed |= transform_stmt(s);
    }
    changed
}

fn transform_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::For { var, iter, body } => {
            // nested loops first (an inner loop's own rewrite is
            // independent of the outer one)
            let mut changed = false;
            for b in body.iter_mut() {
                changed |= transform_stmt(b);
            }
            if let Some((start, end)) = seq_range_bounds(iter) {
                if !stmts_write_var(body, var) {
                    *iter = IrExpr::Array(vec![IrExpr::Range { start, end }]);
                    changed = true;
                }
            }
            changed
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Function { body, .. } => {
            let mut changed = false;
            for b in body.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed
        }
        IrStmt::If {
            then, elsifs, else_, ..
        } => {
            let mut changed = false;
            for b in then.iter_mut().chain(else_.iter_mut()) {
                changed |= transform_stmt(b);
            }
            for (_, b) in elsifs.iter_mut() {
                for s in b.iter_mut() {
                    changed |= transform_stmt(s);
                }
            }
            changed
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                for b in stage.iter_mut() {
                    changed |= transform_stmt(b);
                }
            }
            changed
        }
        IrStmt::Redirect { inner, .. } => {
            let mut changed = false;
            for b in inner.iter_mut() {
                changed |= transform_stmt(b);
            }
            changed
        }
        IrStmt::Case { clauses, .. } => {
            let mut changed = false;
            for c in clauses.iter_mut() {
                for b in c.body.iter_mut() {
                    changed |= transform_stmt(b);
                }
            }
            changed
        }
        IrStmt::Expr(e) => transform_expr_stmt(e),
        IrStmt::Output { value, .. } => transform_expr_stmt(value),
        // a capture assign's RHS carries the `$(for … done)` arrow
        IrStmt::Assign { expr, .. } => transform_expr_stmt(expr),
        _ => false,
    }
}

/// Walk a for-loop-bearing expression: for-loops hide inside the arrows
/// of command substitutions, pipeline stages, redirect/background bodies,
/// and capture assignments — the `IrStmt::Expr(Call(…Arrow…))` and
/// `Assign { expr: Call(…Arrow…) }` shapes. The emitter always has a
/// correct fallback for a Range iterable (bounded materialization), so a
/// loop in an async region transforms safely too.
fn transform_expr_stmt(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Call { args, .. } => {
            let mut changed = false;
            for a in args.iter_mut() {
                changed |= transform_expr(a);
            }
            changed
        }
        IrExpr::Array(items) => {
            let mut changed = false;
            for a in items.iter_mut() {
                changed |= transform_expr(a);
            }
            changed
        }
        _ => false,
    }
}

fn transform_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arrow(stmts) => {
            let mut changed = false;
            for s in stmts.iter_mut() {
                changed |= transform_stmt(s);
            }
            changed
        }
        IrExpr::Call { args, .. } => {
            let mut changed = false;
            for a in args.iter_mut() {
                changed |= transform_expr(a);
            }
            changed
        }
        IrExpr::Array(items) => {
            let mut changed = false;
            for a in items.iter_mut() {
                changed |= transform_expr(a);
            }
            changed
        }
        IrExpr::Object(props) => {
            let mut changed = false;
            for (_, v) in props.iter_mut() {
                changed |= transform_expr(v);
            }
            changed
        }
        _ => false,
    }
}

/// The `[lo, hi]` integer bounds when `iter` is the `$(seq …)` capture
/// shape: `Array([captureWords(Arrow([Expr(exec("seq", args))]))])` — the
/// `for i in $(seq …)` lowering (for-items array wrapping a single
/// command-substitution item) — or the bare call. Nothing else matches.
fn seq_range_bounds(iter: &IrExpr) -> Option<(i64, i64)> {
    let call = match iter {
        IrExpr::Array(items) => match items.as_slice() {
            [IrExpr::Call { .. }] => &items[0],
            _ => return None,
        },
        IrExpr::Call { .. } => iter,
        _ => return None,
    };
    let IrExpr::Call { func, args } = call else { return None };
    if func != "captureWords" {
        return None;
    }
    // exactly one statement in the capture: `exec("seq", args)`
    let [IrExpr::Arrow(stmts)] = args.as_slice() else { return None };
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = stmts.as_slice() else {
        return None;
    };
    if f2 != "exec" {
        return None;
    }
    // bare exec: name + arg array only (env/redirects disqualify)
    let [IrExpr::Str(name, _), IrExpr::Array(seq_args)] = a2.as_slice() else {
        return None;
    };
    if name != "seq" {
        return None;
    }
    let vals: Vec<i64> = seq_args.iter().map(seq_arg_int).collect::<Option<_>>()?;
    match vals.as_slice() {
        // `seq LAST` — GNU seq starts at 1
        [last] => bounds(1, *last),
        // `seq FIRST LAST` — default step 1
        [first, last] => bounds(*first, *last),
        // 3-arg step forms (`seq A S B`), flags, >3 args: keep the
        // runtime path (a step would need a stride the Range node lacks)
        _ => None,
    }
}

fn bounds(start: i64, end: i64) -> Option<(i64, i64)> {
    let span = end.abs_diff(start);
    (span <= MAX_SPAN as u64).then_some((start, end))
}

/// A seq argument must be a plain integer literal: no floats (locale
/// formatting), no leading zeros (GNU seq pads `01 02 …`; bash
/// arithmetic reads `010` as OCTAL 8), no flags, within double-precision
/// exactness.
fn seq_arg_int(a: &IrExpr) -> Option<i64> {
    let s = match a {
        IrExpr::Str(s, _) => s.as_str(),
        IrExpr::Int(i) => {
            return (i.unsigned_abs() <= MAX_SAFE_INT as u64).then_some(*i);
        }
        _ => return None,
    };
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    if s.starts_with('-') && s.len() > 2 && s.starts_with("-0") {
        return None; // -01: same octal/padding concern
    }
    let v: i64 = s.parse().ok()?;
    (v.unsigned_abs() <= MAX_SAFE_INT as u64).then_some(v)
}

// ── body-write scan: a counter loop's `i++` re-reads the binding, so a
// body that writes the loop var would derail the sequence (the materialized
// word list re-iterates regardless — bash semantics). Conservative: any
// assignment / store write / store-writing builtin mentioning the var.

fn stmts_write_var(stmts: &[IrStmt], var: &str) -> bool {
    stmts.iter().any(|s| stmt_writes_var(s, var))
}

fn stmt_writes_var(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => {
            targets.iter().any(|t| t.var == var && t.indices.is_empty())
        }
        IrStmt::Declare { vars, .. } => vars.iter().any(|d| d.name == var),
        IrStmt::DeclareArray { var: name, .. } => name == var,
        IrStmt::Expr(e) => expr_writes_var(e, var),
        IrStmt::If {
            then, elsifs, else_, ..
        } => {
            stmts_write_var(then, var)
                || stmts_write_var(else_, var)
                || elsifs.iter().any(|(_, b)| stmts_write_var(b, var))
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Function { body, .. } => stmts_write_var(body, var),
        // a nested `for var in …` REASSIGNS var in bash (the inner
        // iteration clobbers the outer binding until the outer body
        // ends) — count the loop-var binding itself as a write
        IrStmt::For { var: v, body, .. } => {
            v == var || stmts_write_var(body, var)
        }
        IrStmt::Case { clauses, .. } => clauses.iter().any(|c| stmts_write_var(&c.body, var)),
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| stmts_write_var(s, var)),
        IrStmt::Redirect { inner, .. } => stmts_write_var(inner, var),
        _ => false,
    }
}

fn expr_writes_var(e: &IrExpr, var: &str) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            // store write: sh2.setVar("var", …)
            if func == "setVar" {
                if let [IrExpr::Str(n, _), ..] = args.as_slice() {
                    if n == var {
                        return true;
                    }
                }
            }
            // store-writing builtins: `read i`, `unset i`, `declare -i i`,
            // `let i=i+1`, `eval 'i=…'` — the runtime writes the named
            // vars into the store. Conservative: the var name appearing
            // as an exact arg (`read i`) or as an identifier inside a
            // `let`/`eval` expression (`let i=i+1`) counts.
            if func == "exec" || func == "builtin" {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if matches!(
                        name.as_str(),
                        "read" | "readarray" | "mapfile" | "unset" | "let" | "eval"
                            | "declare" | "typeset" | "local"
                    ) {
                        if let Some(IrExpr::Array(wargs)) = args.get(1) {
                            for a in wargs {
                                if let IrExpr::Str(w, _) = a {
                                    if name == "let" || name == "eval" {
                                        if contains_ident(w, var) {
                                            return true;
                                        }
                                    } else if w == var {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            args.iter().any(|a| expr_writes_var(a, var))
        }
        IrExpr::Arrow(stmts) => stmts_write_var(stmts, var),
        _ => false,
    }
}

/// Does `s` contain `var` as a standalone identifier (word-boundary
/// delimited)? For `let 'i=i+1'` / `eval 'i=$x'` arg strings.
fn contains_ident(s: &str, var: &str) -> bool {
    if var.is_empty() || !var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if &s[start..i] == var {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq_for(args: &[&str], body: Vec<IrStmt>) -> IrStmt {
        let seq_args: Vec<IrExpr> = args
            .iter()
            .map(|a| IrExpr::Str(a.to_string(), crate::ir::StrStyle::DoubleQuoted))
            .collect();
        IrStmt::For {
            var: "i".to_string(),
            iter: IrExpr::Array(vec![IrExpr::Call {
                func: "captureWords".to_string(),
                args: vec![IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".to_string(),
                    args: vec![
                        IrExpr::Str("seq".to_string(), crate::ir::StrStyle::DoubleQuoted),
                        IrExpr::Array(seq_args),
                    ],
                })])],
            }]),
            body,
        }
    }

    #[test]
    fn two_arg_seq_lifts_to_range() {
        let mut st = seq_for(&["1", "10000"], vec![]);
        assert!(transform_stmt(&mut st));
        match st {
            IrStmt::For { iter, .. } => match iter {
                IrExpr::Array(items) => match items.as_slice() {
                    [IrExpr::Range { start, end }] => {
                        assert_eq!((*start, *end), (1, 10000))
                    }
                    _ => panic!("expected Range item"),
                },
                _ => panic!("expected Array iterable"),
            },
            _ => panic!("expected For"),
        }
    }

    #[test]
    fn one_arg_seq_starts_at_one() {
        let mut st = seq_for(&["10"], vec![]);
        assert!(transform_stmt(&mut st));
        let IrStmt::For { iter, .. } = st else { panic!() };
        let IrExpr::Array(items) = iter else { panic!() };
        let IrExpr::Range { start, end } = &items[0] else { panic!() };
        assert_eq!((*start, *end), (1, 10));
    }

    #[test]
    fn descending_seq_is_an_empty_range() {
        // GNU seq 5 1 prints nothing → zero iterations; Range{5,1} with a
        // `i <= end` test also runs zero times.
        let mut st = seq_for(&["5", "1"], vec![]);
        assert!(transform_stmt(&mut st));
        let IrStmt::For { iter, .. } = st else { panic!() };
        let IrExpr::Array(items) = iter else { panic!() };
        let IrExpr::Range { start, end } = &items[0] else { panic!() };
        assert_eq!((*start, *end), (5, 1));
    }

    #[test]
    fn step_three_arg_seq_keeps_runtime_path() {
        let mut st = seq_for(&["1", "2", "10"], vec![]);
        assert!(!transform_stmt(&mut st));
    }

    #[test]
    fn float_and_leading_zero_args_keep_runtime_path() {
        assert!(!transform_stmt(&mut seq_for(&["1", "0.5"], vec![])));
        assert!(!transform_stmt(&mut seq_for(&["01", "10"], vec![])));
        assert!(!transform_stmt(&mut seq_for(&["-01", "10"], vec![])));
    }

    #[test]
    fn body_write_disqualifies() {
        // for i in $(seq 1 3); do i=99; done — a counter loop's i++
        // would read the body-written 99 and derail; the word list
        // re-iterates regardless.
        let body = vec![IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: "i".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Int(99),
        }];
        let mut st = seq_for(&["1", "3"], body);
        assert!(!transform_stmt(&mut st));
    }

    #[test]
    fn setvar_write_disqualifies() {
        let body = vec![IrStmt::Expr(IrExpr::Call {
            func: "setVar".to_string(),
            args: vec![
                IrExpr::Str("i".to_string(), crate::ir::StrStyle::SingleQuoted),
                IrExpr::Int(0),
            ],
        })];
        let mut st = seq_for(&["1", "3"], body);
        assert!(!transform_stmt(&mut st));
    }

    #[test]
    fn huge_range_disqualifies() {
        assert!(!transform_stmt(&mut seq_for(&["1", "2000000"], vec![])));
        assert!(!transform_stmt(&mut seq_for(&["9007199254740993"], vec![])));
    }
}
