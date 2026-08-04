# Java Backend: What It Wants From Core

Status: DRAFT. Mirror of `backends/c/docs/backend-c-core-needs.md`; the
shared asks A1–A7 apply with the Java-specific view. (No worktree yet —
create `backends/java` on `backend/java` when work starts.)

## 0. TL;DR

Java is the **enterprise/portable backend**: a JVM runtime, static but
GC'd, lambdas ✓ (expression-position commands work like JS), and a
`ProcessBuilder`-based spawn model (no fork — heavier than C/Zig/Perl).
Its performance story is the JVM JIT (HotSpot) — the M8 native
lowerings transfer *well* (JIT-compiled loops approach C within ~2–3×).
The differentiator: **bytecode portability** (one `.class` runs
everywhere) and the strong standard library (regex, glob via
`PathMatcher`, files). The weakness: spawn overhead (JVM process startup
+ ProcessBuilder) makes spawn-parity benches the *hardest* to win.

## 1. Consumption path

| Path | Contract | Verdict |
|---|---|---|
| A: ESTree JSON | JS-shaped — no | no |
| B: Rust API | in-process only | no |
| C: **ShIR JSON** (`--shir`) | the ask — a Java generator reads the JSON (Jackson/Gson or a small parser) + renders | **yes** |

Java consumes path C: `debashc --shir > prog.json` → a generator class
renders `.java` → `javac` → `java -cp ... Main`.

## 2. Node inventory

- `Interpolate` → Java string templates (21+) or concatenation — or
  `MessageFormat`; the renderer composes `+` chains (most compatible).
- `Arith` → `long` native ops (`/` and `%` truncate toward zero,
  sign-follow-dividend ✓ — Java matches bash by default — the cleanest
  static arithmetic after C/Zig).
- `Array`/`Index` → `String[]`/`List<String>` (0-based ✓).
- `Arrow` → Java **lambdas** ✓ (single-expression) — multi-statement
  blocks need a named method (shared A8 with python/C/zig/lua).

## 3. Type contract (IrType → Java)

| IrType | Java rendering |
|---|---|
| `Int` | `long` (native primitive — unboxed, fast) |
| `Str` | `String` |
| `Any` | a store map (`Map<String,Object>` or a string-store) — or a `String`-typed store (shell vars are strings; the runtime coerces on arith use, like sh2runtime) |

The `Int`/`Str` split maps to primitives vs `String` — the JVM JIT
specializes both. `Any` vars go to the store (the lift's refusal
serializes directly). No tag needed — `Object` is the union.

## 4. Purity ladder

- `PureCpu` → inline Java (JIT-compiled — near-C hot loops).
- `Emulable` → a `Sh2` runtime class (static methods).
- `Spawn` → `ProcessBuilder` (redirect streams, `waitFor`, `Process.pid`).
  Note: JVM process startup (~100–300ms) + ProcessBuilder (~1–2ms/call)
  means spawn-bound scripts are *worse* than C/JS — the spawn-lift
  lowerings (grepText/cutText/bc) are essential, and the wasm registry
  (Plan 12) is the natural rescue (GraalVM can host wasm; or a small
  `org.graalvm.wasm` path).

## 5. Contract guarantees

### 5.1 Byte strings
Java `String` is UTF-16 — raw bytes need `byte[]` or the U+F800 marker
encoding (which maps cleanly to Java's char — the marker decodes in a
few lines; embedded NULs are fine inside `String` but the JSON gate's
no-raw-NUL rule still applies). The contract's marker format is directly
consumable.

### 5.2 Arithmetic
Java `long` `/` and `%` truncate toward zero + sign-follow-dividend ✓ —
matches bash exactly (like C/Zig). Overflow: Java `long` wraps ✓ (bash's
`$(( ))` wraps at 64-bit — the same). Right-assoc `**` → `Math.pow` +
truncation (double precision — the contract pins the edge; or a
long-pow loop). NaN guard → `Long.parseLong` throws → catch → the whole
test is false ✓ (the "integer expression expected" semantics).

### 5.3 Hygiene + hoisting
Java keywords: `class/if/else/while/for/switch/case/return/new/throws/
synchronized/...` — A6 extends the hygiene pass to the Java list. Java
identifiers: `$` allowed, Unicode allowed — the pass's rules port.
Hoisting: Java declares at method top ✓ (the hoisting guarantee); loop
vars are block-scoped ✓ (the `for i in` leak-prevention).

### 5.4 Statement vs expression
Java has expression statements (unlike C) — a block in expression
position is still not allowed (no statement-expressions) — lambdas ✓ for
single expressions; named methods for multi-statement blocks (A8).
Checked exceptions: the runtime's I/O must declare `throws` — the
contract's purity tags tell the renderer which calls can throw.

### 5.5 Control flow
Native `break`/`continue`/`return` ✓ (labeled break/continue ✓ — the
RETURN-out-of-captured-context maps to a labeled break or an exception
(Java's exception-based control flow is idiomatic — the signal-register
pattern ports as a custom `ControlFlow` exception).

## 6. The `sh2.*` namespace as data (A4)

The 39-callee spec → a `Sh2` class generated from the spec (static
methods). Purity/signature data feeds the renderer's dispatch.

## 7. The runtime port

| sh2.* family | Java implementation |
|---|---|
| `exec`/`pipeline`/`redirect`/`capture`/`subshell`/`background` | `ProcessBuilder` + streams (no fork — Process model) |
| `builtin` | a table of ~30 functions |
| `getVar`/`setVar`/`setArray`/... | a `Map<String,String>` store |
| `param`/`caseMatch`/`test` | `Pattern`/`PathMatcher` + a test parser |
| `arith`/`arithEval`/`idiv`/`imod` | `long` ops + `Math.pow` truncation |
| `bc*` | GraalVM wasm (the shared registry) OR spawn |
| `fs.*` | `Files`/`Path` |
| `positional`/`lastExit`/`functions.set`/`shoptState.set` | static fields |

## 8. Ask list

A1–A7 (shared) + Java-specific:
- **J1**: the arith contract confirms Java's `long` `/`/`%` semantics
  match (they do) and pins `**` (double `Math.pow` + truncate vs a long
  loop — the contract's word).
- **J2**: the classification output includes the "can throw" / purity
  flag per call (checked-exception rendering).
- **J3**: the spawn-lift lowerings are *priority* for Java (spawn
  overhead is the worst of the static backends).

## 9. Honesty

- Easy: `long` arithmetic matches bash by default, lambdas ✓, JIT
  performance near C, strong stdlib, portable bytecode.
- Hard: spawn overhead (ProcessBuilder + JVM startup — the spawn-parity
  story is the weakest), UTF-16 strings (byte-string contract needs the
  marker), checked exceptions, class-per-file structure (the renderer
  must emit a buildable project shape).

## 10. Relationship to the other backends

The JVM's static-plus-GC position (between C/Zig's raw model and
python/lua's dynamic one); lambdas put it in the closure camp (JS/lua/
rust); GraalVM makes the wasm registry (Plan 12) a viable spawn rescue —
the same `bcwasm.wasm` the JS runtime ships.
