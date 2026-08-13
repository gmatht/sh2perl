//! `contains` lift family — the substring-test pattern.
//!
//! Recognises three shell idioms that all mean "does this string
//! contain that literal substring?": grep-test, case-glob, and
//! test-glob. Lowers all three to a single `sh2.contains(x, p)`
//! call, which every backend can render natively (`String.includes`
//! in JS, `index() != -1` in Perl, `str::contains` in Rust, etc.).
//!
//! The exemplar (PLAN §9.1, "Exemplars already landed") is
//! `if/while echo X | grep P >/dev/null 2>/dev/null` →
//! `String(X).includes(P)` — a 180× speedup on `sqrt1337.sh`
//! (10k-iter grep-in-loop) with byte-identical output.
//!
//! Stage 0: the `GrepTest` lift is the trait skeleton (every
//! `try_lift_*` returns `None`; the real recognition logic lands in
//! stage 1 by extracting `shir::try_lift_grep_contains` at
//! shir.rs:1895). The `CaseGlob` and `TestGlob` lifts follow.

use crate::ir::{IrExpr, IrStmt};

/// `if/while echo <arg> | grep <literal> >/dev/null 2>/dev/null`
///            ↓
/// `sh2.contains(arg, literal)`.
///
/// Statement/&&-position pipelines keep their status semantics; the
/// lift only fires in test position. Conservative: only plain literal
/// patterns free of BRE metacharacters (^ $ . [ ] * \), no grep flags,
/// echo with exactly one argument, both fds redirected to /dev/null,
/// exactly two pipeline stages.
pub struct GrepTest;

impl super::PatternLift for GrepTest {
    fn name(&self) -> &'static str {
        "grep_test_contains"
    }
    fn try_lift_expr(&self, _expr: &IrExpr) -> Option<IrExpr> {
        // Stage 0: stub. Stage 1: extract the body of
        // `shir::try_lift_grep_contains` (shir.rs:1895) verbatim.
        None
    }
}

/// `case "$x" in *P*)` → `sh2.contains(x, "P")`.
///
/// Conservative: only the `*P*)` glob form (prefix and suffix `*`
/// wildcards around a literal middle), no `|`-alternation, no
/// character classes, the `nocasematch` invariant must hold
/// (the analysis populates `ctx.case_nocase`; the lift refuses when
/// it is `true`).
pub struct CaseGlob;

impl super::PatternLift for CaseGlob {
    fn name(&self) -> &'static str {
        "case_glob_contains"
    }
    fn try_lift_stmt(&self, _stmt: &IrStmt) -> Option<IrStmt> {
        // Stage 0: stub. Stage 1: walk `IrStmt::Case` arms; the
        // single-pattern `*P*)` form lowers to
        // `sh2.contains(subject, P)`.
        None
    }
}

/// `[ "$x" = *P* ]` → `sh2.contains(x, "P")`.
///
/// The test-expression form of the same substring idea. Same
/// conservativeness rules as `CaseGlob`.
pub struct TestGlob;

impl super::PatternLift for TestGlob {
    fn name(&self) -> &'static str {
        "test_glob_contains"
    }
    fn try_lift_expr(&self, _expr: &IrExpr) -> Option<IrExpr> {
        // Stage 0: stub.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::PatternLift;
    use super::*;

    /// All three lifts must implement the trait without panicking,
    /// and their default `try_lift_*` must return `None` in stage 0
    /// (the corpus is the contract; unit tests pin the no-op shape).
    #[test]
    fn grep_test_default_is_no_op() {
        let lift = GrepTest;
        assert_eq!(lift.name(), "grep_test_contains");
        // The default `try_lift_stmt` returns None.
        let stmt = IrStmt::Expr(IrExpr::Int(0));
        assert!(lift.try_lift_stmt(&stmt).is_none());
        // `try_lift_expr` is the stage-0 stub.
        let expr = IrExpr::Int(0);
        assert!(lift.try_lift_expr(&expr).is_none());
    }

    #[test]
    fn case_glob_default_is_no_op() {
        let lift = CaseGlob;
        assert_eq!(lift.name(), "case_glob_contains");
        let stmt = IrStmt::Expr(IrExpr::Int(0));
        assert!(lift.try_lift_stmt(&stmt).is_none());
        // No `try_lift_expr` for the case form (case is a statement).
        let expr = IrExpr::Int(0);
        assert!(lift.try_lift_expr(&expr).is_none());
    }

    #[test]
    fn test_glob_default_is_no_op() {
        let lift = TestGlob;
        assert_eq!(lift.name(), "test_glob_contains");
        // The default `try_lift_stmt` returns None.
        let stmt = IrStmt::Expr(IrExpr::Int(0));
        assert!(lift.try_lift_stmt(&stmt).is_none());
    }
}
