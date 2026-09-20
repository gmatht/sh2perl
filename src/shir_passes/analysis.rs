//! Analyses — the "what the program means" layer.
//!
//! Each analysis populates a field of `PassContext`. Stage 0 implements
//! them as stubs that return the safe default; stage 1 migrates the
//! real implementations from `shir.rs` (where they currently live as
//! free functions, ferried to the renderer via ten `static Mutex<...>`
//! globals — see `PassContext` for the migration plan).
//!
//! The list of analyses is the canonical one from `Pipeline::canonical`.
//! Each one has a name (for diagnostics) and a single `run` method.

use crate::ir::IrProgram;
use crate::shir_passes::PassContext;

/// Provably-numeric variables → native `let x = 0` + bare reads/writes.
/// Stage 0: stub. Stage 1: migrates `shir::numeric_lift_vars`.
pub struct NumericLift;

impl super::Analysis for NumericLift {
    fn name(&self) -> &'static str {
        "numeric_lift"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.lifted_numeric = crate::shir::numeric_lift_vars(prog);
    }
}

/// Provably-string variables → native `let x = ""` + bare reads/writes.
/// Stage 0: stub. Stage 1: migrates `shir::string_lift_vars`.
pub struct StringLift;

impl super::Analysis for StringLift {
    fn name(&self) -> &'static str {
        "string_lift"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        // string lift is defined as "not numeric, but provably string"
        // and consults the numeric verdict (shir.rs:string_lift_vars).
        ctx.lifted_string = crate::shir::string_lift_vars(prog, &ctx.lifted_numeric);
    }
}

/// Per-function local-variable native lift (function `local` decls that
/// can be rendered as `let` inside the function body).
/// Stage 0: stub. Stage 1: migrates the `local_lift_analysis` referenced
/// in shir.rs (the comment at shir.rs:60).
pub struct LocalLift;

impl super::Analysis for LocalLift {
    fn name(&self) -> &'static str {
        "local_lift"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
}

/// Const/var verdicts per assigned variable — the const-markup analysis.
/// `Const` = exactly one static assignment site that executes at most
/// once (outside loops/function bodies) and is never written by the
/// runtime store, native arith, an array-element write, or a dynamic
/// write (`eval`/`source`). The verdicts populate `ctx.const_vars`;
/// [`crate::shir_passes::transform::ConstMarkup`] attaches them to
/// `IrProgram.var_const` so every backend (and the ShIR JSON contract)
/// carries the markup. The implementation is the real one — it delegates
/// to `shir::analyze_var_const`, the canonical pass shared with the
/// `--shir` serialization path.
pub struct ConstVar;

impl super::Analysis for ConstVar {
    fn name(&self) -> &'static str {
        "const_var"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.const_vars = crate::shir::analyze_var_const(prog).into_iter().collect();
    }
}

/// `set -e` (errexit) may be enabled somewhere in the program.
/// Stage 0: default `false` (safe — the guard wrapper is an identity
/// when errexit is off, so the only cost of `false` is a missed
/// optimisation; a wrong `true` would be a soundness bug).
/// Stage 1: migrates `shir::ir_may_enable_errexit`.
pub struct ErrexitMayEnable;

impl super::Analysis for ErrexitMayEnable {
    fn name(&self) -> &'static str {
        "errexit_may_enable"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.may_errexit = crate::shir::ir_may_enable_errexit(prog);
    }
}

/// `shopt -s nocasematch` may be enabled somewhere. Affects whether
/// string-comparison lifts (e.g. `case *P*)` → `contains`) can be
/// applied without lowercasing both sides.
pub struct NocaseMayEnable;

impl super::Analysis for NocaseMayEnable {
    fn name(&self) -> &'static str {
        "nocase_may_enable"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.case_nocase = crate::shir::ir_may_enable_nocasematch(prog);
    }
}

/// A persistent fd-1 redirect exists somewhere in the program
/// (`exec >file` / `exec 1>&2` / `exec 1>&-`). Disables the native
/// top-level `echo` lowering (which writes `process.stdout` directly,
/// only byte-identical while fd 1 is the default stdout).
pub struct PersistFd1;

impl super::Analysis for PersistFd1 {
    fn name(&self) -> &'static str {
        "persist_fd1"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.persist_fd1 = crate::shir::ir_has_persist_fd1(prog);
    }
}

/// Every function name defined in the program. Drives the sync-fn
/// and native-echo-fn analyses below.
pub struct ProgramFunctions;

impl super::Analysis for ProgramFunctions {
    fn name(&self) -> &'static str {
        "program_functions"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        let mut set = std::collections::HashSet::new();
        crate::shir::collect_program_functions(&prog.stmts, &mut set);
        ctx.program_functions = set;
    }
}

/// Strongly-connected components of the function call graph — the
/// mutually-recursive clusters (the glob matchers, the
/// `test`/`tokenizeTest` parser cluster, …). Populates
/// `ctx.function_sccs` / `ctx.function_scc_index` via
/// [`crate::shir_passes::scc`]. This is the recognition a transform
/// needs to reason about a recursive cluster as a whole (coinductive
/// eligibility, pattern lifts) instead of being defeated by a
/// single-function fixpoint.
pub struct FunctionScc;

impl super::Analysis for FunctionScc {
    fn name(&self) -> &'static str {
        "function_scc"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        let graph = crate::shir_passes::scc::build_call_graph(prog);
        let sccs = crate::shir_passes::scc::tarjan_sccs(&graph);
        ctx.function_scc_index = crate::shir_passes::scc::scc_index(&sccs);
        ctx.function_sccs = sccs;
    }
}

/// Function names whose calls lower to the sync fnCall path. Loops
/// over sync-only call sites go *Sync (the M8 speedup: 10M-iter arith
/// 2.64s → 0.23s loop-only).
pub struct SyncFnCalls;

impl super::Analysis for SyncFnCalls {
    fn name(&self) -> &'static str {
        "sync_fn_calls"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
}

/// Function names eligible for native echo/printf in their body
/// (the body's statements are not under a redirected sink).
pub struct NativeEchoFns;

impl super::Analysis for NativeEchoFns {
    fn name(&self) -> &'static str {
        "native_echo_fns"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
}

/// Whether the program needs the shell runtime at all. A program whose
/// only statement kinds are assignments, declarations, `if`/`for`/`while`,
/// `Output` to stdout and plain arithmetic/string expressions is a
/// straight-line "hello world" that any backend can render with its host
/// language's native constructs — no `sh2.*`/`__sh_*` runtime shims, no
/// redirect/capture buffers, no process spawning.
///
/// The verdict is a conservative OVER-approximation of "needs runtime":
/// any construct whose faithful lowering requires shared mutable state,
/// a child process, or I/O redirection marks the program as runtime-full.
/// A `false` verdict is therefore a licence for a renderer to emit plain
/// host code (the rust backend uses it to inline `print!` and drop the
/// sink statics); a `true` verdict only means the renderer must keep the
/// runtime. Missing a runtime need would be a soundness bug, so the walk
/// treats unknown/extension nodes as runtime-full.
pub struct RuntimeFootprint;

impl super::Analysis for RuntimeFootprint {
    fn name(&self) -> &'static str {
        "runtime_footprint"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.needs_runtime = crate::shir_passes::analysis::needs_runtime(prog);
    }
}

/// The `RuntimeFootprint` walk — see that struct's docs for the contract.
pub fn needs_runtime(prog: &IrProgram) -> bool {
    use crate::ir::{InterpPart, IrExpr, IrStmt};

    fn stmt_needs(s: &IrStmt, out: &mut bool) {
        if *out {
            return;
        }
        match s {
            // pure data / control flow — recurse into children
            IrStmt::Output { value, target, .. } => {
                if target.is_some() {
                    *out = true; // `>&N`-style fd target
                }
                expr_needs(value, out);
            }
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    for i in &t.indices {
                        expr_needs(i, out);
                    }
                }
                expr_needs(expr, out);
            }
            IrStmt::Declare { init, .. } => {
                if let Some(i) = init {
                    expr_needs(i, out);
                }
            }
            IrStmt::DeclareArray { elements, .. } => {
                for e in elements {
                    expr_needs(e, out);
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                expr_needs(cond, out);
                for s in then {
                    stmt_needs(s, out);
                }
                for (c, b) in elsifs {
                    expr_needs(c, out);
                    for s in b {
                        stmt_needs(s, out);
                    }
                }
                for s in else_ {
                    stmt_needs(s, out);
                }
            }
            IrStmt::For { iter, body, .. } => {
                expr_needs(iter, out);
                for s in body {
                    stmt_needs(s, out);
                }
            }
            IrStmt::While { cond, body } => {
                expr_needs(cond, out);
                for s in body {
                    stmt_needs(s, out);
                }
            }
            IrStmt::DoWhile { body, cond, until: _ } => {
                for s in body {
                    stmt_needs(s, out);
                }
                expr_needs(cond, out);
            }
            IrStmt::Block(b) => {
                for s in b {
                    stmt_needs(s, out);
                }
            }
            IrStmt::Expr(e) => expr_needs(e, out),
            IrStmt::Return(Some(e)) => expr_needs(e, out),
            IrStmt::Exit(Some(e)) => expr_needs(e, out),
            IrStmt::Return(None) | IrStmt::Exit(None) | IrStmt::Break | IrStmt::Continue => {}
            // everything below needs the runtime
            _ => *out = true,
        }
    }

    fn expr_needs(e: &IrExpr, out: &mut bool) {
        if *out {
            return;
        }
        match e {
            IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Str(..) | IrExpr::Var(..) => {}
            IrExpr::Index { key, .. } => expr_needs(key, out),
            IrExpr::BinOp { lhs, rhs, .. } => {
                expr_needs(lhs, out);
                expr_needs(rhs, out);
            }
            IrExpr::Ternary { cond, then, else_ } => {
                expr_needs(cond, out);
                expr_needs(then, out);
                expr_needs(else_, out);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        expr_needs(x, out);
                    }
                }
            }
            IrExpr::Call { func, args } => {
                // `builtin`/`exec` cover echo/printf/arithmetic builtins;
                // a plain `test`/`arith`/`getVar`/`param`/`arrayLen` is
                // pure. Anything else (capture, pipeline, source, …) needs
                // the runtime.
                //
                // `setArray`/`setArrayAppend` are pure DATA writes (they
                // store a value in a variable), so a literal-list
                // `a = [1,2,3]` does not by itself drag in the runtime —
                // that verdict gates the runtime-free fast paths (a
                // straight-line `print`s program can then lower natively).
                let pure = matches!(
                    func.as_str(),
                    "builtin" | "exec" | "test" | "arith" | "getVar" | "param"
                        | "arrayLen" | "arrayIndex" | "assign" | "return"
                        | "break" | "continue" | "and"
                        | "setArray" | "setArrayAppend"
                );
                if !pure {
                    *out = true;
                }
                for a in args {
                    expr_needs(a, out);
                }
            }
            IrExpr::Arith(a) => arith_needs(a, out),
            // a literal arg array (`echo`'s words) is pure data; its
            // elements may still be runtime (a capture word)
            IrExpr::Array(items) => {
                for it in items {
                    expr_needs(it, out);
                }
            }
            // capture/regex/range/raw/arrow/arraycomp/lambda/json/
            // ext/methodcall — all runtime (or unknown) by default
            _ => *out = true,
        }
    }

    fn arith_needs(a: &crate::ir::ArithAst, out: &mut bool) {
        use crate::ir::ArithAst;
        if *out {
            return;
        }
        match a {
            ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => {}
            ArithAst::Index { key, .. } => arith_needs(key, out),
            ArithAst::Bin { lhs, rhs, .. } => {
                arith_needs(lhs, out);
                arith_needs(rhs, out);
            }
            ArithAst::Un { arg, .. } => arith_needs(arg, out),
            ArithAst::Cond { test, then, else_ } => {
                arith_needs(test, out);
                arith_needs(then, out);
                arith_needs(else_, out);
            }
            ArithAst::Assign { rhs, .. } => arith_needs(rhs, out),
            ArithAst::IncDec { .. } => {}
            ArithAst::Cast { arg, .. } => arith_needs(arg, out),
            _ => *out = true,
        }
    }

    let mut out = false;
    for s in &prog.stmts {
        stmt_needs(s, &mut out);
    }
    out
}

/// Loops that are inside an async region (capture producer or pipeline
/// consumer — cannot be lowered to *Sync because the producer/consumer
/// binding would be lost).
pub struct AsyncRegionLoops;
impl super::Analysis for AsyncRegionLoops {
    fn name(&self) -> &'static str {
        "async_region_loops"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
}

/// `(( ))` statements whose lastExit write is unread (drop the
/// status-tracking ternary; keep the side effect). Plan 4 liveness.
pub struct LastExitLiveness;

impl super::Analysis for LastExitLiveness {
    fn name(&self) -> &'static str {
        "lastexit_liveness"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
}

/// Loop statements whose per-iteration lastExit tracking is dead
/// (drop `__sh2_loop_last = sh2.lastExit`; emit a bare `while`).
pub struct LoopStatusDeadness;

impl super::Analysis for LoopStatusDeadness {
    fn name(&self) -> &'static str {
        "loop_status_deadness"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::IrProgram;
    // bring the `run` method into scope for direct `ConstVar.run(..)` calls
    use crate::shir_passes::Analysis as _;
    // the use-before-decl lint analysis (defined in its own module)
    use crate::shir_passes::used_before_decl::UseBeforeDecl;

    fn empty_prog() -> IrProgram {
        IrProgram {
            var_storage: vec![],
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
        }
    }

    #[test]
    fn all_analyses_have_unique_names() {
        let analyses: Vec<Box<dyn super::super::Analysis>> = vec![
            Box::new(NumericLift),
            Box::new(StringLift),
            Box::new(LocalLift),
            Box::new(ConstVar),
            Box::new(ErrexitMayEnable),
            Box::new(NocaseMayEnable),
            Box::new(PersistFd1),
            Box::new(ProgramFunctions),
            Box::new(FunctionScc),
            Box::new(SyncFnCalls),
            Box::new(NativeEchoFns),
            Box::new(AsyncRegionLoops),
            Box::new(LastExitLiveness),
            Box::new(LoopStatusDeadness),
            Box::new(UseBeforeDecl),
        ];
        let mut names: Vec<&str> = analyses.iter().map(|a| a.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            analyses.len(),
            "duplicate analysis name(s): {:?}",
            analyses.iter().map(|a| a.name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn all_analyses_run_on_empty_program() {
        // The default PassContext fields must remain valid after every
        // analysis runs on an empty program.
        let prog = empty_prog();
        let analyses: Vec<Box<dyn super::super::Analysis>> = vec![
            Box::new(NumericLift),
            Box::new(StringLift),
            Box::new(LocalLift),
            Box::new(ConstVar),
            Box::new(ErrexitMayEnable),
            Box::new(NocaseMayEnable),
            Box::new(PersistFd1),
            Box::new(ProgramFunctions),
            Box::new(FunctionScc),
            Box::new(SyncFnCalls),
            Box::new(NativeEchoFns),
            Box::new(AsyncRegionLoops),
            Box::new(LastExitLiveness),
            Box::new(LoopStatusDeadness),
            Box::new(UseBeforeDecl),
        ];
        let mut ctx = PassContext::default();
        for a in &analyses {
            a.run(&prog, &mut ctx);
        }
        // Every default field still in its default state.
        assert!(ctx.lifted_numeric.is_empty());
        assert!(ctx.lifted_string.is_empty());
        assert!(ctx.lifted_local.is_empty());
        assert!(ctx.const_vars.is_empty());
        assert!(!ctx.may_errexit);
        assert!(!ctx.case_nocase);
        assert!(!ctx.persist_fd1);
        assert!(ctx.program_functions.is_empty());
        assert!(ctx.sync_fn_calls.is_empty());
        assert!(ctx.native_echo_fns.is_empty());
        assert!(ctx.lastexit_dead.is_empty());
        assert!(ctx.loop_status_dead.is_empty());
        assert!(ctx.async_region_loops.is_empty());
    }

    #[test]
    fn const_var_analysis_populates_verdicts() {
        use crate::ir::{IrExpr, IrStmt, StrStyle};
        // `x=5` once, straight-line → Const; `y` reassigned → Var;
        // `z` written by a loop body → Var (multi-run site).
        let prog = IrProgram {
            var_storage: vec![],
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![
                IrStmt::Assign {
                    targets: vec![crate::ir::AssignTarget {
                        var: "x".to_string(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Int(5),
                    asm: None,
                },
                IrStmt::Assign {
                    targets: vec![crate::ir::AssignTarget {
                        var: "y".to_string(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Int(1),
                    asm: None,
                },
                IrStmt::Assign {
                    targets: vec![crate::ir::AssignTarget {
                        var: "y".to_string(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Int(2),
                    asm: None,
                },
                IrStmt::While {
                    cond: IrExpr::Call {
                        func: "test".to_string(),
                        args: vec![IrExpr::Str("true".to_string(), StrStyle::SingleQuoted)],
                    },
                    body: vec![IrStmt::Assign {
                        targets: vec![crate::ir::AssignTarget {
                            var: "z".to_string(),
                            sigil: None,
                            indices: vec![],
                        }],
                        expr: IrExpr::Int(0),
                        asm: None,
                    }],
                },
            ],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        let mut ctx = PassContext::default();
        ConstVar.run(&prog, &mut ctx);
        assert_eq!(ctx.const_vars.get("x"), Some(&crate::ir::VarKind::Const));
        assert_eq!(ctx.const_vars.get("y"), Some(&crate::ir::VarKind::Var));
        assert_eq!(ctx.const_vars.get("z"), Some(&crate::ir::VarKind::Var));
    }
}
