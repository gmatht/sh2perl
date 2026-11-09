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
//! **Status (2026-11-08): OFFERED, still not consumed.** The analysis was
//! fixed before anything consumes it: `proofs/verus/background_decide.rs`
//! refutes the pre-2026-11-08 set-based check, and the ordered walk below
//! replaces it. The first wiring attempt (`rust_backend` skipping the
//! `__sh_cap` parent snapshot for a THREAD body) found a defect in this
//! module's own convention and was then withdrawn unverified: the verdicts
//! were keyed by statement POINTER, and `shir_to_rust` renders `prog.clone()`,
//! so the addresses could never match and the hook was inert for every body.
//! Verdicts are now keyed by the BODY TEXT (`is_thread_body`) — exact, and
//! stable across the clone — so a consumer can be added without this trap.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). The estree
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
static VERDICTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Drop the cached verdict — the out-of-band caller's reset (see
/// `transforms::reset_verdicts`). These transforms are analysis-only: they
/// return `false` from `run()` and a RENDERER reads the verdict, so without
/// a reset a second run observes the PREVIOUS input's verdict.
pub fn clear_verdicts() {
    *VERDICTS.lock().unwrap() = None;
}

/// The key a verdict is filed under: the body's own text.
///
/// NOT the statement pointer. `shir_to_rust` renders a CLONE of the program
/// (`let mut prog = prog.clone()`), so a pointer taken while the transform
/// walked the original can never match inside the renderer — the first wiring
/// attempt was inert for exactly that reason. The body text is what the
/// classification depends on, and two bodies with the same text get the same
/// verdict, so this key is clone-stable and exact (no hashing, no collisions).
fn body_key(body: &[IrStmt]) -> String {
    format!("{body:?}")
}

/// The verdict for one background body — the renderer-side entry point. False
/// whenever the transform has not run on an equivalent tree, i.e. the
/// conservative FORK default.
pub fn is_thread_body(body: &[IrStmt]) -> bool {
    VERDICTS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(&body_key(body)))
        .unwrap_or(false)
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

fn classify(st: &mut IrStmt, thread: &mut HashSet<String>) {
    if matches!(st, IrStmt::Background(_)) {
        if let IrStmt::Background(body) = st {
            if body_is_self_contained(body) {
                thread.insert(body_key(body));
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

/// Ordered self-contained check for one backgrounded body.
///
/// `defined` is the set of names the body has assigned on EVERY path that
/// reaches the current statement, so a read counts as reading body-local
/// state only when the name is in `defined` AT THAT POINT.
///
/// Comparing a statement's reads against its own total assign set — the
/// previous shape — ignores order and is unsound: `if [ -n "$x" ]; then x=1;
/// echo 7; fi` reads `$x` (the condition) before assigning it, so it was
/// classified THREAD — routed to a fresh runtime once a backend consumes the
/// verdict — which would take the else arm and print nothing where the
/// subshell prints 7. See
/// `proofs/verus/background_decide.rs` (`shipped_check_is_unsound` for the
/// refutation, `intended_implies_shipped` for the proof that this ordered rule
/// loses no verdict the old one issued).
fn body_is_self_contained(body: &[IrStmt]) -> bool {
    let mut defined: HashSet<String> = HashSet::new();
    walk_self_contained(body, &mut defined)
}

/// Forward walk over a statement list; straight-line assignments extend
/// `defined`, control flow does not (a branch or a loop body may not run, so
/// nothing it assigns is defined afterwards — conservative, and the only
/// direction that is sound for a fresh runtime).
fn walk_self_contained(stmts: &[IrStmt], defined: &mut HashSet<String>) -> bool {
    for st in stmts {
        if !stmt_self_contained(st, defined) {
            return false;
        }
    }
    true
}

fn reads_defined(e: &IrExpr, defined: &HashSet<String>) -> bool {
    let mut reads: HashSet<String> = HashSet::new();
    expr_reads(e, &mut reads);
    reads.iter().all(|r| defined.contains(r))
}

fn stmt_self_contained(st: &IrStmt, defined: &mut HashSet<String>) -> bool {
    match st {
        // The RHS and the index keys are read BEFORE the target is assigned:
        // `x=$x` / `a[i]=1` read parent state even though they also write.
        IrStmt::Assign { targets, expr, .. } => {
            if !reads_defined(expr, defined) {
                return false;
            }
            for t in targets {
                for i in &t.indices {
                    if !reads_defined(i, defined) {
                        return false;
                    }
                }
            }
            for t in targets {
                if t.indices.is_empty() {
                    defined.insert(t.var.clone());
                }
            }
            true
        }
        IrStmt::Declare { vars, init, .. } => {
            if let Some(e) = init {
                if !reads_defined(e, defined) {
                    return false;
                }
            }
            for v in vars {
                defined.insert(v.name.clone());
            }
            true
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            for e in elements {
                if !reads_defined(e, defined) {
                    return false;
                }
            }
            defined.insert(var.clone());
            true
        }
        // `echo "$x"` — the read is the whole point of the check, and the
        // old collector dropped it (`Output { .. } => {}`). A `$fh` target
        // names a parent-opened descriptor, so it forks.
        IrStmt::Output { value, target, .. } => target.is_none() && reads_defined(value, defined),
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            if !reads_defined(cmd, defined) {
                return false;
            }
            for a in args {
                if !reads_defined(a, defined) {
                    return false;
                }
            }
            for r in redirects {
                if !reads_defined(r, defined) {
                    return false;
                }
            }
            env.iter().all(|(_, v)| reads_defined(v, defined))
        }
        IrStmt::Warn { expr, .. } => reads_defined(expr, defined),
        IrStmt::Expr(e) => reads_defined(e, defined),
        // pure control flow inside the body — no parent state touched
        IrStmt::Continue | IrStmt::Break => true,
        IrStmt::Block(v) => walk_self_contained(v, defined),
        IrStmt::If { cond, then, elsifs, else_ } => {
            if !reads_defined(cond, defined) {
                return false;
            }
            let mut ok = true;
            for s in then {
                let mut inner = defined.clone();
                ok &= stmt_self_contained(s, &mut inner);
            }
            for (c, b) in elsifs {
                let mut inner = defined.clone();
                ok &= reads_defined(c, &inner);
                for s in b {
                    ok &= stmt_self_contained(s, &mut inner);
                }
            }
            for s in else_ {
                let mut inner = defined.clone();
                ok &= stmt_self_contained(s, &mut inner);
            }
            ok
        }
        IrStmt::For { var, iter, body } => {
            if !reads_defined(iter, defined) {
                return false;
            }
            // the loop variable is body-local; nothing else survives the loop
            let mut inner = defined.clone();
            inner.insert(var.clone());
            walk_self_contained(body, &mut inner)
        }
        IrStmt::While { cond, body } => {
            if !reads_defined(cond, defined) {
                return false;
            }
            let mut inner = defined.clone();
            walk_self_contained(body, &mut inner)
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut inner = defined.clone();
            if !walk_self_contained(body, &mut inner) {
                return false;
            }
            reads_defined(cond, &inner)
        }
        // Observables a worker cannot honour: file writes, pipes, copy
        // semantics, fatal exits, function definitions, and anything the walk
        // does not understand (refuse > guess — an unmodelled node could read
        // or write parent state).
        IrStmt::WriteFile { .. }
        | IrStmt::Pipeline { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_)
        | IrStmt::Return(_)
        | IrStmt::Exit(_)
        | IrStmt::Die { .. }
        | IrStmt::Function { .. }
        | IrStmt::Try { .. }
        | IrStmt::Case { .. }
        | IrStmt::Select { .. }
        | IrStmt::Redirect { .. }
        | IrStmt::Asm { .. }
        | IrStmt::Shaderize { .. }
        | IrStmt::ForInit { .. }
        | IrStmt::SetChildError(_)
        | IrStmt::Goto(_)
        | IrStmt::Label(_)
        | IrStmt::RawText(_)
        | IrStmt::Require(_)
        | IrStmt::Ext(_) => false,
    }
}

fn expr_reads(e: &IrExpr, reads: &mut HashSet<String>) {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            reads.insert(v.clone());
        }
        IrExpr::Index { var, key } => {
            reads.insert(var.clone());
            expr_reads(key, reads);
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
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads(obj, reads);
            for a in args {
                expr_reads(a, reads);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads(cond, reads);
            expr_reads(then, reads);
            expr_reads(else_, reads);
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_reads(expr, reads);
            expr_reads(default, reads);
        }
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
//   keyed by the BODY TEXT (`body_key`), not by a statement pointer:
//   `shir_to_rust` and friends render a CLONE of the program, so a
//   pointer taken while the transform walked the original can never
//   match inside the renderer (the first wiring attempt was inert for
//   exactly that reason). A body the table does not hold is FORK —
//   including when the transform never ran, which is the default a
//   consumer gets from `is_thread_body`. Proof: VERUS.md /
//   proofs/verus/background_decide.rs (`classify_thread_sound`,
//   `route_thread_is_sound`, `unclassified_stays_fork`).
// scope: offered to estree (owner — supersedes backgroundDecide), c,
//   go, perl, sh
// updates: none (first offer)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::AssignTarget;

    fn v(n: &str) -> IrExpr {
        IrExpr::Var(n.to_string(), None)
    }
    fn set(t: &str, e: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget { var: t.to_string(), sigil: None, indices: vec![] }],
            expr: e,
            asm: None,
        }
    }
    fn out(e: IrExpr) -> IrStmt {
        IrStmt::Output { value: e, newline: true, target: None }
    }
    fn iff(c: IrExpr, then: Vec<IrStmt>) -> IrStmt {
        IrStmt::If { cond: c, then, elsifs: vec![], else_: vec![] }
    }

    /// The witness from `proofs/verus/background_decide.rs`: the condition
    /// reads `$x` before the branch assigns it, so the subshell inherits the
    /// parent value and prints 7 while a fresh runtime prints nothing.
    #[test]
    fn read_before_assign_in_one_statement_forks() {
        let body = vec![iff(v("x"), vec![set("x", IrExpr::Int(1)), out(IrExpr::Int(7))])];
        assert!(!body_is_self_contained(&body));
    }

    /// Order across statements counts the other way: an earlier assignment
    /// makes a later read body-local.
    #[test]
    fn assign_then_read_is_self_contained() {
        assert!(body_is_self_contained(&[set("x", IrExpr::Int(1)), out(v("x"))]));
    }

    /// `x=$x` reads before it writes.
    #[test]
    fn self_read_forks() {
        assert!(!body_is_self_contained(&[set("x", v("x"))]));
    }

    /// The read lives in the output value, which the old collector ignored.
    #[test]
    fn bare_parent_read_forks() {
        assert!(!body_is_self_contained(&[out(v("x"))]));
        assert!(!body_is_self_contained(&[set("x", IrExpr::Int(1)), out(v("y"))]));
    }

    /// A branch assignment does not escape the branch.
    #[test]
    fn branch_local_assign_does_not_escape() {
        assert!(!body_is_self_contained(&[
            iff(v("x"), vec![set("x", IrExpr::Int(1))]),
            out(v("x")),
        ]));
    }

    #[test]
    fn literal_output_is_self_contained() {
        assert!(body_is_self_contained(&[out(IrExpr::Int(7))]));
        assert!(body_is_self_contained(&[]));
    }
}

#[cfg(test)]
mod verdict_tests {
    use super::*;
    use crate::ir::AssignTarget;

    fn v(n: &str) -> IrExpr {
        IrExpr::Var(n.to_string(), None)
    }
    fn set(t: &str, e: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget { var: t.to_string(), sigil: None, indices: vec![] }],
            expr: e,
            asm: None,
        }
    }
    fn out(e: IrExpr) -> IrStmt {
        IrStmt::Output { value: e, newline: true, target: None }
    }

    /// The verdict is reachable from the body alone (the renderer-side
    /// contract), it is FALSE for a body that reads parent state, and FALSE
    /// for a body the pass never saw — the conservative default that keeps a
    /// consumer safe when the transform has not run.
    #[test]
    fn verdict_is_keyed_by_the_body_and_defaults_to_fork() {
        let self_contained = vec![set("x", IrExpr::Int(1)), out(v("x"))];
        let reads_parent = vec![out(v("x"))];
        let unseen = vec![out(IrExpr::Int(7))];
        let mut stmts = vec![
            IrStmt::Background(self_contained.clone()),
            IrStmt::Background(reads_parent.clone()),
        ];
        transform(&mut stmts);
        assert!(is_thread_body(&self_contained));
        assert!(!is_thread_body(&reads_parent));
        assert!(!is_thread_body(&unseen), "an unclassifed body must stay FORK");
        clear_verdicts();
    }
}
