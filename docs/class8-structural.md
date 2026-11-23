# Class 8 — the structural idioms of the Rust render

The plan measured four "structural" idioms in the rendered Rust: the
per-variable `thread_local!` store (`__SHV_*`), the implicit `__SH_RC`
last-status register, string-typed arithmetic (`.parse::<i64>()`), and
golfed identifiers (`__sh_i1`, `__sh_arith_v0`, `__v`, `__t`).

This note records, per idiom, which bash rule forces it, what changed, and
what remains.

## (a) `thread_local!` per variable — kept, load-bearing

One cell per shell variable, ~3.8 k declarations in 454 of the 1017
renders. A lexical `let` is observably wrong for a variable that crosses
any of these boundaries:

| bash rule | witness |
|---|---|
| a function assignment writes the caller's variable (`f() { x=2; }; x=1; f; echo $x` → 2) | `a_function_assignment_is_a_global_write` |
| a command substitution runs in a copy (`x=1; y=$(x=9; echo $x); echo "$x $y"` → `1 9`) | `a_command_substitution_assignment_is_isolated` |
| `set -u`, `unset` and trap handlers may reach a variable before any statement of the scope ran, and a trap fires at an arbitrary command boundary | documented in `decl_stmt` |

The renderer does not have to use a cell for *every* variable: the pass-2
local analysis (`compute_local_vars`) proves a variable invisible through
those boundaries and then binds it as a `let` at the top of its scope.
That is why 192 of the campaign's 548 programs had no store at all.

**Two pre-existing divergences found while writing the scoping witnesses**,
both in the lowering rather than in the choice of cell:

* a bare subshell statement renders inline (`{ x = 2; x }`) without saving
  and restoring what it writes, so `x=1; (x=2); echo $x` prints 2 where
  bash prints 1;
* `local v=5` writes the one cell and never restores the caller's value on
  return.

Both are pinned as `#[ignore]`d tests (they name the failing cases) and are
independent of store-vs-`let`: a local `let` needs the same copy/restore
that is missing.

## (b) `__SH_RC` — reduced; kept where `$?` can be read

bash's status of the last command is read only by a construct that asks:
`$?`, `set -e`, `ERR`/`EXIT` traps, `&&`/`||`, a pipeline join. Every such
read is an emitted `__SH_RC.load`, so `drop_dead_rc_stores` (after
`prune_unused_statics`, before `-Oz` golf) works on the finished text:

* no load at all → no status can ever be observed → every standalone
  `__SH_RC.store(…);` line goes;
* exactly one load → it observes the last store *before* it; every other
  store (before or after) goes;
* two or more loads → every store stays (no flow analysis).

A line is a candidate only when it holds **one** statement and stores an
**inert** value (`0`, `(1) as i32`). The renderer also spells
`__SH_RC.store(0, …); println!("…");` and
`__SH_RC.store(__sh_run("…"), …)`; the first carries the program's output,
the second runs the command that produces it, so both keep their line.

## (c) `.parse::<i64>()` — reduced at the literals; kept for text values

bash arithmetic re-reads a variable's VALUE as an expression each time, so
a variable whose write side is not provably numeric must keep its text
(`x=1; x=two; x=3`). `read_num_of_str` therefore emits
`…trim().parse::<i64>().unwrap_or(0)`, which is bash's rule: blanks
ignored, empty/unset or unknown name → 0.

Where the operand is a *literal*, no runtime text exists and the
conversion is folded at render time:

* `expr_num` on a `Str` that does not parse is `0`;
* a slice offset/length literal that is neither a number nor a name (the
  `[@]` whole-array form, `""`) is `0`, instead of the former
  `"@".to_string().trim().parse::<i64>().unwrap_or(0)`.

A variable becomes a native `Cell<i64>` (`is_num`) only when every write
is provably numeric — the case where no conversion can be observed.

## (d) golfed identifiers — not emitted by a default render

`__sh_i1`, `__sh_arith_v0`, `__v` and `__t` occur **0 times** in the 1017
default renders; golf (`golf_rust`) runs only under `-Oz`. The only `__t`
in the corpus (`sh.new/progress.sh`) is a variable the *program* names, not
a golfed identifier. Pinned by
`default_renders_carry_no_golfed_identifiers`, which renders arithmetic,
`for` and `if` and asserts none of the four names appears.

## Census — same 1017 sources, both binaries

| metric | before (`ded18aaa1`) | after |
|---|---|---|
| `__SHV_*` static declarations | 3794 (454 progs) | 3794 (454 progs) |
| `__SH_RC` mentions | 26180 | 25856 |
| `__SH_RC.store` sites | 20415 | 20091 (−324, 114 progs) |
| `__SH_RC.load` sites | 5030 | 5030 |
| `.parse::<i64>()` | 1587 (297 progs) | 1308 (289 progs) |
| `__sh_i1` / `__sh_arith_v0` / `__v` | 0 / 0 / 0 | 0 / 0 / 0 |
| `__t` | 3 (a program's own name) | 3 |
| distinct `__sh_*` callees | 62 | 62 |
| `Command::new("bash")` sites | 904 (578 progs) | 904 (578 progs) |
| helper definitions | 4844 | 4844 |

The campaign's 548-render corpus is a subset of this tree at an earlier
state; the ratios there are the same (~2% of status stores dead, ~18% of
the parses folded).

## Witnesses

Compiled with `rustc -O` and diffed against bash (stdout + exit status):
`examples.new/let-plusassign.sh` (status stores),
`templates/sh/array_append.sh` and `sh.new/028_arrays_indexed.sh` (literal
subsets folded), `sh.new/063_15_complex_function_definition.sh` (status
plus a function's `return`). All four match byte for byte.
