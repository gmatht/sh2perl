# Go backend — known runtime limitations

Renderer: `src/go_backend.rs` (`shir_to_go`), entered via
`shir_render --target go`. The output is self-contained Go
(`package main`, no external deps beyond the stdlib) with an embedded
runtime-helper preamble (emitted per-program by reachability).

Status: see the backend gate (`setup_backends.sh --backend-gate go`).
This file catalogues every fork/exec site and every KNOWN semantic
approximation with its root cause — none is a hidden regression.

## 0. Shell-out inventory (fork/exec sites)

External commands (`ls`, `grep`, `sort`, …) MUST fork/exec — that is
translation, not a fallback. The list below is the *bash -c* layer
(shelling out to `bash` itself) plus delegation-heavy helpers. It
shrinks as constructs lower natively; the per-file metric is
`harness/backend_runtime_metric.sh go` (worktree renderer, full helper
namespace — the gate's `sh2_*` stub pattern is vacuous for this
backend, whose runtime uses plain Go names).

Natively lowered (no child bash):

- `echo`/`printf` statements AND captures (`$(echo …)`), incl. `-n`,
  `-e` (with `\c` truncation, `\xHH`, octal), `%-`literals.
- Pipelines of plain externals (`runPipe`) and pipeline captures
  (`capPipeIn`, incl. a leading echo/printf as input).
- Arithmetic captures and `$((…))` test operands (with div/mod-by-zero
  → 0 and shift/pow guards — bash never panics).
- `mapfile -t arr < file`, globbing (with the leading-dot rule:
  `*` skips dotfiles, `[.]x` does not match `.x`), `case` via
  globMatch, `for`/`while`/`cstyle` loops (labeled-break-correct
  `break` inside `switch`), `read`, redirections of plain paths,
  herestrings/heredocs (piped heredocs wrap in a subshell group so the
  pipe never glues onto the terminator line).
- Assoc arrays live in the `vars` map under `base[key]` (static and
  dynamic keys, `${!map[@]}`, `${arr[@]:off:len}` incl. negatives,
  `pVal`/`pList` homes); integer (`-i`), case (`-l`/`-u`), nameref
  (`-n`), export (`-x`) attributes; `local -a arr=("$@")`.
- Shell-function calls in statement, condition, and capture position
  (in-process closures via `fArgs`); `typeset -f/-F/-p` display;
  background jobs (`go func()` + `__wg`, `wait` joins).
- `exit N` in statement, chain, and value position; `cd`; `eval
  "name=$(…)"` single-assign readback; `${var:off:len}` substrings;
  `~`/`$HOME`-prefix operands in tests; U+F800 private-use byte
  markers decode back to raw bytes.

Remaining `bash -c` sites (last-resort fallback for constructs the
renderer genuinely cannot lower — never a shortcut):

- `capRun(script, env)` — command substitutions with non-plain stages
  (variable commands, builtins other than echo/printf, redirects), and
  `$(…)` operands inside arithmetic strings (`$(( $(wc -l < f) + 1 ))`
  lowers to `s2i(capRun(…))` operands over native int64 operators —
  the blocking reason is the operand arrives as raw script text, not
  ShIR, so structural lowering would require re-parsing).
- `redirRun(script, env)` — statements ditto (pipelines with redirects,
  `eval` of arbitrary code, `source`, dynamic tests).

Each renders through `cmd_text` reconstruction with substitution-aware
quoting (quotes inside `$(…)` stay bare); scalars ship via env, arrays
do NOT cross into the child (assoc/indexed homes stay in-process —
a child-side array assignment is lost, which is why `mapfile` and
single-assign `eval` lower natively instead).

## 1. Gate-policy artifacts (not renderer bugs)

- The go arm requires translation-rc 0 AND bash-rc 0 (it is not in the
  gate's rc-equality group with perl/sh/rust/zsh/c). Scripts that exit
  nonzero BY DESIGN (`057_case.sh` usage error, short-circuit scripts)
  can never pass until the gate moves go to equality. The renderer is
  rc-faithful (`os.Exit`, `st` tracking) so the migration is safe.
- `$0` is the `go run` binary path, not the script path (c/rust pass
  `exec -a` from the gate harness; go's arm does not). Needs a harness
  change, not a renderer change.
- `cd ..`-style tests read the ambient directory above the run CWD,
  which churns under parallel workers — flakes identically for bash.

## 2. Known semantic approximations

- Assoc iteration order is SORTED by key (bash's is hash order).
  Corpus sorts before comparing (`009_arrays.sh`); unsorted consumers
  may differ in order only.
- `declare -p` on sparse indexed arrays materializes holes as `""`
  (bash omits them); `-t`/`-g`/`-x` flags are not tracked for `-p`
  display (only `-a -A -i -l -n -r`).
- `typeset -f` needs reconstructible bodies; anything `cmd_text`
  refuses stays silent (no output rather than wrong output).
- `readonly` is tracked, not enforced (later writes overwrite; bash
  errors to stderr — gate-discarded — and keeps the value). No corpus
  file violates readonly.
- `local`/`declare` have no function-local scoping (one global home
  per name). Shadowing callers would read the global.
- `typeset -i x=$y` with a composite value (`y="1+2"`) coerces via
  s2i (`0`) instead of evaluating (`3`).
- `unset` of an integer/assoc home zeroes/empties it; bash restores
  dynamic typing (later string assigns stay numeric/assoc-typed).
- Integer typing is whole-program: a `declare -i` anywhere types the
  home for earlier statements too (bash would treat pre-declare
  assigns as strings).
- Namerefs resolve static plain targets only (`-n ref='$dyn'`,
  element targets, and use-before-define inside not-yet-rendered
  closures fall back to plain homes).
- `wait pid` waits for ALL background jobs; `wait -n` waits for all.
- Background bodies share homes (no subshell snapshot); concurrent
  map access with assigning bodies could race (corpus bodies are
  sleep/echo).
- `$(exit N)` in capture position assigns empty and continues (bash
  exits); `break`/`continue` levels (`break 2`) lower as level 1.
- `select`, coprocessors, `~user/` tildes, terminal/job-control
  builtins (`fg`, `bg`, `jobs`, `kill %n`, `$!`), and `caller`/`shopt`
  beyond nocasematch are unlowered.
