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
  in the same effort — see below). Proving the pre-fix code is
  impossible: the mutants above are the machine-checked witnesses.
- The three fixed holes (`redundant-store-elim: fix 3 soundness
  holes`, same effort):
  1. `expr_pure` claimed `Arith(_) => true`; `ArithAst::Assign`/
     `IncDec` mutate state (`x=$((i++))` dropped the increment).
     Now a dedicated `arith_pure` mirroring dead_store_elim's.
  2. `stmt_reads` on `Assign` ignored target indices
     (`x=5; arr[x]=1; x=6` dropped `x=5`). Indices are reads now.
  3. `$name` inside plain-`Str` call args was invisible
     (`x=1; y=$(eval 'echo $x'); x=2` dropped `x=1` — confirmed with
     a failing probe before fixing). New `str_reads_var` mirroring
     dead_store_elim's `string_read_names`, wired into the `Call`
     arm and the bare-`Str` arm.
  Each has a failing-then-passing Rust test in the transform's own
  `tests` module; the full `--lib` suite shows the same 20
  pre-existing failures before and after (unrelated backends).
- NOT modelled: `Ext`-node field reads (another worker's uncommitted
  WIP adds an `IrExpr::Ext` arm to the real `expr_reads` at the time
  of writing — out of scope here, keep-direction so the model stays
  the aggressive side), case-pattern text reads, `[*]`-reads-IFS,
  whole-store-dumping builtins inside captures (env-dump shapes),
  nested-statement recursion (the real pass applies the same scan
  per function body — same lemma per body), multi-target assigns.
- The drop rule is stated declaratively (exists a clean justifier);
  the file header records the scan ⟺ declarative equivalence
  argument, including the justifier-cleanliness clause the scan's
  check order implies.

## Running it

Same toolchain as above (`--rlimit 80` for the shadow-induction
obligations):

    RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo \
      /path/to/verus-x86-linux/verus --triggers-mode silent --rlimit 80 proofs/verus/redundant_store_elim.rs
