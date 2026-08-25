# GLSL backend (sketch) — shell → fragment shader

Status: **sketch**. Renders the ShIR (A1) to a GLSL ES 3.00 (WebGL 2)
fragment shader that computes the shell program's stdout into a global
byte buffer and encodes it as the fragment color. The natural home is the
`backends/glsl` worktree (branch `backend/glsl`); the renderer is merged
at `src/glsl_backend.rs` (`shir_to_glsl`), wired to the CLI as
`debashc --shir-in-glsl <a1.json>` / `file --shir-in-glsl`.

```
debashc file --shir foo.sh --raw | debashc --shir-in-glsl - > foo.frag
```

## Why a fragment shader

A fragment shader is the only universally-available GLSL stage whose
result is readable back to the CPU (`readPixels`), and `main()` runs once
per fragment — a 1×N canvas gives N byte slots. `u_mode == 0` (default)
packs `(len, byte0, byte1, byte2)` into `gl_FragColor`; `u_mode == 1`
renders one byte per fragment across `gl_FragCoord.x` (read the red
channel of each pixel).

## String model

GLSL has no string/char type. A string is an `ivec2 (offset, len)` into
the global `const int s_tab[]` string table (ASCII codes). Runtime
concatenation and number-to-string materialize into the `s_scratch`
region — `cat` / `itos` always return a FRESH scratch string, so
expression results are stable until the next materialization. Assignment
re-materializes (`cat(v, ivec2(0,0))`) so stored values never alias a
scratch slot that later expressions overwrite.

## What renders natively

- assignment / `local name=value` / `setVar` / `assign("x", "+=", n)`
- integer arithmetic: `+ - * / % ** << >> & | ^ ~`, comparisons,
  `&& || !`, ternaries, `++`/`--`/`op=` (via `parse_arith`)
- `echo` / `print` (args joined with single spaces), `printf` with
  literal `%s %d %i %%` formats, `\n \t \r` escapes
- `if` / `elsif` / `else`, `while`, `do..while`, `for` over literal
  arrays and numeric ranges, C-style `for`, `break` / `continue`
- `case` — glob dispatch (`* ?` literal patterns) via the iterative
  `globMatch` helper
- numeric tests `[ $i -lt 3 ]` (`-eq -ne -lt -le -gt -ge`), string
  equality `= == !=` (glob patterns when the right side has metachars),
  `-n` / `-z`, bare truthiness, `!` negation
- user functions: hoisted to file scope (GLSL forbids nested
  definitions), args passed through the global `g_pa[]` param array as
  strings; `$1..$N` reads inside functions map to it

## What renders as `/* TODO(unsupported): ... */`

Processes, files, and external binaries are fundamentally unrepresentable
on a GPU — the C backend's refuse-over-guess idiom:

- `exec` of any non-echo/printf/local command, `pipeline`, redirection,
  subshell, background, command substitution (`$(...)`), file tests
  (`-f -d -e`), `write-file`, `die`/`warn`, `eval`, regexes

The renderer stays TOTAL (never panics): every unrepresentable construct
is a comment marker, so the shader always compiles. The trailing summary
line reports the TODO count.

## Known limitations (documented, not hidden)

- **i32 arithmetic**: bash wraps at 2^64, GLSL ES 3.00 `int` at 2^32.
  The `choose_width(lo, hi, target)` table in `shir.rs` already lists
  GLSL as a consumer — the planned bridge is an i64-in-two-ints pack.
- **no argv at top level**: `$1` at top level is `""`/`0` (there is no
  argv on the GPU); inside functions `$N` maps to `g_pa[N-1]`.
- **functions are `void`**: shell status codes are dropped; recursion is
  illegal in GLSL (bash can recurse) — no guard yet.
- **strings are ASCII**: non-ASCII chars become `?` in the table.
- **no `$?`**: the status variable renders as `0` (TODO marker).
- **scoping**: all variables are globals (hoisted), like the C backend's
  store — `local` is a declaration, not a scope.
- **word splitting / globbing** in for-iters and unquoted expansions are
  not modeled — for-iter over a runtime string is TODO.

## bc — fixed-point integers, NOT floats

GNU bc is EXACT decimal fixed-point (`v/10^scale`, TRUNCATION, and the
GNU output format: leading integer zero omitted — `.5` not `0.5` —
trailing scale zeros kept — `2.50` — zero → `0`; `src/bc.rs` is the
reference, 77/77 vs real bc). A float mapping cannot reproduce that:
fp32 has a 24-bit mantissa and ROUNDS, bc TRUNCATES to exact decimal
digits (`.33` ≠ 0.33333334; `1/6` at scale 2 is `.16` while `%.2f` of
the double rounds to `.17`), and the `.5`/`2.50` output format has no
float analogue. So `$(echo EXPR | bc)` / `echo EXPR | bc` lower to:

- **static EXPR** → compile-time fold through `crate::bc::eval` — the
  exact GNU-bc output becomes a string literal (byte-identical);
- **`sqrt($var)`** → runtime integer `isqrt32` (exact truncated root —
  better than the JS `Math.sqrt`/`Math.floor` double path: no 2^53 or
  rounding concern within i32);
- **var-operand arithmetic** (`$sum + $i`, …) → scale-0 INTEGER ops via
  `parse_arith`, with a zero-divisor abort guard (bc's no-stdout-on-
  error); the estree path's documented integer-operand assumption,
  tightened to i32 here.

GLSL ES 3.00 has no double and no 64-bit int, so the JS path's 2^53
operand assumption shrinks to i32 either way — floats would be strictly
worse.

## Validation

- `cargo test --lib glsl_backend` — renderer unit tests.
- Corpus sweep: every `sh2perl/examples/*.sh` renders and validates:
  `debashc file --shir f.sh --raw | debashc --shir-in-glsl - > f.frag &&
  glslangValidator -S frag f.frag` (532/532 as of the sketch).
- Runtime verification requires a WebGL2 context (readPixels): render a
  1 × OUT_CAP canvas with `u_mode = 1` and reassemble stdout from the
  red channel; compare against `bash f.sh`.
