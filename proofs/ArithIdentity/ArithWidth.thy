theory ArithWidth
  imports ArithIdentity
begin

text \<open>
  Section 4 of the arithmetic denotation — the MACHINE-WIDTH reading,
  added for the C backend's checked i64 tier
  (sh2perl/src/c_backend.rs: `tiered_i64_expr`, `tiered_store_general`,
  `spec_checked_expr`).

  `ArithIdentity.thy` gives the shell-int reading \<open>ev\<close>: unbounded
  mathematical integers, errors as \<open>None\<close>.  The C tier renders the
  {+,-,*} fragment over `long long` with `__builtin_*_overflow` and
  spills to GMP when a result leaves i64.  What the tier's `ov` flag
  detects is exactly the disagreement between two readings of the SAME
  tree:

    \<open>ev\<close>   — the denotation (exact, unbounded);
    \<open>ev64\<close>  — the machine-width reading: every integer-valued
              position folded to 64 bits (what a `long long` holds
              after the builtin stores through `*out`).

  This file defines \<open>ev64\<close> and proves the contract that makes the
  tier correct — \<open>ev64_agrees\<close>: when every fold point's exact value
  fits i64, the two readings coincide, so the tier's committed fast
  value IS the denotation.  The converse is refuted concretely
  (`machine_wrap_changes_the_reading`: MAX+1 reads 2\<sup>63 exactly and
  -2\<sup>63 wrapped), which is why the flag-and-spill protocol exists at
  all.

  The Verus model of the emission itself — the flag being set exactly
  when the exact value leaves i64, the gated commit, the spill/break
  escape hatches — is sh2perl/proofs/verus/c_checked_arith.rs; this is
  the shIR-level half of the same contract, over the same \<open>ev\<close>.
  Two deliberate differences from it:

    * Verus's `Tree::Num` holds i64 BY TYPE so its range invariant
      skips leaves, while \<open>arith = Num int\<close> refines nothing — so
      \<open>air\<close> checks literals too and \<open>ev64\<close> folds them.  The
      Isabelle side is the strictly stronger statement;
    * \<open>fits64\<close> is spelled with a strict upper bound, \<open>x < half_u64\<close>,
      rather than \<open>x \<le> i64_max\<close>.  Over \<open>int\<close> the two are the same
      set, because \<open>half_u64 = i64_max + 1\<close>; the strict spelling is
      what lets every obligation below be discharged by linear
      arithmetic, which is the only arithmetic prover in this session
      (there is no \<open>omega\<close> or \<open>presburger\<close> in \<open>Main\<close>).  The easy
      direction of the correspondence is proved as
      \<open>fits64_of_max_bounds\<close>.

  DIVISION CAVEAT (stated, not papered over): \<open>ev\<close>'s `/` `%` arms
  use Isabelle's floored \<open>div\<close>/\<open>mod\<close> while bash and C truncate
  toward zero.  The two readings differ only on negative dividends, the
  arith-identity folds only fire where they coincide (divisor 1 or a
  positive literal), and \<open>ev64\<close> reuses \<open>apply_op\<close> unchanged for
  `/` `%` — so the agreement theorem here is agnostic to that choice
  and claims nothing about it.  Fixing \<open>ev\<close>'s rounding is a separate
  decision.

  Build:  isabelle build -D proofs/ArithIdentity
\<close>

section \<open>1. The i64 width and the wrap\<close>

definition i64_max :: int where
  "i64_max = 9223372036854775807"

definition i64_min :: int where
  "i64_min = - 9223372036854775808"

text \<open>The modulus and its half, spelled out so simp computes them.\<close>

definition u64_mod :: int where
  "u64_mod = 18446744073709551616"

definition half_u64 :: int where
  "half_u64 = 9223372036854775808"

text \<open>The i64 range, in the strict-upper-bound spelling described above.
  Over \<open>int\<close> this is exactly \<open>i64_min \<le> x \<and> x \<le> i64_max\<close>.\<close>

definition fits64 :: "int \<Rightarrow> bool" where
  "fits64 x \<longleftrightarrow> i64_min \<le> x \<and> x < half_u64"

lemma half_is_max_plus_one: "half_u64 = i64_max + 1"
  unfolding half_u64_def i64_max_def by simp

lemma half_minus_mod_is_min: "half_u64 - u64_mod = i64_min"
  unfolding half_u64_def u64_mod_def i64_min_def by simp

lemma fits64_of_max_bounds:
  "i64_min \<le> x \<Longrightarrow> x \<le> i64_max \<Longrightarrow> fits64 x"
  unfolding fits64_def half_u64_def i64_max_def i64_min_def by linarith

text \<open>Two's-complement wrap: the bits C stores through `*out` when the
  exact result leaves i64 — \<open>x mod 2^64\<close> in \<open>[0, 2^64)\<close>, folded into
  the signed range.  The branch on \<open>half_u64\<close> is sign folding.\<close>

definition wrap64 :: "int \<Rightarrow> int" where
  "wrap64 x = (if half_u64 \<le> x mod u64_mod
               then x mod u64_mod - u64_mod
               else x mod u64_mod)"

text \<open>An in-range value is its own wrap — so the machine reading cannot
  diverge from the denotation while \<open>fits64\<close> holds at that position.\<close>

lemma wrap64_id [simp]:
  assumes "fits64 x"
  shows "wrap64 x = x"
proof -
  from assms have lo: "i64_min \<le> x" and hi: "x < half_u64"
    by (simp_all add: fits64_def)
  have core:
    "(if half_u64 \<le> x mod u64_mod
      then x mod u64_mod - u64_mod
      else x mod u64_mod) = x"
  proof (cases "0 \<le> x")
    case True
    have lt: "x < u64_mod"
      using hi unfolding half_u64_def u64_mod_def by linarith
    have xm: "x mod u64_mod = x"
      using True lt by simp
    show ?thesis using xm hi by simp
  next
    case False
    then have nx: "x < 0" by simp
    have negm: "- u64_mod < x"
      using lo unfolding i64_min_def u64_mod_def half_u64_def by linarith
    have xp: "0 \<le> x + u64_mod"
      using negm by linarith
    have xpn: "x + u64_mod < u64_mod"
      using nx by linarith
    have step1: "x mod u64_mod = (x + u64_mod) mod u64_mod"
      by simp
    have step2: "(x + u64_mod) mod u64_mod = x + u64_mod"
      by (rule mod_pos_pos_trivial[OF xp xpn])
    have xm: "x mod u64_mod = x + u64_mod"
      by (rule trans[OF step1 step2])
    have hb: "half_u64 \<le> x + u64_mod"
      using lo unfolding i64_min_def u64_mod_def half_u64_def by linarith
    show ?thesis using xm hb by simp
  qed
  show ?thesis unfolding wrap64_def by (fact core)
qed

text \<open>And every wrap lands back in range — the fast tier's value is
  always a legal `long long`, flag or no flag.\<close>

lemma wrap64_range: "fits64 (wrap64 x)"
proof -
  define m where "m = x mod u64_mod"
  have m0: "0 \<le> m" and mb: "m < u64_mod"
    unfolding m_def u64_mod_def by simp_all
  have T: "half_u64 \<le> m \<Longrightarrow> i64_min \<le> m - u64_mod \<and> m - u64_mod < half_u64"
    using mb
    unfolding i64_min_def half_u64_def u64_mod_def by linarith
  have F: "\<not> half_u64 \<le> m \<Longrightarrow> i64_min \<le> m \<and> m < half_u64"
    using m0
    unfolding i64_min_def half_u64_def u64_mod_def by linarith
  have core:
    "i64_min \<le> (if half_u64 \<le> m then m - u64_mod else m) \<and>
     (if half_u64 \<le> m then m - u64_mod else m) < half_u64"
  proof (cases "half_u64 \<le> m")
    case True
    have "i64_min \<le> m - u64_mod \<and> m - u64_mod < half_u64"
      using True T by blast
    then show ?thesis using True by simp
  next
    case False
    have "i64_min \<le> m \<and> m < half_u64"
      using False F by blast
    then show ?thesis using False by simp
  qed
  show ?thesis
    unfolding fits64_def wrap64_def m_def[symmetric] by (fact core)
qed

section \<open>2. The machine-width reading\<close>

text \<open>\<open>op_fits\<close> is the tier's own per-node range test, in exactly the
  shape the emission's builtins make it: a check only at `{+,-,*}`,
  where a `__builtin_*_overflow` fires; `/` `%` and everything else are
  rendered without a builtin, so they carry no condition.\<close>

definition op_fits :: "string \<Rightarrow> int \<Rightarrow> int \<Rightarrow> bool" where
  "op_fits op x y \<longleftrightarrow>
     (if op = ''+'' then fits64 (x + y)
      else if op = ''-'' then fits64 (x - y)
      else if op = ''*'' then fits64 (x * y)
      else True)"

text \<open>\<open>wrap_apply\<close> is `apply_op` with the {+,-,*} results folded to 64
  bits — the value a `long long` temp holds after the builtin stores
  through `*out`.  Everything else passes through untouched, so `/` `%`
  keep \<open>apply_op\<close>'s exact reading.\<close>

definition wrap_apply :: "string \<Rightarrow> int \<Rightarrow> int \<Rightarrow> int option" where
  "wrap_apply op a b =
     (if op = ''+'' then Some (wrap64 (a + b))
      else if op = ''-'' then Some (wrap64 (a - b))
      else if op = ''*'' then Some (wrap64 (a * b))
      else apply_op op a b)"

fun ev64 :: "store \<Rightarrow> arith \<Rightarrow> int option" where
  "ev64 e (Num n) = Some (wrap64 n)"
| "ev64 e (Bin op l r) =
     (case ev64 e l of
        None \<Rightarrow> None
      | Some a \<Rightarrow>
          (case ev64 e r of
             None \<Rightarrow> None
           | Some b \<Rightarrow> wrap_apply op a b))"
| "ev64 e (UnOp op a) =
     (if op = ''-'' then
        (case ev64 e a of None \<Rightarrow> None | Some v \<Rightarrow> Some (wrap64 (- v)))
      else None)"
| "ev64 e (Cond t c d) =
     (case ev64 e t of
        Some v \<Rightarrow> (if v \<noteq> 0 then ev64 e c else ev64 e d)
      | None \<Rightarrow> None)"
| "ev64 e Other = None"

text \<open>\<open>air\<close> — arith-in-range — is the range invariant shaped exactly
  like \<open>ev64\<close>'s fold points: every position that gets folded to 64
  bits holds an exact value inside i64.  Leaves are checked because
  \<open>arith = Num int\<close> refines nothing; \<open>Other\<close> carries no condition
  because both readings of it are \<open>None\<close>.\<close>

fun air :: "store \<Rightarrow> arith \<Rightarrow> bool" where
  "air e (Num n) = fits64 n"
| "air e (Bin op l r) =
     (air e l \<and> air e r \<and>
      (case ev e l of
         None \<Rightarrow> True
       | Some x \<Rightarrow>
           (case ev e r of
              None \<Rightarrow> True
            | Some y \<Rightarrow> op_fits op x y)))"
| "air e (UnOp op a) =
     (air e a \<and>
      (if op = ''-'' then
         (case ev e a of None \<Rightarrow> True | Some v \<Rightarrow> fits64 (- v))
       else True))"
| "air e (Cond t c d) = (air e t \<and> air e c \<and> air e d)"
| "air e Other = True"

subsection \<open>Extraction: the shape of \<open>air\<close> at one node\<close>

text \<open>\<open>air\<close> is stated as a conjunction wrapped in an option case so it
  never has to name a value that does not exist.  These are the forms
  the induction below consumes, each in the order the emission's own
  recursion would meet them.\<close>

lemma air_binDl: "air e (Bin op l r) \<Longrightarrow> air e l"
  by auto

lemma air_binDr: "air e (Bin op l r) \<Longrightarrow> air e r"
  by auto

lemma air_bin_fit:
  assumes "air e (Bin op l r)" "ev e l = Some x" "ev e r = Some y"
  shows "op_fits op x y"
  using assms by auto

lemma air_unopD: "air e (UnOp op a) \<Longrightarrow> air e a"
  by auto

lemma air_unop_fit:
  assumes "air e (UnOp ''-'' a)" "ev e a = Some v"
  shows "fits64 (- v)"
  using assms by auto

lemma air_condDt: "air e (Cond t c d) \<Longrightarrow> air e t"
  by auto

lemma air_condDc: "air e (Cond t c d) \<Longrightarrow> air e c"
  by auto

lemma air_condDd: "air e (Cond t c d) \<Longrightarrow> air e d"
  by auto

subsection \<open>Where the fold and the exact reading coincide\<close>

lemma wrap_apply_agrees:
  assumes "op_fits op x y"
  shows "wrap_apply op x y = apply_op op x y"
proof -
  have core:
    "(if op = ''+'' then Some (wrap64 (x + y))
      else if op = ''-'' then Some (wrap64 (x - y))
      else if op = ''*'' then Some (wrap64 (x * y))
      else apply_op op x y) = apply_op op x y"
  proof (cases "op = ''+''")
    case True
    then have p1: "op = ''+''" by simp
    have f: "fits64 (x + y)" using assms p1 by (simp add: op_fits_def)
    show ?thesis using p1 f by simp
  next
    case False
    then have n1: "op \<noteq> ''+''" by simp
    show ?thesis
    proof (cases "op = ''-''")
      case True
      then have p2: "op = ''-''" by simp
      have f: "fits64 (x - y)" using assms p2 by (simp add: op_fits_def)
      show ?thesis using n1 p2 f by simp
    next
      case False
      then have n2: "op \<noteq> ''-''" by simp
      show ?thesis
      proof (cases "op = ''*''")
        case True
        then have p3: "op = ''*''" by simp
        have f: "fits64 (x * y)" using assms p3 by (simp add: op_fits_def)
        show ?thesis using n1 n2 p3 f by simp
      next
        case False
        then have n3: "op \<noteq> ''*''" by simp
        show ?thesis using n1 n2 n3 by simp
      qed
    qed
  qed
  show ?thesis unfolding wrap_apply_def by (fact core)
qed

section \<open>3. The agreement theorem\<close>

text \<open>When every fold point holds an in-range exact value, the machine
  reading IS the denotation — so a value the tier commits under
  `if (!ov)` is the shIR meaning of the tree, not an approximation of
  it.  This is the shIR-level statement of what `commit_a_sound` in
  `c_checked_arith.rs` proves about the emission.\<close>

theorem ev64_agrees:
  assumes "air e a"
  shows "ev64 e a = ev e a"
  using assms
proof (induction a arbitrary: e)
  case (Num n)
  then show ?case by simp
next
  case Other
  then show ?case by simp
next
  case (UnOp op a0)
  have ia: "air e a0" using UnOp.prems by (rule air_unopD)
  have ih: "ev64 e a0 = ev e a0" using UnOp.IH ia by blast
  show ?case
  proof (cases "op = ''-''")
    case True
    have pre: "air e (UnOp ''-'' a0)" using UnOp.prems True by simp
    show ?thesis
    proof (cases "ev e a0")
      case None
      then have el: "ev e a0 = None" by simp
      show ?thesis using True ih el by simp
    next
      case (Some v)
      then have el: "ev e a0 = Some v" by simp
      have f: "fits64 (- v)" by (rule air_unop_fit[OF pre el])
      show ?thesis using True ih el f by simp
    qed
  next
    case False
    then have no: "op \<noteq> ''-''" by simp
    show ?thesis using no by simp
  qed
next
  case (Cond t c d)
  have it: "air e t" using Cond.prems by (rule air_condDt)
  have ic: "air e c" using Cond.prems by (rule air_condDc)
  have id: "air e d" using Cond.prems by (rule air_condDd)
  have iht: "ev64 e t = ev e t" using Cond.IH it by blast
  show ?case
  proof (cases "ev e t")
    case None
    then have el: "ev e t = None" by simp
    show ?thesis using el iht by simp
  next
    case (Some v)
    then have el: "ev e t = Some v" by simp
    show ?thesis
    proof (cases "v \<noteq> 0")
      case True
      have ihc: "ev64 e c = ev e c" using Cond.IH ic by blast
      show ?thesis using el iht ihc True by simp
    next
      case False
      have ihd: "ev64 e d = ev e d" using Cond.IH id by blast
      show ?thesis using el iht ihd False by simp
    qed
  qed
next
  case (Bin op l r)
  have il: "air e l" using Bin.prems by (rule air_binDl)
  have ir: "air e r" using Bin.prems by (rule air_binDr)
  have ihl: "ev64 e l = ev e l" using Bin.IH il by blast
  have ihr: "ev64 e r = ev e r" using Bin.IH ir by blast
  show ?case
  proof (cases "ev e l")
    case None
    then have el: "ev e l = None" by simp
    show ?thesis using el ihl ihr by simp
  next
    case (Some x)
    then have el: "ev e l = Some x" by simp
    show ?thesis
    proof (cases "ev e r")
      case None
      then have er: "ev e r = None" by simp
      show ?thesis using el er ihl ihr by simp
    next
      case (Some y)
      then have er: "ev e r = Some y" by simp
      have f: "op_fits op x y" by (rule air_bin_fit[OF Bin.prems el er])
      have g: "wrap_apply op x y = apply_op op x y"
        by (rule wrap_apply_agrees[OF f])
      show ?thesis using el er ihl ihr g by simp
    qed
  qed
qed

section \<open>4. Why the flag exists — the concrete divergence\<close>

text \<open>The negative control, at the denotation level.  MAX+1 leaves i64:
  the exact reading says 2\<sup>63, the machine reading says -2\<sup>63 — the
  bits of `LLONG_MIN`.  The invariant genuinely fails here, so
  \<open>ev64_agrees\<close> says nothing; an unguarded commit would ship the
  wrong number, which is exactly the counterexample
  `unguarded_commit_ships_wrap` gives for the emission.  Stated as four
  separate computations so each fact is usable on its own.\<close>

lemma machine_wrap_exact:
  fixes e :: store
  shows "ev e (Bin ''+'' (Num i64_max) (Num 1)) = Some half_u64"
  by (simp add: i64_max_def half_u64_def)

lemma machine_wrap_wrapped:
  fixes e :: store
  shows "ev64 e (Bin ''+'' (Num i64_max) (Num 1)) = Some i64_min"
  by (simp add: i64_max_def i64_min_def half_u64_def u64_mod_def
                fits64_def wrap64_def wrap_apply_def)

lemma machine_wrap_breaks_the_invariant:
  fixes e :: store
  shows "\<not> air e (Bin ''+'' (Num i64_max) (Num 1))"
  by (simp add: i64_max_def i64_min_def half_u64_def u64_mod_def
                fits64_def op_fits_def)

lemma machine_wrap_changes_the_reading:
  fixes e :: store
  shows "ev64 e (Bin ''+'' (Num i64_max) (Num 1))
         \<noteq> ev e (Bin ''+'' (Num i64_max) (Num 1))"
  by (simp add: i64_max_def i64_min_def half_u64_def u64_mod_def
                fits64_def wrap64_def wrap_apply_def)

end
