# go backend — what still forks a child, and why

`AGENTS.md` §2: a construct is TRANSLATED into Go's own idioms whenever the
renderer can express it; fork/exec is the last resort, and **every remaining
fork/exec site is catalogued here with the reason it is still there**. The list
is meant to shrink.

The rendered output's runtime namespace is `s2s`, `runCmd`, `capCmd`,
`capRun`, `redirRun`, `redirCap` (plus `runPipe`/`capPipe`, which are not in
`harness/backend_runtime_metric.sh`'s seam but fork too). Measured with
`harness/backend_runtime_metric.sh go`.

## Lowered natively already

| construct | Go idiom | notes |
| --- | --- | --- |
| `rm [-f] [-r\|-R] [--preserve-root] path…` | `os.Remove` / `os.RemoveAll` | `-f` swallows only `os.IsNotExist`; `-r` without `-f` adds an `os.Lstat` existence check, because `os.RemoveAll` cannot distinguish "gone" from "never there" the way `rm -r` does. Mode/flags outside that set (`-v`, `-i`, `-d`, `--one-file-system`, `--`) refuse. |
| `mkdir [-p] dir…` | `os.Mkdir` / `os.MkdirAll` | mode `0o777` so the umask decides, as it does for the child. `-m MODE` (takes an operand) and `-v` (writes stdout) refuse. |
| `touch file…` | `os.OpenFile(O_CREATE)` + `os.Chtimes(time.Now())` | zero fork sites left in the corpus; shapes outside the accepted flag/operand set keep the exec. |
| `cd` | `os.Chdir` | |
| `echo` / `printf` | `bprintf` | |
| `exit` | `os.Exit` | |
| `exec` builtins with no binary (`true`, `:`, `read`, `local`, `shift`, …) | inline Go | see `exec_builtin_stmt` / `chain_builtin`. |
| `echo … > f` / `>> f` | `os.WriteFile` / `os.OpenFile(APPEND)` | one fd-1 file redirect only; `>&`, `2>&1`, a second redirect refuse. |
| `printf fmt args… > f` / `>> f` | `os.WriteFile`(`bprintfStr(…)`) | the same `bprintfStr` the statement and capture forms use. Static non-dash format without `%q` only (the external-agreement gate from `native_stage_builtin`); `-v`/`--`/dynamic formats keep the weld. |
| builtin / fs-cmd with only `2>/dev/null` discards | the builtin's own native statements | the helpers never write stderr, so the discards drop out (`builtin_fs_speculative`, in statement, chain, and status-value positions). Anything beyond pure discards keeps the weld. |
| `test`/`[` argv form with static-literal or single-`${…}` operands | `test_render` | e.g. `test foo = bar` → `("foo" == "bar")`, `test /path -ef /other` → `fileSame(…)`. Operands that could have glob-expanded (`*?[`), contain expansion syntax, or are composed (`$d/a`) keep the exec. |
| arithmetic, `let`, parameter expansion, glob matching | inline Go / pure-Go helpers | no child. |
| single external command with argv expressible without a shell | `runCmd(exec.Command(…))` | **still a fork** — the command itself is external. |
| a pipeline whose stages are all single execut-able commands | `runPipe` / `capPipe` | **still forks** each stage. |
| `$( … )`, `x=$( … )` where the body is one extern command | `capCmd(exec.Command(…))` | **still forks**. |

## Remaining fork/exec sites

### 1. `runCmd(exec.Command(cmd, argv…))` — one fork per external command

The bulk of these are commands for which Go has no faithful equivalent, or
whose stdout text is part of what the equivalence gate compares:

| command | ~sites | blocking reason |
| --- | --- | --- |
| `cmp` | 20 | on a difference GNU `cmp` prints `FILE1 FILE2 differ: byte N, line M` to **stdout**, and the byte/line numbers must match exactly. Reproducing that message (and the `--print-bytes`/`-s`/`-l` variants) needs a byte-exact port of GNU cmp, not `bytes.Equal`. |
| `grep` | 12 | output text is the observable behaviour (`-n`, `-c`, `-o`, `-A/-B/-C`, `-w`, `-x`, `-F`, colour-free formatting). `reMatch` covers only the *predicate* use, not the output. |
| `bash -c` / `bash file` | 8 | the construct *is* a nested shell; a native lowering would be a shIR→shIR transform (see `src/transforms/`), not a renderer change. |
| `test` (argv form) | 2 | both are `test $d/a -ef $d/b`-style composed operands (`$var/suffix`): `test_argv_bool` lowers static literals and single `${…}` operands natively, but a composed word has no operand form `test_operand` resolves faithfully (it would misread as `os.Getenv("d/a")`), so they keep the exec. |
| `ls`, `cat`, `diff`, `perl`, `paste`, `comm`, `ln`, `pwd`, `dirname`, `basename`, `readlink`, `uname`, `date`, `sha*sum`, `find`, `hostname`, `which`, `dpkg`, … | ~60 | either output-format sensitive (`ls`, `diff`, `find`, `sha*sum`) or a one-call native lowering not yet written (`pwd`→`os.Getwd`, `readlink -f`→`filepath.EvalSymlinks`, `dirname`/`basename`→`filepath.Dir`/`Base`, `uname -m/-s`→`runtime.GOARCH`/`GOOS` — note bash `uname` spells `Linux`/`x86_64`, Go spells `linux`/`amd64`, so the mapping needs an explicit table). |

`rm`/`mkdir` operands from a **brace** expansion (`mkdir -p project/{src,test}`)
or a **glob** (`rm -f file_*.txt`) still take the fork: the brace case expands
to several argv words and the glob to an argv only the shell knows, and the
shape validators refuse rather than guess (`REFUSE > GUESS`). Both are
mechanical extensions (brace: expand before validating; glob: a Go loop over
`globArgs`).

### 2. `capCmd(exec.Command(…))` — `$( … )` of an external command

| command | ~sites | blocking reason |
| --- | --- | --- |
| `mktemp` | 66 | almost all are the renderer's own `__ps_tmpN` temporaries for process substitution `<(...)`: the shell needs a real path to hand to a child's redirect. A native `os.CreateTemp` needs a statement block (create, close, take `.Name()`), not one expression. |
| `echo` | 33 | mostly a *stage* of a reconstructed pipeline (`echo … \| grep …`), so it is the pipeline that is unlowered, not the echo. The fully-literal ones (`$(echo "x")`) could be folded to a Go string literal at render time. |
| `uname`, `readlink`, `date`, `pwd`, `hostname`, `whoami` | ~20 | see the table above: output spelling differs from the Go API (`uname -m` is `x86_64`, `runtime.GOARCH` is `amd64`), or the value is environment-dependent in a way the gate compares. |
| `cat`, `grep`, `sed`, `find`, `ls`, `diff`, `comm`, `tail`, `egrep` | ~15 | same as the `runCmd` rows. |

### 3. `capRun(script, env)` — `$( … )` whose body is not one command

One `bash -c` child **plus an env literal** for only the written vars the
script text provably reads (`env_lit_for` scans `$name`/`${name}`/assign
targets; anything dynamic — `eval`, `source`, `. f`, indirect/nested
expansions — keeps the complete list). `eval`/`source` statement arms always
pass the full list: the code they run is not in the text. It takes over when
the body is a subshell, has a redirect, is a loop/conditional, or is a
builtin the renderer cannot capture natively. Lowering these means lowering
the *statement* forms (redirects, subshells, pipelines of builtins) natively,
which is the real remaining work and is being taken one construct at a time.

### 4. `redirRun(script, env)` — statement with a redirect or non-simple body

Same story, statement position: `( … ) > f`, `cmd > f` where `cmd` is not a
single exec with stdio-weldable redirects, `for`/`while` loop bodies with
redirects. Each carries the same env literal.

### 5. `runPipe` / `capPipe` — pipelines

A stage list that is not all single execut-able commands (a builtin outside
`native_stage_builtin`, a subshell, a redirect inside a stage) falls back to a
`bash -c` pipeline. Extending `native_stage_builtin` (currently `echo`,
`printf`) is the incremental route.

## How to shrink this list

1. Prefer a construct the runtime handles **one call at a time** (like `rm` →
   `os.Remove`) over adding a bulk helper — a bulk helper lowers the metric
   without lowering anything natively.
2. If the construct generalizes across backends (`cat FILE`, `grep -o`, seq
   ranges …), write it as a **shIR→shIR transform** under `src/transforms/`
   and queue the registry link through `core-requests/`, so every backend
   benefits.
3. Keep the renderer's refusal strict: an argv outside the exact accepted
   shape keeps the fork/exec. `REFUSE > GUESS`.
