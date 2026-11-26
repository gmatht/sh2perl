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
6. **census** — the idiom counters (per-variable statics, `.clone()`,
   `.to_string()`, `#[allow(`, `Command::new("bash")`, emitted `__sh_*`
   helpers, lines) are printed per fixture and totalled. "Idiomatic" is a
   number that has to keep moving down, not a claim in a comment.

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

What still carries `.to_string()`, and why — the census total went **21 → 13**
across the fixtures, and none of the 13 is a word-vector element any more:

| where | n | why it must own |
|---|---|---|
| literal bound to a `String` local (`let name = "world".to_string()`) | 3 | `analyze_var_borrowable` withheld, so the binding's home is `String` — a promoted-binding question, not a word-list one |
| literal passed to a `String` parameter (`greet("there".to_string())`) | 2 | the parameter type comes from the function's `FnSig`, which is `String` |
| `String::from_utf8_lossy(&cap).to_string()` | 2 | the text is borrowed from a temporary capture buffer; it must be owned before the buffer drops |
| BigInt → decimal (`08_bigint`) | 1 | an out-of-`i64` value has no text form other than this |
| `String` store of an export-poisoned variable (`15_export_poison`) | 1 | the store is `RefCell<String>`; the name cannot be a `let` |
| `n.to_string().trim().parse::<i64>()` round-trips (`16`, `17`, `15`) | 4 | the numeric test/compare path stringifies an `i64` and re-parses it — the "numeric type inference" item, not the word plumbing |

The word path itself now converts in exactly one place: an integer read that
lands in a word list (`vec![n.to_string().into()]`), because the value is an
`i64` at run time and text is the semantic conversion.

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
