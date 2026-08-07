//! Worker-submitted IR transforms (core-requests/transforms/*.rs).
//!
//! The estree worker is the single owner of the shared core. Secondary
//! workers (backends + frontends) escalate core needs via
//! `core-requests/` — the strongest form is a CONCRETE IR transform: a
//! self-contained `.rs` module that the estree worker compiles into this
//! crate, then judges by (compile + corpus + metric), bisecting on the
//! corpus to blame a transform that regresses it.
//!
//! Each transform is a `fn(&mut Vec<IrStmt>) -> bool` (returns whether it
//! changed anything). They are gated at RUNTIME by the `DEBASHC_TRANSFORMS`
//! env var (comma-separated names; empty/unset = ALL registered), so the
//! estree worker compiles the crate once (all transforms registered) and
//! bisects by setting `DEBASHC_TRANSFORMS=first-n` — no rebuild per step.

use crate::ir::IrStmt;

pub type TransformFn = fn(&mut Vec<IrStmt>) -> bool;

/// Registered transforms. The estree worker APPENDS entries here (and a
/// `pub mod <name>;` above) when a worker-submitted transform is accepted
/// into the crate. Each entry is (name, transform_fn).
pub mod sub; // placeholder so the module compiles with an empty registry
pub mod sync_ok_loops; // worker-submitted: loop sync/batch verdicts (analysis-only; the renderer hooks read them)
pub mod seq_range_for; // worker-submitted: `for i in $(seq A B)` → native numeric range loop
pub mod process_subst;

pub fn all() -> Vec<(&'static str, TransformFn)> {
    vec![
        // (name, <name>::transform) — estree worker adds entries here
        ("sync-ok-loops", sync_ok_loops::transform),
        ("seq-range-for", seq_range_for::transform),
        // process substitution: the estree corpus path never reaches this
        // (estree.rs transform_cmd rewrites `<(...)` pre-IR) — it serves
        // the --shir export and the A1 ingress (frontend-emitted JSON).
        ("process-subst", process_subst::transform),
    ]
}

/// Names to enable, from `DEBASHC_TRANSFORMS` (comma-separated). Empty or
/// unset = ALL registered transforms.
fn enabled_names() -> Vec<String> {
    std::env::var("DEBASHC_TRANSFORMS")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Apply the enabled transforms to the statement list. Returns true if any
/// changed. Called by `ast_to_ir` after `optimize_stmts` (NOT by
/// `ast_to_ir_raw` — raw = unoptimized).
pub fn apply(stmts: &mut Vec<IrStmt>) -> bool {
    let enabled = enabled_names();
    let mut changed = false;
    for (name, tf) in all() {
        if enabled.is_empty() || enabled.iter().any(|e| e == name) {
            changed |= tf(stmts);
        }
    }
    changed
}

/// Is a named transform enabled under the `DEBASHC_TRANSFORMS` gate
/// (empty/unset = ALL)? The renderer hooks that READ a transform's
/// verdict statics must consult the same gate, so the bisect machinery
/// (env-gated, no rebuild) can disable a transform end-to-end — including
/// the verdict computation a hook would otherwise re-run under the
/// compile lock (see shir.rs `shir_to_estree`: `sync-ok-loops`).
pub fn transform_enabled(name: &str) -> bool {
    let enabled = enabled_names();
    enabled.is_empty() || enabled.iter().any(|e| e == name)
}
