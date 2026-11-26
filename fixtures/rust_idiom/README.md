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

`--check` runs five checks per fixture, in this order:

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
5. **census** — the idiom counters (per-variable statics, `.clone()`,
   `.to_string()`, `#[allow(`, `Command::new("bash")`, emitted `__sh_*`
   helpers, lines) are printed per fixture and totalled. "Idiomatic" is a
   number that has to keep moving down, not a claim in a comment.

Clippy is used as an extra lint source when `cargo clippy` is installed; when
it is not, the gate prints `clippy: unavailable` rather than pretending the
check passed.

Every fixture is compiled except those whose output legitimately needs a
third-party crate in the *generated* program's own manifest. Those are
reported as `NEEDS-CRATE <name>`:

* `08_bigint.sh` — `$((9223372036854775807 + 1))` cannot be an `i64`, so the
  renderer emits `num_bigint::BigInt` (already a dependency of this crate).
  It compiles clean in a crate that has `num-bigint` (verified), but its
  output is *not* bash-identical: bash wraps to `-9223372036854775808`,
  the BigInt lowering prints the exact `9223372036854775808`. This fixture
  pins the render shape, not behavioural equality.

Adding a fixture: drop `NN_something.sh` in `src/`, run the harness without
`--check`, commit both files.

## What each fixture pins

| fixture | shape |
|---|---|
| `01_assign` | plain scalar assignment, arithmetic |
| `02_regex` | `=~` lowered to the native ERE matcher |
| `03_capture` | `$( … )` capture block (still a `bash -c` pipeline inside) |
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
