# sh2perl — Agent Guide

Shell-to-Perl transpiler in Rust. Parses bash to an AST and generates Perl.
An ESTree/JSON backend is planned (see below). This repo is standalone: it
must never reference or write into the workspace that contains it (its CI is
self-contained).

## Build & test

- The user-facing CLI is `otranspilerl-cli` (the workspace crate
  `../otranspilerl`, which statically links THIS core + every renderer):
  `cd ../otranspilerl && cargo build --bin otranspilerl-cli`, then
  `otranspilerl-cli <input> [<output>] [--source-lang L] [--target L]`
  (parse shell → A1 → any backend). This repo builds as a library only:
  `cargo build` / `cargo test --lib`.
- Unit tests: `cargo test --lib`
- Corpus gate: the full 517-example suite runs from the workspace harness
  (a `fail` script outside this repo); this repo only guarantees `cargo
  build`/`cargo test` pass standalone.
- WASM demo: `bash build-wasm.sh` (wasm-pack → `www/pkg`).
- WASI: `bash build-wasi.sh` (WASI command + library modules).

## Architecture (current)

- `src/parser/` — shell → AST (see `docs/AST.md`).
- `src/ir.rs` — Perl IR with `RawText` migration bridges (see
  `docs/ir-design.md`); being generalized into a language-neutral ShIR.
  **`RawText` is a deliberate migration bridge, not a defect — keep it until
  the migration is proven.**
- `src/ir.rs` also hosts the perl renderer (`shir_to_perl`); style
  decisions live in the IR backend, not the generators.
- `src/mir*.rs` — analysis passes (`pub mod mir` currently disabled).

## Planned: ESTree backend (not yet implemented)

- Target: `otranspilerl-cli --target estree file.sh` emits **standard ESTree JSON**
  with shell
  semantics lowered to calls in a documented `sh2.*` runtime namespace
  (`sh2.fs.*`, `sh2.exec`, `sh2.pipeline`, `sh2.capture`, ...). The consumer
  owns the spec (see PLAN.md in the workspace).
- Constraints: async-only codegen (top-level `await`; no `*Sync` callees —
  browsers cannot block); node-compatible error `.code` semantics
  (`ENOENT`, `EISDIR`, ...).
- Roadmap: the workspace plan (PLAN.md) is the authority.

## Guardrails (learned the hard way)

- The IR refactor must be **strictly output-preserving** — run the corpus
  before/after; never "fix" failing tests by blessing regressions.
- `git stash list` contains reverted experiments — check before assuming a
  change is wrong.
- Never `git add .` — scratch files and test artifacts accumulate at the root
  (`__tmp_run_*.pl`, quoted-name files). `.gitignore` covers the known
  patterns; stage explicit paths only.
- `.last_trusted_count` / `.max_tests_passed` are per-run state files — don't
  commit their changes.
- Commit messages follow the corpus convention: "Test results: N passed, M
  failed (fixed K)" when running the suite; describe the fix otherwise.
- `.cursorrules` holds additional conventions for other harnesses.

## Backend worktree discipline (2026-08-25 policy — binding)

Each `backends/<lang>/` worktree owns its renderer, but everything below
applies to every worker writing there:

### 1. Stay in sync with the main architecture

- Merge main into your branch BEFORE every verification run
  (`setup_backends.sh --sync`), and again before pushing.
- Renderers follow main's idioms: the free-function render style of
  `src/java_backend.rs` / `src/python_backend.rs` on main is canonical.
  Do not carry forward private architectures from an older fork of a
  renderer — when main supersedes a shape, port YOUR delta onto main's
  structure, never the reverse.
- Shared core files (`src/shir.rs`, `src/ir.rs`, `src/estree.rs`,
  `src/parser/`, `src/shir_nodes/`, `src/transforms.rs`) are single-owner:
  queue changes through core-requests, don't fork.

### 2. Native idioms over fork/exec

A construct should be TRANSLATED into the target language's native
idioms whenever the backend can express it. Fork/exec (`bash -c`,
subprocess.run with a shell string) is a LAST-RESORT fallback for
constructs the renderer genuinely cannot lower — never a shortcut.

- Every fork/exec site must be catalogued in the backend's
  `docs/backend-<lang>-limitations.md` (or created) with the blocking
  reason; the goal is that list shrinking over time.
- Before adding a new exec fallback, check `src/transforms/`: if the
  construct generalizes ("cat FILE", `grep -o`, seq ranges…), write the
  shIR→shIR transform instead (see §3) so EVERY backend benefits.
- Runtime helpers emitted by a renderer (like java's `__shRun`) are the
  seam for what remains; keep them few and documented.

### 3. Drop-in shIR→shIR transforms via build.rs

Reusable lowering lives as a transform module under `src/transforms/<name>.rs`
(declared in `src/transforms.rs`'s registry), NOT inside any backend's
renderer. The build.rs codegen pattern (see PLUGGABLE_NESTED_TRANSFORMS.md):

- node declarations in `src/shir_nodes/*.node` generate the struct +
  JSON round-trip + ExtNode impl for ALL backends at compile time;
- capability leaves in `src/pipeline_native/capabilities/*.rs` are picked
  up automatically — no shared dispatch edit needed.

Rules for a worker-submitted transform:
1. One file, self-contained: `pub fn transform_program(p: &mut IrProgram)`
   (+ optional `register` metadata), no imports from backend modules.
2. Refuse > guess: leave anything outside the construct's exact shape
   untouched (REFUSE > GUESS, same as the frontends).
3. Gate evidence: the corpus cell(s) it turns green, run against main.
4. Accepted transforms are linked centrally in transforms.rs by the core
   owner — this keeps merge conflicts impossible between backends.

Worktree state hygiene: build artifacts (`target*/`), redirect accidents
(`&1`, `-`, `040`, bare digits), and scratch logs are gitignored — if you
find one tracked, `git rm --cached` it instead of committing around it.
