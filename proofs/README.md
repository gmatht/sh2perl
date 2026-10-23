# proofs/ — machine-checked claims (Isabelle)

## DivModPow2 — `x / 2^n ⟶ x >> n`, `x % 2^n ⟶ x AND (2^n − 1)`

STATUS: **checked**. Isabelle2025-2, built from scratch (`-c`) with
`quick_and_dirty = false`, so no `sorry`/`oops` can be hiding in it.
Runtime ~10 s (HOL image already present).

    isabelle build -c -o quick_and_dirty=false -D proofs

The claim, over `int`, non-negative dividend, `n ≥ 1`:

    0 ≤ x ⟹ 1 ≤ n ⟹ x div 2 ^ n = x >> n ∧ x mod 2 ^ n = x AND (2 ^ n − 1)

### What is proved in `DivModPow2.thy`

- **div_mod_pow2** — the claim as stated (§4).
- **shift_eq** — the shift conjunct, by the induction that was sketched
  for it (§2): `shift_step` (`x >> Suc k = (x div 2) >> k`) and
  `div_halves` (`(x div 2) div 2^k = x div 2^(Suc k)`), the second applied
  to `x div 2` — which needs `0 ≤ x div 2`, which needs `0 ≤ x`.
  `half_nonneg` is used exactly there: on the recursive call, not on the
  statement. That is where the hypothesis earns its keep.
- **shift_eq_floor**, **shr_eq_drop_bit** — the honest bookkeeping around
  it. Over Isabelle's *floor* `div` the identity holds for every `x`,
  guard or no guard (floor division already is the arithmetic shift), and
  `>>` is tied to the library's `drop_bit`, so the notation in the claim
  is not an unchecked private definition.
- **mask_eq_mod** — the mask conjunct by bit extensionality (`bit_eqI`).
  Hypothesis-free over HOL's Euclidean `mod`: `mod` and two's-complement
  `AND` select the same low bits for every `int`.
- **tdiv_nonneg, tmod_nonneg, c_shift_eq, c_mask_eq** (§5) — the same two
  rewrites for what the shell actually computes: C99 6.5.5p6 truncating
  `/` and `%`, modelled by `tdiv`/`tmod`. Here the guard is load-bearing
  and both lemmas carry it.
- **neg_control_shift, neg_control_mask, neg_control_c_mask,
  guard_is_load_bearing** — machine-checked negative control at `x = -3`:
  `-3 / 2 = -1 ≠ -2 = -3 >> 1`, `-3 % 2 = -1 ≠ 1 = -3 AND 1`, and the
  existentially quantified form. The counterexamples are *proved*, not
  asserted: a proof of the claim without the guard would be proving the
  wrong statement. (The mask conjunct's guard is about C's `%`, not about
  `int`'s `AND` — that is what `neg_control_c_mask` isolates.)

### Scope and notes

Tier 1 — unbounded `int` deliberately (a bound that holds without wrapping
holds for every fixed-width execution); that the *target* does not wrap is
a separate obligation (`C_UB_PROOF.md`), not discharged here. Pure HOL, no
AFP, no word-splitting/shell semantics.

Two build details: `AND`/`OR`/`XOR` notation lives in a bundle, so the
theory starts with `unbundle bit_operations_syntax`; `quick_and_dirty =
false` is set in `ROOT`, so an `isabelle build -D proofs` that succeeds is
a proof.

## I32Provable — the `i32-provable` verdict, sound and refuted-then-fixed

STATUS: **checked**. Same build (both sessions build from one command):

    isabelle build -c -o quick_and_dirty=false -D proofs

The obligation, for the analysis-only transform
`sh2perl/src/transforms/i32_provable.rs` (Tier 1 per
`docs/transform-verification.md` — verdict ⟹ fact, IR-shaped data only):

    carries s ⟹ abs (stmt_value s) < 2 ^ 31

### What is proved in `I32Provable.thy`

- **§1–2, the negative control kept in the file.** `is_i32_v0` mirrors
  the estimator as it stood on 2026-10-23 (leaf bounds only, every op
  `+ - *`), and `boom` (2^29·2^29·2^29, three legal literals, value
  2^87) is machine-checked **marked I32 and out of range**:
  `boom_marked_v0`, `boom_overflows_v0`, and `v0_has_false_positive` —
  the manifest's old invariant line, machine-checked false. The session
  fails to build if anyone restores the leaf-only check.
- **§3, the fixed estimator.** `abs_bound` mirrors `arith_abs_bound`
  verbatim after the fix: every node composes its children's bounds and
  `cap` refuses as soon as the composed magnitude would reach 2^31.
- **cap_Some** — the wall, inverted: `cap c = Some b` gives `b = c ∧
  c < 2^31` exactly, so each branch can turn a cap verdict back into an
  equation plus the wall fact.
- **abs_bound_sound** — the invariant, by induction on the tree: a
  proved bound is non-negative, clears the wall, and bounds the
  compound's value. Every branch derives its equations (children's
  bounds, the cap's verdict, the operator's identity) as named facts
  before closing with ordered arithmetic; the closing steps are
  `abs_triangle_ineq`/`abs_triangle_ineq4`, `add_mono`, `mult_mono`,
  `max.cobounded1/2` over the child invariants.
- **is_i32_ex, i32_provable_sound, i32_provable_node_sound** (§5) — the
  verdict as the transform publishes it: statement-level (`carries s`)  and node-level (`is_i32 e`), both `abs (value) < 2^31`.

### Scope and notes

Unbounded `int` (the mirror is `i128` in Rust): a mathematical bound
that holds without wrapping covers every fixed-width execution; that the
target does not wrap is `C_UB_PROOF.md`'s obligation. Refusals only ever
mark FEWER nodes, so the fix cannot turn a correct verdict wrong.

Two Isabelle facts worth knowing before editing this file:
symbol characters (`⟹`, `≤`, `∧`, `≠`) do **not** lex inside quoted
lemma statements in this installation — every formula uses the
`\<Longrightarrow>`/`\<le>`/… ASCII escapes, as the Isabelle sources do
(these build failures show up as `Inner lexical error`); and `value` is
an outer command keyword, so the statement-value function is spelled
`stmt_value` (an `fun value …` header fails as `name expected, but
end-of-input`).

As with DivModPow2: not scanned by `harness/transform-proofs.sh`, not
committed — this README is what claims it. The Rust side carries the
matching tests (`cargo test --lib transforms::i32_provable`, 3 green:
the 2^87 compound refused, the in-range compound still marked, the
cap boundary exact).

## BigIntRange — the `big_arith_range` pass, sound and refuted-then-fixed

STATUS: **checked**. Same build (all three sessions, one command):

    isabelle build -c -o quick_and_dirty=false -D proofs

The obligation for `sh2perl/src/shir_passes/bigint_range.rs` (Tier 1,
expression-level — the statement walk is `⟦stmt⟧`, which is Tier 3 and
stays out of this file; `IrExpr::Arith` delegates to the expression
walk, so the theorem covers what the walk publishes):

    env_sound ρ ν ⟹ range = Some (lo, hi) ⟹ eval ν a = Some v ⟹ lo ≤ v ∧ v ≤ hi

`eval` is the *Python*-int reading the pass certifies under (HOL's `div`/
`mod` on `int` are floor-based, probe-pinned: `-7 mod 3 = 2`,
`6 mod -4 = -2`, matching Rust's `floor_mod`/`floor_div`). The
`env_sound` premise is the state's own contract — without it nothing
relates the range environment to the value environment, and the variable
 case is unprovable, not merely unproved.

### What is proved in `BigIntRange.thy`

- **§1–4, the analysis and its pre-fix twin.** `big_range` mirrors the
  fixed `big_arith_range` verbatim; `v0_range` mirrors the 2026-10-23
  arms (corner min/max for `%` and `//`, zero-endpoint guards only, no
  float check on `Cast`).
- **The negative control: `v0_not_sound`**, built from three
  machine-checked closed counterexamples — `ex_mod_corners` (29 % 30
  lands outside the corner claim (0,10) over dividend [0,100] — the
  DivModPow2 class), `ex_div_straddle` (6 // −1 = −6 outside the corner
  claim (−2,2) over divisor [−5,3]), `ex_mod_negative` (100 % −4 = 0
  outside (−8,−2) over divisor [−9,−3]). `ex_cast_float` isolates the
  third gap honestly: v0 names a range for a float cast the model reads
  as `None`, so that gap has no counterexample *value* and is mirrored
  as refusal + a fixed-vs-v0 fact instead.
- **mult_corners, div_corners** — the corner bounds, each proven with
  the sign case split the Rust now performs: pick the bound's own
  factor by the *other* factor's sign, then the corner by that factor's
  sign (`mult_left/right_mono(_neg)`); division uses `zdiv_mono1` for
  the dividend and `zdiv_mono2(_neg)` for the divisor — all of which
  need a **positive** divisor range, exactly the restriction the fixed
  `//` arm imposes (`r0 > 0`, else refuse).
- **mod_pos_bounds, mod_neg_bounds** — floor-mod by the sign of the
  divisor with *no dividend bound* (the property the old comment
  claimed and its corner loop lacked): `0 < r0` gives `[0, r1 − 1]`,
  `r1 < 0` gives `[r0 + 1, 0]`, via `pos/neg_mod_bound`,
  `pos/neg_mod_sign` (all `[simp]`) plus two `presburger` integer
  steps (linear arithmetic stops at ℚ; `x < y ⟹ x + 1 ≤ y` needs the
  discreteness of `int`).
- **bigint_range_sound** — the obligation, by induction on the tree;
  the `Cond` arm hulls both arms and uses only the taken arm's IH, the
  `**` point arm pins `v1 = l0 ∧ v2 = r0` before its simp, and the
  comparison arms land in `{0,1}` ⊆ `[0,1]`.
- **fix_handles_the_examples** — the fixed pass on the same inputs:
  the `%` interval widens to the sound sign-of-divisor bound, the
  straddle and the float cast refuse.

### Scope and notes

Unbounded `int` throughout, as in the other two sessions. The Rust fix
landed with the proof: the `%` arm is point/sign-of-divisor (corner
loop removed), `//` refuses every non-positive divisor range, a
float-typed `Cast` refuses — with three regression tests
(`cargo test --lib shir_passes::bigint_range`, 11 green). Refusing or
widening a range never makes a claimed interval wrong; the merge with
the i128 walk and the width/veto inputs are untouched.

Isabelle mechanics worth keeping (they cost this round): `Un` is a HOL
keyword (set union), so the constructor is `UnOp` — the same rename
`I32Provable` needed; `case foo: True` colon-renaming is the way to
distinguish nested boolean cases; a premise-position `if` needs
`split: if_split_asm` (plain `if_split` only splits goal position); and
an `and`-lemma whose statements carry their *own* inner premises
(`shows "A ⟹ …" and "B ⟹ …"`) does not compose with `OF` like a single
statement — the mod lemma is two lemmas (`mod_pos_bounds`,
`mod_neg_bounds`) for that reason.

As with the others: not scanned by `harness/transform-proofs.sh`, not
committed — this README is what claims it.

## IntervalFloor — the i128 floor walk of `interval.rs`, sound and
refuted-then-fixed

STATUS: **checked**. All four sessions green under one command,
`quick_and_dirty = false` (so no `sorry` can hide), IntervalFloor and
its parent BigIntRange re-checked from source this round:

    isabelle build -c -o quick_and_dirty=false -D proofs

The obligation for `sh2perl/src/shir_passes/interval.rs` — the Python
(floor-modulo) transfer functions `arith_floor_range` /
`floor_arith_range`, the single verdict `ctx.var_ranges` every backend
reads (Tier 1, expression-level; `refine_floor_mod`'s statement
traversal stays Tier 3, as `big_ir_range`'s did):

    env_sound ρ ν ⟹ top_range a ρ = Some (lo, hi) ⟹ eval ν a = Some v ⟹ lo ≤ v ∧ v ≤ hi

The session *imports* BigIntRange (`imports BigIntRange.BigIntRange`
plus the `= BigIntRange +` parent link in ROOT) and reuses its
skeleton — `arith`, the Python-int `eval`, `env_sound`, and above all
the corner lemmas `mult_corners`/`div_corners`/`mod_pos_bounds`, which
is the payoff for proving them there.

### What is proved in `IntervalFloor.thy`

- **floor_range** — mirrors `arith_floor_range`: `Num`/`Var` points,
  `+`/`-` endpoint sums, `*` min/max over the four corner products;
  everything else (`%`/`/` as *children*, casts, unary, `Cond`,
  `Refuse`) refuses.
- **top_range** — mirrors `floor_arith_range`: `%` clamps to
  `[0, r1 − 1]` under the code's (stronger-than-necessary) premises
  `l0 ≥ 0 ∧ r0 > 0` — kept verbatim, though `mod_pos_bounds` proves the
  dividend bound is not needed; `/` (Python's `//`) takes the four
  endpoint quotients, exact because non-negative operands make
  truncation and floor coincide.
- **floor_range_sound, interval_floor_sound** — the obligations, by
  induction on the tree; the children go through the imported corner
  lemmas, the `*` case through `mult_corners`, `/` through the
  sibling's proven `metis div_corners [OF …]` one-liner.
- **intersect_narrows_sound** — `refine_floor_mod`'s intersect step:
  an interval covering `v` intersected with another one covering `v`
  still covers `v` (unfolding `max_def min_def` — see mechanics below).
- **The negative control: `v0_range` + `v0_not_sound`.** `v0_range`
  is the pre-fix `*` arm with the release build's wrap modelled by
  `wrap128`, and the closed counterexample is one line: `2^64 · 2^64`
  names `[0, 0]` for the value `2^128`. `fix_is_exact_corners` shows
  the fixed arm names it exactly.

### Scope and notes

The Rust fix landed with the proof: the `*` arm now uses
`checked_mul` like its `+`/`-` siblings and every other corner site in
the tree (raw `*`: panic in debug, silent wrap in release — the same
defect class as `1e19 → −8446744073709551616` that the BigInt pass
was built to dodge, one level down). Two truth-worth-keeping details:

- **Reachability, honestly scoped.** Through today's entry points the
  env comes from the wrapping bash walk, whose values cap near 2^63,
  so corner products are ≤ 2^126 and cannot overflow `i128` — the raw
  arm was latent, not live, behind `analyze_ranges`. It becomes live
  with the module's own documented next step (threading
  `IntervalConfig` so python()'s unbounded walk feeds the env), and it
  was inconsistent with its siblings either way. The regression test
  therefore calls the lifted `arith_floor_range` directly with
  `c = 2^120`: red (debug panic) before the fix, green (`None`) after.
- Three tests, `cargo test --lib shir_passes::interval`, 3 green: the
  overflow refusal, the full pipeline's no-panic/no-verdict on a deep
  `c·c` chain, and the floor-mod rule refining `[0, 29]` on a nonneg
  lhs. The full lib suite shows the same 25 failures with and without
  this file (baseline: file reverted to HEAD) — other workers'
  in-flight tree state, none in `interval`.

Isabelle mechanics worth keeping (they cost this round): a **cross-
session theory import** needs BOTH halves of the recipe —
`imports BigIntRange.BigIntRange` (bare `imports BigIntRange` resolves
session-dir-relative and fails) AND the parent link
(`= BigIntRange +`; a dot-import under `= HOL +` fails with "need to
include sessions … in ROOT"). For **nested boolean cases**, rename the
child (`case g: True`) so the bare `True` keeps pointing at the outer
one — the `case op: True` pattern BigIntRange already used. **No
`max_le_iff_disj`/`le_min_iff` lemma exists in this HOL** (grep empty):
the `max … ≤ v`-from-both-sides direction needs
`unfolding max_def min_def` plus `split: if_split_asm`, while the easy
directions (`min ≤ component`, `component ≤ max`) are what `blast` and
the `metis div_corners` line already handle. A **numeral type pinned
in only one disjunct** of an `∨` leaves the other ambiguous
(`0 < (2::'a) ^ 128` fails) — pin `(0::int)` on the side that survives
simp. And a plain `definition` (`wrap128`) is not simp-unfolded:
`add: wrap128_def`.

Box note: the shared `~/.isabelle` stats DB intermittently throws
`SQLITE_CONSTRAINT_PRIMARYKEY`/`SQLITE_READONLY_DBMOVED` when other
workers run Isabelle concurrently — an aborted run is not a proof
failure; retry. Also: the ROOT comment above BigIntRange claiming it
"does not check" is stale as of this build — it builds green from
source as IntervalFloor's parent under `quick_and_dirty = false`.
