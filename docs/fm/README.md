# shIR formal semantics — the text-ops slice

`TextOps/` is a machine-checked denotational semantics for a first slice of
shIR. It is split into three theories that build as one session:

| Theory | Covers |
|---|---|
| `ShIR_TextOps_Base` | the three EXPRESSION nodes the text reductions emit (`StrLen`, `WordCount`, `RegCount`), their `wc` lowering entry (`try_lower_wc` in `src/transforms/text_ops.rs`), and the plain-ASCII guard that entry uses. |
| `ShIR_TextOps_Strip` | the newline strip of command substitution (`NewlineStrip`), on the reversed byte string. |
| `ShIR_TextOps_SortUniq` | the two ordering nodes `SortLines` / `UniqLines` (`try_lower_sort` / `try_lower_uniq`), modelled over the list of lines. |

```
isabelle build -D sh2perl/docs/fm/TextOps      # HOL only, no AFP, ~10 s
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
| …and strips the WHOLE run, not one newline | `rev_drop_last_nl_idem` | chomp-one vs strip-all: a second strip changes nothing. |
| `sort` loses no line | `insert_length`, `isort_length` | the ordering step is a permutation of its input. |
| `uniq -c` counts every line exactly once | `run_count_total` | the printed run lengths sum to the number of input lines. |

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

## Open targets (stated, not yet proved)

These are written out where they belong and left as explicit open targets —
never faked with `sorry` (the session is `quick_and_dirty = false`):

- **`rev_drop_last_nl_prefix`** (`ShIR_TextOps_Strip`): the bytes the strip
  keeps are a prefix of the input. The induction step needs "a newline at
  the head of a non-empty strip survives", which no tactic tried discharges
  in this build (see the box-health note below).
- **`sorted_insert` / `isort_sorted`** (`ShIR_TextOps_SortUniq`): the model
  sort's output is ordered and stable.
- **`drop_runs_idem` / `drop_runs_no_adjacent_eq`**, and hence
  **`sort -u = uniq . sort`**: the run collapse is idempotent and invents
  nothing.

## Box-health note (why the proofs look odd in places)

On this box a fresh `HOL` session cannot discharge
`(xs :: nat list) @ [] = xs` with `by simp`, while the same lemma closes
inside `TextOps`. `rule refl`, `rule append_Cons` and `simp` also fail on
`((a # x) @ y) = a # (x @ y)` from a *non-variable* left-hand side. The
cause is undiagnosed; the workaround used throughout is to state the
helper as an equation in the induction variables and close it with an
explicit `cases`, which is why some proofs below are longer than they
"should" be. Each such spot is commented in the theory.

## Where this should go next (not done here)

The natural next slices, in order of payoff per line of proof:

1. **`Split`/`ArrayLen`/`FieldExtract`** — the composition nodes. `wc -w`'s
   as-built lowering is `ArrayLen(Split(...))`, so a semantics for `Split`
   plus the `ArrayLen` law turns obligation 4 above into a corollary.
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
