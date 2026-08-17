//! Shared library of shIR passes — the lowering layer between the
//! language-neutral shIR and the per-backend renderers.
//!
//! This module (PLAN.md §3 + the kitchen-sink / cut-down / shared-library
//! design discussion) collects the analyses, transforms, and pattern
//! lifts that *every* backend benefits from. The canonical pipeline runs
//! identically for every backend; the only thing that varies is the
//! renderer (ESTree, Perl, and future C/Zig/Python/Lua/Java).
//!
//! # Pipeline shape
//!
//! ```text
//!   shir (kitchen sink)  ─┐
//!                          ├─►  analyses  ─►  PassContext (verdicts)
//!                          │
//!                          └─►  transforms  (IR → IR; semantic-preserving)
//!                                ─►  pattern lifts  (idiom → cheap sh2.* call)
//!                                      ─►  Metric  (call-site tally)
//!                                            ─►  backend renderer
//! ```
//!
//! # Stage 0 (this commit)
//!
//! The traits (`Analysis`, `Transform`, `PatternLift`), the `PassContext`
//! struct, the `Pipeline` runner, and the `Metric` tally are real and
//! tested. Most analysis/transform implementations are *stubs* that
//! return defaults — they do not yet call into the existing shir.rs
//! analyses (those are still in place, still authoritative, still serving
//! the ESTree and Perl backends). The `ConstVar` analysis and the
//! `ConstMarkup` transform are REAL (the first migrated implementations):
//! `ConstVar` delegates to `shir::analyze_var_const` and `ConstMarkup`
//! attaches the verdicts to `IrProgram.var_const`, so the pipeline now
//! runs its transforms on a clone and returns the post-pipeline program.
//! Stage 1 migrates the rest of the shir.rs analyses into the trait
//! implementations; the M3 guardrail (Perl output is byte-identical) is
//! the test that proves the migration is safe.
//!
//! # Why "kitchen sink" and not "cut down"
//!
//! See `docs/ir-design.md` §"The sh2.* boundary". The sh2.* runtime
//! namespace is the cut-down boundary, not a per-backend shIR subset;
//! every shIR node has a rendering, none are unsupported. The shared
//! library is where common patterns are canonicalised to a sh2.* call
//! that every backend can either render natively or fall back to.

pub mod analysis;
pub mod context;
pub mod lifetime;
pub mod metric;
pub mod optimize; // const_prop + dead_store_elim (the A1 optimizer family)
pub mod pattern;
pub mod restructure;
pub mod strip;
pub mod transform;

pub use context::PassContext;
pub use metric::{CalleeCount, Metric};

/// Run ONLY the goto-restructuring transform on a program. Used by the
/// A1 ingress paths (cli --shir-in-estree / --shir-in-perl) so frontend
/// A1 JSON containing `Label`/`Goto` is restructured before rendering,
/// without disturbing the other pipeline transforms' behavior.
pub fn restructure_goto_only(prog: &mut IrProgram) {
    let ctx = PassContext::default();
    restructure::RestructureGoto.run(prog, &ctx);
}

pub use pattern::grep_to_case::GrepToCase;
pub use strip::strip_cfor;
use pattern::PatternLift;

/// Apply every registered pattern lift to the program in place. The
/// walker is the real implementation of `walk_exprs` (stage 1: the
/// stage-0 stub returned 0 always; this one traverses every stmt and
/// every expr, calling `try_lift_stmt` / `try_lift_expr`, and replaces
/// the node with the lift's output when it returns `Some`. Recurses
/// into children so a lift fires on the OUTERMOST applicable shape
/// (the lift that rewrites an `if` to a `case` sees the `if`, not the
/// lifted `then` body — the recursion re-enters the children for
/// *other* lifts that may apply deeper).
pub fn apply_lifts(prog: &mut IrProgram, lifts: &[Box<dyn PatternLift>]) -> usize {
    let mut n = 0;
    for s in prog.stmts.iter_mut() {
        n += walk_stmt(s, lifts);
    }
    for sub in &mut prog.subs {
        n += walk_block(&mut sub.body, lifts);
    }
    n
}

fn walk_block(stmts: &mut Vec<IrStmt>, lifts: &[Box<dyn PatternLift>]) -> usize {
    let mut n = 0;
    for s in stmts.iter_mut() {
        n += walk_stmt(s, lifts);
    }
    n
}

fn walk_stmt(st: &mut IrStmt, lifts: &[Box<dyn PatternLift>]) -> usize {
    // 1) Try the lift on THIS stmt first. The lift is given the
    //    un-recursted shape; if it fires, the children (e.g. an
    //    `if` cond or body) are not re-walked (the lift owns the
    //    rewrite of the whole stmt).
    for l in lifts {
        if let Some(rep) = l.try_lift_stmt(st) {
            *st = rep;
            return 1;
        }
    }
    // 2) Recurse into children.
    let mut n = 0;
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            n += walk_expr(cond, lifts);
            n += walk_block(then, lifts);
            for (ec, eb) in elsifs.iter_mut() {
                n += walk_expr(ec, lifts);
                n += walk_block(eb, lifts);
            }
            n += walk_block(else_, lifts);
        }
        IrStmt::For { iter, body, .. } => {
            n += walk_expr(iter, lifts);
            n += walk_block(body, lifts);
        }
        IrStmt::While { cond, body, .. } => {
            n += walk_block(body, lifts);
            n += walk_expr(cond, lifts);
        }
        IrStmt::DoWhile { body, cond, .. } => {
            n += walk_block(body, lifts);
            n += walk_expr(cond, lifts);
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            n += walk_expr(discriminant, lifts);
            for cl in clauses.iter_mut() {
                n += walk_block(&mut cl.body, lifts);
            }
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => n += walk_block(b, lifts),
        IrStmt::Pipeline { stages, .. } => {
            for stg in stages.iter_mut() {
                n += walk_block(stg, lifts);
            }
        }
        IrStmt::Redirect { inner, .. } => n += walk_block(inner, lifts),
        IrStmt::Expr(e) | IrStmt::Output { value: e, .. } => n += walk_expr(e, lifts),
        IrStmt::Assign { expr, .. }
        | IrStmt::Declare {
            init: Some(expr), ..
        } => n += walk_expr(expr, lifts),
        IrStmt::WriteFile { path, content, .. } => {
            n += walk_expr(path, lifts);
            n += walk_expr(content, lifts);
        }
        _ => {}
    }
    n
}

fn walk_expr(e: &mut IrExpr, lifts: &[Box<dyn PatternLift>]) -> usize {
    // 1) Try the lift on this expression.
    for l in lifts {
        if let Some(rep) = l.try_lift_expr(e) {
            *e = rep;
            return 1;
        }
    }
    // 2) Recurse.
    let mut n = 0;
    match e {
        IrExpr::Arrow(stmts) => n += walk_block(stmts, lifts),
        IrExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                n += walk_expr(a, lifts);
            }
        }
        IrExpr::Array(els) => {
            for a in els.iter_mut() {
                n += walk_expr(a, lifts);
            }
        }
        IrExpr::Object(pairs) => {
            for (_, v) in pairs.iter_mut() {
                n += walk_expr(v, lifts);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            n += walk_expr(lhs, lifts);
            n += walk_expr(rhs, lifts);
        }
        IrExpr::Index { key, .. } => n += walk_expr(key, lifts),
        _ => {}
    }
    n
}

use crate::ir::{IrExpr, IrProgram, IrStmt};

/// An analysis produces verdicts (populates `PassContext` fields). It does
/// NOT mutate the IR. Pure: same input → same output, no globals, no I/O.
pub trait Analysis: Sync {
    fn name(&self) -> &'static str;
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext);
}

/// A transform rewrites the IR in place. Runs on `&mut IrProgram`. Must
/// preserve the meaning under bash — the corpus gate (`./fail-estree`) is
/// the oracle.
pub trait Transform: Sync {
    fn name(&self) -> &'static str;
    fn run(&self, prog: &mut IrProgram, ctx: &PassContext);
}

/// The canonical pipeline order. Every backend uses this same pipeline;
/// the only thing that varies is the renderer.
pub struct Pipeline {
    pub analyses: Vec<Box<dyn Analysis>>,
    pub transforms: Vec<Box<dyn Transform>>,
    /// Pattern lifts (the idiom-rewrite family — e.g. `grep -> case`).
    /// Run after the transforms; the walker recurses into every stmt
    /// and expr, applying each lift in order. A lift that fires on the
    /// outer node owns the rewrite of that node (children are NOT
    /// re-walked for the same lift).
    pub lifts: Vec<Box<dyn PatternLift>>,
}

impl Pipeline {
    /// Build the canonical pipeline. Analyses populate `PassContext`;
    /// transforms mutate the IR. The order is significant:
    ///
    /// 1. Variable-type analyses (numeric/string/local) must run *before*
    ///    any pattern lift that depends on a variable being a native
    ///    binding rather than a sh2.* call.
    /// 2. Program-level safety analyses (errexit, nocasematch, persist_fd1)
    ///    must run *before* any lift that depends on those invariants
    ///    holding (e.g. `case` → `contains` is only sound when
    ///    `nocasematch` is provably off).
    /// 3. Function analyses (sync_fn_calls, native_echo_fns) follow
    ///    program-level safety so they see the same verdicts the lifts
    ///    will see.
    /// 4. Statement-level liveness (lastexit_dead, loop_status_dead,
    ///    async_region_loops) run last in the analysis phase; they're
    ///    the most expensive and the most dependent on the prior
    ///    analyses being populated.
    /// 5. Transforms (constant fold, dead-assign, import minimise,
    ///    const-markup) run after all analyses are complete. They consume
    ///    `PassContext` read-only.
    pub fn canonical() -> Self {
        Pipeline {
            analyses: vec![
                Box::new(analysis::NumericLift),
                Box::new(analysis::StringLift),
                Box::new(analysis::LocalLift),
                Box::new(analysis::ConstVar),
                // variable lifetime verdicts (live spans + escape set) —
                // independent of the lifts; the C backend's per-point
                // buffer sizing / copy-vs-move input.
                Box::new(lifetime::VarLifetimes),
                Box::new(analysis::ErrexitMayEnable),
                Box::new(analysis::NocaseMayEnable),
                Box::new(analysis::PersistFd1),
                Box::new(analysis::ProgramFunctions),
                Box::new(analysis::SyncFnCalls),
                Box::new(analysis::NativeEchoFns),
                Box::new(analysis::AsyncRegionLoops),
                Box::new(analysis::LastExitLiveness),
                Box::new(analysis::LoopStatusDeadness),
            ],
            transforms: vec![
                Box::new(transform::ConstantFold),
                Box::new(transform::DeadAssignmentElim),
                Box::new(transform::ImportMinimize),
                Box::new(transform::StoreToNative),
                Box::new(transform::ConstMarkup),
                // goto elimination: frontends may emit Label/Goto (C goto,
                // labeled-break families); shell has no goto, so this runs
                // for EVERY backend via the shared pipeline.
                Box::new(restructure::RestructureGoto),
            ],
            lifts: vec![
                // The grep-to-case lift: test-position `echo "$x" | grep PAT`
                // (and the `&&`/`||` chain form) → native `IrStmt::Case`,
                // so the sh renderer emits pure POSIX `case "$x" in *PAT*`
                // with zero external-tool dependency. The contains.rs
                // family handles the weaker `sh2.contains` form; this
                // is the strong POSIX-native sibling.
                Box::new(GrepToCase),
            ],
        }
    }

    /// Run the pipeline on a clone of the program. Returns the populated
    /// `PassContext`, the post-pipeline `IrProgram` (transforms applied
    /// in place on the clone — e.g. `ConstMarkup` populates
    /// `var_const`), and the `Metric` tally of sh2.* call sites.
    ///
    /// The pipeline is pure: the input program is not mutated. Backends
    /// that want the post-pipeline IR use the returned program; the
    /// current shir.rs / ir.rs entry points (`shir_to_estree`,
    /// `shir_to_perl`) accept a pre-pipeline `&IrProgram` and rely on the
    /// renderer to do the work, so this method is for the new entry
    /// point `shir_to_<lang>` and for tests.
    pub fn run(&self, prog: &IrProgram) -> (PassContext, IrProgram, Metric) {
        let mut ctx = PassContext::default();
        for a in &self.analyses {
            a.run(prog, &mut ctx);
        }
        let mut work = prog.clone();
        for t in &self.transforms {
            t.run(&mut work, &ctx);
        }
        // Pattern lifts run after the transforms (they read the
        // post-transform IR — the const-markup verdicts, the dead-assign
        // result, etc.). The lift phase mutates `work` in place.
        let lifts = apply_lifts(&mut work, &self.lifts);
        let _ = lifts; // the metric (call-site count) is the
                       // post-lift sh2.* tally; the lift count is
                       // informational. The Metric is re-tallied below
                       // to reflect the post-lift IR.
        let metric = Metric::tally(&work);
        (ctx, work, metric)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrProgram, IrStmt};

    /// The canonical pipeline must be constructible and runnable on a
    /// minimal program without panicking. This is the load-bearing
    /// smoke test for stage 0: if the trait shapes are wrong, this
    /// fails to compile; if the pipeline panics, this fails to run.
    #[test]
    fn pipeline_runs_on_empty_program() {
        let prog = IrProgram {
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        let (ctx, out, metric) = Pipeline::canonical().run(&prog);
        // Stage 0: analyses are stubs that return defaults (ConstVar is
        // real but an empty program assigns nothing).
        assert!(ctx.lifted_numeric.is_empty());
        assert!(ctx.lifted_string.is_empty());
        assert!(ctx.const_vars.is_empty());
        assert!(!ctx.may_errexit);
        assert!(!ctx.case_nocase);
        assert!(!ctx.persist_fd1);
        // An empty program has zero sh2.* call sites.
        assert!(metric.is_empty());
        assert_eq!(metric.total(), 0);
        // Transforms ran on the clone; the input is untouched.
        assert_eq!(prog.stmts, out.stmts);
        assert!(out.var_const.is_empty());
    }

    /// The pipeline is deterministic: same input → same PassContext, same
    /// post-pipeline IR, same Metric. This is the regression test for the
    /// threading model (the ten static globals in shir.rs hid
    /// nondeterminism; the struct exposes it).
    #[test]
    fn pipeline_is_deterministic() {
        let prog = IrProgram {
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::Expr(crate::ir::IrExpr::Int(42))],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        let (ctx1, out1, m1) = Pipeline::canonical().run(&prog);
        let (ctx2, out2, m2) = Pipeline::canonical().run(&prog);
        // PassContext doesn't impl PartialEq yet (raw pointer fields);
        // compare via serialised fields.
        assert_eq!(ctx1.lifted_numeric, ctx2.lifted_numeric);
        assert_eq!(ctx1.lifted_string, ctx2.lifted_string);
        assert_eq!(ctx1.const_vars, ctx2.const_vars);
        assert_eq!(ctx1.may_errexit, ctx2.may_errexit);
        assert_eq!(out1, out2);
        assert_eq!(m1, m2);
    }

    /// The pipeline must report a non-zero Metric when the program
    /// contains sh2.* calls. This pins the contract: every sh2.*
    /// callee in the post-pipeline IR is counted; the metric is the
    /// worker's progress signal.
    #[test]
    fn pipeline_tallies_sh2_calls() {
        use crate::ir::IrExpr;
        let prog = IrProgram {
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::Expr(IrExpr::Call {
                func: "test".to_string(),
                args: vec![IrExpr::Str(
                    "hello".to_string(),
                    crate::ir::StrStyle::SingleQuoted,
                )],
            })],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        let (_ctx, _out, metric) = Pipeline::canonical().run(&prog);
        assert_eq!(metric.total(), 1);
        assert_eq!(metric.count_of("test"), 1);
    }

    /// End-to-end const-markup: the pipeline analysis detects the single
    /// assignment, the transform attaches `var_const` to the returned
    /// program, and the input program stays unannotated (pure pipeline).
    #[test]
    fn pipeline_attaches_const_markup() {
        let prog = IrProgram {
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: "answer".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: crate::ir::IrExpr::Int(42),
                asm: None,
            }],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        let (ctx, out, _metric) = Pipeline::canonical().run(&prog);
        assert!(ctx.is_const("answer"));
        assert_eq!(
            out.var_const,
            vec![("answer".to_string(), crate::ir::VarKind::Const)]
        );
        // the input is untouched — the pipeline is pure
        assert!(prog.var_const.is_empty());
    }
}
