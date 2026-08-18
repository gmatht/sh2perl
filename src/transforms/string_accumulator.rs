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
//!     blk_p=""
//!     while …; do
//!       blk_p="$blk_p$td_a $td_b $td_c 1 1 1 …\n"
//!       …
//!     done
//!     echo "$blk_p" > /dev/webgl/…
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
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). Analysis
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
            // init immediately before the loop, literal, and the var reads
            // only after the loop
            if li == 0 {
                continue;
            }
            if !init_is_literal(&stmts[li - 1], &var) {
                continue;
            }
            if reads_after_loop(&stmts[li + 1..], &var) {
                out.push(Accumulator { var, loop_idx: li, init_idx: li - 1 });
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

/// `v` assigned an Interpolate starting with `Var(v)`.
fn is_self_append(var: &str, expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Interpolate(parts) => match parts.first() {
            Some(InterpPart::Expr(e)) => matches!(e.as_ref(), IrExpr::Var(v, _) if v == var),
            _ => false,
        },
        _ => false,
    }
}

/// `stmts[li-1]` assigns `var` to a literal (the `blk_p=""` init).
fn init_is_literal(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            targets.iter().any(|t| t.var == var && t.indices.is_empty())
                && matches!(expr, IrExpr::Str(_, _))
        }
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
        IrExpr::Call { args, .. } => {
            for a in args {
                expr_read_walk(a, var, found);
            }
        }
        _ => {}
    }
}

