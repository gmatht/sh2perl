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
    cargo run --bin rust_fuzz -- --cases 150 --seed 1

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

A `printf` whose format is a literal is decided at render time
(`classify_printf_format` in `src/rust_backend.rs`) and emitted as
`format!` at the call site, so the ~115-line `__sh_printf` interpreter and
its `atoi`/`atou`/`atof`/`printf_b` cluster fall out of the program. The
decision is a classification with a reason, not a spot fix:

| format | plan |
|---|---|
| literal text | `format!` text (a literal `{`/`}` is doubled — a brace is **not** a format specifier) |
| `%%` | one `%` |
| `%s` | one `{}`, consuming one argument (bash pads, truncates and quotes nothing) |
| `%d`/`%i`/`%u`/`%o`/`%x`/`%c`/`%q` | **fallback** — the argument's numeric/char conversion is the interpreter's job |
| `%b` | **fallback** — backslash escapes come from the *argument*, at run time |
| flags/width/precision (`%5s`, `%.2f`, `*`) | **fallback** |
| `\c` in the format | **fallback** (it truncates the output) |
| a format with no `%s` but with arguments | **fallback** — bash re-applies the format per argument group |
| `%s` with no argument, or a different number of arguments | **fallback** — the interpreter pads/repeats |
| a non-literal format (variable, interpolation, capture) | **fallback** — nothing to decompose |
| anything unparsable | **fallback** |

The fallback is complete, not deleted: `20_printf_dynamic.sh` builds the
format at run time and must keep *and call* the interpreter, so an
over-eager specialization fails the gate instead of changing output.

Measured (`pfx` = specialized sites, `pfi` = calls left on the
interpreter, `pfl` = interpreter lines in the emitted file):

| fixture | pfx | pfi | pfl |
|---|---|---|---|
| 07_redirect (`printf '%s\n' "a b" > out.txt`) | 1 | 0 | 0 |
| 19_printf_literal (4 literal formats, `%%`, braces) | 4 | 0 | 0 |
| 20_printf_dynamic (format from a variable) | 0 | 1 | 115 |
| all other fixtures | 0 | 0 | 0 |
| **TOTAL** | **5** | **1** | **115** |

Before this pass the census could not see any of it: 0 sites were counted,
the interpreter cost 0 emitted lines only because nothing measured it, and
07 passed a literal `false` for an `esc` parameter the helper ignored
(`let _ = esc;`). Now no `echo -e` in a program means the two-argument
`print_words_noesc` is emitted and `drop_esc_arg` removes the argument
from every call site, so 07 reads `__sh_print_words(&[…], false)`.

`rust_fuzz` renders literal formats of every class above (`%%`, a literal
brace, `%d`, a run-time-assembled format), so the classification is
exercised on scripts nobody hand-wrote.

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
| `03_capture` | `$( … )` capture block (still a `bash -c` pipeline inside); word lists are `Cow`/`Word` (`vec!["ls".into()]`) |
| `04_function` | user function with a parameter |
| `05_array` | array in the thread-local store (arrays are not promoted) |
| `06_arith_i64` | integer arithmetic, pow helper |
| `07_redirect` | redirect + literal-`printf` specialization: the 261-line `__sh_printf` interpreter is *gone* from the output |
| `08_bigint` | `num_bigint` lowering for out-of-`i64` constants |
| `09_regex_group`, `10_regex_alt` | best-effort `[[ =~ ]]` (see below) |
| `11_promote` | promoted scalars: `let mut count = 0; count += 1;` |
| `12_local_fn` | function-local promotion: `let who = p0;` |
| `13_dynamic_scope` | **must stay in the store**: a function reads a `main` variable (bash dynamic scoping) |
| `14_capture_pin` | **must stay in the store**: a variable written inside `$( … )` is pinned by the closure/process boundary |
| `15_export_poison` | **must stay in the store**: `export COUNT=…` reaches the name by word, so a `let` would drop the export |
| `16_while_loop` | a `while [ … ]` condition whose status write keeps a block expression |
| `17_overflow` | the documented debug/`-O` divergence: bash wraps, a debug build panics (`# debug-overflow:` marker) |

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

## The fuzz net

    cargo run --bin rust_fuzz -- --cases 150 --seed 1 [--keep]

Renders seeded random programs from the supported subset, compiles each with
`rustc -O -D warnings`, and diffs stdout + exit status against `bash` on the
same script in identical scratch directories. It exists because most of the
idiom work is done by TEXT passes (`compound_assign`,
`drop_single_use_clones`, the `__sh_cat` flattening,
`specialize_literal_printf`, the helper pruner): a text pass can only be
trusted if it is shown not to change meaning on shapes nobody hand-wrote.
The net found two real codegen bugs on its first run — a three-statement
`{ let __b = …; __SH_RC.store(…); __b }` wrapper being inlined into a `while`
condition (invalid Rust), and an orphaned `use …::Ordering` (a `-D warnings`
failure) — both now fixed and pinned by `16_while_loop`.
