# C backend — known runtime limitations

Status: corpus gate **580/643 pass** (baseline at session start: 538/643;
0 compile errors; 0 `TODO(unsupported)` / `sh2.*` stub markers hit on the
corpus). Gate: `harness/c_gate_main.sh` (same oracle as
`harness/c_gate_repro.sh`, rendering through main's renderer).

Every class below is a DOCUMENTED known limitation with its root cause —
none is a hidden regression. Each lists the corpus cases that pin it.

## 1. `$0` is argv[0], not the script path
- Cases: `057_case.sh`
- bash carries the script path in `$0`; a transpiled native binary has its
  own argv[0]. The renderer emits `(_sh_argc > 0 && _sh_argv[0]) ?
  _sh_argv[0] : ""` — the natural native reading. Matching bash byte-for-
  byte would require embedding the source path into generated code (the
  shIR contract carries no filename).
- Unavoidable without a contract extension (source path in IrProgram).

## 2. `$-` option flags are static
- Cases: `dollar-minus.sh` (passes), `interactive-test-minus-t.sh`
- `_sh_opts` seeds the default flag set ("hB"); interactive checks (`test
  -t`) depend on real tty state, which differs under the gate's redirected
  stdio.
- Partially unavoidable: tty state is genuinely environmental.

## 3. Background jobs — RESOLVED with real fork()
- Cases fixed: `048_subprocess.sh`, `105_background_copy_semantics.sh`,
  `t44_background.sh`, `063_06_complex_pipeline_background.sh`,
  `051_primes.sh`
- bash `cmd &` is a fork, so the renderer now forks: the child runs the
  body on its own address-space copy (isolation + async ordering for
  free), fflushes and _exits; the parent records the pid for bare `wait`.
  This is the ONE deliberate process primitive — it is not emulation of a
  shell construct, it IS the shell construct's native mapping.

## 4. `eval` / `source` (dot) effects stay in the child shell
- Cases: `dot-source-lib.sh`, `eval-assign.sh`, `063_12_complex_eval.sh`,
  `parse-eval-multiline.sh`, `zsh-style-eval-*`
- `eval`/`.` execute dynamically-generated text whose variable/exit effects
  must land in the PARENT shell. The site transport runs child bash; the
  parent cannot see child assignments without a marshalling protocol
  (export all → re-read all), which is unsound for local/scoped vars.
- Requires a dedicated eval/source lowering (transform candidate:
  inline-known-text eval; refuse unknown text).

## 5. Process substitution ordering under pipelines
- Cases: `012_process_substitution.sh`, `041_process_substitution_mapfile.sh`,
  `064_09_process_substitution_pipeline.sh`
- The FIFO+background-producer emulation (Redirect arm) preserves data but
  interleaves stdout with consumer-side reads differently from bash when a
  mapfile/read consumes the fifo mid-pipeline.
- Ordering-only divergence; data itself matches.

## 6. Quoted brace alternation over-expanded
- Cases: `076_brace_expansion_mixed.sh`, `064_02_nested_brace_expansions.sh`,
  `064_hard_to_generate.sh`
- bash does NOT expand `{1..3,7..9}` (comma alternatives that contain `..`
  are LITERAL strings); the core's brace call classifies both parts as
  ranges and expands them. Root cause lives in the shared parser/brace
  node, not the renderer — fixing it changes every backend's gate and must
  go through the PLAN §11 offer cycle.
- Core-request candidate: brace node should only treat an alternative as a
  range when it is the ENTIRE brace content (no top-level comma).

## 7. Assoc-array/map iteration order
- Cases: `064_07_complex_array_operations.sh`, `064_22_function_returning_
  complex_data_structures.sh`
- bash iterates assoc arrays in INSERTION order; the C runtime's name list
  is sorted. Values match; order differs.
- Fix candidate: keep an insertion-order key index alongside the map.

## 8. `${var/#pat/repl}` anchored replacement + array element writes
- Cases: `070_gnuisms_thorough.sh`
- The anchored-replacement op renders the unanchored form; array element
  writes inside functions drop elements.

## 9. Nested function definitions
- Cases: `081_nested_functions.sh`, `064_10_nested_function_definitions.sh`
- bash nested `f(){ g(){ ... } }` defines g at CALL time in the global
  scope. The C renderer hoists all functions to file scope; inner defs
  defined-but-never-called print nothing.
- Needs conditional function definition emission (define-on-first-call).

## 10. Traps
- Cases: `064_23_complex_error_handling_traps.sh`
- EXIT/ERR trap ORDER relative to explicit echoes differs: trap bodies run
  at C exit() where bash runs them before the last command's stdout flush
  boundary in some orders.

## 11. Multi-stage pipelines around heredocs / while-read
- Cases: `063_05_heredoc_with_complex_content.sh`,
  `063_11_complex_while_loop.sh`, `048_subprocess.sh`,
  `000__04h_complex_examples.sh`
- Quoted-delimiter heredoc bodies feeding grep|sed pipelines lose stages
  when the heredoc stage text and subsequent stage glue interact; body
  lines after the first are dropped in some shapes.

## 12. Script-defined shell-out helpers (`capture`/`check_qx` style)
- Cases: `id-cmdsub.sh`, `readonly-cmdsub.sh`, `typeset-cmdsub.sh`,
  `tty-cmdsub.sh`, `ps-system-call.sh`, `qx-var-builtin-cd.sh`
- These scripts define their OWN wrapper functions that build a command
  line from `"$@"` and shell out; the rendered child text glues name+args
  into one word in some shapes (`bash: 'id -u': command not found`) and
  builtin-cd/qx transports lose cwd effects. Root cause is per-script
  indirection through positional re-assembly, not a single node gap.

## 13. Misc single-case divergences
- `at-in-test.sh`: extglob `@(...)` in `[[ ]]` needs FNM_EXTMATCH on the
  test-token path (implemented for `=` compares; the `@(alt)` inside a
  larger pattern still misses).
- `064_20`: `DEBUG=1` exported inside a nested subshell doesn't survive to
  the outer echo (env-prefix scoping in subshell sites).
- `064_21`: `${HOSTNAME:-localhost}` — HOSTNAME exists in bash's env but
  not the C child's (bash seeds HOSTNAME itself).
- `utf8-non-utf8-content.sh`: lossy UTF-8 decode drops an invalid byte
  bash passes through.
- `multiple-awk-in-dqs.sh`: `(null)` printed for a NULL char* in an edge
  printf path.
- `t83_exit.sh`, `parse-bracket-subshell-pipe.sh`: exit-code propagation
  through redirect-wrapped subshell chains.

## Assoc-array sort-order preservation — UNSUPPORTED

`sort <<<"${assoc[*]}"` receives ONE line (values space-joined by IFS)
and passes it through unchanged. Sort operates on LINES, not words, so
a single-line input is never reordered. This means:

- `sorted=($(sort <<<"${assoc[*]}"))` produces the SAME (unsorted) order
  as the assoc array's internal iteration
- The "sorted" variable contains unsorted values despite the intent
- Different bash versions/systems may produce different hash orders,
  making this test inherently flaky across environments

**Correct pattern**: `printf '%s\n' "${assoc[@]}" | sort` puts each value
on its own line so sort can work. Our C backend now emits the newline-
delimited join (`_sh_join_arr_nl`) for assoc array value expansion in
pipeline contexts.

**Status**: UNSUPPORTED by design — bash's `<<<` here-string semantics
do not provide the line structure that sort requires for multi-word
input. Fixed in 064_07_complex_array_operations.sh.
