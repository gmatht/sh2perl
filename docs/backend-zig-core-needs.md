# Zig Backend: What It Wants From Core

Status: DRAFT. Mirror of `backends/c/docs/backend-c-core-needs.md`; the
C asks A1–A7 apply with Zig-specific adaptations. Worktree
`backends/zig` (if created), branch `backend/zig`.

## 0. TL;DR

Zig is the **C backend's natural successor**: same consumption shape
(static, no GC, real fork/exec), but with **`comptime`** (compile-time
type introspection) and a **first-class wasm target** — the Zig runtime
can compile to `wasm32` and share the JS runtime's wasm registry. The
type annotations (A2) drive `comptime` dispatch instead of C's
hand-written union; the byte-string story is the cleanest of the static
backends (length-delimited slices — no NUL truncation).

## 1. Consumption path

| Path | Contract | Verdict |
|---|---|---|
| A: ESTree JSON | JS-shaped — no | no |
| B: Rust API | in-process only | no |
| C: **ShIR JSON** (`--shir`) | the ask | **yes** |

Zig consumes path C (a Zig program reads the JSON via a small parser or
`std.json`), renders to Zig source, compiles with `zig build-exe`.

## 2. Node inventory

Same `IrStmt`/`IrExpr` set. Zig-specific:

- `Arith` → native integer ops (`+ - * / %` — Zig's `/` and `%` **truncate
  toward zero** ✓ exactly bash; `@divTrunc`/`@rem` if the compiler's
  default differs — it doesn't).
- `Interpolate` → Zig has **no string interpolation** — `std.fmt.allocPrint`
  (format strings) or manual concatenation — the renderer composes
  `{s}`/`{d}` formats.
- `Array`/`Index` → Zig slices (0-based ✓).
- `Arrow` (expression-position commands) → no closures in Zig — the
  C-classification (§5.4) + named helper functions; Zig's `comptime` fn
  pointers make the dispatch cheap.

## 3. Type contract (IrType → Zig)

| IrType | Zig rendering |
|---|---|
| `Int` | `i64` (native) — or `comptime`-chosen `i32`/`i64` by range |
| `Str` | `[]const u8` |
| `Any` | a tagged union (`union(enum) { int: i64, str: []const u8 }`) or the runtime store |

The `comptime` angle: a var's `IrType` is known at *compile* time, so the
renderer can emit a **comptime-tagged struct** where the store only exists
for `Any` vars — the C doc's "guesses from init literals" problem is
eliminated by construction. The lift's refusal (mixed assignments → `Any`)
serializes directly.

## 4. Purity ladder

- `PureCpu` → inline Zig ops (fast — native code).
- `Emulable` → the Zig runtime lib (a `sh2.zig` module).
- `Spawn` → real `fork`/`execvp`/`dup2`/`pipe`/`waitpid` (C-native ✓ —
  the doc's §4 point that C is "uniquely well-suited to Spawn" applies
  equally to Zig, with the same exact bash fd semantics).

## 5. Contract guarantees

### 5.1 Byte strings
Zig strings are `[]const u8` — **length-delimited, NUL-safe** ✓ — the
cleanest of the static backends (no truncation). The U+F800 marker
encoding decodes in a ~10-line function; embedded NULs pass through
natively (the JSON `\u0000` escape must still be forbidden by the
structural gate for C's sake — Zig could allow it, but one rule for all
is simpler).

### 5.2 Arithmetic
Zig `/` and `%` truncate toward zero and sign-follow-dividend ✓ — the
arith contract (A5a) is satisfied by default. Overflow: Zig detects
overflow in debug builds (`-Drelease-safe`) — the renderer should use
`+%`/`-%` (wrapping ops) to match bash's silent wraparound, or the
contract pins overflow semantics (bash's `$(( ))` wraps at 64-bit — Zig
`+%` matches). Right-assoc `**` → a `pow` runtime fn. NaN guard →
`std.fmt.parseInt` error → the whole test is false ✓.

### 5.3 Hygiene + hoisting
Zig keywords: `fn/const/var/if/else/while/for/switch/case/return/struct/
enum/union/error/...`. A6 extends the hygiene pass to the Zig list.
Hoisting: Zig requires declarations before use (like C) — the hoisting
guarantee is required.

### 5.4 Statement vs expression
No closures — the C classification + helper functions. Zig's `comptime`
can specialize the helpers per purity class.

### 5.5 Control flow
Native `break`/`continue`/`return` ✓ (Zig loops) — the signal register
only for runtime-emulated loops. The loop sync/async tag (C §5.5) → Zig
sync loops are native `while`; async only if a capture needs it (Zig's
async is `async`/`await` keywords — opt-in per function).

## 6. The `sh2.*` namespace as data (A4)

The 39-callee spec → `sh2.zig` (a Zig module generated from the spec).
Purity/signature data drives the comptime dispatch (PureCpu → inline,
else the module's functions).

## 7. The runtime port

| sh2.* family | Zig implementation |
|---|---|
| `exec`/`pipeline`/`redirect`/`capture`/`subshell`/`background` | fork/exec/pipe/dup2 (POSIX) |
| `builtin` | a table of ~30 functions |
| `getVar`/`setVar`/`setArray`/... | `std.StringHashMap` store |
| `param`/`caseMatch`/`test` | `std.fmt` + a test parser |
| `arith`/`arithEval`/`idiv`/`imod` | native ints + `+%` wraparound + `@divTrunc` |
| `bc*` | the SHARED wasm module (Zig loads `bcwasm.wasm` natively — the wasm registry is a first-class asset) |
| `fs.*` | `std.fs` |
| `positional`/`lastExit`/`functions.set`/`shoptState.set` | module-level statics |

## 8. Ask list

A1–A7 (shared) + Zig-specific:
- **Z1**: the arith contract must pin **overflow semantics** (bash wraps
  at 64-bit — Zig's `+%` matches; the renderer needs the contract's
  word).
- **Z2**: the `Arrow`-in-expression classification must be a named-helper
  shape (no closures) — shared with C/python/java.

## 9. Honesty

- Easy: native fork/exec, native `%`/`/` semantics, length-delimited
  strings, comptime dispatch on IrType, wasm-native.
- Hard: no string interpolation (format composition), no closures
  (expression-position helpers), manual memory (allocators — the store's
  lifetime discipline).
- The wasm angle is the differentiator: the Zig runtime can target
  `wasm32` and *become* a wasm module in the shared registry (the Plan 9
  manifest) — a runtime-within-runtime.

## 10. Relationship to the other backends

C's sibling (same static/fork/byte-string story; comptime + wasm-native
improve on it). Shares the named-helper requirement (A8/Z2) with
python/java; shares the wasm registry with JS (Plan 12) and potentially
rust.
