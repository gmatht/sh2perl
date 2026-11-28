# Verus proof of the interval estimator (`i32_provable`)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 interval.rs
    -> verification results:: 5 verified, 0 errors

## Path notes — files cited from this directory that are not here

This repo is standalone (AGENTS.md), so every citation under `proofs/verus/`
either names a file that exists in this tree or names material maintained
OUTSIDE it, which no path here can resolve. The 2026-11 sweep replaced the
bare filenames with descriptions, so no reader is sent to a missing path:

- **the proof-campaign plan** — the planning document behind this proof
  directory. Older headers and manifests cite it as `§(b).3`, `§(b).4`,
  `§(d)` or `P12`; those are historical labels for its findings, not
  headings of this README.
- **the C UB proof notes** — the C backend's UB survey (`§1`, `§2a`, `§2b`,
  `§24`, `§33` in the `c_*.rs` headers).
- **the backend bug log** — the `#7` / `#11a` entries the loop and
  `direct_calls` proofs refer to.
- **the workspace harness probes** — the bash-behaviour probes, cited as
  `<workspace>/harness/*.sh`; `<workspace>` is a placeholder, not a path in
  this repo.
- **the retired coverage script** — formerly used as a key prefix in the
  README's "Coverage sync registered" lines; only the key survives.
- **the core-requests item** `c-20261104-unquoted-empty-word`.

Why keep the names: they are provenance for measurements actually made, and
deleting them would hide where the evidence came from. Why not link them:
this repo's CI must stay self-contained.

## Manifest checker — `check_sync.py`

    python3 proofs/verus/check_sync.py            # everything, and runs the tests
    python3 proofs/verus/check_sync.py --no-tests # shas + pins + parity only
    python3 proofs/verus/check_sync.py -m status_reads -v
    python3 proofs/verus/check_sync.py --list
    python3 proofs/verus/check_sync_selftest.py  # the checker's own self-test

CI actually runs Verus (`.github/workflows/verus.yml`): the pinned release
`0.2026.10.18.7d82994` on rustup toolchain `1.99.1`, cached between runs, is
invoked by `proofs/verus/ci_verify.sh` on every `*.rs` a pull request touches
plus a fixed subset (`status_reads`, `status_reads_traversal`,
`uae_terminators`, `affix_strip_fold`, `text_capture_fold`, `bc_float_clean`,
`shir_native_herestring`). Any summary other than `N verified, 0 errors`
fails the job. The manifests' sha pins are checked separately by
`check_sync.py` in `test.yml`; the two are complementary.

### Declared mutants — `mutate.py`

    python3 proofs/verus/mutate.py              # re-run and assert every mutant
    python3 proofs/verus/mutate.py --list
    python3 proofs/verus/mutate.py --only bc_float_clean
    python3 proofs/verus/mutate.py --calibrate  # measure only, never fails
    python3 proofs/verus/mutate.py --self-check # the gate rejects a wrong row

A header's "measured mutants" table is now executable. Each row declares one
`find`/`replace` pair, the proof's pinned sha256, and the verdict the header
claims: `N-1 verified, M errors` with the failing obligations named, or a
refusal at a named `by (compute)` pin. The tool checks the sha first (a row
can never be applied to a revision the claim was not made on), applies exactly
one occurrence, runs Verus, checks the summary line and the functions the
errors point into, then restores the original bytes and re-hashes them — a
non-identical restore is a hard failure whatever the verdict was. The table
covers the six stages landed 2026-11-24..27 (38 mutants, all reproducing);
`strip` is not in it yet. Two rows record where measurement diverges from the
header's label: `uae/m3` fails at `terminates_stops`, not the `trunc_sound`
the header also names (the 29v/2e count matches), and `text/s2`'s first false
concrete pin is `pin_analysis_shapes`, not `pin_emit_is_richer`.

A `*_sync.txt` manifest is a claim that this proof models THAT source, that its
pins exist, and that these rows were executed. The checker fails the claim
when it rots:

| failure | what it means |
|---|---|
| `stale-source` | a recorded `# source sha:` is not the leading 12 hex of that file's sha256 (or its full digest). One rule only: no alternate digest, no shortened stub, no annotation. The fix is to re-audit the proof against the new source and record the audit, not to bump the sha; a `STALE (…)` annotation does not soften the failure. |
| `sha` | the recorded `proof sha:` no longer matches the `.rs`, or the line is missing. |
| `phantom-pin` | a `pin <name>` row names no `fn <name>(` in the sibling `.rs`. |
| `unrun` | a `pin … : MAPPED` row has no `exec …: PASS` row, or its named test did not run. |
| `bogus-exec` | an `exec …: PASS` row has no `MAPPED` pin row behind it. |
| `parity` | `exec <pin>: UNMIRRORED` contradicts `pin <pin>: MAPPED`. |
| `counts` | `[counts]` claims errors, or its `proof-fns:`/`pins:` numbers disagree with the file. |

The tests are the point of `exec …: PASS`, so the checker runs `cargo test
--lib` once and looks up every named test's own verdict: a `PASS` row whose
test does not run, or runs `FAILED` (or `ignored`), fails the manifest.
`--no-tests` skips that and says so in the summary — a run without the tests
is not evidence of execution.

Two dialects are accepted, because the directory has two generations:
`[counts]` as `N verified, M errors` (with `M == 0`), or as the older
file-shape fact `proof-fns: N` / `pins: M`, whose numbers are checked against
the `.rs` (`proof fn` declarations) and the `[pins]` table.

**No stale rows.** There is no admitted-staleness escape hatch: a source row
whose sha does not match the file fails the manifest, annotated (`…=<sha>
STALE (reason)`) or not, because a manifest that no longer matches its source
must never read as evidence. An audit that finds the drift harmless re-pins
the row to the current file and writes the audit into the manifest, the way
`counted_arith_forinit`, `test_lowering`, `capture_folds`,
`counted_while_forinit`, `copy_propagation`, `textop_folds`, `list_folds` and
`c_decl_coverage` record theirs. `optimize_fold_sync.txt` is a generated fold
table, not a manifest, and is reported as `SKIP`.

`check_sync_selftest.py` proves the checker itself: it mutates one manifest
per failure class — phantom pin, bare stale sha, a stale sha carrying a
`STALE (…)` note, a wrong `proof-fns:`, a `MAPPED` row with no `exec`, a test
name that does not exist, and a test that exists but `FAILED` — asserts each
produces exit 1 with that kind, and restores every file byte-identically
(checked by sha256).

Exit status is 1 if anything failed.

## What is proved

`interval.rs` is a standalone model of the interval rewrite of
`sh2perl/src/transforms/i32_provable.rs` (`arith_bounds`): the `ArithAst`
constructors the analysis touches, the exact operator rules
(`+`/`-`/`*`/unary `-`/ternary hull), and

    proof fn bounds_sound(a: ArithAst)
      ensures  forall|lo, hi| bounds(a) == Some((lo, hi)) ==>
                 lo <= ev(a) && ev(a) <= hi && in_i32(lo, hi)

i.e. a marked node's value lies in its computed interval, and the interval
is inside i32's `[-2^31, 2^31-1]`. That is the obligation the renderers
need, and it is the one the magnitude bound (`arith_abs_bound`) could not
state — see `proofs/I32Provable/I32Provable.thy` §6.

## Why Verus rather than Isabelle here

The Isabelle version of this lemma stalled on the product corner bound
(`min/max` of four products) because the automation has to be told how to
unfold `min`/`max`. Verus's `nonlinear_arith` discharges it in one step
once the four-corner disjunction is supplied — 5 obligations, 0 errors.

## It can fail (checked)

Replacing the product's low corner with `c1` (`lo = c1`) makes the same
command report **4 verified, 1 error** — the obligation is not vacuous. The
file was restored and re-checked at **5 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `interval.rs` duplicates `arith_bounds`
  by hand. Nothing here ties the model to the Rust source, so the two can
  drift — exactly the failure this exercise exists to catch. Tying them
  means putting `verus!`-annotated specs ON the real function, which
  needs the crate to build under Verus (see below), not a copy.
- It says nothing about the emitted C: "the value is in i32" is a
  mathematical statement; that the target does not wrap on the way there
  is the separate obligation in the C UB proof notes.
- `i32_provable` has no renderer consumer in the tree today (checked:
  `is_i32()` is referenced only from `transforms.rs`), so this verdict
  currently changes no emitted byte.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## `echo_fold_output.rs` — a constant `echo` prints what the `Output` node prints

`echo_fold::fold_to_output` replaces `echo WORD…` with `IrStmt::Output` when
every word is a compile-time constant: it scans the leading `-n`/`-e`/`-E`/`--`
flags, joins the remaining words with one space, decodes the `-e` escapes, and
carries the trailing newline in the node's `newline` flag rather than in the
text.

The file models the two ends and proves they print the same bytes, for every
all-literal word list and every environment:

```
out_bytes(fold(ws), env) == out_bytes(S::Echo(ws), env)
```

`out_bytes` is the lowering law: an `Output(text, newline)` prints `text` plus
`\n` when the flag is set, and a surviving `Echo` prints the bash model
(`echo_bytes`). The word list is `W::Lit(Seq<int>) | W::Dyn(nat)`, so a runtime
word (an env read) is a distinct shape, not an empty string.

The model's content, each pinned by a witness and measured red as a mutant
(counts are for the current, bash-faithful file):

- **the newline is not in the text** — `echo -n hi` prints `hi`, and
  `echo hi` prints `hi\n` because the FLAG appends it. Baking `\n` into the
  text while keeping the flag doubles it (18v/13e).
- **the join is one space** — join with no separator gives `ab` for
  `echo a b` (29v/2e).
- **`-e` decodes through the shared table** — `echo -e 'a\nb'` prints a real
  newline; ignoring `-e` leaves the two-character spelling (25v/6e).
- **`--` ends the scan and is consumed** — with `echo -- -n` the `-n` is a
  WORD, so the output is `-n\n`; letting the scan run past `--` swallows it and
  drops the newline (29v/2e).
- **a runtime word is not folded** — folding `Dyn` through the empty
  compile-time placeholder prints only the newline; one environment (`x`)
  disagrees (28v/3e).
- **`\c` truncates the text AND the newline** — `echo -e 'a\cb'` prints `a`
  and nothing else; treating `\c` as an unknown escape prints `a\cb`
  (27v/4e), and keeping the trailing newline (the pre-repair shape) prints
  `a\n` (27v/4e, trapped by four named obligations). The four neighbouring
  shapes are their own pins, each measured against bash's `od -c` bytes: the
  `\c` mid-text (`'ab\ccd'` → `ab`), as the last escape (`'ab\c'` → `ab`),
  after an already-truncated payload (`'a\cb\c'` → `a`) and as the only
  escape (`'\c'` → nothing), plus the no-`\c` control that keeps its
  newline.

`fold_leaves_nothing` (every all-literal list is an `Output`) and
`fold_is_idempotent` are separate obligations, so an identity pass cannot
satisfy the law.

Pins mirror the source's tests: `literal_echo_folds_to_output`,
`flags_and_join`, `dash_e_unescapes`, `expansion_word_does_not_fold`,
`output_without_newline_stays_without_one`,
`dynamic_output_keeps_the_expression`, and `echo_esc_matches_bash_common_cases`
(the rows the model decides: `\n \t \\ \c`), plus the literal escape bytes
(`"hi" = [104,105]`, `"a\nb" = [97,92,110,98]`, `"a\qb" = [97,92,113,98]`).

No hypothesis is carved out for `\c`: the speculative row is gone because the
transform was repaired. `echo_esc_cut` (the shared unescaper) returns
`(text, cut)`, and every newline-deciding consumer honours it —
`fold_to_output`, the Perl renderer's literal-echo path (`ir.rs`), the Rust
renderer's compile-time `const_echo_text`, and the emitted runtime
`__sh_echo_esc` / `__sh_print_words` / `__sh_echo_line`. Measured end to end:
the real pipeline's output is byte-identical to bash's for `echo -e 'a\cb'`,
`'ab\ccd'`, `'ab\c'`, `'a\cb\c'`, `'\c'`, the no-`\c` control, a
literal through a capture, and a runtime word (`echo -e "$MYV"`). The
redirect (`> file`) and `printf` halves of the pass are out of scope: they are
separate laws over separate tables (`redirected_output_to_writefile`;
`constant_printf`/`printf_s_fold`), not instances of this print law.

## sync_ok_loops.rs — the schedule verdicts preserve the emitted bytes (24)

```
    verus --triggers-mode silent --rlimit 40 sync_ok_loops.rs
    -> verification results:: 24 verified, 0 errors
```

**What is proved.** Standalone model of an ANALYSIS-ONLY pass: it never
rewrites the IR, it publishes two pointer-keyed verdicts the ESTree renderer
reads — `sync_ok` (run the loop as ONE sync chunk) and `batch_ok` (run it in
chunks of `BATCH = 1024` with a yield between chunks). The only thing either
verdict may change is the yield cadence, so the law is stated over the emitted
bytes: `batch(b, i, n, k) == eager(b, i, n)` for every body `b`, every
`i <= n` and every checkpoint factor `k >= 1`
(`chunked_emission_preserves_order`, by induction on the chunk sequence with
`eager_split` for the interior split). `k = 1` is the per-iteration await, `k
= BATCH` the checkpointed form and `k >= n` one single chunk, so one equation
covers all three strategies. The census claims are attributed one obligation
each: `sync_ok_implies_finite` (a full-sync run cannot be interrupted, so it
must never select a loop that may not terminate), `cheap_is_within_budget` and
`sync_ok_implies_budget` (the whole run is inside the 200 ms budget by the
uncapped pricing, per-iteration loop overhead included),
`batch_ok_implies_sync_executable` (a chunk must not contain a blocking op or
an unknown-cost call) and `batch_factor_is_a_power_of_two` (`BATCH == 1024`
and `BATCH & (BATCH-1) == 0`, the runtime's bitmask checkpoint).

**Hypothesis, stated not assumed in the law.** A while/do-while whose condition
is data-dependent terminates (`Run::DataDep`) — the code's own stance ("the
corpus has no infinite loops, and bash spins on them too"). A constant-true
condition is NOT covered by it (`Run::Spins`) and is refused. The body is
modelled as the census SUMMARY (priced statements, emitting statements,
blocking ops, unknown-cost ops), which is what `iter_cost` /
`body_output_free` / `batch_executable` compute; modelling the statement list
would only re-derive the census.

**Mutants, all red, no `E0xxx`, each attributing the rule it weakens.** identity
(21v/3e: the completeness pins), flag-everything (18v/6e: finiteness, budget,
the infinite-loop refusal), drop finiteness from the output-free branch
(21v/3e: `sync_ok_implies_finite`), call a Spins loop finite (22v/2e: the
infinite-loop refusal), drop the budget check (19v/5e:
`cheap_is_within_budget`), loosen the budget x1000 (20v/4e: same), drop the
per-iteration overhead (21v/3e: same), drop the sync-executable conjunct
(21v/3e: `batch_ok_implies_sync_executable`), `BATCH` 1024 -> 1000 (22v/2e:
`batch_factor_is_a_power_of_two`).

**Three defects the model forced, all repaired.**

1. `const_true_like` read only `Bool`/`Int`/`Str` conditions, but the parser
   emits `while (( 1 ))` as `exec "let" <words>` and `while [ 1 ]` as
   `test <words>`. Both are a constant TRUE that never changes; unrecognized,
   they were filed as data-dependent (hence finite) and, with an output-free
   body, marked `sync_ok` — an INFINITE loop running with no yield, precisely
   the freeze the module's finiteness rule exists to prevent. Repaired: a
   single integer-literal `let` word and a single operator-free `test` word
   are constant truthiness (a variable or compound expression still claims
   nothing). Tests `a_while_let_true_is_not_finite`,
   `a_while_bracket_true_is_not_finite` (both red before the fix, on the REAL
   parse).
2. The verdict reached the renderer through a POINTER-keyed set, but a
   producer body is CLONED into an arrow before it is lowered
   (`IrExpr::Arrow(stmts.clone())`), so a loop inside `( … )` or `$( … )`
   never matched its recorded address and kept the NATIVE form: no yield and
   no runtime `_capExceeded` cap, i.e. an infinite loop inside a producer
   froze the harness. Repaired in `shir.rs`: the async-region set carries the
   loop NODES as well and `loop_in_async_region` falls back to content
   equality (an over-match only costs a runtime loop where a native one would
   have done). Test
   `estree::an_infinite_while_in_a_producer_keeps_the_runtime_loop` — red
   with EITHER repair reverted, green with both.
3. `cheap_total` priced only the body statements, so `n * 0 <= 200000` held
   for any trip count. Repaired with a per-iteration loop-overhead term, which
   bounds the ITERATIONS too. Honest scope: every priced EMITTING statement
   costs >= 3 µs, so a real emitting loop was already bounded; the hole was
   reachable only for a zero-cost body (which the output-free branch accepts
   anyway) — this tightens the census rather than changing a current verdict.
   Test `a_huge_trip_trivial_body_is_not_cheap`.

**Anchors.** 9 loop shapes (const-false and const-true while, a data-dependent
while, a capture, a subshell and a pipeline producer, an output-free loop,
glob/brace for loops, nested for loops): real bash stdout/exit status vs the
compiled generated C under `MALLOC_PERTURB_=170` — 8/9 SAME. The one
divergence (`x=$(while …; done; echo …)`) reproduces IDENTICALLY with the
pass's changes stashed (verified against the baseline binary), so it is a
pre-existing C-backend bug for a loop inside a command substitution, not a
finding of this model; the same is true of `while (( 0 ))` emitting
`char* 0 = NULL;`. The suite is analysis-only and has no C/Rust consumer, so
the byte-identity of the C path is the expected result. The STATEGY law's real
consumer is the ESTree path, which is anchored STRUCTURALLY (the generated
estree JSON, above) and NOT executed end to end: no JS engine is installed on
this machine (`node`/`bun`/`qjs`/`deno` absent), so "the batched run emits the
same bytes" is proved, and its rendering is pinned, but the rendered program
was not run.


## loop_return_lift.rs — capture at the echo point, stop at the first match (22)

```
    verus --triggers-mode silent --rlimit 40 loop_return_lift.rs
    -> verification results:: 22 verified, 0 errors
```

**What is proved.** Standalone model of the transform that turns

    loop { if C; then echo "$v"; return; fi }   [post]

into

    local flag=0; local ret
    loop { if C; then ret="$v"; flag=1; break; fi }
    if (( flag == 1 )); then echo "$ret"; else [post]; fi

Observing the emitted bytes, `the_lift_preserves_the_observable` is the law for
EVERY iteration sequence (each iteration is `Iter { hit, val }`), every
post-loop body `post`, every trailing body `tail` and every placement:
`go(in_function, it, post, tail, census(in_function, tail)) == go(in_function,
it, post, tail, false)` with `census(in_function, tail) = in_function &&
tail.len() == 0` computed FROM the input — a refused program takes both sides
unchanged, so there is no shape antecedent. The two halves of the placement are
exactly what `break` costs relative to `return`: a `break` leaves only the
loop, so trailing statements are not skipped (non-tail), and at top level a
`return` is an error and the script continues (no function).

**One obligation per load-bearing rule.** `captures_the_matching_iteration` —
the printed value is `it[first_hit].val`, read at the echo point
(`the_value_after_the_match_is_a_different_iteration` shows the loop variable
after the break sits on the NEXT element). `stops_at_the_first_match` — the
flag is set exactly when an iteration matched and the run stops there
(`the_break_really_stops_the_loop` pins `ran == 2` for a hit at 1).
`the_post_arm_runs_exactly_when_the_loop_completes` pins the two arms: the
capture on a hit, the moved `post` on fall-through.

**Not modelled, stated.** The trigger (the then arm is exactly
`[echo "$v", return]` with a pure, quoted `$v`, and a silent else) is the
recognition, not the law; the law holds for every value sequence the lift is
applied to. The capture is a function-scoped local read only on the found path
— a renderer contract. The C path is anchored byte-for-byte; the ESTree path is
not executed (no JS engine on this host).

**Mutants, all red, no `E0xxx`, each naming the rule it weakens.** identity /
no-lift (20v/2e: the census must accept a tail function loop), drop `tail ==
0` (17v/5e), drop `in_function` (19v/3e), accept everything (15v/7e), drop the
flag/break stop rule (16v/6e), keep iterating with the first capture (20v/2e —
the law stays true and only `the_break_really_stops_the_loop` sees it), capture
the next element (17v/5e), drop the post arm (18v/4e).

**Three defects the model forced, all repaired.**

1. The echo value was re-evaluated AFTER the loop. In the matchers' `for f in
   ...` shape the loop variable ends on the element after the match that broke
   out, so the lifted program printed `found 3` where bash prints `found 2`.
   Repaired by capturing into `ret` at the echo point. Test
   `the_value_is_captured_at_the_matching_iteration` (the capture is the first
   write, reads the echoed word, and the post-loop echo reads the capture).
2. The lift fired on a loop that was not in tail position: the original
   `return` skipped the statements after the enclosing block, the new `break`
   did not (measured: an `echo unreachable` ran). Repaired with the `tail`
   conjunct. Test `a_loop_that_is_not_in_tail_position_is_not_lifted`.
3. The lift fired at the TOP LEVEL, where `local` is invalid and `return` does
   not end the script; the generated C failed to build with `redefinition of
   '__sh2_found'`. Repaired with the `in_function` conjunct. Test
   `a_loop_outside_a_function_is_not_lifted`.

**Anchors.** 6 loop/placement shapes (tail function loop, non-tail loop,
nested loop, no-match fall-through, a herestring `while read` wrapped in a
`Redirect`, a quoted non-loop-variable value): real bash stdout+exit vs the
compiled generated C under `MALLOC_PERTURB_=170` — 6/6 SAME, with the lift
firing in 3 and correctly refused in 3 (the herestring loop is refused because
its post statements cannot move out of the redirect). A top-level `return`
shape diverges (`2\nafter` vs `2`) with the pass a NO-OP there (0
`__sh2_found`), so it is the C renderer's own top-level `return` lowering, not
this model; and the multi-name `local i x` form trips a pre-existing C
declaration bug (`i = NULL` into a `uint8_t`) that reproduces with the
transform inactive.


## fuse_fill_consume.rs — the fill and the consumer are the same iteration (37)

```
    verus --triggers-mode silent --rlimit 40 fuse_fill_consume.rs
    -> verification results:: 37 verified, 0 errors
```

**What is proved.** Standalone model of the transform that turns a fill loop

    for ((i=0;i<n;i++)); do a[i]=F; done

followed (possibly across statements) by a consumer loop

    for ((i=0;i<n;i++)); do s=$((s+a[i])); done

into one loop that evaluates `F` at the consumer's own iteration and drops the
array. A `Trace` is the pair of loops as the transform sees them: for each of
`n` iterations the fill's and the consumer's counter value, the store index and
the read index, and whether the store executes; plus the fill RHS `rhs` (an
expression over scalar slots and the counter), the scalar environment at fill
time, the writes between the loops, and the values of any OTHER array reads.
`the_fusion_preserves_the_observable` is the law for EVERY trace:
`go(t, census(t)) == go(t, false)`, where the reference emits the array
elements and the fused side emits `rhs` evaluated in the consumer's
environment. `census` is computed FROM the trace, not from a shape antecedent.

**One obligation per load-bearing rule.** The six census conjuncts are
`all_stored` (a hole would leave `a[read]` at its initial zero),
`affine_fill` (the store index IS the counter), `affine_read`, `same_header`
(same trip count and phase), `stable` (no gap write touches a free name of
`F`, which a forward that sees the gap would otherwise observe) and `alone`
(no other array use observes the array). The value facts are separate:
`the_read_value_is_preserved` (the read element equals the forward at every
iteration), `a_stored_value_is_the_expression_at_its_iteration` (the last
store wins and the affine key makes it THIS iteration's `F`) and
`the_gap_does_not_change_the_expression`.

**Not modelled, stated.** The recognition (which shapes are loops, how `a[i]`
parses to an array/index pair) is what produces a trace; the law holds for
every trace and the transform's own tests pin the recognition. `int` is
unbounded here; 64-bit wrap is the shared renderer/bash contract. The array's
initial zeros are the `MALLOC_PERTURB_`-independent state the generated C has.

**Mutants, all red, no `E0xxx`, each naming the rule it weakens.** identity /
no-fuse (the census must accept the clean witness), dropping each conjunct of
`census` in turn (the law and that conjunct's witness), accept-everything,
`arr_at` ignoring stores (the clean witness's words), `apply` ignoring the gap
(the gap witness) and forwarding at the fill-time counter instead of the
consumer's (the header witness).

**One defect the model forced, repaired.** The census counted writes and
`getVar` calls but not arithmetic EFFECTS, so a consumer body such as
`tmp=$((x=7)); acc=$((acc+zzarr[i]))`, or a gap `k=$((x=7))`, was accepted
while the fill had stored the OLD `x` (measured: bash `2 7`, generated C
`7 7`). Repaired by collecting evaluation-time effects (`ArithAst::Assign`,
`IncDec` and text that cannot be shown effect-free) and vetoing those that hit
`F`'s free names, the counter, the bound or the condition names. Tests:
`consumer_arith_effect_on_a_forwarded_var_vetoes`,
`gap_arith_effect_on_a_forwarded_var_vetoes`,
`consumer_arith_effect_on_the_counter_vetoes`,
`consumer_arith_effect_on_the_bound_vetoes`,
`consumer_text_arith_effect_vetoes`, plus
`consumer_arith_effect_on_an_unread_scratch_still_fuses` as the precision control (all five go red with the two
guard sites disabled).

**Anchor.** 14 shell shapes: real bash stdout+exit vs the compiled generated C
under `MALLOC_PERTURB_` 170 and 85 — 14/14 SAME. Four lifted shapes (canonical,
variable bound, zero-trip, and one exiting 4) lose the array entirely (0
`zzarr` references in the C); eight refused shapes (shifted read, extra
mention, conditional fill, header mismatch, a consumer write to `F`'s free
name, a consumer effect, a gap effect, and a refused shape exiting 5) keep it;
two no-match fall-throughs (fill without consumer, consumer without fill) are
byte-identical. The consumer/gap effect shapes are the ones the repair above
fixed, and they still REFUSE (the array stays).


## text_ops_planner.rs — `wc -l` counts the newline echo appends (11)

```
    verus --triggers-mode silent --rlimit 40 text_ops_planner.rs
    -> verification results:: 11 verified, 0 errors
```

**What is proved.** The pipeline

    echo TEXT | wc -l

is rewritten to a direct count, using the composition the target backend's
node manifest selects: `RegCount(TEXT, /\n/) + 1` on a regex backend
(perl/js), `ArrayLen(Split(TEXT, "\n"))` on C. `the_rewrite_preserves_the_count`
is the law for EVERY text, recognizer verdict and manifest: the printed count
equals bash's `newlines(TEXT) + 1`. The recognizer's verdict is an input to
`go`, so a refused shape (wrong flag, `printf`, multi-word echo, a dynamic
value, three stages) takes the unchanged-pipeline branch — the law is the same
statement on both sides, not a shape antecedent.

**One obligation per composition.** `regex_candidate_counts_the_line` — the
regex backend must ADD the terminator, because the argument is unterminated.
`split_candidate_counts_the_line` — the split backend must NOT add one: the
fields of the argument already include the terminator's line (`fields =
newlines + 1`). The pre-fix counts are pinned separately
(`the_pre_fix_regex_count_is_one_short` for every word,
`the_pre_fix_split_drops_the_empty_line` for the empty word), so a change to
the model that "fixes" history is red.

**Not modelled, stated.** The recognizer (the two-stage `echo TEXT` / `wc -l`
shape and its refusals) produces the text and the verdict; the transform's own
tests pin it. Decimal formatting is the corpus printer's contract. The ESTree
backend is proved but not executed: no JS engine is installed on this host.

**Mutants, all red, no `E0xxx`, each naming the obligation it breaks.** drop
the terminator, count it twice, the pre-fix empty split, `newlines + 2`, forget
the final field, count byte 97 instead of the newline, revert the SPEC to the
pre-fix meaning (`wc_l = newlines`, caught by the concrete pins), the split
backend reusing the regex composition, and loosening the pre-fix pin. The two
compositions are proved equal (`every_candidate_agrees`), so the manifest
choice is routing, not correctness.

**Four defects the model forced, all repaired.**

1. `RegCount(TEXT)` counted the argument, so `echo a | wc -l` printed 0 where
   bash prints 1 — one short for every word. The planner now adds the
   terminator. Test `reg_candidate_adds_the_echo_terminator` (red with the
   `+ 1` reverted).
2. The Perl rendering of RegCount was a list assignment, `() = (TEXT =~ /\n/g)`;
   with `+ 1` it was evaluated in list context and yielded the element count of
   a scalar (0), so the program printed nothing. Scalar context is now forced
   at the node (`scalar(() = (…))`).
3. The C split primitive returned 0 fields for the empty word, so
   `echo "" | wc -l` printed 0 (bash 1). It now returns one empty field.
4. The C emitted for the rewritten program did not compile at all —
   `unknown type name '_sh_vbuf'`: `_sh_splice_v` is emitted with the value
   primitives, but the vbuf type was emitted only inside the shell-runtime
   block, whose last user in this program the planner removes. The type is now
   emitted for either block. 

**Anchor.** 12 shapes: real bash stdout+exit vs the compiled generated C (under
`MALLOC_PERTURB_` 170 and 85) and the generated Perl — 12/12 SAME. Four lifted
literal words (`a`, empty, an embedded newline, a trailing newline; both lower
markers present in the artifacts), six refused shapes (`wc -c`, `wc -w`,
`printf`, a two-word echo, a dynamic `$v`, three stages; neither marker), and
two transform-disabled fall-throughs. The pre-fix measurements are the
failing-first evidence: Perl printed 0/1 where bash prints 1/2, C printed 0 for
the empty word, and the C did not compile.


## Running it

Verus needs rust toolchain `1.99.1-x86_64-unknown-linux-gnu`
(`rustup toolchain install 1.99.1-x86_64-unknown-linux-gnu --profile minimal`)
and, because this box installs rustup under `/root`, an explicit
`RUSTUP_HOME`:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/interval.rs

`--rlimit 40` is needed: the default budget runs out on the product case.

---

# Verus proof of `optimize_stmts` Pass 1 (self-assign + unused-`Declare` elim)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 optimize_stmts.rs
    -> verification results:: 8 verified, 0 errors

## What is proved

`optimize_stmts.rs` is a standalone model of Pass 1 of
`src/ir.rs::optimize_stmts` (the `is_self_assignment` filter and the
init-less-`Declare` filter at `ir.rs:15768`, with the
`collect_referenced_vars` census): interned var names, the three
statement shapes the pass can touch (`Assign`/`Declare`/`Other`), and

    proof fn pass1_sound_top(s: Seq<Stmt>, init: Store)
      requires closed_for(s, init),
      ensures run(pass1(s), init) == run(s, init),

i.e. the pass preserves the final store from any closed start state
(every referenced name bound): a dropped self-assign is the identity
update, a dropped init-less `Declare` performs no update, everything
kept is preserved in order. Supporting lemmas: `run_cons` (cons
unfolding), `insert_same` (rebinding the same value is identity),
`declare_pins` (the model's two empirical pins — see below).

## It can fail (checked)

Changing the model's `step` for `Assign` to write a constant (`0`)
instead of the old value makes the same command report **7 verified,
1 error** — the obligation is not vacuous. The file was restored and
re-checked at **8 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `optimize_stmts.rs` duplicates Pass 1
  by hand. Nothing ties the model to the Rust source, so the two can
  drift — the same caveat as `interval.rs` above.
- The census subtlety that shapes the model: the real census counts a
  `Declare`'s OWN names as references, so a lone `Declare[x]` is KEPT
  (only an empty `Declare[]` drops) — verified empirically against the
  running `optimize_stmts` before modeling, and pinned by
  `declare_pins`. The strong guarantee is the self-assign lemma; the
  `Declare` arm's guarantee is "a kept `Declare` stays kept".
- Pass 0.5 folding is out of scope: `fold_stmt` maps `Assign`/`Expr`
  structurally and clones everything else, so it can neither create nor
  destroy a self-assign or a reference — its soundness is the separate
  constant-fold obligation.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/optimize_stmts.rs

---

# Verus proof of `dead-fn-elim` (dead-function elimination)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 dead_fn_elim.rs
    -> verification results:: 25 verified, 0 errors

## What is proved

`dead_fn_elim.rs` is a standalone model of
`src/transforms/dead_fn_elim.rs` (Passes 1–4: defined names, the
literal census with def names excluded, per-name removal, nested
pruning): interned names, def/call/other statements, the keep rule
(a def survives iff its name occurs as a literal anywhere), and

    proof fn elim_sound(p: Seq<Stmt>, keep_all: bool, fuel: int)
      requires fuel >= 0,
      ensures run_prog(elim(p, keep_all), fuel) == run_prog(p, fuel),

i.e. elimination preserves the call trace (all invocations, externals
included) at every fuel bound — hence for the unbounded runs the fuel
approximates. Execution is a worklist dispatch with first-def-wins
lookup; an undefined callee emits and stops (the shell's external-exec
fallback). The `KEEP_VARIABLES` hatch is a bool parameter (set =
identity). Supporting lemmas: `find_body_kept` (lookup agrees after
elimination for every referenced name), `exec_calls_same` (any
all-referenced worklist runs identically), `top_calls_elim` (top-level
calls unchanged), `body_calls_ref` (a found body mentions only the
program's literals), plus `*_step`/`*_skip`/`*_hit` unfold lemmas and
`elim_pins` (the fidelity pins — see below).

## It can fail (checked)

Inverting the keep rule (dropping *referenced* functions instead of
unreferenced ones) makes the same command report **23 verified,
2 errors** — the obligation is not vacuous. The file was restored and
re-checked at **25 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `dead_fn_elim.rs` duplicates the
  transform by hand. Nothing ties the model to the Rust source, so the
  two can drift — the same caveat as the files above.
- NOT modelled: `InterpPart::Lit` runs. The real `collect_expr_strs`
  skips `Lit` fragments, so a function referenced ONLY through a
  concatenated literal is dropped by the real code but kept by this
  model — the model is conservative (its keep-set is a superset of the
  real one) and the proof covers every drop the model makes.
  Lit-fragment-only drops are out of scope (flagged, not proved).
- `find_body` is first-def-wins; bash uses last-def. Either rule is
  preserved by per-name uniform removal, and both sides of the theorem
  use the same rule — but this is an elim-preservation theorem, not a
  bash-equivalence one.
- Fuel bounds recursion in the spec (Verus needs structural
  termination); the theorem holds for ALL fuel values.
- Fidelity pins (`elim_pins`, mirroring `dead_fn_elim.rs`'s own
  tests): an uncalled fn is removed; a called or indirectly-referenced
  fn is kept.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/dead_fn_elim.rs

---

# Verus proof of `dead-store-elim`'s core (dead-variable elimination)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 dead_store_elim.rs
    -> verification results:: 45 verified, 0 errors

## What is proved

`dead_store_elim.rs` is a standalone model of the core of
`src/transforms/dead_store_elim.rs` (the read/write census, the purity
gate, and the purge, over store + observable-trace semantics):
interned names, `Assign`/`Declare`/`SetVar`/`Output`/`Other`
statements, a structural purity predicate over
`Lit`/`Var`/`Add`/`Call`/`Capture` expressions, and

    proof fn purge_sound_top(p: Seq<Stmt>, implicit: Set<Name>, init: Store)
      requires closed_for(p, init),
      ensures
        run(purge(p, implicit), init).1 == run(p, init).1,
        forall|n: Name| !is_dead(p, implicit, n) ==> lookup(...) == lookup(...),

i.e. purging dead stores preserves the event trace and every live
variable's final value, from any closed start store (every
structurally-read name bound). The induction invariant is
live-agreement (`agree_live`: stores agree on all non-dead names)
plus trace equality; a dropped store is pure (no events, proved by
`pure_no_events`) and dead (its bindings are unobservable). Supporting
lemmas: `eval_same` (evaluations over agreeing state agree),
`keep_step_same`/`drop_step_same` (kept steps run identically,
dropped steps touch only dead state), `step_closed`/`bind_grows`
(steps only add bindings), `bind_all_agree`, plus six fidelity pins —
lone pure store drops, read-later store kept, impure RHS kept,
twice-written kept, shell-implicit kept, dynamic-write freezes all.

## It can fail (checked)

Dropping the purity gate (purging dead stores regardless of RHS
effects) makes the same command report **43 verified, 2 errors** — the
obligation is not vacuous. The file was restored and re-checked at
**45 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `dead_store_elim.rs` duplicates the
  transform's core by hand. Nothing ties the model to the Rust source,
  so the two can drift — the same caveat as the files above.
- The Const-only guard is modelled as single-write
  (`count_writes == 1`): the `analyze_var_const` verdict's key
  structural property for the exploited case. The full const analysis
  is out of scope; the model takes "written once" as the sufficient
  condition.
- Dynamic writes are `Call` nodes with `dyn_write` set; they make the
  read census total, so the dead set is empty and purge is the
  identity — the same outcome as the real early-return, proved
  uniformly rather than as a special case.
- The escapes set is not modelled: every `escapes.insert` in the real
  census is paired with a `reads.insert` on the same name, so escapes
  ⊆ reads and the separate check is defensive redundancy (checked
  arm-by-arm).
- Indexed targets, case-pattern/`Str`-embedded/`[*]`-IFS/`Ext` reads,
  and nested-statement recursion are out of scope as documented in the
  file header: the model covers exactly the droppable class (plain
  scalar stores) over flat sequences, assuming census completeness
  for the covered constructs.
- During this proof two REAL bugs were found in the sibling
  `redundant_store_elim.rs` (not the modelled file): its `expr_pure`
  claims `Arith(_) => true` while `dead_store_elim`'s `arith_pure`
  correctly rejects `Assign`/`IncDec`, and its `stmt_reads` on
  `Assign` ignores target indices, so `x=5; arr[x]=1; x=6` drops a
  live store. Neither affects the modelled file.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/dead_store_elim.rs

---

# Verus proof of `copy-propagation`'s substitution soundness

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 copy_propagation.rs
    -> verification results:: 27 verified, 0 errors

## What is proved

`copy_propagation.rs` is a standalone model of the substitution core
of `sh2perl/src/transforms/copy_propagation.rs` (the single-def table,
the def-before-read gate, and the read-site fold): interned names,
`Assign`/`Output`/`Other` statements over a `Lit`/`Var` expression
language with store + observable-trace semantics, and

    proof fn copy_sound_top(p: Seq<Stmt>, defs: Defs, init: Store)
      requires defs_sound(p, init),
      ensures run(fold_prog(p, defs), init) == run(p, init),

i.e. folding single-def literal reads preserves the run (final store
and event trace) from any closed start store. The single-def table is
a parameter with a soundness precondition (the def site holds the
literal, the write count is 1); the induction invariant is
`prefix_ok` (every already-executed def holds its literal value), and
the run is single-store (both sides from the same store — full
equality threads through, since pointwise `agree` cannot prove `Map`
`==`). Six fidelity pins mirror the transform's own tests
(fold-after-def, refuse-before-def, multi-def count, copy-def
unfolded, opaque-statement refusal, both-reads fold).

## It can fail (checked)

Dropping the def-before-read gate (folding any sound-table read
regardless of position) makes the same command report **25 verified,
2 errors** — the obligation is not vacuous. The file was restored and
re-checked at **27 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `copy_propagation.rs` duplicates the
  transform's substitution core by hand. Nothing ties the model to the
  Rust source, so the two can drift — the same caveat as the files
  above.
- The real `collect_defs`/`count_assigns` machinery (builtin writes,
  arith writes, loop headers, eval-disqualifies-all) is out of scope;
  the model takes "exactly one literal write" as the sufficient
  condition, exactly as the sibling dead-store-elim proof does.
- Copy defs (`x=y`), string-digit arith gates, `param-const-fold`,
  `AffixStrip` folds, quoted-`Interpolate` defs: all out of scope (the
  same single-def/literal core; the extensions reuse this gate).
- Flat statement sequences only; nested bodies are the same induction
  presented per straight-line list.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/copy_propagation.rs

---

# Verus proof of `function-purity`'s verdict soundness

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 function_purity.rs
    -> verification results:: 41 verified, 0 errors

## What is proved

`function_purity.rs` is a standalone model of
`sh2perl/src/transforms/function_purity.rs` (the structural purity
predicates and the call-graph fixpoint): interned names, `FnDef`/
`Assign`/`Decl`/`Output`/`If`/`Loop`/`Other` statements over a
`Lit`/`Var`/`Add`/`Call`/`Capture` expression language, purity as the
meet over the `Impure < Emulable < Pure` lattice, and

    proof fn purity_sound(p: Seq<Stmt>, v: Map<Name, Purity>, f: Name, fuel: int)
      requires table_consistent(p, v), is_defined(p, f),
      ensures
        v.dom().contains(f) && v[f] == 2 ==> run_events(...) == Seq::empty(),
        v.dom().contains(f) && v[f] >= 1 ==> trace_capturable(...),

i.e. under a consistent verdict table, a Pure verdict means an empty
event trace at every fuel and an Emulable verdict means a
capturable-only trace at every fuel. The fixpoint iteration itself is
not modelled (no fuel-indexed table sequence); the theorem takes a
consistent table as a parameter, which the real loop's limit satisfies
by construction. Consistency has three conjuncts: recomputation never
drops below the verdict, every verdict lands in the lattice, and every
key is a defined name (all three are load-bearing — dropping any one
admits a concrete unsound table, see below). Seven fidelity pins mirror
the verdict's behavior (pure-assign body, Output-is-Emulable,
Other-is-Impure, unknown-callee-Impure, pure-call, impure-arg taint,
pure-runs-silent).

## It can fail (checked)

Claiming `Output` of a pure value is Pure (2) instead of Emulable (1)
makes the same command report **39 verified, 2 errors** — the
obligation is not vacuous (the trace carries a Print the verdict
denies). The file was restored and re-checked at **41 verified, 0
errors**. Two further unsound-table classes were found DURING the
proof and are ruled out by the consistency conjuncts: a table claiming
`v[f] = 5` for an UNDEFINED `f` makes `Call(f, [])` compute Pure yet
dispatch a `RunCall` (ruled out by the lattice-bound + defined-keys
conjuncts — the real unknown-callee default is Impure).

## Scope (read this before quoting it)

- It is a proof about a MODEL: `function_purity.rs` duplicates the
  verdict predicates by hand. Nothing ties the model to the Rust
  source, so the two can drift — the same caveat as the files above.
- The fixpoint iteration is not modelled; monotonicity from the
  all-Impure start is out of scope — soundness needs only the limit
  property.
- The `inline_pure_fns` consumer is out of scope (that inlining step is
  Tier-3-flavored: call preservation, not verdict-shaped).
- Flat function bodies; nested statement recursion is the same meet
  presented per straight-line list.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/function_purity.rs

---

# Verus proof of `test-lowering`'s Tier-1 leaf obligations

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 test_lowering.rs
    -> verification results:: 25 verified, 0 errors

## What is proved

`test_lowering.rs` is a standalone model of the per-leaf value
preservation of the four lowering families in
`sh2perl/src/transforms/test_lowering.rs` — nonempty (`-n`/`-z`),
string equality (`==`/`=`/`!=` as `Not(Ne)`/`Ne`), numeric comparison
(`-eq`/`-ne`/`-gt`/`-ge`/`-lt`/`-le`), and glob-affix
(`strHasPrefix`/`contains`/`strHasSuffix` + negations) — over strings
(`Seq<char>`, char-based like the runtime helpers) with var/literal
operands:

    proof fn nonempty_op_sound / streq_op_sound / affix_op_sound / affix_neg_op_sound
      requires closed_for(...),
      ensures lowered_leaf(...) == source_leaf(...),

i.e. each lowered leaf evaluates to the same boolean as the source
test text, from any closed store, plus the numeric arm over i64-range
ints. This is the Tier-1 carve-out P5 names: the full pass obligation
needs statement semantics (status liveness, cond position), but each
leaf rewrite is pure value preservation. Eight fidelity pins mirror
the transform's own tests (nonempty var/empty/zero-negation, string
Eq-as-Not(Ne) true+false, Ne-direct, numeric `-ge`, the four affix
shapes, negated affix).

## It can fail (checked)

Lowering `Eq` to `Ne` directly (dropping the `Not`) makes the same
command report **24 verified, 1 error** — the obligation is not
vacuous (this is the perl-numeric-`==` bug the `Not(Ne)` encoding
exists to avoid). The file was restored and re-checked at **25
verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `test_lowering.rs` duplicates the leaf
  rewrite rules by hand. Nothing ties the model to the Rust source, so
  the two can drift — the same caveat as the files above.
- Status liveness, cond-vs-statement position, the nocasematch gate,
  compound `-a`/`-o` lifting, self-recursion, param-slice operands: all
  out of scope (statement semantics / Tier-3 territory).
- Refusals are modelled as the operand `Option` being `None`: each
  lemma is total only over accepted shapes.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/test_lowering.rs

---

# Verus proof of string-fold Tier-1 obligations (`string_folds.rs`, 11)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 string_folds.rs
    -> verification results:: 11 verified, 0 errors

## What is proved

Two per-node value theorems over a `Str`/`Interpolate` word language:
(a) folding a single-literal `Interpolate([Lit(s)])` to `Str(s)`
preserves the denoted string (`interp_sound`); (b) folding an
empty-default `param(":-"/"-", NAME, "")` to the plain read preserves
the denoted string from any store under the pass's three gates
(plain identifier, not array-homed, program nounset off —
`default_empty_sound`). Six fidelity pins: single-literal folds,
single-Expr refuses, multi-part stays, runtime invisibility, gates
open/refuse, default invisibility.

## It can fail (checked)

Folding the first literal of a MULTI-part interpolate (dropping the
rest) makes the same command report **9 verified, 2 errors**. The file
was restored and re-checked at **11 verified, 0 errors**.

## Scope (read this before quoting it)

- Model, not code (same caveat as the files above). The assign-head
  refusal (shape-preservation) is pinned as a refusal, not a value
  claim. The nounset scan and array census are parameters.
- Coverage sync registered (`string_folds`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/string_folds.rs

---

# Verus proof of arith-identity Tier-1 obligations (`arith_folds.rs`, 11)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 arith_folds.rs
    -> verification results:: 11 verified, 0 errors

## What is proved

(a) `arith_norm_step` preserves the integer denotation (`0-X -> -X`,
`0+X/X+0 -> X`, `X*1/1*X -> X`); (b) float `+ 0.0` strip is the
identity on finite values (`bc_clean_sound`). Six fidelity pins:
negation, add-identity, mul-identity, zero-mul/div refusals,
non-zero no-fold, runtime value, bc strip. Firing: arith-normalize
runs at frontend ingress (not the SH2_TRANSFORMS registry — a 0/562
isolated probe is a probe artifact); bc-float-clean targets the
mimecroft corpus (absent from this checkout).

## It can fail (checked)

Leaving `0+X` unfolded while claiming the canonical form makes the
same command report **10 verified, 1 error**. Restored and re-checked
at **11 verified, 0 errors**.

## Scope

- Model, not code. `/`, `%`, `**` are uninterpreted (refuse-on-doubt,
  exactly as the pass treats them). NaN/infinity out of scope.
- Coverage sync registered (`arith_folds`);
  pins UNMIRRORED (no unit suites in either source — recorded).

---

# Verus proof of test-simplification Tier-1 obligation (`test_folds.rs`, 7)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 test_folds.rs
    -> verification results:: 7 verified, 0 errors

## What is proved

A self-comparison `<op> X X` folds to the table constant
(`-eq/-le/-ge/=` true, `-ne/-lt/-gt/!=` false) and the constant equals
the source denotation in both the numeric and string domains
(`selfcmp_sound_num/str`). Four fidelity pins. Firing: 0/562 on the
shell corpus (no self-comparisons occur) but FIRES on a synthetic
`[ "$x" -eq "$x" ]` probe — mechanics verified live.

## It can fail (checked)

Folding `-le` to false makes the same command report **4 verified,
3 errors**. Restored and re-checked at **7 verified, 0 errors**.

## Scope

- Model, not code. Parens/arithmetic operands out of scope (the pass
  refuses them). Pins UNMIRRORED (no unit suite — recorded).
- Coverage sync registered (`test_folds`).

---

# Verus proof of capture-fold Tier-1 obligations (`capture_folds.rs`, 15)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 capture_folds.rs
    -> verification results:: 15 verified, 0 errors

## What is proved

(a) Stripping trailing newlines is idempotent and echo-join-then-strip
is the substitution value (`strip_idem`, `echo_join_sound`); (b) a
status-0 `Or`/`And` chain evaluates to its reachable arms
(`chain_sound`); (c) splicing a folded `$(echo …)` text span preserves
the text (`text_span_sound`). Six fidelity pins: newline strip, space
join, `$(true)` empty, Or-reachable, And-status-refusal, unknown
refusal. Firing: const-capture-fold 13/562, text-capture-fold 2/562.

## It can fail (checked)

Requiring only ONE status-0 arm for `And` (folding `echo a && false`)
makes the same command report **13 verified, 2 errors**. Restored and
re-checked at **15 verified, 0 errors**.

## Scope

- Model, not code. printf format evaluation, flag detection,
  redirect/multi-statement refusals out of scope (same refusal class).
- Coverage sync registered (`capture_folds`).

---

# Verus proof of list/affix-fold Tier-1 obligations (`list_folds.rs`, 12)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 list_folds.rs
    -> verification results:: 12 verified, 0 errors

## What is proved

(a) Splicing the canonical brace expansion preserves the denoted word
list (`brace_splice_sound`); (b) the single-strip functions denote the
literal-pattern `${v#lit}`/`${v%lit}` shapes (`affix_prefix_sound`,
`affix_suffix_sound`). Six fidelity pins: two-word expansion, empty
group, splice neighbours, prefix strip/keep, suffix strip. Firing:
brace-fold 25/562; affix-strip-fold 0/562 on shell (offered to rs/java
targets — target-gated, not dead).

## It can fail (checked)

Dropping the prefix in the expansion makes the same command report
**11 verified, 1 error**. Restored and re-checked at **12 verified,
0 errors**.

## Scope

- Model, not code. Sequence/range braces, nested braces, Json middles,
  glob-magic patterns out of scope (non-literal refuses in the pass).
- Coverage sync registered (`list_folds`).

---

# Verus proof of text-op literal Tier-1 obligations (`textop_folds.rs`, 15)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 textop_folds.rs
    -> verification results:: 15 verified, 0 errors

## What is proved

The lowered text nodes denote the shell operation on literal operands:
`case_map` preserves length and direction (`StrLen`/case arms),
`str_slice` stays contained (`SubStr`), `gnu_basename` stays bounded
with exact trailing/in-slash cases (`PathName`). Six fidelity pins.
Firing: text-ops fires 180/562 on the C target; literal-operand arms
are a subset. The wc fragment stays Isabelle-only (no duplication).

## It can fail (checked)

Flipping the upper arm to lowercase makes the same command report
**14 verified, 1 error**. Restored and re-checked at **15 verified,
0 errors**.

## Scope

- Model, not code. Field specs, tr sets, sed/sort/uniq programs out of
  scope; wc stays in Isabelle. Non-ASCII case out of scope.
- Coverage sync registered (`textop_folds`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it (all six P6 files)

Same toolchain as above, `--rlimit 40` each:

    for f in string_folds arith_folds test_folds capture_folds list_folds textop_folds; do
      RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
        /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/$f.rs
    done

---

# Verus proof of not-identity Tier-1 obligations (`not_folds.rs`, 13)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 not_folds.rs
    -> verification results:: 13 verified, 0 errors

## What is proved

(a) The double-negation fold `Not(Not(x)) -> x` preserves the boolean
denotation under the pass's duplication-consistency gate
(`fold_sound`); (b) the comparison flip `Not(a OP b) -> a (flip OP) b`
preserves the denotation under the exact-complement hypothesis on the
three op pairs (`flip_sound` + `nat_complements`), which is the
rust-gate's content. Eight fidelity pins: double-fold, single-untouched,
inner/outer inconsistency refusals, Ne->Eq and Lt->Ge flips, two runtime
invisibility checks. Firing: 11 corpus files carry the `!(!(` residue.

## It can fail (checked)

Flipping `Lt` to `Le` (wrong complement) makes the same command report
**11 verified, 2 errors**. Restored and re-checked at **13 verified,
0 errors**. (A pin-counting note: I first wrote the runtime pin with
the wrong expectation — `!(3 Ne 5)` is false, not true; the proof
caught my own error before Verus did.)

## Scope (read this before quoting it)

- Model, not code. The Perl `Eq`-numeric/`Ne`-string asymmetry is the
  documented counterexample for why the flip needs its hypothesis;
  non-Perl backends with unverified op pairs are out of scope (refuse).
- `&&`/`||` De Morgan, quadruple-unwinding traversal out of scope.
- Coverage sync registered (`not_folds`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6 files (`--rlimit 40`).

---

# Verus proof of const-str-cmp-fold Tier-1 obligations (`const_cmp_folds.rs`, 14)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 const_cmp_folds.rs
    -> verification results:: 14 verified, 0 errors

## What is proved

A comparison whose BOTH operands are frozen literals is a
compile-time decision: `Eq/Ne(Str,Str)` (plain styles, glob-free RHS),
`Eq/Ne(Int,Int)` unconditionally, the wrapped `Not(Ne(Str,Str))`
exact_cond (sound on every target — no operator renders), and
`globMatch(Str,Str-literal)` all fold to `Bool` denoting the source
comparison (`const_cmp_sound_eq/ne`, `wrapped_exact_sound`,
`globmatch_sound`). Nine fidelity pins: equal/ne/int literals, glob
refusal, command-style refusal, mixed/dynamic refusals, wrapped
exact, globmatch literal. Firing: measured on the 563-example corpus
(const-propagated case subjects + two-frozen-operand tests).

## It can fail (checked)

Folding `Eq` to inequality makes the same command report **12
verified, 2 errors**. Restored and re-checked at **14 verified,
0 errors**.

## Scope (read this before quoting it)

- Model, not code. Mixed Int/Str, non-literals, command styles, glob
  RHS, claimed affix predicates all refuse (pinned as refusals).
  Traversal out of scope.
- Coverage sync registered (`const_cmp_folds`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7 files (`--rlimit 40`).

---

# Verus proof of `redundant-store-elim` (fixed pass)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 80 redundant_store_elim.rs
    -> verification results:: 53 verified, 0 errors

## What is proved

`redundant_store_elim.rs` is a standalone model of the FIXED
`src/transforms/redundant_store_elim.rs` (a scalar store `x=A` at `i`
drops iff a later scalar store to `x` overwrites it with a clean
middle and a clean justifier): interned names, `Assign` (with target
index expressions) / `Output` / `CallStmt` (observer) / `Other`,
structural purity with side-effecting arithmetic (`ArithSE`, models
`ArithAst::Assign`/`IncDec`) impure, index reads and `Str`-embedded
`$name` reads counted, and

    proof fn rse_sound_top(p: Seq<Stmt>, init: Store)
      requires closed_for(p, init),
      ensures
        run(purge(p), init).1 == run(p, init).1,
        forall|n: Name| lookup(run(purge(p), init).0, n) == lookup(run(p, init).0, n),

i.e. purging is observationally exact — same trace, same value for
every variable. The proof is a shadow argument: after dropping `i`
(target `x`, justifier `j`), `x` is shadowed until the next store;
shadowed vars are never read or observed (`shadow_stops_scan`, proved
from the justifier's cleanliness), so the divergent intermediate
bindings are unobservable. Drop chains always end at a kept store, so
at end-of-program nothing is shadowed (`nothing_shadowed_at_end`).
Supporting lemmas: `eval_same` (evaluations over agreeing state
agree, writes included), `keep_step_same` / `drop_step_same`,
`step_closed`, `shadow_preserved` / `unshadow_before`, plus seven
fidelity pins — basic drop, `ArithSE`-RHS kept, pure-arith control,
index-read kept (bug 2), `srefs`-read kept (bug 3), observer kept,
cascade.

## It can fail (checked)

Two mutants, one per soundness fix:

- Restoring the old `Arith(_) => true` purity lie (modelled
  `ArithSE` pure) makes the same command report **51 verified,
  2 errors**.
- Restoring the old index-blind `stmt_reads` (RHS only) makes the
  same command report **51 verified, 2 errors**.

The file was restored and re-checked at **53 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `redundant_store_elim.rs` duplicates
  the fixed transform by hand. Nothing ties the model to the Rust
  source, so the two can drift — the same caveat as the files above.
- This proof covers the FIXED pass (three soundness holes repaired
  in the same effort): `expr_pure` claimed `Arith(_) => true`
  (`x=$((i++))` dropped the increment — now a dedicated
  `arith_pure` mirroring dead_store_elim's); `stmt_reads` on
  `Assign` ignored target indices (`x=5; arr[x]=1; x=6` dropped
  `x=5` — indices are reads now); `$name` inside plain-`Str` call
  args was invisible (`x=1; y=$(eval 'echo $x'); x=2` dropped `x=1`
  — new `str_reads_var` wired into the `Call` and bare-`Str` arms).
  Each has a failing-then-passing Rust test in the transform's own
  `tests` module.
- NOT modelled: `Ext`-node field reads, case-pattern text reads,
  `[*]`-reads-IFS, whole-store-dumping builtins inside captures,
  nested-statement recursion (same scan per function body),
  multi-target assigns.
- The drop rule is stated declaratively (exists a clean justifier);
  the file header records the scan ⟺ declarative equivalence
  argument, including the justifier-cleanliness clause the scan's
  check order implies.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above (`--rlimit 80` for the shadow-induction
obligations):

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 80 proofs/verus/redundant_store_elim.rs

---

# Verus proof of `array-len-fold` (value preservation)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 array_len_fold.rs
    -> verification results:: 10 verified, 0 errors

## What is proved

`array_len_fold.rs` is a standalone model of
`src/transforms/array_len_fold.rs` (the `fold()` node rewrite with its
refuse-on-doubt guards): the `param("slice", …)` / `arrayLen(…)`
shapes, the guard conjunction (func, arity 3..=4, `"slice"` op, `#`
prefix, plain-identifier base, `"@"` offset, absent-or-empty length),
and

    proof fn fold_sound(e: Expr, s: Store)
      requires foldable(e),
      ensures ev(fold(e), s) == ev(e, s),

i.e. folding preserves the evaluated value (element count of the same
array binding) from any store. Both spellings evaluate against the
same binding, so the fold is a renaming with identical denotation;
the guards' job is only to ensure the slice spelling MEANS element
count. Eight fidelity pins (4-arg, 3-arg, plain-name kept,
numeric-offset kept, non-empty-length kept, non-ident-base kept,
bad-arity kept, folded-counts-elements).

## It can fail (checked)

Dropping the `"@"`-offset guard (folding any offset) makes the same
command report **9 verified, 1 error** — the numeric-offset pin goes
red, so the obligation is not vacuous. The file was restored and
re-checked at **10 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `array_len_fold.rs` duplicates `fold()`
  by hand — guard order included, so the model tracks the code's
  check sequence, not just its verdict. Nothing ties the model to the
  Rust source, so the two can drift — the same caveat as the files
  above.
- Identifier-ness is an oracle (`is_plain_ident` over interned ids):
  the proof covers every other guard and assumes the char-class scan.
- Traversal is out of scope: the real `stmt()`/`expr()` walkers
  recurse everywhere and rewrite bottom-up, but each site is an
  independent local renaming — the theorem is per-node.
- All eight pins are shape-mirrored 1:1 by the transform's own
  in-module tests (15 tests, including traversal and report-shape
  coverage); no `pin_mirrors.rs` addition was needed — the
  dead_fn_elim precedent (already exactly mirrored).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above:

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/array_len_fold.rs

---

# Verus proof of `const-status-cond` (restructuring preservation)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 60 const_status_cond.rs
    -> verification results:: 35 verified, 0 errors

## What is proved

`const_status_cond.rs` is a standalone model of
`src/transforms/const_status_cond.rs` (the 7-row restructuring table
with refusal rules): bare `true`/`:`/`false` conditions with negation
and words flags, `Bool` literals, the moved-command + literal-cond
rewrite per row, and

    proof fn rewrite_sound_top(p: Seq<Stmt>, s: int, f: nat)
      requires f >= 0,
      ensures exec_list(rewrite_list(p), s, f) == exec_list(p, s, f),

i.e. rewriting preserves the (status, print-trace) run from any start
status at every fuel bound — hence for the unbounded runs the fuel
approximates. Execution threads the bash exit-status rules from the
header (body-never-ran reports 0, taken-body reports its last status,
untaken-if reports 0); fuel bounds loop iterations. The induction is
(fuel, size)-lexicographic with two general loop lemmas covering entry
and tails uniformly. Eleven fidelity pins: all 7 table rows as shapes,
words refused, elsifs refused, non-const untouched, plus a fuel-2
behavior run.

## It can fail (checked)

Two mutants, one per load-bearing choice:

- Forgetting to move the command (dropping the prepend) makes the
  same command report **31 verified, 3 errors** — the body-observed
  status is lost.
- `StatusZero` reporting 1 instead of 0 makes the same command
  report **33 verified, 1 error** — the never-ran collapse status is
  load-bearing.

The file was restored and re-checked at **35 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `const_status_cond.rs` duplicates the
  table by hand. Nothing ties the model to the Rust source, so the
  two can drift — the same caveat as the files above.
- `do…while` is refused in the real code; the model omits the node
  (nothing to refuse). Eval-adjacent dynamic effects beyond textual
  `$name` reads, whole-store-dumping builtins inside captures, and
  multi-target assigns are out of scope as documented in the file
  header.
- All shape pins are mirrored 1:1 by the transform's own in-module
  tests (11 tests: every row, both refusals, desugar, non-const);
  the fuel-2 behavior pin is model-internal. No `pin_mirrors.rs`
  addition was needed — the dead_fn_elim precedent.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above (`--rlimit 60` for the fuel/size induction):

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 60 proofs/verus/const_status_cond.rs

---

# Verus proof of the P9 ELIM trio Tier-1 obligations (`elim_trio.rs`, 22)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 elim_trio.rs
    -> verification results:: 22 verified, 0 errors

## What is proved

Three per-pass soundness obligations over store +
observable-trace (prints) semantics, in the shape of the proved elim
family:

(a) `const_condition_elim` (`cce_sound`): pruning a
constant-condition `If` to the taken branch (or first-true elsif /
else), dropping `While(false)`, and rewriting `DoWhile(false)` to
its body (runs exactly once) preserves the run. Conditions are const
booleans; `While(true)` is NOT modelled (the pass refuses it).
(b) `unreachable_after_exit` (`uae_sound`): truncating a statement
list after a DIRECT unconditional `Exit` preserves the run —
execution aborts at `Exit`, so the tail is unobservable. The abort
flag in `run_abort` is the execution model that makes the tail
unobservable; conditional nesting (Exit inside an `If`) never
deadens the outer list (same recursion per body).
(c) `merge_init_assignments` (`mia_sound`): folding an adjacent
`Declare x = <literal>` + `x = <pure rhs>` pair (no self-read) into
`Declare x = <rhs>` preserves the run — the default store is
overwritten before any read can observe it.

Eight fidelity pins: true/false branch selection, while-false
drops, dowhile-runs-once, truncates-at-Exit, keeps-clean-list,
merged-store value, merged-pair agreement.

## It can fail (checked)

Inverting the taken-branch selection (pruning `if true` to the else
arm) makes the same command report **19 verified, 3 errors** — the
obligation is not vacuous. The file was restored and re-checked at
**22 verified, 0 errors**.

## Scope (read this before quoting it)

- Model, not code (same caveat as the files above). The
  `eval_cond`/test-string evaluator is a PARAMETER (a const boolean
  input) — proving `"1 -eq 1"` is string parsing, not the pruning
  obligation. The purity and `reads_var` gates of (c) and the
  array-census gates are parameters; literals satisfy them
  trivially.
- FIRING: no producer in the shell pipeline constructs
  `IrStmt::Exit`/`Return` today (`exit 1` lowers to `Expr(builtin)`)
  — same dead-path class as `for_recovery`. The per-node rule is
  still proved (a true theorem about the rule the pass implements);
  the gate-visible half is the coverage-sync guard set.
- Pins UNMIRRORED (no per-rule unit tests in the three sources —
  recorded, not claimed).
- Coverage sync registered (`elim_trio`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`):

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 40 proofs/verus/elim_trio.rs

---

# Verus proof of slice-identity Tier-1 obligation (`slice_identity.rs`, 10)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 slice_identity.rs
    -> verification results:: 10 verified, 0 errors

## What is proved

`param("slice", NAME, OFF, LEN)` folds to the plain read
`param("", NAME)` preserving the denoted string (`slice_sound`), from
any store, under the pass's exact gate conjunction (3..=4 args, slice
op, plain-identifier name, `OFF` in `{"", "0"}`, `LEN` absent or `""`).
Eight fidelity pins: zero-offset/empty-offset/absent-length folds,
real-offset kept, real-length kept (including `"0"`, which denotes the
empty string), non-identifier kept, plain-read untouched, runtime
invisibility.

## It can fail (checked)

Dropping the plain-identifier gate (folding any name) makes the same
command report **9 verified, 1 error** — the non-ident pin goes red.
Restored and re-checked at **10 verified, 0 errors**. (The value
theorem still holds without the gate — both sides read the same name —
so the pin, not the soundness lemma, is load-bearing here: exactly the
array-len-fold offset-guard shape.)

## Scope (read this before quoting it)

- Model, not code. Identifier-ness is an oracle parameter (char-class
  scan); op styles are not modelled (byte-stability of the export
  round-trip, not value). `"0"` length refuses; 5-arg annotation arity
  refuses (both pinned as refusals). Traversal out of scope (per-node
  theorem, array_len_fold precedent).
- Firing: 0/563 on the shell corpus (no `${v:0}`/`${v:}` shapes occur) —
  FIRES on the 15 synthetic unit inputs (mechanics verified live, the
  test_folds precedent: 0/562 + synthetic firing witness).
- Coverage sync registered (`slice_identity`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`).

---

# Verus proof of case-lowering Tier-1 obligations (`case_lowering.rs`, 29)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 case_lowering.rs
    -> verification results:: 29 verified, 0 errors

## What is proved

(a) Each lowered per-pattern condition denotes its source pattern
(`pat_sound` — Exact via the shared `Not(Ne)` encoding, affixes via
the shared primitives, Glob via the backend's own engine as an
`arbitrary()` oracle called identically on both sides); (b) the
lowered if/elif chain (catch-all = else, post-catch-all dropped)
selects the same branch as the source case for every subject string
(`dispatch_sound`, generalised over the chain-started flag since the
tail always lowers with started=true). Fourteen fidelity pins mirror
the source's 16 unit tests (pure/reserved subjects, literal+affix
dispatch on three live subjects, quoted-star literal, glob
accept/refuse, refused-pattern, impure/arith/positional binds,
leading-catch-all refusal, alternation ORing, nocasematch refusal);
the post-catch-all-dropped shape is model-internal (no source test
asserts the dropped tail).

## It can fail (checked)

Lowering `Prefix` to `HasSuffix` (wrong primitive) makes the same
command report **27 verified, 2 errors**. Restored and re-checked at
**29 verified, 0 errors**.

## Scope (read this before quoting it)

- Model, not code. Quote classification is string parsing (model takes
  classified patterns; Refused = whole-case refusal). The
  nocasematch program probe is a parameter. The leading-catch-all
  refusal and post-catch-all dropping are both modelled (bash
  first-match). The impure-subject single binding is substitution of
  an identical value (copy_propagation single-store shape); temp-name
  freshness is a guard, not a value claim. Traversal out of scope
  (per-site theorem).
- Firing: 38/563 on `--target rs` (every case-bearing corpus file;
  0/563 on `--target pl` by target-gating — the registry gate names
  `perl` while the CLI passes `pl`, so the perl pipeline never enables
  it; measured, not assumed).
- Coverage sync registered (`case_lowering`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`).

---

# Verus proof of else-if-flatten Tier-1 obligation (`elseif_flatten.rs`, 22)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 elseif_flatten.rs
    -> verification results:: 22 verified, 0 errors

## What is proved

Collapsing `else { if … }` (a lone nested `If` with no elsifs of its
own) into the parent's `elsifs` — repeatedly, so any depth lands in
one vector — preserves the selected body under every valuation
(`flatten_step_sound`, `flatten_sound`). Conditions are opaque
predicate ids (the rule is purely structural — the source's own
`a_non_test_condition_is_untouched` test flattens through a
`BinOp(And)` condition); bodies are nat ids. The chain loop is a
fuel-bounded helper (fuel = If count, which strictly drops per firing
step); the scan decomposes via first-hit lemmas. Four fidelity pins
mirror the source's 4 unit tests (single flatten, full chain,
extra-statements refusal, live-valuation selection).

## It can fail (checked)

Dropping the trailing else-body in the splice (appending the elsif but
forgetting `el2`) makes the same command report **18 verified,
4 errors**. Restored and re-checked at **22 verified, 0 errors**.

## Scope (read this before quoting it)

- Model, not code. Status/exit-code threading is out of scope (Tier-3
  statement semantics); the obligation is branch-selection preservation
  (the elim-trio Tier-1 carve-out). Nested-body recursion is the same
  induction per site (traversal out of scope).
- Firing: 2/563 (`090_nested_if_else.sh`,
  `param-expand-default-operator.sh`) + the 4 synthetic unit inputs.
- Coverage sync registered (`elseif_flatten`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`).

---

# Verus proof of seq-range Tier-1 obligation (`seq_range.rs`, 11)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 seq_range.rs
    -> verification results:: 11 verified, 0 errors

## What is proved

Rewriting `for v in $(seq …)` (the captureWords/exec/`seq` shape) to
the bare `Range{start, end}` iterable preserves the iterated integer
list (`seq_range_sound`): 1-arg `[n]` is `1..=n`, 2-arg `[a, b]` is
`a..=b`, descending/empty-at-zero ranges are empty on both sides
(`seq 5 1` prints nothing with default step +1; the empty Range
iterates nothing). Eight fidelity pins mirror the source's 8 unit
tests (two-arg lift, one-arg starts-at-one, descending-is-empty,
step-form refusal, float/leading-zero refusal, body-write refusal,
setVar-write refusal, huge-span refusal).

## It can fail (checked)

Accepting 3-arg (step) forms by taking the first two args (dropping
the step) makes the same command report **9 verified, 2 errors**.
Restored and re-checked at **11 verified, 0 errors**.

## Scope (read this before quoting it)

- Model, not code. Literal parsing (no floats/octal forms/flags,
  2^53 exactness) is string parsing — the model takes parsed ints
  (`None` = refuse) plus `big_ok`/`span_ok` flags; the capture/exec
  shape match is structural recognition (per-site theorem,
  array_len_fold precedent). The body-write scan is a parameter.
  Backend consumption of bare Range (native counters, bounded
  materialization) is out of scope: the obligation is the word-list
  identity at the shIR level.
- Firing: 0/563 on the shell corpus (`for i in $(seq …)` shapes do not
  occur) — FIRES on the 8 synthetic unit inputs (mechanics verified
  live, the test_folds precedent).
- Coverage sync registered (`seq_range`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`).

---

# Verus proof of ternary-desugar Tier-1 obligations (`ternary_desugar.rs`, 9)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 ternary_desugar.rs
    -> verification results:: 9 verified, 0 errors

## What is proved

(a) Rewriting the C frontend's `ternary(cond, a, b)` call into
`Ternary{cond, then, else}` with the normalized `test`/`arith`
condition preserves the selected branch (`ternary_sound`); (b)
normalizing `testArith(s)` to the universally-rendered `arith(s)`
denotes the same predicate (`testarith_sound`). The `test`/`arith`
engines are `arbitrary()` oracles called identically on both sides
(the rewrite preserves the predicate call — the glob-arm shape from
case_lowering). Six fidelity pins record the exact mapping table
(test-string→test, non-test→arith, call passthrough,
testArith normalization, other-refuses, live branch selection);
UNMIRRORED — the transform has no unit suite (suite-less source,
recorded).

## It can fail (checked)

Routing test-strings to the `arith` engine (wrong oracle) makes the
same command report **8 verified, 1 error**. Restored and re-checked
at **9 verified, 0 errors**.

## Scope (read this before quoting it)

- Model, not code. The `is_test_string` split is string parsing
  (classified input here). The estree-path exemption (estree renders
  `ternary` natively) is pipeline placement, not a value claim.
  Backend agreement on the string grammars is renderer contract.
  Traversal (including the ternary-before-descend ordering) out of
  scope (per-site theorem).
- Firing: ingress-only (runs at A1 ingress for `--shir-in-*`, not in
  the SH2 registry) — 0/563 in-registry by construction; fires on
  C-frontend input.
- Coverage sync registered (`ternary_desugar`).

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`).

---

## P12 — `escape_classes.rs`, `f32_narrow.rs` (VERDICT analyses)

The first two models aimed at an ANALYSIS-ONLY pass whose output is a
`static Mutex` verdict table a RENDERER reads, rather than at a
rewriting. They take the `function_purity` shape (P4): the theorem
quantifies over the table the analysis would produce, and proves the
consumer's use of it sound.

### `escape_classes.rs` — 14 obligations

```
    verus --triggers-mode silent --rlimit 40 escape_classes.rs
    -> verification results:: 14 verified, 0 errors
```

**What is proved.** `escape_classes.rs` builds a reference census
(`scopes` : name → the set of function scopes it appears in;
`dynkey` : names indexed by a non-literal key; `whole` : whole-array
positions) and classifies in pass 2. The model states `class_of` as
that classification verbatim over an interned census (`c[n]` = the
number of distinct scopes) and proves the two directions the C
consumer (`c_backend.rs:9929`, `acc_idents`) needs:

- `local_is_plain` / `local_sound`: a **Local** verdict implies the name
  carries **no** dynamic-key mark and **no** whole-array mark — so a
  native binding for it cannot be defeated by an indexed access, because
  there is none in the program.
- `marked_not_local` (+`_whole`) and `multi_scope_store`: any mark at
  all, or a reference from two scopes, forbids Local — the verdict is
  not vacuous.
- `pin_acc_ok_matches_consumer`: the model's acceptance predicate equals
  the C arm `Local || (store_ok && Store)` read off the class, so the
  theorem is about the decision the renderer actually makes.
- `pin_local_accepted_without_store_ok`: the Local arm needs **no**
  `store_ok` premise — which is why the `eval`/`source`/`${!name}` blob
  gates only the Store arm.

Six fidelity pins record the census→class shapes (plain scalar ⇒ Local,
`arr[$i]` ⇒ Store / StoreArray, literal-key array ⇒ NativeArray, two
scopes ⇒ Store). UNMIRRORED — analysis-only source, no unit suite
(recorded, not claimed).

**It can fail (checked).** Dropping the multi-scope arm of `class_of`
(a `Local` verdict for a name referenced from two scopes) reports
**11 verified, 2 errors**. Restored and re-checked at **14 verified,
0 errors**.

**Scope.** The census is a PARAMETER: the theorem is about the
classification given the three sets, not about the census walk
recomputing them — the walk's fidelity is the guard list in the sync
manifest (`multi-scope criterion`, `dynkey mark (assign target)`,
`dynkey mark (Index)`, both consumer arms in `c_backend.rs`).
String-keyed indirection (`$name`, `eval`) is outside the census by
construction and is the C `store_ok` blob's job, not modelled here.
Firing: the verdict fires on 1 of 563 corpus files (measured with
`SH2_DBG_ACC=1` over `--target c`, 2026-10-28: 1 candidate line) —
the analysis is narrow by construction (every array, `local`, or
cross-scope name is refused), so the proof's value is the decision
rule, not corpus coverage. Coverage sync registered
(`escape_classes`).

### `f32_narrow.rs` — 10 obligations

```
    verus --triggers-mode silent --rlimit 40 f32_narrow.rs
    -> verification results:: 10 verified, 0 errors
```

**What is proved.** The verdict (`is_f32_var`, consumed by the C
renderer's `float` home and the estree `Math.fround` lowering) rests
on the exactness-preserving form table in `combine`'s non-literal arm.
The model states the lattice (`FVal { dom, exact, has_cval, cval }`),
`operand_exact` (Float ⇒ `exact`; Int ⇒ the `cval` round-trip;
Unknown ⇒ never) and `form_ok` verbatim.

The table itself is a THEOREM, not a pin set (2026-11-24).
`verdict_exact(fv, v)` is the value-taking verdict ("dom = Float &&
exact ⇒ `f32_exact(v)`"), `denotes`/`bin_val` give each lattice its
concrete value and each op its `real` result, and `verdict_is_exact`
proves: for exact operands `x`,`y` that the lattices denote and any
`op`,`l`,`r` the table accepts, `combine_nonlit(op,l,r)` is exact and
`bin_val(op,x,y)` round-trips through `f32`. So `combine`'s
`exact = operand_exact(l) && operand_exact(r) && <table>` is a fact for
every operand pair, not a table nobody checked. The two IEEE facts a
`real` model cannot derive are CARRIED as named hypotheses:
`zero_is_exact`, and `negation_is_exact` (rounding commutes with `-`,
used by `* -1` and `/ -1`). `verdict_exact_lit` is the literal lane and
`combine_exact_lit` the fold contract. Pins:

- `pin_mod_refused`: `%`/`//`/`**` are never accepted (the table
  enumerates only `+`,`-`,`*`,`/`).
- `pin_general_mul_not_form` / `pin_mul_one_form`: a general `*` of two
  exact floats is REFUSED (the `4097.0 * 4097.0 = 16785409` hazard,
  which f32 rounds to 16785408) while `*±1` is accepted.
- `pin_lit_pair_contract`: an accepted literal fold carries the
  round-trip contract (`f32_exact` of the folded value), not an
  accident of the operand rule.
- `pin_var_lattice` / `pin_unknown_not_exact` / `pin_int_operand_uses_roundtrip`:
  the four lattice states and the two operand rules, including
  `16777217`-style ints refusing.
- `zero_exact`: `*±0` is accepted only because zero itself round-trips.

The pins are UNMIRRORED by design — the sync manifest's pin scan sees
`proof fn pin_*` only, so the two law symbols are guarded BY NAME there
instead (`guard law verdict_is_exact`, `guard literal lane
verdict_exact_lit`) along with the premise the model shares with the
source (`let exact = operand_exact(l)`). The source is not
analysis-only in fact: it carries a 21-test unit suite
(`exact_literal_narrows`, `structured_plus_zero_narrows`, …), but no
pin is mapped onto it.

**It can fail (checked).** Dropping the `*` arm from `form_ok` (the
exactness-preserving product rule) reports **7 verified, 1 error**
(`pin_mul_one_form`). Widening `form_ok`'s `+` arm to EVERY add
(`(op == op_add())`) — the mutant the new law is about — reports
**9 verified, 1 error**, on the `assert(is_zero(r))` in
`verdict_is_exact` that extracts the accepted form's zero. Both
restored and re-checked at **10 verified, 0 errors**.

**Scope.** The model is a `real`-valued abstraction: `f32_round` is
uninterpreted with the idempotence law `f32_exact` spells, and
`fold_val` is uninterpreted (the `py_fmod`/floor semantics are out of
scope — the caller's contract is the round-trip, which is what is
stated). The whole-program at-risk FIXPOINT (dropping candidates whose
inexact op may execute in f32) is not modelled step-by-step; the model
states its premise as the lattice-level rule it preserves
(`cand_subset`: candidates are only removed). Firing: on shell input
the verdict set is EMPTY by construction (`f32_narrow.rs` header: shell
A1 has no float-domain defs) — the proof is for the Python-frontend
consumer. Coverage sync registered (`f32_narrow`).

### Not attempted from the same batch (recorded, not proved)

- ~~`background_decide`~~ — DONE (`background_decide.rs`, 27): the THREAD
  verdict is proved observationally equal to FORK for every body the ordered
  rule accepts, and the shipped set-comparison check is proved UNSOUND on a
  concrete witness. It stays the conditional "middle rung" (the routing's
  environment model is stated, not proved) and `is_thread_body` still has no
  callers outside its own file. See its section below.
- ~~`sync_ok_loops`~~ — DONE (`sync_ok_loops.rs`, 24): criterion 1 and 2 do
  select a rendering strategy, and the Tier-1 obligation is exactly that the
  chosen form preserves the emitted bytes — proved as
  `chunked_emission_preserves_order` for every body, iteration count and
  checkpoint factor, with the finiteness/budget/executability census claims
  as separate obligations. See its section below.
- `type_query` — the eval-time rewrite's obligation is
  "the answer equals what `type` would print", which needs bash's
  function-definition state at the splice point; the transform re-runs
  after `perl_generator_fixed::fixups`, which Tier-1 does not model.

## background_decide.rs — a THREAD verdict prints what FORK prints (27)

    verus --triggers-mode silent --rlimit 40 proofs/verus/background_decide.rs
    -> verification results:: 27 verified, 0 errors

`transforms/background_decide.rs` is analysis-only: for each `cmd &` body it
stores a verdict, THREAD (a worker with a FRESH runtime may run the body) or
FORK (bash's own subshell, on the current chain). The routing is sound iff a
THREAD body prints the bytes FORK would, and that is what the model states
and proves (`thread_equals_fork`): `intended_ok(s, {})` implies
`as_fork(s, m) == as_thread(s, m)` for every inherited environment `m`.

What is proved:

- **`thread_equals_fork`** — the equivalence above, under the ordered rule
the module's own doc states ("a variable the body itself assigns first …
before it is read"). `IfRead` requires its condition's name to be bound, and
`defined_after` keeps only what BOTH arms bind, because bash cannot know
which one ran; `agree_lemma` carries "the two environments agree on every
name the body needs" through the `if`.
- **`intended_implies_shipped`** — the ordered rule is at least as strict as
the shipped SET comparison, so fixing the check would refuse nothing the
pass accepts today.
- **`shipped_check_is_unsound`** — the negative control, and the reason this
file exists. `is_self_contained_stmt` collects reads and assigns into two
HashSets and compares them as sets, so it accepts
`if [ -n "$x" ]; then x=1; echo 7; fi`: the subshell inherits `x=1` and
prints `7`, a fresh runtime prints nothing. The lemma proves all four halves
of that witness (the shipped check accepts, the ordered rule refuses, FORK
prints one value, THREAD prints none).
- **`classify_thread_sound`** / **`route_thread_is_sound`** — the verdict end
to end over the SHIPPED walk (`walk_self_contained` accepts ⇒
`intended_ok`) and then over the shipped predicate `is_thread_body`, whose
table can hold only accepted bodies (`table_entry_is_accepted`).
`walk_ok_implies_intended` is the bridge, and it needs
`walk_after_dominated` (the walk keeps straight-line binds only, the rule
also keeps what both arms bind) plus `walk_ok_mono`.
- **`unclassified_stays_fork`, `absent_table_routes_fork`** — the refusal
direction: an absent table (transform never ran) or a body with no entry
routes FORK. **`side_effect_stays_fork`** — a file write or a control
transfer is never accepted (`side_effect_never_accepted`), so it can never
enter the table.

**It can fail (checked).** Accepting any read in the shipped walk
(`walk_ok`'s `OutRead` arm ⇒ `true`) reports **25 verified, 2 errors**, both
in the bridge (`walk_ok_implies_intended`, `walk_ok_mono`); making the
ORDERED rule accept any read (`intended_ok`'s `OutRead` arm ⇒ `true`)
reports **25 verified, 2 errors**, in `agree_lemma` and
`intended_implies_shipped`. The file was restored byte-identically after
each run (sha256 `d026993f6882…`) and re-checked at **27 verified, 0
errors**.

**Scope (read this before quoting it).** This is the conditional "middle
rung": it mentions the denotation of a statement (`run`), so the
equivalence holds under a stated environment model — reads inherit the
parent's bindings, writes die with the subshell, stdout is shared — not for
arbitrary bash. Not modelled: concurrency and scheduling, signals, exit
codes, fd state, PATH resolution, `Exec` subprocess effects, the
per-statement `body.iter().all(…)` loop, and the pointer-keyed verdict
cache. The table's key is the body's `Debug` text (`body_key`); "equal text
⇒ equal body" is an assumption about the derived `Debug`, not a proved
injectivity lemma. And `is_thread_body` has NO call sites outside its own
file, so no emitted program depends on the verdict yet: the proof makes it
usable and gateable, not wired.

There is no `background_decide_sync.txt` and no `pin_*` fidelity obligation
(18 `proof fn`s, 27 verified obligations), so there are no rows for
`check_sync.py` to run; the concrete evidence is in-file, in
`shipped_check_is_unsound`'s witness.

---

## C-generator `cstr` escaper — `c_str_lit.rs` (19 obligations)

```
    verus --triggers-mode silent --rlimit 40 c_str_lit.rs
    -> verification results:: 19 verified, 0 errors
```

**What is proved.** Standalone model of the C backend's STRING-LITERAL
ESCAPER — `cstr` (`src/c_backend.rs:10732`), the single funnel for every
emitted C string literal (178 call sites incl. heredoc bodies AND the
printf FORMAT string built by `heredoc_body_c`). Models the C lexer as a
finite-state machine over one char per step (`run` + `flush`) and proves
the round trip:

- `round_trip` — THE OBLIGATION: `decode(emit(s), Normal) ==
  Some(bytes_of(s))` for every valid source string — the bytes the C
  compiler puts in the program are exactly the bytes bash would have
  seen;
- `decode_char` — the per-char bridge: `decode(emit_char(c) ++ b) ==
  pre(byte_of(c), decode(b))`, the locality property that is exactly
  what the hex spelling fails;
- `oct3_value` — `fold_digits(oct3(v), 8) == v` for `v ≤ 255` (the
  `{:03o}` spelling is value-exact);
- pins: the measured literal texts (`\001a`, `\0019`, `\0017` digit
  followers, named escapes, marker `\263` for byte 0xAB).

**It can fail (checked).** The SHIPPED-HEX DIVERGENCE pins are the
negative control: `decode` of the OLD `\xNN` spelling gives the MEASURED
wrong values — `emit_old([1,'a'])` decodes to `Some([26])` not `[1,97]`
(t1), `[1,'9']` to `Some([25])` (t5), and `[1,'A','B','C']` to `None`
(the out-of-range arm, t7). Each is an `assert(... != ...)`-style pin
against `bytes_of`, so reintroducing the hex spelling fails loudly.

**Scope.** Domain `valid_c`: ASCII (≤ 127) or marker (U+F800..F8FF); NUL
round-trips at this level but bash values are NUL-free anyway. NOT
modelled: non-ASCII Unicode re-encoding (raw passthrough is
byte-identity under UTF-8); `read_source`'s marker DECODING (input side);
the atoll numeric fold's separate unescape; printf `%` semantics
(`%` doubling lives in `heredoc_body_c`, upstream of `cstr`); compiler
source-charset assumptions (UTF-8 default). Unknown escapes are refused
(fail-closed: `cstr` never emits one). Gate-registered
(`c_str_lit.rs:19:40`). The `c_*` models are C-generator emission
decisions, not shIR→shIR passes, so they carry no
the retired coverage script baseline — their living tie is the pin↔probe
mapping, as in the C UB proof notes.

## The C-generator redirect reorder + quoter — `c_redirect.rs` (46 obligations)

```
    verus --triggers-mode silent --rlimit 40 c_redirect.rs
    -> verification results:: 46 verified, 0 errors
```

**What is proved.** Standalone model of the C backend's redirect
emission: the fd-prefix round trip (`fd_parse`), the word quoter
(`quote_roundtrip`/`esc_in_sq` — the bytes the child shell word-parses
out of `_sh_word(v)` are exactly `v`), the dup/close raw arm
(`raw_arm_no_quotes` — `2>&1`/`2>&-` emit no quote characters), and the
heredoc reorder: `reorder_safe` says `emit_order` preserves the fd
table for every starting table under `safe(rds)`, `commute_here` is the
swap it rests on, and `n1_fd0_conflict` proves the side condition
NECESSARY (measured 2026-11-01 `/tmp/redprobe`: bash `cat <<EOF <
in.txt` → FROM_FILE, the generated C → FROM_HEREDOC; the swapped script
agrees in both).

**It can fail (checked).** The named mutants are the negative
controls: `m1` (unescaped word — an embedded `'` closes it early),
`m2` (`>&'1'` would name a file), `m3` (terminator not moved). The
proof also REFUTED an over-strong lemma on the way: `ends_with_concat`
without the `|suf| <= |s|` guard is false, and the guarded form is what
the model states.

**Model defect found and fixed.** `reads0` covered `W|R|Rplus` while
`written` matches `W|Wc|A|R|Rplus` — a Wc/A Rd with a `&0` target read
`t.get(0)` with `reads0 == false`, making `written_frame` provably
false; `reads0` now covers exactly `written`'s mode set (the strict,
sound direction for `safe`).

**Scope + discipline.** `by (compute)` over `run` with a symbolic state
diverges (measured >15 min in the interpreter thread) — the quoter uses
the structural `run_one`/`run_concat` chain instead, and ground seq
facts are explicit asserts. Beyond the quoting layer, the child's word
splitting is bash's contract; the IR has no Close mode so `2>&-`/`2>-`
render identically (`n2` pin). Gate-registered (`c_redirect.rs:46:40`);
no sync baseline — the pin↔probe mapping (`/tmp/redprobe`) is its
living tie.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`).

## `type_query.rs`

Target: `sh2perl/src/transforms/type_query.rs` (`type NAME...` -> one
`echo` line per queried name). 17 verified obligations, 0 errors.

The theorem is stated over the call's fields (`names`, the `-`-flag and
non-Str-word refusal flags), per call site. What it proves:

* `rewrite_shape` -- a rewrite preserves the queried names and their
  order; a refusal leaves the call untouched. Order is the `head -1`
  property: `type doselect 2>/dev/null | head -1` must report `doselect`.
* `answer_complete` -- every emitted line has an answer, because the
  rewrite's premise is `all_classified`; nothing is answered by guessing.
* `unclassified_refuses` -- one unclassifiable name refuses the WHOLE
  call. A partial rewrite would print the known lines and silently drop
  bash's `not found` line for the unknown one.
* `pin_function_precedes_builtin` / `pin_external_no_claim` -- a program
  function shadows a builtin of the same name, and a name in neither set
  is never answered (`type ls` must not say "a shell builtin").

Fidelity: the function set and the builtin table are parameters. The
table is MEASURED, not recited: `builtin_table_matches_compgen` asserts it
equals `compgen -A builtin` on the bash that runs the suite (61 names —
the recited table was 58 and missing the special builtins `.`, `:` and
`break` until this test existed), and
`builtin_table_excludes_externals_and_has_no_copies` keeps it
duplicate-free with `ls`/`head` absent. Set `SH2_BASH` to measure a
different build. The echo's per-line TEXT is not modelled (a
line is a function of its name and classification); the node carries the
names in order, which is what the consumer reads.

---

# Verus proof of `terminator_binop` (`terminator_binop.rs`, 19)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 terminator_binop.rs
    -> verification results:: 19 verified, 0 errors

## What is proved

`terminator_binop.rs` is a standalone model of
`src/transforms/terminator_binop.rs`: the fold
`Expr(BinOp{op: And|Or, lhs: <control call>, rhs})` →
`Expr(<control call>)`, i.e. `break || exit 1` loses its dead right
operand. Trace + status + abort-flag semantics:

    proof fn bin_control_dead(lhs, is_and, rhs)
      requires is_control(lhs),
      ensures  step(Bin{lhs, is_and, rhs}, false)
               == step(Plain(lhs), false)

for BOTH operators, ANY right operand, and ANY status the control call
reports — the operator is never consulted, so `break` (0), `exit 3` (3)
and `return 2` (2) all fold. `fold_stmt_sound` is the per-statement
lemma; `fold_list_sound` lifts it over a statement list by induction,
so the transform's body recursion is that theorem instantiated at each
body list (a body is just another list). `run_list` threads the abort
flag, which is the vocabulary the other termination folds reuse: once a
step aborts, the tail is unobservable.

`capture_and_fold_unsound` is the REFUSAL, not a nicety: inside `$( )`
a control word is the subshell's business. Measured on this box:

    while true; do break 99; done; echo $?   -> 0
    while true; do break && echo RHS; done   -> no RHS
    while true; do break || echo RHS; done   -> no RHS
    exit 3 && echo RHS; echo $?              -> no RHS, rc=3
    f() { return 2 || echo RHS; }; f         -> no RHS, rc=2
    x=$(break && echo RHS); echo "[$x]"      -> "[RHS]"
    x=$(break || echo RHS); echo "[$x]"      -> "[]"
    i=0; while [ $i -lt 1 ]; do i=1; if true; then break; fi; echo TAIL; done
                                             -> no TAIL

So in capture position `&&`'s right operand DOES run: folding there
would silently drop output. The model proves exactly that difference
(`step(..., subshell=true)` prints the RHS, the folded form prints
nothing), which is why the transform only visits a statement-position
`BinOp`. The abort flag also crosses a body boundary
(`pin_abort_through_if`: `if true; then break; fi; echo TAIL` never
prints TAIL).

## It can fail (checked)

Two mutants, one per load-bearing choice:

- A control call returning normally instead of aborting (the `aborted`
  flag dropped) makes the same command report **14 verified, 5 errors**
  — `&&`'s right operand becomes reachable and the fold loses it.
- Ignoring the subshell flag (control aborts inside `$( )` too) makes
  the same command report **18 verified, 1 error** — the refusal
  theorem's premise collapses.

The file was restored and re-checked at **19 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: `terminator_binop.rs` duplicates the
  fold by hand. Nothing ties the model to the Rust source, so the two
  can drift — the same caveat as the files above.
- **The transform is PARKED.** `terminator_binop` has no `pub mod`
  line in `src/transforms.rs` (69 declarations for 77 files), so the
  rule is in the tree but changes no emitted byte today — the same
  gate-visible status as `loop_opt`'s `DEAD CODE` note. The theorem is
  about the rule the module implements; wiring it up is a separate,
  measured decision (the corpus carries the shapes:
  `examples/parse-break-or.sh`, `parse-return-and.sh`,
  `parse-return-or.sh`).
- Name recognition (`control_call_name`) is modelled as data and
  pinned, not proved: the bare spelling and the `builtin`/`exec`
  wrapper both resolve to a control word; a non-static word
  (`"$cmd" || …`) is refused. That is a lookup, not a value theorem.
- Bodies (`If`/`Nested`) exist to show the abort flag crossing a body
  boundary; the model's fold is stated per list, so the transform's
  traversal recursion adds no obligation.
- The emitted targets are not modelled: the obligation is that the
  rewrite preserves the shell run, not that a renderer's `break` arm
  emits the right bytes (those are the C UB proof notes / per-backend
  obligations).
- `ran` distinguishes "some step happened" from an empty tail, so a
  list's final status is its last real step's (`[Fail]` stays 1).
- Pins mirror 6 of the source's 7 unit tests (break-or, lifted break,
  exit-or, ordinary-guard kept, and-after-control, capture refusal);
  the 7th (loop-body recursion) is the per-list instantiation above.
- No `*_sync.txt` baseline: the source is suite-only and the model is a
  reduction, so pins are recorded rather than transcribed.


## stray_reserved_word.rs — a stray `)` / `}` is a parse error (14)

`transforms/stray_reserved_word.rs`. bash treats a bare `)`/`}` in command
position as a grammar token, i.e. a parse error: the statements BEFORE it
have run, then it exits 2 and discards the tail. The parser recovers it as
an ordinary command, so every backend used to EXECUTE it — status 127, tail
still running — which is what the corpus gate saw
(`exit code (bash=2 perl=127)`).

Proved (`lower_matches_bash`, every program and every fuel):

    proof fn lower_matches_bash(p, fuel)
      ensures run_ir(lower(p), fuel) == run_bash(p, fuel)

where `run_bash` treats a stray as the abort and `run_ir` is the PRE-FIX
runtime (stray exec'd as a command, tail continues). `lower` replaces the
stray and everything after it with `Warn + Exit(2)`, keeping the prefix.
`gap_witness` states the disagreement for `echo; ); echo` (127 with both
traces vs status 2 with only the prefix) as an obligation of its own, so the
theorem is not "the pass agrees with the pass". `abort_keeps_token` /
`truncation_is_load_bearing` pin the diagnostic token and the one-element
abort.

Mutant, measured: keeping the tail after the abort is **12 verified,
2 errors** on those two truncation pins, while execution equivalence still
holds — an exit after the abort cannot run the tail either way. So
truncation is recorded as a code-size claim, not a status one.
Pins mirror the source's three tests (`pin_prefix_survives_and_exits_2`,
`pin_quoted_arg_untouched`, `pin_ordinary_command_untouched`) plus the `}`
token.

## merge_init_assignments.rs — the pair, as a whole-program theorem (15)

`transforms/merge_init_assignments.rs`. `elim_trio.rs` (c) already proved the
single step, but with `pure` and `reads_self` as PARAMETERS. This file makes
the guards structure of `lower` and quantifies over programs:

    proof fn lower_sound(p, st)
      ensures run(lower(p), st) == run(p, st)

The local fact is `pair_equals_merged`: both the pair and the merged
`Declare(x, e)` bind `x` to `eval(e, st)`, because the default is
overwritten before anything reads it and `e` (which does not read `x`)
evaluates the same before and after the default is installed
(`eval_ignores_own_binding`, `insert_insert`).

Both guards are load-bearing as theorems about the shipped rule: dropping
`!reads` is **14 verified, 1 error** (`lower_sound` fails on the witness
`x=5; Declare(x,0); x=x` → 0 unmerged vs 5 merged) and dropping the name
check is **13 verified, 2 errors**. The adjacency guard has its own witness
(`adjacency_guard_is_load_bearing`: an observation between the two statements
would print the default `0`, not `7`) and `non_adjacent_pair_is_refused`
proves the non-adjacent shape is left alone.

Scope: the pair shape only — indexed assignment targets, `asm`, and the
`pure` predicate's call/capture exclusions are absent from the model's
expression language rather than assumed; nested-body recursion is the same
local step at each depth.


## definedor_hoist.rs — one evaluation for `${a:-b}` (9)

`shir_passes/definedor_hoist.rs` (a `shir_passes/` transform, not a
`transforms/` module — the shIR passes had 13 of 26 modelled before this). `x = ${a:-b}` arrives as
`Assign(x, DefinedOr { expr: a, default: b })`, and a renderer with no
`DefinedOr` case emits the subject twice — once for the test, once for the
value — so `$(cmd)` or `$((i++))` runs twice. The pass binds it first:

    __sh2_dor_0 = a;
    x = ${__sh2_dor_0:-b};

Proved (`hoist_matches_bash`): the hoisted pair writes the same `x` as
bash's one-shot `${a:-b}` and performs the same number of evaluations
(`temp_test_agrees` is the hinge: the temp holds exactly the value the
test saw, so the second line's `:-` decision agrees with the first line's
read). The temp is the only extra binding, and that is stated in the
theorem rather than hidden: `[t] == eval(a)` and every other name
(`y != t, y != x`) is unchanged.

`naive_renderer_is_the_bug` keeps the disagreement as an obligation: the
double-emitting renderer performs 2 evaluations where bash performs 1, with
the same value — the exact cost the pass removes. Mutant, measured: binding
the temp to `0` instead of the subject reports **7 verified, 2 errors**.

Scope, stated in-file: freshness of `__sh2_dor_<n>` is a HYPOTHESIS — the
numbering makes a collision unlikely, but a user variable with exactly that
name would collide and the pass does not check. The `Declare`-init and
leaf-subject refusals are pinned by predicate/test, not re-derived per node.


## inline_pure_fns.rs — substitution, and the check it does not do (15)

`transforms/inline_pure_fns.rs` (parked offer: not in `transforms::all()`).
A call to a single-expression pure leaf is replaced by the body with
`getVar("N")` swapped for argument N. Proved (`inline_preserves`): value and
effect count match the call under the pass's purity check PLUS a hypothesis
the pass does not check — that no argument reads a name the body writes.

`capture_witness` keeps the hazard: body `x=5; getVar(0)` called with `$x`
returns the OLD value in bash (arguments are evaluated before the body) and
the NEW one after inlining; `double_effect_witness` keeps the impure-argument
double run (1 vs 2 evaluations). Mutants, measured: off-by-one substitution
**10 verified, 5 errors**; dropping the purity conjunct **14 verified,
1 error**. The positional-read model is one argument (a two-argument call is
the same argument repeated), stated in-file.

## cat_heredoc.rs — the bytes of a literal heredoc (13)

`transforms/cat_heredoc.rs`. `cat <<EOF` with a literal body is a pure
print; the pass rewrites it to `Output(body)`. `lift_matches_bash` proves the
emitted bytes equal bash's for every environment whenever the pass fires,
including the interpolating-but-expansion-free body
(`expansion_free_is_text`). The other guards are pinned as facts about
`lifts` (one redirect, fd 0, `heredoc` not `heredoc-tabs`, no operands), so
dropping one is red even though the bytes theorem cannot see it.
`expansion_guard_is_load_bearing` keeps the witness: `$x!` must not be
printed raw. Mutants, measured: drop the expansion guard **10 verified,
3 errors**; drop the mode check **12 verified, 1 error**. Pins mirror four of
the source's eight tests (quoted lift, unquoted-no-expansion lift, expansion
refusal, quoted `$5` text) plus the shape refusals.

## builtin_lift.rs — a carrier rename with a table guard (12)

`transforms/builtin-lift.rs`. `exec("cmd", args)` becomes
`builtin("cmd", args)` for the shared namespace table. Two obligations:
`lift_preserves` (the program's value is unchanged) and
`lift_reaches_every_position` (every expression position is reached, so no
spawn is left behind). Soundness depends exactly on the table check:
`lifting_an_off_table_name_is_wrong` shows an off-table name whose native op
differs, and the mutant that lifts everything is **9 verified, 3 errors**;
forgetting the `Nested` recursion arm is **11 verified, 1 error**
(completeness). Stated in-file: the runtime half — a table name's native op
matches the external binary — is the namespace contract, an assumption the
model does not discharge.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above (`--rlimit 40`).

# Verus proof of `optimize_stmts` Pass 0.5 (`fold_arith_ast`)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 optimize_fold.rs
    -> verification results:: 23 verified, 0 errors

## What is proved

`optimize_fold.rs` models the constant fold that Pass 0.5 runs on every
program: `fold_stmt` → `fold_expr` → `fold_arith_ast`, with the
`fold_bin_op` / `fold_un_op` / `pow_wrapping` tables. Three groups:

    proof fn fold_sound(a, s)
      ensures den(fold(a), s) == den(a, s)          // value preservation

    div_zero_refused / mod_zero_refused / min_div_minus1_refused /
    pow_negative_refused / unknown_op_refused       // the REFUSAL table

    pow_sq_eq_naive:      pow_sq(b,e) == pow(b,e)
    pow_wrapping_residue: pow_sq(b,e) % 2^64 == pow(b,e) % 2^64

The refusal theorems are the ones that matter for shipping: `fold_bin_op`
answers `None` exactly where bash raises a runtime error (zero divisor,
`i64::MIN / -1`, negative exponent) or where the op is not modelled, and
`fold_keeps_shape` shows `None` keeps the node, so the fold never invents
a value. Division is modelled truncating (`tdiv`/`tmod`), matching Rust
and bash rather than Verus's floor `/`; two pins cover `-7/2 = -3` and
`-7%2 = -1`.

`pow_wrapping`'s comment — "repeated squaring is the same product mod
2^64" — is now the pair of theorems above: the squaring loop equals
`vstd`'s `pow`, hence the residues agree mod 2^64. The remaining tie is
to Rust's `i64::wrapping_mul` state, which is exec-only; see Scope.

## It can fail (checked)

Replacing the `/` arm with unconditional `Some(tdiv(a, b))` makes the
same command report **19 verified, 4 errors** — `div_zero_refused`,
`min_div_minus1_refused` and the two corresponding pins go red. The file
was restored and re-checked at **23 verified, 0 errors**.

## Scope (read this before quoting it)

- It is a proof about a MODEL: the arms of `fold_bin` are transcribed by
  hand. `optimize_fold_sync.txt`
  is generated by `<workspace>/harness/fold-table-extract.sh` from `src/ir.rs` and
  carries every source arm's op/guard/body plus the model arm each op
  maps to, so a new arm, a changed guard or a dropped model arm fails
  the gate. That is drift detection, not generation: Rust's
  `wrapping_*` operations have no Verus spec form, so the table cannot
  be compiled from source into the model.
- **Domain.** The arms `+ - * ** << >>` are modelled as their
  mathematical values, so the *value* half is a no-overflow statement
  (shift counts in `0..64`). Outside that domain bash wraps and masks;
  the model does not encode that, and says so rather than proving it.
  The *refusal* half is not domain-limited.
- `& | ^` are opaque bit functions called identically on both sides.
- `Index`/`Assign`/`Cast` arith nodes are not modelled: the fold only
  recurses into them.
- `fold_arith_const` (the arithmetic string parser) is an oracle: the
  model starts from a parsed tree.
- The tables have no unit suite, so all pins are `UNMIRRORED` by design
  (the proof-campaign plan §(b).4).


## stray_reserved_word.rs — a stray `)` / `}` is a parse error (14)

`transforms/stray_reserved_word.rs`. bash treats a bare `)`/`}` in command
position as a grammar token, i.e. a parse error: the statements BEFORE it
have run, then it exits 2 and discards the tail. The parser recovers it as
an ordinary command, so every backend used to EXECUTE it — status 127, tail
still running — which is what the corpus gate saw
(`exit code (bash=2 perl=127)`).

Proved (`lower_matches_bash`, every program and every fuel):

    proof fn lower_matches_bash(p, fuel)
      ensures run_ir(lower(p), fuel) == run_bash(p, fuel)

where `run_bash` treats a stray as the abort and `run_ir` is the PRE-FIX
runtime (stray exec'd as a command, tail continues). `lower` replaces the
stray and everything after it with `Warn + Exit(2)`, keeping the prefix.
`gap_witness` states the disagreement for `echo; ); echo` (127 with both
traces vs status 2 with only the prefix) as an obligation of its own, so the
theorem is not "the pass agrees with the pass". `abort_keeps_token` /
`truncation_is_load_bearing` pin the diagnostic token and the one-element
abort.

Mutant, measured: keeping the tail after the abort is **12 verified,
2 errors** on those two truncation pins, while execution equivalence still
holds — an exit after the abort cannot run the tail either way. So
truncation is recorded as a code-size claim, not a status one.
Pins mirror the source's three tests (`pin_prefix_survives_and_exits_2`,
`pin_quoted_arg_untouched`, `pin_ordinary_command_untouched`) plus the `}`
token.

## merge_init_assignments.rs — the pair, as a whole-program theorem (15)

`transforms/merge_init_assignments.rs`. `elim_trio.rs` (c) already proved the
single step, but with `pure` and `reads_self` as PARAMETERS. This file makes
the guards structure of `lower` and quantifies over programs:

    proof fn lower_sound(p, st)
      ensures run(lower(p), st) == run(p, st)

The local fact is `pair_equals_merged`: both the pair and the merged
`Declare(x, e)` bind `x` to `eval(e, st)`, because the default is
overwritten before anything reads it and `e` (which does not read `x`)
evaluates the same before and after the default is installed
(`eval_ignores_own_binding`, `insert_insert`).

Both guards are load-bearing as theorems about the shipped rule: dropping
`!reads` is **14 verified, 1 error** (`lower_sound` fails on the witness
`x=5; Declare(x,0); x=x` → 0 unmerged vs 5 merged) and dropping the name
check is **13 verified, 2 errors**. The adjacency guard has its own witness
(`adjacency_guard_is_load_bearing`: an observation between the two statements
would print the default `0`, not `7`) and `non_adjacent_pair_is_refused`
proves the non-adjacent shape is left alone.

Scope: the pair shape only — indexed assignment targets, `asm`, and the
`pure` predicate's call/capture exclusions are absent from the model's
expression language rather than assumed; nested-body recursion is the same
local step at each depth.


## definedor_hoist.rs — one evaluation for `${a:-b}` (9)

`shir_passes/definedor_hoist.rs` (a `shir_passes/` transform, not a
`transforms/` module — the shIR passes had 13 of 26 modelled before this). `x = ${a:-b}` arrives as
`Assign(x, DefinedOr { expr: a, default: b })`, and a renderer with no
`DefinedOr` case emits the subject twice — once for the test, once for the
value — so `$(cmd)` or `$((i++))` runs twice. The pass binds it first:

    __sh2_dor_0 = a;
    x = ${__sh2_dor_0:-b};

Proved (`hoist_matches_bash`): the hoisted pair writes the same `x` as
bash's one-shot `${a:-b}` and performs the same number of evaluations
(`temp_test_agrees` is the hinge: the temp holds exactly the value the
test saw, so the second line's `:-` decision agrees with the first line's
read). The temp is the only extra binding, and that is stated in the
theorem rather than hidden: `[t] == eval(a)` and every other name
(`y != t, y != x`) is unchanged.

`naive_renderer_is_the_bug` keeps the disagreement as an obligation: the
double-emitting renderer performs 2 evaluations where bash performs 1, with
the same value — the exact cost the pass removes. Mutant, measured: binding
the temp to `0` instead of the subject reports **7 verified, 2 errors**.

Scope, stated in-file: freshness of `__sh2_dor_<n>` is a HYPOTHESIS — the
numbering makes a collision unlikely, but a user variable with exactly that
name would collide and the pass does not check. The `Declare`-init and
leaf-subject refusals are pinned by predicate/test, not re-derived per node.


## inline_pure_fns.rs — substitution, and the check it does not do (15)

`transforms/inline_pure_fns.rs` (parked offer: not in `transforms::all()`).
A call to a single-expression pure leaf is replaced by the body with
`getVar("N")` swapped for argument N. Proved (`inline_preserves`): value and
effect count match the call under the pass's purity check PLUS a hypothesis
the pass does not check — that no argument reads a name the body writes.

`capture_witness` keeps the hazard: body `x=5; getVar(0)` called with `$x`
returns the OLD value in bash (arguments are evaluated before the body) and
the NEW one after inlining; `double_effect_witness` keeps the impure-argument
double run (1 vs 2 evaluations). Mutants, measured: off-by-one substitution
**10 verified, 5 errors**; dropping the purity conjunct **14 verified,
1 error**. The positional-read model is one argument (a two-argument call is
the same argument repeated), stated in-file.

## cat_heredoc.rs — the bytes of a literal heredoc (13)

`transforms/cat_heredoc.rs`. `cat <<EOF` with a literal body is a pure
print; the pass rewrites it to `Output(body)`. `lift_matches_bash` proves the
emitted bytes equal bash's for every environment whenever the pass fires,
including the interpolating-but-expansion-free body
(`expansion_free_is_text`). The other guards are pinned as facts about
`lifts` (one redirect, fd 0, `heredoc` not `heredoc-tabs`, no operands), so
dropping one is red even though the bytes theorem cannot see it.
`expansion_guard_is_load_bearing` keeps the witness: `$x!` must not be
printed raw. Mutants, measured: drop the expansion guard **10 verified,
3 errors**; drop the mode check **12 verified, 1 error**. Pins mirror four of
the source's eight tests (quoted lift, unquoted-no-expansion lift, expansion
refusal, quoted `$5` text) plus the shape refusals.

## builtin_lift.rs — a carrier rename with a table guard (12)

`transforms/builtin-lift.rs`. `exec("cmd", args)` becomes
`builtin("cmd", args)` for the shared namespace table. Two obligations:
`lift_preserves` (the program's value is unchanged) and
`lift_reaches_every_position` (every expression position is reached, so no
spawn is left behind). Soundness depends exactly on the table check:
`lifting_an_off_table_name_is_wrong` shows an off-table name whose native op
differs, and the mutant that lifts everything is **9 verified, 3 errors**;
forgetting the `Nested` recursion arm is **11 verified, 1 error**
(completeness). Stated in-file: the runtime half — a table name's native op
matches the external binary — is the namespace contract, an assumption the
model does not discharge.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## Running it

Same toolchain as above (`--rlimit 40`).

# Verus proof of test_lowering at STATEMENT level (`test_lowering_stmt`)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 test_lowering_stmt.rs
    -> verification results:: 18 verified, 0 errors

## What is proved

The shipped `test_lowering.rs` model proves the LEAF half: each lowered
test leaf has the same boolean value as the source text. `test_lowering`
is the hottest pass in the corpus (42/563 examples) and it rewrites
conditions inside `If`, `While`, `elsif` and statement-position `&&`/
`||` chains, so the pass's contract is about the RUN. This file is the
statement-level rung, using the same recipe `const_status_cond.rs` uses
for `$?`:

    proof fn run_preserved(s, st, fuel)     // requires leaf_ok()
      ensures run(lower_s(s), st, fuel) == run(s, st, fuel)

    proof fn refusal_keeps_the_test(s)      // a `$?`-reading chain is
      ensures the condition is unchanged     // never lowered

`run` returns `(store, status, trace)` and `cstat` is bash's branch rule
(`&&`/`||` yield the deciding side's status). `leaf_ok()` is a
store-polymorphic hypothesis over two SEPARATE oracles
(`leaf_value` vs `lowered_value`), so the theorem is falsifiable: if the
leaf rewrite were wrong, the hypothesis would be false and the theorem
would not apply. Discharging `leaf_ok()` is `test_lowering.rs`'s job.

## It can fail (checked)

Dropping the `reads_status` guard (always lowering) makes the same
command report **15 verified, 3 errors** — `refusal_keeps_the_test`
fails, so the `$?`-observability refusal is load-bearing. Restored and
re-checked at **18 verified, 0 errors**.

## Scope (read this before quoting it)

- The leaf table is a HYPOTHESIS here, not re-proved; the run theorem is
  parametric in it. That is the point of the middle rung.
- `$?` observability is a per-chain predicate (`reads_status`), not a
  re-derivation of `walk_lastexit_liveness` + the cond-position rule;
  proving the liveness walk is Tier-3.
- `While` is fuel-indexed: the theorem says the rewrite does not change
  a run at any fuel bound (conditions are equal, so trip counts are
  equal), not that any loop terminates.
- Statements without conditions (Redirect/Pipeline/Try/For) only
  recurse; the Seq/If/While induction covers their shape.
- No unit suite for the model; pins are `UNMIRRORED`, and the source
  tests named there exercise the refusal side, not the run theorem.


## stray_reserved_word.rs — a stray `)` / `}` is a parse error (14)

`transforms/stray_reserved_word.rs`. bash treats a bare `)`/`}` in command
position as a grammar token, i.e. a parse error: the statements BEFORE it
have run, then it exits 2 and discards the tail. The parser recovers it as
an ordinary command, so every backend used to EXECUTE it — status 127, tail
still running — which is what the corpus gate saw
(`exit code (bash=2 perl=127)`).

Proved (`lower_matches_bash`, every program and every fuel):

    proof fn lower_matches_bash(p, fuel)
      ensures run_ir(lower(p), fuel) == run_bash(p, fuel)

where `run_bash` treats a stray as the abort and `run_ir` is the PRE-FIX
runtime (stray exec'd as a command, tail continues). `lower` replaces the
stray and everything after it with `Warn + Exit(2)`, keeping the prefix.
`gap_witness` states the disagreement for `echo; ); echo` (127 with both
traces vs status 2 with only the prefix) as an obligation of its own, so the
theorem is not "the pass agrees with the pass". `abort_keeps_token` /
`truncation_is_load_bearing` pin the diagnostic token and the one-element
abort.

Mutant, measured: keeping the tail after the abort is **12 verified,
2 errors** on those two truncation pins, while execution equivalence still
holds — an exit after the abort cannot run the tail either way. So
truncation is recorded as a code-size claim, not a status one.
Pins mirror the source's three tests (`pin_prefix_survives_and_exits_2`,
`pin_quoted_arg_untouched`, `pin_ordinary_command_untouched`) plus the `}`
token.

## merge_init_assignments.rs — the pair, as a whole-program theorem (15)

`transforms/merge_init_assignments.rs`. `elim_trio.rs` (c) already proved the
single step, but with `pure` and `reads_self` as PARAMETERS. This file makes
the guards structure of `lower` and quantifies over programs:

    proof fn lower_sound(p, st)
      ensures run(lower(p), st) == run(p, st)

The local fact is `pair_equals_merged`: both the pair and the merged
`Declare(x, e)` bind `x` to `eval(e, st)`, because the default is
overwritten before anything reads it and `e` (which does not read `x`)
evaluates the same before and after the default is installed
(`eval_ignores_own_binding`, `insert_insert`).

Both guards are load-bearing as theorems about the shipped rule: dropping
`!reads` is **14 verified, 1 error** (`lower_sound` fails on the witness
`x=5; Declare(x,0); x=x` → 0 unmerged vs 5 merged) and dropping the name
check is **13 verified, 2 errors**. The adjacency guard has its own witness
(`adjacency_guard_is_load_bearing`: an observation between the two statements
would print the default `0`, not `7`) and `non_adjacent_pair_is_refused`
proves the non-adjacent shape is left alone.

Scope: the pair shape only — indexed assignment targets, `asm`, and the
`pure` predicate's call/capture exclusions are absent from the model's
expression language rather than assumed; nested-body recursion is the same
local step at each depth.


## definedor_hoist.rs — one evaluation for `${a:-b}` (9)

`shir_passes/definedor_hoist.rs` (a `shir_passes/` transform, not a
`transforms/` module — the shIR passes had 13 of 26 modelled before this). `x = ${a:-b}` arrives as
`Assign(x, DefinedOr { expr: a, default: b })`, and a renderer with no
`DefinedOr` case emits the subject twice — once for the test, once for the
value — so `$(cmd)` or `$((i++))` runs twice. The pass binds it first:

    __sh2_dor_0 = a;
    x = ${__sh2_dor_0:-b};

Proved (`hoist_matches_bash`): the hoisted pair writes the same `x` as
bash's one-shot `${a:-b}` and performs the same number of evaluations
(`temp_test_agrees` is the hinge: the temp holds exactly the value the
test saw, so the second line's `:-` decision agrees with the first line's
read). The temp is the only extra binding, and that is stated in the
theorem rather than hidden: `[t] == eval(a)` and every other name
(`y != t, y != x`) is unchanged.

`naive_renderer_is_the_bug` keeps the disagreement as an obligation: the
double-emitting renderer performs 2 evaluations where bash performs 1, with
the same value — the exact cost the pass removes. Mutant, measured: binding
the temp to `0` instead of the subject reports **7 verified, 2 errors**.

Scope, stated in-file: freshness of `__sh2_dor_<n>` is a HYPOTHESIS — the
numbering makes a collision unlikely, but a user variable with exactly that
name would collide and the pass does not check. The `Declare`-init and
leaf-subject refusals are pinned by predicate/test, not re-derived per node.


## inline_pure_fns.rs — substitution, and the check it does not do (15)

`transforms/inline_pure_fns.rs` (parked offer: not in `transforms::all()`).
A call to a single-expression pure leaf is replaced by the body with
`getVar("N")` swapped for argument N. Proved (`inline_preserves`): value and
effect count match the call under the pass's purity check PLUS a hypothesis
the pass does not check — that no argument reads a name the body writes.

`capture_witness` keeps the hazard: body `x=5; getVar(0)` called with `$x`
returns the OLD value in bash (arguments are evaluated before the body) and
the NEW one after inlining; `double_effect_witness` keeps the impure-argument
double run (1 vs 2 evaluations). Mutants, measured: off-by-one substitution
**10 verified, 5 errors**; dropping the purity conjunct **14 verified,
1 error**. The positional-read model is one argument (a two-argument call is
the same argument repeated), stated in-file.

## cat_heredoc.rs — the bytes of a literal heredoc (13)

`transforms/cat_heredoc.rs`. `cat <<EOF` with a literal body is a pure
print; the pass rewrites it to `Output(body)`. `lift_matches_bash` proves the
emitted bytes equal bash's for every environment whenever the pass fires,
including the interpolating-but-expansion-free body
(`expansion_free_is_text`). The other guards are pinned as facts about
`lifts` (one redirect, fd 0, `heredoc` not `heredoc-tabs`, no operands), so
dropping one is red even though the bytes theorem cannot see it.
`expansion_guard_is_load_bearing` keeps the witness: `$x!` must not be
printed raw. Mutants, measured: drop the expansion guard **10 verified,
3 errors**; drop the mode check **12 verified, 1 error**. Pins mirror four of
the source's eight tests (quoted lift, unquoted-no-expansion lift, expansion
refusal, quoted `$5` text) plus the shape refusals.

## builtin_lift.rs — a carrier rename with a table guard (12)

`transforms/builtin-lift.rs`. `exec("cmd", args)` becomes
`builtin("cmd", args)` for the shared namespace table. Two obligations:
`lift_preserves` (the program's value is unchanged) and
`lift_reaches_every_position` (every expression position is reached, so no
spawn is left behind). Soundness depends exactly on the table check:
`lifting_an_off_table_name_is_wrong` shows an off-table name whose native op
differs, and the mutant that lifts everything is **9 verified, 3 errors**;
forgetting the `Nested` recursion arm is **11 verified, 1 error**
(completeness). Stated in-file: the runtime half — a table name's native op
matches the external binary — is the namespace contract, an assumption the
model does not discharge.

## typeset_attrs.rs — live attributes and the value bash stores (29)

`transforms/typeset_attrs.rs`. `typeset -i n` / `-l s` are not one-shot: the
attribute is LIVE, so every later assignment to that name is coerced. The
model carries the attribute state machine (`scan` / `attrs_at`, order-aware)
and proves the REWRITE LAW:

- `rewrite_matches_bash` — for every attribute and every literal text, the
  value the pass's expression produces equals the value bash stores. Discharged
  by cases: canonical decimal folds to itself and bash's arithmetic agrees
  (`int(arith(T))` otherwise, both reading a non-numeric word as 0), case
  folding for `-l`/`-u`, and the `-il` composition where bash coerces to the
  integer FIRST and folds the stored decimal (`fold_sees_the_coerced_value`).
- `octal_fold_is_wrong` — `042` is 34 in bash arithmetic; a naive leading-zero
  fold claims 42, so the pass's refusal is load-bearing (the model of the
  refusal, `strict_dec`, is what makes the law true).
- guards pinned: `-lu` recorded as nothing while either single fold would
  change the value, `typeset -r` recording nothing, other names untouched, an
  assignment BEFORE the declaration untouched, every declared name recorded.

Hypotheses kept in-file: the backends' `arith` node implements bash's
arithmetic for the full language, and `lc`/`uc` match bash per locale — the
model only folds ASCII, exactly the pass's guard. Text is modelled as
character codes; variables inside the arithmetic text are left to `arith`.
The source's four state-machine tests are mirrored as `pin_*` proofs.

Mutants, measured: order-ignoring `attrs_at` **28 verified, 1 error**; `-il`
folded the wrong way **28 verified, 1 error**; no-attribute coercion **27
verified, 2 errors**. Accepting a leading zero in `strict_dec` is caught one
stage earlier — `arith("042") == 34` simplifies to `42 == 34` in a
`compute_only` assert, so that mutant is rejected at simplification time and
has no verified/error count.

## capture_node_fold.rs — a carrier rename whose refusal is a bash fact (20)

`transforms/capture_node_fold.rs`. `Call{"capture",[Arrow]}` becomes the
first-class `Capture` node in value position; both render `$(…)`, but through
different backend arms (the Perl `Capture` arm is the one with the native
paths).

- `fold_preserves_output` — the carrier law: for every expression, the folded
  program's substitution bytes are unchanged. This is conditional on the two
  arms agreeing in every target; that carrier contract is stated in-file, not
  proved.
- `words_position_fold_would_glue` — the load-bearing refusal, and a bash
  fact: in WORD position bash splits a `$( )` result on IFS, so
  `basename $(pwd)` passes one argument per word, while the non-splitting node
  passes the whole output as ONE. `captureWords` is therefore never folded
  (2 arguments vs 1 for `a b`).
- `status_read_blocks_the_fold` — a body that reads `$?` keeps the call
  spelling. The model proves the guard is structural and total
  (`reads_status`), which is what lets the status-save arms keep the spelling
  they key on; it does not claim the fold would change bash's `$?`.
- `fold_leaves_no_foldable_call` — completeness: every value position is
  reached (nested, interpolated, both children), so no foldable call survives.
  `fold_preserves_guards` is the helper the surviving-call case needs.

Mutants, measured: fold the word spelling too **18 verified, 2 errors**; drop
the `$?` guard **18 verified, 2 errors**; drop the Arrow guard **18 verified,
2 errors**; miss the second child of a two-position node **17 verified,
3 errors**. Treating the non-splitting node as splitting is rejected at
simplification (`2 == 1` in a `compute_only` assert). The source's six tests
are mirrored as `pin_*` proofs.

## fbdev_gpu_elim.rs — a dead-on-the-corpus pass with a real law (21)

`transforms/fbdev_gpu_elim.rs`. After `fbdev-gpu` lifts writes to `/dev/fb0`
into `GpuOp` nodes, a `GpuOp` that is an EXACT, CONSECUTIVE duplicate of its
predecessor is dropped. The pass fires 0 times on the 552-example corpus, but
it is not dead code — it has a `transform`, 15 tests and a real law:

- `elim_preserves_visible` — the states the program OBSERVES (every `Read`
  plus the final state) are identical before and after the elimination. That
  is why dropping a write is allowed: nothing observes it.
- `non_consecutive_duplicate_is_load_bearing` — the naive alternative goes
  red: for `Op(1); Op(2); Op(1); Read` the read sees 1 under the pass and 2
  under "drop every write whose value was already written". The
  consecutiveness guard is what makes the law true.
- `elim_leaves_no_duplicates` + `pin_is_idempotent` — a second run changes
  nothing (`is_idempotent`).
- `Op(id)` uses one id for device+verb+slot+payload, so "different device /
  verb / slot / payload" are all "different id" (`pin_keeps_different_ids`,
  `duplicate_with_different_slot_is_kept`).

Hypotheses in-file: observers are `Read` statements plus the final state
(anything else that reads the device between two writes is exactly why only
consecutive duplicates are removed); the model is one flat list and the pass's
per-body recursion is the same theorem on a smaller list; registration order
(`after fbdev-gpu` and `-batch`) and the 0/552 corpus coverage are not
correctness claims.

Mutants, measured: naive dedupe **13 verified, 8 errors**; dedupe ignoring the
op identity **19 verified, 2 errors**; identity pass **17 verified, 4 errors**.

## frontend_capture_fold.rs — the stripping rule, not the walk (18)

`shir_passes/frontend_capture_fold.rs`. A frontend's A1 spells
`v = $(echo hello)` as `Call{"capture",[Arrow([echo hello])]}` (or
`Capture{native:false}`), which the shell pipeline would already have folded.
The pass replaces it with the literal.

- `fold_preserves_value` — the rewrite law: the folded program evaluates to
  the same bytes, for every expression and every environment.
- `newline_strip_is_load_bearing` — the bash fact that makes it true:
  command substitution STRIPS trailing newlines, so `$(echo hello)` is
  `hello`, not the printed `hello\n`. A fold emitting the printed bytes is a
  different value (and the mutant is red).
- `flag_refusal_is_load_bearing` — `echo -n x` prints no newline, so the
  plain-echo verdict cannot serve it; the pass refuses flagged bodies.
- `runtime_body_refusal_is_load_bearing` — `$(echo $x)` is runtime: two
  environments disagree, so no literal can replace the capture.
- `fold_leaves_no_foldable_capture` and `fold_is_idempotent` — completeness
  over every value position and fixpoint, which is what keeps an identity
  pass from satisfying the law.

Pins mirror the source's four tests (`a_constant_echo_capture_folds_to_its_output`,
`a_variable_word_keeps_the_capture`, `a_real_command_keeps_the_capture`,
`an_echo_with_a_flag_keeps_the_capture`) plus a nested-capture pin.

Hypotheses in-file: reachability/registration of the pass, the
`fold_const_echo` verdict itself (modelled as three outcomes rather than
re-derived), the `echo` stdout model (word + newline; `printf` formats,
multi-statement bodies, redirects and `&&`/`||` chains are refused by the
source and outside the model), and corpus domain.

Mutants, measured: identity **15 verified, 3 errors**; fold the printed bytes
**14 / 4**; fold `-n` bodies **17 / 1**; fold runtime words **15 / 3**.

## split_fold.rs — the licence, not the walk (41)

`shir_passes/split_fold.rs`. An unquoted expansion yields the value's words;
when the value is provably free of IFS whitespace the split produces exactly
one field, so `split[e]` is a no-op and the pass replaces it with `e`.

- `fold_preserves_fields` — the law: the expression observes the same word
  list (and carries the same text) after the fold, for every expression and
  every environment.
- `words_nonsep` — the tokenizer obligation: a whitespace-free NON-EMPTY value
  splits to exactly `[v]`.
- `empty_is_zero_words` — `split("")` is ZERO fields while the bare word is
  ONE empty field. The pre-fix rule folded on whitespace-freedom alone and
  collapsed the two (measured 2026-11-05); the witness reconstructs it and the
  mutant is red.
- `whitespace_is_load_bearing` — `split("a b")` is two fields, so the
  whitespace half is not decoration either.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint, so
  an identity pass cannot satisfy the law.
- `pin_custom_ifs_gate` — with `IFS=:` the whitespace-free `a:b` splits into
  two fields; the pass returns early on `program_may_custom_ifs`, which is why
  the model keeps the separator set as a parameter.
- `pin_element_carveout_keeps` — element stores into an array that is not
  already int-homed keep their split (conservative: it protects the C ingest's
  array homing, not the value).

Pins mirror the source's tests: `folds_int_typed_var_read`,
`keeps_unknown_var`, `keeps_whitespace_literal`, `folds_spaceless_literal`,
`nospace_is_not_nonempty`, `keeps_split_of_empty_literal`,
`folds_split_with_a_nonempty_part`, `custom_ifs_blocks_the_fold`,
`nested_split_and_subs`, `keeps_element_split_of_non_int_array`,
`folds_element_split_of_int_array`, and the funnel shapes.

Hypotheses in-file: reachability/registration of the pass; the verdicts it
consumes rather than computes (`analyze_var_nospace`'s bit — the `NSVar` node
with `nospace_env_ok`; the A2 int verdict — the `Int` node with
`int_env_ok`, canonical decimal being non-empty and whitespace-free); the
custom-IFS gate; the element carve-out; corpus domain.

Mutants, measured: drop the non-emptiness half **36 verified, 5 errors**; drop
the whitespace half **38 / 3**; identity **33 / 8**; fold on either half
**34 / 7**.

## frontend_int_lift.rs — spelling vs node, and the pass's scope (33)

`shir_passes/frontend_int_lift.rs`. A frontend spells a number as the string
`"42"` and records only the A2 verdict `var_types[name] = Int`; renderers read
the shell convention, so the pass rewrites the spelling to the node.

- `fold_preserves_text` — the value law: the text the expression denotes is
  unchanged.
- `lift_preserves_text` — the scope law: `lift_stmt` rewrites only a
  single-target `Assign` (or singleton `Declare`) of a name that is A2-`Int`
  AND written exactly once; every other statement is not walked, so a
  foldable value inside it survives (pinned).
- `decimal_spelling_is_canonical` — the parser's answer is the canonical
  spelling, so the replacement keeps the text. The permissive variant that
  would also accept `"07"` is a red mutant.
- `checked_is_exact` — `arith_eval` accepts only in-i64 intermediates, and an
  accepted value IS the mathematical value; the wrapping mutant writes a
  different integer for `i64::MAX + 1`.
- `fold_leaves_nothing` / `fold_is_idempotent` — completeness and fixpoint; an
  identity pass is red.

Pins mirror the source's tests: `an_int_typed_literal_becomes_an_int_node`,
`an_untyped_literal_is_untouched`, `a_literal_arithmetic_expression_folds_exactly`,
`overflow_refuses`, `a_float_spelling_is_not_an_int`, plus the scope and
whole-list pins.

Hypotheses in-file: reachability/registration of the pass; the A2 int verdict
(`frontend_types::int_names`) and the `count_writes` census, both taken as
given; the parser's canonicity (`int_literal` refuses signs, floats, `0x`,
underscores and leading zeros, and range-checks i64); corpus domain.

Mutants, measured: permissive parser **31 verified, 2 errors**; wrapping
arithmetic **28 / 5**; folding a runtime-valued tree **31 / 2**; identity
**27 / 6**.

## scoped_hoist.rs — a single-assignment local lives in a `let`, not a cell (21)

`shir_passes/scoped_hoist.rs` decides where a main-scope name's VALUE is stored.
When the name is written once, the write precedes every read, every read is in
the main scope, and at least one of them is a condition read the IR carries, the
value lives in a main-scope `let`; otherwise it stays in the renderer's global
cell and every read goes through `getVar` (`__SHV_<name>`).

The file models the two homes over an event sequence:

```verus
Ev = TopWrite | SelfWrite | SecondWrite | ReadMain | CondReadMain
   | WordCondMain | ReadOther | WordReadOther | EnvRead | Dynamic
Home = Global | Local
```

`go(evs, home, bind, cell, local)` is the observable projection: the bytes a
program with that event sequence prints. The census (`is_main_read`,
`only_main_reads`, `any_cond_ir`, `any_ir`, `first_is_top`, `tail`) is computed
FROM the sequence, and `hoist(evs)` is `Local` exactly when the census licenses
it. The law is hypothesis-free:

```verus
go(evs, hoist(evs), bindable(evs), empty(), empty())
  == go(evs, Home::Global, bindable(evs), empty(), empty())
```

for every `evs: Seq<Ev>`. `bindable(evs)` is not a side condition on the
program: it is the second parameter of the theorem — the renderers' binding
census only sees IR reads (`Var`/`getVar`), so a program whose only "read" is a
word text (`$s` inside `test -n "$s"`) gets no `let`. The law covers that shape
too, in the `bind = false` instantiation, instead of asserting a fact the
renderer does not provide.

The content is the census, and every arm is pinned by a witness whose two homes
print different bytes, then measured red as a mutant of the model:

| mutant | result | obligations that go red |
|---|---|---|
| identity (`hoist` always `Global`) | 17 verified, 4 errors | `hoist_preserves_output`, `a_conditional_reader_is_hoisted` (x3), `eligible_implies_hoisted`, `word_read_beside_ir_read_is_hoisted` |
| a read in ANOTHER scope counts as main | 19 / 2 | `main_reads_agree`, `law_holds_at_every_witness` |
| an env read counts as main | 19 / 2 | `main_reads_agree`, `law_holds_at_every_witness` |
| a second write counts as a read | 19 / 2 | `main_reads_agree`, `law_holds_at_every_witness` |
| a dynamic store counts as a read | 19 / 2 | `main_reads_agree`, `law_holds_at_every_witness` |
| no condition-read requirement | 16 / 5 | `hoist_preserves_output`, `a_plain_reader_is_not_hoisted`, `word_only_condition_read_is_load_bearing`, `permissive_hoist_is_not_a_law`, `law_holds_at_every_witness` |
| no first-mention requirement | 17 / 4 | `hoist_preserves_output`, `read_before_write_is_load_bearing`, `self_reference_is_load_bearing`, `law_holds_at_every_witness` |
| a self-referential RHS may hoist | 18 / 3 | `hoist_preserves_output`, `self_reference_is_load_bearing`, `law_holds_at_every_witness` |

No mutant fails with `E0xxx` (no type or borrow error stands in for a wrong
proof): every failure is `assertion failed` or `postcondition not satisfied`,
attributed to the named obligation above. `law_holds_at_every_witness` is the
law instantiated at each refusal shape with no lemma in between, so a widened
census is refuted by the law itself and not only by the agreement lemma it
consumes.

Completeness obligations, stated separately so an identity pass cannot satisfy
the law: `a_conditional_reader_is_hoisted` (an eligible program IS hoisted),
`eligible_implies_hoisted`, `a_plain_reader_is_not_hoisted` (a reader with no
condition read is NOT), and `word_read_beside_ir_read_is_hoisted`.

The refusal pins, each carrying the two traces:
`read_before_write_is_load_bearing` (a condition read before the assignment
prints nothing under `Global` and the unbound marker under `Local`),
`self_reference_is_load_bearing` (`s=$s` reads the cell before it is written),
`nested_read_is_load_bearing`, `env_read_is_load_bearing`,
`second_write_is_load_bearing`, `dynamic_store_is_load_bearing`,
`word_only_condition_read_is_load_bearing` and
`permissive_hoist_is_not_a_law` (both holding the PRE-REPAIR behaviour).

Pins mirror the source's tests, 12 of which pass.

### The repair the model forced

The pass used to accept a word-text-only read (`$s` inside `test -n "$s"`) as
satisfying `read_in_cond`. No renderer's binding census sees a word text, so the
emitted Rust had no `let` and stopped compiling (`E0425: cannot find value s`);
the C-shaped double declaration (`char* s = NULL; char* s = W;`) is the same
defect. `word_text_read()` now sets `word_read_in_cond` only, and the `Str` arm
of `walk_expr` uses it, so the pass refuses rather than guessing. The gap
itself is in the RENDERERS (their census ignores word texts), which is out of
this rung; the model states the refusal as a theorem (`bindable` as a
parameter) instead of hiding it in an antecedent.

### Execution anchors

Real generated Rust vs real bash, `cmp` on stdout, `W` set to `""` and `x`:
12 of 12 comparisons SAME — `s="$W"` with an IR condition read
(`let s = std::env::var("W")...`), a second store, a read before the
assignment, a read from a function body (`__SHV_s`), a self-referential RHS,
and an uppercase `HOME` name. The boundary case — `test -n "$s"` as the ONLY
read — transpiles to a program rustc rejects with exactly `E0425` and no
duplicate declaration, which is the pre-existing renderer gap the pass now
refuses; the anchor records it rather than claiming a pass.

Hypotheses in-file: registration of the pass (it runs inside `frontend_ingress`
for every non-estree target, including `.shir` input); the renderers' binding
census (`compute_local_vars` sees IR reads only); the renderer's global-cell
contract (a `Global` read of an uninitialised name is bash's empty string, and
the cell survives the top-level store). None of these is an antecedent of the
law.

## split_inplace.rs — the buffer-reuse flag preserves the output (29)

`shir_passes/split_inplace.rs` rewrites `for w in $X; do …; done` from the
legacy `Call{"split", [Var X]}` iterator to `Ext(Split{in_place: true})`, which
licenses the C renderer to tokenize the variable's OWN buffer (stamping the
delimiters with NUL) instead of copying it into a scratch.

The file models the two homes over an event sequence and proves they print the
same bytes:

```verus
go(evs, inplace_ok(evs), w0(), delim(), false, false, 0)
  == go(evs, false, w0(), delim(), false, false, 0)
```

for every `evs: Seq<Ev>`, where

```verus
Ev = SplitLoop | LoopReads | LoopWrites | ReadX | WriteX | ReadWord
   | WildX | ExportX | FnX | Allexport | Other
```

`inplace_ok` is computed FROM the sequence (`has_loop`, `has_write`,
`!has_hazard`, `!has_export`, `!has_fn`, `!has_allexport`, `suffix_ok(post)`),
so the law has no antecedent on the shape: a refused program takes both sides
with the flag `false`, and an accepted one earns the equality from the census.

What the flag MEANS is the C renderer's contract, modelled as three facts:
the loop prints the SAME words in both homes (in-place tokenization keeps the
field boundaries); the buffer's BYTES change, so a later read, an export or a
function mention sees `[1,0,2]` instead of `[1,32,2]`; and the word array
points INTO that buffer, so a store after the loop destroys the words bash
keeps as copies, while a store inside the loop frees the block under the
iterator (a `fault` marker in the trace).

### The two defects the model forced

1. **The rewrite deleted the loop.** The C `for` lowering matched only
   `IrExpr::Call{func=="split"}` and `render_ext_expr` returns `None` for
   `Split` on C, so `Ext(Split)` fell through to the generic array path:
   `for w in "$1"` emitted `/* TODO(unsupported): for item type */` and printed
   NOTHING (bash: `a|b|c`). The arm now handles `Ext(Split)` — it splits the
   variable's own heap buffer when `in_place` is set and keeps the copying form
   otherwise.
2. **A later store outlived the words.** The census allowed a store after the
   loop because "no later read of the source exists", but the loop variable
   points into the source buffer: `y=$1; for w in $y; …; done; y=<longer>;
   echo "$w"` printed a stale byte (`last=\x05`) instead of `c`. A later store
   is now only eligible when no later statement reads the loop variable (a
   function body that mentions it counts). Measured both ways, including the
   shorter-store `memmove` path.

A third rule came out of the same reading: a source the program never stores
is refused, because `_sh_sset` is what gives a variable a heap block — an
unstored name is `getenv`/`argv` storage, which this pass must not stamp.

### Mutants — each a real edit, each red, no `E0xxx`

| mutant | result | obligations that go red |
|---|---|---|
| identity (`inplace_ok` false) | 27 verified, 2 errors | `an_eligible_loop_is_flagged`, `a_word_read_without_a_later_store_is_fine` |
| allow a loop body that touches the source | 26 / 3 | `split_in_place_preserves_output` (precondition), `body_read_is_load_bearing`, `body_write_is_load_bearing` |
| allow a later read of the source | 26 / 3 | `suffix_agrees`, `law_at_the_refusal_witnesses`, `later_read_is_load_bearing` |
| drop the store-after-loop rule | 26 / 3 | `suffix_agrees`, `law_at_the_refusal_witnesses`, `a_store_that_outlives_a_word_read_is_load_bearing` |
| allow an export | 26 / 3 | `split_in_place_preserves_output` (precondition), `law_at_the_refusal_witnesses`, `export_is_load_bearing` |
| allow a function mention | 26 / 3 | `split_in_place_preserves_output` (precondition), `law_at_the_refusal_witnesses`, `fn_mention_is_load_bearing` |
| drop the buffer-ownership rule | 28 / 1 | `an_unstored_source_is_refused` |
| flag everything | 17 / 12 | the law (precondition), `law_at_the_refusal_witnesses`, `a_hazardous_loop_is_not_flagged`, `an_unstored_source_is_refused`, `allexport_is_refused`, and all seven two-trace pins |

Every failure is `assertion failed`, `precondition not satisfied` or
`postcondition not satisfied`; `grep -lE '^error\[E0[0-9]+\]'` over the eight
logs finds nothing. `law_at_the_refusal_witnesses` is the law instantiated at
each refusal shape with no lemma in between, so a widened census is refuted by
the law itself, not only by the agreement lemma it consumes.

### Completeness

`an_eligible_loop_is_flagged` (a clean loop off a stored source IS flagged, so
an identity pass is red), `a_hazardous_loop_is_not_flagged`,
`an_unstored_source_is_refused`, `allexport_is_refused` (labelled
conservative: the model cannot make `allexport` load-bearing, so it is stated
as a refusal, not claimed as necessary), `a_word_read_without_a_later_store_is_fine`
(the rule refuses the PAIR, not the read), and `law_at_the_simplest_witness`.

### Execution anchors

Real generated C vs real bash, `cmp` on stdout, `MALLOC_PERTURB_=170` so a
use-after-free shows up: **50 of 50 comparisons SAME, 0 diffs, 0 build
failures** — 10 programs × 5 arguments (`a b c`, `one`, empty,
`  a  b `, `a<TAB>b`). The programs cover the eligible shape (in-place
lowering present in the generated C), a store after the loop, a body read, a
second store that outlives a word read (both the grow and the `memmove`
paths), an export, a function mention, a loop variable equal to the source, an
unstored source (`$HOME`), and a defining-only function. Before the repairs
the first printed nothing (`TODO(unsupported)`) and the last two printed a
stale byte.

Hypotheses in-file: the pass's registration and its C-only gating; the C
storage contract (`_sh_sset` gives every string variable a `_sh_xstrdup`/in-
place heap block, so a program-stored name's buffer is writable and owned);
`_sh_split`'s tokenization (`IFS` delimiters stamped with NUL, word pointers
into the buffer); and that a `Str` literal source never reaches the flag (the
pass matches `Var`/`getVar` only). None of these is an antecedent of the law.

## string_accumulator.rs — the chunk-array lowering preserves the output (22)

`transforms/string_accumulator.rs` recognizes `v="$v$seg"` in a loop whose only
writes to `v` are such self-appends, and (for the JS/ESTree renderer) rewrites
it to a chunk array: `setArray` seeds it after the init, each append becomes
`setArrayAppend`, and ONE `concatItems` materializes it right after the loop.
The C renderer keeps the analysis and uses it for an `_alen_` length mirror.

The file models the two strategies over a statement trace and proves they emit
the same bytes:

```verus
go(evs, acc_ok(evs)) == go(evs, false)
```

for every `evs: Seq<Ev>`, where

```verus
Ev = Init(s) | InitOther(s) | Append(s) | AppendOther(s)
   | LateWrite(s) | Read | Join
```

`eager` re-concatenates on every append; `chunked` DEFERS — it extends the
chunk array's joined value and bumps the push count, leaving `v` stale — and
`Join` sets `v` to the deferred concatenation. The observable is the list of
values every read observed, in order.

`acc_ok` is computed FROM the trace: at least one append, no non-literal init
(no text to seed with), no append the lowering cannot rewrite, and no read
taken while an append is outstanding. So a refused program takes both sides
with the flag `false`; an accepted one earns the equality from the deferred-
concatenation invariant `joined == eager_v`.

### The two defects the model forced

`body_reads_var_nonleading` exempted a self-append from the mid-loop-read check
whenever ANY target of the assignment named the variable — but `lower` only
rewrites an assignment with exactly one, non-indexed target. Two shapes were
therefore ACCEPTED and never rewritten:

1. a **multi-target** assignment `blk="$blk$x" w=…`: the append kept
   re-concatenating `blk` inside the loop, and the single post-loop
   `concatItems` then overwrote that work — the segment was lost;
2. an **indexed** target `blk[i]="$blk$x"`: it writes the ARRAY `blk`, which
   the join then clobbered.

The exemption now uses the lowering's exact predicate. Both tests
(`a_multi_target_self_append_refuses_the_verdict`,
`an_indexed_self_append_refuses_the_verdict`) failed before the fix.

### Mutants — each a real edit, each red, no `E0xxx`

| mutant | result | obligations that go red |
|---|---|---|
| identity (`acc_ok` false) | 20 verified, 2 errors | `an_accumulator_is_flagged`, `a_read_before_the_loop_is_fine` |
| allow a stale read | 19 / 3 | `agreement`, `a_stale_read_is_refused`, `law_at_the_refusal_witnesses` |
| allow an unrewritable append | 19 / 3 | the law (precondition), `an_unrewritable_append_is_refused`, `law_at_the_refusal_witnesses` |
| allow a non-literal init | 19 / 3 | the law (precondition), `a_nonliteral_init_is_refused`, `law_at_the_refusal_witnesses` |
| drop `has_append` | 21 / 1 | `an_init_only_trace_is_not_flagged` |
| flag everything | 16 / 6 | the law (precondition), the three refusals, `an_init_only_trace_is_not_flagged`, `law_at_the_refusal_witnesses` |
| drop the init seed | 18 / 4 | `agreement`, `the_chunks_really_accumulate`, `an_unrewritable_append_would_differ`, `the_join_is_load_bearing` |
| join by the stale value | 17 / 5 | `agreement`, `the_chunks_really_accumulate`, `an_unrewritable_append_would_differ`, `a_nonliteral_init_would_differ`, `the_join_is_load_bearing` |

### Completeness and non-vacuity

`an_accumulator_is_flagged` (an identity pass is red), `the_chunks_really_
accumulate` (two appends became two pushes and the read sees the whole
concatenation), `a_read_before_the_loop_is_fine` (a pre-loop read is not
stale), `a_stale_read_is_refused`, `an_unrewritable_append_is_refused`,
`a_nonliteral_init_is_refused`, `an_init_only_trace_is_not_flagged`, and the
element-level pins that exhibit where each refusal's two strategies diverge
(`a_stale_read_would_differ` reads `a` vs `a+x`, and so on).

### Anchors

Two, because the rewrite has two consumers:

- **Real execution (C mirror):** 11 programs, bash stdout vs the compiled
  generated C, `cmp`, `MALLOC_PERTURB_=170`: **11 of 11 SAME, 0 diffs, 0 build
  failures** (`cc … -lgmp`). Five of them have the `_alen_` mirror active
  (`SH2_DBG_ACC=1` reports "ACC tracked 1…2"), covering empty and non-empty
  inits, two accumulators, a zero-iteration loop and a conditional append.
- **The estree lowering (reference semantics):**
  `the_lowered_chunks_concatenate_to_the_eager_concatenation` runs `analyze`
  and `lower` on real IR and checks that the lowered chunk text (seed items +
  every `setArrayAppend`, in program order) equals `init ++ segs` for four
  shapes, and that exactly one `concatItems` materialization is emitted. No JS
  engine is installed on this machine, so the estree program was not executed
  end to end; the lowering's output is checked structurally and against the
  model instead.

Hypotheses in-file: the trigger census (`self_append_vars`, the
last-write-before-the-loop init, `reads_after_loop`) — i.e. that the trace is
complete; the segment's evaluation order (it stays in place); the renderers'
`setArray`/`concatItems` contract; and, for C, that every store funnel updates
the `_alen_` mirror. None is an antecedent of the law.

## Running it

Same toolchain as above (`--rlimit 40`).

## `fbdev_gpu_batch.rs` — the same-verb batch fold (36 obligations)

The transform merges a maximal run of consecutive `GpuOp`s that share
`device` + `verb` into one `GpuOp` whose payload is the run's payloads joined
by a single `\n`. The observable is the device **byte stream**: a dispatch
contributes `payload ++ "\n"` on its verb, an `Observe` contributes nothing.

The law is a fold over one flat op list:

- `run_bytes([d1..dk]) = payload_1 ++ "\n" ++ … ++ payload_k ++ "\n"`, and
  `joined(payloads) ++ "\n"` is the same sequence — `joined`
  (`the_join_preserves_the_run_bytes`) and `dispatched` (the byte-level fold)
  are the two halves, proved together.
- `go_bytes` lifts it over the pass: for EVERY list of ops, `bytes(batch(ops))
  == bytes(ops)` (`the_batch_preserves_the_bytes`), with `go`'s three arms
  (extend the run / flush and dispatch / flush and observe) each carrying its
  own obligation and the identity (`flush([]) = []`, a run of one is left
  alone) as lemmas.
- `a_merged_dispatch_keeps_the_runs_verb`: every byte of the run stays on the
  run's verb, so a join cannot send a payload to a call verb.

Pinned pre-fix / naive facts: `a_naive_merge_of_a_call_verb_is_a_different_
argument`, `the_pass_refuses_a_call_verb`, `an_empty_payload_still_contributes_
its_line` (a join that skips empty payloads loses a terminator),
`a_run_of_one_is_left_alone`, `the_wrong_separator_does_not_reproduce_the_
stream`, and the measured anchor witnesses (`law_at_the_witnesses`).

NOT claimed (stated in-file): a pass-level theorem that each `Observe` still
sees the same byte prefix. The pass moves no statement — it replaces a run in
place — so that property is structural; the `obs_*` lemmas that did verify say
a dispatch-only run contributes no observation and observations concatenate,
but the full induction did not close under this Verus's automation and is left
out rather than asserted.

### Mutants

`<workspace>/harness/mut-fbdev-batch.py` applies 11 independent edits inside the named
definitions: empty separator, doubled separator, skip an empty payload,
first-only, reversed, merged op adds a terminator, run key ignores the verb,
`starts_run` ignores the joinable set, `flush` drops a single op, the device
dispatch loses its terminator, `flush` drops the pending run. Measured:
**11/11 RED**, each naming a failing obligation, none an `E0xxx`.

### Anchor

`<workspace>/harness/anchor-fbdev-batch.sh` runs the emitted C for real against
`<workspace>/harness/fbdev-soft` (a deterministic software device: payload+`\n` to a file
or stdout; no X, no root, no framebuffer) and diffs its device bytes, stdout
and exit status against bash, which runs the same program with the device root
substituted to a FIFO-backed directory. Measured: **8/8** — 3 lifted shapes
(one `fb_*` call carrying the run's lines; a value join on `$1`/`$2`; an
embedded-newline payload), 3 refused shapes (a break, two verbs, a single
write), and 2 fall-throughs (no match, `SH2_TRANSFORMS=dead-store-elim`) on
both C and Perl. The Perl backend never lifts fb writes, which the
fall-through arm asserts.

### Running it

```sh
RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/fbdev_gpu_batch.rs          # 36 verified, 0 errors
python3 <workspace>/harness/mut-fbdev-batch.py          # 11/11 red
bash <workspace>/harness/anchor-fbdev-batch.sh          # pass=8 fail=0
```

## `fbdev_gpu.rs` — the device-write lift (17 obligations)

The transform lifts `echo PAYLOAD > /dev/fb0/VERB` into
`GpuOp{device, slot, verb, payload}`. A device write's observable is the pair
**(path, bytes)**: the path `DEVICE/VERB` (or `DEVICE/SLOT/VERB`), and the
bytes `payload ++ "\n"` — the echo line the device parses.

- `path_of` and `wire` are the two halves: `a_top_level_verb_is_device_slash_verb`
  and `a_slot_keeps_its_path` prove the path composition;
  `the_line_keeps_its_terminator` and `the_line_is_not_empty` prove the line.
- `the_lift_preserves_the_device_stream` is the driver law: for EVERY
  statement list, the lifted list's (path, bytes) sequence equals the
  original's (`go` vs `go_lifted`), with `the_lift_is_a_pointwise_image` as
  the per-statement form — so an op that forgets a slot, a verb or adds a
  terminator is red.

Pinned pre-fix / naive facts (each a measured witness):

| fact | measured defect |
|---|---|
| `the_naive_fallback_loses_the_line` | the shipped `fputs(payload)` fallback wrote `1` for `depthmask` where bash writes `1\n` |
| `an_empty_write_is_still_a_line` | `echo "" > dev` is the one-byte line `\n` |
| `an_empty_write_is_not_the_naive_fallback` | the pre-fix path dropped that line entirely |
| `a_naive_slot_join_moves_the_write` | always spelling `device/slot/verb` gives `device//verb` for a top-level verb |
| `a_printf_write_is_not_an_echo_write` | `printf 'x' > dev` writes `x` with no terminator |
| `an_echo_option_is_not_a_payload` | `echo -n > dev` writes nothing |

NOT MODELLED (stated in-file): the device's own parser (what a line means to
`fb_blocks`), whether `fopen` succeeds, and the lift's recursion into nested
bodies (a nested body is a statement list of the same shape, carried by the
same `go`). `printf` and `echo -n` appear only as REFUSED shapes.

### Repairs the model forced

1. The GpuOp **fallback** in C, Perl, Go and Zig now writes the echo
   terminator. Before: `fputs(payload)` / `print $fh payload` /
   `os.WriteFile(…, []byte(payload))` / `writeAll(payload)` — every lifted
   line on a verb with no `fb_*` primitive lost its last byte (default
   transform set: `echo "1" > /dev/fb0/depthmask` emitted `1`).
2. The lift refuses **printf** (both `printf 'x\n'` and `printf 'x'` were
   byte-DIFF: `x\n\n` and `x\n`).
3. The lift refuses a lone **echo option word**: it had been lifting `-n` as
   the payload.
4. An **empty payload** is lifted (it is one `\n` line), which closes the
   defect the batch rung measured.

### Mutants

`<workspace>/harness/mut-fbdev-gpu.py` — 12 independent edits inside the named
definitions (line terminator dropped/doubled, top-level path loses its slash,
slotted path collapses, lift adds a terminator / forgets the verb / forgets
the slot, stream reverses / drops the first statement, printf read as an
echo, echo option read as a payload, naive slot join). Measured: **12/12
RED**, each naming a failing obligation, none an `E0xxx`.

### Anchor

`<workspace>/harness/anchor-fbdev-gpu.sh` — **9/9**, really executed: 3 lifted through
the C call (`fb_blocks` marker, including the empty payload), 2 lifted
through the GpuOp fallback (`depthmask`, the slotted `uniform/1f/uCamYaw`;
`fputc('\n', _f)` marker), 3 refused (printf, `echo -n`, an interpolated
device path; no markers) and 1 transform-disabled fall-through. Device bytes,
stdout and exit status are compared against bash on a FIFO-backed device, on
C AND Perl.

### Running it

```sh
RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/fbdev_gpu.rs               # 17 verified, 0 errors
python3 <workspace>/harness/mut-fbdev-gpu.py          # 12/12 red
bash <workspace>/harness/anchor-fbdev-gpu.sh          # pass=9 fail=0
```

## `fbdev_gpu_calls.rs` — the call-emission rewrite (20 obligations)

The transform replaces a lifted `GpuOp` with the in-process call the same
program would make had it named the primitive directly:

| write | call |
|---|---|
| `echo P > /dev/fb0/blocks` | `fb_blocks(P)` |
| `echo P > /dev/fb0/overlay` | `fb_overlay(P)` |
| `echo P > /dev/fb0/overlay_flash` | `fb_overlay_flash(P)` |
| `echo swap > /dev/fb0/call` | `fb_present()` |

`fb_present` takes NO argument, so the present mapping is sound for exactly
the one literal payload the device names as a present; any other `call`
payload (e.g. `clear`) is refused because the argumentless call would DROP it.
This is the difference the batch rung could not state: the call table is
closed, and the present's line is fixed.

The laws:

- `a_single_payload_call_keeps_the_path` / `a_present_keeps_the_path` — the
  call addresses the write's own path (a slotted write sent to the bare verb
  would land on another path).
- `a_single_payload_call_keeps_the_line` / `a_present_keeps_the_line` — the
  device line is `payload ++ "\n"`, and the present's is the fixed
  `"swap" ++ "\n"`.
- `the_emitted_call_is_the_redirect_status` — an emitted call leaves status 0
  from any previous status. This is the statement-status law: the shell's
  device write is a redirect (status 0), so the call must set it; the pre-fix
  emitter left `_sh_rc` stale.
- `the_stream_of_calls_keeps_the_device_stream` — for EVERY op list, the
  emitted stream's lines equal the device-write stream's lines (`go` vs
  `go_calls`).

Refusals are obligations: `a_slotted_op_is_refused`,
`a_foreign_device_is_refused`, `an_unmapped_verb_is_refused`.

Pinned naive rules: `a_fallback_is_not_a_redirect_status` (the measured stale
status), `the_pre_fix_present_polarity_is_not_the_redirect_status`
(`ret - 1` at the device's actual 0 is -1), `a_call_payload_is_not_droppable`,
`a_slotted_write_lands_on_another_path`,
`an_unmapped_verb_is_not_a_device_call`, and the witness `law_at_the_witnesses`.

### The repair the model forced

The emitted `void` primitives carry no status, so the C backend left `_sh_rc`
**stale** after them: `false; echo x > /dev/fb0/blocks; echo $?` printed 1
where bash prints 0 (measured). The void call now stores the redirect's
status — `(_sh_rc = ({fb}, 0), _sh_rc == 0)` — and `fb_status("fb_present")`
maps a non-negative return to 0 (the pre-fix `ret - 1`, with
`runtime/fbdev.c` returning 0, produced **-1** for a successful present).

Out of model: the device's parser, the call's inability to fail, and the
payload-expression census. The FALLBACK's status is still stale — measured for
a plain file redirect too, so it is a pre-existing C-backend gap, reported
rather than modelled.

### Mutants

`<workspace>/harness/mut-fbdev-gpu-calls.py` — 12 independent edits inside the named
definitions: any call payload becomes a present, slotted ops/foreign
devices/unmapped verbs become calls, `blocks` unmapped, the present lands on
the wrong path, the wire call drops its terminator, the present writes
nothing, either call leaves the old status, the stream reverses, the pre-fix
polarity returns. Measured: **12/12 RED**, each naming an obligation, none an
`E0xxx`.

### Anchor

`<workspace>/harness/anchor-fbdev-gpu-calls.sh` — **10/10**, really executed on C and
Perl against `<workspace>/harness/fbdev-soft` (now declaring `fb_present()`, which writes
`swap\n` to the sink, as `runtime/fbdev.c` does): 3 wire-call shapes, 2
present shapes (`echo swap > …/call` → `fb_present()`, including after a
failing command with `echo "rc=$?"` → 0, matching bash), 3 refused (a
non-`swap` `call` payload, `depthmask`, a slotted write) and 1
transform-disabled fall-through. Device bytes, stdout and exit status are
compared against bash on a FIFO-backed device. Perl never lifts (C-target
gated) and is checked for exactly that.

### Running it

```sh
RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/fbdev_gpu_calls.rs        # 20 verified, 0 errors
python3 <workspace>/harness/mut-fbdev-gpu-calls.py    # 12/12 red
bash <workspace>/harness/anchor-fbdev-gpu-calls.sh    # pass=10 fail=0
```

## `fbdev_gpu_calls.rs` — the call-emission rewrite (20 obligations)

The transform replaces a lifted `GpuOp` with the in-process call the same
program would make had it named the primitive directly:

| write | call |
|---|---|
| `echo P > /dev/fb0/blocks` | `fb_blocks(P)` |
| `echo P > /dev/fb0/overlay` | `fb_overlay(P)` |
| `echo P > /dev/fb0/overlay_flash` | `fb_overlay_flash(P)` |
| `echo swap > /dev/fb0/call` | `fb_present()` |

`fb_present` takes NO argument, so the present mapping is sound for exactly
the one literal payload the device names as a present; any other `call`
payload (e.g. `clear`) is refused because the argumentless call would DROP it.

The laws:

- `a_single_payload_call_keeps_the_path` / `a_present_keeps_the_path` — the
  call addresses the write's own path (a slotted write sent to the bare verb
  would land on another path).
- `a_single_payload_call_keeps_the_line` / `a_present_keeps_the_line` — the
  device line is `payload ++ "\n"`, and the present's is the fixed
  `"swap" ++ "\n"`.
- `the_emitted_call_is_the_redirect_status` — an emitted call leaves status 0
  from any previous status. This is the statement-status law: the shell's
  device write is a redirect (status 0), so the call must set it; the pre-fix
  emitter left `_sh_rc` stale.
- `the_stream_of_calls_keeps_the_device_stream` — for EVERY op list, the
  emitted stream's lines equal the device-write stream's lines (`go` vs
  `go_calls`).

Refusals are obligations: `a_slotted_op_is_refused`,
`a_foreign_device_is_refused`, `an_unmapped_verb_is_refused`.

Pinned naive rules: `a_fallback_is_not_a_redirect_status` (the measured stale
status), `the_pre_fix_present_polarity_is_not_the_redirect_status`
(`ret - 1` at the device's actual 0 is -1), `a_call_payload_is_not_droppable`,
`a_slotted_write_lands_on_another_path`,
`an_unmapped_verb_is_not_a_device_call`, and the witness `law_at_the_witnesses`.

### The repair the model forced

The emitted `void` primitives carry no status, so the C backend left `_sh_rc`
**stale** after them: `false; echo x > /dev/fb0/blocks; echo $?` printed 1
where bash prints 0 (measured). The void call now stores the redirect's
status — `(_sh_rc = ({fb}, 0), _sh_rc == 0)` — and `fb_status("fb_present")`
maps a non-negative return to 0 (the pre-fix `ret - 1`, with
`runtime/fbdev.c` returning 0, produced **-1** for a successful present).

Out of model: the device's parser, the call's inability to fail, and the
payload-expression census. The FALLBACK's status is still stale — measured for
a plain file redirect too, so it is a pre-existing C-backend gap, reported
rather than modelled.

### Mutants

`<workspace>/harness/mut-fbdev-gpu-calls.py` — 12 independent edits inside the named
definitions: any call payload becomes a present, slotted ops/foreign
devices/unmapped verbs become calls, `blocks` unmapped, the present lands on
the wrong path, the wire call drops its terminator, the present writes
nothing, either call leaves the old status, the stream reverses, the pre-fix
polarity returns. Measured: **12/12 RED**, each naming an obligation, none an
`E0xxx`.

### Anchor

`<workspace>/harness/anchor-fbdev-gpu-calls.sh` — **10/10**, really executed on C and
Perl against `<workspace>/harness/fbdev-soft` (now declaring `fb_present()`, which writes
`swap\n` to the sink, as `runtime/fbdev.c` does): 3 wire-call shapes, 2
present shapes (`echo swap > …/call` → `fb_present()`, including after a
failing command with `echo "rc=$?"` → 0, matching bash), 3 refused (a
non-`swap` `call` payload, `depthmask`, a slotted write) and 1
transform-disabled fall-through. Device bytes, stdout and exit status are
compared against bash on a FIFO-backed device. Perl never lifts (C-target
gated) and is checked for exactly that.

### Running it

```sh
RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/fbdev_gpu_calls.rs        # 20 verified, 0 errors
python3 <workspace>/harness/mut-fbdev-gpu-calls.py    # 12/12 red
bash <workspace>/harness/anchor-fbdev-gpu-calls.sh    # pass=10 fail=0
```

## `shir_pipeline_native.rs` — the pipeline statement-shape rewrite (25 obligations)

A statement-position `pipeline(…)` call whose every stage is one
exec/builtin of a natively-emulable command becomes the structured
`IrStmt::Pipeline`, carrying the same stages:

| write | before | after |
|---|---|---|
| `cat f.txt \| wc -l` | `Expr(Call{pipeline, [Array(stages)]})` (one shell runner) | `Pipeline{stages}` (stages rendered in place) |

The law is the stage list plus the runner:

- `the_lowering_keeps_the_stages` (stronger reading: the sequence IS the
  source sequence), with `the_lowering_keeps_the_stage_count`,
  `the_lowering_keeps_a_stage_command`, `the_lowering_keeps_the_order` and
  `the_lowering_keeps_the_producer` as per-index readings the mutant sweep
  names.
- `the_lowering_removes_the_shell_runner` — the call form costs one
  `_sh_pipeline` runner, the statement form none; `the_call_form_costs_a_shell_runner`
  is the negative half.
- `the_lowering_is_the_identity_elsewhere` — non-pipeline statements and every
  refused pipeline are unchanged.

Refusals are obligations: `an_empty_pipeline_is_refused`,
`a_non_native_stage_is_refused`, `a_multi_statement_stage_is_refused`,
`a_refused_pipeline_keeps_the_shell_runner`.

The stream law `the_stream_is_preserved` holds over every statement list, and
`the_runners_left_are_the_refused_ones` says no accepted pipeline keeps a
runner.

Pinned wrong rules: `a_non_native_name_is_not_native` (`sed`),
`the_naive_every_pipeline_lowering_keeps_a_runner`,
`a_dropped_stage_is_a_lost_stage`, `reversed_stages_are_a_different_stream`,
and the witness `law_at_the_witnesses`.

### Scope: what the model does NOT claim

The commands' semantics (that the native `grep` emulation is grep) — that is
what the byte anchor measures, not the model. The Perl renderer still
reconstructs the pipeline text (`__sh_pipe_run(q{…})`) for some accepted
shapes, measured by the anchor and reported: the runner reduction is a C-side
property, a Perl renderer gap rather than a transform rule. The source's
first-stage re-check is redundant (the every-stage test implies it) — noted,
not modelled.

### Mutants

`<workspace>/harness/mut-shir-pipeline-native.py` — 13 independent edits inside the named
definitions: the table drops `grep`, the table admits everything, the
stage-shape test is ignored, empty pipelines are accepted, the last stage goes
unchecked, `lower` drops the first stage, `lower` reverses, `lower` keeps the
call form, `lower` rewrites non-pipeline statements, the statement form keeps a
runner, the call form costs none, `stages_of` loses the stages, `drop_first`
becomes the identity. Measured: **13/13 RED**, each naming an obligation, none
an `E0xxx`.

### Anchor

`<workspace>/harness/anchor-shir-pipeline-native.sh` — **10/10**, really executed on the
compiled C and the generated Perl against bash (stdout and exit status): 7
accepted shapes (`cat|wc`, `echo|cat`, `cat|grep`, `seq|tail`, a three-stage
`tr` chain, a pipeline inside `if`, a pipeline inside a loop body) and 3
refused (`echo|sed`, `cat|head`, a redirected pipeline). The rewrite is
measured firing: the no-transform C carries `_sh_pipeline` and the transform C
does not (3 → 0 for every accepted shape), and the refused shapes' C is
byte-identical with an unchanged runner count. No shim is needed — these
shapes use only files/stdout and the languages' own builtins, so every
backend here is executed end to end. Not executed: Go and Zig (no toolchain on
this machine); the transform is renderer-independent, so what it needs is a
renderer that consumes `IrStmt::Pipeline`, and C and Perl both do.

### Running it

```sh
RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/shir_pipeline_native.rs      # 25 verified, 0 errors
python3 <workspace>/harness/mut-shir-pipeline-native.py # 13/13 red
bash <workspace>/harness/anchor-shir-pipeline-native.sh # pass=10 fail=0
```

## `grep_pcre.rs` — the argv rewrite and the token table (47 obligations)

`grep -P 'PCRE'` becomes `grep -E 'ERE'`: one call's argv is rewritten and the
pattern is translated by a syntax-directed token table, or the pattern is
refused and the call is left alone.

| write | before | after |
|---|---|---|
| `grep -Pn '\d+' f.txt` | argv `[-Pn, \d+, f.txt]` | argv `[-En, [0-9]+, f.txt]` |
| `grep -P -- -x f.txt` | argv `[-P, --, -x, f.txt]` | argv `[-E, --, -x, f.txt]` |
| `grep -P 'a+?b' f.txt` | refused | unchanged |

The model is split where the provable part ends:

- **The argv** (structural, no regex semantics): `the_rewrite_keeps_the_operands`,
  `the_rewrite_keeps_every_pass_through_flag`, `the_rewrite_makes_the_mode_extended`
  (starts `-E`, exactly one `E`, no `P`), `the_rewrite_keeps_the_separator`.
- **The pattern** (a fold `tr` over the per-token table `tr1`):
  `a_literal_run_is_copied` (a run of literals emits its characters, in order),
  the six exact-output class lemmas, `the_group_prefix_becomes_a_plain_group`,
  `the_quantifier_marker_is_all_a_possessive_loses`,
  `the_escaped_literal_keeps_its_backslash`, `the_copied_class_is_copied`,
  `the_interval_text_is_kept`.
- **The refusals**: `has_bad` is an existential over the token list;
  `a_lazy_quantifier_is_refused`, `a_refused_escape_is_refused`, and
  `an_accepted_pattern_has_no_bad_token` (the converse, by definition).
- **The lookbehind**: `the_lookbehind_absorption_keeps_the_prefix` states
  `(?<=X)Y` → `X` ++ Y with `X` recorded as the strip prefix.

Pinned wrong rules: `the_ten_pass_through_flags_are_kept` (exactly
`o v c q l n s w x i`; `P`/`E`/`-` not in it),
`a_dash_pattern_without_a_separator_is_an_option`,
`a_dropped_flag_is_not_the_same_call`,
`a_dropped_operand_is_not_the_same_call`,
`the_naive_keep_the_pc_classes_is_not_the_written_table`,
`a_dropped_literal_breaks_the_pattern`,
`an_atomic_group_is_not_a_plain_group_without_the_marker`, and the witness
`law_at_the_witnesses`.

### The repair the model forced

The rebuilt argv dropped the source's `--` separator, so `grep -P -- -x`
emitted `grep -E -x` — grep then reads the PATTERN as an option: usage error,
exit 2, no output, where bash prints `-x` and exits 0. Measured on the
compiled C and the generated Perl. The fix keeps `--` in front of the pattern
when the source had one (`if after_dd { push Str("--") }`), pinned by
`a_dash_pattern_keeps_the_separator` / `a_separator_is_not_invented` in the
transform's tests, by A4 and `a_dash_pattern_without_a_separator_is_an_option`
in the model, and by the anchor's `dash_pattern` shape.

### Scope: what the model does NOT claim

The PCRE↔ERE *language* equivalences the table relies on — `[0-9]` ≡ `\d`,
`(X)` ≡ `(?>X)`, greedy ≡ possessive, `[[:alnum:]_]` ≡ `\w` under the C
locale, absorbing a fixed lookbehind prefix changing only the bytes `-o`
prints — are claims about a matcher, not about the rewrite. They are bounded
here and measured by the anchor, which runs the real `grep -P` and the emitted
program on the same inputs. Also not modelled: the scanner (where a class
ends, which `(?` form a prefix is, whether `{` is a quantifier), the
`$VAR` constant-map resolution, and the insertion point of the `sed` strip
stage inside a capture pipeline (the unit tests pin the structure).

### Mutants

`<workspace>/harness/mut-grep-pcre.py` — 25 independent edits inside the named
definitions: mode flag not extended / keeps `P`, the pass-through flag word
dropped, the pass-through set losing `i` or `x` or admitting `P`, the
separator dropped or invented, the operands dropped, the pattern slot lost or
off by one, the option test ignoring the separator, the fold losing its first
token, the digit/word class tables, the possessive marker kept, the group
prefix kept, the group close dropped, a literal dropped, the escaped literal's
backslash dropped, a copied class dropped, the interval text dropped, the two
refusal arms accepted, the lookbehind prefix dropped. Measured: **25/25 RED**,
each naming an obligation, none an `E0xxx`.

### Anchor

`<workspace>/harness/anchor-grep-pcre.sh` — **17/17 shapes** plus 2 emission checks,
really executed on the compiled C and the generated Perl against bash
(stdout and exit status). Accepted (12): `\d+` filter, `-o` with `\w+`, `\W`
(no match), possessive `a*+`, atomic `(?>a)b`, `-c`, `-v`, `-i`, `-q`, the
`--`-separated dash pattern, a `$pat` variable, and a non-matching pattern
(exit 1). Refused (5): lookbehind under `-o` bare, a lazy quantifier, a
back-reference, `\b`, and `-e` multi-pattern (exit 2 from grep, in both
programs). For accepted shapes the emission's grep flag word carries `E` and
no `P`, the pattern slot carries the translated text, and the `--` shape keeps
its separator; refused shapes' C and Perl emissions are byte-identical to the
no-transform ones. Not executed: Go and Zig (no toolchain on this machine:
`go=none zig=none`).

### Running it

```sh
cd sh2perl && RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/grep_pcre.rs                  # 47 verified, 0 errors
python3 <workspace>/harness/mut-grep-pcre.py     # 25/25 red
bash <workspace>/harness/anchor-grep-pcre.sh              # pass=19 fail=0
```

## `fs_cmd_lift.rs` — a tag-only lift whose refusal is the static-argv lemma (31)

`transforms/fs_cmd_lift.rs`. A statement-position `exec rm|mkdir|touch …`
whose words are all PLAIN literals becomes the shared `FsCommand` node. The
node's Rust arm reuses the `native_fs_stmt` renderer that already handled the
call, so the lift changes no emitted byte (`changed=0` over the 548 renders);
the model proves that this is a CARRIER rewrite rather than a behaviour one.

- **The static-argv guarantee** — `a_plain_word_has_no_hazard_char`: a word
  built only from the plain set (`[A-Za-z0-9]` plus `/._-+:,@=~`) contains no
  byte outside it, hence no `*`, `?`, `$`, space, quote, backslash or `;`.
  bash would resolve those at runtime while the IR holds one `Str`, so the
  refusal is exactly what keeps the node's argv equal to bash's argv; the six
  hazard characters are each instantiated as a pin.
- **The carrier law** — `the_walk_preserves_command_and_argv`: the sequence of
  `(cmd, argv)` leaves, in order, is unchanged by the walk. A dropped,
  renamed or reordered word is red.
- **Completeness and legality** — `every_liftable_exec_is_a_node` (no liftable
  exec survives anywhere in the tree) and `only_legal_nodes_are_built`
  (nothing the renderer cannot spell becomes a node), with the `offered` gate
  pinned in both directions (`gate-ignored` and `gate-always-off` are red).
- **The acceptance table is the renderer's table** — `rm` allows `f r R d`,
  `mkdir` `p v`, `touch` none; `--` is a separator that survives into argv, a
  lone `-` is an operand and is refused, and a `--long` form is refused. Each
  letter is its own pin, so widening one table is red.
- **Refusals stay exec** — a dynamic word, a glob, an unmodelled flag, a
  long flag, an empty argv, a lone `-`, and `sleep`/`mktemp`/`cat` (their
  observable value is the RESULT, not a side effect).

Scope: the model is about the LIFT — the same command and argv reach the
renderer. The renderer's own job (bash's per-operand status aggregation, the
`-f` missing-path rule, the diagnostics) is the node's pre-existing contract,
measured end to end by the anchor. The filesystem is out of model, as is the
`args.get(1)` absent-vs-empty distinction (both refuse).

### Mutants

`<workspace>/harness/mut-fs-cmd-lift.py` — 27 independent edits inside the named
definitions: the punctuation set admitting `*` or space or losing `-`; the
walk skipping the plain check; an empty word or a lone `-` accepted; the
command table admitting `sleep`/`mktemp` or losing `touch`; `rm` losing `f` or
admitting `i`; `mkdir` losing `v`; the letter table unchecked; `--long`
accepted; the separator refused; the acceptance predicate dropping a word,
the plain check or the flag check; the walk lifting refused shapes or
reverting a node; the walk missing a block/while/function/else or the second
statement; the offered gate ignored or always off. Measured: **27/27 RED**,
each naming an obligation (`pin_*`, `skeleton_walk_leaf`,
`every_liftable_exec_is_a_node`, `only_legal_nodes_are_built`), none an
`E0xxx`.

### Anchor

`<workspace>/harness/anchor-fs-cmd-lift.sh` — **19/19 shapes** (10 accepted, 9 refused),
compiled and run against bash in the same sandbox, comparing stdout and exit
status. Accepted: `rm -f`, `rm -rf`, `rm -d`, a plain `rm`, `mkdir -p`/`-pv`/
plain, `touch` with one and three operands, and the `--` separator. Refused: a
glob, a variable operand, `rm -i`, `mkdir --parents`, no operand, a lone `-`,
and the non-fs `sleep`/`mktemp`/`cat`. Two extra checks: the Rust emission
with the lift is BYTE-IDENTICAL to the no-transform emission for every shape
(the tag-only claim, measured), and on Perl — which does not declare the node
— each lifted statement becomes the backend's "not yet supported" die while a
refused shape's emission is unchanged, which is how the accepted/refused
classification is observed at all. Recorded, not claimed fixed: two `mkdir`
statements on an existing directory print the "File exists" diagnostic twice
in the Rust output where bash prints it once, identically with and without
the lift (a pre-existing renderer divergence; the anchor therefore compares
stdout and status). Go and Zig were not executed (`go=none zig=none` on this
host).

### Running it

```sh
cd sh2perl && RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/fs_cmd_lift.rs               # 31 verified, 0 errors
cd .. && python3 <workspace>/harness/mut-fs-cmd-lift.py  # 27/27 red
bash <workspace>/harness/anchor-fs-cmd-lift.sh           # pass=19 fail=0
```

## `fs_lift.rs` — the operand parse, the shape, and the GNU tables (26)

`transforms/fs_lift.rs`. A `test` call whose text is exactly a file-predicate
shape (`[ A -ef B ]`, `-nt`, `-ot`, `-N A`) becomes `FsSameFile` /
`FsNewerThan` / `FsModifiedSinceRead`. The walk covers CONDITIONS as well as
bodies (that is where these calls live), and the manifest gate is per node, so
a backend without the arm keeps the `test` call.

- **The operand parse is the content** (`classify`): an operand is a plain
  variable read (`$name`/`${name}`, identifier names only) or a literal path
  in one of the three static forms — unquoted with no hazard byte,
  single-quoted verbatim (an inner `'` refuses), double-quoted with no `$`,
  backtick, backslash or `"`. Pinned by `a_plain_name_is_an_identifier`,
  `unquoted_operand_has_no_hazard`, `quoted_operand_is_literal`,
  `single_quoted_operand_is_verbatim`, and by refusals for a glob, an indexed
  read, `$1`, a prefixed expansion, a capture, a mid-word quote and a
  backslash. `[ A -ef B ]` is ONE `Str` at IR time; bash would resolve those
  characters at runtime, so this parse is what makes the node's path the
  path bash would stat.
- **The shape law**: the operands are carried IN ORDER (a swap is red), `-nt`
  sets `newer = true` and `-ot` `false`, a two-token text that is not `-N` is
  refused, and `-N` with two words is refused when the middle word is no
  operator. Each of the three gates refuses without changing the call.
- **The predicate meanings** are stated independently of the lift: `-ef`
  needs both sides to exist with equal dev/ino; the missing-operand table says
  exactly one ordering predicate holds when exactly one side exists and
  neither when both are missing; `-N` is strict (`mt > at`, so equal
  timestamps are not "modified") and needs existence; `ot_is_not_nt` keeps the
  flag direction honest by a witness where the two disagree.

Out of model (stated): the token split (`split_whitespace` is the pass's own,
modelling it would prove the parser), `trim`, and the renderers' `stat(2)`
agreement — the nodes' existing A1 contract, measured by the anchor. The
`-O`/`-G` refusals are recorded with their reason (effective uid/gid are not
exposed to the emitted program).

### Mutants

`<workspace>/harness/mut-fs-lift.py` — 24 independent edits inside the named definitions:
the name start/tail, the hazard tables losing `*`, `$`, a quote or a
backslash, the unquoted / double-quoted / single-quoted acceptance arms, the
empty token, `-nt`/`-ot` swapped or forced, the operand order, `-N` arity, the
four-token shape, the three gates, and the four predicate bodies (existence,
strictness, and the missing-side rule). Measured: **24/24 RED**, each naming
an obligation (`pin_*`, `the_*_table`, `minus_n_*`), none an `E0xxx`.

### Anchor

`<workspace>/harness/anchor-fs-lift.sh` — **19/19 shapes** (12 accepted, 7 refused). The
controlled pair is `text-ops` (baseline) vs `text-ops,fs-lift` (test): the
lift runs inside that walker, so "accepted" is measured as a CHANGED emission
and "refused" as byte-identity, and the expected class is asserted per shape.
Each lifted program is compiled and run against bash in a fresh fixture,
comparing stdout and exit status. Recorded, not attributed to the lift:
`variables`' pre-lift baseline does not build (the lift's inline path is what
compiles it), `indexed`'s baseline already differs from bash, and
`[ -N f g ]` reaches the lift joined as `-N fg` (a front-end text join that
predates the pass — the model refuses the two-operand shape regardless). Go
and Zig were not executed (`go=none zig=none`).

### Running it

```sh
cd sh2perl && RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/fs_lift.rs                   # 26 verified, 0 errors
cd .. && python3 <workspace>/harness/mut-fs-lift.py       # 24/24 red
bash <workspace>/harness/anchor-fs-lift.sh                # pass=19 fail=0
```

## `text_filter_lift.rs` — the static text pipeline (30)

`transforms/text_filter_lift.rs`. A pipeline whose stages are all statically
known — a literal producer (`echo`, `printf`, `seq`, `cat PATH`) followed by
`wc`, `head`, `tail`, `uniq` or `cut -d/-f` — becomes one `TextFilter` node.
The model proves the acceptance decision and the emitted op list; the node's
byte output is its pre-existing A1 contract, measured by the anchor.

- **The shape**: every stage is exactly one `builtin`/`exec` call whose argv
  is all literal `Str` words (`stage_ok`). A dynamic word and a two-statement
  stage are refused by their own pins, and `the_second_stage_is_a_filter`
  plus `a_one_stage_pipeline_is_refused` pin the pipeline arity.
- **Producers**: `echo` scans the leading `-e/-n/-E` run only until the first
  non-flag word (a later `-n` is a WORD), records the suppressed newline, and
  under `-e` requires every escape to be one of `\n \t \r \\` (a lone
  trailing backslash refuses). `printf` takes one format with no `%`.
  `seq A B` needs positive integers with `B >= A`, and `head 3` / `head 13`
  are the OPERAND form, not counts. `cat PATH` needs one non-empty,
  non-dash, `;`-free path. Each rule has its refusal pin.
- **Filters**: `wc -l|-w|-c` with no operand; `head/tail -N` or `-n N` with a
  positive count and no `+`; bare `uniq`; `cut -d D -f LIST` in either flag
  order, with a one-character non-`;` delimiter and a comma list of positive
  fields (no empty, leading, trailing or zero field; no range).
- **One op per stage, in order**: `filters_of_len` states the emitted list has
  exactly as many entries as there are filter stages; the concrete pins show
  the exact op for `echo hi | wc -l`, `seq 1 10 | head -3`,
  `echo a:b | cut -d: -f2` and `cat f | uniq`.
- **The `;` encoding is unambiguous**: the node joins its ops with `;`, so a
  carried path or delimiter containing `;` refuses, as does a zero field.

Out of model (stated): the renderer's A1 (producer bytes — `echo`'s join,
newline and four decodes, `printf`'s format, `seq`'s range — the filter
outputs and `cat`'s read-error text), the `format!` spelling of the op
string, `parse::<i64>` overflow (a digit string is its mathematical value),
and the statement walk (`finish` models the stage list each pipeline
occurrence hands it).

### Mutants

`<workspace>/harness/mut-text-filter-lift.py` — 28 independent edits inside the named
definitions: the stage flags, the echo prefix scan and its stop-at-a-word
rule, the flag probe, the escape table and its lone-backslash case, the `-e`
gate, `printf`'s `%`/arity/escape checks, `seq`'s order and positivity, the
`cat` path's `;`, dash and emptiness rules, the cut delimiter length and `;`
guard, the field list (zero, empty, skipped), the `-N`/`-n` count parse, the
`wc` flag match, `uniq`'s arity, the op accumulation and the pipeline arity.
Measured: **28/28 RED**, each naming an obligation (`pin_*`,
`a_*_is_refused`, `the_second_stage_is_a_filter`, `filters_of_len`), none an
`E0xxx`.

### Anchor

`<workspace>/harness/anchor-text-filter-lift.sh` — **24/24 shapes** (9 accepted, 15
refused). The controlled pair is `text-ops` (baseline) vs
`text-ops,text-filter-lift` (test): accepted is measured as a CHANGED
emission and refused as byte-identity, with the expected class asserted per
shape. Each lifted program is compiled and run against bash in a fresh
fixture, comparing stdout and exit status. Accepted: `echo|wc`,
`seq 1 10|head -3`, `echo a:b|cut -d: -f2`, the reversed cut flag order,
`cat f|uniq`, `echo -n hi|wc -c`, `printf 'a\nb\n'|wc -l`, `seq 3|tail -1`,
`echo a b c|wc -w`. Refused: `awk`, multi-file `head`, `tail -f`,
`tail -n +2`, `head -c`, `cut -c`, a field range, a two-character
delimiter, `wc -L`, the `wc` operand form, `uniq -c`, `tee`, `cat` with no
operand, a one-stage pipeline and a dynamic word. Go and Zig were not
executed (`go=none zig=none`).

### Running it

```sh
cd sh2perl && RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo \
  ~/verus-0.2026.10.18.7d82994/verus --triggers-mode silent --rlimit 60 \
  proofs/verus/text_filter_lift.rs           # 30 verified, 0 errors
python3 <workspace>/harness/mut-text-filter-lift.py  # 28/28 red
bash <workspace>/harness/anchor-text-filter-lift.sh           # pass=24 fail=0
```

## status_reads.rs — the `$?` register liveness law (28)

    verus --triggers-mode silent --rlimit 40 proofs/verus/status_reads.rs
    -> verification results:: 28 verified, 0 errors

`shir_passes/status_reads.rs` rewrites nothing, so it is easy to file away as
"nothing to prove". It owes two laws, and they are the only reason the Python
backend may drop a `__sh_rc = …` line:

- **STORE LIVENESS** (`dead_store_law`): a store the analysis marks dead
  (`!live[i]`) is observed by nothing — no `$?` read sees it before the next
  definite store overwrites it, and the process exit does not see it. The
  closed form `observed_after` (the exit, or a later read before any store)
  is proved equal to the backward scan (`live_agrees`), so the scan's `need`
  carry is not merely an approximation of itself.
- **REGISTER SOUNDNESS** (`no_reader_keeps_only_the_exit_store`): with no
  `$?` reader anywhere, every store but the last is dead and the last is live,
  so `sys.exit(__sh_rc)` is exact; and a read with no live store before it
  observes the register's INITIAL value (`first_read_observes_init`), so the
  initialiser cannot be dropped (`pin_init_read_needs_the_initialiser`).

The third term the source adds to `live[i]` — a statement that does not
*definitely* store but whose nested list reads `$?` before its own first store
— is load-bearing, not decoration: without it the analysis drops a store its
own nested read can observe (`pin_nested_after_write_keeps_the_store`).

### Mutants, measured

The four mutants the plan names are all red, and each is measured in both
spellings — the verdict side (`stmt_terminates`/`esc_one` answering wrongly) and
the transform side (`trunc` pruning where it must not) — because the verdict-side
spelling is caught by a pin's own assertion at elaboration, which would not by
itself show that the *laws* are load-bearing. Transform side: pruning after an
`if`-nested terminator **30 verified, 1 error** (`trunc_sound`), pruning after a
`case` whose body ends in a terminator **30v/1e** (`trunc_sound`), and treating a
top-level `Return` as a terminator **29v/2e** (`trunc_sound` and
`return_truncates_only_a_function_body`). Verdict side: the `if`, `case` and
nested-break spellings are each refuted at elaboration by their own pin
(`pin_conditional_exit_does_not_deaden`, `pin_case_exit_may_not_fire`,
`pin_nested_break_does_not_escape`). The `case` verdict-side mutant needed
`decreases` clauses added to elaborate at all — the version without them is a
syntactic rejection, not evidence. Restored byte-identically after every run;
counts and shas are in `uae_terminators_sync.txt`. No mutant stayed green, so no
vacuity was found here (unlike `affix_strip_fold`'s rule relabel, which was green
until `fold_rule_classifies` was added).

### Source drift

`src/shir.rs` has moved on since this proof was written (brace expansion,
74463779b and 4106ff5f6), so the recorded digest was re-pinned after an audit:
`stmt_terminates`, `list_terminates`, `is_true_cond`, `loop_escapes` (with its
nested `has_break`) and `program_observes_pipestatus` are byte-identical between
the previously recorded revision (7d1c08df8) and HEAD, and the concatenation of
those bodies digests `0559d98e0e41` at both. The manifest now records HEAD's file
digest (`86b738598fb8`) with that audit rather than a silent bump.

### Scope (read this before quoting it)

- The three facts the machine consumes (`reads`, `stores`, `nested`) are
  **taken as given**: that `stmt_reads_status_in` really is a
  may-over-approximation of the `$?` occurrences (over ~30 `IrStmt`/`IrExpr`
  variants) is a separate traversal obligation, proved in
  `status_reads_traversal.rs`. `program_reads_status`'s `$?`-in-Debug fallback
  makes the direction that matters total in practice, but that is an
  argument, not a proof, and this file does not discharge it.
- Bodies: `body_store_liveness` localises a function/sub body into its own
  per-statement vector, and `escaped` (a body whose first read precedes its
  own first store) turns localisation off. Both are sound by construction —
  a per-body vector is the same scan over a shorter list, and "off" means keep
  every store — but the call-site machine (which body runs where, with which
  caller status) is out of this model. So is the `None`-means-keep-everything
  default.
- The body-localisation and `program_reads_status` tests belong to the
  companion models.

### Pins, executed

All eleven rows are MAPPED to generated tests: `pin_specs/status_reads.json`
names each witness, `gen_pin_tests.py` writes
`src/shir_passes/status_reads_liveness_pin_tests.rs` driving the real
`status_store_liveness`, and a row flips to MAPPED only after its own test
passes under `cargo test --lib pin_tests`. `pin_empty_list` records a
divergence rather than hiding it: the model's `live` on an empty list is the
empty vector, while the source answers `None` ("keep every store") — equal in
effect, and `None` is the conservative side.

## status_reads_traversal.rs — the owed `$?`-traversal law, discharged (70)

    verus --triggers-mode silent --rlimit 40 proofs/verus/status_reads_traversal.rs
    -> verification results:: 70 verified, 0 errors

`status_reads.rs` proves the store-liveness law **under** three per-statement
facts it takes as given (`reads`, `stores`, `nested`) and says so in its
header: "that `stmt_reads_status_in` is a may-over-approximation of the `$?`
occurrences … is a syntax-traversal property over ~30 `IrStmt`/`IrExpr`
variants … an argument, not a proof." This file is that proof, over a grammar
with one constructor per source arm (one divergence is reported in Scope),
and **two independent recursions**: the
analysis transcribed from the source, and a semantic side that collects what
the walk must visit (`flat_*` into a sequence of atoms), emits what the
renderer writes (`emit_s`), or orders the events a nested list produces
(`evs_l`).

### What is proved

- **`reads_exact_s` / `reads_exact_x`** (and the list/clause helpers): the
  analysis's answer EQUALS the atom collection, so both directions are
  caught — a forgotten arm (false negative) and an over-claim. Corollary
  `reads_complete_s` is the plan's `reads_is_may_approx`, which exists under
  that name for the consumer.
- **`stores_is_must`**: `stmt_definitely_stores_status(s)` implies the
  statement's own emitted fragment writes the register. The right-hand side
  is written from the renderer's placement, so this is the arm where the
  command-substitution assign is load-bearing: it renders through
  `__sh_bash` and leaves the register alone, which is exactly why the
  analysis must not trust it. The control-flow constructs pass because
  bash's `if`/`while`/`for`/`case` set `$?` on every path (0 when no branch
  runs) — that fact is in `emit_s`, not in the analysis.
- **`nested_complete`** (exposed as the plan's `nested_is_may_approx`): a
  nested list that reads `$?` before its own first
  definite store implies `stmt_nested_reads_before_store(s)`. One-way on
  purpose: the source also counts a condition read there, so it may
  over-claim (keep more stores), never miss. The bridge
  `starts_read(evs_l(l)) == rbs_l(l)` is the induction that makes the event
  ordering and the source's list predicate the same verdict.
- **`facts_sound`**: all three, in one statement, for the consumer.

### Mutants, measured

Counted (a verification failure, after the mutation compiles):
dropping the `Cap`/backtick arm gives **69 verified, 1 error** at
`reads_exact_x`; the test operand never reporting a read
(`X::Var(_) => false`) gives the same **69 verified, 1 error** there; a
literal text never being a read (`Part::LitQ(_) => false`), **69 verified,
1 error**; the same `If`-must-store shape on an arm no pin covers
(`DeclArr`) gives **69 verified, 1 error** in `stores_is_must`'s `DeclArr`
arm; and the nested walk dropping an arm no pin covers (`Subshell`) gives
**69 verified, 1 error**, `nested_complete` running out of rlimit.

Refuted one stage earlier, at simplification, by the concrete pins: the
nested walk dropping `Redirect` (`pin_nested_redirect_reads_before_store`)
and calling `If` a must-store while `emit_s` stops writing the register for
an `If` fragment (`pin_control_flow_stores`) — the same rejection class the
README records for typeset_attrs' leading-zero mutant. (An earlier version of
this section called the literal-text mutant a simplification rejection; the
measurement above is the correct one.) Restored byte-identically after each
run (sha256 in `status_reads_traversal_sync.txt`).

Re-measured 2026-11-28 against the committed bytes (proof sha
`ad95c02d8d5a`, source sha `7319a9ef6136`): the same verdicts, re-confirmed
after every restore. The two named mutants a pin refutes at simplification
were additionally measured with a sibling arm **no pin covers** — `Wh`
must-store with no `emit_s` write, `Bg` dropped from the nested walk, and
`Interp` dropped from the read walk — each **69 verified, 1 error**, naming
`stores_is_must`, `nested_complete` and `reads_exact_x`. A simplification
rejection of a pinned shape therefore has a law-level twin: the theorems,
not the pins, are what make these recursions load-bearing.

### Scope (read this before quoting it)

- One constructor per source arm, but an expression's children are carried as
  a `Seq` (Rust layout; the structural order is the sequence's). `Interp`
  carries literal parts with their `$?` bit and expression parts; `Call`
  classifies the function (status-named, `block`, `getVar("?")`, a capture
  spelling, the four non-storing print arms, plain).
- TEXT is atomic: `StrQ(q)` / `LitQ(q)` mean "this text contains `$?`". The
  model does not re-derive `str::contains`; it states the property the
  analysis keys on, which `program_reads_status`'s Debug fallback provides
  in practice.
- KNOWN DIVERGENCE, stated rather than smoothed over: the `Function` arm.
  The model scans the body *and* its named blocks; the source's
  `stmts_read_status_in` matches `IrStmt::Function { body, .. }`, dropping
  them. That is the safe direction for liveness (the model never calls such a
  body status-free), but it is not arm-for-arm: a `$?` read inside a named
  block is missed by the shipped analysis, and only
  `program_reads_status`'s `$?`-in-Debug fallback catches it.
- `emit_s` is a model of the RENDERER's register placement, not a proof of
  it; that the backends emit those instructions is a renderer contract,
  anchored end-to-end, not proved here.
- Out of model, stated: the call-site machine for bodies (the per-body
  vector is the same scan, see `status_reads.rs`), and the variants the
  source's own `_ => false` arm declines to analyse — the model carries
  `OtherX`/`OtherS` for those and analyses them as nothing, exactly as the
  source does.
- The source's `a_sub_body_localises_its_own_reads` and
  `a_body_read_of_the_callers_status_keeps_everything` are the body-machine
  tests and are not this file's.

## uae_terminators.rs — `unreachable_after_exit`'s other four terminators (31)

    verus --triggers-mode silent --rlimit 40 proofs/verus/uae_terminators.rs
    -> verification results:: 31 verified, 0 errors

`elim_trio.rs`'s `uae_sound` covers the *direct `Exit`* arm only. The
transform prunes after every statement `shir::stmt_terminates` calls a
terminator, and that verdict has four more arms: `Break`, `Continue`,
`Return` (only inside a function body), and `While`/`ForInit` with a
constant-true condition and no escaping `break`. This file proves each one
against a run that carries a control signal, and proves the transform's
truncation sound in general.

### What is proved

- **`trunc_sound`**: truncating a list at its first verdict-terminator
  preserves the run — its trace AND its control signal — for every list.
  `trunc_stmt_sound` lifts that through the per-statement recursion the pass
  performs, so a nested body is truncated too.
- **`loop_without_escape_never_falls_through`**: a break-free constant-true
  loop contributes nothing after it; `terminates_stops` supplies the step
  that says so, via `no_escape_no_brk`.
- **`return_truncates_only_a_function_body`**, and the three signal
  instances `break_truncates_its_own_list` /
  `continue_truncates_its_own_list` / `exit_truncates_its_own_list`.

### The refusals, proved as pins

`pin_conditional_exit_does_not_deaden` and `pin_case_exit_may_not_fire`: an
`if`/`case`-nested `exit` deadens nothing after the *enclosing* statement,
because the arm may not run — this is why only a DIRECT terminator prunes.
`pin_nested_break_does_not_escape` (a nested loop owns its breaks),
`pin_case_break_escapes` (bash's `case` captures nothing) and
`pin_break_call_escapes` (`while true; do break || exit 1; done`, where
`break` arrives as a control call inside a `BinOp`) pin the three
loop-escapability decisions; the first two of those mirror the source's own
`a_break_inside_a_nested_loop_does_not_escape` and
`a_break_inside_a_case_still_escapes_the_loop`, and the third mirrors
`a_break_control_call_makes_the_loop_escapable`.

### Pins, executed

All ten rows are MAPPED to generated tests: `pin_specs/uae_terminators.json`
names each witness, `gen_pin_tests.py` writes
`src/transforms/uae_terminators_pin_tests.rs` driving the real
`unreachable_after_exit::transform`, and a row flips to MAPPED only after its
own test passes under `cargo test --lib pin_tests::`. The continue pin uses a
`Block` rather than a loop body: a break-free `while true` is itself a
terminator, so the pass keeps it whole and never descends into it — a
different fact from the one that pin states.

### Mutants, measured

Counted: truncating after an `if`-nested terminator gives **30 verified,
1 error** (`trunc_sound`); treating a top-level `Return` as a terminator
**29 verified, 2 errors** (`trunc_sound` and
`return_truncates_only_a_function_body`); pruning after a `case`-nested
`Exit` **30 verified, 1 error** (`trunc_sound`). Refuted at simplification
by a concrete pin: counting a nested loop's `break` as escaping
(`pin_nested_break_does_not_escape`), dropping the terminator from the kept
prefix (`pin_trunc_shapes`), letting a `case` break fall through instead of
ending the loop (`pin_case_break_escapes`), and running past a non-None
signal (`pin_break_stops_the_list`). Restored byte-identically after each
run (sha256 in `uae_terminators_sync.txt`).

### Scope (read this before quoting it)

- The model unrolls a loop body ONCE and returns `Spin` ("another iteration
  would start"): enough for the reachability claim this pass makes, not a
  full loop semantics.
- `cond_const`/`cond` are the source's `is_true_cond` (`IrExpr::Bool(true)`);
  a `while (( 1 ))` is out of this model.
- `Case { fires, … }` records whether a clause matched on the run, which is
  the whole point of the refusal; `BreakCall` is the control-call spelling.
  `Die` is folded into `Exit`.
- Not modelled, stated: the pass's `changed` bookkeeping and named-block
  rebuilding, and the other consumers of the verdict (they read the same
  functions, so a verdict change is the same theorem).

## affix_strip_fold.rs — `${v#lit}` is one `strip_prefix` (52)

    verus --triggers-mode silent --rlimit 40 proofs/verus/affix_strip_fold.rs
    -> verification results:: 52 verified, 0 errors

All sixteen pins are generated tests (`src/transforms/affix_pin_tests.rs`) that drive the
real pass; the two value rows the manifest used to leave UNMIRRORED now compare the folded
node's `strip_prefix`/`strip_suffix` value against what BASH prints for `${v#lit}`,
`${v##lit}`, `${v%lit}`, `${v%%lit}` and the star forms. `fold_rule_classifies` was added
because a relabel mutant (swap the literal rule 1 and the star rule 2) was measured GREEN
(51v/0e) without it — every other law only asks `!= 0`. Seven mutants are red, recorded
in the manifest with their exact counts.

The fold turns `param("#"|"##"|"%"|"%%", NAME, PAT)` into one `AffixStrip`
node when `PAT` is a magic-free literal. That is only sound because bash's
`#`/`##` (and `%`/`%%`) agree for such a pattern: exactly one prefix can be
the literal, so shortest ≡ longest and the strip removes at most one
occurrence. This file proves that against a scanner that ENUMERATES the
matching prefixes (lengths upward for shortest, downward for longest)
instead of assuming the closed form.

### What is proved

- **`literal_shortest_eq_longest`**: the two scans return the same cut (the
  literal's own length) or both find nothing.
- **`fold_preserves_value`** and its suffix mirror: bash's `${v#lit}` and
  `${v##lit}` are both the node's `strip_prefix(lit)` value — likewise
  `${v%lit}`/`${v%%lit}` and `strip_suffix(lit)`.
- **`literal_strip_is_one_occurrence`**: the result is `v`, or `v` minus
  exactly one leading (resp. trailing) occurrence.
- **`star_forms_preserve_value`**: `#lit*` ≡ `#lit` and `%*lit` ≡ `%lit` —
  the shortest matching prefix/suffix of a one-star pattern is the body
  itself — which is exactly the two `AffixStrip` star arms.
- **`fold_is_idempotent`** (convergence): a second visit is the identity,
  because the fold's output is a NODE and not a `param(…)` call, so the
  one-shot rewrite can never re-enter its own rule.
  **`fold_rules_are_exclusive`** backs it up: a star is glob magic, so the
  literal branch and the star branch cannot both claim one pattern and the
  rule choice is a function of the pattern, not of visit order.

### The pass's analysis, as its own recursion

The value laws above check the NODE against bash's scan; they do not say the
pass's own walk reaches every fold site or that its shape classifier claims
exactly the strippable patterns. The pass is modelled a second time, as a
recursion (`fold_t`/`fold_ss`) over a term tree whose `Call` node carries the
argument list, and checked against the semantic domain (`strippable`, written
from bash's rules — literal, or one end-pinned star with a magic-free body):

- **`fold_rule_covers`**: every strippable pattern is claimed. A shape
  branch forgotten in the classifier makes this red.
- **`fold_rule_sound`**: nothing bash cannot strip is claimed — the empty
  pattern, a middle star, and a star with a `?`/`[`/`\` body all stay out.
- **`fold_t_covers`**: one visit folds every strippable site at every depth.
  A forgotten `Call`/child arm leaves a nested `Param` behind and this is
  red.
- **`fold_t_preserves`**: each rewritten site keeps the semantic value
  (`preserved`, pointwise through the argument lists), so a wrong op mapping
  is red. The op gate (`#`/`##`/`%`/`%%` only) is part of the model.

### The pins

`pin_double_star_differs` is the table's negative control: on `a/x/b/x`,
`${v#*x}` is `/b/x` while `${v##*x}` is empty. `pin_dirname_basename` pins
`${p%/*}` / `${p##*/}`; `pin_no_occurrence_is_the_value` the no-match rule;
`pin_empty_body_star` the `%*` identity; `pin_magic_is_refused` and
`pin_star_shape` the two refusals (`*`/`?`/`[`/`\`, and a middle star, a
second star, or no star at all). The analysis recursion adds
`pin_nested_site_is_visited` (a fold site under a `Call` is rewritten by the
visit), `pin_middle_star_is_refused`, `pin_star_body_magic_is_refused` (the
shape gate is not enough — the body is checked too), and
`pin_non_strip_op_is_not_a_site` (a non-strip op stays a call).

### Mutants, measured

Counted: making the fold's output re-enter the rewritable class (a `Node`
that is itself a `param(…)` call) gives **48 verified, 3 errors** — in
`fold_t_covers`, `fold_t_preserves` and `fold_is_idempotent`, so both the
coverage and the convergence laws are load-bearing. Refuted at
simplification by a concrete pin: stripping the affix without checking it is
present and over-stripping one character past it
(`pin_literal_one_length`), `*` no longer counted as glob magic
(`pin_magic_is_refused`), the visit forgetting a call's arguments
(`pin_nested_site_is_visited`), the classifier forgetting the literal branch
(same pin), and the classifier accepting a middle star
(`pin_middle_star_is_refused`). Restored byte-identically after each run
(sha256 in `affix_strip_fold_sync.txt`).

### Scope (read this before quoting it)

- The four `find`/`rfind` Cut identities (`%lit*`, `%%lit*`, `#*lit`,
  `##*lit`) are modelled as the `CutAt` node's value and pinned on concrete
  inputs, but the step from the shortest/longest SCAN to `find`/`rfind`
  (that the matched prefix ends exactly at the occurrence) is **stated, not
  proved** — the same table the source states.
- Strings are `Seq<char>`; the NAME-slot lexical gate (`is_plain_ident` /
  `is_text_operand_name`) and the target gates (`fold_target_ok` /
  `cut_target_ok`) are not modelled: they decide whether the fold is
  offered, never its value. Their refusals are the source's own tests.
- The empty-body arms (`%*` identity, `%%*` empty) are value-level pins.
- The traversal is modelled at the argument-list level (`Term::Call` /
  `fold_ss`): the visit is ONE bottom-up pass with no fixpoint loop, so
  convergence is `fold_is_idempotent` (a second visit is the identity) rather
  than a termination proof of a loop that does not exist, and
  `fold_t_covers` is the reachability fact. The statement/expression KINDS
  are not modelled one-for-one (they reduce to "a node with children"); the
  model checks that no child is skipped.
- The op × shape table is not modelled: `fold_rule` keys on the pattern,
  because both replacement families are value-equal wherever they apply.
  Which of the eight op/shape pairs the source refuses (`## lit*`, `%% *lit`
  with a non-empty body) decides only whether the fold fires, never the
  value — those refusals are not counted as proved here.

## text_capture_fold.rs — folding constant `$(echo …)` text (43)

    verus --triggers-mode silent --rlimit 40 proofs/verus/text_capture_fold.rs
    -> verification results:: 43 verified, 0 errors

The pass replaces a `$( … )`/backtick substitution inside the RAW TEXT that
`test`/`param`/`arith` arguments carry, when that substitution's output is
provably constant. Two claims license it, and the file proves both.

### (A) What the substitution prints

`analysis` is the pass's `constant_output`, transcribed; `emit` is an
independent semantic side that is deliberately **richer** (it still calls
`echo -n hi` and a newline-carrying word constant, where the analysis
refuses — for single-line re-parse and flag-formatting reasons). The law is
the one-way direction the fold needs: **`constant_output_sound`** —
`analysis(h, ws) = Some(r)` implies the command really prints exactly `r`,
in every environment. `analysis_is_literal` adds that a computed replacement
is literal text.

`emit` shares the analysis's `all_lit`/`any_flag`/`join` helpers, so it would
move in lockstep with a weakened analysis. A second, independent semantic
recursion answers that: `printed` assigns each word its runtime value (`Lit`
prints its text, `Rt` has no constant value) and assembles the text itself,
while `flags_in` collects echo flags on the semantic side. **`analysis_prints_
the_output`** refines the analysis against *that* side: `analysis(h, ws) =
Some(r)` implies `prints(h, ws) = Const(r)`. `printed_is_join` and
`flags_in_is_any_flag` are the only bridges to the shared helpers, so a
guard forgotten in the analysis disagrees with `printed` instead of weakening
both sides at once.

### (B) What the re-parse sees

Command-substitution output is never re-parsed for quoting, but text
spliced into the shell text is. The model's token stream therefore has
`Ch` (literal), `Syn` (a character the re-parse treats as syntax), `Sp` and
`Sub`. `expand` is the ORIGINAL's stream (a substitution's `"` stays
literal), `foldtok` the folded text's (the same character is now syntax).
**`fold_preserves_the_token_stream`**: when the gate holds — no syntax
character in the replacement — both streams are identical, so the fold
introduces no quoting, escaping or expansion. `safe_is_not_syntax` and
`quoted_gate_is_no_syntax` prove the source's two gates imply exactly that
premise, and `fold_is_sound` states both halves together.

### Pins

`pin_a_quote_breaks_the_parse` is the load-bearing counterexample: `a"b` is
`Ch('a'), Ch('"'), Ch('b')` in the substitution's stream but
`Ch('a'), Syn('"'), Ch('b')` after splicing, so the gate must refuse it.
`pin_printed_assembles_the_text` pins the independent recursion's own
values (empty, `a b`, an `Rt` word, a flag, an unknown head).
`pin_analysis_shapes` pins the accepted and refused content shapes (hi / a
flag / a variable / an unknown head / `true` / `:`), `pin_emit_is_richer`
the richer semantic side, `pin_safe_splice` the two charsets (space and `*`
refused unquoted, space allowed inside quotes, `"`/`$`/`\` refused there),
`pin_flag_classes_agree` the flag classes (`-e`/`-E`/`-ne` are flags, `-x`
is not), and `pin_splice_of_the_test_operand` the source's own operand shape
`"$(echo hello)"`.

All ten are **executed**, not hand-checked: they are generated
to `src/transforms/text_capture_pin_tests.rs` from
`proofs/verus/pin_specs/text_capture_fold.json` by `gen_pin_tests.py --run`,
which drives the real `transform` / `constant_output` / `safe_repl` and flips
a row only for a test whose own `… ok` line it observed (10/10 PASS). The
three rows that used to be UNMIRRORED are now executed too: two compare
`constant_output` against what BASH prints for the same command, and the
third splices each accepted replacement into a real `[ … ]` script and
requires the same exit status as the original substitution.

### Two vacua, found by mutation and closed

Both mutants below used to leave the file **green**, and both were fixed in
the proof rather than explained away:

1. **Flag charset.** `is_flag`, `emit` and `flags_in` all read the single
   `all_ne_e`, so narrowing its charset to `n` verified at **37v/0e** — while
   the pass would happily fold `$(echo -e hi)` into `-e hi`, which bash never
   prints. Fix: `flag_text`/`is_flag_sem`, the semantic side's own flag
   recursion (used by `prints`), with `flag_text_is_all_ne_e` and
   `is_flag_sem_is_is_flag` as the bridge. The mutant now breaks the bridge.
2. **Line breaks.** `analysis` and `emit` both read `any_broken`/`has_ws`, so
   dropping `\r` from it verified at **41v/0e** (after fix 1) — while a `\r`
   replacement would follow the fold into a single-line operand. Fix:
   `breaks` inside `printed`, with `has_ws_is_breaks` as the bridge.

### Mutants, measured

Against the committed proof bytes, each restored byte-identically (sha256 in
`text_capture_fold_sync.txt`), baseline 43v/0e:

- Law-level (**42 verified, 1 error** each): the semantic flag test losing its
  length check (`is_flag_sem_is_is_flag`), the semantic flag charset losing
  `E` (`flag_text_is_all_ne_e`), the gate skipping a substitution
  (`lemma_tokens`, i.e. `fold_preserves_the_token_stream`), the substitution's
  and the folded text's spaces becoming literal characters (`lemma_chars`),
  and the line-break check no longer refusing `\r` (`has_ws_is_breaks`) —
  the vacuity-2 mutant.
- Pin-level (rejected at simplification, recorded as such): the flag charset
  narrowed to `n` (`pin_flag_classes_agree` — the vacuity-1 mutant, whose
  law-level twins are the first two bullets), a doubled join separator and an
  analysis that trusts a flag (both `pin_analysis_shapes`), and a plain
  character made syntax inside quotes (`pin_a_quote_breaks_the_parse`).

### Scope (read this before quoting it)

- The **lexer** is out of model: `scan`, `find_close`, the quote modes, the
  wholesale skip of single-quoted regions, `$((…))` balance and span
  splicing. The model starts from "here is the substitution and the text
  around it", which is what the lexer guarantees; a lexer bug (an unbalanced
  or mis-detected span) is not covered here.
- Word classification collapses `shell_words`' per-character refusals
  (`\ $ ` | & ; < > ( ) { } * ? [ ] # ! ~`, quoted expansions) into one
  `Rt` class; the individual spellings are the source's own tests.
- The echo flag's **formatting** (`-e` interpreting escapes) is not
  modelled: a flag makes `emit` `NotPlain` and the analysis refuse.
- The IR traversal and the `TEXT_FUNCS` position list are structural: any
  subset of positions that still funnels through `fold_text` is covered by
  (A)+(B) unchanged.

## bc_float_clean.rs — the trailing `+ 0.0` strip removes the identity (27)

    verus --triggers-mode silent --rlimit 40 proofs/verus/bc_float_clean.rs
    -> verification results:: 27 verified, 0 errors

`transforms/bc_float_clean.rs` rewrites an `echo "scale=K; EXPR" | bc`
capture in place: when the LAST additive term of the argument text is the
literal `0.0` and the remainder still carries a decimal, that term is
deleted. The surgery is `trim_end`, `rsplit_once('+')`, a `trim()` and a
second `trim_end`, so what the pass needs is a STRUCTURAL claim about the
text it deletes — not a floating-point one.

### (A) The strip deletes exactly an identity tail

`strip_add_zero` transcribes `strip_trailing_add_zero` over whole-string
cursors (`trim_len` for `trim_end`, `last_plus` for `rsplit_once('+')`,
`skip_ws` for `trim()`), and the semantic side is an INDEPENDENT five-state
DFA — `ws* '+' ws* '0' '.' '0' ws*` — written from the grammar, not from
the pass's cursors. **`strip_removes_only_the_identity`**: if the strip
returns `Some(p)` then `p` is the string's prefix, it does not end in
whitespace, and the span the strip deleted is accepted by the DFA.
`identity_tail_is_one_zero_term` goes further: inside an accepted span the
non-whitespace characters are exactly `+`, `0`, `.`, `0`, in that order —
so the deleted text is ONE additive term and it is zero.

### (B) Removing that term preserves the value

The value model is a list of integer terms summed by `total`. The identity
bc relies on — `x + 0.0 == x` for finite decimals — is proved there as
`total(a.push(0), 0) == total(a, 0)` (`lemma_total_push` +
`add_zero_is_the_identity`), and `strip_preserves_the_value` states (A) and
(B) together for any string the strip accepts.

### Pins

`pin_strip_shapes` the accepted shapes (with and without whitespace on
either side of `+`, a `scale=2; 3.5 + 0.0` argument), `pin_strip_refusals`
the refusals that carry the safety (`+ 0`, `+ 0.00`, a `0.0` that is not the
last term, `-0.0`, no `+` at all, the empty string), `pin_the_decimal_gate`
the second gate — `1 + 0.0` becomes `1`, which has no decimal, so the caller
refuses it (the shader backend's float verdict must not change), while
`1.5 + 0.0` is accepted. `pin_dfa_agrees` checks the recogniser on the same
spans and `pin_one_occurrence_per_pass` records that one pass removes one
term (`3 + 0.0 + 0.0` keeps its inner identity — the caller re-runs).

### Mutants, measured

Counted (the run still verifies 26 of 27 and fails exactly one): the strip
dropping the `s[j] == '0'` check — caught by the law's DFA step into
`SeeZero`, 26v/1e; the DFA's `AfterPlus` also accepting `'1'` — caught by
the term-shape equation in `lemma_run_consumes`, 26v/1e; the term model's
`total` base case being 1 — caught by `lemma_total_push`, 26v/1e. Refuted at
simplification by a concrete `by (compute)` pin (the run aborts before its
summary line): the trailing-whitespace gate dropped, the whitespace before
`+` kept, `Trail` accepting any character, `BeforePlus` not skipping
whitespace, `trim_start` not trimming, `last_plus` seeing only an adjacent
`+`, and `skip_ws` skipping nothing. Each was restored byte-identically,
sha256 re-checked (see `bc_float_clean_sync.txt`).

### Scope (read this before quoting it)

- **The float arithmetic is not modelled.** Verus has no reals here; the
  proof is over integer terms. `x + 0.0 == x` for finite decimals is bc's
  arithmetic fact, stated and not proved. It also fails for non-finite
  values — out of the transform's domain.
- The `echo … | bc` PIPELINE shape check (`clean_pipeline`: `exec`/`builtin`
  echo, two stages, the second `exec bc`, exactly one argument), the
  `Interpolate` last-part rule and the scan of earlier literal parts for a
  decimal, and the IR traversal are all out of model.
- Only the renderer contract's INPUT predicate (`has_dot` on the stripped
  text) is modelled, not the verdict itself.
- The analysed module has **no `#[cfg(test)]` block** (measured:
  `grep -c 'cfg(test)'` → 0), so every pin row in the manifest is
  UNMIRRORED and no `[exec]` PASS row exists for this proof.

### Cluster F2 — the literal `grep` fold (34 obligations + 12 executed pins)

    verus --triggers-mode silent --rlimit 40 proofs/verus/grep_literal_fold.rs
    -> verification results:: 37 verified, 0 errors

`native_static_grep_redirect` folds `echo LIT | grep [flags] PAT > FILE` into
`WriteFile(FILE, grep_output(words, input))`. Every byte it writes comes from
one analysis function, and that function refuses every pattern carrying a
metacharacter — so F2, the cluster the plan priced as "the model is the cost",
turns out to be a LITERAL matcher plus grep's context arithmetic. This file
proves that analysis against an independent semantic side:

- **The two matchers.** `word_boundary_hit` is a left-to-right scan asking
  "equal bytes here, non-word neighbours"; the semantic side is "some offset
  has equal bytes and non-word neighbours" and `word_scan_is_boundary` refines
  one into the other. `String::contains` gets the same treatment
  (`contains_scan_is_plain`), so a scan that skips offset 0 or stops a byte
  early disagrees with its own first witness.
- **The selection and its context.** `grep_output` turns the matching line
  indices into a range list (`start = m-before` clamped at 0, `end =
  m+after+1` clamped at the line count, a new range only when `start >
  last.end`). The semantic side is "print line `i` iff some match `m` has
  `m - before <= i <= m + after`", written straight from the manual.
  `ranges_select_the_context` proves the two index sets equal in BOTH
  directions, and `merge_sound` shows one merge step is exactly a union — so
  the clamping, the `+1` and the `start <= end` (not `<`) test are all
  load-bearing.
- **Pins, executed.** Twelve rows, all MAPPED, all generated:
  `pin_specs/grep_literal_fold.json` names each witness, `gen_pin_tests.py`
  writes `src/transforms/grep_pin_tests.rs` driving the real `grep_output` and
  `word_boundary_hit`, and a row flips only after its own test passes. The
  rows include the refusals (metacharacter pattern, unknown flag, file
  operand, non-numeric `-A`) and both `-w` boundary cases.
- **Mutants, measured** on this exact file (sha `01140df8b4f0`, each restored
  byte-identically): drop `hit_at`'s right boundary 36v/1e; `win_end` without
  the `+1` 35v/2e; a merge that never extends 34v/3e; a scan that skips a byte
  36v/1e.

Three shapes reach the fold and do NOT agree with bash. They are recorded, not
smoothed over: the fold emits `status_exec(true)` where `grep` exits 1 on no
match (d1); its parse accepts `-q`/`-s` and prints the matched lines anyway,
where `grep -q` writes an empty file (d2); and the empty pattern matches the
phantom empty element `split('\n')` leaves behind, printing one line more than
bash (d3). `phantom_is_never_a_match` proves a non-empty pattern never reaches
that element, which is why the body law may model the input's real lines.
Fixing d1–d3 is a source change and therefore a later stage.

Declared out of model: the `printf` producer arm (its bytes are
`decode_capture_escapes`'s law), the fd/mode gate (cluster B's
`native_redirect.rs`), the line join from ranges to bytes (a renderer step over
the selection proved here), locale, and grep's stderr.

### Cluster G (part 1) — the constant-pipeline fold (16 obligations + 10 executed pins)

    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_constant_pipeline.rs
    -> verification results:: 16 verified, 0 errors

Cluster G is nineteen folds; this stage takes the one that owns a whole
statement and needs nothing but a status: `native_constant_pipeline` rewrites
`true/false | true/false …` to `status_exec(last stage == "true")`.

- **Two independent definitions.** The pass's side (`pass_decides`) is the
  universal gate over the stages' **syntax** — one statement, an exec/builtin
  Call, `true` or `false`, no words — plus a read of the LAST stage's *name*.
  The semantic side is stated over the stages' **behaviours** (bytes written,
  status exited) with bash's rule `pipeline_status = sts.last()`. They meet
  only through `stage_spec`, the definition of what the two builtins do.
  `fold_is_sound` proves the rewrite keeps bash's status and drops no output
  (`pipeline_stdout(outs).len() == 0`).
- **Which stage, and why it is load-bearing.** Shifting the read to
  `stages[0]` turns it red (12v/4e); so does making the semantic rule report
  `sts[0]` (11v/5e); so does dropping `status == is_true(s)` from `stage_spec`
  (15v/1e); so does weakening the gate to `is_call` only (10v/6e).
- **Pins, executed.** Ten rows, all MAPPED, all generated:
  `pin_specs/shir_native_constant_pipeline.json` names each witness and
  `gen_pin_tests.py` writes `src/transforms/cp_pin_tests.rs`, which builds the
  real IR and calls the real `native_constant_pipeline` — including the six
  refusals (opaque stage, worded builtin, empty pipeline, multi-statement
  body, non-Call body, non-pipeline shape).
- **d1, a recorded divergence.** `set -o pipefail` makes `false | true` exit 1,
  where the fold writes `status_exec(true)`. The IR carries no pipefail state
  in this fold, so there is nothing to gate on: the generated
  `pin_pipefail_disagrees` runs **both** bash forms and shows the disagreement,
  rather than the model asserting it.
- **Residue, stated not claimed.** bash's rule is modelled as
  `sts.last()`, not as an inductive execution fold. An earlier draft carried
  the fold (consume each stage's status in turn) and the induction "fold ==
  last"; Verus's per-function fuel makes that induction expensive at
  `--rlimit 40`, so the fold is the documented justification and the
  last-element rule is the model. What is proved is the part a pass can get
  wrong: which stage the status comes from, and whether that stage's status is
  what its name says.

Declared out of model: a stage's stdin, `true()`/`false()` shell functions
shadowing the builtins, `shopt -s lastpipe`, and the rest of cluster G (the
let/declare folds, `command -v`, trap, subshell and perl folds).

### Cluster G (part 2) — the `command -v` decision and its /dev/null gate (19 obligations + 11 executed pins)

    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_command_v.rs
    -> verification results:: 19 verified, 0 errors

`command -v NAME` asks whether NAME resolves through PATH. `shir_native_stmt.rs`
answers it in two places — `native_command_v` (statement position) and
`command_v_redirect_truth` (condition position) — from a hard-coded name list
plus one literal missing name, gated on the shape, the `-v` flag and the sink.

- **Two independent definitions, and the gap between them named.** The pass
  consults a LIST; bash consults PATH (`resolves(avail, name)` = membership over
  the reference PATH's programs). `status_law`/`cond_status_law` prove the fold's
  status equals bash's resolution *given* the list names programs that really
  resolve and the missing name really does not — and that hypothesis is
  **measured, not assumed**: generated pins run `command -v` for every name in
  the list (`pin_the_fold_claims_every_listed_name`,
  `pin_an_absent_listed_name_diverges`, `pin_the_missing_name_is_absent`).
- **The sink is load-bearing.** The rewrite prints nothing where bash prints the
  path, so the two agree on stdout only behind `> /dev/null`
  (`sink_law`). Hence `redirect_to_devnull` is not decoration, the condition fold
  refuses every other shape (`cond_refuses_every_non_devnull_sink`), and the
  statement fold refuses a file redirect (`stmt_refuses_a_file_sink`).
- **Mutants** (on sha `29fa8993181d`, each restored byte-identically): the known
  arm inverted 11v/8e; every name treated as known 14v/5e; the condition fold
  accepting any sink 18v/1e; bash modelled as always printing a path 18v/1e;
  `/dev/null` no longer swallowing 18v/1e.
- **d1 (recorded).** The statement fold also fires on the BARE `command -v NAME`,
  where bash prints the resolved path to stdout and the fold prints nothing. The
  generated `pin_bare_form_is_lossy` runs bash to show the dropped output; a fix
  needs a source gate, so it is a later stage.
- **d2 (found by the measurement).** The list names `python`, which this machine
  does not have (only `python3`), so `command -v python > /dev/null` exits 1
  while the fold writes status 0. `pin_an_absent_listed_name_diverges` pins the
  source's answer, and the generated test reports which listed names are absent
  here rather than asserting a machine-specific set.
- **Residue, stated not claimed.** The two source copies each repeat the same
  ~53-name list; that agreement is a *data* equality (two long disjunctions), so
  it is checked exhaustively by the generated `pin_the_two_copies_agree_…` (which
  also caught that the fold mutates its argument), not "proved" by unfolding 53
  string equalities.

Declared out of model: PATH order and shadowing, aliases and functions,
`command -v`'s output format for builtins and keywords, and extra redirect specs.

### Cluster G (part 3) — the `! command <'' >''` condition fold (25 obligations + 9 executed pins)

    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_command_noop_cond.rs
    -> verification results:: 25 verified, 0 errors

The only fold in `shir_native_stmt.rs` that rewrites a CONDITION into a constant.
It accepts `redirect(Call{builtin|exec, [Str "command", Array []]}, specs…)`,
optionally behind one `!`, and writes `Bool(not_wrapped)` — a statement about
the REDIRECT, since `command` with no arguments would itself succeed.

- **Two independent definitions.** `fold` is the pass's shape match;
  `run_status` is bash's order of operations — perform the redirects (an empty
  target can never be opened, status 1), then run `command` (status 0).
  `status_law` joins them where the redirect is statically decided, and
  `redirect_failure_dominates` / `no_specs_command_succeeds` state the shell side
  on its own. `the_empty_target_is_what_the_law_needs` shows the premise is
  tight: the same fold answer with no specs has status 0.
- **Refusals are conservatisms, not requirements.** A worded `command -v X`,
  another program, another launcher spelling, a redirect body that is not exactly
  one Call, a non-redirect condition, and a double `!` are all refused;
  `refusal_is_conservative` proves the refused empty-target case is already
  decided — the fold loses a rewrite, never correctness.
- **d3, recorded not fixed.** The fold never inspects the specs, so it answers
  questions about the filesystem: `if command > /dev/null` (bash TRUE, fold
  `false`) and `if ! command <$IN >$OUT`, the corpus' own `lex-dot-in-var.sh`
  shape (bash FALSE with openable files, fold `true`). Both are pinned
  (`pin_the_fold_ignores_the_target`, `pin_a_dynamic_target_is_a_guess`) and
  measured with bash; `the_fold_never_inspects_the_specs` is the missing guard.
  A later stage's fix is a source gate requiring every spec's target to be empty.
- **Mutants** (sha `df9ec95b9b5d`, each restored byte-identically): Not flag
  inverted 16v/9e; worded `command` accepted 22v/3e; any program accepted
  23v/2e; any launcher accepted 24v/1e; `decided_truth` inverted 18v/7e; a
  failing redirect reported as 0 20v/5e; a non-redirect condition folded 22v/3e.
- **Residue.** Specs are modelled by target only (the fold reads neither fd nor
  mode); `words` by its length; `builtin`/`exec` are one `Func::Direct`; the law
  requires at least one spec, since a zero-spec `redirect` Call is not a parse
  product.

## `shir_native_stmt.rs` — no single law; a decomposition plan (cluster C proved)

    proofs/verus/shir_native_stmt_plan.md

Measured this turn: 7140 lines, 105 top-level `fn`s, **60 fold entry points**
dispatched from `transform_stmt` (44 distinct in the `Expr` arm alone, 18 in
`Redirect`), and only **8 tests** in its `#[cfg(test)]` module — so 52 folds
have no source test and their pins will be model-level. The file is a union
of sixty independent rewrites over the whitelist command set; the ⇒ law that
the last five proofs share has no single instance here, only sixty, and one
of them (`record_file_write` → `native_cat_file_pipeline`) depends on
CROSS-STATEMENT state (`files: HashMap<String, String>`) that nothing
currently states.

The plan splits it into ten stages, each in the established shape (an
independent semantic side, the ⇒ law, load-bearing refusals as pins, measured
mutants, a `*_sync.txt`, a section here): C empty herestring → status first
(18–24 obligations, reuses `uae_terminators`' `run_abort`), then H the
quoting laws, E echo/printf bytes, B redirect writes plus the `files`
soundness invariant, D1–D3 the `test` renderer and chains, F1 tr/sort/wc,
F2 a declared BRE subset for grep, G the status/identity folds, F3 find/ls,
F4 xargs/comm/diff, and A the dispatch-coverage and `files` invariant as the
closing structural stage. Each stage's out-of-model list (renderers, the
`./fail-shir` metric, the whitelist itself, `lower_printf`, locales and
permissions, stderr) is in the plan and repeats in the stage's own section.

### Cluster C, proved: empty herestring → status

    proofs/verus/shir_native_herestring.rs      (20 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_herestring.rs

`empty_herestring_to_status` collapses `cmd … <<< ''` to `status_exec(ok)`.
The pass's analysis is transcribed as its own recursions (`plain`, `has_dyn`,
`has_c`, `dash_ok`, `one_lit_pattern`); the semantic side `bash` is written
from what `cat`/`tr`/`grep` do over empty stdin, with its own recursions
(`s_dash`, `s_plain`, `s_dyn`, `s_c`, `s_pattern`). The law
**`empty_herestring_emits_the_true_status`**: whenever `collapse` returns
`Some(ok)`, bash prints nothing and exits 0 (ok = true) or 1 (ok = false).

Pins, rejected by computation: bare `tr` (exits 1), `grep -c` (prints `0`),
a `grep` file operand, an empty pattern, an interpolated pattern, `cat FILE`,
a non-empty herestring, and any other command. Mutants, measured: dropping
the `-c`, file-operand, `cat`-operand, interpolated-pattern or bare-`tr`
refusal is rejected at simplification by `pin_refusals`; **accepting an
arbitrary dynamic word has no pin**, so the proof itself fails —
**19 verified, 1 error** at `lemma_dyn` (`has_dyn == s_dyn`), the refinement
bridge the law rests on. Six
mutants, each restored byte-identically (sha256 in
`shir_native_herestring_sync.txt`).

The other nine clusters of the plan remain **open** — this section proves C
only, not `shir_native_stmt.rs` as a whole.

### Cluster H, proved: quoting/escape round-trips

    proofs/verus/shir_native_quoting.rs        (19 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_quoting.rs

Five escape functions emit literals into five different target syntaxes:
`perl_quote` (a Perl double-quoted body), `tr_set_escape` (inside a Perl
`tr///` set), `decode_capture_escapes` (`$(printf …)` text),
`decode_echo_escapes` (`echo -e` output), and `regex_escape_literal` (a
grep BRE literal). Cluster H's law is the inverse of each, written as one
mode-parameterised theorem plus one citation per syntax:

- **`escape_roundtrip(mode, s)`**: `unquote(mode, quote(mode, s)) == Some(s)`
- `perl_double_quote_roundtrip`, `tr_set_roundtrip`,
  `captures_escape_roundtrip`, `echo_escape_roundtrip`,
  `bre_escape_roundtrip` — the same theorem instantiated per target.

The two sides are independent: `needs`/`key`/`quote` are the encoder (the
source's match arms), while `unquote` is a parser that reads only the
target syntax's `dec` table and never consults the encoder's decisions.
`key_is_inverse` is the single bridge (`needs(c) ⇒ dec(key(c)) = Some(c)`),
proved by cases; the round trip is then an induction that carries an
arbitrary suffix.

Pins, rejected by computation: a `$` in a Perl literal is escaped (it would
interpolate), a `"` is escaped (it would end the literal), a newline in a
`tr///` set is escaped (a raw one ends the operator at a physical line
break), `echo -e` and the capture decoder refuse an unknown escape instead
of guessing, every BRE metacharacter is escaped, and plain letters pass
through every syntax unchanged. Mutants, measured: decoding `\n` as a tab,
decoding `tr`'s `\0` as `O` (**18 verified, 1 error** each), and accepting
an unknown `\q` (**18 verified, 1 error** at
`pin_echo_unknown_escape_refused`); dropping the encoder's tail, dropping
the `$` escape, or refusing every BRE escape is rejected at simplification
by the pins. Six mutants, each restored byte-identically (sha256 in
`shir_native_quoting_sync.txt`).

What this does **not** claim: the Perl literal's surrounding quotes are the
renderer's contract (the body is what is proved); `render_test_text` and
`render_flag_substitution_test` are renderers whose law is cluster D's test
grammar, not an escape table; `static_pipeline_truth` belongs to cluster F2;
and the other 51 folds remain open. The module's 8 tests do not cover these
functions, so every pin row is UNMIRRORED by design and no `[exec]` PASS row
exists. The remaining clusters (E, B, D, F, G, A) are still open — this
section lands H, not `shir_native_stmt.rs` as a whole.

### Cluster E, proved: echo/printf byte equivalence

    proofs/verus/shir_native_echo.rs           (26 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_echo.rs

`native_echo_stmt` and `native_printf_stmt` replace a command with an
`Output` node carrying literal bytes. Two laws:

- **`echo_emits_the_same_bytes`**: the emitted `(text, newline)` equals what
  bash's echo writes. The pass side transcribes the source pipeline — only
  the FIRST word can be a flag (`-n`/`-e`/`-E`), `-e` decodes every word,
  empty words are dropped, words are joined with one space. The semantic
  side computes the flag tuple, the argument bytes and the trailing newline
  independently.
- **`printf_emits_the_same_bytes`**: the source's index loop over the format
  (`%s`, `%%`, `\n \t \r \\`, exact argument count) agrees with a slice
  recursion written from printf's format language; the bridge
  `printf_index_is_slice` proves the two recursions agree at every index.

Pins, rejected by computation: `-n` clears the newline; `-e` decodes while
`-E` does not; an unknown leading-dash flag is refused; a glob word keeps the
runtime path; printf adds no newline implicitly; extra (or missing)
arguments are refused; `%d` and any conversion outside `%s`/`%%` is refused.
All five of the plan's mutants are refuted at simplification by those pins
(keeping the newline, decoding without `-e`, accepting an unknown flag,
treating a glob as literal, ignoring extra printf arguments) — none reaches
the verifier, so this cluster has no counted mutant. Each restored
byte-identically (sha256 in `shir_native_echo_sync.txt`).

Domain and exclusions, stated rather than hidden: the echo law excludes a
later word that starts with `-` (the fold inspects only the first; upstream
word shaping is expected to have handled it) and interior empty words (the
source drops them as continuation artifacts). `printf -v` is an assignment
plus status and belongs to cluster G; the `echo` escape table is cluster H's
proved syntax and is reused rather than re-claimed; the renderer that turns
`Output { newline }` into backend bytes, bash stderr and locales are out of
model. The module's 8 tests do not cover these folds, so every pin row is
UNMIRRORED by design. Clusters A–B and D, F, G remain open.

### Cluster B, proved: redirect writes

    proofs/verus/shir_native_redirect.rs       (21 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_redirect.rs

`native_echo_write_file_stmt` replaces `echo/printf … > FILE` with native
Perl that writes literal bytes; `record_file_write` records the same content
for later `cat FILE | …` evaluation; `file_redirect_block` only wraps the
exec (a renderer fallback trigger, modelled here as an identity effect).

- **`redirect_writes_the_same_bytes`**: whenever the write fold fires, the
  `(path, bytes, mode)` it emits is exactly bash's file effect — the
  command's stdout at the literal target, in the redirect's mode. The pass
  side transcribes the source: one spec, fd 1, `w|wc`, literal target,
  echo/printf, the echo loop's `-e`/`-E`/`-n` handling and single content
  word, printf's single `%`-free word. The semantic side computes the
  shell's leading-option stdout and the file's new bytes independently;
  `parse_echo_agrees` proves the fold's anywhere-flags scan matches the
  shell's leading scan under the law's domain.
- **`append_mode_appends`**: applying a write in mode `a` gives
  `old ++ bytes`; `w`/`wc` give `bytes`.

Pins, rejected by computation: a dynamic target, fd other than 1, a second
redirect spec, append in this (write-only) fold, a non-echo/printf command,
printf with a `%` conversion, `echo -n`'s missing newline and `echo -e`'s
decoded bytes. Mutants, measured: treating append as a truncating write is a
counted failure (**20 verified, 1 error** at `append_mode_appends`); wrapping
a dynamic target, ignoring fd, keeping `-n`'s newline and accepting a `%`
printf are refuted at simplification by the pins. Each restored
byte-identically (`shir_native_redirect_sync.txt`).

Exclusions, stated: the `files` map's cross-statement invariant is cluster
A's; `native_sort_file_stmt`, the heredoc writers and `emit_file_op` write
through different command models and are not claimed here; glob words are
outside the domain and the source has no glob gate in this fold (unlike
`native_echo_stmt`); fd 0/2, `2>&1`, heredocs/herestrings and renderer
output are out of model. The module's 8 tests do not cover these folds, so
every pin row is UNMIRRORED. Clusters A, D, F and G remain open.

### Cluster D1, proved: the atomic `test` round trip

    proofs/verus/shir_native_test_render.rs      (15 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_test_render.rs

`render_test_tokens` turns a tokenised test into an IR expression. D1's law
is the round trip `eval(parse(render(t))) = eval(t)` for its atomic shapes:
one token (truthiness), a two-token unary test (`-e -f -d -s -S -h/-L -a -z
-n`) and a three-token binary test (`=`/`==`/`!=`, the six numeric
comparisons, `>`/`<`).

- **`render_tokens_is_bash`**: whenever the renderer accepts a token list,
  `eval_expr` of the emitted expression equals bash's `test` verdict for the
  same tokens in the same world. The pass side transcribes the 1/2/3-token
  arms into `Expr`; the semantic side is `bash_atom`, bash's own rules
  (truthiness, unary predicates over a `World`, string equality, numeric
  numification, lexicographic ordering), written independently of `Expr`.
- **`test_renders_to_the_same_truth`** restates it as the plan's law.

Pins, rejected by computation: a bare word is truthiness; `-f` is a file test
(not the flag-regex heuristic); `-a` is existence; `-z`/`-n` are length
tests; numeric comparison numifies non-numbers to 0; `=` is string equality;
an unknown unary flag is refused. Mutants, measured: making any dash word on
the unary arm truthiness is a counted failure (**11 verified, 4 errors**);
accepting an unknown unary as `-f` is counted (**12 verified, 3 errors**);
swapping `-z`/`-n` and rendering a bare word as always-true are counted
(**13 verified, 2 errors** each); rendering `-eq` as string equality is
rejected at simplification by `pin_numeric_numifies`. Each restored
byte-identically (`shir_native_test_render_sync.txt`).

Exclusions, stated: `render_test_text`'s top-level connector split (`&&`,
`-a`, `||`, `-o`) is **cluster D2/D3**, not claimed here — its split order
does not match bash's precedence when connector kinds are mixed, so its law
needs a domain premise and its own proof. `render_flag_substitution_test`
(`$(...)`) is a runtime capture; parentheses and `!` are the shared parser's;
globs, `=~`, `-ef` and `$var` operands are outside this round trip; renderer
output text is out of model. The module's 8 tests do not cover this
renderer, so every pin row is UNMIRRORED. Clusters A, D2/D3, F and G remain
open.

### Cluster D2, proved: the chain folds

    proofs/verus/shir_native_chain.rs             (18 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_chain.rs

The three chain rewrites turn `test … && …`, `(test … && …) || …` and pure
command `&&` chains into `If`/`Block` structures. D2's laws compare the
structure the pass emits against bash's own status/effect semantics:

- **`test_and_to_if_law`**: `test b && THEN` → `If b THEN` (no else) keeps
  the status and the executed-command sequence; a false test's non-zero
  status survives because no else runs.
- **`test_chain_to_if_law`**: `(test b && THEN) || ELSE` → `If b THEN ELSE`
  when THEN is always-success. This is where the fold's always-success gate
  is load-bearing: a fallible THEN would make bash run ELSE on failure while
  the If form would not.
- **`cmd_and_chain_to_if_law`**: a command chain `e0 && e1 && …` → the fold's
  `e0; If ($? == 0) { e1; If ($? == 0) { … } }`, with the same status and
  the same executed-command sequence.

The semantic side (`bash_status`/`bash_trace`/`bash_test_and`/`bash_test_chain`)
computes bash's `&&`/`||` behaviour directly; the pass side builds the
If/Block structures (`build_chain`, `pass_test_and`, `pass_test_chain`) and
gives them the backend's semantics (`status`/`trace`), including the
distinction between a `$? == 0` read (writes no status, so an empty else
keeps the command's own) and a `test` condition (writes 0/1).

Pins: a fallible THEN under `||` diverges; a false condition with no else
keeps status 1; `cd` is not chain-native; a pure-boolean lead is refused;
a mixed `&&`/`||` chain is refused. Mutants, measured: dropping the
always-success gate, testing `true` instead of `$? == 0`, accepting `cd`,
accepting a pure-boolean lead, and dropping the else branch are counted
failures (17 verified, 1 error each); the mixed-op gate mutant is rejected
before verification because the pin's fuel reveal no longer applies. Each
restored byte-identically (`shir_native_chain_sync.txt`).

Exclusions, stated: `redirect_call_to_stmt` and
`flatten_redirect_status_block` are structural identities here; the static
pipeline branch (which keeps the pipeline running ahead of the taken branch)
is out of model; the renderer half — `render_test_text`'s connector split,
`!`, globs, `-a`/`-o` in `[[ ]]`, an unset variable under `set -u`, a
dynamic word — is D1/D3, not claimed; D1's atomic round trip is reused;
cluster A's `files` invariant and backend status rendering are out of model.
Clusters A, D3, F and G remain open.

### Cluster D3, proved: the if/while condition families

    proofs/verus/shir_native_cond.rs               (10 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_cond.rs

Plan §D assigns D3 the if/while condition rewrites. The laws compare what
the emitted Perl does to the two status variables against bash's own
short-circuit behaviour:

- **`if_test_cond_law`**: `test COND && B` becomes a double status write
  (`$main_exit_code = $CHILD_ERROR = COND ? 0 : 1`) followed by the
  conditional body. A false test writes 1 and skips B; a true test runs B,
  whose own status wins — equal to bash's `&&` on both variables.
- **`test_exit_stmt_law`**: `test COND || exit N` becomes
  `unless (COND) { exit N }; write 0;`. A true test leaves 0 and continues;
  a false test exits with N, skipping the trailing write.
- **`while_test_cond_law`**: the natively rendered guard reads the loop
  variable from an environment (not its name) and decides every one of the
  six integer comparisons exactly as bash does.

Pins: a failing body still wins on the true path; omitting the trailing
reset leaves a stale status; negating the unless guard exits wrongly; the
guard reads the environment; an unknown operator is refused. Mutants,
measured: writing only `$CHILD_ERROR` and inverting the condition status are
counted (**8 verified, 2 errors**); dropping the trailing reset, negating
the unless guard, rendering `-lt` as `<=`, and treating the variable as a
literal are counted (**9 verified, 1 error** each). Each restored
byte-identically (`shir_native_cond_sync.txt`).

Exclusions, stated: `native_while_multi_cond`'s loop restructure is **not**
proved — its equivalence needs the echoed output and a loop-invariant guard,
stated as unproved rather than assumed. `render_test_text`'s flat connector
split, `!`, parentheses, globs and `-a`/`-o` in `[[ ]]` remain renderer
work; D1's atomic round trip and D2's chain laws are reused, not re-claimed;
`render_flag_substitution_test` is out of model; the backend status registers
and byte renderer are out of model. The module's 8 tests do not cover these
rewrites, so every pin row is UNMIRRORED. Clusters A, F and G remain open.

### Cluster D3, completed: the multi-condition `while`

    proofs/verus/shir_native_while_multi.rs        (9 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_while_multi.rs

`native_while_multi_cond` restructures `while { echo LIT; test COND } do BODY`
into `while (1) { echo LIT; if (COND) {} else { $main_exit_code =
$CHILD_ERROR = 0; break } BODY }`. `multi_cond_loop_law` proves the same
final status and the same per-iteration echo/body events:

- **semantic side** `bash_loop` runs bash's own while: echo then test each
  iteration, body only on true, status 0 when the test ends the loop, the
  body's status on a body break.
- **pass side** `run_em`/`pass_loop` interpret the emitted structure: echo
  first, the guard's false arm resets both status variables and breaks, the
  body writes its status and may break.

The guard-stability invariant (`GUARD_STABLE`: the body does not change what
the test reads) is an explicit hypothesis; it is preserved because the same
guard is passed to every iteration and sufficient because the false arm stops
the loop. Pins: a failing test exits 0, a body break keeps the body's status,
dropping the Reset leaks a stale status, the echo runs on the failing
iteration too, and a non-literal-echo prefix is refused. Mutants, measured:
dropping the Reset (**6 verified, 3 errors**), inverting the guard (**5v/4e**),
moving the echo after the test (**8v/1e**), dropping the body's status write
and ignoring a body break (**7v/2e** each). Each restored byte-identically
(`shir_native_while_multi_sync.txt`).

Exclusions, stated: echoed bytes and the byte renderer are out of model; a
body that mutates the tested variables is out of domain and reported; the
flat connector split, `!`, parentheses, globs and `-a`/`-o` in `[[ ]]` remain
renderer work; D1's atomic round trip and D3's other three functions are
reused, not re-claimed; D2's chain laws are not re-claimed. **Clusters A, F
and G remain open.**

### Cluster F1, partly proved: `tr` / `wc -l` / `head -c` / `tail -c`

    proofs/verus/shir_native_f1.rs                (18 verified, 0 errors)
    verus --triggers-mode silent --rlimit 40 proofs/verus/shir_native_f1.rs

F1 groups five filter commands; this lands the text-model parts that can be
proved now:

- **`tr_join_law`**: translating each producer line and re-joining is the
  same text as bash's `tr` over the joined stream. **`tr_line_count_law`**:
  translation preserves the line structure. Domain hypotheses, stated: the
  mapped character is not the newline separator (`a != '\n'` — the fold maps
  line contents while bash would also map separators), and every line
  carries no newline of its own.
- **`wc_l_law`**: `native_wc_l_pipeline`'s `lines().count()` equals bash's
  `wc -l` newline count on the fold's newline-terminated producer.
- **`head_bytes_law` / `tail_bytes_law`**: `native_head_tail_stmt`'s
  `-c N` first/last-byte windows with their length clamp match bash.

Pins: `tail -c` clamps to the whole input, `head -c 0` is empty, `tr` leaves
characters outside its set alone, `wc` needs one newline per line. Mutants,
measured: skipping the translation (**17v/1e**), an off-by-one newline count
(**15v/3e**), head taking the suffix (**17v/1e**), and tail clamping to empty
(**16v/2e**) are counted; dropping join's later lines is rejected before
verification because `join_cons`'s fuel reveal no longer applies. Each
restored byte-identically (`shir_native_f1_sync.txt`).

**Not proved, stated:** `sort`'s plain ordering and its `-n`/`-r` refusals
(needs an ordering/permutation uniqueness model), the line-mode
`head -N`/`tail -N` split and joined trailing newline, and
`native_literal_subshell_wc_stmt`'s literal grep/sed tree (F2). Locales,
multibyte cutting and the byte renderer are out of model; D1–D3 are reused,
not re-claimed. Clusters F2–F4, G and A remain open.

#### F1 continued: plain `sort` and the line-mode windows

    proofs/verus/shir_native_f1.rs                (47 verified, 0 errors)

- **`plain_sort_law`** — `native_sort_stmt`'s plain branch splits the
  producer, drops a trailing empty segment, sorts and re-joins. Two
  independent properties of that output are proved: it is nondecreasing
  under a byte-lexicographic order (`sorted`/`sorted_at`, via the pass's
  insertion sort `insert`/`isort`), and it is a permutation of the input
  (`same_multiset`, counting each line). The semantic side is those two
  relations, never a second sort. `-n`/`-h` use a different comparison and
  stay out of model; unknown flags (`-u`, `--reverse`, `-k2`, a file
  operand) are refused by `plain_sort_flag`, pinned. Note the plan expected
  the source to refuse `-n`/`-r`; it implements both, so `-r` is a
  plain-order flag here and the pins cover what is really refused.
- **Line-mode `head -N` / `tail -N`** — the fold's split/join
  (`split_nl`/`join_lines`/`source_lines`) is modelled and its emitted bytes
  are pinned, but the byte-level equality with bash's line copy is **not
  proved**: `head -N` past the last line appends a stray newline, `tail -N`
  on a terminated producer selects the trailing empty segment and prints
  nothing, and `tail -N` on an unterminated producer adds a newline bash
  does not. Those are source divergences, recorded as pins rather than
  assumed away.

Second mutant measurement (47v/0e baseline): insert comparing backwards
(**42v/5e**), isort dropping its head (**45v/2e**), `plain_sort_flag`
accepting `-n` (**46v/1e**), `source_lines` omitting its newline
(**44v/3e**), `join_lines` adding a trailing newline (**43v/4e**). All
restored byte-identically.

#### F1 line mode: the general window law

`head_window_law` and `tail_window_law` now prove, for an arbitrary line
list and an arbitrary `N`, that bash's byte-level line window equals the
fold's `source_lines` of the selected lines — on the explicit domain
`nl_free_lines(ls)` (no line carries a newline) and `nonempty_lines(ls)`
(an empty selected line makes the fold print nothing where bash prints a
blank line). The bash side steps the byte stream (`take_lines`/`skip_lines`);
the fold side is `join_lines`/`source_lines` as the source writes it. The
proofs step with `subrange(1, len)` rather than `drop_first`, which is not
usefully specified on the empty sequence in this toolchain.

Off domain the general selection statement is `tail_terminated_selection` (the
fold's `tail -N` selection on a newline-terminated producer is the last `N-1`
real lines plus the trailing empty segment), and the four pins
(`pin_head_one_line`, `pin_head_beyond_the_last_line_adds_a_newline`,
`pin_tail_lines_selects_the_trailing_empty_segment`,
`pin_tail_lines_add_a_newline_bash_does_not`) record the `N = 1` witnesses.
The general-in-`N` off-domain theorems beyond that selection identity are
proved in the next section.

Third mutant measurement (62v/0e baseline): `join_lines` adding a newline to a
one-line selection (**55v/7e**), `take_lines` not decrementing (**61v/1e**),
`nonempty_lines` admitting empty lines (**59v/3e**), `source_lines` omitting
its newline (**56v/6e**), `skip_lines` not decrementing (**61v/1e**). All
restored byte-identically.

#### F1 line mode: the off-domain divergences, general in N

The divergences the earlier pass only pinned are now proved for an arbitrary
`N` over an explicit witness family: a newline-free, non-empty line list
`ls`, with producer text `join_lines(ls) + "\n"` (newline-terminated) or
`join_lines(ls)` (unterminated).

* `head_past_end_diverges` — for `N >= ls.len() + 1`, the fold's window on
  the terminated producer followed by one newline is the whole selection
  text: bash prints the text once, the fold terminates it twice.
* `skip_lines_unterminated` + `tail_unterminated_diverges` — bash's
  drop-`(len-N)` on the unterminated producer is the last `N` lines' text
  with no final newline; the fold's output is that text plus a newline.
* `tail_terminated_selection` (already general in `N`) records the
  trailing-empty-segment selection on the terminated producer, which is why
  the fold's `tail -N` prints fewer real lines than bash's.

The supporting inductions are `nl_count`, `nl_count_concat`, `nl_count_nl`,
`nl_count_join`, `no_nl_text_zero` (the terminated producer carries exactly
`ls.len()` newlines), `take_lines_whole`, `join_snoc_empty`,
`skip_lines_unterminated`, `nonempty_lines_subrange` and
`nonempty_join_len`. No bridging lemma assumes the conclusion: the bash side
(`take_lines`/`skip_lines` over the byte stream) and the fold side
(`join_lines`/`source_lines`) are separate recursions, and every step is
`subrange`, never `drop_first`. The narrowed domain stays explicit — a first
empty line would make the fold print nothing where bash prints a blank
line — and the N=1 pins remain as witnesses.

Fourth mutant measurement (74v/0e baseline), all counted and restored
byte-identically (`sha256` re-checked): `join_snoc_empty` dropping the
appended newline (**72v/2e**), `take_lines_whole` accepting
`n == nl_count` (**73v/1e**), `skip_lines_unterminated` claiming a trailing
newline (**72v/2e**), `tail_unterminated_diverges` dropping the fold's
newline (**73v/1e**), `head_past_end_diverges` dropping the fold's extra
newline (**73v/1e**). A sixth mutant (`nl_count` always zero) is refuted at
simplification: `reveal_with_fuel` rejects a non-recursive function.
