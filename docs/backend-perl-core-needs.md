# Perl Backend: What It Wants From Core

Status: DRAFT. The Perl backend is the **original** backend (path B today —
it consumes the IR in-process), and the only one with a live corpus view:
**437/529 passing, 92 failing** (`fail` harness: ~79 stdout mismatches +
~10 generation refusals — all CORRECTNESS gaps, not speed). This doc
mirrors `backends/c/docs/backend-c-core-needs.md` with the Perl-specific
perspective.

## 0. TL;DR

Perl does NOT need a ground-up rewrite — it already consumes the shared
IR (`IrProgram` in-process; `generator/` renders `IrStmt`/`IrExpr` → Perl).
What it needs is the **same corpus-driven lowering investment the ESTree
path got** (M8): the 79 mismatches are generator-coverage gaps (escapes,
parameter expansion, cmdsub edges), not architecture. After correctness,
the **XS layer** (Perl's C-extension mechanism — the analogue of the
wasm/JS natives) gives the hot paths: a native arith/text/bc module.

## 1. Consumption path

| Path | Contract | Verdict |
|---|---|---|
| A: ESTree JSON | JS-shaped — no | no |
| B: **Rust API (in-process IR)** | today's path — `generator/` renders `IrProgram` → Perl | **today** |
| C: ShIR JSON (`--shir`) | the cross-backend contract (A1) — a Perl backend could consume it out-of-process | later, optional |

Perl is the one backend where path B is natural (Rust + Perl both compile
natively; a `debashl` callable). Path C matters only if a *pure-Perl* tool
chain (no Rust binary) is a product goal.

## 2. Node inventory

The IR was originally **Perl-shaped** (ir-design.md: sigils, `qx{}`,
`defined`, `my`). The ESTree path added nodes (`IrStmt::Expr`, `Arith`,
`Arrow`) the Perl generator ignores. The 88 failures cluster around
constructs the generator renders wrong or refuses:
- parameter expansion (`${x//p/r}`, `${x:2:3}`, `${!prefix*}`) — the
  ESTree path's `sh2.param` covers these; the Perl generator's inline
  regex/substitution handling is partial.
- process substitution `<(...)` (tty-cmdsub, 096_head_procsub) — the
  ESTree path's temp-file materialization has no Perl equivalent.
- command-substitution capture edges (the cmdsub family) — the Perl
  `qx{}` path + trailing-newline semantics.
- the `typeset`/`declare` attribute family.

Ask P1: **publish the failing-88 list as `docs/perl-backlog.md`** — each
entry: the construct, the bash behavior, the current Perl output, the
mismatch — the mechanical port list for the generator.

## 3. Type contract (IrType → Perl)

| IrType | Perl rendering |
|---|---|
| `Int` | scalar (Perl scalars are all SV — no unboxing win, but `use integer` or IV-preferring ops) |
| `Str` | scalar |
| `Any` | the store hash (Perl's natural dynamic store) |

Perl is dynamically typed — `IrType` has **no representation win** (no
compile-time types). Its value is for the **XS layer**: a var provably
`Int` can use XS integer ops / IV packing instead of string SVs. Low
priority; the lift's verdicts still serialize (A2) for uniformity.

## 4. Purity ladder

`PureCpu`/`Spawn`/`Emulable` (A3):
- `PureCpu` → inline Perl (the M8 native lowerings port as Perl
  expressions — `index`/`rindex`/`substr`/`s///` replace the JS
  `includes`/`slice`/`replace`; `$((...))` → native scalar arithmetic).
- `Emulable` → the generated inline helper functions.
- `Spawn` → `qx{}`/`system`/`open '|-'` — Perl's fork-capable model
  (fork/exec ✓ like C) — the spawn-parity wins apply (the grep→index
  lift, the echo|cut lift, the native-bc).

The M8 lowering ladder ports **directly** — Perl is a C-adjacent dynamic
language; `s///`, `index`, `substr` are native and fast.

## 5. Contract guarantees

### 5.1 Byte strings
Perl strings ARE byte strings ✓ — the U+F800 marker encoding decodes in
a 5-line `sh2_emit` shim (or, better: Perl handles raw bytes natively,
so the marker is only needed for the JSON/structural gate — a pure-Perl
path could bypass it entirely). Embedded NUL: Perl scalars hold NULs
natively ✓ (no truncation — unlike C). Perl is the *cleanest* consumer
of the byte-string contract.

### 5.2 Arithmetic
Perl `%` follows the dividend ✓ (matches bash). Integer division:
`int($a/$b)` truncates toward zero ✓. Overflow: Perl IVs are 64-bit;
bigger → NV (double) — the 2^53 edge exists (like JS) — the wasm bc
story (exact) applies. `0^0` etc.: the arith-contract (A5a) ports
directly; Perl's `**` is right-assoc ✓.

### 5.3 Hygiene + hoisting
Perl keywords: `my/our/local/if/elsif/else/while/for/foreach/sub/
package/use/require/...`. A6 extends the hygiene pass. `$if` is fine in
Perl (sigils disambiguate!) — the hygiene need is narrower than C/JS
(a var named `if` renders `$if` — legal). Hoisting: Perl needs `my`
declarations before use ✓ — the hoisting guarantee (decls at function
top) is directly satisfiable.

### 5.4 Statement vs expression
Perl has no expression statements (like C) — expression-position
commands need `do { ... }` blocks (Perl's statement-expression) or
helper subs. The C classification (§5.4) + a Perl `do{}` shape.

### 5.5 Control flow
Perl has native `last`/`next`/`return` ✓ (the break/continue/return
signals map to Perl's loop controls — cleaner than JS). RETURN-out-of-
captured-context: Perl's `last` with a label, or the signal-register
pattern. The loop sync tag → Perl is **always sync** (no async/await —
Perl's fork-based concurrency) — every loop can be the "sync" form; the
JS path's promise machinery is simply absent (a *simplification*).

## 6. The `sh2.*` namespace as data (A4)

The 39-callee spec → Perl: the generated program embeds an inline
runtime (helper subs) — the spec generates those mechanically. Purity/
signature data feeds the generator's lowering decisions (which Exec →
native).

## 7. The runtime port

| sh2.* family | Perl implementation |
|---|---|
| `exec`/`pipeline`/`redirect`/`capture`/`subshell`/`background` | `fork`/`exec`/`pipe`/`dup2` (real POSIX, C-style ✓ — Perl's model is fork-native) |
| `builtin` | inline helpers (the generated program's own subs) |
| `getVar`/`setVar`/`setArray`/... | a `%store` hash |
| `param`/`caseMatch`/`test` | `glob`/`qr//`/a test parser |
| `arith`/`arithEval`/`idiv`/`imod` | scalar ops + `int()` truncation |
| `bc*` | XS bignum OR `Math::BigInt` OR spawn — after correctness |
| `fs.*` | `open`/`unlink`/`mkdir`/`stat` |
| `positional`/`lastExit`/`functions.set`/`shoptState.set` | package globals |

## 8. Ask list

A1–A7 (shared) + Perl-specific:
- **P1**: publish the failing-88 backlog (the mechanical generator-port
  list).
- **P2**: port the ESTree path's lowering fixes construct-by-construct
  against P1 (the M8 lessons, Perl-rendered).
- **P3** (later): XS modules for the hot primitives (native arith/text/
  bc) — the Perl wasm-analogue — after the corpus is green.

## 9. Honesty

- Easy: Perl is C-adjacent + dynamic (fork/exec native, byte strings
  native, loop controls native, always-sync) — the *cleanest* spawning
  model of the interpreted backends.
- Hard: the 88 failures are real generator work (no shortcuts); the
  "debashc failed to generate code" refusals need parser/IR coverage,
  not just rendering.
- The XS layer is a *performance* investment — do it only if Perl
  runtime speed matters; correctness first.

## 10. Relationship to the other backends

The original backend + the shared IR's reason for existing (ir-design.md
was written for it). Its M8-port (P2) is the template the OTHER backends'
corpus-drive loops follow. The XS layer (P3) is the Perl analogue of the
wasm registry (Plan 12) — and Perl could also load the shared wasm
modules (Inline::Wasm) if that path ever matters.
