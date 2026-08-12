//! JS backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/js`). Consumes the ShIR and emits the ESTree JSON contract;
//! JS text generation and execution are delegated to the harness's
//! estree→JS implementation — `harness/estree-gen.mjs` (astring +
//! lower.js, from sh2runtime) prints the tree and
//! `harness/estree-runner.mjs` runs it under node with the real `sh2.*`
//! runtime namespace (`harness/sh2-namespace.mjs`).
//!
//! This REPLACES the original draft `shir_to_js` (rendered a small native
//! subset and emitted `sh2.*`/`TODO(unsupported)` stubs for everything
//! else). The worktree now IS the estree→JS backend: the emitter stays a
//! dumb data printer, the runtime owns the shell semantics (PLAN §1.2),
//! and the backend gate executes the emitted JSON through
//! estree-runner.mjs exactly like the estree corpus gate.
//!
//! The output of `--shir-in-js` is therefore byte-identical to
//! `--shir-in-estree`; the js worktree keeps its own CLI entry so the
//! per-language fleet gate (`setup_backends.sh --backend-gate js`) probes
//! and exercises the same contract with the same execution path.

use crate::ir::IrProgram;

/// Render an `IrProgram` to the ESTree JSON contract (the JS backend's
/// output). JS text is produced by the harness's estree→JS printer
/// (`estree-gen.mjs`), which the gate runs via `estree-runner.mjs`.
pub fn shir_to_js(prog: &IrProgram) -> String {
    crate::shir::shir_to_estree_json(prog).unwrap_or_else(|e| {
        // Keep the renderer total: a serialization error is a core/contract
        // bug — emit an object that is valid JSON but not valid ESTree, so
        // the gate's `node estree-runner.mjs` step fails loudly (render
        // rc=0, executor rejects) instead of panicking the pipeline.
        format!("{{ \"contract_version\": 1, \"error\": \"shir_to_estree_json: {e}\" }}")
    })
}
