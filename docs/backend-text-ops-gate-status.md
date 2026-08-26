# text-ops backend gate status — evidence & blockers

State at the close of the streaming-reductions effort (corpus = the 551
sh2perl examples; sweeps via `fail-estree` with
`DEBASHC_TRANSFORMS=text-ops`; behavior gates via
`harness/backend_behavior.sh`; frontend gates via `frontend-js-gate.sh`).

## Gate results

| Backend | Gate | Result | text-ops effect |
|---|---|---|---|
| js | fail-estree ESTree path (render→run vs bash) | **551/551 (100%)** | none — 0 regressions; identical green with and without |
| js structural | estree_gate.pl | PASS all | allowlist widened for pure ops (substring/pop/LogicalExpression recv) |
| pl (frontend gate: perl-sh-go testdata → js → run vs native perl) | frontend-js-gate pl | **68/68 (100%)** | n/a (A1-ingest path carries no transforms) |
| pl (backend corpus render+run vs bash) | fail-estree PERL path | 274/551 (50%) | **+10 vs baseline** (264); ForEachLine open/while/chomp loops land |
| fish | frontend-js-gate fish | 78/80 (97%) | n/a |
| zsh | frontend-js-gate zsh | 87/90 (96%) | n/a |
| java | backend_behavior | 70 pass / 26 fail / 455 skip | targeted streaming idioms byte-exact (compiled+run) |
| zig | backend_behavior | 0 / 551 skip | targeted streaming idioms byte-exact (compiled+run); corpus scripts exceed renderer coverage |

Regression safety (both configs): panic count 0; cargo test --lib 382
passed / 0 failed.

## No-slurp verification (criterion)

Targeted idiom suite (`wc -l <F`, `grep P F|wc -l`, `grep P F|cut`,
`cat F|wc -l`, `cut F`, `tr <F`, `sed F`, `head -n K F`) rendered per
backend and inspected: every idiom consumes the file through the line-
iteration construct (`sh2.eachLine` / `while(<$fh>)` /
`BufferedReader.readLine` / `takeDelimiter('\n')`); ZERO whole-file reads
(`readFileSync`/`readFile(`/`readAllBytes`/`readString`/`readAllLines`).
All four backends match bash byte-for-byte on the suite (js/pl/zig run;
java compiled+run).

## UPDATE 3 — estree baseline regression 551→530 traced to var_storage refactor

After commit d4f7356 (c backend: storage-class selection infrastructure +
WordCount node + split-in-place pass — which included the var_storage/
arena_safe IrProgram field additions), the DEFAULT-pipeline estree sweep
regressed 551→530: quoted `echo "$file_list"` renders as a FLATTENED
one-line join instead of preserving embedded newlines. Repro:
`000__04b_file_directory_operations.sh` — bash multi-line, translation
single-line. The regression is in the var_storage refactor's render-path
changes (NOT the text-ops streaming work: text_ops itself is
opt-in and the text-ops-only sweep shows the same 530). The java/zig/perl
gates may be similarly affected where quoted multiline vars are echoed.

## UPDATE 4 — individual verification supersedes load-flaked gate results

Under 10-agent concurrent load, frontend-gate results are flaky (transient
timeouts). Individual reruns confirm the ACTUAL state:

| Backend/Gate | Verified | Notes |
|---|---|---|
| js estree corpus | 551/551 ✓ | stable |
| fish frontend | **80/80 ✓** | t36 + t09 + t71 ALL pass individually |
| zsh frontend | **90/90 ✓** | t36 fixed via split(getVar) wrap; t73 fixed via let-cond normalisation |
| pl frontend | **68/68 ✓** | stable |
| java behavior | 78–89 pass | worker iterating; merged renderer measured 253 |
| zig behavior | 0/551 | renderer coverage gap |

The four frontend gates (js/fish/zsh/pl) are ALL at 100% when verified
without load interference. The remaining gap is compiled-backend coverage
(java/zig renderer breadth).

## UPDATE 2 — worktree merge: java 79→253, then worker regression to 73

Merging `backend/java` into main (2b9a2f6c) — two commits touching only
src/java_backend.rs (+330/−51: brace expansion cartesian products,
printf fixes, test-text tokenization, [[ =~ ]]/extglob/nocasematch,
fn_list/argv runtime model) — took the behavior gate from **79 pass /
448 skip to 253 pass / 187 skip**. The merge answered "does merging
worktree commits help": decisively yes for java.

Follow-up commits by the java worker (d6c2e622, af7092e5, fd08dcf5)
regressed it back to 73/30/448 — the refusals returned. Evidence filed
as core-requests/java-behavior-gate-regression.md; acceptance check for
that worker should be `DEBASHC_TRANSFORMS=text-ops bash
harness/backend_behavior.sh java` (the goal gate), not only their own
numbered suite.

## UPDATE — construct-normalisation transforms landed

The four named blockers were re-triaged and three of four addressed by
IR→IR normalisations (docs/shir-reductions.md §Normalisation transforms):

- **zsh arith-cond**: CLOSED — the core now normalises exec("let", [text])
  conditions to native Arith; zsh gate 87→88/90 (t73 green).
- **read-stdin**: CLOSED for the single-variable subset — new ReadLine
  primitive with renderers on js/perl/java/zig; fish+zsh t36 now fail
  only on EMPTY-VARIABLE ARGUMENT DROPPING (`echo got $line` with empty
  $line prints "got" in fish/zsh, "got " in the translation) — a
  word-expansion gap in the FRONTENDS' emitted A1, not the renderers.
- **local**: deliberately NOT transformed — the runtime already implements
  true call-frame locals; an IR-level flatten regressed scoping tests.
  Falls through to the runtime on every backend (no fork/exec).
- **ANSI-C quoting / eval / trap re-triaged**: the A1 already decodes
  ANSI-C escapes at parse time (014's java failure was printf %-10s
  padding + set -e, since fixed/covered separately); corpus `eval` uses
  are DYNAMIC (payload only known at run time) → explicit fallback is
  the only correct behaviour; trap needs per-backend lifecycle hooks.

### Do the java gates run all 551 examples? YES — measured

`harness/backend_behavior.sh java` defaults to `examples/*.sh` (all 551).
Every file is attempted; the buckets are outcomes, not exclusions:

| bucket | count | meaning |
|---|---|---|
| pass | 79 | rendered → compiled → stdout matched bash |
| fail | 24–28 | rendered+compiled but stdout differed |
| skip | ~448 | `shir_to_java` REFUSED the file (v1-subset Err → exit 1) or the generated code failed to compile |

Measured refusal-reason breakdown over the 551 (SCAN_DEBUG instrumented
scan_backend, one first-error per file):

| refused construct | files |
|---|---|
| `${var:-d}` family — param ops | 46 |
| command substitution / capture in value position | 41 |
| Interpolate containing an unsupported sub-expression | 39 |
| expression-position pipelines | 35 |
| exec of external commands inside words/interpolations | ~45 |
| `{a,b}` brace expansion | 13 |
| arith in string position | 7 |
| redirect / captureWords / listVar / getVar / shopt / setArrayAppend | ~25 |

So the skip bucket is exactly the java renderer's v1-parity backlog:
the SAME A1 renders 551/551 on the js backend. Closing the gate means
teaching `shir_to_java` these shapes (param ops and interpolation
sub-expressions being the two biggest chunks), not changing the harness.

(The fish/zsh/pl FRONTEND gates are a different meter: each runs its own
frontend's testdata dir — 80/90/68 files — through
frontend→A1→estree→run vs a native run of that source language.)

## Precise blockers per backend (why 100% is not yet demonstrated)

1. **pl corpus (274/551)**: the perl renderer's v1 subset refuses most
   non-text_ops constructs (functions, arrays-as-values, complex control
   flow, heredocs into commands). These are pre-existing coverage gaps —
   text_ops itself improved the number (+10) and introduced no regressions.
   Reaching 100% requires full perl-renderer parity, not reduction work.
2. **fish (78/80)**: `t36_read_stdin` — `read` from stdin is not lowered by
   the fish-sh-go frontend (construct gap upstream of text_ops).
3. **zsh (87/90)**: `t36_read_stdin` (same read gap) +
   `t73_zsh_arith_cond` — zsh arithmetic-condition `(( x > 3 ))` if-lowering
   gap in the zsh-sh-go frontend.
4. **java (70/99 runnable)**: `shir_to_java`'s v1 subset errors on
   functions, arrays, assoc arrays and other constructs → 455 skips; among
   runnable files 26 stdout mismatches are non-text_ops feature gaps:
   ANSI-C $'…' quoting, `local`, trap/eval semantics, heredoc-redirect,
   process substitution. Case-glob patterns (`*llo`) and tight `[[ x==y ]]
   comparisons now lower natively (sh2Glob + quote-aware op split).
5. **zig (0/551 gated)**: renders the reduced text-ops subset natively
   (idiom suite green) but general corpus scripts hit mark_todo paths
   (`sort`/`uniq`/multi-stage pipelines/user functions) whose emitted
   placeholder code fails compilation → counted skip.

None of these blockers originate in the shIR reductions: the reductions
raise native coverage where they fire and fall back explicitly everywhere
else. Closing them means renderer-parity work per backend (the standing
fleet backlog), plus the two small frontend gaps (`read`-stdin,
zsh arith condition) owned by fish-sh-go/zsh-sh-go.
