theory BigIntRange
  imports Main
begin

section \<open>The BigInt range pass, mirrored from the Rust\<close>

text \<open>
  Mirror of \ \ \<open>sh2perl/src/shir_passes/bigint_range.rs\<close>: the expression walk
  \<open>big_arith_range\<close> -- the BigInt-level one that names literals past i128 where
  the checked-i128 pass declines -- and, as the negative control, the same walk as
  it stood before the 2026-10 fix (called \<open>v0\<close> here).

  The obligation, exactly as \ \ \<open>docs/transform-verification.md\<close> states it for
  Tier 1 analysis soundness:
    \<open>range = Some (lo, hi)\<close> implies the Python-int value of the expression lies
  in \<open>[lo, hi]\<close>.

  The value reading is *Python* integer semantics -- unbounded, floor division and
  sign-of-divisor modulo -- because that is the semantics under which the pass
  certifies. HOL's \<open>div\<close>/\<open>mod\<close> on \<open>int\<close> are exactly floor-based
  (probe-pinned: \<open>- 7 mod 3 = 2\<close> and \<open>6 mod - 4 = - 2\<close>), matching Rust's
  \<open>floor_mod\<close>/\<open>floor_div\<close>, which adjust truncation to floor explicitly.

  The pre-fix pass had three soundness gaps:
    (1) \<open>%\<close> took min/max of the value at the four interval *corners* -- false
        for floor-mod, whose value set over an interval is an interval the
        corners miss (the DivModPow2 class). The arm's own comment said the
        sign-of-divisor property needs no dividend bound; the corner loop beside
        it needed exactly that property and did not have it.
    (2) \<open>//\<close> took corner min/max without refusing ranges that straddle 0,
        where floor-div's value set jumps at the pole. Divisor monotonicity is
        available here only for positive ranges, so the fix refuses every
        non-positive divisor range rather than repairing the negative ones.
    (3) \<open>Cast\<close> ignored a float-typed cast, whose value the pipeline cannot
        read as an integer. (This one has no counterexample *value* in the
        model: eval reads a float cast as \<open>None\<close>, so soundness is vacuous
        there. The fix is still mirrored and checked below.)

  \<open>v0_range\<close> below reproduces the pre-fix arms verbatim; \<open>v0_not_sound\<close> is
  the machine-checked counterexample set, and \<open>bigint_range_sound\<close> is the
  fixed pass's obligation. The statement-level walk (\<open>big_ir_range\<close>,
  \<open>analyze_var_ranges_big\<close>) is not mirrored: the pass's obligation is
  per-expression, and a statement interpretation is Tier 3 by the rule in the
  verification doc.
\<close>

section \<open>The expression tree\<close>

datatype arith =
    Num int
  | Var string
  | Bin string arith arith
  | UnOp string arith
  | Cond arith arith arith
  | Cast bool arith
  | Refuse

text \<open>
  \<open>Cast f a\<close> carries the pipeline's float flag: \<open>True\<close> means a
  float-typed cast (\<open>IrType::Float64\<close>/\<open>Float32\<close>), whose value this pass
  refuses to read. \<open>Refuse\<close> stands for every ArithAst form the walk does
  not model -- all of which yield \<open>None\<close>, a declined range.
\<close>

section \<open>The Python-int value reading\<close>

fun eval :: "(string \<Rightarrow> int) \<Rightarrow> arith \<Rightarrow> int option" where
  "eval \<nu> (Num n) = Some n"
| "eval \<nu> (Var x) = Some (\<nu> x)"
| "eval \<nu> (Refuse) = None"
| "eval \<nu> (Cast f a) = (if f then None else eval \<nu> a)"
| "eval \<nu> (UnOp op a) =
     (if op = ''-''
      then (case eval \<nu> a of None \<Rightarrow> None | Some z \<Rightarrow> Some (- z))
      else None)"
| "eval \<nu> (Cond t a b) =
     (case eval \<nu> t of
        None \<Rightarrow> None
      | Some tv \<Rightarrow> if tv \<noteq> 0 then eval \<nu> a else eval \<nu> b)"
| "eval \<nu> (Bin op l r) =
     (case eval \<nu> l of
        None \<Rightarrow> None
      | Some vl \<Rightarrow>
          (case eval \<nu> r of
             None \<Rightarrow> None
           | Some vr \<Rightarrow>
               if op = ''+'' then Some (vl + vr)
               else if op = ''-'' then Some (vl - vr)
               else if op = ''*'' then Some (vl * vr)
               else if op = ''**'' then
                 (if vr < 0 then None else Some (vl ^ nat vr))
               else if op = ''%'' then
                 (if vr = 0 then None else Some (vl mod vr))
               else if op = ''//'' then
                 (if vr = 0 then None else Some (vl div vr))
               else if op = ''=='' then Some (if vl = vr then 1 else 0)
               else if op = ''!='' then Some (if vl \<noteq> vr then 1 else 0)
               else if op = ''<'' then Some (if vl < vr then 1 else 0)
               else if op = ''>'' then Some (if vl > vr then 1 else 0)
               else if op = ''<='' then Some (if vl \<le> vr then 1 else 0)
               else if op = ''>='' then Some (if vl \<ge> vr then 1 else 0)
               else None))"

section \<open>The range analysis, as fixed (mirrors big\_arith\_range)\<close>

fun big_range :: "arith \<Rightarrow> (string \<Rightarrow> (int \<times> int) option) \<Rightarrow> (int \<times> int) option" where
  "big_range (Num n) \<rho> = Some (n, n)"
| "big_range (Var x) \<rho> = \<rho> x"
| "big_range Refuse \<rho> = None"
| "big_range (Cast f a) \<rho> = (if f then None else big_range a \<rho>)"
| "big_range (UnOp op a) \<rho> =
     (case big_range a \<rho> of
        None \<Rightarrow> None
      | Some (l, h) \<Rightarrow> if op = ''-'' then Some (- h, - l) else None)"
| "big_range (Cond t a b) \<rho> =
     (case big_range t \<rho> of
        None \<Rightarrow> None
      | Some _ \<Rightarrow>
          (case big_range a \<rho> of
             None \<Rightarrow> None
           | Some (la, ha) \<Rightarrow>
               (case big_range b \<rho> of
                  None \<Rightarrow> None
                | Some (lb, hb) \<Rightarrow> Some (min la lb, max ha hb))))"
| "big_range (Bin op l r) \<rho> =
     (case big_range l \<rho> of
        None \<Rightarrow> None
      | Some (l0, l1) \<Rightarrow>
          (case big_range r \<rho> of
             None \<Rightarrow> None
           | Some (r0, r1) \<Rightarrow>
               if op = ''+'' then Some (l0 + r0, l1 + r1)
               else if op = ''-'' then Some (l0 - r1, l1 - r0)
               else if op = ''*'' then
                 Some (min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))),
                       max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1))))
               else if op = ''**'' then
                 (if l0 = l1 \<and> r0 = r1 \<and> r0 \<ge> 0
                  then Some (l0 ^ nat r0, l0 ^ nat r0)
                  else None)
               else if op = ''%'' then
                 (if l0 = l1 \<and> r0 = r1 then
                    (if r0 = 0 then None else Some (l0 mod r0, l0 mod r0))
                  else if r0 > 0 then Some (0, r1 - 1)
                  else if r1 < 0 then Some (r0 + 1, 0)
                  else None)
               else if op = ''//'' then
                 (if r0 > 0 then
                    Some (min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))),
                          max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1))))
                  else None)
               else if op = ''=='' \<or> op = ''!='' \<or> op = ''<'' \<or> op = ''>'' \<or> op = ''<='' \<or> op = ''>=''
                 then Some (0, 1)
               else None))"

text \<open>
  Arm-by-arm correspondence with the Rust (the fix applied in place):
  \<open>+\<close>, \<open>-\<close>, \<open>*\<close>, \<open>**\<close>, comparisons, \<open>Un\<close>, \<open>Cond\<close>, \<open>Num\<close>, \<open>Var\<close>:
  unchanged. \<open>*\<close> is min/max over the four corner products, \<open>**\<close> is
  point-exact (the code's honest "might be" collapses to "is" when both
  ranges are points), comparisons give \<open>(0, 1)\<close>, \<open>Cond\<close> hulls both
  arms and ignores the test's range exactly as before.
  \<open>%\<close>: point-exact when both ranges are points and the divisor is nonzero;
  otherwise the sign-of-divisor intervals \<open>(0, r1 - 1)\<close> for \<open>r0 > 0\<close>
  and \<open>(r0 + 1, 0)\<close> for \<open>r1 < 0\<close> -- no dividend bound needed, as the
  old comment said -- and refusal when the divisor range straddles 0.
  \<open>//\<close>: corners as before, but only for \<open>r0 > 0\<close>; every other divisor
  range is refused.
  \<open>Cast\<close>: a float-typed cast is refused.
\<close>

section \<open>The pre-fix analysis (negative control)\<close>

fun v0_range :: "arith \<Rightarrow> (string \<Rightarrow> (int \<times> int) option) \<Rightarrow> (int \<times> int) option" where
  "v0_range (Num n) \<rho> = Some (n, n)"
| "v0_range (Var x) \<rho> = \<rho> x"
| "v0_range Refuse \<rho> = None"
| "v0_range (Cast f a) \<rho> = v0_range a \<rho>"
| "v0_range (UnOp op a) \<rho> =
     (case v0_range a \<rho> of
        None \<Rightarrow> None
      | Some (l, h) \<Rightarrow> if op = ''-'' then Some (- h, - l) else None)"
| "v0_range (Cond t a b) \<rho> =
     (case v0_range t \<rho> of
        None \<Rightarrow> None
      | Some _ \<Rightarrow>
          (case v0_range a \<rho> of
             None \<Rightarrow> None
           | Some (la, ha) \<Rightarrow>
               (case v0_range b \<rho> of
                  None \<Rightarrow> None
                | Some (lb, hb) \<Rightarrow> Some (min la lb, max ha hb))))"
| "v0_range (Bin op l r) \<rho> =
     (case v0_range l \<rho> of
        None \<Rightarrow> None
      | Some (l0, l1) \<Rightarrow>
          (case v0_range r \<rho> of
             None \<Rightarrow> None
           | Some (r0, r1) \<Rightarrow>
               if op = ''+'' then Some (l0 + r0, l1 + r1)
               else if op = ''-'' then Some (l0 - r1, l1 - r0)
               else if op = ''*'' then
                 Some (min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))),
                       max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1))))
               else if op = ''**'' then
                 (if l0 = l1 \<and> r0 = r1 \<and> r0 \<ge> 0
                  then Some (l0 ^ nat r0, l0 ^ nat r0)
                  else None)
               else if op = ''%'' then
                 (if r0 = 0 \<or> r1 = 0 then None
                  else Some (min (l0 mod r0) (min (l0 mod r1) (min (l1 mod r0) (l1 mod r1))),
                             max (l0 mod r0) (max (l0 mod r1) (max (l1 mod r0) (l1 mod r1)))))
               else if op = ''//'' then
                 (if r0 = 0 \<or> r1 = 0 then None
                  else Some (min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))),
                             max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))))
               else if op = ''=='' \<or> op = ''!='' \<or> op = ''<'' \<or> op = ''>'' \<or> op = ''<='' \<or> op = ''>=''
                 then Some (0, 1)
               else None))"

text \<open>
  Differences from \<open>big_range\<close>, and nothing else: no float check on
  \<open>Cast\<close>; \<open>%\<close> and \<open>//\<close> guard only against a *zero endpoint*
  (hence never against an interior zero, and \<open>//\<close> not against straddling 0
  at all) and otherwise take corner min/max.
\<close>

section \<open>Integer-discreteness steps\<close>

text \<open>
  HOL's linear arithmetic stops at the rationals, so the two places where the
  published intervals step by one over \<open>int\<close> need \<open>presburger\<close>.
\<close>

lemma int_succ_le: "z < w \<Longrightarrow> z + 1 \<le> w"
  for z w :: int
  by presburger

lemma int_le_pred: "x < y \<Longrightarrow> x \<le> y - 1"
  for x y :: int
  by presburger

section \<open>Corner lemmas\<close>

text \<open>
  Multiplication: the value set over the box is bounded by the four corner
  products; each bound picks a corner by the sign of the *other* factor first,
  then the sign of the corner's own factor, which is exactly the case split the
  proof does.
\<close>

lemma mult_corners:
  fixes l0 l1 vl r0 r1 vr :: int
  assumes "l0 \<le> vl" "vl \<le> l1" "r0 \<le> vr" "vr \<le> r1"
  shows "min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))) \<le> vl * vr"
    and "vl * vr \<le> max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1)))"
proof -
  have l1ub: "0 \<le> vr \<Longrightarrow> vl * vr \<le> l1 * vr"
    by (rule mult_right_mono [OF assms(2)])
  have l0ub: "vr < 0 \<Longrightarrow> vl * vr \<le> l0 * vr"
    by (metis assms(1) mult_right_mono_neg order_less_imp_le)
  have l1lb: "vr < 0 \<Longrightarrow> l1 * vr \<le> vl * vr"
    by (metis assms(2) mult_right_mono_neg order_less_imp_le)
  have l0lb: "0 \<le> vr \<Longrightarrow> l0 * vr \<le> vl * vr"
    by (rule mult_right_mono [OF assms(1)])
  have up_ge: "0 \<le> li \<Longrightarrow> li * vr \<le> li * r1" for li
    by (rule mult_left_mono [OF assms(4)])
  have up_neg: "li < 0 \<Longrightarrow> li * vr \<le> li * r0" for li
    by (metis assms(3) mult_left_mono_neg order_less_imp_le)
  have lo_ge: "0 \<le> li \<Longrightarrow> li * r0 \<le> li * vr" for li
    by (rule mult_left_mono [OF assms(3)])
  have lo_neg: "li < 0 \<Longrightarrow> li * r1 \<le> li * vr" for li
    by (metis assms(4) mult_left_mono_neg order_less_imp_le)
  have k1: "l0 * r0 \<le> max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1)))"
    by (rule max.cobounded1)
  have k2: "l0 * r1 \<le> max (l0 * r1) (max (l1 * r0) (l1 * r1))"
    by (rule max.cobounded1)
  have k3: "l1 * r0 \<le> max (l1 * r0) (l1 * r1)"
    by (rule max.cobounded1)
  have k4: "l1 * r1 \<le> max (l1 * r0) (l1 * r1)"
    by (rule max.cobounded2)
  have j2: "max (l0 * r1) (max (l1 * r0) (l1 * r1))
            \<le> max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1)))"
    by (rule max.cobounded2)
  have j3: "max (l1 * r0) (l1 * r1)
            \<le> max (l0 * r1) (max (l1 * r0) (l1 * r1))"
    by (rule max.cobounded2)
  have n1: "min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))) \<le> l0 * r0"
    by (rule min.cobounded1)
  have n2: "min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1)))
            \<le> min (l0 * r1) (min (l1 * r0) (l1 * r1))"
    by (rule min.cobounded2)
  have n3: "min (l0 * r1) (min (l1 * r0) (l1 * r1)) \<le> min (l1 * r0) (l1 * r1)"
    by (rule min.cobounded2)
  have n2c: "min (l0 * r1) (min (l1 * r0) (l1 * r1)) \<le> l0 * r1"
    by (rule min.cobounded1)
  have n3c: "min (l1 * r0) (l1 * r1) \<le> l1 * r0"
    by (rule min.cobounded1)
  have n4c: "min (l1 * r0) (l1 * r1) \<le> l1 * r1"
    by (rule min.cobounded2)
  have upper: "vl * vr \<le> max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1)))"
  proof (cases "0 \<le> vr")
    case True
    have le: "vl * vr \<le> l1 * vr" by (rule l1ub [OF True])
    show ?thesis
    proof (cases "0 \<le> l1")
      case True
      have "l1 * vr \<le> l1 * r1" by (rule up_ge [OF True])
      with le k4 j3 j2 show ?thesis by (meson order_trans)
    next
      case False
      have nz: "l1 < 0" using False by simp
      have "l1 * vr \<le> l1 * r0" by (rule up_neg [OF nz])
      with le k3 j3 j2 show ?thesis by (meson order_trans)
    qed
  next
    case False
    have nn: "vr < 0" using False by simp
    have le: "vl * vr \<le> l0 * vr" by (rule l0ub [OF nn])
    show ?thesis
    proof (cases "0 \<le> l0")
      case True
      have "l0 * vr \<le> l0 * r1" by (rule up_ge [OF True])
      with le k2 j2 show ?thesis by (meson order_trans)
    next
      case False
      have nz: "l0 < 0" using False by simp
      have "l0 * vr \<le> l0 * r0" by (rule up_neg [OF nz])
      with le k1 show ?thesis by (meson order_trans)
    qed
  qed
  have lower: "min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))) \<le> vl * vr"
  proof (cases "0 \<le> vr")
    case True
    have ge: "l0 * vr \<le> vl * vr" by (rule l0lb [OF True])
    show ?thesis
    proof (cases "0 \<le> l0")
      case True
      have "l0 * r0 \<le> l0 * vr" by (rule lo_ge [OF True])
      with ge n1 show ?thesis by (meson order_trans)
    next
      case False
      have nz: "l0 < 0" using False by simp
      have "l0 * r1 \<le> l0 * vr" by (rule lo_neg [OF nz])
      with ge n2 n2c show ?thesis by (meson order_trans)
    qed
  next
    case False
    have nn: "vr < 0" using False by simp
    have ge: "l1 * vr \<le> vl * vr" by (rule l1lb [OF nn])
    show ?thesis
    proof (cases "0 \<le> l1")
      case True
      have "l1 * r0 \<le> l1 * vr" by (rule lo_ge [OF True])
      with ge n2 n3 n3c show ?thesis by (meson order_trans)
    next
      case False
      have nz: "l1 < 0" using False by simp
      have "l1 * r1 \<le> l1 * vr" by (rule lo_neg [OF nz])
      with ge n2 n3 n4c show ?thesis by (meson order_trans)
    qed
  qed
  then show "min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))) \<le> vl * vr"
    by blast
  show "vl * vr \<le> max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1)))"
    by (rule upper)
qed

text \<open>
  Division. The dividend side is monotone for any sign under a positive divisor
  (\<open>zdiv_mono1\<close>); the divisor side needs the dividend's sign
  (\<open>zdiv_mono2\<close>/\<open>zdiv_mono2_neg\<close>) and a positive divisor range --
  which is exactly the restriction the fixed \<open>//\<close> arm imposes.
\<close>

lemma div_corners:
  fixes l0 l1 vl r0 r1 vr :: int
  assumes "l0 \<le> vl" "vl \<le> l1" "0 < r0" "r0 \<le> vr" "vr \<le> r1"
  shows "min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))) \<le> vl div vr"
    and "vl div vr \<le> max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))"
proof -
  have vr_pos: "0 < vr"
    by (rule order_less_le_trans [OF assms(3) assms(4)])
  have l1ub: "vl div vr \<le> l1 div vr"
    by (rule zdiv_mono1 [OF assms(2) vr_pos])
  have l0lb: "l0 div vr \<le> vl div vr"
    by (rule zdiv_mono1 [OF assms(1) vr_pos])
  have up_ge: "0 \<le> li \<Longrightarrow> li div vr \<le> li div r0" for li
    by (rule zdiv_mono2 [OF _ assms(3) assms(4)])
  have up_neg: "li < 0 \<Longrightarrow> li div vr \<le> li div r1" for li
    by (rule zdiv_mono2_neg [OF _ vr_pos assms(5)])
  have lo_ge: "0 \<le> li \<Longrightarrow> li div r1 \<le> li div vr" for li
    by (rule zdiv_mono2 [OF _ vr_pos assms(5)])
  have lo_neg: "li < 0 \<Longrightarrow> li div r0 \<le> li div vr" for li
    by (rule zdiv_mono2_neg [OF _ assms(3) assms(4)])
  have k1: "l0 div r0 \<le> max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))"
    by (rule max.cobounded1)
  have k2: "l0 div r1 \<le> max (l0 div r1) (max (l1 div r0) (l1 div r1))"
    by (rule max.cobounded1)
  have k3: "l1 div r0 \<le> max (l1 div r0) (l1 div r1)"
    by (rule max.cobounded1)
  have k4: "l1 div r1 \<le> max (l1 div r0) (l1 div r1)"
    by (rule max.cobounded2)
  have j2: "max (l0 div r1) (max (l1 div r0) (l1 div r1))
            \<le> max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))"
    by (rule max.cobounded2)
  have j3: "max (l1 div r0) (l1 div r1)
            \<le> max (l0 div r1) (max (l1 div r0) (l1 div r1))"
    by (rule max.cobounded2)
  have n1: "min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))) \<le> l0 div r0"
    by (rule min.cobounded1)
  have n2: "min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1)))
            \<le> min (l0 div r1) (min (l1 div r0) (l1 div r1))"
    by (rule min.cobounded2)
  have n3: "min (l0 div r1) (min (l1 div r0) (l1 div r1)) \<le> min (l1 div r0) (l1 div r1)"
    by (rule min.cobounded2)
  have n2c: "min (l0 div r1) (min (l1 div r0) (l1 div r1)) \<le> l0 div r1"
    by (rule min.cobounded1)
  have n3c: "min (l1 div r0) (l1 div r1) \<le> l1 div r0"
    by (rule min.cobounded1)
  have n4c: "min (l1 div r0) (l1 div r1) \<le> l1 div r1"
    by (rule min.cobounded2)
  have upper: "vl div vr \<le> max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))"
  proof (cases "0 \<le> l1")
    case True
    have le: "vl div vr \<le> l1 div vr" by (rule l1ub)
    have "l1 div vr \<le> l1 div r0" by (rule up_ge [OF True])
    with le k3 j3 j2 show ?thesis by (meson order_trans)
  next
    case False
    have le: "vl div vr \<le> l1 div vr" by (rule l1ub)
    have nz: "l1 < 0" using False by simp
    have "l1 div vr \<le> l1 div r1" by (rule up_neg [OF nz])
    with le k4 j3 j2 show ?thesis by (meson order_trans)
  qed
  have lower: "min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))) \<le> vl div vr"
  proof (cases "0 \<le> l0")
    case True
    have ge: "l0 div vr \<le> vl div vr" by (rule l0lb)
    have "l0 div r1 \<le> l0 div vr" by (rule lo_ge [OF True])
    with ge n2 n2c show ?thesis by (meson order_trans)
  next
    case False
    have ge: "l0 div vr \<le> vl div vr" by (rule l0lb)
    have nz: "l0 < 0" using False by simp
    have "l0 div r0 \<le> l0 div vr" by (rule lo_neg [OF nz])
    with ge n1 show ?thesis by (meson order_trans)
  qed
  then show "min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))) \<le> vl div vr"
    by blast
  show "vl div vr \<le> max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))"
    by (rule upper)
qed

text \<open>
  Floor-mod by the sign of the divisor, with no dividend bound -- the property
  the old comment claimed and the corner loop lacked. \<open>pos_mod_bound\<close>,
  \<open>neg_mod_bound\<close>, \<open>pos_mod_sign\<close>, \<open>neg_mod_sign\<close> are all \<open>[simp]\<close>
  in HOL; only the integer steps are done by hand.
\<close>

lemma mod_pos_bounds:
  fixes r0 r1 vr v :: int
  assumes "r0 \<le> vr" "vr \<le> r1"
  shows "0 < r0 \<Longrightarrow> 0 \<le> v mod vr \<and> v mod vr \<le> r1 - 1"
proof -
  assume a: "0 < r0"
  have vr_pos: "0 < vr"
    by (rule order_less_le_trans [OF a assms(1)])
  have "0 \<le> v mod vr"
    by (rule pos_mod_sign [OF vr_pos])
  moreover have "v mod vr < vr"
    by (rule pos_mod_bound [OF vr_pos])
  then have "v mod vr < r1"
    using assms(2) by linarith
  then have "v mod vr \<le> r1 - 1"
    by (rule int_le_pred)
  ultimately show "0 \<le> v mod vr \<and> v mod vr \<le> r1 - 1"
    by blast
qed

lemma mod_neg_bounds:
  fixes r0 r1 vr v :: int
  assumes "r0 \<le> vr" "vr \<le> r1"
  shows "r1 < 0 \<Longrightarrow> r0 + 1 \<le> v mod vr \<and> v mod vr \<le> 0"
proof -
  assume a: "r1 < 0"
  have vr_neg: "vr < 0"
    using a assms(2) by linarith
  have "vr < v mod vr"
    by (rule neg_mod_bound [OF vr_neg])
  then have "vr + 1 \<le> v mod vr"
    by (rule int_succ_le)
  then have lo: "r0 + 1 \<le> v mod vr"
    using assms(1) by linarith
  have "v mod vr \<le> 0"
    by (rule neg_mod_sign [OF vr_neg])
  with lo show "r0 + 1 \<le> v mod vr \<and> v mod vr \<le> 0"
    by blast
qed

section \<open>The obligation of the fixed pass\<close>

text \<open>
  The pass reads an environment state and produces a range; its soundness is
  the Rust's contract exactly: *given* a state whose ranges contain the
  environment's values, a produced range contains the expression's value.
  The state-soundness premise is what makes the variable case true -- without
  it there is nothing relating \<open>\<rho>\<close> to \<open>\<nu>\<close>.
\<close>

definition env_sound :: "(string \<Rightarrow> (int \<times> int) option) \<Rightarrow> (string \<Rightarrow> int) \<Rightarrow> bool" where
  "env_sound \<rho> \<nu> \<longleftrightarrow> (\<forall> x l h. \<rho> x = Some (l, h) \<longrightarrow> l \<le> \<nu> x \<and> \<nu> x \<le> h)"

theorem bigint_range_sound:
  "env_sound \<rho> \<nu> \<Longrightarrow> big_range a \<rho> = Some (lo, hi) \<Longrightarrow> eval \<nu> a = Some v \<Longrightarrow> lo \<le> v \<and> v \<le> hi"
proof (induction a arbitrary: lo hi v \<nu> \<rho>)
  case (Num n)
  from Num show ?case by simp
next
  case (Var x)
  have es: "env_sound \<rho> \<nu>" by (rule Var.prems(1))
  from Var.prems(2) have r: "\<rho> x = Some (lo, hi)" by simp
  from Var.prems(3) have e: "\<nu> x = v" by simp
  from es r have "lo \<le> \<nu> x \<and> \<nu> x \<le> hi" by (auto simp: env_sound_def)
  moreover from e have "v = \<nu> x" by simp
  ultimately show ?case by simp
next
  case Refuse
  from Refuse show ?case by simp
next
  case (Cast f a1)
  have es: "env_sound \<rho> \<nu>" by (rule Cast.prems(1))
  show ?case
  proof (cases f)
    case True
    with Cast.prems show ?thesis by simp
  next
    case False
    from False Cast.prems(2) have r: "big_range a1 \<rho> = Some (lo, hi)" by simp
    from False Cast.prems(3) have e: "eval \<nu> a1 = Some v" by simp
    from Cast.IH [OF es r e] show ?thesis by blast
  qed
next
  case (UnOp op a1)
  have es: "env_sound \<rho> \<nu>" by (rule UnOp.prems(1))
  show ?case
  proof (cases "op = ''-''")
    case True
    show ?thesis
    proof (cases "big_range a1 \<rho>")
      case None
      with UnOp.prems True show ?thesis by simp
    next
      case (Some p)
      then obtain l h where lr: "big_range a1 \<rho> = Some (l, h)"
        by (metis option.inject prod.exhaust_sel)
      from UnOp.prems True lr have vals: "lo = - h \<and> hi = - l" by simp
      show ?thesis
      proof (cases "eval \<nu> a1")
        case None
        with UnOp.prems True show ?thesis by simp
      next
        case (Some z)
        have ev: "eval \<nu> a1 = Some z" using Some by blast
        have "eval \<nu> (UnOp op a1) = Some (- z)" using True Some by simp
        with UnOp.prems(3) have vv: "v = - z" by simp
        from UnOp.IH [OF es lr ev] have "l \<le> z \<and> z \<le> h" .
        with vals vv show ?thesis by simp
      qed
    qed
  next
    case False
    with UnOp.prems show ?thesis by simp
  qed
next
  case (Cond t a1 b)
  have es: "env_sound \<rho> \<nu>" by (rule Cond.prems(1))
  show ?case
  proof (cases "big_range t \<rho>")
    case None
    with Cond.prems show ?thesis by simp
  next
    case (Some tt)
    note tr = Some
    show ?thesis
    proof (cases "big_range a1 \<rho>")
      case None
      with Cond.prems Some show ?thesis by simp
    next
      case (Some p)
      then obtain la ha where ar: "big_range a1 \<rho> = Some (la, ha)"
        by (metis option.inject prod.exhaust_sel)
      show ?thesis
      proof (cases "big_range b \<rho>")
        case None
        with Cond.prems tr ar show ?thesis by simp
      next
        case (Some q)
        then obtain lb hb where br: "big_range b \<rho> = Some (lb, hb)"
          by (metis option.inject prod.exhaust_sel)
        from Cond.prems tr ar br have vals: "lo = min la lb \<and> hi = max ha hb" by simp
        show ?thesis
        proof (cases "eval \<nu> t")
          case None
          with Cond.prems show ?thesis by simp
        next
          case (Some tv0)
          have evt: "eval \<nu> t = Some tv0" using Some by blast
          show ?thesis
          proof (cases "tv0 = 0")
            case True
            have "eval \<nu> (Cond t a1 b) = eval \<nu> b" using Some evt True by simp
            with Cond.prems(3) have evb: "eval \<nu> b = Some v" by simp
            from Cond.IH(3) [OF es br evb] have "lb \<le> v \<and> v \<le> hb" .
            with vals show ?thesis by (metis max.cobounded2 min.cobounded2 order_trans)
          next
            case False
            have "eval \<nu> (Cond t a1 b) = eval \<nu> a1" using Some evt False by simp
            with Cond.prems(3) have eva: "eval \<nu> a1 = Some v" by simp
            from Cond.IH(2) [OF es ar eva] have "la \<le> v \<and> v \<le> ha" .
            with vals show ?thesis by (metis max.cobounded1 min.cobounded1 order_trans)
          qed
        qed
      qed
    qed
  qed
next
  case (Bin op a1 a2)
  have es: "env_sound \<rho> \<nu>" by (rule Bin.prems(1))
  show ?case
  proof (cases "big_range a1 \<rho>")
    case None
    with Bin.prems show ?thesis by simp
  next
    case (Some p1)
    then obtain l0 l1 where lr1: "big_range a1 \<rho> = Some (l0, l1)"
      by (metis option.inject prod.exhaust_sel)
    show ?thesis
    proof (cases "big_range a2 \<rho>")
      case None
      with Bin.prems Some show ?thesis by simp
    next
      case (Some p2)
      then obtain r0 r1 where lr2: "big_range a2 \<rho> = Some (r0, r1)"
        by (metis option.inject prod.exhaust_sel)
      show ?thesis
      proof (cases "eval \<nu> a1")
        case None
        with Bin.prems show ?thesis by simp
      next
        case (Some v1)
        note evl = Some
        show ?thesis
        proof (cases "eval \<nu> a2")
          case None
          with Bin.prems evl show ?thesis by simp
        next
          case (Some v2)
          note evr = Some
          from Bin.IH(1) [OF es lr1 evl] have lb: "l0 \<le> v1 \<and> v1 \<le> l1" .
          from Bin.IH(2) [OF es lr2 evr] have rb: "r0 \<le> v2 \<and> v2 \<le> r1" .
          from lb have lb1: "l0 \<le> v1" and lb2: "v1 \<le> l1" by blast+
          from rb have rb1: "r0 \<le> v2" and rb2: "v2 \<le> r1" by blast+
          show ?thesis
          proof (cases "op = ''+''")
            case True
            from Bin.prems lr1 lr2 True have vals: "lo = l0 + r0 \<and> hi = l1 + r1" by simp
            moreover from Bin.prems(3) evl evr True have ev: "v = v1 + v2" by simp
            ultimately show ?thesis using lb1 lb2 rb1 rb2 by linarith
          next
            case False
            note ne1 = False
            show ?thesis
            proof (cases "op = ''-''")
              case True
              from Bin.prems lr1 lr2 True have vals: "lo = l0 - r1 \<and> hi = l1 - r0" by simp
              moreover from Bin.prems(3) evl evr True have ev: "v = v1 - v2" by simp
              ultimately show ?thesis using lb1 lb2 rb1 rb2 by linarith
            next
              case False
              note ne2 = False
              show ?thesis
              proof (cases "op = ''*''")
                case True
                from Bin.prems lr1 lr2 True
                have vals: "lo = min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))) \<and>
                            hi = max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1)))" by simp
                moreover from Bin.prems(3) evl evr True have ev: "v = v1 * v2" by simp
                ultimately show ?thesis using mult_corners [OF lb1 lb2 rb1 rb2] by blast
              next
                case False
                note ne3 = False
                show ?thesis
                proof (cases "op = ''**''")
                  case True
                  show ?thesis
                  proof (cases "l0 = l1 \<and> r0 = r1 \<and> r0 \<ge> 0")
                    case pc: True
                    from lb1 lb2 rb1 rb2 pc have eqs: "v1 = l0 \<and> v2 = r0" by linarith
                    from Bin.prems lr1 lr2 True pc have vals: "lo = l0 ^ nat r0 \<and> hi = l0 ^ nat r0" by simp
                    from Bin.prems(3) evl evr True have ev: "v = v1 ^ nat v2" by (simp split: if_split_asm)
                    from vals ev eqs show ?thesis by simp
                  next
                    case nc: False
                    with Bin.prems lr1 lr2 True show ?thesis by simp
                  qed
                next
                  case False
                  note ne4 = False
                  show ?thesis
                  proof (cases "op = ''%''")
                    case op: True
                    show ?thesis
                    proof (cases "l0 = l1 \<and> r0 = r1")
                      case pt: True
                      show ?thesis
                      proof (cases "r0 = 0")
                        case True
                        with Bin.prems lr1 lr2 op pt show ?thesis by simp
                      next
                        case False
                        from Bin.prems lr1 lr2 op pt False have vals: "lo = l0 mod r0 \<and> hi = l0 mod r0" by simp
                        from lb1 lb2 rb1 rb2 pt have eqs: "v1 = l0 \<and> v2 = r0" by linarith
                        from Bin.prems(3) evl evr op False eqs have ev: "v = v1 mod v2" by simp
                        from vals ev eqs show ?thesis by simp
                      qed
                    next
                      case np: False
                      show ?thesis
                      proof (cases "r0 > 0")
                        case True
                        from Bin.prems lr1 lr2 op np True have vals: "lo = 0 \<and> hi = r1 - 1" by simp
                        from Bin.prems(3) evl evr op have ev: "v = v1 mod v2" by (simp split: if_split_asm)
                        from mod_pos_bounds [OF rb1 rb2 True] have mb: "0 \<le> v mod v2 \<and> v mod v2 \<le> r1 - 1" by blast
                        from ev have "v mod v2 = v" by simp
                        with mb have "0 \<le> v \<and> v \<le> r1 - 1" by simp
                        with vals show ?thesis by simp
                      next
                        case False
                        show ?thesis
                        proof (cases "r1 < 0")
                          case True
                          from Bin.prems lr1 lr2 op np False True have vals: "lo = r0 + 1 \<and> hi = 0" by simp
                          from Bin.prems(3) evl evr op have ev: "v = v1 mod v2" by (simp split: if_split_asm)
                          from mod_neg_bounds [OF rb1 rb2 True] have mb: "r0 + 1 \<le> v mod v2 \<and> v mod v2 \<le> 0" by blast
                          from ev have "v mod v2 = v" by simp
                          with mb have "r0 + 1 \<le> v \<and> v \<le> 0" by simp
                          with vals show ?thesis by simp
                        next
                          case nn: False
                          with Bin.prems lr1 lr2 op np False show ?thesis by simp
                        qed
                      qed
                    qed
                  next
                    case False
                    note ne5 = False
                    show ?thesis
                    proof (cases "op = ''//''")
                      case op: True
                      show ?thesis
                      proof (cases "r0 > 0")
                        case True
                        from Bin.prems lr1 lr2 op True
                        have vals: "lo = min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))) \<and>
                                    hi = max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))" by simp
                        moreover from Bin.prems(3) evl evr op have ev: "v = v1 div v2" by (simp split: if_split_asm)
                        ultimately show ?thesis
                          by (metis div_corners [OF lb1 lb2 True rb1 rb2])
                      next
                        case False
                        with Bin.prems lr1 lr2 op show ?thesis by simp
                      qed
                    next
                      case False
                      note ne6 = False
                      show ?thesis
                      proof (cases "op = ''=='' \<or> op = ''!='' \<or> op = ''<'' \<or> op = ''>'' \<or> op = ''<='' \<or> op = ''>=''")
                        case cmp: True
                        from cmp consider (ceq) "op = ''==''" | (cne) "op = ''!=''" |
                          (clt) "op = ''<''" | (cgt) "op = ''>''" |
                          (cle) "op = ''<=''" | (cge) "op = ''>=''" by blast
                        then show ?thesis
                        proof cases
                          case ceq
                          from Bin.prems lr1 lr2 ceq have vals: "lo = 0 \<and> hi = 1" by simp
                          from Bin.prems(3) evl evr ceq have ev: "v = (if v1 = v2 then 1 else 0)" by simp
                          with vals show ?thesis by (simp split: if_split)
                        next
                          case cne
                          from Bin.prems lr1 lr2 cne have vals: "lo = 0 \<and> hi = 1" by simp
                          from Bin.prems(3) evl evr cne have ev: "v = (if v1 \<noteq> v2 then 1 else 0)" by simp
                          with vals show ?thesis by (simp split: if_split)
                        next
                          case clt
                          from Bin.prems lr1 lr2 clt have vals: "lo = 0 \<and> hi = 1" by simp
                          from Bin.prems(3) evl evr clt have ev: "v = (if v1 < v2 then 1 else 0)" by simp
                          with vals show ?thesis by (simp split: if_split)
                        next
                          case cgt
                          from Bin.prems lr1 lr2 cgt have vals: "lo = 0 \<and> hi = 1" by simp
                          from Bin.prems(3) evl evr cgt have ev: "v = (if v1 > v2 then 1 else 0)" by simp
                          with vals show ?thesis by (simp split: if_split)
                        next
                          case cle
                          from Bin.prems lr1 lr2 cle have vals: "lo = 0 \<and> hi = 1" by simp
                          from Bin.prems(3) evl evr cle have ev: "v = (if v1 \<le> v2 then 1 else 0)" by simp
                          with vals show ?thesis by (simp split: if_split)
                        next
                          case cge
                          from Bin.prems lr1 lr2 cge have vals: "lo = 0 \<and> hi = 1" by simp
                          from Bin.prems(3) evl evr cge have ev: "v = (if v1 \<ge> v2 then 1 else 0)" by simp
                          with vals show ?thesis by (simp split: if_split)
                        qed
                      next
                        case False
                        with Bin.prems lr1 lr2 ne1 ne2 ne3 ne4 ne5 ne6 show ?thesis by simp
                      qed
                    qed
                  qed
                qed
              qed
            qed
          qed
        qed
      qed
    qed
  qed
qed

section \<open>Negative control: the pre-fix pass is unsound\<close>

text \<open>
  Three counterexamples, one per arithmetic gap. Each is a closed expression
  whose range \<open>v0_range\<close> claims while \<open>eval\<close> produces a value
  outside it. The fourth fact shows the float-cast gap the other way: v0 names
  a range for a value the model refuses to read.
\<close>

lemma ex_mod_corners:
  "v0_range (Bin ''%'' (Var ''x'') (Num 30)) (\<lambda>_. Some (0, 100)) = Some (0, 10)"
  "eval (\<lambda>_. 29) (Bin ''%'' (Var ''x'') (Num 30)) = Some 29"
  by simp_all

lemma ex_div_straddle:
  "v0_range (Bin ''//'' (Num 6) (Var ''b'')) (\<lambda>_. Some (- 5, 3)) = Some (- 2, 2)"
  "eval (\<lambda>_. - 1) (Bin ''//'' (Num 6) (Var ''b'')) = Some (- 6)"
  by simp_all

lemma ex_mod_negative:
  "v0_range (Bin ''%'' (Num 100) (Var ''b'')) (\<lambda>_. Some (- 9, - 3)) = Some (- 8, - 2)"
  "eval (\<lambda>_. - 4) (Bin ''%'' (Num 100) (Var ''b'')) = Some 0"
  by simp_all

lemma ex_cast_float:
  "v0_range (Cast True (Num 5)) (\<lambda>_. None) = Some (5, 5)"
  "big_range (Cast True (Num 5)) (\<lambda>_. None) = None"
  "eval (\<lambda>_. 0) (Cast True (Num 5)) = None"
  by simp_all

theorem v0_not_sound:
  "\<exists> a \<rho> \<nu> lo hi v. v0_range a \<rho> = Some (lo, hi) \<and> eval \<nu> a = Some v \<and> (v < lo \<or> hi < v)"
proof -
  let ?a = "Bin ''%'' (Var ''x'') (Num 30)"
  let ?\<rho> = "(\<lambda>_. Some (0, 100))"
  let ?\<nu> = "(\<lambda>_. 29)"
  have "v0_range ?a ?\<rho> = Some (0, 10) \<and> eval ?\<nu> ?a = Some 29 \<and> (29 < (0::int) \<or> 10 < (29::int))"
    by simp
  then show ?thesis by blast
qed

section \<open>The fixed pass on the same inputs\<close>

text \<open>
  On the counterexamples: the corner interval for \<open>%\<close> widens to the
  sound sign-of-divisor interval, and the two inputs the fixed pass cannot
  bound are refused (\<open>None\<close>). For the float cast, the fix refuses
  where v0 named a range.
\<close>

lemma fix_handles_the_examples:
  "big_range (Bin ''%'' (Var ''x'') (Num 30)) (\<lambda>_. Some (0, 100)) = Some (0, 29)"
  and "big_range (Bin ''//'' (Num 6) (Var ''b'')) (\<lambda>_. Some (- 5, 3)) = None"
  and "big_range (Bin ''%'' (Num 100) (Var ''b'')) (\<lambda>_. Some (- 9, - 3)) = Some (- 8, 0)"
  by simp_all

end
