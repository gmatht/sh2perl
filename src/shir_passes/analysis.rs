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
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {
        // Stage 0: pass-through. The real implementation walks the IR
        // looking for variables whose every assignment is provably
        // numeric (no string interpolation, no command substitution,
        // no mixed-type re-assignment). Lives in shir.rs:4641 today.
    }
}

/// Provably-string variables → native `let x = ""` + bare reads/writes.
/// Stage 0: stub. Stage 1: migrates `shir::string_lift_vars`.
pub struct StringLift;

impl super::Analysis for StringLift {
    fn name(&self) -> &'static str {
        "string_lift"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {
        // Stage 0: pass-through. Lives in shir.rs:12884 today.
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
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {
        // Stage 0: default. The real implementation scans every
        // assignment and exec for `set -e` / `set +e` / `set -o errexit`.
        // Default field is `false`; `PassContext::default()` provides it.
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
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
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
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
}

/// Every function name defined in the program. Drives the sync-fn
/// and native-echo-fn analyses below.
pub struct ProgramFunctions;

impl super::Analysis for ProgramFunctions {
    fn name(&self) -> &'static str {
        "program_functions"
    }
    fn run(&self, _prog: &IrProgram, _ctx: &mut PassContext) {}
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

    fn empty_prog() -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
        }
    }

    #[test]
    fn all_analyses_have_unique_names() {
        let analyses: Vec<Box<dyn super::super::Analysis>> = vec![
            Box::new(NumericLift),
            Box::new(StringLift),
            Box::new(LocalLift),
            Box::new(ErrexitMayEnable),
            Box::new(NocaseMayEnable),
            Box::new(PersistFd1),
            Box::new(ProgramFunctions),
            Box::new(SyncFnCalls),
            Box::new(NativeEchoFns),
            Box::new(AsyncRegionLoops),
            Box::new(LastExitLiveness),
            Box::new(LoopStatusDeadness),
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
            Box::new(ErrexitMayEnable),
            Box::new(NocaseMayEnable),
            Box::new(PersistFd1),
            Box::new(ProgramFunctions),
            Box::new(SyncFnCalls),
            Box::new(NativeEchoFns),
            Box::new(AsyncRegionLoops),
            Box::new(LastExitLiveness),
            Box::new(LoopStatusDeadness),
        ];
        let mut ctx = PassContext::default();
        for a in &analyses {
            a.run(&prog, &mut ctx);
        }
        // Every default field still in its default state.
        assert!(ctx.lifted_numeric.is_empty());
        assert!(ctx.lifted_string.is_empty());
        assert!(ctx.lifted_local.is_empty());
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
}
