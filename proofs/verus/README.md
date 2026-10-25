# Verus proof of the interval estimator (`i32_provable`)

STATUS: **checked.** Verus `0.2026.10.18.7d82994`, rust toolchain
1.99.1:

    verus --triggers-mode silent --rlimit 40 interval.rs
    -> verification results:: 5 verified, 0 errors

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
  is the separate obligation in `C_UB_PROOF.md`.
- `i32_provable` has no renderer consumer in the tree today (checked:
  `is_i32()` is referenced only from `transforms.rs`), so this verdict
  currently changes no emitted byte.

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
- Coverage sync registered (`verus-sync-coverage.sh:string_folds`).

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
- Coverage sync registered (`verus-sync-coverage.sh:arith_folds`);
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
- Coverage sync registered (`verus-sync-coverage.sh:test_folds`).

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
- Coverage sync registered (`verus-sync-coverage.sh:capture_folds`).

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
- Coverage sync registered (`verus-sync-coverage.sh:list_folds`).

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
- Coverage sync registered (`verus-sync-coverage.sh:textop_folds`).

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
- Coverage sync registered (`verus-sync-coverage.sh:not_folds`).

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
- Coverage sync registered (`verus-sync-coverage.sh:const_cmp_folds`).

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
- Coverage sync registered (`verus-sync-coverage.sh:elim_trio`).

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
- Coverage sync registered (`verus-sync-coverage.sh:slice_identity`).

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
- Coverage sync registered (`verus-sync-coverage.sh:case_lowering`).

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
- Coverage sync registered (`verus-sync-coverage.sh:elseif_flatten`).

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
- Coverage sync registered (`verus-sync-coverage.sh:seq_range`).

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
- Coverage sync registered (`verus-sync-coverage.sh:ternary_desugar`).

## Running it

Same toolchain as the P6/P7/P8 files (`--rlimit 40`).
