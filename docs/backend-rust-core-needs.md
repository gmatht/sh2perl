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

### 5.2b Variable storage: `let` where provable, ONE store elsewhere

The store is the fallback, not the rule — and it is exactly one store, not one
`thread_local!` per name (see §5.2b-i). A scalar becomes a plain Rust binding
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
let mut count = 0;          // was: __sh_vw_i("count", |v| v.set(0))
count += 1;                 // was: __sh_vw_i("count", |v| v.set(v.get() + 1))
let name = "world".to_string();
```

and what keeps it sound where bash's dynamic scoping or a subshell would be
violated: `value=1; show() { echo "$value"; }` keeps the store entry, and so
does `$( n=$((n+1)); … )`. Fixtures 11–16 in `fixtures/rust_idiom/` pin both
directions, and `22_vars_store` pins the store itself.

#### 5.2b-i The one store

Every unpromoted name lives in `__SH_VARS`, a `thread_local!`
`RefCell<__ShStore>`:

```rust
struct __ShStore { vars: BTreeMap<String, __ShVar>, ro_trip: bool, at_line: u32 }
struct __ShVar   { val: __ShVal, exported: bool, readonly: bool }
struct __ShVal   { s: String, int: Option<i64>, words: Option<Vec<Word>>,
                   ints: Option<Vec<i64>>, assoc: Option<BTreeMap<String, String>> }
```

- The key is the **shell name as a string literal** (`__sh_v_s("COUNT")`), so
  a call site the conversion missed fails to compile (identifier where `&str`
  is required) — the fixture gate catches it instead of a silent rebinding.
- One accessor per storage class, named the way the call site reads:
  `__sh_v_i/_s/_w/_n/_m`, `__sh_v_set_i/_s/_w/_n/_m`, the in-place
  `__sh_vw_i/_s/_w/_n/_m` (materialize → body → write back through
  `__sh_v_put`), `__sh_v_len`, `__sh_v_w_at/_n_at`, `__sh_v_m_get/_set/
  _remove/_keys/_values`.
- `__ShVar` carries the two attributes a shell keeps per name, so `export`
  (`__sh_v_export` — the flag makes every later write re-sync `std::env`, which
  a one-shot `set_var` did not), `readonly` (`__sh_v_mark_readonly` — a later
  write is refused) and `unset` (`__sh_v_unset`, refusing a readonly name) are
  store operations, not renderer bookkeeping.
- **A refusal prints bash's diagnostic, and its location is threaded in, not
  post-processed.** Bash writes `<prog>: line N: <name>: readonly variable`
  (and `<prog>: line N: unset: <name>: cannot unset: readonly variable`); the
  store writes exactly that. `<prog>` is `__sh_prog()` =
  `std::env::args().next()`, which is the same expression `$0` renders to, so
  the compiled program's `$0` and its diagnostic prefix are the same string by
  construction. `N` is `__sh_v_at(N)`, a marker the renderer emits at the head
  of a top-level statement whose text contains a checked store write, taken
  from the IR's `stmt_lines` markup; `__sh_v_where()` reads it back when a
  refusal fires (0 = unknown, and the message falls back to the bare
  `<prog>:` prefix rather than a wrong line). The marker is anchored at the
  statement's head because emitting a statement can collapse a dead
  `__SH_RC.store` below its start, which would invalidate a splice index
  computed before the emission. The accessor API and the store shape are
  otherwise untouched.
- **Coverage of that diagnostic.** `22_vars_store` marks itself
  `# stderr-exact:` and the harness then compares stderr byte for byte (and
  runs the Rust binary with argv[0] = the fixture path, which is the string
  bash's `$0` is) in both the debug and the `-O` build, alongside the
  `$?`-after-refusal check. The fuzz grammar has the same case (readonly write
  + `unset`) and the net compares the `(stdout, stderr, rc)` triple for every
  program, at seed 1 in 13 of 60 programs. **Not matched:** `stmt_lines` marks
  top-level commands only, so a refusal inside a function body, an
  `if`/`for`/`while` body or a subshell command list prints the bare
  `<prog>:` prefix (or an enclosing statement's line, if its marker already
  ran) where bash prints the nested statement's line. Closing that needs
  per-command line markup from the parser, which this pass does not touch;
  what remains is a *location*, not a message or a status.
- `ro_trip` is the `$?`-adjacent half: a refused write sets it and the next
  status write consumes it (`__SH_RC.store(if __sh_v_rc_trip() { 1 } else { 0 } …)`),
  so `$?` is 1 for the failed statement and 0 after the next command, like
  bash.
- A scope saves the **entry** (`__sh_v_save` / `__sh_v_restore_one`), so a
  subshell restores value *and* flags in one place.
- The helper is sliced (`vars` core + `vars_i/_s/_w/_n/_m/_len`) and pruned per
  program; `__SH_VARS` is emitted only when a store call exists. That keeps the
  `-D warnings` build clean without a `#[allow(dead_code)]`: a fixed monolithic
  helper would carry unused variants/methods, which rustc rejects as dead code.

Measured over the fixture set (`fixtures/rust_idiom/expected/*.rs`): per-
variable `static __SHV_…` **6 → 0**, store call sites naming a key
**21 → 48**, programs carrying the store **7 of 22** (one `__SH_VARS` each),
emitted lines **2965** over all 22 fixtures (`22_vars_store`: 425 lines, 26
store call sites). The cost is helper lines: ~210 per program that uses it
(the `$0`/line plumbing for the refusal diagnostic is inside that budget), so
emitted lines went **1371 → 2364** over the 21 pre-existing fixtures. The per-variable static it
replaced was one line each, so this is a shape/robustness win, not a line-count
one; the census prints both numbers so the trade stays visible.

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
fixture (24 promoted / 46 store accesses over 22 fixtures) from the same
verdict the renderer uses, and fails a fixture when a promoted name has no
binding in the emitted text or when a per-variable `static __SHV_…`
reappears. What stays stored is documented in `fixtures/rust_idiom/README.md`
with the reason per name.

### 5.2c Post-render specialization and the emitted-code contract

- **printf specialization is a classification, not a spot fix.** A call
  site's format is classified at render time by `classify_printf_format`
  (`PrintfPlan { fmt_src, text, args }`) and decomposed by
  `literal_printf_replacement` only when the whole call is provably
  equivalent to the interpreter: a literal format, its escape conversions
  resolved at render time (`printf_unescape`), `%%` → one `%`, a literal
  `{`/`}` doubled (a brace is not a format specifier; an undoubled one would
  emit an invalid `format!` string), and every `%s` → one `{}` **matched
  one-for-one with the arguments the site actually passes** (bash pads,
  truncates and quotes nothing for `%s`). A slotless format collapses to a
  single string only when the site passes no arguments at all (`&[]` is an
  empty list, not an argument) — bash re-applies a slotless format once per
  argument group.
- **The fallback conditions are named in `PrintfFallback`** so a refusal is
  a decision the census counts: `NotLiteral` (a variable, an interpolation,
  a capture — nothing to decompose), `Unsupported` (`%d`/`%i`/`%u`/`%o`/
  `%x`/`%c`/`%q`, widths, precision, `*`, a lone `%`), `BackslashB` (`%b`
  escapes come from the argument at run time), `StopOutput` (`\c` truncates
  the output), and `Arity` (fewer/more arguments than `%s` slots — the
  interpreter pads with empties or repeats the format). When in doubt the
  site falls back; a wrong specialization is a semantic regression, so the
  accepted set stays narrow and the guard conservative.
- **The interpreter is kept, not deleted**, because those shapes still need
  it: `%d`/`%u`/`%c` conversion, `%b`'s run-time escape expansion,
  widths/precision/flags, `\c`, arity padding/repetition, and any format
  only known at run time. Its dependency cluster
  (`__sh_printf`, `__sh_printf_b`, `__sh_atoi`, `__sh_atou`, `__sh_atof`,
  `__sh_echo_esc`, `__sh_q_printf`) is pruned per program: `19_printf_literal`
  emits none of them (only `__sh_print_words`), while `20_printf_dynamic`
  emits all seven.
- **Census, per fixture and total** (`cargo run --bin rust_fixtures`; `pfx`
  = specialized sites, `pfi` = calls left on `__sh_printf`, `pfl` =
  interpreter body lines): `07_redirect` 44 lines / 1 / 0 / 0,
  `19_printf_literal` 31 lines / 4 / 0 / 0, `20_printf_dynamic` 235 lines /
  0 / 1 / 114, the other 18 fixtures 0 / 0 / 0 — **TOTAL 21 fixtures, 1371
  lines, pfx 5, pfi 1, pfl 114**. Before/after, measured rather than
  remembered: before this pass (commit `e4fe8c187`) the census had no printf
  columns, 0 of 18 fixtures called `__sh_printf` (`git grep -c` over the
  expected files) and 0 interpreter lines were measured — yet the 114-line
  body was still reachable by any literal-format site that fell back (a
  slotless `printf '100%%\n'` used to). After: 5 specialized / 1 interpreted
  / 114 lines, all 114 in the dynamic fixture; `20` (235 lines) versus `19`
  (31 lines) is what keeping the interpreter costs.
- **Fixtures pin both sides**: `19_printf_literal` is the specialized path
  (four literal sites including `%%` and literal braces; zero
  `__sh_printf`, identical output), `20_printf_dynamic` is the fallback (a
  run-time-assembled format must keep *and call* the interpreter, so an
  over-eager specialization fails the gate instead of changing output).
  `rust_fuzz` renders every class (`%%`, a literal brace, `%d`, a run-time
  format), so the classification is exercised on scripts nobody hand-wrote.
  Two real bugs fell out of making the rule explicit: a slotless literal
  format with an empty argument list used to fall back (`printf '100%%'`
  shipped the interpreter), and a literal brace in a format produced an
  invalid `format!` string.
- **No `echo -e` means no `esc` parameter**: the two-argument
  `print_words_noesc` helper is emitted and `drop_esc_arg` removes the
  literal `false` from every call site. `07_redirect` used to pass
  `, true, false` to a helper that did `let _ = esc;`; it now reads
  `__sh_print_words(&[…], false)` and still emits 44 lines.
- Generated code carries no bare `.unwrap()` and no blanket `#[allow(...)]`
  for the codegen-noise lints (`dead_code`, `non_upper_case_globals`,
  `unused_variables`/`_parameters`/`_imports`, `unused_mut`, needless
  borrows). Measured on the journal fixtures: `#[allow(` 9 → 0,
  `non_upper_case_globals` 8 → 0, `dead_code` 1 → 0, `.unwrap()` 2 → 0,
  `.expect(`/`panic!`/`unreachable!` 0 → 0, emitted lines 553 → 547. The
  *decided dispositions*:
  - Unpromoted variables are **store keys, not statics** — `__sh_v_s("COUNT")`
    takes the shell name as a string literal, so there is no `static`
    identifier that could need `non_upper_case_globals` and no per-name
    declaration at all. A promoted local keeps the shell spelling (one name →
    one storage class, which the post-render leak guard and `-Oz` golf rely
    on); the store key is derived in one place (`tls`), so every accessor and
    the body agree.
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
  differential net `cargo run --bin rust_fuzz` (seeded; 60 generated programs
  by default, each run under `bash` and under the rendered+compiled Rust in
  BOTH a debug and a `-O` build, comparing stdout, stderr and the exit status;
  `--script` replays a minimized case and the summary line reports
  seed/programs/divergences). Its grammar includes a capture glued to literal
  text in one unquoted word (`x=$(cmd)z`, `x=z$(cmd)`, `x=$(cmd)-$(cmd)`) —
  the shape whose parsing dropped the literal until `append_plain_text` /
  `merge_expansion_into_word` learned to turn any single-expansion word into
  interpolation parts. It also generates **pipelines** (an all-external
  multi-stage chain, a `builtin | external…` chain, a chain into `$()` and into
  a word, and the one redirection-stage fallback), **shell output
  interleaved with a child's inherited fd 1**, which is the shape that keeps
  the flush discipline honest, and a **refused readonly write** (`readonly
  R="$b"`, then `R="$b-x"` and `unset R`, compared including stderr, with
  argv[0] aligned to bash's `$0`). Clippy is a
  required check, not an optional extra: if the component is missing the gate
  prints `HARD GAP: clippy did not run` and exits non-zero. It already pays
  for itself — `clippy::print_with_newline`, `clippy::format_in_format_args`
  and `clippy::useless_vec` were all live in the emitted code and are now
  fixed in the emitter (`println!("x")` instead of `print!("x\n")`, one
  `format!` instead of a nested pair, `&[a, b]` instead of `&vec![a, b]`).

### 5.2d Pipelines and stdout: the emitted program owns the process wiring

A pipeline used to be reconstructed as command text and handed to
`bash -c`. Every stage whose argv this renderer can spell is now spawned by
the emitted program itself and connected with real OS pipes (`__sh_pipe`):
`a | b`, a multi-stage chain, a stage with arguments/globs it expands, and a
chain feeding `$()` or a following word. Real pipes — not a byte buffer
between stages — are what keeps SIGPIPE working (`yes | head`), `__SH_PIPESTATUS`
gets one entry per stage, and `$?` stays the last stage's status, as in bash.

The fallback is kept for the shapes whose argv cannot be spelled at render
time, and the emitter names them: a stage carrying a redirection or heredoc, a
subshell or `&&`/`||` stage, an exec whose command word is not a plain literal
(`"$cmd" …` could name a builtin), and one carrying per-command env
assignments. Those still become one `bash -c` pipeline, because there the
shell text *is* the command. `21_pipeline_interleave` is the fixture that pins
both halves: `03_capture`, `cat … | tr … | wc -l`, `printf … | sort | head`
and pipeline-into-`$()` shapes render with no `bash -c`, while the
redirection-stage shape still does and still matches bash byte-for-byte.

Interleaving discipline: shell output goes out through `print!`/`println!`
(Rust's line-buffered stdout), so a child that inherits fd 1 is preceded by an
explicit flush — `printf 'pre-'` without a newline sits in the buffer, and
without the flush the child's bytes would come out first, which bash never
does. Both `__sh_spawn` and `__sh_pipe` flush before spawning when the child
inherits fd 1; the fixture fails if that flush is removed (verified by removing
it).

The widening also exposed a real bug in the emitter's own text passes: the
`pass-4` dead-status-store collapse removed the previous line whenever it held
a `__SH_RC.store(...)`, on the assumption that the payload was side-effect
free. It is not for `__SH_RC.store(__sh_pipe(…))` — collapsing it deleted the
pipeline itself. The collapse now requires a call-free payload
(`rc_store_payload_pure`).

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
| `exec`/`pipeline`/`redirect`/`capture`/`subshell`/`background` | `std::process::Command` + real OS pipes: a pipeline the renderer can spell is spawned stage by stage in the emitted program itself (`__sh_pipe`, no `bash -c`), the rest keep the shell-text fallback |
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
