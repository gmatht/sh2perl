# C Backend: What It Wants From Core

Status: IMPLEMENTED as of 2026-08-04 (asks A1–A6 landed; A7 partial).
Canonical home: `docs/backend-c-core-needs.md` (this repo). The
worktree-local copy (backends/c/docs/) is superseded. Companion contract
docs: `docs/arith-contract.md`, `docs/estree-contract.md`. The A4 namespace
spec (`sh2-namespace.json`) lives in the workspace harness.

This repo (sh2perl) is self-contained and must never reference the
workspace; the doc describes the contract only.

## 0. TL;DR

The C backend does not want to consume the ESTree JSON contract (it is
JS-flavored: `process.stdout.write`, `Array.join`, `Math.trunc`,
`String(x)`, `TemplateLiteral` — a C renderer must reverse-engineer all of
it). It wants **one language-neutral, serialized ShIR** with **conservative
type annotations** and a **machine-readable `sh2.*` spec**. Good news: core
already computed most of this for the JS path (the numeric lift, the purity
ladder, the callee whitelist); the asks were mostly about *publishing* what
already existed.

## 1. The consumption-path problem

Three ways a C backend can talk to core:

| Path | Contract | Verdict |
|---|---|---|
| A: ESTree JSON (`--estree`) | JS-shaped ESTree + `sh2.*` calls | wrong shape long-term |
| B: Direct ShIR via Rust API (`shir::ast_to_ir`) | `IrProgram` in-process | works only for Rust-written backends |
| C: **Serialized ShIR JSON** (`debashc file --shir`) | language-neutral IR as JSON | **the ask — DONE (A1)** |

Path C mirrors the ESTree-JSON decision that decoupled sh2runtime: core
lowers once, every backend renders. It does not replace `--estree`; it
*adds* a second, semantic contract for static/heterogeneous targets.

## 2. The node inventory C needs (present)

The renderer walks these (via ShIR JSON or the Rust API):

- `IrStmt`: `Output`, `WriteFile`, `Assign`, `Declare`, `DeclareArray`,
  `If`, `For`, `While`, `DoWhile`, `Exec`, `Pipeline`, `Exit`, `Case`,
  `Redirect`, `Function`, `Subshell`, `Background`, `Block`, `Expr`,
  `Return`.
- `IrExpr`: `Int`, `Str`, `Var`, `Index`, `BinOp`, `Call`, `MethodCall`,
  `Ternary`, `Interpolate`, `Capture`, `Range`, `Arith`, `Array`, `Ident`,
  `Bool`, `Json`, `Object`, `Arrow` (JS-only).

Present and adequate. What was missing was not nodes but **contract**:
type, purity, classification, and hygiene metadata on top of these nodes.

## 3. The type contract (DONE — A2)

PLAN.md v2 rejected C because "C needs type inference + runtime lib". Core
already runs the analysis for the JS path: `numeric_lift_vars`
(shir.rs:4590 "assignment is provably numeric... no `sh2.setVar` +
`arithEval`"; shir.rs:4085 "the numeric lift only admits sources that parse
as integers"). A2 surfaces it:

- `ir::IrType { Int, Str, Any }` — the verdict enum.
- `shir::analyze_var_types(&IrProgram) -> Vec<(String, IrType)>` — numeric
  lift → `Int`, string lift → `Str`, neither (runtime store) → `Any`.
- `IrProgram.var_types` carries the verdicts; the ShIR JSON emits
  `var_types` (sorted by name, deterministic).

A C renderer emits `long long` for `Int` vars and a string/var-store for
everything else — exactly the draft's split, driven by core instead of the
renderer's guess. Existing backends ignore the field (additive).

## 4. The purity / lowering-ladder contract (DONE — A3)

The M8 ladder (PLAN.md §9) is JS-flavored. A3 makes it data:

- `shir_json` attaches `"purity"` to every `Call` node (and `IrStmt::Exec`/
  `IrStmt::Pipeline`), mapping the `sh2.*` namespace to
  `PureCpu | Emulable | Fs | Spawn | Control` per the A4 spec. `exec`
  refines by command name: builtin (`SYNC_BUILTINS`, now `pub(crate)`) →
  `Emulable`, external → `Spawn`.
- Verified on real scripts: `ls /tmp` → Spawn, `wc -l`/`head -3`/`echo` →
  Emulable, pipeline/capture → Spawn.

C is uniquely well-suited to `Spawn`: real `fork`/`execvp`/`dup2`/`pipe`
with exact bash fd semantics — no process-emulation layer (the JS path's
hardest problem).

## 5. Contract guarantees

### 5.1 Byte-string semantics — `docs/estree-contract.md` §1 (DONE — A5)

C strings are NUL-terminated; bash strings are byte strings. The emitter
preserves non-UTF-8 bytes via U+F800 private-use chars →
`\x01SH2BYTE\x01<HEX>\x01` markers; the C runtime's output path must decode
them (a ~10-line function). Embedded NUL: the gate must reject raw NULs in
JSON strings (C truncates) — flagged as a gate TODO in the contract doc.

### 5.2 Arithmetic semantics — `docs/arith-contract.md` (DONE — A5)

C's `long long` matches bash `$(( ))` if the edges are pinned: truncation
toward zero, `%` sign follows dividend, right-assoc `**` → `pow`, logicals
→ 0/1, zero divisor aborts the whole expansion (`sh2.idiv`/`sh2.imod`
throw), NaN guard (`Number.isNaN` → test false; C: `strtoll` + endptr),
empty/unparseable operand → 0. The doc is normative with a §8 test list.

### 5.3 Identifier hygiene + declaration hoisting (DONE — A6)

- `safe_ident` (the loop-var mangler) now also avoids **C keywords**
  (`int`, `long`, `char`, `sizeof`, `struct`, ...) beyond the JS list.
  Output-preserving on the corpus (no example names a loop var after a C
  keyword). Plain `Var` names still reach renderers raw — C renderers apply
  their own keyword map (documented in `docs/estree-contract.md` §2).
- Declaration hoisting is a contract guarantee (`docs/estree-contract.md`
  §3): `VariableDeclaration`s precede use; loop-scoped vars don't leak.

### 5.4 Statement vs expression classification

C has no closures (the JS path's `Arrow` is a JS-ism). The ShIR JSON
preserves the `Arrow`/`Expr` distinction as data; a C renderer chooses
statement emission vs helper-function+call. No new node kind needed.

### 5.5 Control-flow signals

JS implements break/continue/return out of captured contexts via runtime
signals (`sh2.break`/`sh2.continue`/`sh2.return`, `*Sync` loop twins). The
ShIR JSON marks these `Control` (A3); C uses plain `break`/`continue`/
`return` for native loops and a signal register for emulated ones.

## 6. The `sh2.*` namespace as data (DONE — A4)

The spec `sh2-namespace.json` (workspace harness) is the single source:
39 corpus callees + `sh2.fs.*` + special objects + the native-JS surface,
each with purity class, async flag, signature, semantics. The structural
gate, the JS runtime, and the C runtime all derive from it. Here, the
emitter's callee surface (`src/estree.rs` + `src/shir.rs`) must stay in
sync with it — see `docs/estree-contract.md` §5.

## 7. What the C runtime must port

| sh2.* family | C implementation |
|---|---|
| `exec`, `pipeline`, `redirect`, `capture`, `subshell`, `background` | real `fork`/`execvp`/`dup2`/`pipe`/`waitpid` + fd table — C-native |
| `builtin` | table of ~30 builtins from the harness `builtins.json` |
| `getVar`/`setVar`/array family | hash-table string store |
| `param`, `caseMatch`, `test` | POSIX `glob(3)`/`fnmatch`/`regexec` + test parser |
| `arith`/`idiv`/`imod` | native `long long` + abort semantics (§5.2) |
| `bc*` (bignum) | spawn `bc` — the only genuinely spawn-required family |
| `fs.*` | POSIX `open/read/write/stat/unlink/mkdir` |

## 8. The ask list (status)

| # | Ask | Where | Status |
|---|---|---|---|
| A1 | `debashc file --shir`: serialize `IrProgram` as language-neutral JSON | `src/shir_json.rs` + CLI (`file --shir`, `--shir`) + `pub mod shir_json` | **DONE** |
| A2 | `IrType {Int,Str,Any}` + `analyze_var_types` serializing the lift verdicts | `src/ir.rs`, `src/shir.rs`, `IrProgram.var_types` | **DONE** |
| A3 | purity tag on `Call`/`Exec`/`Pipeline` (A4 mapping) | `src/shir_json.rs` | **DONE** |
| A4 | machine-readable `sh2.*` spec | workspace harness `sh2-namespace.json` | **DONE** |
| A5 | normative docs: byte strings, arith contract, hygiene/hoisting | `docs/arith-contract.md`, `docs/estree-contract.md` | **DONE** |
| A6 | C-keyword hygiene in `safe_ident` | `src/shir.rs` | **DONE** |
| A7 | publish the builtin/external classification from core | `SYNC_BUILTINS` now `pub(crate)`; the canonical runtime list stays harness-side (`builtins.json`) | **PARTIAL** — core list + runtime list should be unified |

Remaining for the C backend itself (worktree-side, not core): a renderer
library over the ShIR JSON (or the Rust API), the C runtime port (§7), a
gate + metric mirroring the JS harness.

## 9. What the draft proved

The ESTree-JSON-consumer draft ran end-to-end (`sh → debashc --estree →
renderer → gcc → run`) and matched bash on the lowerable subset. Its
failures were exactly the v2 objections, now concrete: type inference
(guessed `long long` vs `char*` from init literals — core has the real
analysis, now serialized) and the runtime lib (§7). It also surfaced
C-specific gotchas the contract absorbs: immutable string literals
(in-place `toupper` segfaulted on `"world"`), `!==`/`===` don't exist,
`String(x)`/`Array.join`/`Math.trunc` are JS-isms needing shims.

## 10. Relationship to the other backends

The `backends/{python,zig,go}` worktrees face the identical contract
problem; A1+A2+A4 serve all of them. The estree worker's improvement mode
(M8) is JS-specific — nothing here competes: the C asks serialize decisions
the worker already made.
