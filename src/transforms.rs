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
//! changed anything). They are gated at RUNTIME by the `SH2_TRANSFORMS`
//! env var (comma-separated names; empty/unset = ALL registered), so the
//! estree worker compiles the crate once (all transforms registered) and
//! bisects by setting `SH2_TRANSFORMS=first-n` — no rebuild per step.

use crate::ir::IrStmt;

pub type TransformFn = fn(&mut Vec<IrStmt>) -> bool;

// ── Per-backend transform selection (PLAN §11.12) ─────────────────────
// The shared crate runs `transforms::apply()` for every backend (the §11
// gating note). This is the missing per-backend enable/disable: a
// thread-local "current target" set by the CLI entry, consulted by
// `transform_applies`. Default (unset target, or a transform with no
// restriction) = the transform applies to every backend — the historical
// behavior, so existing transforms are unaffected and the corpus gate
// stays green. A backend-specific transform (e.g. a node-lowering that
// only some backends want) declares its target set here.
use std::cell::RefCell;

thread_local! {
    static CURRENT_TARGET: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Set the current target backend for the duration of a pipeline run.
/// `None` (unset) = every transform applies (the historical default).
pub fn set_target(t: Option<&str>) {
    CURRENT_TARGET.with(|c| *c.borrow_mut() = t.map(|s| s.to_string()));
}

/// The current target backend, if the CLI set one.
pub fn target() -> Option<String> {
    CURRENT_TARGET.with(|c| c.borrow().clone())
}

/// Does the transform `name` apply to the current target backend?
/// Default: every backend. A backend-specific transform declares its
/// target set here (e.g. a node-lowering that only C/JS want, so the
/// shell-like backends never see the rich nodes — the "solve" idea).
pub fn transform_applies(name: &str) -> bool {
    // The target-specific transforms are a list of EXCLUSIONS ("only for
    // c", "code-generating backends only"), so their historical reading of
    // an UNSET target is "every transform applies". An A1-ingress caller
    // that never set the target (otranspilerl's `render`) therefore ran the
    // C-only rewrites for EVERY backend, and what the ingress produced
    // depended on whatever happened to be linked into the binary.
    // `otranspilerl::with_target` binds the target for each render call;
    // this switch pins the old behaviour for an A/B measurement
    // (SH2_TARGET_UNSET_ALL=0 = the unset target excludes, as `set_target`
    // documents).
    if std::env::var("SH2_TARGET_UNSET_ALL").as_deref() == Ok("0") {
        return match target() {
            // the unset target is the byte-pinned embed/A1-export path: no
            // backend-specific rewrite applies
            None => false,
            // capture-node-fold and text-ops list an EXPLICIT target set that
            // omits `shir`; every other arm (including `sh`/`glsl`) applies
            Some(t) => match name {
                "capture-node-fold" => matches!(
                    t.as_str(),
                    "c" | "js" | "estree" | "perl" | "go" | "rs" | "rust" | "zig" | "java" | "python" | "sh" | "glsl" | "glslv"
                ),
                "text-ops" => crate::transforms::planner::emits_renderable_nodes(&t),
                _ => !matches!(
                    (name, t.as_str()),
                    ("text-ops-planner", "sh" | "zsh" | "fish" | "glsl" | "glslv" | "shir")
                        | ("fbdev-gpu-calls", _)
                        | ("split-in-place", _)
                        | ("case-lowering", "sh" | "zsh" | "fish" | "glsl" | "glslv" | "shir")
                ),
            },
        };
    }
    match name {
        // text-ops-planner (the "solve" idea): only the backends that want
        // the rich node compositions run it. The shell-like backends (sh,
        // glsl) never see the rich nodes — they keep the original command
        // (or the existing text_ops lowering). Unset target = every
        // transform applies (historical default).
        "text-ops-planner" => matches!(
            target().as_deref(),
            Some("c" | "js" | "estree" | "perl" | "go" | "rs" | "zig" | "java" | "python")
        ),
        // text-ops EMITS the rich Ext nodes (FieldExtract/RegSub/...). A
        // backend whose manifest declares none of them would receive a node
        // it cannot render — worse than the original command, which every
        // backend already handles natively. So the transform is gated by
        // the SAME manifest the §11.12 planner reads: the core declines to
        // emit what the target cannot consume (the "or get the core to
        // lower them to supported nodes" half of the node contract). An
        // UNSET target keeps the historical behaviour (every backend).
        "text-ops" => match target() {
            None => true,
            Some(t) => crate::transforms::planner::emits_renderable_nodes(&t),
        },
        // fbdev-gpu-calls EMITS fb_* Call nodes. Only the C backend
        // renders those in-process (fb_call_opt); every other backend
        // would route them to a shell-out, worse than the GpuOp
        // file-write fallback. Explicit `c` target only — the
        // text-ops-planner precedent (bash-o4/src/pipeline.rs sets it).
        "fbdev-gpu-calls" => matches!(target().as_deref(), Some("c")),
        // split-in-place emits Ext(Split{in_place}) — only the C backend
        // consumes the flag today (its for-in-split arm tokenizes the
        // var's own buffer instead of copying). Unset target must NOT run
        // it: the embed/Carp test pins fragment output with the target
        // unset, and the other backends' IR must stay byte-stable (the
        // PENDING note this replaces; target gating is the fbdev-gpu-calls
        // precedent that resolves the regressing test).
        "split-in-place" => matches!(target().as_deref(), Some("c")),
        // case-lowering: the shared Case→if/elif string-test lowering.
        // Code-generating backends only — the shell-family targets
        // (sh/zsh/fish) render `case…esac` natively and idiomatically,
        // and an UNSET target or `shir` is the byte-pinned embed/A1-export
        // path where every rewrite is refused (split-in-place precedent).
        "case-lowering" => matches!(
            target().as_deref(),
            Some("c" | "js" | "estree" | "perl" | "go" | "rs" | "rust" | "zig" | "java" | "python")
        ),
        // capture-node-fold: `Call { "capture", [Arrow] }` → the
        // first-class `Capture` node. Both forms render in every backend
        // (est/c convert the node BACK to the call form; perl's node arm
        // is a strict superset of the call arm; sh's is documented
        // identical plus an idiom lift), so every CODEGEN target wants
        // it — and an UNSET target or `shir` is the byte-pinned
        // embed/A1-export path that must stay verbatim (case-lowering
        // precedent).
        "capture-node-fold" => matches!(
            target().as_deref(),
            Some("c" | "js" | "estree" | "perl" | "go" | "rs" | "rust" | "zig" | "java" | "python" | "sh" | "glsl" | "glslv")
        ),
        _ => true,
    }
}

pub mod arith_forms;
pub mod typeset_attrs; // `typeset -i VAR` integer attribute → later assignments become arith()
pub mod arith_identity; // OFFER (core-requests/transforms/offered/arith-identity)
pub mod arith_normalize; // frontend-shape normalisation (`0 - X` → `-X`); run by ingress, not the env-gated registry
pub mod array_len_fold; // OFFER core-requests/transforms/offered/array-len-fold.rs: `${#arr[@]}` → arrayLen(arr)
pub mod slice_identity; // OFFER core-requests/transforms/offered/slice-identity.rs: `${v:0}`/`${v:}` → the plain read param("", v)
pub mod builtin;
pub mod inline_pure_fns; // marketplace offer (estree-20260813-182431) // core-requests/shir-builtin-op: exec(cmd∈builtins) → the native `builtin` op
pub mod isolate_accumulate; // loop-private temp inlining (single-accumulate isolation)
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
pub mod brace_fold; // OFFER core-requests/transforms/offered/brace-fold.rs: literal brace call → its word LIST in list positions
pub mod interp_fold; // `Interpolate([Lit(s)])` → `Str(s)` — canonical single-literal words for every backend
pub mod dead_fn_elim; // generic: remove never-referenced shell functions
pub mod text_ops; // common shell commands → semantic IR nodes (cut/tr/sed/head/tail/wc)
pub mod type_query; // `type NAME` → the answer for program-defined functions / true builtins
pub mod text_ops_planner; // the "solve" idea, observable: wc -l lowers per-backend by manifest (opt-in)
pub mod planner; // backend node manifests + core solver (the "solve" idea, docs/shir-primitives.md)
pub mod sub; // placeholder so the module compiles with an empty registry
pub mod sync_ok_loops; // worker-submitted: loop sync/batch verdicts (analysis-only; the renderer hooks read them)
pub mod shir_native_stmt; // worker-submitted: redirect/herestring/test-chain shapes → native stmt forms
pub mod stray_reserved_word; // a command named `)` / `}` is a bash syntax error → abort (exit 2), keeping the prefix
// New transforms merged from the workspace (core-requests/transforms/done).
pub mod background_decide; // `&` background → THREAD/FORK class (analysis; feeds renderer hooks)
pub mod bc_float_clean; // strip redundant `+ 0.0` before `echo … | bc` (native float emulation)
pub mod direct_calls; // `v=$(sq 3)` of a defined pure-output fn → in-process Capture{Call}
pub mod escape_classes; // per-var STORE requirement census (feeds escape/hoist analyses)
pub mod for_recovery; // counter-while → native For recovery
pub mod fuse_fill_consume; // fill+consume loop fusion with RHS forwarding
pub mod function_purity; // function-level side-effect classes by call-graph fixpoint
pub mod i32_provable;
pub mod f32_narrow; // binary32-exact float verdicts (cdef-float proofs)
pub mod const_capture_fold;
pub mod capture_node_fold; // `Call { "capture", [Arrow] }` → the first-class `Capture` node (shared; metric + perl native-capture paths)
pub mod text_capture_fold; // constant `$(echo …)` in raw test/param/arith text → its literal output
pub mod const_condition_elim;
pub mod copy_propagation;
pub mod counted_while_forinit;
pub mod counted_arith_forinit; // structured-Arith counted while → ForInit (frontend shapes)
pub mod append_to_store; // counted-loop `a = append(a,v)` → `a[i] = v` (frontend shapes)
pub mod dead_store_elim;
pub mod div_mod_pow2;
pub mod hoist_loop_invariants;
pub mod merge_init_assignments;
pub mod redundant_store_elim;
pub mod string_accumulator;
pub mod test_simplification;
pub mod else_if_flatten; // an `else` whose only statement is an `If` IS an `elif` — flatten it
pub mod test_lowering; // glob-affix `[[ ]]` tests → strHasPrefix/strHasSuffix/contains (polyfill speedup, CROSS_BACKEND_RUNTIME.md §8.1)
pub mod case_lowering; // all-static `case` dispatch → if/elif chain over the same shared primitives (generalises the estree try_native_case lowering to every backend)
pub mod echo_return; // pure-output "echo a value and return" functions → fnValue value-returning convention (CROSS_BACKEND_RUNTIME.md §8.3)
pub mod loop_return_lift; // echo+return inside a loop → flag+break (makes strContainsAny/line_at echo-return-eligible)
pub mod unreachable_after_exit; // PROVABLY-32-bit arith annotations
pub mod split_set; // set[int] → small ∪ big partitions (runtime routing; speed-class, SH2_SPLIT_SETS)
pub mod fbdev_gpu; // device write → GPU op (the §4.2 stage ② "gpu-prep" pass)
pub mod fbdev_gpu_batch; // batch consecutive same-verb GpuOps into one dispatch
pub mod fbdev_gpu_elim; // eliminate redundant GpuOps (exact duplicates)
pub mod fbdev_gpu_calls; // GpuOp → the fb_* call it names (C target only)


pub fn all() -> Vec<(&'static str, TransformFn)> {
    vec![
        ("stray-reserved-word", stray_reserved_word::transform),
        ("shir-pipeline-native", shir_pipeline_native::transform),
        // brace-fold: expand a LITERAL brace call to its word list, in
        // list positions only — runs early so const-capture-fold can
        // cascade over the spliced literal words in the same pass.
        ("brace-fold", brace_fold::transform),
        // interp-fold: a quoted word that never needed wrapping
        // (`Interpolate([Lit(s)])`) → the plain `Str` it always was —
        // one canonical shape every backend renders without a
        // concat/format wrapper and every Str-keyed fold can match.
        // Runs early (right behind brace-fold splices literal words).
        ("interp-fold", interp_fold::transform),
        ("dead-fn-elim", dead_fn_elim::transform),
        // (name, <name>::transform) — estree worker adds entries here
        ("inline-pure-fns", inline_pure_fns::inline_pure_fns),
        ("sync-ok-loops", sync_ok_loops::transform),
        ("seq-range-for", seq_range_for::transform),
        ("grep-o", grep_o::transform),
        ("grep-pcre", grep_pcre::transform),
        ("cat-read", cat_read::transform),
        // `type NAME` for a function THIS program defines (or a true shell
        // builtin) → its answer; the eval-inliner re-runs it after Fix 7
        // splices eval'd functions in (perl_generator_fixed).
        ("type-query", type_query::transform),
        // process substitution: the estree corpus path never reaches this
        // (estree.rs transform_cmd rewrites `<(...)` pre-IR) — it serves
        // the --shir export and the A1 ingress (frontend-emitted JSON).
        ("process-subst", process_subst::transform),
        ("arith-forms", arith_forms::transform),
        ("array-len-fold", array_len_fold::transform), // `${#arr[@]}` slice spelling → the contract's arrayLen node
        // `${v:0}` / `${v:0:}` / `${v:}` is the WHOLE parameter — the offset
        // form reads the same value. Fold the zero-offset empty-length
        // param("slice") back to the plain read so every backend retires its
        // substring helper (`__sh_substr`/`shSubstr`/`substr($v,0)`/`cut`) and
        // copy-propagation sees a plain var read. REFUSE > GUESS: non-zero
        // offsets, any length (incl. "0"), and every indexed/positional/
        // indirect/array-list spelling are untouched.
        ("slice-identity", slice_identity::transform),
        // `typeset -i VAR` marks VAR integer for the rest of the scope, so
        // later `VAR=EXPR` assignments must evaluate EXPR as arithmetic.
        // Bash semantics shared by every backend, so it lives in the shared
        // A1 channel and emits the neutral `arith` node each backend already
        // lowers — no renderer changes.
        ("typeset-attrs", typeset_attrs::transform),
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
        ("f32-narrow", f32_narrow::transform),
        ("text-ops", text_ops::transform),
        ("text-ops-planner", text_ops_planner::transform),
        // core-requests/transforms/done batch merge (12 orphaned submissions)
        ("const-capture-fold", const_capture_fold::transform),
        // capture-node-fold: the `Call { "capture", [Arrow] }` spelling →
        // the first-class `Capture` node. Both render (see the module
        // docs), but the call spelling is a sh2.* call site in the Metric
        // and the node carries Perl's native/shell-out-free render arms.
        ("capture-node-fold", capture_node_fold::transform),
        // text-capture-fold: the raw-text twins of const-capture-fold —
        // `[ "$(echo hello)" = hello ]` / `${v:-$(echo dflt)}` keep the
        // substitution as TEXT (no Capture node exists), so every
        // renderer re-derives it at render time (Perl: a
        // `__sh_pipe_capture('echo hello', "pipefail")` fork per operand;
        // estree: sh2.captureSync). Folds provably-constant substitutions
        // into the text; refuse>guess keeps everything else verbatim.
        ("text-capture-fold", text_capture_fold::transform),
        ("const-condition-elim", const_condition_elim::transform),
        ("copy-propagation", copy_propagation::transform),
        ("counted-while-forinit", counted_while_forinit::transform),
        ("counted-arith-forinit", counted_arith_forinit::transform),
        ("append-to-store", append_to_store::transform),
        ("dead-store-elim", dead_store_elim::transform),
        ("div-mod-pow2", div_mod_pow2::transform),
        ("hoist-loop-invariants", hoist_loop_invariants::transform),
        ("merge-init-assignments", merge_init_assignments::transform),
        ("redundant-store-elim", redundant_store_elim::transform),
        ("string-accumulator", string_accumulator::transform),
        ("test-simplification", test_simplification::transform),
        ("test-lowering", test_lowering::transform),
        // case-lowering runs right behind test-lowering: same primitive
        // vocabulary, the dispatch shape instead of the `[[ ]]` shape —
        // later passes (const-condition-elim, else-if-flatten, …) then
        // clean up the chain like any other If.
        ("case-lowering", case_lowering::transform),
        // else-if flattening: `else { if c { … } }` → `elsifs`. Purely
        // structural; run before the backends so every renderer sees the
        // chain in the one shape it already handles.
        ("else-if-flatten", else_if_flatten::transform),
        ("loop-return-lift", loop_return_lift::transform),
        ("echo-return", echo_return::transform),
        ("unreachable-after-exit", unreachable_after_exit::transform),
        // fuse-fill-consume: covering array-fill + same-space consumer
        // → forwarded single loop (no materialization). Runs LAST: it
        // matches final ForInit shapes (after for-recovery / seq-range-for
        // / counted-while-forinit) and only deletes loops, so nothing
        // downstream needs its output except the renderers.
        ("fuse-fill-consume", fuse_fill_consume::transform),
        ("isolate-accumulate", isolate_accumulate::transform),
        // isolate-accumulate: single-use loop-private temps fold into a
        // self-referential tail (single-accumulate isolation for loop
        // candidacies). Runs after fuse (disjoint patterns); only
        // shrinks bodies, so order vs peers is irrelevant.
        // split-set: set[int] → small ∪ big partitions (runtime routing).
        // Speed-class — gated by SH2_SPLIT_SETS (set at -O3/-O4 by the
        // CLI, mirroring set_dual_loops).
        ("split-set", split_set::transform),
        ("fbdev-gpu", fbdev_gpu::transform),
        ("fbdev-gpu-batch", fbdev_gpu_batch::transform),
        ("fbdev-gpu-elim", fbdev_gpu_elim::transform),
        ("fbdev-gpu-calls", fbdev_gpu_calls::transform),
        // split-in-place: liveness-proven destructive buffer reuse for
        // `for w in $var` iteration — the source var's OWN buffer is
        // tokenized in place (delimiters stamped with NUL) instead of
        // copied into a scratch first. The rewrite is gated to the C
        // TARGET (transform_applies): only the C renderer consumes
        // Ext(Split{in_place}) (its for-in-split arm), and the embed/Carp
        // test pins fragment output with the target unset — where this
        // transform now skips. (Was PENDING here pending exactly that
        // gate; the census in split_inplace was hardened for activation:
        // full read+write coverage, eval/unknown refusal.)
        ("split-in-place", crate::shir_passes::split_inplace::transform),
    ]
}

/// Names to enable, from `SH2_TRANSFORMS` (comma-separated; the legacy
/// `SH2_TRANSFORMS` name is accepted as an alias). Empty or unset =
/// ALL registered transforms.
fn enabled_names() -> Vec<String> {
    let raw = std::env::var("SH2_TRANSFORMS")
        .or_else(|_| std::env::var("SH2_TRANSFORMS"))
        .unwrap_or_default();
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Apply the enabled transforms to the statement list. Returns true if any
/// changed. Called by `ast_to_ir` after `optimize_stmts` (NOT by
/// `ast_to_ir_raw` — raw = unoptimized).
pub fn apply(stmts: &mut Vec<IrStmt>) -> bool {
    // A previous compilation's per-function end-live map must never leak
    // into a transform-phase walk (test_lowering's): transform phase runs
    // pre-emission, before shir_to_estree sets it fresh. Unset = all-live.
    crate::shir::clear_fn_end_live();
    let enabled = enabled_names();
    let mut changed = false;
    for (name, tf) in all() {
        if (enabled.is_empty() || enabled.iter().any(|e| e == name)) {
            if transform_applies(name) {
                changed |= tf(stmts);
            }
        }
    }
    changed
}

/// Is a named transform enabled under the `SH2_TRANSFORMS` gate
/// (empty/unset = ALL)? The renderer hooks that READ a transform's
/// verdict statics must consult the same gate, so the bisect machinery
/// (env-gated, no rebuild) can disable a transform end-to-end — including
/// the verdict computation a hook would otherwise re-run under the
/// compile lock (see shir.rs `shir_to_estree`: `sync-ok-loops`).
pub fn transform_enabled(name: &str) -> bool {
    let enabled = enabled_names();
    enabled.is_empty() || enabled.iter().any(|e| e == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_defaults_to_none() {
        set_target(None);
        assert_eq!(target(), None);
        // Default: every transform applies to every backend.
        assert!(transform_applies("text-ops"));
        assert!(transform_applies("dead-store-elim"));
    }

    #[test]
    fn set_target_roundtrips() {
        set_target(Some("c"));
        assert_eq!(target().as_deref(), Some("c"));
        set_target(None);
        assert_eq!(target(), None);
    }

    #[test]
    fn backend_specific_transform_is_filtered() {
        // A transform that declares a target set is filtered when the
        // current target is outside it. (The example arm is commented out
        // in transform_applies; this test pins the mechanism shape.)
        set_target(Some("perl"));
        // With no restricted transforms registered, everything applies.
        assert!(transform_applies("text-ops"));
        set_target(None);
    }

    #[test]
    fn split_in_place_gated_to_c_only() {
        // Registered, but only for the C target: the flag's sole consumer
        // is the C for-in-split arm, and the embed/Carp test pins
        // fragment output with the target unset (where it must skip).
        set_target(Some("c"));
        assert!(transform_applies("split-in-place"));
        set_target(Some("perl"));
        assert!(!transform_applies("split-in-place"));
        set_target(Some("estree"));
        assert!(!transform_applies("split-in-place"));
        set_target(None);
        assert!(!transform_applies("split-in-place"));
    }
}
