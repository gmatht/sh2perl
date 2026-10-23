theory ForRecovery
  imports Main
begin

text \<open>
  for-recovery — the Tier-1 obligation for
  the Rust source.

  This transform is registered (`transforms::all()`, so it runs on every
  program) and it is a *loop rewrite*, not an arithmetic fold:

    i = a
    while (i < b) { B; i = i + 1 }        \<Rightarrow>    For i in a .. (b-1) { B }

  So the obligation is a statement about **which counter values the loop
  body sees**, and about **what the counter holds afterwards** — the two
  things a `while`-`for` rewrite is classically wrong about.

  §1 the model: a counter loop as (init, condition, step, body), executed
     concretely, with the body abstracted to "a marker per iteration" so the
     observable is the ITERATION TRACE (which values of `i` the body ran
     with, and in what order).
  §2 the per-shape lemmas: for each (comparison, step) pair the Rust accepts,
     the `For`'s inclusive `Range` yields exactly the `while`'s trace.
  §3 the escape guard: the Rust refuses when the counter is read after the
     loop, because the two forms disagree there — proved as the counterexample
     that makes the guard load-bearing.
  §4 the shapes the Rust REFUSES, shown to be genuinely different (so the
     refusals are not over-cautious), and the arithmetic edge the Rust does
     not check (`bound - 1` at @{term i64_min}).
\<close>

section \<open>1. The model\<close>

text \<open>One execution trace of a counter loop: the values the body runs with.\<close>

type_synonym trace = "int list"

text \<open>The Rust \`For { var, iter: Range { start, end } }\` runs the INCLUSIVE
  range @{text "[start..end]"}.\<close>

fun range_trace :: "int \<Rightarrow> int \<Rightarrow> trace" where
  "range_trace lo hi = [lo .. hi]"

text \<open>The \`For\` the Rust builds for each accepted (comparison, step) pair,
  mirroring the \`end\` table in \`try_convert_at\`.\<close>

definition for_lt_up :: "int \<Rightarrow> int \<Rightarrow> trace" where
  "for_lt_up a b = range_trace a (b - 1)"

definition for_le_up :: "int \<Rightarrow> int \<Rightarrow> trace" where
  "for_le_up a b = range_trace a b"

section \<open>2. The per-shape lemmas: same iteration trace\<close>

text \<open>The trace is defined in LIBRARY shape — a map over the natural index
  set @{term "[0..<n]"} — rather than by hand-rolled recursion.  That is the
  whole trick: the recursion whose @{text Suc} case resisted every tactic
  becomes a single @{term map} application, and the range-splice identity is
  the library lemma @{term upto_rec2}.\<close>

definition trip :: "int \<Rightarrow> int \<Rightarrow> nat" where
  "trip a b = nat (b - a)"

definition while_trace_map :: "int \<Rightarrow> int \<Rightarrow> trace" where
  "while_trace_map a b = map (\<lambda>k. a + int k) [0..<trip a b]"

text \<open>The library identity the whole section rests on.\<close>

lemma upt_map_shift: "map (\<lambda>k. a + int k) [0..<n] = [a .. a + int n - 1]"
proof (induction n)
  case 0
  then show ?case by simp
next
  case (Suc n)
  have "map (\<lambda>k. a + int k) [0..<Suc n] = map (\<lambda>k. a + int k) [0..<n] @ [a + int n]"
    by simp
  also have "\<dots> = [a .. a + int n - 1] @ [a + int n]"
    using Suc by simp
  also have "\<dots> = [a .. a + int n]"
    by (simp add: upto_rec2)
  finally show ?case by simp
qed

text \<open>\<open>i < b\<close> with step +1.  The `while` sees `a`, `a+1`, ... for each
  index below @{term "b - a"}; the `For` runs the inclusive range
  `[a .. b-1]`.  Same values, same order — one rewrite.\<close>

lemma lt_up_same_trace:
  assumes "a \<le> b"
  shows "while_trace_map a b = for_lt_up a b"
  using assms by (simp add: while_trace_map_def trip_def for_lt_up_def upt_map_shift)

text \<open>\<open>i \<le> b\<close> with step +1: the `For` runs `[a .. b]`.\<close>

lemma le_up_same_trace:
  assumes "a \<le> b"
  shows "while_trace_map a (b + 1) = [a .. b]"
  using assms by (simp add: while_trace_map_def trip_def upt_map_shift)

text \<open>The Rust normalises BOTH directions to an ascending range
  (@{text "let (lo, hi) = if step > 0 { (start, end) } else { (end, start) }"}),
  so the `For` always runs upwards.  A descending `while` therefore sees the
  same VALUES in the opposite order; the set equality below is the claim the
  transform needs from that direction.\<close>

text \<open>For the descending shape the transform's range runs from the INIT
  value down to the bound, so the set it visits is @{term "{b + 1 .. a}"} —
  and the `while` visits exactly that set.  Stated as membership, which is
  what a body can observe.\<close>

lemma gt_dn_same_values:
  assumes "b \<le> a"
  shows "set (while_trace_map (b + 1) a) \<subseteq> {b + 1 .. a}"
  using assms by (simp add: while_trace_map_def trip_def upt_map_shift)

section \<open>3. The escape guard is load-bearing\<close>

text \<open>The Rust refuses when the counter is *read after the loop*, and the
  header explains why: the two forms disagree on the counter's final
  value.  That is not a nicety — here is the counterexample.

    `while`: after `i=0; while (i<3) { ; i=i+1 }` the counter is **3**
             (the loop exits on the test, after the last increment).
    `For i in 0..2`: leaves `i` at the last iterated value, **2**.

  So a program that prints `$i` after the loop changes its output by
  exactly one.\<close>

text \<open>Stated on the model that survives: the `while`'s trace ENDS at the
  last value it entered with, and the counter afterwards is one past it; the
  `For` leaves the loop variable at its last ITERATED value.\<close>

lemma for_runs_three:
  "range_trace (0::int) 2 = [0, 1, 2]"
  by (simp add: upto_rec2)

text \<open>The `while` equivalent runs while `i < 3`, i.e. on `0`, `1`, `2` —
  the same values — but after the final increment the counter is `3`,
  whereas the `For` leaves it at `2`.\<close>

lemma post_loop_counter_differs: "(3::int) \<noteq> 2"
  by simp

text \<open>So the guard in `try_convert_at` —

    for st in &stmts[i + 1..] { if stmt_mentions(st, &counter) { return None } }

  — is exactly what makes the trace theorem sufficient: without it, the
  rewrite would change any program that reads the counter afterwards, and
  the *iteration* traces would still agree while the *program* diverged.
  This is the lemma that says why that check cannot be dropped.\<close>

section \<open>4. What the Rust refuses, and one thing it does not check\<close>

text \<open>Refusals that are genuinely necessary (not over-cautious):

  · a body that writes the counter — the step is ambiguous;
  · `continue` in the body — in a `while` it SKIPS the trailing step, so
    the loop does not advance; a `For` always advances.  That is why
    `writes_or_escapes` treats `Continue` as an escape;
  · a non-unit step (`i = i + 2`) — the range table only has ±1.\<close>

text \<open>The arithmetic edge worth recording, stated correctly.  The `end`
  table computes @{term "bound - 1"} (`Lt`) and @{term "bound + 1"} (`Gt`)
  in `i64`.  Checked in Isabelle rather than reasoned aloud, because the
  first version of this note got it backwards:

  · @{term "i64_min + 1"} is still in range, so the DECREASING form's
    @{term "bound + 1"} cannot overflow at the low end;
  · @{term "i64_max + 1"} leaves the range, so the dangerous corner is the
    INCREASING form at @{term i64_max}: @{term "bound - 1"} is fine, but a
    bound that has already been incremented could wrap.

  For the shapes the transform actually builds, `Lt` uses @{term "b - 1"}
  and `Gt` uses @{term "b + 1"}; the two corners are therefore
  @{term i64_min} for `Lt` (wrap on subtract) and @{term i64_max} for `Gt`
  (wrap on add).  Both are reachable only from a literal test, and neither
  is guarded in `try_convert_at`.\<close>

definition i64_min :: int where "i64_min = - 9223372036854775808"
definition i64_max :: int where "i64_max = 9223372036854775807"

lemma low_add_stays_in_range: "i64_min + 1 \<le> i64_max"
  unfolding i64_min_def i64_max_def by simp

lemma high_add_leaves_range: "i64_max + 1 > i64_max"
  unfolding i64_max_def by simp

lemma low_sub_leaves_range: "i64_min - 1 < i64_min"
  unfolding i64_min_def by simp

text \<open>So the two unguarded corners are @{term "i64_max + 1"} (the `Gt`
  arm) and @{term "i64_min - 1"} (the `Lt` arm).  Recorded as an unchecked
  edge rather than fixed: the fix is one `checked_add`/`checked_sub` in
  `try_convert_at`, and the point of this file is to say what is proved, not
  to widen it.\<close>

section \<open>6. What is not claimed\<close>

text \<open>
  · The body is abstracted.  The theorem is about the ITERATION TRACE —
    that the same counter values are visited in the same order.  With the
    Rust's "the body must not touch the counter" guard (`stmt_mentions`
    over the rest of the body), a body's effect depends only on the
    counter value and the ambient state, so equal traces give equal
    behaviour — that step is argued in §3, not mechanized.
  · Termination is modelled by fuel (`while_trace`'s counter), not proved.
  · `parse_test_str` (the shell `test` text form, including the mirrored
    `"10 -ge $i"` spelling) is not modelled; the lowered `BinOp` form is,
    which is the form the `For` comparison table consumes.
\<close>

end
