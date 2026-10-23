theory DivModPow2
  imports Main
begin

unbundle bit_operations_syntax

text \<open>
  div-mod-pow2 — the claim, machine-checked, over int:

    0 \<le> x \<Longrightarrow> 1 \<le> n \<Longrightarrow> x div 2 ^ n = x >> n \<and> x mod 2 ^ n = x AND (2 ^ n - 1)

  Built with \<open>quick_and_dirty = false\<close>, so no sorry/oops can hide here.

  Section 1 names the operators the way the target languages name them.
  tdiv/tmod are C99 6.5.5p6 (bash's arithmetic expansion inherits them),
  and >> (our infix for shr) is the shift AS THE MACHINE DOES IT — divide
  by two, discard the remainder, repeat — so that neither side of the
  claim is true by definition: the induction below is what connects them.

  Section 2 proves the shift conjunct by the induction that was sketched
  for it: x >> Suc k = (x div 2) >> k, and (x div 2) div 2 ^ k =
  x div 2 ^ (Suc k), the latter applied to x div 2 — which requires
  0 \<le> x div 2, which requires 0 \<le> x.  That is exactly where the
  hypothesis earns its keep: the induction hypothesis is stated only for
  non-negative dividends, so every recursive use has to re-establish it.
  shift_eq_floor records the same identity for Isabelle's floor division
  WITHOUT the guard, because that is honest: Isabelle's div floors, so it
  already is the arithmetic shift, and the guard is not what makes that
  conjunct true.  What makes it load-bearing is the shell's truncating /,
  which section 5 treats.

  Section 3 proves the mask conjunct at the bit level (bit_eqI: two ints
  are equal when their bits are).  Over Isabelle's int it needs no
  hypothesis at all — HOL's mod is the Euclidean one, always \<ge> 0, and AND
  selects the same low bits — and section 5 shows exactly where that stops
  being true: C's % is truncating, so for a negative dividend it is the
  AND that is wrong, not the guard.

  Section 4 is the claim as stated.

  Section 5 reads the same two rewrites the way the shell computes them
  and machine-checks the negative control: at x = -3 both conjuncts fail,
  so the guard is load-bearing rather than decorative.

  Scope: int is unbounded here, deliberately — a bound that holds without
  wrapping holds for every fixed-width execution.  That the *target* does
  not wrap is a separate obligation (C_UB_PROOF.md), not one this file
  discharges, and nothing here says anything about word splitting.
\<close>

section \<open>1. The operators, named as the target languages name them\<close>

text \<open>Truncating division — C99 6.5.5p6: the quotient has its fraction
  discarded, so its sign follows the dividend's.  Division by zero is
  undefined in C; here it returns 0 so the function is total.  The
  divisor the rewrite ever produces, 2^n, is positive.\<close>

definition tdiv :: "int \<Rightarrow> int \<Rightarrow> int" where
  "tdiv x y = (if y = 0 then 0
               else if (x < 0) = (y < 0) then abs x div abs y
               else - (abs x div abs y))"

text \<open>Truncating remainder — C99 6.5.5p6 defines % as a - (a / b) * b,
  and that is the definition used here.\<close>

definition tmod :: "int \<Rightarrow> int \<Rightarrow> int" where
  "tmod x y = x - y * tdiv x y"

text \<open>Right shift, as the machine does it: divide by two, discard the
  remainder, repeat.  The recursion is on the shift amount, and its step
  is the one the informal argument used.  (A logical and an arithmetic
  shift coincide on a non-negative dividend, so this is >> for the only
  case the claim covers.)\<close>

fun shr :: "int \<Rightarrow> nat \<Rightarrow> int" where
  "shr x 0 = x"
| "shr x (Suc k) = shr (x div 2) k"

notation shr (infixr ">>" 100)

text \<open>The step, as stated in the sketch: shifting by Suc k is halving,
  then shifting by k.\<close>

lemma shift_step: "x >> Suc k = (x div 2) >> k"
  by simp

section \<open>2. The shift conjunct, by the induction sketched for it\<close>

text \<open>The hypothesis the induction step needs, and the reason it needs
  it: the induction hypothesis is applied to x div 2.\<close>

lemma half_nonneg: "0 \<le> (x::int) \<Longrightarrow> 0 \<le> x div 2"
  by (simp add: pos_imp_zdiv_nonneg_iff)

text \<open>The step lemma of the sketch: (x div 2) div 2 ^ k = x div 2 ^ (Suc k).
  HOL proves it for all x — floor division composes this way unconditionally —
  which is more than the sketch claimed, and worth recording as such.\<close>

lemma div_halves: "x div 2 div 2 ^ k = (x div 2 ^ (Suc k) :: int)"
  by (simp add: zdiv_zmult2_eq [symmetric])

text \<open>The claim's shift conjunct, by the sketched induction.  Note where
  half_nonneg is used: on the recursive call, not on the statement.\<close>

lemma shift_eq:
  assumes "0 \<le> x"
  shows "x div 2 ^ n = (x >> n :: int)"
using assms
proof (induction n arbitrary: x)
  case 0
  then show ?case by simp
next
  case (Suc n)
  have "x div 2 ^ Suc n = x div 2 div 2 ^ n"
    by (simp add: div_halves)
  also have "(x div 2) div 2 ^ n = (x div 2) >> n"
    using Suc.IH [OF half_nonneg [OF Suc.prems]] .
  finally show ?case by simp
qed

text \<open>Honest counterpart: the same identity for Isabelle's own division,
  for every x, guard or no guard.  Isabelle's div floors, and two's-
  complement >> floors, so they are the same function already.  This is
  why the guard is not what makes the first conjunct true over int — it is
  what makes it true for the shell's truncating /, which is section 5.\<close>

lemma shift_eq_floor: "x div 2 ^ n = (x >> n :: int)"
proof (induction n arbitrary: x)
  case 0
  then show ?case by simp
next
  case (Suc n)
  have "x div 2 ^ Suc n = x div 2 div 2 ^ n"
    by (simp add: div_halves)
  also have "(x div 2) div 2 ^ n = (x div 2) >> n"
    using Suc.IH [of "x div 2"] .
  finally show ?case by simp
qed

text \<open>shr really is Isabelle's shift: drop_bit n x is the library's bit
  shift on int (defined as x div 2 ^ n), and on a non-negative dividend it
  agrees with the algorithm above.  So >> in the claim is not private
  notation with an unchecked meaning.\<close>

lemma shr_eq_drop_bit:
  assumes "0 \<le> x"
  shows "x >> n = drop_bit n x"
  using assms by (simp add: drop_bit_eq_div shift_eq)

section \<open>3. The mask conjunct, at the bit level\<close>

text \<open>Why bit extensionality is the right tool: AND of the low n bits and
  mod 2 ^ n both mean "keep the bits below n", and bit_eqI turns that into
  an equality of ints.  HOL's mod is Euclidean (always \<ge> 0) and int's AND
  is two's complement, so no sign ever enters — hence no hypothesis.\<close>

lemma mask_eq_mod: "x mod 2 ^ n = (x AND (2 ^ n - 1) :: int)"
proof (rule bit_eqI)
  fix k
  have "bit (x mod 2 ^ n) k = (k < n \<and> bit x k)"
    by (simp add: take_bit_eq_mod [symmetric] bit_take_bit_iff)
  moreover have "bit (x AND (2 ^ n - 1)) k = (bit x k \<and> k < n)"
    by (simp add: bit_and_iff mask_eq_exp_minus_1 [symmetric] bit_mask_iff)
  ultimately show "bit (x mod 2 ^ n) k = bit (x AND (2 ^ n - 1)) k"
    by (simp add: conj_commute)
qed

section \<open>4. The claim, as stated\<close>

theorem div_mod_pow2:
  assumes "0 \<le> x" and "1 \<le> n"
  shows "x div 2 ^ n = (x >> n :: int) \<and> x mod 2 ^ n = x AND (2 ^ n - 1)"
  using assms by (auto intro: shift_eq mask_eq_mod)

section \<open>5. The shell/C reading, where the guard is load-bearing\<close>

text \<open>The two rewrites as bash actually computes them.  On a non-negative
  dividend with a positive divisor, truncation and floor coincide, so the
  guarded claim goes through for tdiv/tmod too.\<close>

lemma tdiv_nonneg:
  assumes "0 \<le> x" "0 < y"
  shows "tdiv x y = x div y"
  using assms by (simp add: tdiv_def)

lemma mod_eq_diff:
  "x mod (d::int) = x - d * (x div d)"
  by (simp add: mult_div_mod_eq [symmetric] ac_simps eq_diff_eq)

lemma tmod_nonneg:
  assumes "0 \<le> x" "0 < y"
  shows "tmod x y = x mod y"
  unfolding tmod_def using assms
  by (simp add: tdiv_nonneg mod_eq_diff)

lemma c_shift_eq:
  assumes "0 \<le> x" "1 \<le> n"
  shows "tdiv x (2 ^ n) = (x >> n :: int)"
  using assms by (simp add: tdiv_nonneg shift_eq zero_less_power)

lemma c_mask_eq:
  assumes "0 \<le> x" "1 \<le> n"
  shows "tmod x (2 ^ n) = (x AND (2 ^ n - 1) :: int)"
proof -
  have "tdiv x (2 ^ n) = x div 2 ^ n"
    using assms by (simp add: tdiv_nonneg zero_less_power)
  moreover have "x mod 2 ^ n = x AND (2 ^ n - 1)"
    by (rule mask_eq_mod)
  ultimately show ?thesis
    by (simp add: tmod_def mod_eq_diff [symmetric])
qed

text \<open>The negative control, machine-checked rather than observed: at a
  negative dividend C's truncating / and % land on the other side of the
  shift and the mask.  -3 / 2 = -1 but -3 >> 1 = -2 (floor); -3 % 2 = -1
  but -3 AND 1 = 1.  So a proof of the claim that did not carry the
  guard would be proving the wrong statement — 0 \<le> x is doing real
  work, exactly as the 4.97% agreement figure suggested.\<close>

lemma neg_control_shift: "tdiv (- 3) 2 \<noteq> ((- 3) >> 1)"
  by (simp add: tdiv_def)

lemma neg_control_mask: "tmod (- 3) 2 \<noteq> ((- 3::int) AND 1)"
  by (simp add: tdiv_def tmod_def)

lemma guard_is_load_bearing:
  "\<exists>x::int. x < 0 \<and> tdiv x 2 \<noteq> x >> 1"
  by (rule exI [where x = "- 3"]) (simp add: tdiv_def)

text \<open>And the converse direction of the negative control, for
  completeness: the MASK conjunct's guard is about C's %, not about int's
  AND.  mask_eq_mod holds for every x precisely because HOL's mod is
  Euclidean; for a negative dividend C's % is negative and the AND is
  not, so tmod to AND is guarded even though mod to AND is not.\<close>

lemma neg_control_c_mask: "tmod (- 3) 2 \<noteq> ((- 3::int) mod 2)"
  by (simp add: tdiv_def tmod_def)

end
