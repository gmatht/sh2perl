# Universal Backend Contract — What Every Backend Wants From Core

Status: DRAFT. The merge of `backends/c/docs/backend-c-core-needs.md` and
its mirrors: `docs/backend-{python,perl,zig,lua,java,rust}-core-needs.md`.
One language-neutral contract; seven consumers; zero backend-specific
core changes (every ask is additive + output-preserving).

## 0. The one-paragraph contract

Core lowers shell → **one language-neutral, serialized ShIR** annotated
with **conservative type verdicts** and a **machine-readable `sh2.*`
spec**. Every backend renders that contract in its own idioms. The
ESTree JSON stays (sh2runtime consumes it); ShIR JSON is a *second,
semantic* contract for static/heterogeneous targets. Core already
computes most of this for the JS path — the asks publish decisions the
lowering already made.

```
shell ──► Shell AST ──► ShIR (IrProgram)
                              │  A1: serialize (--shir JSON + schema + gate)
                              │  A2: IrType {Int, Str, Any} per var
                              │  A3: purity {PureCpu, Emulable, Spawn} per call
                              │  A4: sh2.* namespace spec (data)
                              │  A5: normative docs (byte-string, arith,
                              │      hygiene/hoisting, control-flow)
                              ▼
   ┌──────┬──────┬──────┬──────┬──────┬──────┬──────┐
   │  C   │Python│ Perl │ Zig  │ Lua  │ Java │ Rust │  (renders + runtime lib)
   └──────┴──────┴──────┴──────┴──────┴──────┴──────┘
```

## 1. The consumption paths

| Path | Contract | Consumers |
|---|---|---|
| A: ESTree JSON | JS-shaped (`process.stdout.write`, `Math.trunc`, `TemplateLiteral`…) | JS runtime only — wrong shape for everyone else |
| B: Rust API | in-process `IrProgram` (types already `pub`) | **Perl** (today), **Rust** (the reference) |
| C: **ShIR JSON** (`--shir`) | language-neutral IR + annotations | **C, Python, Zig, Lua, Java** — the cross-backend contract |

Perl and Rust consume B naturally (they compile against `debashl`); the
others need C (the serializer). B and C must serialize the *same* IR —
the JSON schema is the contract's canonical form.

## 2. The shared asks (all backends)

| # | Ask | What it gives every backend |
|---|---|---|
| A1 | `debashc --shir`: serialize `IrProgram` as JSON + schema + structural gate | the cross-backend contract itself (precedent: `--mir`) |
| A2 | `IrType {Int, Str, Any}` — serialize the numeric lift's existing verdicts | the type split every static backend needs (C: long long vs char*, Zig/Java/Rust: i64/long vs String, …) |
| A3 | purity tag (`PureCpu`/`Emulable`/`Spawn`) on `Exec`/`Pipeline` | the lowering-ladder as data — each backend renders the same decision |
| A4 | machine-readable `sh2.*` spec (39 callees: name, arity, arg types, purity, sync/async, `$?` effects, errors) | every backend's runtime module/table generated from one source |
| A5 | normative docs: byte-string encoding, arith contract, decl-hoisting + identifier-hygiene guarantees | the cross-backend semantics C cannot reverse-engineer from JS shapes |
| A6 | extend the hygiene pass to ALL backends' keywords | reserved-word-safe names everywhere (per-language lists) |
| A7 | publish the builtin/external classification (builtins.json already exists) | the Emulable/Spawn boundary as data |

**The universal rule**: every ask is additive — existing backends ignore
the new fields; the corpus gate + determinism + structural gate verify
each landing.

## 3. The type contract (A2) — per-language mapping

The lift's verdict: `$((...))` → Int; string ops never widen; mixed
numeric+string assignments → `Any` (the store owns it).

| IrType | C | Python | Perl | Zig | Lua | Java | Rust |
|---|---|---|---|---|---|---|---|
| `Int` | `long long` | `int` | scalar SV | `i64` (comptime-chosen) | integer (5.3+) | `long` | `i64` + `wrapping_*` |
| `Str` | `char*` | `str` | scalar SV | `[]const u8` | string | `String` | `String` |
| `Any` | tagged/string store | dict | `%store` hash | union / store | store table | `Map`/`Object` | `HashMap` + enum |

Dynamic backends (Python/Perl/Lua) get the *smallest* win (no unboxing);
static backends (C/Zig/Java/Rust) get the real one. The `Any` fallback
rule is universal (the lift's refusal serializes).

## 4. The purity ladder (A3) — per-language rendering

| Purity | JS (today) | C/Zig/Perl/Rust | Python | Lua | Java |
|---|---|---|---|---|---|
| `PureCpu` | inline JS | inline native (fastest) | inline (dispatch win only) | inline (+LuaJIT near-C) | inline (+JIT near-C) |
| `Emulable` | sync `sh2.*` | runtime table | runtime module | runtime module | runtime class |
| `Spawn` | child_process | **fork/exec/dup2** (exact bash fd semantics) | subprocess (heavier) | io.popen (**weakest**) | ProcessBuilder (**heaviest**) |

**The universal insight**: the fork-capable backends (C/Zig/Perl/Rust)
are the spawn-naturals; the process-model backends (Python/Java) and
io.popen (Lua) need the spawn-LIFT lowerings (grepText/cutText/bc — the
wasm registry) the most.

## 5. The shared guarantees (A5)

### 5.1 Byte strings
- Encoding: the U+F800 private-use marker (`\x01SH2BYTE\x01<HEX>\x01`),
  already stable — **the cross-backend encoding**, documented.
- NUL: the structural gate forbids raw NUL in JSON (`\u0000` truncates
  in C). Length-delimited backends (Zig/Rust/Lua/Perl) could allow it —
  one rule for all is simpler.
- Per-language: Perl/Lua/Zig/Rust handle raw bytes natively (cleanest);
  C needs the marker decode in `sh2_emit`; Java (UTF-16) + JS map the
  marker through chars.

### 5.2 Arithmetic (the one contract with per-language traps)
| rule | matches bash by default | needs a shim |
|---|---|---|
| `/` truncates toward zero | C, Zig, Rust, Java, Perl (`int($a/$b)`) | **Python** (`//` floors), **Lua** (`//` floors) |
| `%` sign follows dividend | C, Zig, Rust, Java, Perl | **Python** (follows divisor), **Lua** (follows divisor) |
| 64-bit wrap | Java (`long`), bash | **Rust** (debug panics → `wrapping_*` MANDATORY), C/JS (UB/exact — pin) |
| right-assoc `**` | bash | all → `pow` + truncate (pin the overflow edge) |
| NaN guard ("integer expression expected" → whole test false) | all | JS `Number.isNaN`; Python `int()` catch; Rust `parse` err; C `strtoll` endptr; Lua `tonumber` nil |

**Ask A5a is the C doc's**: publish `docs/arith-contract.md` with the
table above normative — every backend tests against it (they share no
test with the JS runtime).

### 5.3 Hygiene + hoisting (A6)
- The hygiene pass extends to per-language keyword lists: C
  (for/while/int/static/switch/…), Python (def/class/import/with/…),
  Perl (narrower — sigils disambiguate: `$if` is legal), Zig
  (fn/const/var/struct/…), Lua (and/or/not/function/local/…), Java
  (class/synchronized/throws/…), Rust (fn/let/mut/loop/match/… — plus
  the `r#name` raw-identifier escape).
- Hoisting is a **contract guarantee**: declarations before use (C/Zig/
  Rust/Java need it; Python/Perl/Lua are naturally top-down); loop-scoped
  vars must not leak (per-language scoping rules).

### 5.4 Statement vs expression
| backend | expression-position commands |
|---|---|
| JS, Lua, Rust, Java (lambdas) | closures/lambdas — the JS `Arrow` pattern |
| C, Python, Zig, Perl, Java (multi-stmt) | **named helper functions** — the classification (A3's sibling) must emit a helper shape, not an arrow |

Rust is the cleanest (blocks ARE expressions); Python's lambda
limitation is the worst (helpers mandatory).

### 5.5 Control flow
- Native `break`/`continue`/`return`: C/Zig/Rust/Java/Perl ✓; JS via
  runtime signals (the `*Sync` twins' registers); **Lua has no
  `continue`** (goto/loop pattern — the contract specifies the lowering).
- RETURN-out-of-captured-context: the signal register (JS), labeled
  break (Java/Rust/Perl `last`), exceptions (Python — idiomatic), goto
  (Lua/C).
- The loop sync/async tag (which loops are provably-sync) → every
  non-JS backend is *always sync* (no promise machinery — a
  simplification); JS needs the tag to pick `*Sync` twins.

## 6. The `sh2.*` namespace as data (A4)

The 39-callee corpus surface (`arith, arithEval, arrayIndex, arrayItems,
arrayLen, assign, background, basename, block, break, builtin, capture,
captureWords, caseMatch, continue, cstyleForSync, cutText, dirname, exec,
fnCall, forLoop, forLoopSync, getVar, grepText, idiv, imod, join,
listVar, param, pipeline, redirect, return, setArray, setArrayAppend,
setVar, subshell, test, whileLoop, whileLoopSync` + `fs.*`, `positional`,
`lastExit`, `functions.set`, `shoptState.set`, `bcSqrt`, `unsupported`)
→ one machine-readable spec (name, arity, arg types, purity, sync/async,
`$?` effects, error semantics). Every backend's runtime table is
generated from it — no more three-way duplication (emitter, gate,
runtime).

## 7. The wasm registry (the shared asset)

`bcwasm.wasm` (the bc number core) proved the mechanism in the JS
runtime. The universal contract makes it **cross-backend**:
- JS: `_loadWasm` in sh2runtime (done).
- Zig/Rust: load the same `.wasm` natively (or compile the same
  `number.rs` in — it's the same crate for Rust).
- Python/Java/Lua: via ctypes/GraalVM/a small C shim.
- The Plan 9 manifest (`{js: [...], wasm: [...]}`) is the registry; the
  spawn-fallback option (`SH2_RUNTIME_SPAWN=1`-style) is the oracle.

## 8. The per-language doc index

| doc | the one-line identity |
|---|---|
| `backend-c-core-needs.md` (worktree c) | the original; static + fork + NUL-truncation caveat |
| `backend-python-core-needs.md` | convenience backend; `//`/`%` traps; spawn heavier; PyPy/LuaJIT caveat |
| `backend-perl-core-needs.md` | the original backend; 88-failure generator backlog; always-sync; XS = wasm-analogue |
| `backend-zig-core-needs.md` | C's successor; comptime types; wasm-native |
| `backend-lua-core-needs.md` | embedded; LuaJIT near-C; weakest spawn; `//`/`%` traps; no continue |
| `backend-java-core-needs.md` | JVM portable; `long` matches; heaviest spawn; GraalVM wasm |
| `backend-rust-core-needs.md` | the reference; in-process; wrapping-ops mandatory; wasm-native |

## 9. The implementation order (all additive, corpus-gated)

1. **A4/A5/A7** — harness/docs data (no core changes) — doable NOW.
2. **A1** — `--shir` serializer + schema (the cross-backend contract;
   Rust owns it — the biggest new capability).
3. **A2/A3** — the semantic heart (type verdicts + purity) — in the
   single-owner window after the lowering phase settles.
4. **A6** — per-language keyword hygiene (tiny, output-preserving).
5. **The wasm registry** (Plan 9/12) — the shared asset every backend
   consumes.

Each verified by: corpus green (both backends), determinism
byte-identical, structural gate green.
