# sh2perl (debashc) — Agent Guide

Shell-to-Perl transpiler in Rust. Parses bash to an AST and generates Perl.
An ESTree/JSON backend is planned (see below). This repo is standalone: it
must never reference or write into the workspace that contains it (its CI is
self-contained).

## Build & test

- Build: `cargo build --bin debashc`
- Unit tests: `cargo test`
- Corpus gate: the full 517-example suite runs from the workspace harness
  (a `fail` script outside this repo); this repo only guarantees `cargo
  build`/`cargo test` pass standalone.
- WASM demo: `bash build-wasm.sh` (wasm-pack → `www/pkg`).

## Architecture (current)

- `src/parser/` — shell → AST (see `docs/AST.md`).
- `src/ir.rs` — Perl IR with `RawText` migration bridges (see
  `docs/ir-design.md`); being generalized into a language-neutral ShIR.
  **`RawText` is a deliberate migration bridge, not a defect — keep it until
  the migration is proven.**
- `src/generator/` — AST → IrProgram → Perl text. Style decisions live in the
  IR backend, not the generators.
- `src/mir*.rs` — analysis passes (`pub mod mir` currently disabled).

## Planned: ESTree backend (not yet implemented)

- Target: `debashc --estree file.sh` emits **standard ESTree JSON** with shell
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
