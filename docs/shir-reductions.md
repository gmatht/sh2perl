# ShIR reduction catalogue — bash idioms → primitive nodes

This is the core's candidate table: common bash text idioms lowered to
compositions of the primitive node types (StrLen, Split, ArrayLen, Join,
ArrayIndex, SubStr, Case, Contains, Trim, Repeat, RegCount, RegReplace,
MapChars, StartsWith, EndsWith, ArraySlice). Backends implement the leaves;
the core picks the composition (see docs/shir-primitives.md).

Each row notes the **data-source dependency** where the reduction differs
(file/stream vs literal string), per the source-aware planner.

## Length & substring

| Bash idiom | Primitive composition | Example in corpus |
|-----------|----------------------|-------------------|
| `${#var}` | `StrLen(var)` | `bench-strlen.sh`, `051_primes.sh` |
| `${var:N}` | `SubStr(var, N, all)` | `${z:5}` |
| `${var:N:M}` | `SubStr(var, N, M)` | `010_substring_loop.sh` |
| `expr substr "$x" a b` | `SubStr(x, a-1, b)` | |

## Case transform

| Bash idiom | Primitive | Corpus |
|-----------|-----------|--------|
| `${var,,}` | `Case(var, lower)` | `000__04f`, `058` |
| `${var^^}` | `Case(var, upper)` | `${name^^}` |
| `${var^}` / `${var,}` | `CaseFirst(var, upper/lower)` | |
| `tr 'a-z' 'A-Z'` | `Case(str, upper)` | `000__06`, `000__04c` |
| `tr 'A-Z' 'a-z'` | `Case(str, lower)` | |
| `tr 'abc' 'xyz'` | `MapChars(str, abc, xyz)` | |

## Replace & substitution

| Bash idiom | Primitive composition | Corpus |
|-----------|----------------------|--------|
| `${var//pat/repl}` | `RegReplace(var, pat, repl, all)` | `${var//o/0}`, `${name// /_}` |
| `${var/pat/repl}` | `RegReplace(var, pat, repl, first)` | |
| `sed 's/pat/repl/'` | `RegReplace(str, pat, repl, first)` | `sed 's/'` |
| `sed 's/pat/repl/g'` | `RegReplace(str, pat, repl, all)` | `sed 's/\t/ /g'` |
| `sed 's/^ *//'` / `s/ *$//` | `Trim(str)` | |
| `tr -d 'c'` | `RegReplace(str, c, "", all)` | `tr -d '\n'` |

## Count (source-dependent!)

| Bash idiom | literal-string source | stream/file source |
|-----------|----------------------|--------------------|
| `wc -l` | `RegCount(str, /\n/)` | `readlineLoop { i++ }` |
| `wc -w` | `ArrayLen(Split(str, /\s+/))` | `readLoop { i++ per word }` |
| `wc -c` | `StrLen(str)` | `readLoop { bytes += len(chunk) }` |
| `find … \| wc -l` | — | `readlineLoop` (13 corpus uses) |

## Substring test (contains / prefix / suffix)

| Bash idiom | Primitive | Corpus |
|-----------|-----------|--------|
| `grep -q P` | `Contains(str, P)` | `015_grep_advanced` |
| `case $x in *P*)` | `Contains(x, P)` | `063`, `063_07` |
| `[[ $x == *P* ]]` | `Contains(x, P)` | |
| `[[ $x == P* ]]` | `StartsWith(x, P)` | |
| `[[ $x == *P ]]` | `EndsWith(x, P)` | |
| `grep -q ^P` | `StartsWith(line, P)` | |
| `grep -q P$` | `EndsWith(line, P)` | |

## Field / path extraction

| Bash idiom | Primitive composition | Corpus |
|-----------|----------------------|--------|
| `cut -d, -f2` | `ArrayIndex(Split(str, ","), 1)` | `000__06`, `bench-cut` |
| `cut -d: -f1,3` | `Join(Filter(ArrayIndex…), ":")` | `064_hard_to_generate` |
| `basename p` | `ArrayIndex(Split(p, "/"), -1)` | `000__04a`, `999_pwd` |
| `${p##*/}` | `ArrayIndex(Split(p, "/"), -1)` | `parse-dollar-brace-hash-hash` |
| `dirname p` | `Join(ArraySlice(Split(p,"/"),0,-1), "/")` | `058` |
| `${p%/*}` | `Join(ArraySlice(Split(p,"/"),0,-1), "/")` | |

## Lines (head / tail / join)

| Bash idiom | Primitive composition | Corpus |
|-----------|----------------------|--------|
| `head -n 5` | `Join(ArraySlice(Split(s,"\n"), 0, 5), "\n")` | `063_11`, `065` |
| `tail -n 5` | `Join(ArraySlice(Split(s,"\n"), -5), "\n")` | `063_20` |
| `printf '%s\n' a b` | `Join(Array(a,b), "\n")` | |
| `paste -sd,` | `Join(arr, ",")` | |

## Structure (sort / uniq)

| Bash idiom | Primitive composition |
|-----------|----------------------|
| `sort` | `Sort(Array)` (requires the split-then-sort shape) |
| `sort -u` | `Sort + Uniq(Array)` |
| `uniq` | `Uniq(Array)` |
| `comm a b` | set-op over `Array` |

## Encoding / misc

| Bash idiom | Primitive composition |
|-----------|----------------------|
| `echo "$x" \| xargs` | `Trim(x)` |
| `printf '%5s' '' \| tr ' ' c` | `Repeat("c", 5)` |
| `yes line \| head -n 3` | `Repeat("line\n", 3)` |

## Reduction-graph edges implied by this catalogue

These rows are the edges the core's planner walks. Each is verified once
against bash; the backend implements the leaves.

- `CountLines` → `RegCount(/\n/)` (string source) **or** `readLoop i++` (stream)
- `CountWords` → `ArrayLen(Split(/\s+/))` **or** `readLoop`
- `CutField` → `ArrayIndex(Split(delim))`
- `Basename` → `ArrayIndex(Split("/"), -1)`
- `Dirname` → `Join(ArraySlice(Split("/")), "/")`
- `Trim` → `RegReplace(^\s+ and \s+$)` **or** a dedicated `Trim` leaf
- `ToLower/ToUpper` → `Case`
- `ReplaceAll` → `RegReplace(..., all)`
- `Sort` → `Sort(Array)`
- `Head/Tail` → `ArraySlice + Join`

The planner keeps these as candidate (op, source) paths; a backend reaches
the ones it can express from its `nodes.txt` manifest, recursively, and
falls back to `sh2.*` / the original command otherwise.

## Implementation status (as-built)

These reductions are implemented in `text_ops` and verified **byte-exact vs
bash** at statement level:

| Idiom | Node | Status |
|-------|------|--------|
| `${#v}` | StrLen (param `len` / getVar `#v`) | ✅ |
| `${v^^}`/`${v,,}` | CaseTransform | ✅ |
| `${v:N:M}` | SubStrExtract | ✅ |
| `${v##*/}`/`${v%/*}`, `basename`/`dirname` | PathName | ✅ (statement level) |
| `cut -d, -fN` | FieldExtract | ✅ |
| `tr 'A-Z' 'a-z'` | CaseTransform | ✅ |
| `tr 'a' 'b'` | CharTranslate | ✅ |
| `sed 's///'` | RegSub | ✅ |
| `grep -q P` | StringContains | ✅ |
| `wc -c` | StrLen | ✅ |
| `wc -l` | RegCount(text+\"\\n\") | ✅ |
| `wc -w` | ArrayLen(Split(/\s+/)) | ✅ |
| `head`/`tail -n` | TakeLines | ✅ |
| `xargs` | StringTrim | ✅ |
| `yes X | head -n K` | RepeatStr("X\\n", K) | ✅ |
| `head -n K F` | ForEachLine(F, Output(l), limit=K) — early-exit streaming head | ✅ |
| `grep -c P F` | ForEachLine(F, guarded n+=1) → count | ✅ |
| `printf 'X%.0s' ARGS…` | RepeatStr("X", static-count incl. brace ranges) | ✅ |
| `x=$(echo X \| cut/tr/wc/sed …)` | capture-assign → value composition + SetChildError(0); grep excluded (status idiom) | ✅ |
| `echo "$v" \| cut/tr/sed/wc/head/tail` | variable-source pipelines (single getVar/interpolated echo arg) reduce natively | ✅ |
| `grep P F \| cut flags` / `cat F \| cut flags` | ForEachLine(F, if Contains(l,P) then Output(FieldExtract)) — ALL cut args passed (glued-flag filter used to drop ':','2') | ✅ |
| `head/tail -c N` on static text | folds to the EXACT byte substring (already_nl=true — never re-append) | ✅ |
| `tr -s SET` | squeeze = run collapse to ONE char (`replace(/c+/g,"c")`), identity translate when SET2 empty; `-d -s` and range squeeze sets refuse → runtime | ✅ |
| `cut -dD -fN-M` / `-fN,M` | FieldExtract ranges: split + ascending index pick, no-delim line passes whole, missing trailing fields dropped — native in estree/java/perl | ✅ |

**Newline-exactness rule (as-built):** the stage-1 extractor records whether
bash's source output ends with a terminal newline (echo: always; printf:
last literal; bare literals: their own text). Line-oriented ops pass that
through so the Output wrapper re-appends ONLY an echo-stripped newline; wc
always prints its own terminating newline; byte-takes fold to exact bytes
and never append.

**Backend renderings of ForEachLine (all O(1) memory, zero fork/exec):**

| Backend | rendering | verified |
|---|---|---|
| js/estree | `sh2.eachLine(src, (l) => {…}, limit?)` — readline over createReadStream in the runtime | ✅ byte-exact vs bash |
| perl | `open my $fh,'<',src or die; while (my $l = <$fh>) { chomp $l; … } close $fh;` — fresh per-loop handle, loop vars never scalar-hoisted | ✅ byte-exact vs bash |
| java | `try (BufferedReader __r = new BufferedReader(new FileReader(src))) { String l; while ((l = __r.readLine()) != null) { … } }` (+ counter break for limit) | ✅ compiled + run, byte-exact vs bash |
| zig | reader loop — IrStmt::Ext still panics (documented gap below) | ❌ pending |

**Scope boundary (updated):** capture-internal reduction NO LONGER falls
back wholesale — `try_reduce_capture_assign` reduces the ASSIGN's
expression (`x=$(echo X | cut/tr/wc/sed …)` → value composition +
`SetChildError(0)`), preserving the trailing-newline strip and `$?`. grep
stays excluded from captures by design (status idiom, not a value idiom).
Dynamic-source pipelines reduce to ForEachLine streaming compositions;
anything the core cannot reduce falls back explicitly to the runtime
(`sh2.fieldExtract`, `sh2.pipelineSync`, …) — never an eager slurp.

**Not yet reduced (fall back to original):** `tail F` (cannot stream), `sort`/`uniq`, `seq | head`,
`awk`, `[[ $x == P* ]]` (test-string parsing), multi-stage pipelines with a
dynamic source beyond the two-stage grep/cat forms above.
