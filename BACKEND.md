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

Measured 2026-10-14 (this worktree, 560 files): **3732 → 3577 tokens**
(772 → 711 fn definitions), and `as`-casts **327 → 316**. The corpus gate is
**PASS=645 SKIP=7 FAIL=0** — fully green; the one red file it started with
(`parse-redirect-in-case-pattern.sh`) was a real miscompile and is now fixed
(see item 4 below).

Callout classes retired, all in the textual post-pass chain in `shir_to_rust`:

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
3. **Empty-trap calls.** `drop_dead_trap_calls` removes `__sh_run_traps();`
   from a program whose rendered text contains no
   `__SH_TRAPS.lock().unwrap().push` (27 corpus files call it while never
   installing a trap, so it drains an always-empty queue — an empty `for`
   loop). Dropping the calls lets `prune_unused_helpers`/
   `prune_unused_statics` collect the helper and its static, which also
   retires the `__sh_spawn` it called.
4. **`case` patterns holding an expansion (a correctness fix, not just a
   fold).** Bash expands a pattern before matching, so `case $x in $y)` and
   `case $x in $(cmd) )` compare against the VALUE. The renderer only routed
   `$( … )` through `dollar_interp`; every other `$` form fell through to the
   literal branch and compared against the TEXT `"$y"`, which never matches.
   `contains_unescaped_dollar_expansion` now routes `$name`/`${…}`/`$(…)` all
   through `dollar_interp` (borrowed to `&str` for the pattern slot), and a
   pattern that is exactly `$(echo LIT)` folds to the constant
   (`literal_echo_cmdsub`), so it spawns nothing.
   Pinned by `case_pattern_expansion_forms` / `literal_echo_cmdsub_folds`.
5. **Redundant index casts.** `drop_literal_index_casts` removes `as usize`
   from a BARE non-negative integer-literal index (`get(0 as usize)` →
   `get(0)`, `let i = 1 as usize` → `let i = 1`). Both positions are `usize`
   ones, so Rust infers the type; a computed index, a `-1`, or a suffixed
   literal keeps its cast. 11 sites over 7 files.

**Latent core bug found while fixing (4), not hit by the corpus:** an
assignment whose ONLY use is a `$var` `case` pattern is dropped from the ShIR
altogether — `x=bar; case bar in $x) …` emits no assignment and no
`var_types` entry, so the pattern reads an env var instead. The pattern text
arrives un-expanded from `src/parser/control_flow.rs:381` (`Word::literal`),
and the shared analysis never counts a pattern as a use. The renderer fix in
(4) makes the expansion correct once the variable exists (which is the case
for `$( … )` patterns, where the ShIR emits its own `Assign`); the vanished
assignment needs a SHARED-CORE change and is left for the core owner.

Policy (2026-08-25, AGENTS.md §Backend worktree discipline — binding):
merge main before every verification run; translate constructs into
NATIVE backend idioms (fork/exec is a last-resort fallback, catalogued
in docs/backend-<lang>-limitations.md); share reusable lowerings as
drop-in shIR->shIR transforms under src/transforms/ linked via
build.rs codegen — never fork shared dispatch code.
