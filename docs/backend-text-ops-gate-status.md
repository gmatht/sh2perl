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
| java | backend_behavior | 69 pass / 30 fail / 452 skip | targeted streaming idioms byte-exact (compiled+run) |
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
4. **java (69/99 runnable)**: `shir_to_java`'s v1 subset errors on
   functions, arrays, assoc arrays, case-with-fallthrough and other
   constructs → 452 skips; among runnable files 30 stdout mismatches are
   non-text_ops lowerings (process substitution, advanced param ops).
5. **zig (0/551 gated)**: renders the reduced text-ops subset natively
   (idiom suite green) but general corpus scripts hit mark_todo paths
   (`sort`/`uniq`/multi-stage pipelines/user functions) whose emitted
   placeholder code fails compilation → counted skip.

None of these blockers originate in the shIR reductions: the reductions
raise native coverage where they fire and fall back explicitly everywhere
else. Closing them means renderer-parity work per backend (the standing
fleet backlog), plus the two small frontend gaps (`read`-stdin,
zsh arith condition) owned by fish-sh-go/zsh-sh-go.
