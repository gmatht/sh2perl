# Lua Backend: What It Wants From Core

Status: DRAFT. Mirror of `backends/c/docs/backend-c-core-needs.md`; the
shared asks A1–A7 apply with the Lua-specific view. (No worktree yet —
create `backends/lua` on `backend/lua` when work starts.)

## 0. TL;DR

Lua is the **embedded-scripting backend**: a tiny interpreter, tables as
the universal structure, real closures ✓ (expression-position commands
work like JS), and byte strings are native ✓. Its performance story is
**LuaJIT**: under LuaJIT, hot loops approach C speed — the M8 native
lowerings (native arith/contains) transfer *well*, which makes Lua the
best *performance-per-effort* interpreted backend. The weakness: the
spawn model (no fork — only `os.execute`/`io.popen`).

## 1. Consumption path

| Path | Contract | Verdict |
|---|---|---|
| A: ESTree JSON | JS-shaped — no | no |
| B: Rust API | in-process only | no |
| C: **ShIR JSON** (`--shir`) | the ask — Lua reads JSON (a small parser or `cjson`) + renders | **yes** |

Lua consumes path C; the renderer outputs a `.lua` script run by the
system `lua`/`luajit`.

## 2. Node inventory

- `Interpolate` → Lua string concatenation or `string.format` (no
  f-strings pre-5.4; 5.4 has no f-strings either — concat).
- `Array`/`Index` → Lua **tables**. CAREFUL: Lua tables are 1-based by
  convention — bash arrays are 0-based. The `arrayIndex`/`arrayItems`
  rendering must map indices (`i+1` or a 0-based table with `[0]` keys —
  possible but non-idiomatic). A core ask: the contract pins the
  array-index convention per backend (or the ShIR JSON normalizes to
  0-based and each backend adapts).
- `Arith` → Lua 5.3+ has integers (`//` is FLOOR division — bash
  truncates toward zero — `math.tointeger(a/b)` or a trunc shim; `%` in
  Lua follows the divisor — **Lua `%` does NOT match bash** for negative
  operands — `math.fmod` + adjustment needed). See §5.2.
- `Arrow` → Lua **has closures** ✓ — expression-position commands become
  anonymous functions, exactly the JS pattern.

## 3. Type contract (IrType → Lua)

| IrType | Lua rendering |
|---|---|
| `Int` | Lua integer (5.3+: `integer` subtype — fast path) |
| `Str` | Lua string |
| `Any` | the runtime's store table (Lua's dynamic store is a table — cheap) |

Under LuaJIT, an `Int` var renders as a Lua number (double or int) —
JIT-compiled arithmetic is near-native. `IrType` also lets the renderer
skip the store for provably-`Int`/`Str` vars (locals) — the same split
the JS lift made.

## 4. Purity ladder

- `PureCpu` → inline Lua (under LuaJIT these are JIT-compiled — fast).
- `Emulable` → the Lua runtime module (`sh2.lua`).
- `Spawn` → `io.popen`/`os.execute` — Lua's spawn story is the WEAKEST
  of all backends: no fork, no robust pipe model (`io.popen` is
  one-directional, no fd manipulation). The spawn-lift lowerings
  (grepText/cutText/bc) matter MOST here; a `Spawn`-heavy script is
  where Lua needs a helper (a C `lua` extension or the wasm registry).

## 5. Contract guarantees

### 5.1 Byte strings
Lua strings ARE byte strings ✓ native (no NUL issue — length-delimited).
The U+F800 marker decodes in a few lines. Cleanest of the interpreted
backends alongside Perl.

### 5.2 Arithmetic
Two Lua traps, both contract-pinnable:
- `//` is floor division (bash truncates toward zero) → `math.tointeger`
  truncation shim.
- `%` follows the divisor (bash follows the dividend) → `math.fmod`-based
  rem shim.
- Integers: Lua 5.3+ distinguishes integer/float; the renderer wants
  `integer` subtypes for `$(( ))`. The arith-contract (A5a) must pin
  these two edges explicitly (they are Lua-only deviations).
- NaN guard: `tonumber(x)` returns nil → the whole test is false ✓.

### 5.3 Hygiene + hoisting
Lua keywords: `and/or/not/if/else/elseif/while/for/function/local/
return/break/end/do/then/repeat/until/...`. A6 extends the hygiene pass.
Hoisting: Lua locals are block-scoped from declaration — the hoisting
guarantee (decls at function top) is satisfiable; the loop-var scoping
must not leak (Lua `for` vars are loop-local ✓ natural).

### 5.4 Statement vs expression
Lua HAS closures ✓ — but only *one* expression per function body (no
multi-statement lambdas) — the Arrow classification still wants a
named-function shape for multi-statement blocks (shared A8 with
python/C/zig/java).

### 5.5 Control flow
Native `break` ✓; Lua has NO `continue` (pre-5.4? — 5.4 ADDED `goto` —
no `continue` still; a `while true do ... break end` or `goto` pattern
for continue). `return`-out-of-captured-context → the signal-register
pattern or Lua's `goto`. The loop sync tag → Lua is always-sync (no
async) — every loop is the sync form.

## 6. The `sh2.*` namespace as data (A4)

The 39-callee spec → `sh2.lua` (a Lua module generated from the spec).

## 7. The runtime port

| sh2.* family | Lua implementation |
|---|---|
| `exec`/`pipeline`/`redirect`/`capture`/`subshell`/`background` | `io.popen` (limited) / `os.execute` — the weak spot; consider a C helper or wasm |
| `builtin` | a table of ~30 functions |
| `getVar`/`setVar`/`setArray`/... | a store table |
| `param`/`caseMatch`/`test` | `string.match`/patterns + a test parser |
| `arith`/`arithEval`/`idiv`/`imod` | native ints + the §5.2 shims |
| `bc*` | the shared wasm module (a `lua` wasm loader, e.g. a small C extension) OR spawn |
| `fs.*` | `io.open`/`os.remove`/`os.rename`/`lfs` |
| `positional`/`lastExit`/`functions.set`/`shoptState.set` | module-level table fields |

## 8. Ask list

A1–A7 (shared) + Lua-specific:
- **L1**: the arith contract pins the `//` floor-division and `%`
  sign rules as Lua-deviation notes (they differ from C/bash).
- **L2**: the array-index convention — Lua tables are 1-based; the ShIR
  JSON should normalize indices and let each backend adapt (or the
  renderer maps `i → i+1`).
- **L3**: a `continue`-without-`continue` lowering (Lua has no
  continue) — the control-flow contract must specify it.

## 9. Honesty

- Easy: closures ✓ (expression-position commands), byte strings ✓,
  tables as store, tiny runtime, always-sync, and under LuaJIT the
  *fastest* interpreted backend (near-C hot loops).
- Hard: the spawn model is genuinely weak (no fork); `//`/`%` arithmetic
  deviations; 1-based tables vs 0-based bash arrays; no continue.
- The performance ceiling (LuaJIT) is the reason to choose Lua over
  python/perl for embedded execution.

## 10. Relationship to the other backends

The closure backend (like JS/rust — expression-position commands work);
the C-adjacent arithmetic (needs the pinning like python's `%`/`//`); the
byte-string-clean camp (with perl); the wasm registry (Plan 12) is the
spawn-model rescue (a wasm-loaded `bc`/`tr`/`sed` avoids io.popen).
