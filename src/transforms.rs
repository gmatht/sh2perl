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
pub mod grep_pcre; // `grep -P 'PCRE'` → portable `grep -E 'ERE'` (lookbehind absorbed + var patterns reduced; refuse>guess)
pub mod cat_read; // `cat [-n] FILE` → ForEachLine streaming loop (native in every backend)
pub mod process_subst;
pub mod ternary_desugar; // C frontend's `ternary(cond,a,b)` call → backend-neutral Ternary + test-call (non-estree backends)
pub mod seq_range_for; // worker-submitted: `for i in $(seq A B)` → native numeric range loop
/// Registered transforms. The estree worker APPENDS entries here (and a
/// `pub mod <name>;` above) when a worker-submitted transform is accepted
/// into the crate. Each entry is (name, transform_fn).
pub mod shir_pipeline_native;
pub mod dead_fn_elim; // generic: remove never-referenced shell functions
pub mod text_ops; // common shell commands → semantic IR nodes (cut/tr/sed/head/tail/wc)
pub mod sub; // placeholder so the module compiles with an empty registry
pub mod sync_ok_loops; // worker-submitted: loop sync/batch verdicts (analysis-only; the renderer hooks read them)
pub mod shir_native_stmt; // worker-submitted: redirect/herestring/test-chain shapes → native stmt forms
// New transforms merged from the workspace (core-requests/transforms/done).
pub mod background_decide; // `&` background → THREAD/FORK class (analysis; feeds renderer hooks)
pub mod bc_float_clean; // strip redundant `+ 0.0` before `echo … | bc` (native float emulation)
pub mod direct_calls; // `v=$(sq 3)` of a defined pure-output fn → in-process Capture{Call}
pub mod escape_classes; // per-var STORE requirement census (feeds escape/hoist analyses)
pub mod for_recovery; // counter-while → native For recovery
pub mod function_purity; // function-level side-effect classes by call-graph fixpoint
pub mod i32_provable;
pub mod const_capture_fold;
pub mod const_condition_elim;
pub mod copy_propagation;
pub mod counted_while_forinit;
pub mod dead_store_elim;
pub mod div_mod_pow2;
pub mod hoist_loop_invariants;
pub mod merge_init_assignments;
pub mod redundant_store_elim;
pub mod string_accumulator;
pub mod test_simplification;
pub mod test_lowering; // glob-affix `[[ ]]` tests → strHasPrefix/strHasSuffix/contains (polyfill speedup, CROSS_BACKEND_RUNTIME.md §8.1)
pub mod echo_return; // pure-output "echo a value and return" functions → fnValue value-returning convention (CROSS_BACKEND_RUNTIME.md §8.3)
pub mod loop_return_lift; // echo+return inside a loop → flag+break (makes strContainsAny/line_at echo-return-eligible)
pub mod unreachable_after_exit; // PROVABLY-32-bit arith annotations


pub fn all() -> Vec<(&'static str, TransformFn)> {
    vec![
        ("shir-pipeline-native", shir_pipeline_native::transform),
        ("dead-fn-elim", dead_fn_elim::transform),
        // (name, <name>::transform) — estree worker adds entries here
        ("inline-pure-fns", inline_pure_fns::inline_pure_fns),
        ("sync-ok-loops", sync_ok_loops::transform),
        ("seq-range-for", seq_range_for::transform),
        ("grep-o", grep_o::transform),
        ("grep-pcre", grep_pcre::transform),
        ("cat-read", cat_read::transform),
        // process substitution: the estree corpus path never reaches this
        // (estree.rs transform_cmd rewrites `<(...)` pre-IR) — it serves
        // the --shir export and the A1 ingress (frontend-emitted JSON).
        ("process-subst", process_subst::transform),
        ("arith-forms", arith_forms::transform),
        // native-stmt normalisation (fail-shir: perl shell-out elimination):
        // `echo args > file` → Block-wrapped exec (native select redirect),
        // empty herestrings → status exec, `test && echo || echo` → If.
        // NOTE: exec-to-builtin (shir-builtin-op-20260816) is NOT in the
        // ast_to_ir channel — the rewrite happens at the A1 EXPORT
        // (shir_json::shir_to_shir_json) so the analyses and every
        // exec-keyed renderer arm stay untouched; the exported contract
        // carries the op and the renderers erase/accept at entry.
        //
        // New transforms merged from the workspace (core-requests/transforms/done).
        // Analyses (escape-classes, function-purity, background-decide, i32-provable)
        // compute statics the renderer hooks read; direct-calls / for-recovery /
        // bc-float-clean make structural rewrites.
        ("background-decide", background_decide::transform),
        ("bc-float-clean", bc_float_clean::transform),
        ("direct-calls", direct_calls::transform),
        ("escape-classes", escape_classes::transform),
        ("for-recovery", for_recovery::transform),
        ("function-purity", function_purity::transform),
        ("i32-provable", i32_provable::transform),
        ("text-ops", text_ops::transform),
        // core-requests/transforms/done batch merge (12 orphaned submissions)
        ("const-capture-fold", const_capture_fold::transform),
        ("const-condition-elim", const_condition_elim::transform),
        ("copy-propagation", copy_propagation::transform),
        ("counted-while-forinit", counted_while_forinit::transform),
        ("dead-store-elim", dead_store_elim::transform),
        ("div-mod-pow2", div_mod_pow2::transform),
        ("hoist-loop-invariants", hoist_loop_invariants::transform),
        ("merge-init-assignments", merge_init_assignments::transform),
        ("redundant-store-elim", redundant_store_elim::transform),
        ("string-accumulator", string_accumulator::transform),
        ("test-simplification", test_simplification::transform),
        ("test-lowering", test_lowering::transform),
        ("loop-return-lift", loop_return_lift::transform),
        ("echo-return", echo_return::transform),
        ("unreachable-after-exit", unreachable_after_exit::transform),
        // split-in-place: liveness-proven destructive buffer reuse for
        // `for w in $var` iteration (C tokenizes the var's own buffer;
        // GC'd languages ignore the flag — see shir_passes/split_inplace)
        // ("split-in-place", crate::shir_passes::split_inplace::transform):
        //   PENDING — activating it changes embed/Carp output (test
        //   ir::tests::embed_injects_carp_for_emulations regresses); the
        //   c worker's in-flight work continues. Flip this line when done.
        //   (Module declared in shir_passes/mod.rs; compiles inert.)
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
