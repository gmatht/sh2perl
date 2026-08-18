//! unreachable-after-exit — drop straight-line statements that follow an
//! unconditional `Exit` / function `Return`.
//!
//! ## Need
//! Shell code (and generated/template code) commonly guards failures with
//! `exit 1` / `return 1` early-outs, and authors leave live-looking but
//! dead statements after them (or the frontend synthesizes them). Every
//! backend emits the dead statements — a compare-and-branch that can
//! never be taken. The shIR carries `IrStmt::Exit`/`IrStmt::Return`, so
//! the unreachable tail is removable at the IR level for all backends at
//! once.
//!
//! ## Scope — the sound version
//! Within a statement LIST (a body / top-level block / function body):
//!   - after a DIRECT `IrStmt::Exit(_)` (unconditional — not nested in an
//!     if/loop), every later statement in that list is dropped;
//!   - after a DIRECT `IrStmt::Return(_)` inside a FUNCTION body, the rest
//!     of that body is dropped;
//!   - an `Exit`/`Return` nested inside an `If`/`While`/`Case` is
//!     CONDITIONAL — it does NOT deaden the statements after the
//!     enclosing statement (the recursion handles the branch, then the
//!     enclosing list continues normally).
//! Only a top-of-block unconditional exit prunes. Anything ambiguous is
//! left alone.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). Complements
//! dead-store-elim and const-condition-elim (different unreachable
//! classes).

use crate::ir::{IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let (out, changed) = pass(stmts.drain(..).collect(), false);
    *stmts = out;
    changed
}

fn pass(stmts: Vec<IrStmt>, in_fn: bool) -> (Vec<IrStmt>, bool) {
    let mut out: Vec<IrStmt> = Vec::new();
    let mut changed = false;
    let mut dead = false;
    for st in stmts {
        if dead {
            changed = true;
            continue;
        }
        // unconditional exit prunes the rest of this list
        if matches!(st, IrStmt::Exit(_)) || (in_fn && matches!(st, IrStmt::Return(_))) {
            out.push(st);
            dead = true;
            continue;
        }
        // recurse into nested bodies (never outside a function for Return)
        match st {
            IrStmt::If { cond, then, elsifs, else_ } => {
                let (t2, c1) = pass(then, in_fn);
                let mut eds = Vec::new();
                let mut c2 = c1;
                for (c, b) in elsifs {
                    let (b2, cb) = pass(b, in_fn);
                    c2 |= cb;
                    eds.push((c, b2));
                }
                let (e2, c3) = pass(else_, in_fn);
                changed |= c2 | c3;
                out.push(IrStmt::If { cond, then: t2, elsifs: eds, else_: e2 });
            }
            IrStmt::For { var, iter, body } => {
                let (b2, c) = pass(body, in_fn);
                changed |= c;
                out.push(IrStmt::For { var, iter, body: b2 });
            }
            IrStmt::While { cond, body } => {
                let (b2, c) = pass(body, in_fn);
                changed |= c;
                out.push(IrStmt::While { cond, body: b2 });
            }
            IrStmt::DoWhile { body, cond, until } => {
                let (b2, c) = pass(body, in_fn);
                changed |= c;
                out.push(IrStmt::DoWhile { body: b2, cond, until });
            }
            IrStmt::Function { name, body, .. } => {
                let (b2, c) = pass(body, true);
                changed |= c;
                out.push(IrStmt::Function { name, body: b2, named_blocks: Vec::new() });
            }
            IrStmt::Block(v) => {
                let (b2, c) = pass(v, in_fn);
                changed |= c;
                out.push(IrStmt::Block(b2));
            }
            IrStmt::Subshell(v) => {
                let (b2, c) = pass(v, in_fn);
                changed |= c;
                out.push(IrStmt::Subshell(b2));
            }
            IrStmt::Background(v) => {
                let (b2, c) = pass(v, in_fn);
                changed |= c;
                out.push(IrStmt::Background(b2));
            }
            IrStmt::Case { discriminant, clauses } => {
                let mut cl2 = Vec::new();
                let mut cc = false;
                for mut clause in clauses {
                    let (b2, c) = pass(clause.body, in_fn);
                    cc |= c;
                    clause.body = b2;
                    cl2.push(clause);
                }
                changed |= cc;
                out.push(IrStmt::Case { discriminant, clauses: cl2 });
            }
            other => out.push(other),
        }
    }
    (out, changed)
}

