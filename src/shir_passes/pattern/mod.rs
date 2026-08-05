//! Pattern lifts — recognise common shell idioms and rewrite them to a
//! cheaper shIR shape (usually: replace a `sh2.exec(...)` + `sh2.pipeline(...)`
//! with a single `sh2.<idiom>(...)` call, or replace a runtime call with
//! a native expression).
//!
//! The corpus gate (`./fail-estree`) decides which lifts are sound; this
//! trait only declares the shape. The pattern family is the unit of
//! reuse — `grep -q P file`, `case $x in *P*)`, and `[ "$x" = *P* ]`
//! are all one family ("substring test") and one lift (`GrepTest` +
//! `CaseGlob` + `TestGlob`) lowers all three to `sh2.contains(x, p)`.
//!
//! The inventory here mirrors PLAN §9.1 ("the lowering ladder"). Stage
//! 0: the trait and module skeleton. Stage 1: extract the existing
//! `try_lift_grep_contains` from shir.rs:1895 into `contains.rs::GrepTest`,
//! the `whileLoopSync` lowering into `sync_loop.rs::WhileLoopSync`, etc.

pub mod contains;

use crate::ir::{IrExpr, IrStmt};

/// A pattern lift recognises a common shell idiom and rewrites it.
/// Conservative: only lifts when correctness can be proven on the
/// corpus. "A good idea that can't be proven on the corpus is dropped,
/// not force-fit." (PLAN §9.1 guardrail.)
pub trait PatternLift: Sync {
    fn name(&self) -> &'static str;
    /// Returns Some(replacement) if the lift applies to this statement
    /// (e.g. a `while` loop whose body is provably sync), None otherwise.
    fn try_lift_stmt(&self, _stmt: &IrStmt) -> Option<IrStmt> {
        None
    }
    /// Returns Some(replacement) if the lift applies to this expression
    /// (e.g. a `pipeline` whose stages match the grep-test shape),
    /// None otherwise.
    fn try_lift_expr(&self, _expr: &IrExpr) -> Option<IrExpr> {
        None
    }
}

/// Walk every expression in `prog` and apply `lift`. Returns the
/// number of expressions the lift rewrote (the metric diff is the
/// commit signal).
///
/// Stage 0: the walker is provided here so the trait has a default
/// integration path. Stage 1 wires the lifts into the pipeline (after
/// the transforms, before the renderer).
pub fn walk_exprs<F: FnMut(&IrExpr) -> Option<IrExpr>>(_prog: &crate::ir::IrProgram, _f: F) -> usize {
    // Stage 0: not wired. The trait's default `try_lift_expr` returns
    // `None` for every lift, so a stage-0 call always returns 0.
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrProgram, IrStmt};
    // Sanity: the submodule is wired in.
    #[test]
    fn contains_submodule_is_wired() {
        let _ = contains::GrepTest;
        let _ = contains::CaseGlob;
        let _ = contains::TestGlob;
    }

    /// The walker must be safe on an empty program and on a program
    /// with no sh2.* call sites. (The current stub returns 0 always.)
    #[test]
    fn walker_handles_empty_program() {
        let prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
        };
        let n = walk_exprs(&prog, |_| None);
        assert_eq!(n, 0);
    }

    /// The walker must be safe on a program whose statements are
    /// non-sh2 expressions (the lift's `try_lift_expr` is never
    /// consulted when no caller invokes it).
    #[test]
    fn walker_handles_non_liftable_program() {
        let prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::Expr(IrExpr::Int(42))],
            subs: vec![],
            var_types: vec![],
        };
        let n = walk_exprs(&prog, |_| Some(IrExpr::Int(0)));
        assert_eq!(n, 0);
    }
}
