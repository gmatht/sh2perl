theory IntervalFloor
  imports BigIntRange.BigIntRange
begin

section \<open>The i128 floor walk of interval.rs, mirrored from the Rust\<close>

text \<open>
  Mirror of \ \ \<open>sh2perl/src/shir_passes/interval.rs\<close>: the Python
  (\Floor-modulo) transfer functions \ \ \<open>arith_floor_range\<close> (the
  recursive \ \ \<open>+\<close>/\<open>-\<close>/\<open>*\<close> interval evaluator) and
  \ \ \<open>floor_arith_range\<close> (the top-level \ \ \<open>%\<close>/\<open>/\<close> rule that
  refines an assignment's range). This is the i128 walk the
  BigInt-range pass merges with; \ \ \<open>ctx.var_ranges\<close> is the verdict
  every backend reads, so the transfer functions are exactly Tier 1
  (verdict \<Rightarrow> fact, intervals and names only -- the statement
  traversal of \ \ \<open>refine_floor_mod\<close> is Tier 3 and stays unmirrored,
  as \ \ \<open>big_ir_range\<close>'s did).

  Everything here reuses the BigIntRange session's skeleton: the same
  \ \ \<open>arith\<close> tree, the same Python-int \ \ \<open>eval\<close>, the same
  \ \ \<open>env_sound\<close> premise, the same corner lemmas
  (\ \ \<open>mult_corners\<close>, \ \ \<open>div_corners\<close>, \ \ \<open>mod_pos_bounds\<close>) --
  which is the point of proving them there.

  One bug fell out of mirroring, and it is a different *kind* from the
  last round's: not a wrong rule but a wrong width. The \ \ \<open>+\<close> and
  \ \ \<open>-\<close> arms (and every other corner site in the tree --
  \ \ \<open>shir.rs\<close>'s checked walk, three sites in \ \ \<open>c_backend.rs\<close>)
  use \ \ \<open>checked_*\<close>; the \ \ \<open>*\<close> arm computed its four corner
  products with a raw \ \ \<open>*\<close>. On overflow that panics in a debug
  build and, in release, wraps -- and a wrapped corner is a silently
  wrong verdict, which is the same defect class as
  \ \ \<open>bigint=10000000000000000000 presenting as
  -8446744073709551616\<close> that the BigInt pass was built to dodge, one
  level down. The fix makes \ \ \<open>*\<close> use \ \ \<open>checked_mul\<close> like its
  siblings; \ \ \<open>v0_range\<close> below mirrors the pre-fix behaviour
  (release wrap, modelled by \ \ \<open>wrap128\<close>) and \ \ \<open>v0_not_sound\<close>
  machine-checks that it names a wrong interval.

  The bridge between Rust and this model, stated once: the fixed
  \ \ \<open>checked_*\<close> ops return \ \ \<open>Some p\<close> exactly when the
  mathematical result is representable in \ \ \<open>i128\<close>, and then
  \ \ \<open>p\<close> *is* that result -- so the mirror is the exact \ \ \<open>int\<close>
  arithmetic whenever the code returns anything at all, and refusal
  otherwise. That is definitional in the Rust, not a proof
  obligation here (the representation step is the standard
  word-vs-int bridge the other sessions leave to \ \ \<open>C_UB_PROOF.md\<close>).
\<close>

definition wrap128 :: "int \<Rightarrow> int" where
  "wrap128 x = (x + 2 ^ 127) mod 2 ^ 128 - 2 ^ 127"

section \<open>The recursive interval evaluator (mirrors arith\_floor\_range)\<close>

text \<open>
  Arm-by-arm: \ \ \<open>Num\<close> is a point (the Rust's \ \ \<open>*i as i128\<close> cast
  from the \ \ \<open>i32\<close> literal is exact); \ \ \<open>Var\<close> reads the state;
  \ \ \<open>+\<close>/\<open>-\<close> add the endpoints (the Rust guards with
  \ \ \<open>checked_add/sub\<close>, which -- per the bridge -- refuses exactly
  what this model never reaches); \ \ \<open>*\<close> is min/max over the four
  corner products; everything else -- including \ \ \<open>%\<close>/\<open>/\<close> as
  *children*, casts, unary minus -- refuses.
\<close>

fun floor_range :: "arith \<Rightarrow> (string \<Rightarrow> (int \<times> int) option) \<Rightarrow> (int \<times> int) option" where
  "floor_range (Num n) \<rho> = Some (n, n)"
| "floor_range (Var x) \<rho> = \<rho> x"
| "floor_range Refuse \<rho> = None"
| "floor_range (Cast f a) \<rho> = None"
| "floor_range (UnOp op a) \<rho> = None"
| "floor_range (Cond t a b) \<rho> = None"
| "floor_range (Bin op l r) \<rho> =
     (case floor_range l \<rho> of
        None \<Rightarrow> None
      | Some (l0, l1) \<Rightarrow>
          (case floor_range r \<rho> of
             None \<Rightarrow> None
           | Some (r0, r1) \<Rightarrow>
               if op = ''+'' then Some (l0 + r0, l1 + r1)
               else if op = ''-'' then Some (l0 - r1, l1 - r0)
               else if op = ''*'' then
                 Some (min (l0 * r0) (min (l0 * r1) (min (l1 * r0) (l1 * r1))),
                       max (l0 * r0) (max (l0 * r1) (max (l1 * r0) (l1 * r1))))
               else None))"

section \<open>The top-level \ \ \<open>%\<close>/\<open>/\<close> rule (mirrors floor\_arith\_range)\<close>

text \<open>
  Called only on assignment right-hand sides, and only for the two
  operators: \ \ \<open>%\<close> clamps to \ \ \<open>[0, r1 - 1]\<close> under the code's
  (stronger-than-necessary) premises \ \ \<open>l0 \<ge> 0 \<and> r0 > 0\<close> -- the
  dividend bound is not needed for the fact itself, as
  \ \ \<open>mod_pos_bounds\<close> proves and the bigint-range fix exploited --
  and \ \ \<open>/\<close> (Python's \ \ \<open>//\<close> lowers as \ \ \<open>/\<close>) takes the four
  endpoint quotients, exact because both operands are then non-negative,
  where truncation and floor coincide. Anywhere else, no refinement.
\<close>

fun top_range :: "arith \<Rightarrow> (string \<Rightarrow> (int \<times> int) option) \<Rightarrow> (int \<times> int) option" where
  "top_range (Bin op l r) \<rho> =
     (case floor_range l \<rho> of
        None \<Rightarrow> None
      | Some (l0, l1) \<Rightarrow>
          (case floor_range r \<rho> of
             None \<Rightarrow> None
           | Some (r0, r1) \<Rightarrow>
               if op = ''%'' then
                 (if l0 \<ge> 0 \<and> r0 > 0 then Some (0, r1 - 1) else None)
               else if op = ''/'' then
                 (if l0 \<ge> 0 \<and> r0 > 0 then
                    Some (min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))),
                          max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1))))
                  else None)
               else None))"
| "top_range a \<rho> = None"

section \<open>The pre-fix evaluator (negative control)\<close>

text \<open>
  Identical to \ \ \<open>floor_range\<close> except the \ \ \<open>*\<close> arm: the corner
  products are computed and then wrapped to \ \ \<open>i128\<close>, which is what
  the release build's raw \ \ \<open>*\<close> does. (The debug build panics
  instead -- same reachability, louder.)
\<close>

fun v0_range :: "arith \<Rightarrow> (string \<Rightarrow> (int \<times> int) option) \<Rightarrow> (int \<times> int) option" where
  "v0_range (Num n) \<rho> = Some (n, n)"
| "v0_range (Var x) \<rho> = \<rho> x"
| "v0_range Refuse \<rho> = None"
| "v0_range (Cast f a) \<rho> = None"
| "v0_range (UnOp op a) \<rho> = None"
| "v0_range (Cond t a b) \<rho> = None"
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
                 Some (min (wrap128 (l0 * r0)) (min (wrap128 (l0 * r1))
                       (min (wrap128 (l1 * r0)) (wrap128 (l1 * r1)))),
                       max (wrap128 (l0 * r0)) (max (wrap128 (l0 * r1))
                       (max (wrap128 (l1 * r0)) (wrap128 (l1 * r1)))))
               else None))"

section \<open>The obligations\<close>

lemma floor_range_sound:
  "env_sound \<rho> \<nu> \<Longrightarrow> floor_range a \<rho> = Some (lo, hi) \<Longrightarrow> eval \<nu> a = Some v \<Longrightarrow> lo \<le> v \<and> v \<le> hi"
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
  from Cast.prems show ?case by simp
next
  case (UnOp op a1)
  from UnOp.prems show ?case by simp
next
  case (Cond t a1 b)
  from Cond.prems show ?case by simp
next
  case (Bin op a1 a2)
  have es: "env_sound \<rho> \<nu>" by (rule Bin.prems(1))
  show ?case
  proof (cases "floor_range a1 \<rho>")
    case None
    with Bin.prems show ?thesis by simp
  next
    case (Some p1)
    then obtain l0 l1 where lr1: "floor_range a1 \<rho> = Some (l0, l1)"
      by (metis option.inject prod.exhaust_sel)
    show ?thesis
    proof (cases "floor_range a2 \<rho>")
      case None
      with Bin.prems Some show ?thesis by simp
    next
      case (Some p2)
      then obtain r0 r1 where lr2: "floor_range a2 \<rho> = Some (r0, r1)"
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
                with Bin.prems lr1 lr2 ne1 ne2 show ?thesis by simp
              qed
            qed
          qed
        qed
      qed
    qed
  qed
qed

text \<open>
  The \ \ \<open>%\<close>/\<open>/\<close> refinement of an assignment's range, and the
  narrow-only intersection step beside it. \ \ \<open>refine_floor_mod\<close>
  intersects the refined interval with the existing one and keeps the
  intersection only when it is non-empty -- sound exactly when both
  intervals cover the value, which is what the lemma records.
\<close>

lemma intersect_narrows_sound:
  assumes "lo \<le> v" "v \<le> hi" "el \<le> v" "v \<le> eh"
  shows "max lo el \<le> v \<and> v \<le> min hi eh"
  unfolding max_def min_def using assms by (simp split: if_split_asm)

theorem interval_floor_sound:
  "env_sound \<rho> \<nu> \<Longrightarrow> top_range a \<rho> = Some (lo, hi) \<Longrightarrow> eval \<nu> a = Some v \<Longrightarrow> lo \<le> v \<and> v \<le> hi"
proof (induction a arbitrary: lo hi v \<nu> \<rho>)
  case (Num n)
  then show ?case by simp
next
  case (Var x)
  then show ?case by simp
next
  case Refuse
  then show ?case by simp
next
  case (Cast f a1)
  then show ?case by simp
next
  case (UnOp op a1)
  then show ?case by simp
next
  case (Cond t a1 b)
  then show ?case by simp
next
  case (Bin op a1 a2)
  have es: "env_sound \<rho> \<nu>" by (rule Bin.prems(1))
  show ?case
  proof (cases "floor_range a1 \<rho>")
    case None
    with Bin.prems show ?thesis by simp
  next
    case (Some p1)
    then obtain l0 l1 where lr1: "floor_range a1 \<rho> = Some (l0, l1)"
      by (metis option.inject prod.exhaust_sel)
    show ?thesis
    proof (cases "floor_range a2 \<rho>")
      case None
      with Bin.prems Some show ?thesis by simp
    next
      case (Some p2)
      then obtain r0 r1 where lr2: "floor_range a2 \<rho> = Some (r0, r1)"
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
          from floor_range_sound [OF es lr1 evl] have lb: "l0 \<le> v1 \<and> v1 \<le> l1" .
          from floor_range_sound [OF es lr2 evr] have rb: "r0 \<le> v2 \<and> v2 \<le> r1" .
          from lb have lb1: "l0 \<le> v1" and lb2: "v1 \<le> l1" by blast+
          from rb have rb1: "r0 \<le> v2" and rb2: "v2 \<le> r1" by blast+
          show ?thesis
          proof (cases "op = ''%''")
            case True
            show ?thesis
            proof (cases "l0 \<ge> 0 \<and> r0 > 0")
              case g: True
              from Bin.prems lr1 lr2 True g have vals: "lo = 0 \<and> hi = r1 - 1" by simp
              from Bin.prems(3) evl evr True have ev: "v = v1 mod v2" by (simp split: if_split_asm)
              from g have pos: "0 < r0" by blast
              from mod_pos_bounds [OF rb1 rb2 pos] have mb: "0 \<le> v mod v2 \<and> v mod v2 \<le> r1 - 1" by blast
              from ev have "v mod v2 = v" by simp
              with mb have "0 \<le> v \<and> v \<le> r1 - 1" by simp
              with vals show ?thesis by simp
            next
              case ng: False
              with Bin.prems lr1 lr2 True show ?thesis by simp
            qed
          next
            case False
            note ne1 = False
            show ?thesis
            proof (cases "op = ''/''")
              case True
              show ?thesis
              proof (cases "l0 \<ge> 0 \<and> r0 > 0")
                case g: True
                from Bin.prems lr1 lr2 True g
                have vals: "lo = min (l0 div r0) (min (l0 div r1) (min (l1 div r0) (l1 div r1))) \<and>
                            hi = max (l0 div r0) (max (l0 div r1) (max (l1 div r0) (l1 div r1)))" by simp
                from Bin.prems(3) evl evr True have ev: "v = v1 div v2" by (simp split: if_split_asm)
                from g have pos: "0 < r0" by blast
                from vals ev show ?thesis
                  by (metis div_corners [OF lb1 lb2 pos rb1 rb2])
              next
                case ng: False
                with Bin.prems lr1 lr2 True show ?thesis by simp
              qed
            next
              case False
              with Bin.prems lr1 lr2 ne1 show ?thesis by simp
            qed
          qed
        qed
      qed
    qed
  qed
qed

section \<open>Negative control: the pre-fix \ \ \<open>*\<close> arm, wrapped\<close>

text \<open>
  \ \ \<open>2 ^ 64\<close> is one \ \ \<open>i128\<close> legible value; its square,
  \ \ \<open>2 ^ 128\<close>, is one past it. The release build's raw multiply
  wraps the corner to \ \ \<open>0\<close> and the pass names \ \ \<open>[0, 0]\<close> for a
  value the Python reading calls \ \ \<open>2 ^ 128\<close>.
\<close>

lemma ex_unchecked_product_wraps:
  "v0_range (Bin ''*'' (Num (2 ^ 64)) (Num (2 ^ 64))) (\<lambda>_. None) = Some (0, 0)"
  "eval (\<lambda>_. 0) (Bin ''*'' (Num (2 ^ 64)) (Num (2 ^ 64))) = Some (2 ^ 128)"
  by (simp_all add: wrap128_def)

theorem v0_not_sound:
  "\<exists> a \<rho> \<nu> lo hi v. v0_range a \<rho> = Some (lo, hi) \<and> eval \<nu> a = Some v \<and> (v < lo \<or> hi < v)"
proof -
  let ?a = "Bin ''*'' (Num (2 ^ 64)) (Num (2 ^ 64))"
  have "v0_range ?a (\<lambda>_. None) = Some (0, 0)
        \<and> eval (\<lambda>_. 0) ?a = Some (2 ^ 128)
        \<and> ((0::int) < 2 ^ 128)"
    by (simp add: wrap128_def)
  then show ?thesis by blast
qed

text \<open>
  The fixed arm in the model: the exact corners, no wrap -- which is
  what \ \ \<open>checked_mul\<close> returns whenever it returns anything.
\<close>

lemma fix_is_exact_corners:
  "floor_range (Bin ''*'' (Num (2 ^ 64)) (Num (2 ^ 64))) (\<lambda>_. None) = Some (2 ^ 128, 2 ^ 128)"
  "eval (\<lambda>_. 0) (Bin ''*'' (Num (2 ^ 64)) (Num (2 ^ 64))) = Some (2 ^ 128)"
  by simp_all

end
