# Rust backend idiom fixtures

Small shell programs whose *rendered Rust* is checked in, so the shape of
the backend's output can be reviewed and diffed like ordinary code, and a
differential fuzz net that tries the same shapes at random.

    src/*.sh          the shell input
    expected/*.rs     what `rust_backend::shir_to_rust` emits for it

Re-render after a backend change, then look at the diff:

    cargo run --bin rust_fixtures            # rewrite expected/*.rs
    git diff fixtures/rust_idiom/expected

## The gate

    cargo run --bin rust_fixtures -- --check
    cargo run --bin rust_fuzz              # 60 programs, seed 1, debug + release

`--check` runs six checks per fixture, in this order:

1. **stale** — the fresh render equals the checked-in `expected/*.rs`.
2. **A1 parity** — the same source rendered through the A1-JSON ingress that
   `shir_render --target rust` uses (`ast_to_ir` → A1 JSON → IR → shared
   transforms → structuring passes → renderer) produces the *same text*. This
   is what keeps the harness's mirror of the render path honest: a backend
   change the mirror missed would otherwise leave the gate green.
3. **rustc** — the program compiles under `rustc --edition 2021 -D warnings`
   in BOTH a debug and a `-O` build. Debug matters: generated arithmetic uses
   plain operators (release wraps like bash, debug panics on overflow), so a
   fixture that exercises that divergence must say so in its own source with
   `# debug-overflow: <reason>`; any other debug panic is a failure.
4. **behaviour** — each build runs in a scratch directory seeded with the same
   three files the `bash` run sees, and stdout, exit status and every file it
   creates must equal `bash <fixture>`.
5. **clippy** — every built fixture is linted (`cargo clippy` in a one-file
   scratch package) in BOTH the debug and the `-O` profile with
   `-D warnings`; any finding fails the fixture. This is the half of the
   metric that catches families nobody named by hand —
   `clippy::print_with_newline`, `format_in_format_args`, `useless_vec` all
   came out of it and are fixed in the emitter now. A missing component is a
   **hard gap**, not a skip: the summary prints `HARD GAP: clippy did not
   run` and the gate exits 2. (`rustup component add clippy`.)
6. **census** — the idiom counters (`let` bindings and `thread_local!`
   store statics — the promoted-vs-stored split — `pfx`/`pfi` specialized
   vs interpreted printf sites, `pfl` interpreter lines, `.clone()`,
   `.to_string()`, `#[allow(`, `abort`, `Command::new("bash")`, emitted
   `__sh_*` helpers, lines) are printed per fixture and totalled.
   "Idiomatic" is a number that has to keep moving down, not a claim in a
   comment. A non-zero `abort` count is a hard failure in every mode: a
   generated program may not contain `.unwrap()`, `.expect(`, `panic!` or
   `unreachable!`.

## The word vector type

Every word list — `__sh_print_words`, `__sh_wq`, `__sh_cat`, `__sh_split_ifs`,
`__sh_lines`, `__sh_read_fields`, `__sh_glob`, `__sh_brace`, `__sh_aindex`,
`__sh_printf`'s arguments, and the text-array store — carries `Word`, a
`std::borrow::Cow<'static, str>` alias emitted once in the prelude:

    type Word = std::borrow::Cow<'static, str>;

so a word that is a compile-time literal is emitted as `vec!["done".into()]`
and stays *borrowed* (`Cow::Borrowed`) with no allocation and no
`.to_string()`, while a runtime value (a capture, an interpolation, an
integer read) is adopted as `Cow::Owned` from the `String` it already built.
The alias is pruned from programs that use no word list.

What still carries `.to_string()`, and why — the census total went **21 → 11**
across the fixtures (13 → 11 in this round), and none of the 11 is a
word-vector element or a numeric comparison operand:

| where | n | why it must own |
|---|---|---|
| literal bound to a `String` local (`let name = "world".to_string()`) | 3 | `analyze_var_borrowable` withheld, so the binding's home is `String` — a promoted-binding question, not a word-list one |
| literal passed to a `String` parameter (`greet("there".to_string())`) | 2 | the parameter type comes from the function's `FnSig`, which is `String` |
| `String::from_utf8_lossy(&cap).to_string()` | 2 | the text is borrowed from a temporary capture buffer; it must be owned before the buffer drops |
| BigInt → decimal (`08_bigint`) | 1 | an out-of-`i64` value has no text form other than this |
| a numeric value entering a word list (`n.to_string().into()`) | 1 | the value is an `i64`; text is the semantic conversion bash performs at that boundary |
| the `String` home of an export-poisoned variable (`15_export_poison`) | 2 | the store is `RefCell<String>`; `export` reaches the name, so the binding cannot be a typed `let` |

## Numeric text round-trips

A value that the shell compares or does arithmetic with must stay *numeric*
in the emitted program. Two shapes were leaking it:

* **comparison operands** — `[ "$c" -gt 0 ]` rendered the operand through its
  `String` form and then appended `.trim().parse::<i64>()`:
  `c.to_string().trim().parse::<i64>() > 0` for a `let mut c: i64`. The test
  parser now resolves a numeric-comparison operand (`-eq … -ge`) to the
  number when the home is already `i64` (`TestParser::operand_numeric`), so
  `16_while_loop` / `17_overflow` are `while { let __b = c > 0; … }`.
* **text homes parsed for arithmetic** — `read_num_of_str` copied the
  `String` out of the store only to throw it away. It now parses inside the
  borrow (`__SHV_X.with(|v| v.borrow().trim().parse::<i64>().unwrap_or(0))`)
  or in place for a promoted `String` local; only capture slots, loop
  bindings and borrowable `&str` locals keep the owned read.

Census, measured from the committed `expected/*.rs` (before = this round's
starting HEAD):

| counter | before | after |
|---|---|---|
| `.to_string()` | 13 | **11** |
| `.clone()` | 5 | **4** |
| `trim().parse::<i64>()` | 4 | **2** |
| `__sh_atoi(` | 0 | 0 |
| `borrow().len() as i64` | 1 | 1 |

Per fixture, `to_string`/`clone`/`parse` (`05`, `15`, `16`, `17` are the ones
that moved): `01 1/0/0`, `02 1/0/0`, `03 1/1/0`, `04 1/0/0`, `05 0/0/1`,
`06 0/0/0`, `07 0/0/0`, `08 1/0/0`, `09 0/0/0`, `10 0/0/0`, `11 1/2/0`,
`12 1/0/0`, `13 0/0/0`, `14 2/0/0`, `15 2/1/1`, `16 0/0/0` (was `1/0/1`),
`17 0/0/0` (was `1/0/1`).

The two surviving parses, and the one surviving `len() as i64`:

* `05_array` — `sum += x.trim().parse::<i64>()` where `x` iterates a *text*
  array (`["1", "2", "3"]`). The element is a `&str` by construction and
  bash's `$((sum + x))` accepts any numeric *text*; parsing is the semantic
  operation, not a detour through a number that already existed.
* `15_export_poison` — `__SHV_COUNT.with(|v| v.borrow().trim().parse::<i64>())`.
  `export COUNT` reaches the name as a string, so the home is
  `RefCell<String>`; there is no `i64` to carry.
* `05_array` — `__SHV_arr.with(|v| v.borrow().len() as i64)` is already the
  numeric length (`Vec::len()` promoted once); it is the fixed point of the
  array-length read, not a stringify-reparse.

## Promotion: the safe slice

Most variables are a bare Rust binding, not a `thread_local!` static. The
decision is a structural verdict (`compute_local_vars` in
`src/rust_backend.rs`), never a heuristic, and every rule is a refusal:

A name is promoted only when ALL hold:

1. it is *declared* (a write exists — an undeclared name reads the process
   environment, and a `let` would shadow it with `""`);
2. its first mention is a top-level write in the owning scope (that is where
   the `let` lands) and it is read at least once;
3. every mention — read or write — lives in ONE scope: `main`, or one
   function body. Two scopes means bash's dynamic scoping can connect them,
   so it stays in the store;
4. no mention is *pinned* (a capture block, a separate process, a thread);
5. it is not an array or an assoc array (their storage is a `Vec`/map);
6. it is not a shell special (`$?`, `$#`, `$@`, `$$`, …).

Whole-program vetoes (every name keeps its store) — the poison list:

| construct | why |
|---|---|
| `eval`, `.`/`source` | can read and write any name at run time |
| `declare -n` / nameref | aliases storage by RUNTIME name and can be retargeted |
| `${!name}` | indirection reads a name computed at run time |
| any `getVar`/`setVar` whose name argument is not a literal | the name is computed |

Per-name poisons (that name keeps its store, the rest of the program may
still promote): `export`, `readonly`, `declare -g`, `unset`, `read`,
`mapfile`/`readarray`, `getopts`, `set`, `trap`, `printf -v NAME`, `let`,
and any `NAME=value` word handed to those builtins.

Measured per fixture (`cargo run --bin rust_fixtures`), `let` / `store`:

| fixture | let | store | the stored names, and why |
|---|---|---|---|
| 01, 02, 04, 06, 08, 09, 10, 11, 12, 16, 17 | 2,1,1,3,1,1,1,3,2,1,3 | 0 | — |
| 03_capture | 1 | 0 | — |
| 05_array | 1 | 1 | `arr` is an array (`Vec<Word>` store) |
| 07_redirect | 0 | 0 | no variables |
| 13_dynamic_scope | 0 | 1 | `value` is written in `main` and read in `show` — two scopes |
| 14_capture_pin | 1 | 1 | `n` is written inside `$( … )` (a subshell) |
| 15_export_poison | 0 | 1 | `COUNT` is exported (reachable by name from a child) |
| 18_eval_poison | 0 | 1 | `eval` can reach `n` at run time |
| **TOTAL** | **22** | **5** | |

11_promote and 18_eval_poison are the two halves of the boundary written
down as programs: `count`, `name`, `total` and `n` in 11 become `let`
bindings (`let mut count = 0;`, `for n in [1, 2, 3]`), while 18's
`n=0; n=$((n + 1)); eval 'n=$((n + 10))'; echo "$n"` keeps `__SHV_N` and
prints the same `11` as bash. Loosening the slice must fail 18, not change
what it prints.

The numbers are measured, not asserted: the harness asks the renderer for
`promoted_names` (the same `compute_local_vars` call the emission makes) and
counts `static __SHV_` declarations in the emitted text, then **fails the
fixture** if a promoted name has no `let`/`let mut`/`for … in` binding in
that text (skipped for fixtures that carry a `TODO(unsupported)` marker,
where the construct was not lowered at all).

Still stored, and the next widening if it ever happens: a variable whose
only write sits inside a branch (`if c; then x=1; fi; echo "$x"`) stays in
the store — a `let` there would need a hoisted default initializer, which
rustc flags as an unused assignment for the path that always writes first.

## printf: decomposing a literal format

A `printf` call site is decided at **render** time. `classify_printf_format`
(in `src/rust_backend.rs`) turns the format literal into
`PrintfPlan { fmt_src, text, args }`, and `literal_printf_replacement`
emits a `format!` at the call site only when the whole call is provably
equivalent to the interpreter:

- the format is a Rust string literal the renderer already produced (so the
  site's format is known statically, not built at run time);
- its **escape conversions** happen at render time (`\n`, `\t`, `\\`,
  `\NNN` … — `printf_unescape`), `%%` becomes one literal `%`, and a literal
  `{`/`}` is doubled in `fmt_src` — a brace is *not* a format specifier, and
  an undoubled one would emit an invalid `format!` string;
- every `%s` becomes one `{}`, and the number of `%s` slots equals the number
  of arguments the site actually passes (bash pads, truncates and quotes
  nothing for `%s`);
- a format with no `%s` is only collapsed to a single string when the site
  passes **no** arguments (`&[]` is an empty list, not an argument) — bash
  re-applies a slotless format once per argument group.

Anything else FALLS BACK to `__sh_printf`, and the interpreter is then still
emitted in that program. The reasons are named in `PrintfFallback`, so a
refusal is a decision the census can count, not a silent miss:

| condition | `PrintfFallback` |
|---|---|
| the format is not a literal (a variable, an interpolation, a capture, anything computed) | `NotLiteral` |
| a conversion or flag the specializer does not model exactly: `%d`/`%i`/`%u`/`%o`/`%x`/`%c`/`%q`, widths, precision, `*` (the argument's numeric/char conversion is the interpreter's job) | `Unsupported` |
| `%b` — backslash escapes come from the *argument* at run time, so they cannot be resolved at render time even for a literal format | `BackslashB` |
| `\c` in the format — it truncates the output and drops everything after it | `StopOutput` |
| arity mismatch: more/fewer arguments than `%s` slots (the interpreter pads with empties or repeats the format) | `Arity` |
| an unparsable format (a lone `%` before a non-conversion) | `Unsupported` |

When in doubt the site falls back; a wrong specialization is a semantic
regression, so the accepted set stays narrow and the guard conservative.
The fallback interpreter is kept whole (not deleted) precisely because these
shapes still need it: `%d`/`%u`/`%c` conversions, `%b`'s run-time escape
expansion, widths/precision/flags, `\c`, arity padding/repetition, and any
format only known at run time. `__sh_printf`'s dependencies
(`__sh_atoi`, `__sh_atou`, `__sh_atof`, `__sh_printf_b`, `__sh_q_printf`)
are pruned per program, so a program that never falls back carries none of
them.

### Pinned by fixtures

- `19_printf_literal.sh` — four literal sites (`'name=%s\n'`, `'%s-%s\t%s\n'`,
  `'100%%\n'`, `'braces {a} and {b} stay literal\n'`). The emitted file has
  **no** `__sh_printf` and no interpreter helper at all, and prints exactly
  what bash prints.
- `20_printf_dynamic.sh` — the format is assembled at run time
  (`fmt="$piece/$piece\n"`); the site must keep **and call** the
  interpreter, so an over-eager specialization fails the gate instead of
  silently changing output.
- `07_redirect.sh` — one literal site inside a redirect. It no longer carries
  the always-false `esc` bool: the helper is `fn __sh_print_words(ws, nl)`
  and the call reads `__sh_print_words(&[…], false)`.

### Measured

The census prints, per fixture and in total, `pfx` (sites specialized),
`pfi` (call sites left on `__sh_printf`), `pfl` (lines of interpreter body in
the emitted file). From `cargo run --bin rust_fixtures`:

| fixture | lines | pfx | pfi | pfl |
|---|---|---|---|---|
| 07_redirect | 44 | 1 | 0 | 0 |
| 19_printf_literal | 31 | 4 | 0 | 0 |
| 20_printf_dynamic | 235 | 0 | 1 | 114 |
| the other 18 fixtures | — | 0 | 0 | 0 |
| **TOTAL (21 fixtures)** | **1371** | **5** | **1** | **114** |

Before/after, measured rather than remembered:

| | before this pass (`e4fe8c187`) | after (fresh run) |
|---|---|---|
| census columns | none (the split was unmeasured) | `pfx` 5 / `pfi` 1 / `pfl` 114 |
| fixtures calling `__sh_printf` | 0 of 18 (`git grep -c` over the expected files) | 1 of 20 (`20_printf_dynamic`) |
| interpreter lines emitted, total | 0 measured — and reachable by any literal-format site that fell back, e.g. a slotless `printf '100%%\n'`, which used to keep the 114-line body | 114, all in `20` |
| 07_redirect | 44 lines, `__sh_print_words(&[…], nl, esc)` called with `false, false` | 44 lines, `__sh_print_words(&[…], nl)` |
| 19_printf_literal | did not exist | 31 lines, 4 specialized, 0 interpreter lines |

The cost of keeping the interpreter is visible in the same table: `20` is
235 lines to `19`'s 31, and 114 of those lines are the interpreter body.

`rust_fuzz` renders literal formats of every class above (`%%`, a literal
brace, `%d` with a numeric argument, and a format assembled at run time), so
the classification is exercised on scripts nobody hand-wrote.

## Naming, lints and aborts

The emitted program is meant to read as if a person wrote it: no
`#[allow(...)]` blanket and no `.unwrap()`. Census over the committed
`expected/*.rs` (before = the previous commit, after = the render in this
tree):

| counter | before | after |
|---|---|---|
| `#[allow(` — total | 9 | **0** |
| ├ `non_upper_case_globals` | 8 | 0 |
| └ `dead_code` | 1 | 0 |
| `.unwrap()` | 2 | **0** |
| `.expect(` | 0 | 0 |
| `panic!(` / `unreachable!(` | 0 | 0 |
| emitted lines | 553 | 547 |

The rules behind the zeros:

- **Storage names are uppercase.** A user-var `thread_local!` is
  `__SHV_COUNT`, not `__SHV_value`, so the declaration needs no
  `non_upper_case_globals` allow. The name is still derived from the mangled
  base (`static_ident`, case-collision suffix `_2`, `_3` when `a` and `A`
  both exist), so the leak guard and `-Oz` golf see the same identifier the
  uses do. The stray `#[allow(non_upper_case_globals)]` that used to sit in
  front of `fn main()` (where the lint cannot even apply) is gone with the
  `attrs_ok` flag that threaded it.
- **A lone string literal is its own format string.** `__sh_print_words`
  folded to a print call on a constant word emits `println!("done")`, not
  `println!("{}", "done")` (`clippy::print_literal`); a trailing `\n` in the
  literal becomes `print!` (`print_with_newline`), and a literal containing
  `{`/`}` keeps the `{}` placeholder form because it would need doubling.
- **The ERE helper's `enum N` is fully live**, so its `#[allow(dead_code)]`
  was stale and is removed (proved by compiling the file with it stripped).
- **No panic path.** `let Some((head, rest)) = v.split_first() else { … }`
  replaces `v.split_first().unwrap()` (infallible by construction: the
  compiler sees the empty case), and a redirect target that cannot be opened
  falls back with `.or_else(|_| File::open("/dev/null")).ok()` instead of
  `unwrap_or_else(|_| File::open("/dev/null").unwrap())`. `.ok()` cannot
  panic: if even `/dev/null` cannot be opened the sink is simply left unset,
  and the sink-detection passes key on the *set* prefix
  (`__SH_OUTFILE_TL.with(|v| *v.borrow_mut() = `), not on the old `Some(`
  spelling, so nothing inlines a live sink away.

Remaining `#[allow]` shapes the emitter can still produce, recorded at the
emission site in `src/rust_backend.rs` and absent from all 17 fixtures
(census 0):

| lint | when | why the emission cannot avoid it |
|---|---|---|
| `non_snake_case` | a *promoted local* whose shell name has an uppercase letter (`let COUNT = …`) | the local keeps the shell spelling so one name maps to one storage class for the whole program (the leak guard and the golf renamer both rely on that); renaming it would need a second name map |
| `unused_assignments` | a promoted local whose first write is control-flow dependent (`local_dead_init`) | bash zero-initializes; `let mut x;` is not provably assigned on every path, so the default init stays and may be overwritten unread |

## What each fixture pins

| fixture | shape |
|---|---|
| `01_assign` | plain scalar assignment, arithmetic |
| `02_regex` | `=~` lowered to the native ERE matcher |
| `03_capture` | `$( … )` capture block whose `ls -1 \| wc -l` is wired by the emitted program (`__sh_pipe`), no `bash -c`; word lists are `Cow`/`Word` (`vec!["ls".into()]`) |
| `04_function` | user function with a parameter |
| `05_array` | array in the thread-local store (arrays are not promoted) |
| `06_arith_i64` | integer arithmetic, pow helper |
| `07_redirect` | redirect + a literal-`printf` site decomposed to `format!`: the emitted file calls no part of the 115-line `__sh_printf` interpreter |
| `08_bigint` | `num_bigint` lowering for out-of-`i64` constants |
| `09_regex_group`, `10_regex_alt` | best-effort `[[ =~ ]]` (see below) |
| `11_promote` | promoted scalars: `let mut count = 0; count += 1;` |
| `12_local_fn` | function-local promotion: `let who = p0;` |
| `13_dynamic_scope` | **must stay in the store**: a function reads a `main` variable (bash dynamic scoping) |
| `14_capture_pin` | **must stay in the store**: a variable written inside `$( … )` is pinned by the closure/process boundary |
| `15_export_poison` | **must stay in the store**: `export COUNT=…` reaches the name by word, so a `let` would drop the export |
| `16_while_loop` | a `while [ … ]` condition whose status write keeps a block expression |
| `17_overflow` | the documented debug/`-O` divergence: bash wraps, a debug build panics (`# debug-overflow:` marker) |
| `18_eval_poison` | **must stay in the store**: `eval` reaches `__SHV_N` by text, so no promotion |
| `19_printf_literal` | **pins the specialized path**: four literal formats (`%s`, `%%`, literal braces) render as `format!`, zero `__sh_printf` in the file |
| `20_printf_dynamic` | **pins the fallback**: a format assembled at run time must keep *and call* the 115-line interpreter |
| `21_pipeline_interleave` | **pins the process/IO path**: pipelines spawned by the emitted program (`cat … \| tr … \| wc -l`, `printf … \| sort \| head`, a chain into `$()` and into a word), shell output interleaved with a child's inherited fd 1, and the one named `bash -c` fallback (a stage carrying a redirection) |

Fixtures 13–15 are the negative half of the promotion work and are as
important as 11–12: promotion is only sound where no construct can reach the
name from another scope or from another process.

## Promotion rules (what may become a `let`)

A scalar keeps `thread_local!` storage unless ALL of:

* every mention sits in ONE scope (`main`, or one user function);
* its first mention is a top-level write in that scope (that is where the
  `let` lands) and it is read at least once;
* no mention is a **pin** — a closure body (`Capture`/`Lambda`/`Arrow`), a
  separate-process construct (`( )`, `&`, a pipeline) or opaque asm;
* no construct can reach the name dynamically — `eval`/`source` (which bail
  the whole analysis), `export`/`readonly`/`unset`/`read`/`mapfile`/
  `getopts`/`set`/`trap`/`printf -v`, `declare -n`/`-g`, arrays/assoc, and
  shell specials (`$?`, `$!`, `$#`, …).

The shared `risky` bit in `list_profile::scopes` is deliberately NOT
consulted: it models a renderer that lowers an argument through a file-scope
helper which *captures* the value. This renderer splices the read inline
(`read_str` hands back the binding's name), so `s="x"; if [[ "$s" == *x* ]]`
is a plain `let`. `Use::pin` is the part that does apply here.

## Known-best-effort fixtures

`09_regex_group.sh` and `10_regex_alt.sh` use `[[ "$x" =~ ^v([0-9]+)... ]]` —
a parenthesized group in a `[[ ]]` test, which the front end does not lower
yet, so they render a `// TODO(unsupported)` marker. They are kept because
they pin two things: the native ERE matcher is *not* reached for them (they
must therefore still compile), and the dead `let` left behind by the
unsupported construct is removed rather than warned about.

## The differential fuzz net

    cargo run --bin rust_fuzz                       # 60 programs, seed 1, both profiles
    cargo run --bin rust_fuzz -- --cases 300 --seed 7
    cargo run --bin rust_fuzz -- --script case.sh   # replay one program
    cargo run --bin rust_fuzz -- --cases 20 --keep  # keep every scratch dir

Each generated program is run twice on the same bytes — under `bash` and under
the freshly rendered + compiled Rust — and **stdout, stderr and the exit
status** must all match. Every program is built in BOTH profiles (plain
`rustc`, which keeps overflow checks on, and `rustc -O`), so a
resolved-to-plain-operator arithmetic path that wraps in bash but panics in a
debug build is a divergence, not a crash to ignore: a generated-code panic
writes to stderr and exits 101, which the diff catches like any other
divergence. The net exists because most of the idiom work is done by TEXT
passes (`compound_assign`, `drop_single_use_clones`, the `__sh_cat`
flattening, `specialize_literal_printf`, `drop_esc_arg`, the helper pruner)
and several of them decide a rewrite by classifying or counting identifiers in
a string: a pass like that can only be trusted if it is shown not to change
meaning on shapes nobody hand-wrote.

The generator is a seeded LCG and the seed is a first-class input, so any
failure is one command away from reproducing: the failing case stays in its
scratch directory and the run prints
`--script <dir>/case.sh --seed <seed>`. One summary line reports seed,
profiles, programs and divergences, so a regression — or a quiet collapse in
coverage — is visible at a glance:

    rust_fuzz: seed=1 profile=debug+release programs=60 matched=60 diverged=0 render_fail=0 rustc_fail=0

The grammar reaches the seams the recent passes work on: quoted vs unquoted
expansion of a value with a space, multi-word `echo`/`printf` argument lists,
command substitution — nested, word-split in a `for` list, and **glued to
literal text inside one word** (`x=$(cmd)z`, `x=z$(cmd)`,
`x=$(cmd)-$(cmd)`, quoted and unquoted) — `$var` in
arithmetic and in `[ ]` comparisons, positional parameters and loop variables,
`case`, `$?`, `false || …`, redirects (`>` and `>>`) read back through a
capture, **pipelines** — an all-external multi-stage chain, a
`builtin | external…` chain, a chain feeding `$()` and one feeding a word, and
a stage carrying a redirection (the named `bash -c` fallback) — **shell output
interleaved with a child's inherited fd 1** (`printf 'pre-'` with no newline,
then an external command, then more shell output), single-use string
variables, repeated literals — and
`printf` sites of every class the specializer classifies: literal formats with
0/1/N `%s` slots, `%%`, escapes, `\c`, literal braces, `%d`, `%c`, both arity
mismatches, a slotless format that still passes an argument, and a format
assembled at run time (the interpreter fallback).

Documented seeds: **1** (the default, and what a gate run uses) plus 7,
424242 and 20261126 for a wider sweep — all four ran clean (60 programs at
seed 1, 100 at each of the others).

Divergences this net found, each fixed in `src/rust_backend.rs`:

- three `Cow`/`String` type mismatches left by the `Word = Cow` change — a
  `for` item read out of a `Vec<Word>`, a `read` target, and a folded constant
  echo word (`vec!["0".to_string()]` where a `Word` was required). No fixture
  had any of the shapes; all three were `-D warnings` build failures in the
  generated program.
- `drop_esc_arg` skipped a nested `__sh_print_words(…)` call *inside* another
  call's word list, leaving a 3-argument call against the 2-argument helper
  (E0061). The pass now rewrites the arguments recursively and never abandons
  the scan on a shape it cannot delimit.
- a semantic bug in the `printf` interpreter: bash keeps `\c` in a **format**
  literally — it only truncates in a `%b` **argument** — while the render-time
  unescape and the interpreter both stopped the output. Both now match bash.
- a capture glued to literal text lost the literal (`x=$(cmd)z` rendered as
  `x=hi`). The word parser's continuation path (`append_plain_text` /
  `merge_expansion_into_word` in `src/parser/words.rs`) could only extend a
  `Literal`, a `Variable` or an existing interpolation, so a fragment after a
  bare `CommandSubstitution` was consumed and then discarded (`_ => false`
  after `lexer.next()`). Both helpers now turn ANY single-expansion word into
  interpolation parts through `expansion_into_parts`, so the word keeps every
  part — unquoted or quoted, with a prefix, a suffix or several fragments.
- the dead-status-store collapse in `emit` deleted the **previous line** when
  it stored `__SH_RC`, assuming the payload had no effect. Native pipelines
  made that visible: `__SH_RC.store(__sh_pipe(…))` carries the whole pipeline
  in its argument, so collapsing it silently dropped the command (bash printed
  `1`, the emitted program printed nothing). The collapse now requires a
  call-free payload (`rc_store_payload_pure`); a store of a helper call is
  kept.
