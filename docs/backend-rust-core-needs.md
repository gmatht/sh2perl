# Rust Backend: What It Wants From Core

Status: DRAFT. Mirror of `backends/c/docs/backend-c-core-needs.md`. Rust
is the **home language**: debashc IS Rust, the IR types are already
`pub`, and a Rust backend can consume path B (in-process) with zero
serialization overhead — the fastest to build and the closest to the
core. Worktree `backends/rust`, branch `backend/rust`.

## 0. TL;DR

The Rust backend is the **reference static backend**: it consumes the
ShIR in-process (no JSON hop), renders a `.rs` crate, and its runtime
shares the core's own type system (`IrType` maps to real Rust types).
Its byte-string story is the cleanest of all backends (`Vec<u8>`
natively — no NUL/UTF-16 issues), and it compiles to wasm natively (the
shared wasm registry, Plan 12, is a first-class asset — a Rust runtime
can *be* a wasm module). The honest caveat: Rust's borrow checker makes
the runtime store's lifetime discipline real work (a `RefCell`/`Mutex`
store, or an arena).

## 1. Consumption path

| Path | Contract | Verdict |
|---|---|---|
| A: ESTree JSON | JS-shaped — no | no |
| B: **Rust API (in-process `IrProgram`)** | types already `pub`; a renderer crate calls `debashl::shir::ast_to_ir` | **today** — the cheapest path |
| C: ShIR JSON (`--shir`) | useful for the OTHER backends; a Rust backend doesn't need the JSON hop | optional |

Rust is the one backend where path B is the *right* answer (no FFI, no
JSON). Path C still matters for the *contract* itself (Rust is the
serializer's author — `serde` on `IrProgram` is the A1 implementation).

## 2. Node inventory

- `Interpolate` → `format!` (Rust's string interpolation — the direct
  map, with `{}`/`{n}` placeholders).
- `Arith` → native integer ops (`/` and `%` truncate toward zero,
  sign-follow-dividend ✓ — Rust matches bash by default; `wrapping_*`
  for the 64-bit wrap).
- `Array`/`Index` → `Vec<String>` (0-based ✓).
- `Arrow` → Rust **closures** ✓ (single-expression + multi-statement
  blocks via `|| { ... }` — the closure camp with JS/lua/java).
- `Assign`/`Var` → the renderer emits `let mut` / direct binding for
  lifted vars, the store for `Any`.

## 3. Type contract (IrType → Rust)

| IrType | Rust rendering |
|---|---|
| `Int` | `i64` (native, unboxed) |
| `Str` | `String` / `&str` |
| `Any` | a store (`HashMap<String, StoreValue>` — `StoreValue` an enum, or a string store + arith coercion like sh2runtime) |

The lift's verdicts map to real Rust types — the C doc's "guesses from
init literals" is eliminated by construction (the same as Zig's
comptime, but with Rust's type checker as the verifier). The borrow
checker then *enforces* the split: lifted vars are plain bindings; the
store is the only shared mutable state.

## 4. Purity ladder

- `PureCpu` → inline Rust ops (native — the fastest).
- `Emulable` → the runtime crate (`sh2` module).
- `Spawn` → `std::process::Command` (fork/exec via libc — C-level ✓ —
  Rust's spawn is fast and exact).

## 5. Contract guarantees

### 5.1 Byte strings
Rust strings are UTF-8 `String` — raw bytes need `Vec<u8>`/`&[u8]`. The
U+F800 marker encoding decodes in a few lines; embedded NULs pass
natively (`Vec<u8>` is length-delimited ✓ — only the JSON gate's no-raw-
NUL rule applies). The contract's marker format is directly consumable.

### 5.2 Arithmetic
Rust `/` and `%` truncate toward zero + sign-follow-dividend ✓ — matches
bash exactly. Overflow: Rust's *debug builds panic* on overflow — the
renderer MUST use `wrapping_add`/`wrapping_sub`/etc. (or `+%`-style
ops) to match bash's silent 64-bit wrap — the arith contract (A5a) must
pin this (Rust-specific: the wrapping ops are a *requirement*, not an
option — a plain `+` in debug tests would panic). Right-assoc `**` →
`i64::pow` (overflow → wrapping semantics per the contract). NaN guard →
`str::parse::<i64>()` error → the whole test is false ✓.

### 5.3 Hygiene + hoisting
Rust keywords: `fn/let/mut/if/else/while/for/loop/match/struct/enum/
impl/trait/return/...` — A6 extends the hygiene pass. Rust also has
**raw identifiers** (`r#if`) — a second, non-renaming escape. Hoisting:
Rust requires declarations before use ✓ — the hoisting guarantee is
required; the borrow checker makes the loop-var scoping natural (loop
bindings are block-scoped).

### 5.4 Statement vs expression
Rust HAS closures ✓ AND expression statements ✓ — blocks are expressions
in Rust (`{ ... }` yields a value) — expression-position commands map
*cleanly* (the IR's `Arrow`-in-expression → a Rust block or closure).
Rust is the *cleanest* for this — no helper-function workaround needed.

### 5.5 Control flow
Native `break`/`continue`/`return` ✓ (labeled loops ✓ — the
RETURN-out-of-captured-context maps to labeled break or the signal
register; Rust's `'label: loop` is idiomatic). The loop sync tag → Rust
is always-sync (no async needed unless a capture requires it — and
Rust's async is `async`/`await` with a runtime like tokio — opt-in).

## 6. The `sh2.*` namespace as data (A4)

The 39-callee spec → the runtime crate's `pub fn` surface (generated
from the spec). The spec's Rust form is the *canonical* one (the core
already uses these names in `sh2_call`).

## 7. The runtime port

| sh2.* family | Rust implementation |
|---|---|
| `exec`/`pipeline`/`redirect`/`capture`/`subshell`/`background` | `std::process::Command` + `libc` fork/pipe/dup2 (exact POSIX) |
| `builtin` | a table of ~30 functions |
| `getVar`/`setVar`/`setArray`/... | a `HashMap` store behind a `RefCell`/`Mutex` (single-threaded: `RefCell` — same as sh2runtime's map) |
| `param`/`caseMatch`/`test` | `regex`/`globset` crates + a test parser |
| `arith`/`arithEval`/`idiv`/`imod` | `i64` + `wrapping_*` |
| `bc*` | the SHARED wasm module (a Rust runtime loads `bcwasm.wasm` via `wasmtime`/`wasmer` — or compiles the same `number.rs` in — it's the same crate!) |
| `fs.*` | `std::fs` |
| `positional`/`lastExit`/`functions.set`/`shoptState.set` | module-level statics |

## 8. Ask list

A1–A7 (shared) + Rust-specific:
- **R1**: the arith contract must make the **wrapping ops mandatory**
  (Rust debug panics on overflow — a plain `+` is a latent test
  failure).
- **R2**: A6's hygiene list includes Rust's keywords; the raw-identifier
  escape (`r#name`) is the preferred fix.
- **R3**: the A1 serializer (`shir_to_shir_json`) is *the* Rust
  implementation of the cross-backend contract — Rust owns it.

## 9. Honesty

- Easy: in-process consumption (no JSON), native arithmetic, closures +
  expression-statements (the cleanest expression-position story),
  wasm-native, the same language as the core.
- Hard: the borrow checker's lifetime discipline for the store (a
  `RefCell`-wrapped map with careful borrow scopes); compile times;
  the wrapping-op discipline (§5.2).
- The wasm angle: a Rust backend's runtime can compile to
  `wasm32-unknown-unknown` and *be* a module in the shared registry —
  the strongest wasm story of the static backends.

## 10. Relationship to the other backends

The reference static backend — Zig's sibling (same native semantics,
comptime→types), C's successor (safety + the same POSIX model), and the
author of the A1 serializer every other backend consumes. The wasm
registry (Plan 12) is shared with JS/Zig/Lua/Java.
