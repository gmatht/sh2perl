# Rust Backend: What It Wants From Core

Status: DRAFT. Mirror of `backends/c/docs/backend-c-core-needs.md`. Rust
is the **home language**: otranspilerl-cli IS Rust, the IR types are already
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
| B: **Rust API (in-process `IrProgram`)** | types already `pub`; a renderer crate calls `sh2perl core::shir::ast_to_ir` | **today** — the cheapest path |
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
bash exactly. Overflow: Rust's *release* builds wrap two's-complement on
overflow — exactly bash's 64-bit wrap — while *debug* builds panic.
The renderer therefore emits plain `+ - * /` for source arithmetic (what
a Rust programmer writes) and reserves `wrapping_*` for arithmetic it
emits as an explicit loop, where the wrap is the loop's semantics rather
than an accident: `__sh_pow` uses `wrapping_mul` for that reason.
Consequence worth stating plainly: generated programs are expected to be
built in release mode (as the `-O` pipeline does); a generated program
compiled with `-C debug-assertions=on` will panic on an overflow that
bash would have wrapped. `wrapping_*` on every expression was the
earlier rule here; it was dropped because it is both unreadable and not
what `rustc` needs in the shipped artifact. An overflow that no size
analysis can bound is instead handled where it is *observable* — a
constant-folded arithmetic expression that cannot fit an `i64` is
rendered through `num_bigint`, producing the exact mathematical value
(`9223372036854775807 + 1` → `9223372036854775808`) where bash would
print the wrapped `-9223372036854775808`. That is a known, deliberate
divergence for constants that overflow at render time; it is the one
shape in `fixtures/rust_idiom/` whose output is not bash-identical.
Right-assoc `**` → `__sh_pow` (wrapping semantics). NaN guard →
`str::parse::<i64>()` error → the whole test is false ✓.

### 5.2b Variable storage: `let` where provable, the store elsewhere

The store is the fallback, not the rule. A scalar becomes a plain Rust binding
when all of these hold:

- every mention sits in ONE scope (`main`, or one user function);
- its first mention is a top-level write in that scope (that is where the
  `let` lands) and it is read at least once;
- no mention is *pinned*: a closure body (`Capture`/`Lambda`/`Arrow`), a
  separate-process construct (`( )`, `&`, a pipeline) or opaque asm;
- no construct can reach the name dynamically: `eval`/`source` bail the whole
  analysis; `export`/`readonly`/`unset`/`read`/`mapfile`/`getopts`/`set`/
  `trap`/`printf -v` poison the name; `declare -n`/`-g` bail it; arrays,
  assoc arrays and shell specials never promote.

The shared `Use::risky` bit is deliberately not consulted — it models a
renderer that lowers an argument through a *capturing* file-scope helper, and
this renderer splices every read inline. `Use::pin` is the half that applies.

That is what makes the emitted shapes match what a Rust programmer writes:

```rust
let mut count = 0;          // was: __SHV_count.with(|v| v.set(0))
count += 1;                 // was: __SHV_count.with(|v| v.set(v.get() + 1))
let name = "world".to_string();
```

and what keeps it sound where bash's dynamic scoping or a subshell would be
violated: `value=1; show() { echo "$value"; }` keeps the static, and so does
`$( n=$((n+1)); … )`. Fixtures 11–16 in `fixtures/rust_idiom/` pin both
directions.

#### Promotion is a refusal, not a heuristic

`compute_local_vars` promotes a name only when every one of these fails to
fire: undeclared (env read), first mention not a top-level write, never
read, mentions in more than one scope (dynamic scoping), a pinned mention
(capture/subprocess/thread), an array/assoc store, a shell special. The
whole-program poison list is `eval`, `.`/`source`, `declare -n` (nameref),
`${!name}` indirection (a `!`-prefixed name — indirection reads a name
computed at run time) and any non-literal `getVar`/`setVar` name; the
per-name poisons are `export`/`readonly`/`declare -g`/`unset`/`read`/
`mapfile`/`getopts`/`set`/`trap`/`printf -v NAME`/`let` and the `NAME=value`
words those take. The harness reports the promoted-vs-stored split per
fixture (22 promoted / 5 stored over 18 fixtures) from the same verdict the
renderer uses, and fails a fixture when a promoted name has no binding in
the emitted text. What stays stored is documented in
`fixtures/rust_idiom/README.md` with the reason per name.

### 5.2c Post-render specialization and the emitted-code contract

- A `printf` whose format is a literal template of text, `%s` and `%%` is
  turned into a `format!` at the call site, which retires the `__sh_printf`
  interpreter (and its `atoi`/`atof`/`printf_b` cluster) from programs that
  only ever format literals: `07_redirect.rs` went from 261 emitted lines to
  44. Computed formats and the conversions the pass does not model (`%d`,
  widths, `%b`, `%q`, `%c`) keep the interpreter.
- Generated code carries no bare `.unwrap()` and no blanket `#[allow(...)]`
  for the codegen-noise lints (`dead_code`, `non_upper_case_globals`,
  `unused_variables`/`_parameters`/`_imports`, `unused_mut`, needless
  borrows). Measured on the journal fixtures: `#[allow(` 9 → 0,
  `non_upper_case_globals` 8 → 0, `dead_code` 1 → 0, `.unwrap()` 2 → 0,
  `.expect(`/`panic!`/`unreachable!` 0 → 0, emitted lines 553 → 547. The
  *decided dispositions*:
  - Storage globals are emitted under **uppercase** names
    (`static_ident`: `__SHV_` + the mangled base uppercased, `_2`/`_3`
    suffix on a case-only collision), so `non_upper_case_globals` never
    fires and the declaration reads like hand-written Rust. The name is
    derived in one place, so `tls`, the declaration, the post-render leak
    guard and `-Oz` golf all agree.
  - A poisoned lock still yields its data
    (`unwrap_or_else(|e| e.into_inner())`), the capture slot defaults to the
    empty buffer, and a non-hex digit reads as 0 (`unwrap_policy`).
  - The two remaining panicking shapes are gone by construction:
    `split_first` uses `let Some(..) = .. else { .. }` (the compiler sees the
    empty case, so there is nothing to unwrap), and an unopenable redirect
    target is `.or_else(|_| File::open("/dev/null")).ok()` — an unset sink
    discards the same bytes `/dev/null` would, and cannot abort. The sink
    detectors key on the set prefix, not on `Some(`, so the panic-free shape
    cannot be mistaken for "no sink".
  - `clippy::print_literal`/`print_with_newline`: a folded constant echo of a
    lone string literal prints the literal as its own format string
    (`println!("done")`, or `print!` when it ends in a newline) instead of
    `println!("{}", "done")`; a literal containing `{`/`}` keeps the
    placeholder form.
  - Indexing inside the helpers is guarded by construction: every
    `p[i]`/`bytes[i]` is inside a `while i < len` loop or preceded by an
    explicit `i < len` test on the same expression, and slice boundaries come
    from `char_indices`/`is_ascii_*` checks, so no helper can panic on a
    boundary or an index.
- Two `#[allow]` shapes remain emittable but never fire on the fixtures
  (census 0), each recorded at its emission site: `non_snake_case` for a
  promoted local whose shell name has an uppercase letter (the local keeps
  the shell spelling so one name maps to one storage class), and
  `unused_assignments` for a promoted local whose first write is
  control-flow dependent (bash zero-initializes; `let mut x;` is not
  provably assigned on every path).
- Every word list (the `__sh_print_words` argument, `__sh_wq`, `__sh_cat`,
  `__sh_split_ifs`, `__sh_lines`, `__sh_read_fields`, `__sh_glob`,
  `__sh_brace`, `__sh_aindex`, `__sh_printf`'s arguments) and the text-array
  store carry `Word = std::borrow::Cow<'static, str>`, emitted once in the
  prelude and pruned when unused. A literal word is `vec!["done".into()]` and
  stays `Cow::Borrowed` (no allocation, no `.to_string()`); a runtime value is
  adopted from the `String` it already built. The alias is what lets one call
  site hold both: a bare `&str` element and an owned `String` element cannot
  live in the same `&[Vec<S>]` under a generic `S`.
- Numbers stay numbers. A value the source compares or does arithmetic with
  is carried as `i64` in the emitted program: a numeric-comparison operand
  (`[ "$c" -gt 0 ]`, `-eq … -ge`) resolves to the number when its home is
  already `i64` (`TestParser::operand_numeric`), instead of being stringified
  into `c.to_string().trim().parse::<i64>()`. `read_num_of_str` parses a text
  home *inside* the borrow (no `borrow().clone()` copy) or in place for a
  promoted `String` local. The remaining text→number conversions are
  semantic, not detours: an element of a text array read by `$((sum + x))`
  (`05_array`), and the `RefCell<String>` home of an `export`-poisoned name
  (`15_export_poison`) — `export` reaches the name as a string, so there is no
  `i64` to carry. `__sh_atoi` is not emitted by the shell renderer at all
  (census 0), and `borrow().len() as i64` is the array length read, already
  numeric.
- The gate for all of this is `cargo run --bin rust_fixtures -- --check`
  (stale check, A1-ingress parity, `rustc -D warnings` in debug *and* `-O`,
  behaviour vs `bash`, **clippy `-D warnings` in debug *and* `-O`**, idiom
  census, which now also counts `abort`s and fails the gate on a single
  generated `.unwrap()`/`.expect()`/`panic!`/`unreachable!`) plus the
  differential net `cargo run --bin rust_fuzz`. Clippy is a
  required check, not an optional extra: if the component is missing the gate
  prints `HARD GAP: clippy did not run` and exits non-zero. It already pays
  for itself — `clippy::print_with_newline`, `clippy::format_in_format_args`
  and `clippy::useless_vec` were all live in the emitted code and are now
  fixed in the emitter (`println!("x")` instead of `print!("x\n")`, one
  `format!` instead of a nested pair, `&[a, b]` instead of `&vec![a, b]`).

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
| `arith`/`arithEval`/`idiv`/`imod` | `i64` with plain ops (`wrapping_*` only in the emitted `__sh_pow` loop — see §5.2) |
| `bc*` | the SHARED wasm module (a Rust runtime loads `bcwasm.wasm` via `wasmtime`/`wasmer` — or compiles the same `number.rs` in — it's the same crate!) |
| `fs.*` | `std::fs` |
| `positional`/`lastExit`/`functions.set`/`shoptState.set` | module-level statics |

## 8. Ask list

A1–A7 (shared) + Rust-specific:
- **R1**: the arith contract must say *which* overflow semantics the
  target owes. For Rust the answer is the native one: release builds wrap
  exactly as bash does (§5.2), so the emitted code uses plain operators;
  `wrapping_*` is applied only where the renderer itself writes the loop
  (`__sh_pow`). Generated programs are release-mode artifacts; a debug
  build of one may panic where bash wraps.
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
  keeping emitted *shape* idiomatic (dead `__SH_RET` stores, single-use
  `.clone()`, `println!("{}", format!(..))`) — the `idiom_peephole` pass
  plus the checked-in fixtures in `fixtures/rust_idiom/` are the guard
  against that regressing.
- The wasm angle: a Rust backend's runtime can compile to
  `wasm32-unknown-unknown` and *be* a module in the shared registry —
  the strongest wasm story of the static backends.

## 10. Relationship to the other backends

The reference static backend — Zig's sibling (same native semantics,
comptime→types), C's successor (safety + the same POSIX model), and the
author of the A1 serializer every other backend consumes. The wasm
registry (Plan 12) is shared with JS/Zig/Lua/Java.
