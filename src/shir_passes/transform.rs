//! Transforms — IR-to-IR rewrites that preserve meaning under bash.
//!
//! Stage 0: stubs that pass the program through unchanged. Stage 1
//! migrates the real implementations:
//! - `ConstantFold` ← `ir::optimize_stmts` fold pass
//! - `DeadAssignmentElim` ← `ir::optimize_stmts` dead-assign pass
//! - `ImportMinimize` ← the M6 import registry (commit 2f70f9d, lives
//!   in `generator/mod.rs` today)
//!
//! The M3 guardrail (`pipeline_preserves_perl_backend` in the test
//! scaffold) is the proof that the migration is safe: the Perl output
//! must be byte-identical before and after the pipeline runs.

use crate::ir::IrProgram;
use crate::shir_passes::PassContext;

/// Fold provably-constant `$((...))` arithmetic and `Int BinOp` chains
/// to integer literals. The Rust evaluator lives in `ir::optimize_stmts`;
/// the lifted Verdicts (`IrType::Int`) come from the A2 var-type
/// annotations on `IrProgram.var_types`.
pub struct ConstantFold;

impl super::Transform for ConstantFold {
    fn name(&self) -> &'static str {
        "constant_fold"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        // Stage 0: the real fold is still in `ir::optimize_stmts`, called
        // from `shir_to_perl` and from `shir::ast_to_ir`. The M3 guardrail
        // proves the output is unchanged when the migration lands.
        let _ = prog;
    }
}

/// Eliminate self-assignments (`x=$x`) and unused declarations
/// (`my $x;` where `$x` is never read). The real pass lives in
/// `ir::optimize_stmts`; migration target.
pub struct DeadAssignmentElim;

impl super::Transform for DeadAssignmentElim {
    fn name(&self) -> &'static str {
        "dead_assignment_elim"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let _ = prog;
    }
}

/// Table-driven `use` emission: replaces the ad-hoc `needs_qx` /
/// `needs_strict` / `needs_warnings` / `needs_features` booleans that
/// the Perl generator used to consult with a single Vec of imports
/// derived from the constructs present in the program. Landed in M6
/// (commit 2f70f9d); lives in `generator/mod.rs` today. Migration target.
pub struct ImportMinimize;

impl super::Transform for ImportMinimize {
    fn name(&self) -> &'static str {
        "import_minimize"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let _ = prog;
    }
}

/// Attach the const/var verdicts to the IR: `IrProgram.var_const` is
/// populated from the [`analysis::ConstVar`] verdicts in `PassContext`
/// (sorted by name for deterministic serialization). This is the
/// "extend the shIR with the const/var markup" step of the pipeline —
/// after it runs, every consumer of `IrProgram` (renderers, the ShIR
/// JSON contract via `shir_json.rs`) sees the markup without recomputing
/// it. Idempotent: re-running overwrites with the same verdicts.
pub struct ConstMarkup;

impl super::Transform for ConstMarkup {
    fn name(&self) -> &'static str {
        "const_markup"
    }
    fn run(&self, prog: &mut IrProgram, ctx: &PassContext) {
        let mut verdicts: Vec<(String, crate::ir::VarKind)> =
            ctx.const_vars.iter().map(|(n, k)| (n.clone(), *k)).collect();
        verdicts.sort_by(|a, b| a.0.cmp(&b.0));
        prog.var_const = verdicts;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shir_passes::Transform as _;
    use crate::ir::IrProgram;

    fn empty_prog() -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        }
    }

    #[test]
    fn all_transforms_have_unique_names() {
        let transforms: Vec<Box<dyn super::super::Transform>> = vec![
            Box::new(ConstantFold),
            Box::new(DeadAssignmentElim),
            Box::new(ImportMinimize),
            Box::new(ConstMarkup),
        ];
        let mut names: Vec<&str> = transforms.iter().map(|t| t.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            transforms.len(),
            "duplicate transform name(s): {:?}",
            transforms.iter().map(|t| t.name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn transforms_are_pure_pass_through_in_stage_0() {
        // Stage 0 contract: the migrated-later transforms are stubs that
        // must not mutate the program (ConstMarkup is the exception — it
        // is the real const-markup transform and is tested separately).
        // The migration lands in stage 1; this test pins the no-op
        // behaviour so a premature wiring-up is caught immediately.
        let mut prog = empty_prog();
        let ctx = PassContext::default();
        let snapshot = prog.clone();
        let transforms: Vec<Box<dyn super::super::Transform>> = vec![
            Box::new(ConstantFold),
            Box::new(DeadAssignmentElim),
            Box::new(ImportMinimize),
        ];
        for t in &transforms {
            t.run(&mut prog, &ctx);
        }
        assert_eq!(prog, snapshot);
    }

    #[test]
    fn const_markup_attaches_sorted_verdicts() {
        use crate::ir::{IrExpr, IrStmt};
        let mut ctx = PassContext::default();
        ctx.const_vars
            .insert("z".to_string(), crate::ir::VarKind::Var);
        ctx.const_vars
            .insert("x".to_string(), crate::ir::VarKind::Const);
        let mut prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::Expr(IrExpr::Int(1))],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        ConstMarkup.run(&mut prog, &ctx);
        // sorted by name: x before z
        assert_eq!(
            prog.var_const,
            vec![
                ("x".to_string(), crate::ir::VarKind::Const),
                ("z".to_string(), crate::ir::VarKind::Var),
            ]
        );
        // idempotent
        let before = prog.var_const.clone();
        ConstMarkup.run(&mut prog, &ctx);
        assert_eq!(prog.var_const, before);
    }
}
