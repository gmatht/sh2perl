# rust backend (worktree: /mnt/wsl/projects/sh2loop/sh2perl/backends/rust, branch: backend/rust)

Shared core (do NOT fork): src/shir.rs (ShIR + lowering), src/ir.rs,
src/estree.rs (node model), src/parser/. Consume the ShIR; render it in
your language's idioms.

Yours (in THIS worktree): the renderer, the runtime namespace, the corpus
gate, and the sh2.*-usage metric for rust.

Merge discipline:
- commit on backend/rust; merge main BEFORE each verification run
- push to main only when the commit does NOT touch the shared core
- core changes are single-owner (the estree worker during the lowering
  phase) — queue, don't fork

Verify: the corpus gate must stay 100% and the metric must only go down.

## The `__sh_*` callout metric

The seam is the runtime-call namespace the renderer emits: `__sh_[A-Za-z_0-9]+`
in the rendered Rust (NOT `sh2.*`, which the stub gate greps for). The metric
tallies tokens over `sh2perl/examples/*.sh`, rendered through
`otranspilerl-cli --target shir | shir_render --target=rust -` — the same path
the gate uses. `rt_metric_wt.sh`/`rt_profile.sh` measure through the worktree
renderer; `harness/backend_runtime_metric.sh` sends rust down its default arm
(`$CLI --target rust`), which links the MAIN tree's `src/rust_backend.rs` and
therefore reports a **different** renderer — never compare the two.

Measured 2026-10-14 (this worktree, 560 files): **3732 → 3581 tokens**
(772 → 713 fn definitions). The corpus gate is unchanged at
PASS=644 SKIP=7 FAIL=1, the one failure being the pre-existing parser bug
`parse-redirect-in-case-pattern.sh` (a `case` pattern holding `$( … )` renders
the literal `"$__sh_case2_casepat_0"` — unrelated to callouts).

Two unneeded-callout classes were retired, both in the textual post-pass chain
in `shir_to_rust`:

1. **`__sh_cat` fixpoint.** `cat_rewrite` (the `concat_vec_peephole` /
   `mixed_cat_peephole` fold) was a single left-to-right pass, so a `__sh_cat`
   NESTED inside a region it had already rewritten was copied through
   untouched — e.g. the `__sh_cat(&[__sh_glob(p)])` inside
   `__sh_cat(&[{ … .map(|p| …) }])` on `064_19_*`. `cat_peephole` has always
   looped to a fixpoint; `cat_rewrite` now does too (`cat_scan` is the pass).
   Its helper-prune predicate also had to stop testing the bracket form
   `__sh_cat(&[`: a variable argument renders as `__sh_cat(&__b)`, and
   collecting the definition while that call survives is an E0425.
2. **Dead-sink prints in user functions.** `inline_sinkfree_prints` rewrites
   only `fn main`'s body, because a user function may be entered under a
   capture/file sink. When the program has NO sink anywhere (`sinks_dead`),
   that reason does not hold, so `inline_deadsink_prints` lowers the
   `__sh_puts`/`__sh_print_words` word-list forms over the WHOLE text — 19
   live calls across 7 files sat in user functions of sink-free programs.

A third, narrower fold rides the same idea: `drop_dead_trap_calls` removes
`__sh_run_traps();` from a program whose rendered text contains no
`__SH_TRAPS.lock().unwrap().push` (27 corpus files call it while never
installing a trap, so it drains an always-empty queue — an empty `for` loop).
Dropping the calls lets `prune_unused_helpers`/`prune_unused_statics` collect
the helper and its static, which also retires the `__sh_spawn` it called.

Policy (2026-08-25, AGENTS.md §Backend worktree discipline — binding):
merge main before every verification run; translate constructs into
NATIVE backend idioms (fork/exec is a last-resort fallback, catalogued
in docs/backend-<lang>-limitations.md); share reusable lowerings as
drop-in shIR->shIR transforms under src/transforms/ linked via
build.rs codegen — never fork shared dispatch code.
