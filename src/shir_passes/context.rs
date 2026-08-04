//! `PassContext` — the struct that ferries analysis verdicts from the
//! analysis phase to the renderer. Replaces the ten
//! `static Mutex<Option<…>>` globals that lived in shir.rs (see PLAN.md
//! §"The sh2.* boundary"; the comment at shir.rs:5228 documents the
//! determinism-test race the globals were guarding).
//!
//! The struct is constructed once per compilation, before any concurrent
//! reader touches it — the race goes away because there is no global
//! mutable state shared between the pre-passes and the emission.

use std::collections::{HashMap, HashSet};

use crate::ir::IrStmt;

/// All analysis verdicts, populated by the analysis passes, read by the
/// renderer (`shir_to_estree`, `ir_to_perl`, future `shir_to_<lang>`).
#[derive(Default)]
pub struct PassContext {
    // ── Variable lifts (M6 + M8) ─────────────────────────────────
    /// Provably-numeric variables → native `let x = 0` + bare reads/writes.
    /// Populated by [`crate::shir_passes::analysis::NumericLift`].
    pub lifted_numeric: HashSet<String>,

    /// Provably-string variables → native `let x = ""` + bare reads/writes.
    /// Populated by [`crate::shir_passes::analysis::StringLift`].
    pub lifted_string: HashSet<String>,

    /// Per-function local lifts: fn name → set of locals lifted to `let`.
    /// Populated by [`crate::shir_passes::analysis::LocalLift`].
    pub lifted_local: HashMap<String, HashSet<String>>,

    // ── Program-level safety (M6 + M8) ───────────────────────────
    /// `set -e` may be enabled somewhere in this program.
    /// Conservatively `true` (an over-approximation never produces a
    /// wrong native lowering — it just leaves a `sh2.guard(...)` wrapper
    /// in place, which is an identity function when errexit is off).
    pub may_errexit: bool,

    /// `shopt -s nocasematch` may be enabled somewhere. Lifts that
    /// compare strings (e.g. `case *P*)` → `contains`) must lowercase
    /// both sides when this is `true`, or refuse the lift.
    pub case_nocase: bool,

    /// A persistent fd-1 redirect exists somewhere
    /// (`exec >file` / `exec 1>&2` / `exec 1>&-`).
    /// Native top-level `echo` writes `process.stdout` directly; that
    /// is only byte-identical while fd 1 is the module's default
    /// stdout. A persistent redirect anywhere in the program disables
    /// the native-echo lowering.
    pub persist_fd1: bool,

    // ── Function analysis (M6 + M8) ──────────────────────────────
    /// Every function name defined in the program.
    pub program_functions: HashSet<String>,

    /// Function names whose calls lower to the sync fnCall path. Loops
    /// over sync-only call sites go *Sync (the M8 sync-loop speedup:
    /// 10M-iter arith 2.64s → 0.23s).
    pub sync_fn_calls: HashSet<String>,

    /// Function names eligible for native echo/printf in their body
    /// (the body's statements are not under a redirected sink).
    pub native_echo_fns: HashSet<String>,

    // ── Statement-level liveness (M6 + M8) ───────────────────────
    /// `(( ))` statements whose lastExit write is unread (drop the
    /// status-tracking ternary; keep the side effect).
    /// Raw pointer keys are stable between the pre-pass and emission
    /// (the IR tree is immutable there); see shir.rs:97.
    pub lastexit_dead: HashMap<*const IrStmt, bool>,

    /// Loop statements whose per-iteration lastExit tracking is dead
    /// (drop `__sh2_loop_last = sh2.lastExit`; emit a bare `while`).
    pub loop_status_dead: HashMap<*const IrStmt, bool>,

    /// Loops that are inside an async region (capture producer or
    /// pipeline consumer — cannot be lowered to *Sync because the
    /// producer/consumer binding would be lost).
    pub async_region_loops: HashSet<*const IrStmt>,
}

impl PassContext {
    /// True when a variable is lifted to a native binding (either numeric
    /// or string). The renderer uses this to decide between a bare
    /// identifier (lifted) and a `sh2.getVar(name)` call (not lifted).
    pub fn is_lifted(&self, name: &str) -> bool {
        self.lifted_numeric.contains(name) || self.lifted_string.contains(name)
    }

    /// The sh2.* namespace is the floor: every backend renders the
    /// contract; backends may inline further for language idiom.
    /// This helper is the canonical "is this a sh2.* call?" check.
    ///
    /// The IR carries the BARE runtime name (e.g. `test`, `exec`,
    /// `pipeline`) — the `sh2.` prefix is added by the ESTree emitter
    /// at render time. So the check accepts both forms: prefixed
    /// (`sh2.test`, `sh2.exec`) for the post-render view, bare
    /// (`test`, `exec`) for the IR view.
    ///
    /// Stage 0: a static list of known runtime names plus a prefix
    /// test. Stage 1: load the contract from `sh2-namespace.json` and
    /// check the parsed set.
    pub fn is_sh2_call(func: &str) -> bool {
        if func.starts_with("sh2.") {
            return true;
        }
        matches!(
            func,
            // Variables
            "getVar" | "setVar" | "setArray" | "param" | "arith"
            | "arithEval" | "arithAssn" | "brace" | "glob"
            // File system
            | "fs.readFile" | "fs.writeFile" | "fs.stat"
            // Exec / pipeline
            | "exec" | "pipeline" | "redirect" | "capture" | "captureWords"
            | "subshell" | "background" | "commandSubstitution"
            // Control
            | "if" | "while" | "until" | "for" | "case" | "caseMatch"
            | "forLoop" | "whileLoop" | "cstyleFor" | "forIn" | "forOf"
            | "whileLoopSync" | "cstyleForSync"
            // Test
            | "test" | "contains" | "testExpression"
            // Definition
            | "define" | "fnCall" | "unsupported"
            // Status / exit
            | "lastExit" | "exit" | "guard" | "shopt" | "idiv" | "imod"
            // Misc
            | "echo" | "printf" | "cd" | "read" | "export" | "local"
            | "declare" | "typeset" | "set" | "unset" | "eval" | "source"
            | "shift" | "wait" | "kill" | "trap" | "return" | "break" | "continue"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_lifted_handles_both_kinds() {
        let mut ctx = PassContext::default();
        ctx.lifted_numeric.insert("i".to_string());
        ctx.lifted_string.insert("name".to_string());
        assert!(ctx.is_lifted("i"));
        assert!(ctx.is_lifted("name"));
        assert!(!ctx.is_lifted("x"));
    }

    #[test]
    fn is_sh2_call_recognises_namespace() {
        // Prefixed form (the post-render view).
        assert!(PassContext::is_sh2_call("sh2.exec"));
        assert!(PassContext::is_sh2_call("sh2.caseMatch"));
        assert!(PassContext::is_sh2_call("sh2.contains"));
        // Whitelisted sync loops (see PLAN §2.2).
        assert!(PassContext::is_sh2_call("whileLoopSync"));
        // Bare form (the IR view — the emitter prepends sh2. at render time).
        assert!(PassContext::is_sh2_call("exec"));
        assert!(PassContext::is_sh2_call("test"));
        assert!(PassContext::is_sh2_call("pipeline"));
        assert!(PassContext::is_sh2_call("getVar"));
        assert!(PassContext::is_sh2_call("setVar"));
        // Not a sh2.* call — a regular user-defined function.
        assert!(!PassContext::is_sh2_call("my_custom_func"));
        assert!(!PassContext::is_sh2_call("println"));
    }
}
