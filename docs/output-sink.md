# The output sink and word printing — what is native, what still routes

Shell output is not `println!`: `echo` joins its words with one space and may
or may not add a newline, `echo -e` decodes escapes (and `\c` truncates the
rest, newline included), `printf` has its own format language, and every one
of those can be *redirected* (`>`, `>>`, `2>`) or *captured* (`$(…)`, `<(…)`,
a command in a pipeline) — in which case the bytes do not go to the process's
stdout at all. The renderer therefore has one dispatcher, `__sh_sink`, whose
destination is, in order: the `__SH_OUTFILE_TL` file (a redirect in force),
the `__SH_OUT` capture buffer, then `std::io::stdout()`. The word printers
(`__sh_print_words`, `__sh_puts`) call it.

`sink_free_here()` is the render-time verdict that no sink can be active at
this site: inside a capture it is false by definition (`capture_depth > 0`),
at top level outside a function body it is true, and inside a function body
it is true only when that function is never invoked under a redirect
(`fn_sink_never`). Every construct below is lifted exactly when that verdict
is true.

## Per-construct table

| construct | what is emitted | route |
|---|---|---|
| `echo` with constant words (`echo a b`, `echo -n`, `echo -e 'a\tb'`), sink-free | one precomputed `print!` / `println!` | **native** (existing constant fast path) |
| `echo "x$a" "y$b"` — every word one element, sink-free, no `-e` | `println!("x{} y{}", …)` in one format call | **native** (existing direct path) |
| `echo "pre" "${arr[@]}"`, `echo $list`, `echo "$@"` — a splat/array part, sink-free, no `-e` | `{ let __sh_w = [..].concat(); if !__sh_w.is_empty() { print!("{}", __sh_w.join(" ")); } println!(); }` — flattened words, single-space join, `echo`'s newline (a zero-word `echo` prints just the newline) | **native since this pass** |
| `echo -n` with a splat part, sink-free | the same join with no `println!` | **native since this pass** |
| any `echo` inside `$(…)` / `<(…)` / a redirected block | `__sh_print_words` → `__sh_sink` | **helper, by design**: the capture buffer or outfile IS stdout here; a direct `print!` would both leak to the terminal and store nothing |
| an `echo` in a function body that some call site reaches under a redirect | `__sh_print_words` → `__sh_sink` | **helper, by design**: the sink destination is a call-site property, not a body property (a body that is never called under a redirect takes the native paths — `fn_sink_never`) |
| dynamic `echo -e` (`echo -e "v$a\tw"`) | `__sh_print_words(…, true)` + `__sh_echo_esc` | **helper, by design**: escape decoding is a runtime function (`\c` truncation, octal/hex escapes, `\\`, `\0nnn`); inlining the decoder at every site duplicates it, and `printf %b` is a different decoder from `echo -e` — the two are not interchangeable |
| `echo` whose word can raise the arith-fail flag (`$((…))` over an empty positional) | the guarded `__sh_cat` + `__sh_print_words` form | **helper, by design**: bash suppresses the WHOLE simple command when a word's arithmetic fails, which needs the flag read between evaluation and printing |
| constant `echo` at a sink-possible site | `__sh_puts` | **helper, by design**: the constant is known, the destination is not |
| `printf` with a literal format and a matching `%s`/`%d`/`%i` operand count | `format!` | **native** (existing); `__sh_printf` is emitted only for formats that need it (`%q`, format reuse, width/precision the native path does not model) — 0 corpus sites |

## Census

At the HEAD this table was written (548 renders, `555 *.sh` − 7 that fail
`bash -n`):

| helper | call sites before | after |
|---|---|---|
| `__sh_print_words` | 401 (incl. the definition line) | **364** |
| `__sh_sink` | 520 | **488** |
| `__sh_puts` | 149 | 149 |
| `__sh_echo_esc` | 14 | 14 |
| `__sh_printf` | 0 | 0 |

Every remaining `__sh_print_words` / `__sh_puts` call site is one of the rows
marked **helper, by design** above: the new branch takes every site where
`!esc && sink_free_here()`, so what is left is `-e`, the arith guard, or a
sink that can be active at that site. There is no sink-printing site left
that a render-time verdict could prove safe.

Measured breakdown of the 274 remaining `__sh_print_words` call sites in the
corpus: **221 are inside a function body** (a call site of that function may
run under a redirect), the rest are inside `$(…)`/`<(…)`/a redirect at the
site itself; **40 are `echo -n`** (`nl = false`, same join, no newline);
**0 are `-e`** and **0 take the arith-guard form** — the corpus's 7
`__sh_echo_esc` uses are helper-internal (the `echo_line`/`print_words`
definitions), not call sites, so the `-e` route is exercised by the unit
tests rather than the example set.

Not lifted, and why:

* **`printf` with `%q` / format reuse / `%b`** — `format!` cannot express
  shell quoting or bash's re-application rule, and `%b` is a different
  decoder from `echo -e`.
* **NUL payloads through `String`** — a word list is `Vec<String>`; a NUL
  byte in a shell word is impossible (bash truncates at it), so the corpus
  has no such site, and `printf '...\0'` stays on the printf path.
* **Diagnostics interleaved with stdout** — stderr ordering relative to a
  buffered stdout is the process's, not the renderer's; the sink does not
  reorder either stream.
