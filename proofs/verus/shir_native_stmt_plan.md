# `transforms/shir_native_stmt.rs` — decomposition plan (why this one is not a law)

    source: src/transforms/shir_native_stmt.rs   7140 lines
    proof status: cluster C PROVED (shir_native_herestring.rs, 20 verified / 0 errors);
                  cluster H PROVED (shir_native_quoting.rs, 19 verified / 0 errors);
                  cluster E PROVED (shir_native_echo.rs, 26 verified / 0 errors);
                  cluster B PROVED (shir_native_redirect.rs, 21 verified / 0 errors);
                  cluster D1 PROVED (shir_native_test_render.rs, 15 verified / 0 errors);
                  cluster D2 PROVED (shir_native_chain.rs, 18 verified / 0 errors);
                  cluster D3 PROVED (shir_native_cond.rs 10 verified / 0 errors;
                  native_while_multi_cond in shir_native_while_multi.rs, 9 verified / 0 errors);
                  cluster F1 PROVED for tr/wc -l/head -c/tail -c, plain sort,
                  the general line-mode head -N/tail -N window law on the
                  nl_free/nonempty domain, and the off-domain divergences
                  generally in N over that witness family (shir_native_f1.rs
                  74 verified / 0 errors): head past the last line and tail -N
                  on an unterminated producer at the output level, tail -N on a
                  newline-terminated producer as the general selection identity
                  (its output relation is pinned at N = 1);
                  still open here: the byte renderer, the flat-text connector
                  split, F2's matcher, F2-F4/G/A; D1-D3 are reuse only;
                  cluster F2 PROVED (grep_literal_fold.rs, 37 verified / 0 errors);
                  cluster G part 1 PROVED (shir_native_constant_pipeline.rs,
                  16 verified / 0 errors: the constant true/false pipeline fold,
                  its status law, its six refusals and d1 pipefail);
                  cluster G part 2 PROVED (shir_native_command_v.rs, 19 verified /
                  0 errors: native_command_v and command_v_redirect_truth against
                  PATH resolution, the /dev/null sink law, d1 bare-form loss and
                  d2 absent-listed-name);
                  cluster G part 3 PROVED (shir_native_command_noop_cond.rs,
                  25 verified / 0 errors: the `! command <'' >''` condition
                  fold, its redirect-failure law, six conservative refusals and
                  d3 -- the fold never inspects the redirect target);
                  clusters A, F3, F4 and the REST of G still planned — see §4

This is a PLAN, not a proof. It exists because `shir_native_stmt` is the one
output-changing transform left whose property is not a single statement: it is
a union of sixty independent rewrites that happen to share a file, a dispatch
function and a `transform_stmt` recursion. The last five proofs
(`status_reads`, `status_reads_traversal`, `uae_terminators`,
`affix_strip_fold`, `text_capture_fold`, `bc_float_clean`) each had ONE
semantic side; here there is no such thing to write down.

## 0. Measured inventory (the numbers this plan is sized from)

Counted this turn, not estimated:

| quantity | value |
|---|---|
| file length | 7140 lines |
| top-level `fn`s | 105 |
| fold entry points called from `transform_stmt` | 60 |
| `#[cfg(test)]` module starts at | line 6957 |
| tests in that module | 8 |
| uses of the `files: HashMap<String, String>` state | 62 |
| dispatch shape | `transform_stmt` (215 lines), 12 `IrStmt` arms |

Eight tests cover sixty rewrites: **52 of the 60 folds have no source test at
all**, so their pins are model-level and the manifests will be mostly
UNMIRRORED — that is a property of the file, and each cluster's manifest must
say so rather than manufacture PASS rows.

The twelve `IrStmt` arms are not twelve independent properties either,
measured this turn: the `Expr` arm makes 54 fold calls (44 distinct, of
which 10 are the pipeline-expression sub-branch), the `Redirect` arm 19
calls (18 distinct), the `Pipeline`-statement arm 3 calls (2 distinct), and
`If` and `While` 2 each before they recurse. A forgotten fold in this
dispatch is invisible to any law about an individual fold.

Two flags/state make it worse, and both need their own treatment:

1. `lower_printf: bool` — the same statement shape is folded differently
   depending on a caller-provided flag (`native_printf_stmt` and
   `native_printf_v_stmt` are gated on it). Any law must be stated per flag
   value, not "for this transform".
2. `files: HashMap<String, String>` — `record_file_write` records the literal
   content of a file the program has written earlier, and later folds
   (`native_cat_file_pipeline`, `slurp_op`) substitute that content. This is
   the only CROSS-STATEMENT semantic dependency in the file, and it is
   currently unstated anywhere.

## 1. Why one law cannot exist here

The five laws already in `proofs/verus/` all fit one shape:

> an analysis function A(s) decides a syntactic question; an independent
> semantic function S(s) decides the corresponding semantic question;
> A(s) = Some ⇒ S(s) holds (plus pins for the load-bearing refusals).

For this file A would have to be `transform_stmt`, whose "syntactic question"
is sixty different questions in sequence, and S would have to be a relation
between an `IrStmt` and bash's actual stdout/status/filesystem effects. The
⇒ direction then requires a semantics for the entire whitelist command set
(hundreds of lines of bash behaviour: `grep`'s BRE, `sort`'s collation,
`find`'s traversal, `printf`'s format language, `tr`'s sets, `ls`'s
formatting). That is a shell semantics project, not a proof: it would swamp
the rest of the proof surface and, worse, it would be a semantics nobody
could falsify — the file's own tests already show where the divergences live
(mtime comparisons, `find | head`, unary `-f`).

So the plan is the opposite: **decompose until each piece is as small as one
of the five laws already landed**, and prove two cross-cutting facts (the
`files` invariant, the dispatch skeleton) once.

## 2. The claim shape every cluster must produce

Each cluster is a normal stage in the established pattern:

- one `proofs/verus/<cluster>.rs` with a **semantic side independent of the
  pass** (a text/status/filesystem model written from the command's
  documented behaviour, never by transcribing the fold);
- the ⇒ law for each fold in the cluster (whatever the fold emits, the
  command really does);
- one pin fn per **load-bearing refusal** — the refusal that, if dropped,
  would make the fold wrong (not merely conservative);
- a MEASURED mutant table (each mutant restored byte-identically, sha256
  recorded), split into counted (verification re-runs with N−1 obligations)
  and simplification-rejected (aborts at a `by (compute)` pin);
- `<cluster>_sync.txt` in the existing format, with `[exec]` rows only where
  a source test really ran;
- a README section stating what is proved and what is explicitly out of
  model.

A cluster that cannot produce an independent semantic side is NOT a proof
candidate — it is a "structural" fold, and it goes in cluster A as a stated
refusal instead.

## 3. The clusters

Sizes are the fold functions' own line counts; obligation counts are
estimates, to be replaced by measurements on first compile.

### A — dispatch skeleton and the `files` state (structural, ~2 obligations)

Functions: `transform` (61), `transform_with_printf` (66), `transform_stmt`
(91, 215 lines), `transform_expr` (306), `record_file_write` (2516),
`native_cat_file_pipeline` (2549), `slurp_op` (2615).

Two claims, both real but modest:

1. `files_monotone`: a fold never makes `files` claim LESS than the source
   really wrote — every `record_file_write` inserts the exact content the
   redirect folded to, and no fold deletes an entry. Combined with
   `files_sound` (every entry equals what bash would have in the file at that
   point), this licenses the later substitutions. Model: a `Map<path, text>`
   with a write-only step; the semantic side is "the sequence of writes bash
   performs". Mutants: record the PRE-fold content; record on a refused
   redirect; let a later fold ignore the map (that one is caught by the
   cluster-B laws, not here).
2. `dispatch_covers_every_fold`: every function listed in §0's dispatch table
   is reachable from `transform`, and no fold is dead code. This is a
   grep-level check today; it becomes a proof only if the dispatch is
   expressed as a list in the model. Cheap, worth doing once, because the
   file grows by appending to those lists.

Explicitly NOT proved here: bottom-up order, the "do not rewrite pipeline
stages" rule, and the re-entrant `transform_stmt` fixpoint. Those are
statements about what the recursion is allowed to see, and they are better
enforced by the dispatch-list check than by a law.

### B — redirect writes (family 1 of the file's own header; PROVED)

Functions: `file_redirect_block` (3284), `native_echo_write_file_stmt`
(3705) with its `emit_file_op` helper (4033), `native_sort_file_stmt`
(3330), `native_cat_heredoc_writefile` (1062),
`native_cat_heredoc_redirect_call` (2168), `record_file_write` (2516).

Semantic side: for a literal-target redirect of an `echo`/`printf` exec,
the bytes bash leaves in the file = `expand(words) ++ newline?`, opened in
mode `w`/`a`. The fold's emitted `WriteFile`/`Output` carries the same bytes.

Law: `redirect_writes_the_same_bytes` + `append_mode_appends`.

Load-bearing refusals (pins): dynamic target (`> "$f"`, `> /tmp/$$.out`) —
the fold must not fire; `fd != 1`; a second redirect spec; a mode outside
`w|wc|a`; a non-`echo`/`printf` command (`file_redirect_block`'s
`matches!(cmd, "echo" | "printf")`); `echo -n` (no newline — the fold's
newline decision); `echo -e` escapes; a glob word.

Mutants: wrap a dynamic target too; ignore `fd`; treat `a` as `w`; emit the
newline for `-n`; accept any command.

Note: `file_redirect_block`'s rewrite is a `Block` wrap with NO content
change — its law is "the wrapped and unwrapped forms have the same stdout,
status and filesystem effect", which is really a renderer contract (the Perl
backend takes the native fallback only for the wrapped form). That half is
out of model; state it.

### C — empty herestrings collapse to a status (smallest real law; PROVED)

Functions: `empty_herestring_to_status` (6876), `status_exec` (84),
`exec_parts` (369).

Semantic side: `stdout(cmd, stdin = "")` and `status(cmd, stdin = "")` for
`cat`, `tr SET1 [SET2]`, `grep PAT` — three commands, each a couple of lines
of real semantics (`cat` copies stdin → empty stdout, status 0; `tr` with at
least one set → empty stdout, status 0; `grep PAT` with a non-empty pattern
over empty input → empty stdout, status 1).

Law: `empty_herestring_emits_the_true_status` — for each of the three
commands, the emitted `status_exec(ok)` has `ok` = the status bash would
return, and stdout is empty on both sides.

Load-bearing refusals (pins): bare `tr` (no sets) exits 1, so it must NOT
collapse to `true`; `grep -c` would print `0`, so it must be refused;
`grep` with two plain words (a file operand — no such file, different
status); an empty pattern; an interpolated pattern (may expand to many
words); `cat FILE` (a file operand changes both streams); a non-empty
herestring.

Mutants: `tr` bare → true; drop the `-c` check; `grep` with a file operand →
false; `cat` with a file operand → true; accept an interpolated pattern.

Estimated 18–24 obligations. This is the recommended next stage: it is the
whole file's shape in miniature (three commands, five refusals, a status
model we already have from `uae_terminators`) and it reuses `run_abort`.

### D — `test` chains → `If`

Functions: `test_and_to_if` (448), `single_stmt` (510), `test_chain_to_if`
(521), `chain_stmt_expr` (591), `redirect_call_to_stmt` (626),
`cmd_and_chain_to_if` (679), `flatten_redirect_status_block` (769),
`single_always_true_exec` (806), `is_always_true_exec` (1723),
`native_test_chain` (1170), `native_echo_or_chain` (4293),
`native_echo_grep_test_chain` (4899), `native_if_test_cond` (5833),
`native_while_test_cond` (5892), `native_while_multi_cond` (5957),
`native_test_exit_stmt` (5900), `extract_test_words` (5809),
`render_test_words` (1301), `render_test_text` (1435),
`render_test_tokens` (1502), `render_flag_substitution_test` (1473).

Semantic side: a `test`-expression evaluator (`Test` grammar: `-e`, `-f`,
`-d`, `-z`, `-n`, `-r`, `-w`, `-x`, string `= !=`, integer `-eq -ne -lt …`,
`!`, `-a`, `-o`, parentheses) over a `Tree` of existing paths and a
`Map<name, text>` of variables. The fold's emitted `If` condition must
evaluate to the same boolean.

This is the largest cluster and the one with the most interesting law:
`render_test_words` is a RECOGNISER-AND-SERIALISER, so the law is the
round-trip `eval(parse(render(t))) = eval(t)` for every word vector the fold
accepts — the same shape as `text_capture_fold`'s token-stream law, with
`test`'s grammar in place of the shell lexer.

Load-bearing refusals (pins): a fallible then-arm (`cat`, `grep`, …) must not
become an if-branch (`test_and_to_if`'s "always-success" gate); `cd` chains
(the source's own test says they keep their dedicated renderers); a negated
`!` chain; `-a`/`-o` inside `[[ ]]`; glob words; an unset variable under
`set -u`; a dynamic word that could expand to several words.

Mutants: drop the always-success gate (the `cat` case goes wrong); treat `!`
as negation of the wrong level; let `-o` bind tighter than `-a`; accept a
dynamic word; render `-f` as a flag regex instead of a file test (the
source's own test `unary_file_test_renders_a_filetest_not_the_flag_regex`
pins this one).

MAPPED pins available: `command_chain_lowers_to_status_gated_ifs`,
`longer_command_chains_nest`, `non_statement_elements_refuse_the_chain_rewrite`,
`cd_chains_keep_their_dedicated_renderers`,
`unary_file_test_renders_a_filetest_not_the_flag_regex`,
`negated_and_unhandled_shapes_reach_the_shared_parser`.

Split it further if it grows: D1 `render_test_*` (the round trip), D2 the
chain folds (`test_and_to_if` / `test_chain_to_if` / `cmd_and_chain_to_if`),
D3 the if/while condition families.

### E — `echo`/`printf` byte equivalence (PROVED)

Functions: `native_echo_stmt` (2984), `decode_echo_escapes` (3110),
`native_printf_stmt` (3173), `native_printf_v_stmt` (3138),
`is_quoted_echo_expr` (3231), `literal_text` (3256), `output_stmt` (2045),
`native_printf*`'s format handling.

Semantic side: `echo [-n] [-e] [-E] WORDS` output bytes (`decode_echo_escapes`
written independently: `\n \t \\ \0NNN \c` …), and `printf FMT ARGS` output
bytes (a small format-language model: `%s %d %%`, `\n`, repeated format).

Law: `echo_emits_the_same_bytes`, `printf_emits_the_same_bytes`.

Refusals: `-n` vs `-e` vs `-E` vs an unknown flag (refused); a glob word
(must stay dynamic); a word with a `\` continuation; `printf` with more args
than conversions; `printf -v` (writes to a variable — status only).

Mutants: keep the newline for `-n`; decode escapes without `-e`;
accept an unknown flag as no-op; treat a glob word as literal; drop
`printf`'s format repetition.

### F — filter pipelines (split by command; four stages, not one)

This is family "the pipeline prints a literal", the group where a text model
does the real work. Order them by how simple the command's semantics is:

**F1 `tr` / `sort` / `wc -l` / `head` / `tail`** — `native_echo_tr_sort_stmt`
(5238), `native_literal_subshell_wc_stmt` (5420), `native_head_tail_stmt`
(1735), `native_head_tail_pipeline` (2355), `native_wc_l_pipeline` (2459),
`tr_set_escape` (5220), `sort` flags. Semantic side: character-set
translation, line sorting (byte order only — locales out of model), counting
lines, taking a prefix/suffix. **Best value per line of proof in the whole
file.**

**F2 `grep`** — `native_echo_grep_stmt` (3417, the largest fold at 288
lines), `native_echo_filter_stmt` (4617), `native_grep_file_pipeline` (3812),
`native_static_grep_redirect` (2764), `grep_output` (2802),
`native_grep_devnull_or` (1151), `native_grep_herestring` (2206),
`native_grep_o_herestring` (2308), `word_boundary_hit` (2881). Semantic side
needs a BRE subset (literal, `^`, `$`, `.`, `*`, `[...]`, `\<`/`\>`) —
declare the subset explicitly and refuse the rest in the model the way the
fold refuses in the source. The model's regex must be written as a matcher,
not as a transcription of `grep_output`.

**F3 `find` / `ls`** — `native_find_stmt` (6111), `native_ls_stmt` (4449),
`native_ls_dir_pipeline_stmt` (6760), `human_size` (2153). Semantic side: a
FIXED tree (`Vec<(path, size, mtime)>`) plus the documented traversal order.
mtime and permission predicates go out of model (the file's own test says
mtime comparison must not degrade to the flag heuristic).

**F4 `xargs` / `cat` pipelines / `comm` / `diff`** — `native_echo_xargs_stmt`
(5115), `native_cat_file_pipeline` (2549), `native_cat_herestring` (2488),
`native_cat_var_or_stmt` (6459), `native_comm_stmt` (6581),
`native_diff_files_stmt` (5652), `native_diff_procsub` (2251),
`native_echo_bc_stmt` (1100). Semantic side: word-appending (`xargs` default
command `echo`), concatenation, line-set intersection/difference/union under
`-1/-2/-3`, and diff output — the last is a real cost; consider proving only
"refuses everything but byte-identical inputs" and stating the rest.

### G — status-only and identity folds

Functions: `native_literal_let_stmt` (817), `native_declare_typeset_stmt`
(922), `native_constant_pipeline` (1036), `normalize_literal_exec_words`
(2921), `native_eval_assign_stmt` (1219), `eval_assign_parts` (1257),
`native_command_v` (2077), `command_v_redirect_truth` (2114),
`native_subshell_redirect_stmt` (6367), `native_subshell_eval_noop_stmt`
(6719), `native_command_noop_cond` (6549), `native_trap_stmt` (5060),
`native_rm_rf_stmt` (5705), `native_bash_script` (2008),
`native_perl_e_stmt` (1898), `native_perl_ne_stmt` (1933),
`perl_print_prefix` (1979), `native_env_grep_or_stmt` (6621),
`empty_herestring_to_status` (already cluster C), `native_sort_stmt` (1828).

These need only a STATUS model plus a variable/assignment model — reuse
`uae_terminators`' `run_abort` and `status_reads`' `$?` facts. Several are
"this rewrite changes nothing observable" (`normalize_literal_exec_words`,
`native_subshell_eval_noop_stmt`), which is a cheap law with real value: it
tells future readers the fold is safe to keep expanding.

Sub-cluster G*: the two `perl` folds and `native_bash_script` emit Perl
source text; their law is a QUOTING law (`perl_quote` produces a literal that
Perl re-parses to the same string), the same shape as cluster H.

### H — literal quoting and escaping (the cross-cutting text law; PROVED)

Functions: `perl_quote` (2054), `render_test_text` (1435),
`render_flag_substitution_test` (1473), `tr_set_escape` (5220),
`regex_escape_literal` (6858), `decode_capture_escapes` (403),
`decode_echo_escapes` (3110), `static_pipeline_truth` (413).

Law: for every string the escape functions accept, `unquote(quote(s)) = s` in
the TARGET language (Perl double quotes, `test` word syntax, `tr` set
syntax, BRE). This is one law per target syntax, each small, and it is
exactly the law `text_capture_fold` proved for shell text — reuse its
`Ch`/`Syn` token-stream idea rather than inventing a new one.

Do H before E/F: every emitted literal in E, F1 and G* depends on it, and a
bug in `perl_quote` would otherwise show up as an unexplained F failure.

## 4. Order and budget

Recommended order, smallest semantic surface first:

1. **C** — empty herestring → status (18–24 obligations, reuses `run_abort`).
2. **H** — quoting laws (one per target syntax; ~15 each).
3. **E** — echo/printf bytes (~30–40).
4. **B** — redirect writes + `files` soundness (§A.1) (~30).
5. **D1 → D2 → D3** — test rendering, then chains, then conditions
   (~40–60 total; split if a stage exceeds ~40).
6. **F1** — tr/sort/wc/head/tail (~30).
7. **F2** — grep BRE subset (~40+, the model is the cost).
8. **G** — status/identity folds (~25, mostly mechanical).
9. **F3, F4** — find/ls, then xargs/comm/diff (~30 each; F4's diff may stop
   at "refuses everything but identical inputs").
10. **A** — dispatch coverage + `files` invariant as the closing structural
    stage.

That is roughly ten stages. Each ends with the same acceptance checks:
`verus --triggers-mode silent --rlimit 40` → 0 errors; measured mutants with
byte-identical restore; `<cluster>_sync.txt`; a README section; one commit
touching only `proofs/verus/`.

Per-cluster expected mutant count 4–6. Where the source has no test for a
fold (52 of 60), the manifest says UNMIRRORED and there is no `[exec]` row —
do not invent one.

## 5. Out of model, for every cluster

State these once, in the README section that introduces the whole
decomposition, and repeat the relevant ones per cluster:

- the `IrStmt`/`IrExpr` CONSTRUCTION by the frontends (the model starts from
  an already-built node, as in every other proof here);
- the RENDERERS: whether Perl/ESTree actually emit the native form for the
  emitted node. `file_redirect_block`'s whole purpose is a renderer
  fallback trigger; its law can only be "same observable output", not "the
  Perl backend takes this branch";
- the `system('bash', …)` call-site metric (`./fail-shir`), which is the
  reason the transform exists but not a semantic property;
- the whitelist itself (`harness/shir-whitelist.txt`): coverage of the
  whitelist is a policy, not a theorem;
- `lower_printf`'s value: laws are stated per value;
- time, locales, permissions and the filesystem beyond the fixed tree model;
- bash's own error messages on stderr (the transform is about stdout, status
  and files, and no cluster models stderr text).

## 6. What would falsify this plan

The plan is wrong if a cluster turns out to need no independent semantic
side (then fold it into cluster A as a stated structural claim) or if two
clusters share enough model that they should be one file (then merge before
committing, not after). The first stage to run — cluster C — is deliberately
the one that tests the shape: if C cannot produce a semantic side for three
commands and five refusals, the rest of this plan needs rewriting.
