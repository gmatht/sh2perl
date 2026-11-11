# shIR formal semantics — the text-ops slice

`TextOps/` is a machine-checked denotational semantics for a first slice of
shIR. It is split into five theories that build as one session:

| Theory | Covers |
|---|---|
| `ShIR_TextOps_Base` | the three EXPRESSION nodes the text reductions emit (`StrLen`, `WordCount`, `RegCount`), their `wc` lowering entry (`try_lower_wc` in `src/transforms/text_ops.rs`), and the plain-ASCII guard that entry uses. |
| `ShIR_TextOps_Strip` | the newline strip of command substitution (`NewlineStrip`), on the reversed byte string. |
| `ShIR_TextOps_SortUniq` | the two ordering nodes `SortLines` / `UniqLines` (`try_lower_sort` / `try_lower_uniq`), modelled over the list of lines. |
| `ShIR_TextOps_Compose` | the composition nodes `Split` / `ArrayLen` / `FieldExtract`, the `ArrayLen(Split(...))` reading of `wc -w`, and `cut`'s delimiter split (which KEEPS empty fields). |
| `ShIR_TextOps_Space` | each engine's measured `\s` class (`measure-space.sh` is the pin), the word count over an ARBITRARY class (`nwords_by`), and where the emitted word split diverges from `wc -w` — with proved witnesses and an ASCII-soundness result per engine. |

```
isabelle build -D sh2perl/docs/fm/TextOps      # HOL only, no AFP, ~30 s
```

Nothing here models shell state, streams or command execution. `wc` is a
function on the byte string it is handed, which is the level at which the
node contract — "this node means this value, in every renderer" — lives.

## What is proved

| Obligation | Lemma | Why it matters |
|---|---|---|
| The lowering agrees with the meaning | `lowered_wc_sound` | For every mode and source, the node `lowered_wc` emits denotes exactly `wc_count`. This is the per-node soundness statement the marketplace contract needs. |
| `wc -l` is the newline count of the text it is GIVEN | `echo_a_b_line_count`, `echo_n_x_line_count` | `echo a b \| wc -l` is 1, `echo -n x \| wc -l` is 0. |
| …and `length + 1` is not it | `strlen_plus_one_is_not_line_count`, `line_count_is_not_length_plus_one` | The pre-2026-10-22 emission, refuted. It shipped 2 for `echo a b \| wc -l` on every backend because the node said "line count" and the emission meant "length plus one". |
| The ASCII guard is neither vacuous nor a loss | `ascii_bytes_is_strlen`, `non_ascii_bytes_refused`, `utf8_e_acute_refused` | `wc -c` counts bytes, `StrLen` renderers count codepoints; the reduction fires exactly where they agree. |
| The word count matches the shell at its edges | `words_*` (8 lemmas) | empty, single word, leading/trailing/doubled whitespace, newline as separator, all-whitespace. |
| The strip keeps no leading newline | `rev_drop_last_nl_keep`, `rev_drop_last_nl_all_nl` | command substitution removes the trailing run; a surviving strip never starts with one, and a strip of nothing but newlines is empty. |
| …and strips the WHOLE run, not one newline | `rev_drop_last_nl_idem` (via `arg_cong_nl`) | chomp-one vs strip-all: a second strip changes nothing. |
| `sort` loses no line | `insert_length`, `isort_length` | the ordering step is a permutation of its input. |
| `uniq -c` counts every line exactly once | `run_count_total` | the printed run lengths sum to the number of input lines. |
| Composition: `ArrayLen(Split(t))` IS the word count | `array_len_split_is_word_count`, `lowered_wc_w_via_split` | The as-built `wc -w` reading. `split_ws` drops the empty fields a plain `split` would produce, `array_len` counts the rest, and `ws_scan` counts word starts — the three agree on every byte string, so obligation 4 above is a corollary rather than an independent model. |
| …and the filtered split is the one that has to be filtered | `unfiltered_split_overcounts_leading`, `unfiltered_split_overcounts_trailing`, `filtered_split_counts_*` | Two negative controls: an unfiltered split of `" a"` returns two fields for one word, and the filtered idiom returns one. The drop-empty step is load-bearing, not cosmetic. |
| A field is never empty and never contains whitespace | `split_ws_fields_clean`, `field_scan_no_empty`, `field_scan_ws_free` | Composition needs the shape, not just the count: `StrLen(FieldExtract(...))` downstream of a `Split` measures a word, not a run of whitespace. |
| `cut -dD -fN` keeps the empty fields | `length_split_on`, `cut_keeps_empty_field`, `cut_empty_field_is_empty`, `cut_field_two` | `a,,b` with `-d, -f2` is the empty string, and it is NOT what the whitespace split would return (`ws_split_would_lose_empty_field`) — the two splits are deliberately different nodes' business. |
| Every engine's class CONTAINS POSIX space, and each divergence is an equation | `posix_subset_all`, `js_extra_locus`, `py_extra_locus`, `perl_extra_locus`, `go_extra_locus`, `perl_bytes_extra_locus` | "Where the emit is wrong" is a proved predicate over every byte, not a paragraph. Every class is a SUPERSET of `is_ws`, so no renderer splits LESS than the shell: divergences are always over-counts. |
| …and the differences are exactly the documented ones | `posix_is_the_ascii_part_of_unicode`, `perl_bytes_is_unicode_minus`, `js_is_unicode_plus_bom_minus_nel`, `py_is_unicode_plus_ascii_separators`, `go_measured_is_unicode`, `perl_unicode_measured_is_unicode` | JS carries U+FEFF and loses U+0085; Python adds 0x1C..0x1F BELOW 128; Perl's own class depends on whether the string is upgraded (a runtime property, not a codegen one) — the byte model loses NBSP and NEL. |
| The emitted expression is NOT the node's value — proved, twice | `js_nbsp_rendering_is_not_the_model`, `python_separator_rendering_is_not_the_model` | NBSP in UTF-8 (0xC2 0xA0) is one word to `wc -w` and two to `\s`-splitting JS; 0x1C is one word to `wc -w` and two to Python's `str.split()`, and it is ASCII. |
| An ASCII source is sufficient for three renderers and INSUFFICIENT for Python | `js_ascii_rendering_is_sound`, `go_ascii_rendering_is_sound`, `perl_unicode_ascii_rendering_is_sound`, `python_has_no_ascii_guard`, `rendering_is_sound_exactly_for_the_posix_classes` | The guard `lowered_wc` already applies to `StrLen` would fix JS/Go/Perl and cannot fix Python: `python_has_no_ascii_guard` refutes the guarded statement with an ASCII string. |

## Deliberate modelling choices

- **Byte strings, not character strings.** `type_synonym bytes = "nat list"`.
  This is what makes the ASCII guard meaningful: in an unencoded model
  `-c` and `-m` are both `length`, so the guard's role lives in the
  renderer's codepoint `StrLen`, and the model states it as the condition
  (`ascii`) under which the two agree.
- **Counts as folds, no token lists.** `nl_count` is a filtered length;
  `word_count` counts word STARTS (`ws_starts`). An earlier draft defined
  `wc -w` as "split into maximal whitespace-free runs and drop the empties";
  the run-list function needs a nested `if`/`let` whose reduction behaviour
  under `simp` is hostile, and none of the obligations need the list — only
  the count. The fold form is the one an implementation would write anyway.
- **No trailing-newline flag in the model.** The lowering is a function of
  `(mode, source text)`. The bug was a second newline invented inside the
  line arm; a shape that cannot be written that way is the shape the model
  rules out.
- **Two splits, on purpose.** `split_ws` (whitespace, drops empties) and
  `split_on` (literal delimiter, keeps them) are separate functions because
  they are separate nodes' business: `wc -w` counts words, `cut -f2` picks a
  column, and `cut` on `a,,b` must yield the empty field. Collapsing them
  into one "split" would make the model disagree with `cut`.
- **The classes are MEASURED, and the measurement is committed.**
  `TextOps/measure-space.sh` prints each engine's whitespace class over
  U+0000..U+10FFFF with the version strings it ran against, so the
  `*_ws_list` tables can be re-derived rather than trusted as a doc quote.
  The lists are defined independently (not as "Unicode plus these"), which
  makes every relation between them a proof (`*_extra_locus`,
  `perl_bytes_is_unicode_minus`) instead of a re-definition.
- **`FieldExtract` is modelled as "the N-th field, or nothing"**
  (`field_or_empty`), 1-based like `cut`, with the past-the-end case
  printing an empty line rather than erroring. The node's `fields` field is
  a `vec_field_range` and its `output_delimiter` is not modelled yet (see
  open targets).

## Open targets (stated, not yet proved)

These are written out where they belong and left as explicit open targets —
never faked with `sorry` (the session is `quick_and_dirty = false`):

- **`rev_drop_last_nl_prefix`** (`ShIR_TextOps_Strip`): the bytes the strip
  keeps are a prefix of the input. Everything reduces to one step —
  `a # ((z # zs) @ w) = z # (zs @ (a # w))` with `a = 10` in scope, i.e.
  associativity of `@` under a hypothesis — which no tactic in this build
  discharges (see the box-health note below).
- **`sorted_insert` / `isort_sorted`** (`ShIR_TextOps_SortUniq`): the model
  sort's output is ordered and stable.
- **`drop_runs_idem` / `drop_runs_no_adjacent_eq`**, and hence
  **`sort -u = uniq . sort`**: the run collapse is idempotent and invents
  nothing.
- **`FieldExtract`'s ranges and `--output-delimiter`**: the model covers
  `-fN` only. `-fN-M`, `-fN,`, `--complement` and `-o` splicing are
  unmodelled, so `fields: vec_field_range` and `output_delimiter` carry no
  obligation yet.

### Closed: `\s` is not POSIX space (2026-11-11)

This was the first open target this list carried. `ShIR_TextOps_Space` closed it the
way the entry said it had to be closed — by putting the engines' class in the
model — and the answer is narrower and less comfortable than the prose was:

- every measured class is a SUPERSET of POSIX space, so the divergence is
  always an over-count, and the extra bytes are listed by `*_extra_locus`
  as equations over all `c`;
- the emit is refuted by two evaluated witnesses (NBSP over UTF-8 for JS,
  0x1C for Python), and the ASCII-soundness result holds for JS, Go and Perl
  (Unicode model) but not Python, where the counterexample is itself ASCII;
- Perl's DEFAULT (byte) `\s` is a different set from its Unicode one, so the
  class is a property of the run-time string, not of the emitted code —
  `perl_bytes_is_unicode_minus`;
- Java, C and Sh render `wc -w` through `shWcWords` / `_sh_wc_words`, not
  through a regex split, so this gap is a property of the Perl/Estree/Go/
  Rust/Python renderers only.

What is deliberately NOT decided: whether the fix is to refuse `-w` on
non-ASCII sources, to narrow each renderer to `[\t\n\v\f\r ]`, or to count
bytes. Each needs the locus lists and none is a proof obligation.

## Box-health note (why the proofs look odd in places)

Narrowing the failure took a while; the reproducible rule is:

**a lemma whose conclusion rewrites a non-variable `a # (x @ y)`, while a
hypothesis `a = 10` is in scope, does not close** — `simp`, `rule refl`,
`rule append_Cons` and `metis` all fail. The same lemma without the
hypothesis closes immediately, and with the hypothesis it does not even
typecheck as a standalone statement (`Clash of types "nat" and "_ list"`).
`simp+` on one such step ran for 233 s before being killed. This is the
one step left in `rev_drop_last_nl_prefix`.

What works, and is used throughout:

- state the helper as an equation in the induction variables and close it
  by an explicit `cases` on the list, never by `simp` on a cons/append shape;
- prove the congruence you need once (see `arg_cong_nl`) rather than
  rewriting under the function inside each induction step;
- fetch an induction hypothesis with `by assumption` (or `Cons(1)` plus
  `..`) — `using Cons(1) by simp` frequently fails here;
- never state a CONJUNCTION over a recursive function's output and induct on
  both halves at once: `∀w ∈ set (field_scan …). w ≠ [] ∧ ws-free w` ran
  >240 s without finishing (with `simp_all`, `auto` and the `fun` induction
  rule alike), while the two halves as separate inductions close in ~6 s
  (`field_scan_no_empty` + `field_scan_ws_free`, recombined in
  `field_scan_fields_clean`). Carry a named predicate (`ws_free`) instead of
  an inline `∀`, and pass the *unfolded* hypothesis to `simp`, not both the
  hypothesis and its definition at once — `simp add: h ws_free_def` rewrites
  the goal past the point where `h` can match, which reads as a 60 s hang.

One parser trap, not a prover one: do not name a parameter `done`.
`proof (induction cs arbitrary: cur done)` is an *outer syntax* error
("keyword `)` expected, but end-of-input") because `done` terminates the
method argument; the failure points at the following `case`, which is
misleading. The accumulator here is called `acc`.

`rev_drop_last_nl_idem` was blocked for hours until those three rules were
applied; it now goes through with no `simp` on a cons/append shape at all.
This is a tooling defect, not a modelling one: worth reporting upstream
with the `a = 10` repro, and worth remembering before blaming a definition.

## Where this should go next (not done here)

The natural next slices, in order of payoff per line of proof:

1. ~~**`Split`/`ArrayLen`/`FieldExtract`**~~ — done in `ShIR_TextOps_Compose`
   (2026-11-11). `wc -w`'s as-built lowering is `ArrayLen(Split(...))`, and
   `array_len_split_is_word_count` makes it a corollary of the word-count
   obligation. What is left of this slice is the `-fN-M` range and the
   output-delimiter splicing noted under open targets.
2. **`StringTrim`/`StringAffix`/`AffixStrip`** — pure string algebra, no
   state; the same shape as this file, and the reductions that fire least
   (several are declared-but-unemitted, where a semantics would say whether
   they are even worth emitting).
3. **`CaseTransform`/`CharTranslate`** — total functions on bytes given a
   mapping table; the interesting part is the `tr` SET semantics, which is
   the one place a shell's own definition (ranges, `[a-z]` vs locale) has to
   be written down before it can be implemented consistently.
4. **`IrStmt` control flow** — `Block`/`If`/`While`/`Assign` with a variable
   store. This is where a formal reading stops being a per-node lemma and
   starts being a semantics: `Subshell` copy semantics, `Pipeline` capture
   (the transform that must NOT fire inside `$( )`), and the `x=$(...)`
   strip-trailing-newlines rule are all stateful and currently specified
   only by tests. It needs an effect model (store + stdout + exit code) —
   weeks, not hours, and only worth starting once the leaf nodes above are
   settled.

The transform marketplace (`PLAN.md` §11) is the consumer: every
analysis-only transform already argues its soundness in prose, and
`core-requests/transforms/done/div-mod-pow2.rs/proof/` is the existing
precedent for carrying an Isabelle session next to a request. This directory
generalises that from "one arithmetic invariant" to "the meaning of a node",
which is the thing the node contract actually promises.
