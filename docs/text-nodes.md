# The text nodes, as `rs` renders them

Classes 3 and 7 of the campaign (`quoting`/`word splitting` and `text ops`) are
carried by the shared `text_ops` family of Ext nodes. The transform already
built those nodes for every backend; what a backend needs is a *manifest*
declaration plus a real render arm. `rs` declares every node in
`planner::node_manifest("rs")` and renders it in `rust_backend.rs`
(`ext_std_str`/`ext_std_num` and the `expr_str` arms); the two lists are kept
in one-to-one sync by `planner::tests::node_manifest_names_have_real_renderers`
and `every_declared_node_is_a_known_node_type`, and the *runtime* half is
pinned by `rust_backend::tests::offered_text_nodes_execute_like_bash` (render →
`rustc -O` → compare stdout/stderr/status with bash, byte for byte) and
`constructed_text_nodes_execute_like_bash` for the nodes that no transform in
this pipeline builds from shell source.

## Per-node table

| node | Rust arm | the rule (where bash and Rust disagree, this is what the arm does) |
|---|---|---|
| `StrLen` | `.len() as i64` | **BYTE** length — the node's denotation (`TextOps#lowered_wc_sound`: `denote StrLen_n s = length s` over the byte sequence) and `wc -c`. Bash's `${#s}` counts CHARACTERS under a UTF-8 locale, so the two disagree on non-ASCII text; the transform lowers `${#s}` to this node too, sound where bytes == chars (ASCII). Switching the arm to `.chars().count()` would instead make every non-ASCII `wc -c` wrong, so the byte reading stays and the divergence is the comment at the arm. |
| `WordCount` | `split_whitespace().count()`-shaped scan | bash counts words as maximal runs of non-IFS-whitespace; the arm scans the same class (space, tab, newline). |
| `RegCount` | newline scan | `wc -l` is a count of `\n`, not `split('\n').len()` (off by one on a trailing newline); `echo -n x \| wc -l` is 0. |
| `Split` | the IFS split (`split_ifs_inline`, or `__sh_split_ifs` when the program assigns IFS) | see "word splitting" below. |
| `ArrayLen` | `len()` of the split | `"${a[@]}"` counts elements, and an empty expansion is 0 elements, not one empty word. |
| `CharTranslate` | `chars().map(..)` over the EXPANDED map | `tr` is a BYTE map: its ranges/classes are expanded at lift time, and `tr '[:lower:]' '[:upper:]'` leaves `É` alone where `to_uppercase()` would not — that is why `tr` lowers here and not to `CaseTransform`. |
| `CaseTransform` | `to_uppercase()`/`to_lowercase()` | the BASH PARAMETER casing (`${v^^}`, `${v,,}`), which case-maps code points. |
| `FieldExtract` | split-and-index over ONE line | `cut -dD -fLIST`: a line with no delimiter prints WHOLE; a field past the end is empty; `-f2-` and `-f2-3` are ranges. |
| `PathName` | strip trailing `/`, then cut at the last `/` | GNU `basename`/`dirname`, including `basename /` → `/`, `dirname x` → `.`, and `dirname /a/b/` → `/a`. The node is only built from a real command (a flagged call keeps its command form), never from `${p##*/}` — that strip is a glob and differs on a trailing slash. |
| `SubStrExtract` | char-indexed `skip`/`take` | `${v:off:len}` indexes CHARACTERS; a negative offset counts from the end (bash: `${v: -3}`), a negative length is 0, an offset past the end is empty. |
| `TakeLines` | line iterator with `take(n)` | `head -n N`: fewer lines than N is the whole input, and a final line without `\n` is still a line. |
| `StringTrim` | `trim()` | `${v# }`-class whitespace trim, both ends. |
| `StringAffix` | `starts_with`/`ends_with` over the literal affix | the `[[ $v == P* ]]` predicate; a 1/0 spelling in a value context. |
| `AffixStrip` | `strip_prefix`/`strip_suffix`, text passed through on no match | the node's pattern is an arbitrary affix, but its only producer emits it for a magic-free literal; a star lowers to `CutAt` and `?`/`[`/`\` are refused. |
| `CutAt` | `find`/`rfind` + slice | the four single-star strip identities (`${v%%lit*}`, `${v%lit*}`, `${v#*lit}`, `${v##*lit}`): `*` matches the empty string, the shortest/longest match picks the cut point, and no match leaves the text unchanged. |
| `CutsetTrim` | `trim_matches(\|c\| cutset.contains(c))` | `${v##[ab]}`-class cutset trim; `left`/`right` select the one-sided form. |
| `RepeatStr` | `{t}.repeat(n)` with `n <= 0` → empty | `yes x \| head -n N` is N copies of the line; a non-positive count prints nothing. |
| `SortLines` | stable `sort_by` over the lines | numeric or lexical key, `-r`, `-u`; the corpus's `SortLines` case is lowercase-only input because `sort` collates by locale and the render must not pretend to a different order. |
| `UniqLines` | run walk (`windows`) | adjacent runs only (never a global dedup), `-s` projection, `-c`'s width-7 prefix. |
| `NewlineStrip` | `trim_end_matches('\n')` | `$(cmd)` strips ALL trailing newlines, not one. |
| `StringContains` | `.contains(pat)` | `${v#*p*}`-class containment predicate. |

## Word splitting (class 3)

An unquoted expansion splits on IFS. The renderer spells bash's DEFAULT
(`" \t\n"`) as a compile-time `split` closure — the class, empty runs
collapsed, no leading/trailing field — so the emitted program carries **no**
`__sh_split_ifs` for it. When the program ASSIGNS `IFS` anywhere (any block,
any function body — `assigns_ifs` walks them all), the separator set is a
runtime value, so the split goes through `__sh_split_ifs`, the one
implementation of bash's rule: whitespace runs collapse, a non-whitespace
separator keeps an empty field (`a::b:` → `a`, ``, `b`), a trailing separator
adds none, and `IFS=` disables splitting entirely. `__SHV_IFS` starts at
`" \t\n"` in `decl_stmt`, because a split rendered before the first assignment
must still see bash's default.

## Not declared, and why

* **`RegSub`** (`sed s///`, `${v/pat/rep}`) and **`RegexpFind`** (`grep -o RE`):
  `std` has no regex ENGINE, and the nodes' specs are over a COMPILED matcher
  (POSIX BRE/ERE with backreferences, greedy `*`, character classes). A
  hand-rolled matcher would be exact on a subset and silently wrong on the
  rest; the `regex` crate the emitted Cargo project uses is not `std` and is
  leftmost-first, not POSIX leftmost-longest. These commands therefore keep
  their command form (class 4, "reducible, not eliminable").
* **`ReadLine`** as a value node: the `read` builtin carries an exit status
  (1 at EOF) the node's denotation does not, so the renderer reads the line
  natively inline where the status is observable.
* **`CutAt`** is deliberately not a manifest node: declaring it re-routes
  unrelated echoes (the manifest is consulted by the pipeline, not only by
  the fold); the fold's own `cut_target_ok` gate owns the offer.

## Census (548 renders, `330660db3` → the class-3/7 commit)

| metric | before | after |
|---|---|---|
| programs emitting `bash -c` | 223 | **191** |
| `__sh_spawn`/`__sh_run` call sites | 791 | **617** |
| distinct `__sh_*` callees | 58 | **43** |
| helper definitions | 1315 | **917** |
| `__sh_*` call sites | 3804 | **2732** |
| helpers-free programs | 192 | **263** |
| `__sh_q` | 226 (77 programs) | 219 (74) |
| `__sh_wq` | 140 (45) | 110 (43) |
| `__sh_split_ifs` | 80 (36) | **2 (1)** |
| `__sh_read_fields` | 18 (9) | **0** |

The 2 remaining `__sh_split_ifs` sites are the correctness fix above: a
program that assigns IFS must consult it, where a compile-time closure would
split on the wrong characters.

## What still shells out after this pass

Every remaining spawn is a construct whose real semantics is not a text node:

* `grep` (56 sites), `sed` (10) — class 4 regex; no `std` engine.
* `awk` (11), `perl` (4), `xargs` (3), `bc` (10) — real interpreters.
* `cat` (19), `ls` (16), `find` (5), `stat` (2), `gzip`/`zstd` (7),
  `strings` (2), `tee` (2), `rm` (2) — command emulation / stat, the next
  pass's fs family.
* `sort`/`wc`/`head`/`cut`/`tr` still appear in ~40 sites whose SHAPE the
  transforms do not build a node for: a pipeline whose input is a bare
  variable (`echo "$v" | cut -d: -f1`), a loop body piped to `sort`
  (`done | sort`), a dynamic count (`head -n "$max"`), a process substitution
  with FILE operands (`diff <(sort a) <(sort b)`), or a here-string
  (`tr ... <<< "$s"`). Widening those lifts is transform work, not renderer
  work, and is left for that pass — the renderer already has the arms.
