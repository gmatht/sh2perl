# fish backend — gate diagnosis (2026-10-09)

Every verdict below was produced by running the command in this session (in this
worktree, with the binary built from the working tree), not by reading a
previous session's summary.

## 0. The premise "failing to build" is boilerplate

`setup_backends.sh` prints `The <lang> backend in <dir> is failing to build.`
for **every** red gate, whatever the cause. It is not a statement about `cargo`.

| Check | Result |
|---|---|
| `cargo build` | 0 errors (warnings only) |
| `cargo build --bin shir_render` | 0 errors — the gate's entry builds |
| renderer entry probe | `shir_render --target=fish -` answers (invalid shIR → `ShIR JSON ingress`, rc 1, by design) |
| `cargo test --lib fish_backend` | **6 passed, 0 failed** (3 of them added here) |

`cargo test --lib` (whole crate) is `1038 passed; 14 failed`. The 14 are
`c_backend::*`, `estree::*`, `ir::*` and `shir::range_analysis_tests::*` —
**none** fish-related, and **verified pre-existing this session**: reverting
`src/fish_backend.rs` to HEAD and re-running gives the same 14 failures
(1035 passed — the delta is exactly the 3 tests added here).

## 1. The red was the corpus RENDER leg: 3 files refused to render

`642/652 corpus render OK, 3 fail (0 stubs, 0 equiv, 0 static-analysis), 7 skip`.
Since the stub and equiv counts were 0, the 3 had to be *render* failures, and
they were — each exits 1 with no output, which is what the gate counts as a fail:

```
$ otranspilerl-cli --target shir examples/<f> | shir_render --target=fish -
shir_render: fish: param op not renderable: "+"          # variable-plus-operator.sh
shir_render: fish: redirect mode not renderable: "wc"    # parse-redirect-clobber.sh
shir_render: fish: sh renderer: Ext node unsupported     # 070_gnuisms_thorough.sh
```

| file | root cause | fix |
|---|---|---|
| `variable-plus-operator.sh` | `param_to_fish` had no `+` / `:+` arm, so `${V+x}` fell to the catch-all refusal | new arm (+ `setness_guard_fish` / `set_if_fish`) |
| `parse-redirect-clobber.sh` | `redirect_to_fish` knew `r`/`w`/`a`/`r+` but not the ClobberOutput mode string | `"wc" => ">"` |
| `070_gnuisms_thorough.sh` | `stmt_to_fish` / `stmt_inline` refused **every** `IrStmt::Ext`; the `cat FILE` transform emits `ForEachLine` (strategy §2 of `shir-primitives.md`) | new `ext_node_to_fish` (ForEachLine), wired into both paths |

### 1a. `${v+w}` is the SETNESS predicate, and the spelling is load-bearing

`+` means "v is set" (a variable assigned `""` **is** set); `:+` means "set AND
non-empty". The core pins the two as distinct and forbids either degrading to
the bare variable — `ir.rs::param_plus_lowers_to_a_set_test_not_the_value`
records the silent wrong answer that produced. fish:

* `set -q NAME` is fish's setness test and nothing else → the `+` guard
  (measured: unset → rc 1, set-but-empty → rc 0);
* `test -n "$NAME"` is false for unset **and** empty → the `:+` guard.

The expansion is emitted as the **quoted `$()`** form, and both parts matter
(both measured against fish 3.7.0):

```
test -n "$(set -q V; and printf '%s' x; or printf '%s' '')"
```

* **quoted**: the guard-failed case prints nothing, and an unquoted command
  substitution then expands to ZERO words — `test -n "${V+x}"` becomes a
  one-argument `test -n`, which is TRUE, and the guarded branch runs for an
  UNSET variable (the exact silent wrong answer above);
* **`$()` not `(…)`**: fish does **not** expand a bare `(…)` inside double
  quotes. `"(cmd)"` is literal text. The first cut of this fix used the plain
  `(…)` the rest of the file uses, and the guard string was the 40-character
  literal `(set -q V; …)` — non-empty, therefore unconditionally true. Caught by
  running the render under fish, not by reading it.

Verified end to end on the corpus file: unset, set (`V=1`) and set-but-empty
(`V=`) all print exactly what bash prints.

## 2. `>|` is mode "wc", and fish needs no distinct spelling

`shir.rs` gives `RedirectOperator::ClobberOutput => ("wc", 1)`. bash's `>|`
differs from `>` only under `set -C` (noclobber) — and fish has no noclobber:
the `set` arm already lowers `set -C` to the no-op `:`. Plain `>` is therefore
the faithful fish form of both. Verified: `parse-redirect-clobber.sh` renders
`… >"$tmpf"` and its stdout + rc match bash.

## 3. ForEachLine: the `source` is a PATH, so it is READ, not executed

`field source: expr  # file path expression` (`docs/shir-primitives.md` §2), and
every producer writes a path (`cat_read.rs`: `IrExpr::Str(file)`; `text_ops.rs`:
`path` / `files[0]`). So:

```
while read -l VAR; BODY; end < PATH          # limit: None
head -n N PATH | while read -l VAR; BODY; end  # limit: Some(N) — streaming early exit
```

* **not** `PATH | while …` — a bare path is not a command, that pipe would try
  to EXECUTE the file;
* **not** `< (cmd)` — fish has no process substitution and `<` needs a real
  file (which is why `limit` takes the `head` pipe instead);
* `read -l VAR` reads the line **verbatim** — measured: internal, leading and
  trailing spaces survive and `count` is 1 (it does not IFS-split), which is
  what a line iterator needs. End to end on a `cat FILE` source: stdout is
  byte-identical to bash, spacing included;
* `-l` (block-local) is right here because ForEachLine desugars a
  file-consuming COMMAND, so there is no bash-visible loop variable whose
  post-loop value could differ (unlike a user-written `while read` loop).

An Ext tag with no lowering still refuses, naming the tag (`refuse > guess`),
and never emits a stub.

## 4. The equivalence leg is NOT scored here — and installing fish changes that

The report row says `3 fail (0 stubs, 0 equiv, …)` **and** `equiv: 0 pass vs
bash`, but that suffix is not evidence the leg ran: it is
`${eq_gate:+…}`, and `eq_gate="0"` is a non-empty string, so it prints for the
disabled case too. `eq_gate=1` requires `command -v fish`
(`setup_backends.sh:1441`; fish is a `*)` scaffold arm, so `g_eq=1` and
`g_stubgate=1` at `:1367`). Every gate row in `gate-reports/backend-fish.report`
shows `0 equiv`, i.e. **the fish gate here measures render + stub only**. This
box had no `fish` binary until one was installed to verify the fixes above.

With fish 3.7.0 installed, the worktree-local mirror `harness/fish_gate.sh`
(which *does* run the equivalence leg) measures **461/652, 184 equiv failures,
0 render failures, 0 stubs**. Those 184 are **pre-existing and much larger than
this task**; two examples:

* the `_readlink()` / `_cmp()` polyfill prologues are emitted as **POSIX sh**
  (`case … esac`, `for … done`, `$(( ))`) into fish output, which is a fish
  PARSE error — `070_gnuisms_thorough.sh` now renders and is stub-free, but
  dies at line 15 *inside its polyfill*, before any of its own code runs;
* 28 corpus renders do not pass `fish -n` at all.

The three files fixed here: the first two now pass the equivalence leg too
(stdout **and** rc equal to bash); `070` passes the gate's legs (render + stub)
and remains blocked only by the unrelated polyfill defect above.

**CAUTION for whoever runs the gate next:** installing `fish` flips this
backend's gate from render+stub to render+stub+equiv, i.e. it turns a 3-fail
gate into a ~184-fail gate. The package was installed for the measurements
above and then removed (`apt-get remove --purge fish fish-common`) to leave the
box as found. Re-enable deliberately (`apt-get install -y fish`) only when
working on equivalence.

## 5. Verification and blast radius

* Canonical legs (render + `sh2.*`/`TODO(unsupported)` stub) over the full
  652-file corpus: **645 pass, 0 render-fail, 0 stub files, 7 skip, 0
  emit-fail** — i.e. `645/652 corpus render OK, 0 fail (0 stubs …)`.
* **Blast radius is exactly the three files**: over all 652 A1s, the `+`/`:+`
  param op occurs in 1 file, redirect mode `"wc"` in 1, and an `Ext` statement
  in 1 — the three above. The three edits are disjoint `match` arms, so no
  other file's output can change.
* Runtime delegation (`harness/backend_runtime_metric.sh fish`): **0 calls /
  560 files**, unchanged — the new lowerings use only fish builtins
  (`set -q`, `test`, `read`, `printf`) and plain `head`.

## 6. Not fixed here (reported, not silently worked around)

* the POSIX-sh polyfill prologues (`_readlink`, `_cmp`) — a fish PARSE error
  wherever they are emitted;
* a second, latent copy of the same bug class, found while auditing the
  redirect-mode table: the `wc -l FILE` command lowering (`fish_backend.rs`,
  the `"wc" =>` arm in the `wc` capability, *not* `redirect_to_fish`) emits
  bash — `while IFS= read -r _; do n=$((n+1)); done < F`. Measured: it fires on
  **0** corpus files, so it is a landmine rather than a current cause;
* `braced_to_fish`'s own `":+" | "+"` arm (`test -s "$V"` — a FILE test applied
  to a variable's value) and its `:-` sibling. Neither fires on the corpus
  (the gate's A1 carries the structured `param` call, handled by §1a), so
  changing them would alter currently-passing output for no measured gain;
* the unquoted `${V+x}` shape: the **core's** A1 already rewrites it to
  `param(":-", …)` before any renderer sees it (the pre-pass `ir.rs` warns
  about), so no backend-side fix is possible — shared core, reported here.

---

# Session 2 (same day) — the EQUIVALENCE leg is live now

`fish` 3.7.0 is installed, so `setup_backends.sh`'s fish arm now scores the
equivalence leg (see §4 above, which described the render+stub-only era).
Measured at this session's start and end with `harness/fish_gate.sh` (the
worktree-local mirror of the same contract):

| | pass | fail | stubs | skip |
|---|---|---|---|---|
| start | 563/652 | 82 (all equiv) | 0 | 7 |
| end | **587/652** | **58** (all equiv) | 0 | 7 |

`harness/backend_runtime_metric.sh fish` is unchanged at **0 calls / 560
files** — every fix below uses fish builtins (`count`, `string`, `set`,
`read`, `math`) or plain shell, never a runtime helper.

## The nine root causes fixed (24 files)

1. **Stale flat-array scalars in arithmetic.** Commit bee7b44ad made indexed
   arrays native fish LISTS (`set a 10 20 30`), but `ArithAst::Index`,
   `braced_arith_to_fish` (`${#arr[@]}`, `${arr[n]}`) and `arr_elem_ref` still
   emitted the retired per-element scalar — `$a_1`, `$arr_len`, `$args_i` —
   which now name UNSET variables. `$(( a[1] + 1 ))` printed 1, not 21;
   `while (( i < ${#args[@]} ))` never ran a single iteration. Now: fish's own
   1-based `$a[2]`, `$(count $a)`, and (for a computed index) an evaluated temp
   — fish does NOT do arithmetic inside `[ ]`, so `"$a[$i+1]"` is a runtime
   "Invalid index value". t85_arith_index, arith-array-index-expr,
   parse-arithmetic-nested-array, redirect-in-arithmetic, 063_09, …
2. **`${arr:-d}` re-parsed its own rendering.** The element-0 subject
   (`$LIST[1]`) was fed back through `var_ref_to_fish`, which keeps only
   identifier characters — so the guard tested the unrelated `$LIST1` and
   printed empty. parse-array-plusassign, parse-arithmetic-nested-array.
3. **`shift` and `set --`.** fish has no `shift` ("Unknown command") and
   `set -- a b c d` fell into the `set`-OPTIONS arm and became `:`. Both edit
   the positional list, which fish keeps in the ordinary variable `argv`
   (`set -e argv[1]`, `set argv a b c d`). id-cmdsub, tty-cmdsub,
   readonly-cmdsub, parse-at-slice, parse-at-slice-param.
4. **The core's `__sh2lena_` / `__sh2lens_` arith placeholders**
   (`shir.rs::native_arith_text_len`; estree has a Var arm for them, fish had
   none) were emitted as literal unset variables. Decoded in `arith_rewrite`,
   `arith_to_fish` and `var_ref_to_fish`; the spelling is `$( … )`, not a bare
   `( … )`, because it lands inside `math "…"`.
5. **Template quoting, twice over.** (a) `param("")` already renders one quoted
   word, and `interp_to_fish` quoted it again → `""$y"""extra"`, which fish
   reads as EMPTY. (b) Splicing that quoted word while a template quote was
   OPEN left the variable unquoted, and fish expands a word as the cartesian
   product of its parts — an unset variable contributes ZERO elements, so the
   WHOLE word vanished (unicode-in-string). parse-dollar-in-param-expand,
   unicode-in-string, parse-and-or-chain-with-assign, 000__04h.
6. **`grep -r PATTERN DIR --include=G` dropped the command word**: the arm
   rebuilt the line from `args`, but the command word is `cmd`, not an arg — so
   the render started at `-r` and fish tried to RUN `-r`. 017_grep_context.
7. **`mapfile`/`readarray` never received its input.** The bulk
   `set -l _mf (cat); set {name} $_mf` put the caller's appended `< tmp` on the
   trailing `set`, while `(cat)` inherited the SCRIPT's stdin (measured:
   `set lines (cat) < f` leaves `lines` EMPTY). Now a `while read -l` loop, so
   the redirect reaches the reader. 012, 041.
8. **The core's process-substitution cleanup clobbered `$status`**
   (`transforms/process_subst.rs::rm_exec` appends `rm -f $__ps_tmpN …` as the
   block's LAST statement, so `diff <(a) <(b); echo $?` read 0). The block arm
   snapshots `$status` before the cleanup (`is_ps_tmp_cleanup`) and re-asserts
   it after. process-substitution, 064_01, 070_cmp_basic. The same clobber
   existed in this renderer's OWN `lower_procsub_stmt_fish` single-substitution
   form, fixed the same way.
9. **The test module was flaky ~1 run in 5** (NOT caused by this session's
   changes; adding tests only made it visible). `shir_to_fish` publishes its
   array/assoc/int analysis in PROCESS globals and cargo runs tests on parallel
   threads. All renders now go through `render_prog`, which holds `RENDER_LOCK`.
   Verified deterministic over 5 consecutive runs; `cargo test --lib` is 1059
   passed / 14 failed, and those 14 are the same `c_backend`/`estree`/`ir`/
   `shir` ones that fail at HEAD (none fish).

## Pre-existing, NOT a regression

`harness/fish_gate.sh 062_hard_to_lex` fails at HEAD too. It renders
`ls -la | grep "^d" | head -5`, and the gate writes `_r.fish` into the scratch
dir BETWEEN the bash run and the fish run — so the two `ls` calls see different
directory metadata. `harness/fish_one.sh` writes the render first and reports
OK. A harness artifact, not a renderer bug.

## Remaining 58, by root cause (ranked, all still render+stub clean)

* **subshell / background scope** (049_local, 064_20, 048_subprocess,
  105_background_copy_semantics): `( … )` and `… &` render as `begin … end`,
  which in fish neither forks nor opens a scope, so `set`/`set -e`/`export`
  leak out.
* **dynamic function scope** (081, 064_10): a nested function cannot see the
  enclosing call's locals. fish's `function NAME --no-scope-shadowing` is the
  faithful bash semantics, but plain `local x=v` renders as `set x v`, which
  with that flag would write the CALLER's variable — the flag and a
  `local` → `set -l` change must land together.
* **`eval`** (eval-assign, 063_12, parse-eval-multiline): `eval "y=$x+1"` is
  emitted verbatim and fish cannot parse `name=value`.
* **`$(( … ))` in VALUE position** (redirect-in-arithmetic, t78, 064_06, 078,
  arith-postfix-status): fish's `math` refuses relationals, `?:` and
  assignment/`++`, and `(( i++ ))`'s status (the OLD value) is not modelled.
* **`IFS=` prefixes** (088, 064_07, 063_11) are dropped for `read`, which is
  right for `IFS=` (whole line) and wrong for `IFS=:` (field split).
* **bash-managed variables** (dollar-minus `$-`, param-expand-default-operator
  `$BASH_VERSION`, 064_21 `$HOSTNAME`).
* Singles: `${x::-2}` (parse-substring-double-colon), `printf -v`
  (printf-format-and-v), extglob `@(a|b)` (at-in-test), assoc keys containing
  `:`/`,` producing invalid fish identifiers (063_hard_to_parse, 058),
  `$(wc -l < /etc/passwd)` inside `$(( ))` (064_14).

**Not renderer-fixable** (recorded so nobody re-chases them): `dot-source-lib`
(writes BASH source at runtime and sources it), `096_head_procsub` (needs a
genuinely concurrent producer), `057_case`/`qx-var-builtin-cd`/`test_grep`
(`$0` and CWD are harness-determined), and the `grep`-is-really-`rg` bash shim
this box installs (`type grep` → a function calling `rg`), which makes
`parse-and-or-chain-with-assign`'s stderr differ.

---

# Session 3 — first the BUILD was broken, then eight lowerings

| | pass | fail | stubs | skip |
|---|---|---|---|---|
| start | 587/652 | 58 (all equiv) | 0 | 7 |
| end | **595/652** | **50** (all equiv) | 0 | 7 |

`harness/backend_runtime_metric.sh fish` is **unchanged at 0 calls / 560
files** — every fix below is fish builtins (`test`, `if`, `:`, `string
replace`/`sub`/`match`, `$hostname`), never a runtime helper.

## 0. "failing to build" was LITERAL this time — the target volume died

Not boilerplate. `/dev/sdf` (`/mnt/wsl/tmpbtrfs`, where this worktree's
`target` symlink pointed) took I/O errors, the btrfs transaction aborted, and
the kernel force-remounted it read-only:

```
btrfs_dev_stat_inc_and_print: BTRFS error (device sdf): bdev errs: wr 49, rd 23
BTRFS error (device sdf state A): Transaction aborted (error -5)
BTRFS info (device sdf state EA): forced readonly
```

`cargo build --bin shir_render` → `failed to open …/target/debug/.cargo-lock:
Read-only file system`, and the previously built `shir_render` binary
**segfaulted** (rc 139) because its pages live on that device. Same fault hit
every backend whose `target` pointed there (js, python, rust, sh, zsh).

Fix: repoint the worktree's `target` symlink at a healthy volume, exactly as
`c` and `perl` already do —

```
ln -sfn /mnt/wsl/projects/.fishtarget backends/fish/target
```

`target` is gitignored, and `setup_backends.sh`'s gate passes the symlink
straight to `build-lock.sh --share-target`, so nothing else had to change.
Then `cargo build --bin shir_render` (67 s, warm artifact cache) and the
gate's own renderer probe answered as designed (invalid shIR → `ShIR JSON
ingress`, rc 1). **Do not remount that device rw** — it is still faulting.

## 1. The eight lowerings (each verified by running both sides)

1. **`$(( a > b ))` in VALUE position** was a bare `(math …)`, and `math`
   answers every relational with "Logical operations are not supported, use
   `test` instead" on stderr and prints NOTHING — so the word vanished
   instead of expanding to 1/0. `arith_needs_test` detects a relational or
   ternary anywhere in the tree, and `arith_value_subst_fish` lowers it to
   fish's own `test`/`if`. `arith_cond_ast_fish` now delegates to the same
   function, so its plain case is byte-identical to before.
   redirect-in-arithmetic, t78_arith_ternary.
2. **`((i++))` statement STATUS.** `set` and `math` both succeed, so the
   statement carried status 0; bash's `(( … ))` status is the VALUE's
   truthiness and a POSTFIX increment yields the OLD value. The increment now
   re-asserts it — `test $i__sh2old -ne 0` for `i++`, `test $i -ne 0` for
   `++i`. arith-postfix-status.
3. **`${x/pat/repl}` replaced only the FIRST match; the renderer used `-a`.**
   The IR keeps `param("/")` and `param("//")` DISTINCT (measured on
   `y=one-two-one`), so the old comment claiming a conflation was stale:
   `-a` belongs to `//` only. 070_gnuisms_thorough (`sub=1-two-1` → `1-two-one`).
4. **A standalone redirect rendered as `'' >file`** — "Unknown command" in
   fish, so the file was never created at all (`stat` then failed and the size
   printed EMPTY where bash prints 0). `exec("")` now lowers to fish's no-op
   `:` — `: >file` truncates exactly as bash does. parse-standalone-redirect.
5. **`${x:off:-N}` (negative length) was DROPPED**, returning the whole tail.
   fish's negative `--end` is the same quantity (measured: `${x:0:-2}` on
   "hello world" = "hello wor" = `string sub --start 1 --end -2`).
   parse-substring-double-colon.
6. **extglob `@(a|b)` in `[[ ]]` matched nothing.** fish's `string match` is a
   GLOB matcher and reads `|` literally; `@(…)` consumes the whole word, so it
   is the anchored regex `^(a|b)$` (`-rq`). Only a glob-and-regex-free
   alternative list is lowered — otherwise the literal-glob form is kept
   (REFUSE > GUESS). at-in-test.
7. **`$HOSTNAME` → `$hostname`.** bash sets HOSTNAME in EVERY shell (it is not
   exported — a child bash sets it itself) and fish keeps the same value under
   its lowercase spelling, so this is the `$?` → `$status` rename, not a stub:
   `${HOSTNAME:-localhost}` printed the host under bash and the DEFAULT under
   fish. No corpus script ever ASSIGNS HOSTNAME, and the only other readers are
   quoted heredoc bodies where the text is literal (064_08 still passes).
   064_21.
8. **`$(cmd)` inside `$(( … ))` was rewritten as arithmetic identifiers** —
   `$(wc -l < /etc/passwd)` came out `$($wc -$l < /$etc/$passwd)`, which `math`
   rejected, so the value was empty. fish expands `$( … )` inside the
   double-quoted `math "…"` argument, so the substitution is now copied through
   VERBATIM (`cmdsubst_span` tracks quotes and nesting). 064_14.

## 2. Still open, and what is NOT worth chasing

* `harness/fish_gate.sh` reports 50 failing where `harness/fish_failmap.sh`
  reports ~49: the gate writes `_r.fish` into the scratch dir BETWEEN the two
  runs, so `062_hard_to_lex`'s `ls -la` sees different directory metadata
  (documented in Session 2; a harness artifact, not a renderer bug).
* **five of the remaining list are the box, not the backend**: `grep` is an
  exported/functions-file SHIM in this box's bash that dispatches to
  `rg` (color codes `\e[0m\e[1m\e[31m`, regex semantics). The reference side
  therefore is NOT GNU grep, and the fish side (real grep) cannot agree with
  it. Fixing those in the renderer would be fixing the harness.
* `cargo test --lib`: 1060 passed / **15 failed**, and every failure is in
  `c_backend`/`estree`/`ir`/`rust_backend`/`shir` — none fish, none reachable
  from `fish_backend.rs`. Same set as before this session's edits.
* `src/render_ext_expr/handlers/mod.rs` and `src/render_ext_estree/handlers/mod.rs`
  are regenerated (`// @generated by build.rs`) with a shuffled module order on
  every build — revert them rather than committing them.
