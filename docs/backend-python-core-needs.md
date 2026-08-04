# Python Backend: What It Wants From Core

Status: DRAFT. Mirror of `backends/c/docs/backend-c-core-needs.md` (the C
backend's asks A1–A7 are shared; this doc is the Python-specific view).
Worktree `backends/python` (if created) branch `backend/python`.

## 0. TL;DR

Python consumes the same serialized ShIR + type annotations as C, but its
runtime is an **interpreted module** (no compile step), its store is a
dict, and its hot loops are *interpreter-bound* — so the M8 speedups
(native arith/contains) transfer at the *dispatch* level (fewer runtime
calls) rather than the *execution* level (Python is ~30–100× slower per
op than C/JS JIT). The honest position: Python is a *convenience* backend
(portability, embedding, scripting glue), not a *performance* backend —
except under PyPy or with C extensions.

## 1. Consumption path

| Path | Contract | Verdict |
|---|---|---|
| A: ESTree JSON | JS-shaped — wrong shape | no |
| B: Rust API | in-process only | no |
| C: **ShIR JSON** (`debashc --shir`) | the ask — Python reads it directly (json module) | **yes** |

Python is the *purest* consumer of path C: no compile step, `json.load`
+ a renderer module is the whole integration.

## 2. Node inventory

Same `IrStmt`/`IrExpr` set as C (§2). Python-specific notes:

- `Interpolate` → **f-strings** (`f"hello {name}"`) — a direct, clean map.
- `Array`/`Index` → Python lists (0-based ✓ matches bash arrays).
- `Arith` → native `int` ops (`//` differs: Python `//` floors; bash
  truncates toward zero — use `int(a / b)` or `math.trunc` — see §5.2).
- `Arrow` (expression-position commands) → **lambdas are single-expression
  only** — Python cannot put statements in expressions. The Arrow problem
  is *worse* than in JS: expression-position commands must lower to
  named helper functions (`def __stage_1(): ...`) — the classification
  data from C's §5.4 is a hard requirement, not a nicety.

## 3. Type contract (IrType → Python)

| IrType | Python rendering |
|---|---|
| `Int` | native `int` variable (unboxed, fast) |
| `Str` | native `str` |
| `Any` | the runtime's dict store (the only way Python needs — everything is dynamic anyway) |

Python's dynamic typing makes `Any` *cheap* (no tagged union needed — a
dict value is whatever it is). The `IrType` annotations still matter for
**PEP 484 hints** (readability, IDE, mypy) and for choosing `int` vs
`str` native ops. The `Any` fallback (mixed numeric/string assignments →
store) is the same rule; the lift's refusal serializes directly.

## 4. Purity ladder

`PureCpu`/`Spawn`/`Emulable` tags (A3) map to:
- `PureCpu` → inline Python (native ops — but see §0: the *win* is fewer
  runtime calls, not faster execution).
- `Emulable` → the Python runtime module's builtin table.
- `Spawn` → `subprocess.run(..., capture_output=True)` — Python's
  subprocess module. Note: Python's spawn overhead is ~2–5× JS's
  child_process (no v8-fast spawn) — spawn-parity benches fare *worse*
  in Python. The spawn-lift lowerings (grepText/cutText/bc) matter MORE
  here.

## 5. Contract guarantees

### 5.1 Byte strings
Python 3 separates `str` and `bytes`. The U+F800 marker encoding
(§5.1 C) maps cleanly: decode the marker → the runtime returns `bytes`
where the shell produced raw bytes; `str` elsewhere. Core's guarantee
(no raw NUL in JSON, marker format stable) is directly consumable.

### 5.2 Arithmetic
The one Python gotcha: `//` is floor division, bash truncates toward
zero. The contract must state truncation → Python renders `int(a/b)` or
a `sh2_idiv` shim. `%` sign-follows-dividend: Python's `%` follows the
*divisor* — **Python `%` does NOT match bash** for negative operands —
the runtime needs `math.fmod`-based rem or a shim. Overflow: Python ints
are unbounded ✓ (better than C/JS). NaN guard: `int(x)` raises
ValueError → catch → the whole test is false (bash's "integer expression
expected") — clean.

### 5.3 Hygiene + hoisting
Python keywords differ (def/class/import/from/as/pass/None/True/False/
lambda/with/yield...). A6 extends the hygiene pass to the Python list. A
variable named `class` must become `class_` (or the pass's convention).
Hoisting: Python has no declarations — module-level assignments are
naturally top-down ✓ (hoisting is trivially satisfied).

### 5.4 Statement vs expression
The C doc's classification is *required* (lambdas can't host statements);
Python additionally wants the `Block`-in-expression form lowered to a
**helper function + call** (def before use — Python's def order matters
at module scope; the hoisting guarantee covers it).

### 5.5 Control flow
Python has native `break`/`continue`/`pass` ✓. `return`-out-of-captured-
context: the runtime signal register (the JS path's `sh2.return`
pattern) — or, because Python can *raise* a control-flow exception
(`class _ReturnSignal(Exception)`), the signal story is a clean
exception-based port. The loop sync/async tag (C §5.5) → Python's sync
loops are plain `while`; async loops → asyncio (only when a capture
needs it — Python's async is opt-in).

## 6. The `sh2.*` namespace as data (A4)

The 39-callee spec feeds the Python runtime module directly: a dict
`SH2 = {...}` or a module with `def getVar(...)` etc. The purity/
signature data generates the Python builtin table mechanically.

## 7. The runtime port

| sh2.* family | Python implementation |
|---|---|
| `exec`/`pipeline`/`redirect`/`capture`/`subshell`/`background` | `subprocess` + `tempfile` (no fork — Process/Popen model) |
| `builtin` | a table of ~30 functions (echo/printf/cd/read/...) |
| `getVar`/`setVar`/`setArray`/`arrayIndex`/`listVar` | a `dict` store (vars + arrays) |
| `param`/`caseMatch`/`test` | `fnmatch`/`re` + a test parser (like sh2runtime) |
| `arith`/`arithEval`/`idiv`/`imod` | native ints + the §5.2 truncation/fmod shims |
| `bc*` | the wasm module via a `ctypes`/embedded loader — OR spawn — Python can load the SAME `bcwasm.wasm` the JS runtime ships (a shared wasm registry! see §10) |
| `fs.*` | `os`/`pathlib` |
| `positional`/`lastExit`/`functions.set`/`shoptState.set` | module globals |

## 8. Ask list

A1–A7 (the C doc's) apply unchanged. Python-specific: **A8** — the
`Block`-in-expression form must be a *named-helper* shape (def), not an
arrow/lambda, in the classification output.

## 9. Honesty

- Easy: the renderer is the simplest of all backends (f-strings, dynamic
  store, no compile step); byte strings and unbounded ints are natural.
- Hard: **performance is capped by the interpreter**. The M8 wins are
  dispatch-level; hot loops remain ~30–100× slower than C/JS. The
  spawn-parity story is worse (subprocess overhead). If Python
  *performance* ever matters, the path is PyPy or C extensions (the XS
  story, Python-style) — not the renderer.
- The `%`/`//` arithmetic edges are the top correctness trap (§5.2).

## 10. Relationship to the other backends

Shares A1/A2/A3/A4 with every backend. Unique: the **shared-wasm angle**
— Python can load the same `bcwasm.wasm` (ctypes/wasmer) the JS runtime
uses, making the wasm-bake (Plan 12) a cross-backend asset rather than a
JS-only one. The expression-position-helper requirement (A8) is shared
with C/zig/java (no expression statements); JS/lua/rust are the closure
backends.
