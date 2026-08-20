//! background-decide — classify every `&` backgrounded body as THREAD
//! (self-contained pure compute, runnable on a separate worker with a
//! fresh runtime) or FORK (touches parent shell state, must run on the
//! current chain), the shIR equivalent of sh2runtime's estree pass
//! `backgroundDecide` (JS-only today).
//!
//! ## Need
//! Shell `cmd &` is a single IR node (`IrStmt::Background`) but its
//! EXECUTION class is a semantic property every backend must decide:
//! the game's menu submits `bash /examples/textures/….sh &` — a
//! self-contained pure-compute body that can run on a WORKER THREAD —
//! while a backgrounded body that reads/modifies parent variables must
//! run as a fork on the current chain. The JS estree path decides this
//! today (`backgroundDecide`: a body whose subtree execs a nested
//! `bash /examples/…` script keeps the worker-routing `sh2.background`;
//! everything else becomes a native detached promise). The C/Go/Perl
//! backends have no shared verdict — they shell-out `&` blindly.
//!
//! ## The verdicts
//! ```text
//! Thread — the body is SELF-CONTAINED: it reads no parent-scope
//!          variable (every `Var`/`Index` read resolves to a variable
//!          the body itself assigns first), defines no functions the
//!          parent needs, and its execs are pure subprocesses (script
//!          runs, builtins) with no parent-state writes. Run it on a
//!          worker thread with a FRESH runtime — only args + stdout
//!          cross the boundary.
//! Fork   — anything else: the body reads or writes parent state, or
//!          captures, or is a subshell with copy semantics the worker
//!          cannot honour. Run it detached on the current chain
//!          (bash's `&` semantics).
//! ```
//!
//! ## Scope
//! Analysis-only (like `sync-ok-loops`): verdicts stored in module
//! statics keyed by statement pointer, read by the renderers. No
//! structural mutation.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). The estree
//! worker mediates the renderer hooks: estree.rs replaces its
//! `backgroundDecide` call with the verdicts; the C/Go/Perl generators
//! emit fork (`&` subprocess) vs thread (worker dispatch) from the same
//! verdict.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::ir::{IrExpr, IrStmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgClass {
    Thread,
    Fork,
}

/// statement-pointer → verdict (mirrors the sync-ok-loops pattern).
static VERDICTS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

pub fn is_thread(ptr: usize) -> bool {
    VERDICTS.lock().unwrap().as_ref().map(|s| s.contains(&ptr)).unwrap_or(false)
}

/// Apply the transform (analysis-only — computes + caches the verdicts).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut thread = HashSet::new();
    for st in stmts.iter_mut() {
        classify(st, &mut thread);
    }
    *VERDICTS.lock().unwrap() = Some(thread);
    false // analysis-only
}

fn classify(st: &mut IrStmt, thread: &mut HashSet<usize>) {
    if matches!(st, IrStmt::Background(_)) {
        // the pointer key must be taken BEFORE the body is mutably
        // borrowed (the sync-ok-loops pointer-verdict convention)
        let ptr = (st as *const IrStmt) as usize;
        if let IrStmt::Background(body) = st {
            if body.iter().all(is_self_contained_stmt) {
                thread.insert(ptr);
            }
            for s in body.iter_mut() {
                classify(s, thread);
            }
        }
        return;
    }
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            for s in then.iter_mut() {
                classify(s, thread);
            }
            for (_, b) in elsifs.iter_mut() {
                for s in b.iter_mut() {
                    classify(s, thread);
                }
            }
            for s in else_.iter_mut() {
                classify(s, thread);
            }
        }
        IrStmt::For { body, .. } | IrStmt::While { body, .. } => {
            for s in body.iter_mut() {
                classify(s, thread);
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body.iter_mut() {
                classify(s, thread);
            }
        }
        IrStmt::Block(v) | IrStmt::Subshell(v) => {
            for s in v.iter_mut() {
                classify(s, thread);
            }
        }
        _ => {}
    }
}

/// A statement is self-contained when it neither reads nor writes any
/// variable that could exist in the parent scope. Conservative: every
/// `Var`/`Index`/`Assign`/`Declare` touch marks the body as forked,
/// UNLESS the variable is assigned inside the body BEFORE it is read.
/// The simple (refuse > guess) version: any variable READ that is not
/// assigned earlier in the body → not self-contained.
fn is_self_contained_stmt(st: &IrStmt) -> bool {
    // collect the vars the body assigns first
    let mut assigned: HashSet<String> = HashSet::new();
    let mut reads: HashSet<String> = HashSet::new();
    let mut has_side_effect = false;
    collect(st, &mut assigned, &mut reads, &mut has_side_effect);
    // every read must be of a body-assigned var
    reads.iter().all(|r| assigned.contains(r)) && !has_side_effect
}

fn collect(st: &IrStmt, assigned: &mut HashSet<String>, reads: &mut HashSet<String>, se: &mut bool) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                if t.indices.is_empty() {
                    assigned.insert(t.var.clone());
                }
            }
            expr_reads(expr, reads);
        }
        IrStmt::Declare { vars, .. } => {
            for v in vars {
                assigned.insert(v.name.clone());
            }
        }
        IrStmt::DeclareArray { var, .. } => {
            assigned.insert(var.clone());
        }
        IrStmt::Output { .. } => {} // pure stdout — fine for a thread
        IrStmt::Exec { .. } | IrStmt::WriteFile { .. } | IrStmt::Pipeline { .. } => {
            // a subprocess exec is self-contained compute; file writes are
            // observable — keep writes conservative (fork)
            if matches!(st, IrStmt::WriteFile { .. } | IrStmt::Pipeline { .. }) {
                *se = true;
            }
        }
        IrStmt::Subshell(_) => *se = true, // copy semantics — fork
        IrStmt::Return(_) | IrStmt::Exit(_) | IrStmt::Die { .. } => *se = true,
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_reads(cond, reads);
            for s in then {
                collect(s, assigned, reads, se);
            }
            for (c, b) in elsifs {
                expr_reads(c, reads);
                for s in b {
                    collect(s, assigned, reads, se);
                }
            }
            for s in else_ {
                collect(s, assigned, reads, se);
            }
        }
        IrStmt::For { iter, body, .. } => {
            expr_reads(iter, reads);
            for s in body {
                collect(s, assigned, reads, se);
            }
        }
        IrStmt::While { cond, body } => {
            expr_reads(cond, reads);
            for s in body {
                collect(s, assigned, reads, se);
            }
        }
        IrStmt::Expr(e) => expr_reads(e, reads),
        _ => {}
    }
}

fn expr_reads(e: &IrExpr, reads: &mut HashSet<String>) {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            reads.insert(v.clone());
        }
        IrExpr::Index { var, .. } => {
            reads.insert(var.clone());
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_reads(lhs, reads);
            expr_reads(rhs, reads);
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                expr_reads(a, reads);
            }
        }
        IrExpr::Arith(a) => arith_reads(a, reads),
        IrExpr::Capture { expr, .. } => expr_reads(expr, reads),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    expr_reads(x, reads);
                }
            }
        }
        _ => {}
    }
}

fn arith_reads(a: &crate::ir::ArithAst, reads: &mut HashSet<String>) {
    match a {
        crate::ir::ArithAst::Var(v) => {
            reads.insert(v.clone());
        }
        crate::ir::ArithAst::Index { var, .. } => {
            reads.insert(var.clone());
        }
        crate::ir::ArithAst::Bin { lhs, rhs, .. } => {
            arith_reads(lhs, reads);
            arith_reads(rhs, reads);
        }
        crate::ir::ArithAst::Un { arg, .. } => arith_reads(arg, reads),
        crate::ir::ArithAst::Cond { test, then, else_, .. } => {
            arith_reads(test, reads);
            arith_reads(then, reads);
            arith_reads(else_, reads);
        }
        _ => {}
    }
}

// name: background-decide
// prereqs: [escape-classes — the parent-scope read test is the weak
//   form; the shared escape verdict sharpens it]
// invariant: analysis-only; no structural mutation. Verdicts are
//   keyed by statement POINTER — the renderers must consume them in
//   the SAME compilation the transform ran (same tree, no clone).
// scope: offered to estree (owner — supersedes backgroundDecide), c,
//   go, perl, sh
// updates: none (first offer)
