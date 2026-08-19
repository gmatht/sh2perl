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

pub mod arith_forms;
pub mod arith_identity; // OFFER (core-requests/transforms/offered/arith-identity)
pub mod builtin;
pub mod inline_pure_fns; // marketplace offer (estree-20260813-182431) // core-requests/shir-builtin-op: exec(cmd∈builtins) → the native `builtin` op
pub mod grep_o; // `grep -o PAT` → the generic grepMatches(text, pattern, flags) op
pub mod process_subst;
pub mod seq_range_for; // worker-submitted: `for i in $(seq A B)` → native numeric range loop
/// Registered transforms. The estree worker APPENDS entries here (and a
/// `pub mod <name>;` above) when a worker-submitted transform is accepted
/// into the crate. Each entry is (name, transform_fn).
pub mod shir_pipeline_native;
pub mod sub; // placeholder so the module compiles with an empty registry
pub mod sync_ok_loops; // worker-submitted: loop sync/batch verdicts (analysis-only; the renderer hooks read them)
pub mod shir_native_stmt; // worker-submitted: redirect/herestring/test-chain shapes → native stmt forms
// OFFERED transforms (core-requests/transforms/offered/) — staged for per-backend bisect
pub mod const_capture_fold;
pub mod const_condition_elim;
pub mod copy_propagation;
pub mod dead_store_elim;
pub mod div_mod_pow2;
pub mod hoist_loop_invariants;
pub mod redundant_store_elim;
pub mod string_accumulator;
pub mod test_simplification;
pub mod unreachable_after_exit;
pub mod counted_while_forinit;
pub mod merge_init_assignments;

pub fn all() -> Vec<(&'static str, TransformFn)> {
    vec![
        ("shir-pipeline-native", shir_pipeline_native::transform),
        // (name, <name>::transform) — estree worker adds entries here
        ("inline-pure-fns", inline_pure_fns::inline_pure_fns),
        ("sync-ok-loops", sync_ok_loops::transform),
        ("seq-range-for", seq_range_for::transform),
        ("grep-o", grep_o::transform),
        // process substitution: the estree corpus path never reaches this
        // (estree.rs transform_cmd rewrites `<(...)` pre-IR) — it serves
        // the --shir export and the A1 ingress (frontend-emitted JSON).
        ("process-subst", process_subst::transform),
        ("arith-forms", arith_forms::transform),
        // native-stmt normalisation (fail-shir: perl shell-out elimination):
        // `echo args > file` → Block-wrapped exec (native select redirect),
        // empty herestrings → status exec, `test && echo || echo` → If.
        //
        // GATED OFF BY DEFAULT: this prior-session rewrite regresses the
        // estree gate — its `status_exec(true)` markers break the estree
        // dead-flags pass (66 estree unit-test failures: 72 with it on → 6
        // with it off; the backend renderers are unaffected either way,
        // 26/26 with or without it). The dead-flags liveness interaction
        // is subtle (the `exec true` status marker isn't dropped from
        // `Block([…, exec true])`), so it stays disabled by default until
        // that is fixed. Re-enable per-run with
        // DEBASHC_TRANSFORMS=shir-native-stmt.
        // shir-native-stmt is PERL-RENDERER-ONLY (applied in
        // ir.rs shir_to_perl): its rewrites (echo>file → Block-wrapped
        // exec, status_exec markers, test&&echo||echo → If) regress the
        // estree backend's native folding (writeFile for echo>file,
        // native echo, dead-flags liveness). Re-enable per-run with
        // DEBASHC_TRANSFORMS=shir-native-stmt for bisecting.
        // ("shir-native-stmt", shir_native_stmt::transform),
        // ── OFFERED (core-requests/transforms/offered/) — staged for the per-backend bisect —
        ("arith-identity", arith_identity::transform),
        ("const-capture-fold", const_capture_fold::transform),
        ("const-condition-elim", const_condition_elim::transform),
        ("copy-propagation", copy_propagation::transform),
        // GATED OFF BY DEFAULT: dead-store-elim is over-aggressive for the
        // current unit tests — it removes assignments to UNREAD variables
        // (`x=42`, `y=$((x+1))`, …) that the estree-lowering and shIR
        // analysis tests check, causing 11 test failures (18 with it on →
        // 7 with it off; the backend renderers are unaffected, 26/26 with
        // or without it). Re-enable per-run with
        // DEBASHC_TRANSFORMS=dead-store-elim.
        ("dead-store-elim", dead_store_elim::transform),
        ("div-mod-pow2", div_mod_pow2::transform),
        ("hoist-loop-invariants", hoist_loop_invariants::transform),
        ("redundant-store-elim", redundant_store_elim::transform),
        ("string-accumulator", string_accumulator::transform),
        ("test-simplification", test_simplification::transform),
        ("unreachable-after-exit", unreachable_after_exit::transform),
        ("counted-while-forinit", counted_while_forinit::transform),
        ("merge-init-assignments", merge_init_assignments::transform),
        // NOTE: exec-to-builtin (shir-builtin-op-20260816) is NOT in the
        // ast_to_ir channel — the rewrite happens at the A1 EXPORT
        // (shir_json::shir_to_shir_json) so the analyses and every
        // exec-keyed renderer arm stay untouched; the exported contract
        // carries the op and the renderers erase/accept at entry.
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
