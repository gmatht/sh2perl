//! string-accumulator — identify the `v="$v$seg"` self-append pattern in
//! a loop and mark it the renderers can lower to a chunk-array + join
//! instead of re-concatenating a growing string, the shIR equivalent of
//! the stalled proposal estree-20260813-184909 (string-accumulator).
//!
//! ## Need
//! The game's render path builds the block list this way (4 A1
//! self-append sites; the two in `try_draw`/`draw_block` run 768×/
//! frame):
//!
//! ```text
//! blk_p=""
//! while …; do
//!   blk_p="$blk_p$td_a $td_b $td_c 1 1 1 …\n"
//!   …
//! done
//! echo "$blk_p" > /dev/webgl/…
//! ```
//!
//! The emitted code re-evaluates a growing template literal per append
//! (`blk_p = \`${blk_p}${td_a} …\n\``) — ~1500 appends/frame growing to
//! ~30KB. Analysis-only (like `sync-ok-loops`): the transform marks the
//! accumulator and the renderer lowers it — `__acc_v.push(seg)` per
//! append and ONE `__acc_v.join("")` per post-loop read.
//!
//! ## The verdict
//! A scalar var `v` is an ACCUMULATOR when ALL of:
//!   - it is initialized to a literal (usually `""`) before a loop,
//!   - every write inside the loop is a SELF-APPEND: `v` assigned an
//!     `Interpolate` that begins with `Var(v)` (the `"$v$seg"` shape),
//!   - `v` is not written outside the loop after the init, and
//!   - every read of `v` is AFTER the loop (its final value), except the
//!     self-append's own leading read.
//! The transform stores the verdict (var, the init statement index, the
//! loop statement index, the post-loop read indexes) in module statics;
//! the renderers emit the push/join lowering. `newline`/interpolation of
//! each `seg` is preserved exactly.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Analysis
//! only, no structural mutation. Renderer hooks: estree.rs (the shrinking
//! `blk_p` cons-chain), and any backend with a mutable string builder.

use std::sync::Mutex;

use crate::ir::{InterpPart, IrExpr, IrStmt};

#[derive(Debug, Clone, PartialEq)]
pub struct Accumulator {
    pub var: String,
    /// index into the top-level statement list of the loop stmt
    pub loop_idx: usize,
    /// index of the init statement (before the loop)
    pub init_idx: usize,
}

static VERDICTS: Mutex<Option<Vec<Accumulator>>> = Mutex::new(None);

/// Drop the cached verdict — the out-of-band caller's reset (see
/// `transforms::reset_verdicts`). These transforms are analysis-only: they
/// return `false` from `run()` and a RENDERER reads the verdict, so without
/// a reset a second run observes the PREVIOUS input's verdict.
pub fn clear_verdicts() {
    *VERDICTS.lock().unwrap() = None;
}

pub fn accumulators() -> Vec<Accumulator> {
    VERDICTS.lock().unwrap().clone().unwrap_or_default()
}

/// Apply the transform (analysis-only — computes + caches the verdicts).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut out = Vec::new();
    for (li, st) in stmts.iter().enumerate() {
        let body = match st {
            IrStmt::While { body, .. } | IrStmt::For { body, .. } => body,
            _ => continue,
        };
        for var in self_append_vars(body) {
            // The init is the LAST statement before the loop that writes
            // `var` — NOT necessarily `stmts[li - 1]`. The canonical shape
            // interleaves other initialisers (`blk=""; i=0; while …`), so
            // requiring adjacency (as this analysis did) missed the very
            // case the request is about. Scanning back for the last write
            // keeps the "no intervening write" guarantee: a later
            // non-literal rebind is found FIRST and refused by the literal
            // check below.
            let Some(ii) = (0..li).rev().find(|&k| writes_var(&stmts[k], &var)) else {
                continue;
            };
            if !init_is_literal(&stmts[ii], &var) {
                continue;
            }
            // A mid-loop read observes the growing string — the deferred
            // push/join lowering would change its value. Require the body
            // to read the accumulator only as the self-append's leader.
            if body_reads_var_nonleading(body, &var) {
                continue;
            }
            if reads_after_loop(&stmts[li + 1..], &var) {
                out.push(Accumulator { var, loop_idx: li, init_idx: ii });
            }
        }
    }
    *VERDICTS.lock().unwrap() = Some(out);
    false // analysis-only
}

/// The vars whose ONLY writes in this body are self-appends.
fn self_append_vars(body: &[IrStmt]) -> Vec<String> {
    let mut writes: Vec<String> = Vec::new();
    let mut bad: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in body {
        if let IrStmt::Assign { targets, expr, .. } = s {
            for t in targets {
                if t.indices.is_empty() && !writes.contains(&t.var) {
                    writes.push(t.var.clone());
                }
                if !is_self_append(t.var.as_str(), expr) {
                    bad.insert(t.var.clone());
                }
            }
        } else {
            // a non-assign statement reads/writes unknown things — mark
            // everything crystal-clear only (conservative: bail on any
            // other statement in the body)
            return Vec::new();
        }
    }
    writes.into_iter().filter(|v| !bad.contains(v)).collect()
}

/// Is this expression a READ of `var`? The real A1 carries reads three ways:
/// a bare `Var`/`Ident`, a `getVar("v")` call, or `param("", "v")` — the
/// last two hold the name as a STRING argument, so a walk that only matches
/// `IrExpr::Var` sees nothing at all (`echo "$blk"` lowers to
/// `getVar("blk")`).
fn is_var_read(e: &IrExpr, var: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == var,
        IrExpr::Call { func, args } => {
            let idx = if func == "param" { 1 } else { 0 };
            matches!(func.as_str(), "getVar" | "param")
                && matches!(args.get(idx), Some(IrExpr::Str(n, _)) if n == var)
        }
        _ => false,
    }
}

/// `v` assigned an Interpolate starting with a read of `v` (the `"$v$seg"`
/// shape).
fn is_self_append(var: &str, expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Interpolate(parts) => match parts.first() {
            Some(InterpPart::Expr(e)) => is_var_read(e, var),
            _ => false,
        },
        _ => false,
    }
}

/// Does this statement WRITE `var`? (assign target / declare / setVar)
fn writes_var(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => {
            targets.iter().any(|t| t.var == var && t.indices.is_empty())
        }
        IrStmt::Declare { vars, .. } => vars.iter().any(|d| d.name == var),
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "setVar" => {
            matches!(args.first(), Some(IrExpr::Str(n, _)) if n == var)
        }
        _ => false,
    }
}

/// Assigned to a CONSTANT string (the `blk_p=""` init). The parser lowers
/// `v=""` to `Interpolate([Lit("")])`, not `Str`, so both forms qualify —
/// an interpolation qualifies only when every part is a literal (no read at
/// all, so seeding a chunk array with it cannot change evaluation order).
fn init_is_literal(st: &IrStmt, var: &str) -> bool {
    let expr = match st {
        IrStmt::Assign { targets, expr, .. }
            if targets.iter().any(|t| t.var == var && t.indices.is_empty()) =>
        {
            expr
        }
        IrStmt::Declare { vars, init: Some(init), .. }
            if vars.iter().any(|d| d.name == var) =>
        {
            init
        }
        _ => return false,
    };
    match expr {
        IrExpr::Str(_, _) => true,
        IrExpr::Interpolate(parts) => parts.iter().all(|p| matches!(p, InterpPart::Lit(_))),
        _ => false,
    }
}

/// `var` is read after the loop (its final value is observed) — this is
/// what makes the join-at-read lowering valid (there are no mid-loop
/// reads besides the self-append's own leading `Var(v)`).
fn reads_after_loop(after: &[IrStmt], var: &str) -> bool {
    let mut found = false;
    for st in after {
        read_walk(st, var, &mut found);
    }
    found
}

fn read_walk(st: &IrStmt, var: &str, found: &mut bool) {
    if *found {
        return;
    }
    match st {
        IrStmt::Output { value, .. } => expr_read_walk(value, var, found),
        IrStmt::Assign { expr, .. } => expr_read_walk(expr, var, found),
        IrStmt::Exec { cmd, args, .. } => {
            expr_read_walk(cmd, var, found);
            for a in args {
                expr_read_walk(a, var, found);
            }
        }
        IrStmt::WriteFile { path, content, .. } => {
            expr_read_walk(path, var, found);
            expr_read_walk(content, var, found);
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_read_walk(cond, var, found);
            for s in then {
                read_walk(s, var, found);
            }
            for (c, b) in elsifs {
                expr_read_walk(c, var, found);
                for s in b {
                    read_walk(s, var, found);
                }
            }
            for s in else_ {
                read_walk(s, var, found);
            }
        }
        IrStmt::While { cond, body } => {
            expr_read_walk(cond, var, found);
            for s in body {
                read_walk(s, var, found);
            }
        }
        IrStmt::For { iter, body, .. } => {
            expr_read_walk(iter, var, found);
            for s in body {
                read_walk(s, var, found);
            }
        }
        IrStmt::Expr(e) => expr_read_walk(e, var, found),
        _ => {}
    }
}

fn expr_read_walk(e: &IrExpr, var: &str, found: &mut bool) {
    if *found {
        return;
    }
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            if v == var {
                *found = true;
            }
        }
        IrExpr::Index { var: v, key } => {
            if v == var {
                *found = true;
            }
            expr_read_walk(key, var, found);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_read_walk(lhs, var, found);
            expr_read_walk(rhs, var, found);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    expr_read_walk(x, var, found);
                }
            }
        }
        // A command's argv / an array literal / an object value carries the
        // operand in a CHILD list — walking only scalar operands missed
        // `echo "$blk"` (its read sits inside the `builtin` argv Array), so
        // the verdict never fired on the canonical shape.
        IrExpr::Array(items) => {
            for i in items {
                expr_read_walk(i, var, found);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props {
                expr_read_walk(v, var, found);
            }
        }
        IrExpr::Splice(inner) => expr_read_walk(inner, var, found),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_read_walk(cond, var, found);
            expr_read_walk(then, var, found);
            expr_read_walk(else_, var, found);
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_read_walk(expr, var, found);
            expr_read_walk(default, var, found);
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_read_walk(obj, var, found);
            for a in args {
                expr_read_walk(a, var, found);
            }
        }
        IrExpr::Call { func, args } => {
            // `getVar("v")` / `param("", "v")` — the name is a STRING arg,
            // so the operand walk below cannot see the reference.
            let idx = if func == "param" { 1 } else { 0 };
            if matches!(func.as_str(), "getVar" | "param")
                && matches!(args.get(idx), Some(IrExpr::Str(n, _)) if n == var)
            {
                *found = true;
            }
            for a in args {
                expr_read_walk(a, var, found);
            }
        }
        _ => {}
    }
}

/// Is `var` read ANYWHERE in the loop body outside a self-append's leading
/// self-read? A mid-loop read observes the growing string, so deferring the
/// concatenation to a `join` at the post-loop read sites would change its
/// value — the lowering must be refused.
fn body_reads_var_nonleading(body: &[IrStmt], var: &str) -> bool {
    let mut found = false;
    for s in body {
        match s {
            IrStmt::Assign { targets, expr, .. }
                if targets.iter().any(|t| t.var == var) && is_self_append(var, expr) =>
            {
                if let IrExpr::Interpolate(parts) = expr {
                    for p in parts.iter().skip(1) {
                        if let InterpPart::Expr(e) = p {
                            expr_read_walk(e, var, &mut found);
                        }
                    }
                }
            }
            other => read_walk(other, var, &mut found),
        }
        if found {
            return true;
        }
    }
    false
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, IrExpr, IrStmt, StrStyle};

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

    fn get(var: &str) -> IrExpr {
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![IrExpr::Str(var.to_string(), StrStyle::DoubleQuoted)],
        }
    }

    fn echo(var: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![get(var)]),
            ],
        })
    }

    /// The request's own repro — `blk=""; i=0; while …; done; echo "$blk"` —
    /// must be recognized. Every shape here is the one the REAL A1 carries:
    /// the init is an `Interpolate` of literals (not a bare `Str`), the
    /// self-read is `getVar("blk")` (not `Var`), the init is not adjacent to
    /// the loop, and the post-loop read is another `getVar`.
    #[test]
    fn recognizes_canonical_accumulator() {
        let init = assign(
            "blk",
            IrExpr::Interpolate(vec![InterpPart::Lit(String::new())]),
        );
        let i_init = assign("i", IrExpr::Str("0".into(), StrStyle::DoubleQuoted));
        let body = vec![
            assign(
                "blk",
                IrExpr::Interpolate(vec![
                    InterpPart::Expr(Box::new(get("blk"))),
                    InterpPart::Lit(" x ".into()),
                    InterpPart::Expr(Box::new(get("i"))),
                ]),
            ),
            assign("i", IrExpr::Str("1".into(), StrStyle::DoubleQuoted)),
        ];
        let mut stmts = vec![
            init,
            i_init,
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body,
            },
            echo("blk"),
        ];
        transform(&mut stmts);
        let acc = accumulators();
        assert_eq!(
            acc,
            vec![Accumulator {
                var: "blk".to_string(),
                loop_idx: 2,
                init_idx: 0,
            }],
            "canonical self-append accumulator must be recognized"
        );
    }

    /// A mid-loop read observes the growing string, so the verdict must be
    /// refused (a deferred join would return a different value).
    #[test]
    fn refuses_mid_loop_read() {
        let init = assign(
            "blk",
            IrExpr::Interpolate(vec![InterpPart::Lit(String::new())]),
        );
        let body = vec![
            echo("blk"),
            assign(
                "blk",
                IrExpr::Interpolate(vec![
                    InterpPart::Expr(Box::new(get("blk"))),
                    InterpPart::Lit("x".into()),
                ]),
            ),
        ];
        let mut stmts = vec![
            init,
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body,
            },
            echo("blk"),
        ];
        transform(&mut stmts);
        assert!(
            accumulators().is_empty(),
            "a mid-loop read must refuse the verdict"
        );
    }

    /// A non-literal re-init before the loop poisons the verdict.
    #[test]
    fn refuses_nonliteral_init() {
        let body = vec![assign(
            "blk",
            IrExpr::Interpolate(vec![
                InterpPart::Expr(Box::new(get("blk"))),
                InterpPart::Lit("x".into()),
            ]),
        )];
        let mut stmts = vec![
            assign("blk", get("seed")),
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body,
            },
            echo("blk"),
        ];
        transform(&mut stmts);
        assert!(accumulators().is_empty(), "non-literal init must refuse");
    }
}
