theory I32Provable
  imports Main
begin

text \<open>
  i32-provable — the Tier-1 obligation for the analysis-only transform
  sh2perl/src/transforms/i32_provable.rs, in the shape
  docs/transform-verification.md prescribes: verdict \<Longrightarrow> fact, over
  IR-shaped data only, no shell semantics.

  THE VERDICT as published: an Assign/Expr statement carrying an Arith
  node is marked I32 (keyed by fn-name + statement index), and a renderer
  trusts it — estree emits `| 0` where it would have emitted
  Math.trunc, C/Go/Rust/GLSL emit a native 32-bit type.  So the whole
  obligation is:

      carries s \<Longrightarrow> abs (stmt_value s) < 2 ^ 31

  THE FIRST PROOF ATTEMPT FAILED, and that is recorded rather than
  papered over.  Section 1 mirrors the estimator as it stood on
  2026-10-23 — every literal below 2^30, every op + - * — and section 2
  machine-checks the false positive that mirror has: three legal
  literals, only `*`, value 2^87, marked I32.  The manifest's invariant
  line claimed "the estimate is monotone, so a false POSITIVE is
  impossible by construction"; no estimate existed to be monotone.  A
  lemma cannot certify a false statement, so the estimator was fixed
  (arith_abs_bound composes a per-node bound and refuses as soon as it
  would leave i32) and sections 3-5 prove the theorem for the code that
  now exists.  Nothing was blessed: the counterexample stays in the file,
  next to the theorem it refuted.

  Honest bookkeeping about what is NOT claimed here: refusals only ever
  mark FEWER nodes, so the fix cannot turn a correct verdict into a wrong
  one (the manifest allows false negatives); and this is a statement
  about the compound's mathematical value in unbounded int — that the
  target's arithmetic does not wrap before reaching i32 is the separate
  wrapping obligation (C_UB_PROOF.md), not this file's.
\<close>

section \<open>1. The estimator as it stood (leaf-only) — mirrored verbatim\<close>

text \<open>The IR's arithmetic fragment, with every form the estimator
  refuses collapsed into \<open>Refuse\<close>: \<open>Var\<close>, \<open>Ident\<close>, \<open>Index\<close>,
  \<open>Assign\<close>, \<open>IncDec\<close>, \<open>Sizeof\<close>, \<open>Cast\<close> and every non-\<open>+ - *\<close>
  operator all return \<open>false\<close> in the Rust, so folding them together
  cannot make a theorem about those arms stronger than the code.
  (\<open>Un\<close> is spelled \<open>UnOp\<close> here only because Isabelle's inner syntax
  already uses \<open>Un\<close> for union.)\<close>

datatype arith =
    Num int
  | Bin string arith arith
  | UnOp string arith
  | Cond arith arith arith
  | Refuse

text \<open>The value of a compound, in unbounded int.  Bash arithmetic is
  truncating, but the estimator refuses every \<open>div\<close>/\<open>mod\<close>, so only
  \<open>+ - *\<close>, unary \<open>-\<close> and the ternary reach \<open>eval\<close> on a marked node;
  anything else is defined to 0 and is unreachable under the verdict.\<close>

fun eval :: "arith \<Rightarrow> int" where
  "eval (Num n) = n"
| "eval (Bin op l r) =
     (if op = ''+'' then eval l + eval r
      else if op = ''-'' then eval l - eval r
      else if op = ''*'' then eval l * eval r
      else 0)"
| "eval (UnOp op a) = (if op = ''-'' then - eval a else 0)"
| "eval (Cond t a b) = (if eval t \<noteq> 0 then eval a else eval b)"
| "eval Refuse = 0"

text \<open>\<open>is_i32_v0\<close> is arith_is_i32 as it was written before the fix:
  leaf bounds only, no composition across nodes.  Kept because the
  counterexample in section 2 is about THIS predicate, and because the
  diff between it and the current one is exactly the theorem's content.\<close>

fun is_i32_v0 :: "arith \<Rightarrow> bool" where
  "is_i32_v0 (Num n) = (abs n < 2 ^ 30)"
| "is_i32_v0 (Bin op l r) =
     ((op = ''+'' \<or> op = ''-'' \<or> op = ''*'') \<and> is_i32_v0 l \<and> is_i32_v0 r)"
| "is_i32_v0 (UnOp op a) = ((op = ''-'') \<and> is_i32_v0 a)"
| "is_i32_v0 (Cond t a b) = (is_i32_v0 t \<and> is_i32_v0 a \<and> is_i32_v0 b)"
| "is_i32_v0 Refuse = False"

section \<open>2. The negative control: what that predicate cannot prove\<close>

text \<open>The compound that the leaf-only check accepted: three literals,
  each below 2^30, only \<open>*\<close>, value 2^87.  In C/Go/Rust/GLSL a renderer
  trusting this verdict declares a 32-bit type; in estree it turns
  Math.trunc into `| 0`, which is ToInt32 and therefore NOT the same
  value for 2^87.\<close>

definition boom :: arith where
  "boom = Bin ''*'' (Bin ''*'' (Num (2 ^ 29)) (Num (2 ^ 29))) (Num (2 ^ 29))"

lemma boom_marked_v0: "is_i32_v0 boom"
  by (simp add: boom_def)

lemma boom_value_v0: "eval boom = 2 ^ 87"
  by (simp add: boom_def)

lemma boom_overflows_v0: "\<not> (abs (eval boom) < 2 ^ 31)"
  by (simp add: boom_def)

text \<open>The manifest's invariant, as a statement, machine-checked false for
  the old estimator.  This is the lemma that makes the session
  non-vacuous in the other direction: it fails to build if anyone
  restores the leaf-only check.\<close>

lemma v0_has_false_positive:
  "\<not> (\<forall>e. is_i32_v0 e \<longrightarrow> abs (eval e) < 2 ^ 31)"
proof
  assume *: "\<forall>e. is_i32_v0 e \<longrightarrow> abs (eval e) < 2 ^ 31"
  from * [THEN spec, of boom] boom_marked_v0 boom_overflows_v0 show False by blast
qed

section \<open>3. The estimator as it now stands: a composed per-node bound\<close>

text \<open>\<open>cap\<close> is the i32 wall: a bound that would reach 2^31 is a
  refusal, not a bound.  Mirrors the two constants in arith_abs_bound.\<close>

definition cap :: "int \<Rightarrow> int option" where
  "cap b = (if b < 2 ^ 31 then Some b else None)"

text \<open>\<open>abs_bound e = Some b\<close> means: |value of e| is at most b, and b is
  an i32-representable magnitude.  Every node composes its children's
  bounds — \<open>+\<close>/\<open>-\<close> add magnitudes (|a - b| \<le> |a| + |b|), \<open>*\<close>
  multiplies them — and refuses as soon as the composed value would
  leave i32.  The literal rule is unchanged (|n| \<lt> 2^30); what changed is
  that the leaves' answer is now checked against the wall at every node
  on the way up, which is what the first version never did.\<close>

fun abs_bound :: "arith \<Rightarrow> int option" where
  "abs_bound (Num n) = (if abs n < 2 ^ 30 then Some (abs n) else None)"
| "abs_bound (Bin op l r) =
     (case abs_bound l of
        None \<Rightarrow> None
      | Some bl \<Rightarrow> case abs_bound r of
          None \<Rightarrow> None
        | Some br \<Rightarrow>
            (if op = ''+'' then cap (bl + br)
             else if op = ''-'' then cap (bl + br)
             else if op = ''*'' then cap (bl * br)
             else None))"
| "abs_bound (UnOp op a) = (if op = ''-'' then abs_bound a else None)"
| "abs_bound (Cond t a b) =
     (case abs_bound t of
        None \<Rightarrow> None
      | Some bt \<Rightarrow> case abs_bound a of
          None \<Rightarrow> None
        | Some ba \<Rightarrow> case abs_bound b of
            None \<Rightarrow> None
          | Some bb \<Rightarrow> Some (max ba bb))"
| "abs_bound Refuse = None"

text \<open>\<open>is_i32\<close> is the Rust's \<open>arith_abs_bound(a).is_some()\<close>.  The
  \<open>Cond\<close> arm mirrors the Rust exactly: the test's bound is established
  and then DISCARDED — the value of the ternary is one of its arms, never
  an arithmetic combination with the test — so only the two arms' max
  bounds the result.\<close>

definition is_i32 :: "arith \<Rightarrow> bool" where
  "is_i32 e \<equiv> abs_bound e \<noteq> None"

section \<open>4. The theorem the transform actually owes\<close>

lemma lit_below:
  assumes "abs n < 2 ^ 30"
  shows "abs n < (2 ^ 31 :: int)"
proof -
  have "(2 ^ 30 :: int) < 2 ^ 31" by simp
  with assms show ?thesis by (rule order_less_trans)
qed

text \<open>Two helpers the branches lean on, proved first.  The cap inverts
  exactly: a bound that cleared 2 ^ 31 came back unchanged, and one
  that did not came back as a refusal — the wall stands between the
  two, so the branches can turn `cap (x) = Some b` back into an
  equation about `x` plus the fact that `x` clears the wall.  The
  closing steps then combine the child invariants (conjunctions, whose
  parts are extracted with blast rather than assumed) by ordered
  arithmetic.  Every branch derives its equations — the children's
  bounds, the cap's verdict, the operator's identity — as named facts
  before the closing step: what bounds `b` is those equations, not the
  bare premise.\<close>

lemma cap_Some:
  assumes "cap c = Some b"
  shows "b = c \<and> c < 2 ^ 31"
proof (cases "c < 2 ^ 31")
  case True
  note cond = True
  have un: "cap c = Some c"
    unfolding cap_def by (rule if_P [OF cond])
  from un assms cond show ?thesis by auto
next
  case False
  note cond = False
  have un: "cap c = None"
    unfolding cap_def by (rule if_not_P [OF cond])
  from un assms show ?thesis by simp
qed

lemma abs_bound_sound:
  "abs_bound e = Some b \<Longrightarrow> 0 \<le> b \<and> b < 2 ^ 31 \<and> abs (eval e) \<le> b"
proof (induction e arbitrary: b)
  case (Num n)
  show ?case
  proof (cases "abs n < 2 ^ 30")
    case True
    note cond = True
    have ab: "abs_bound (Num n) = Some (abs n)"
      unfolding abs_bound.simps by (rule if_P [OF cond])
    from ab Num.prems have inj: "Some (abs n) = Some b" by simp
    from inj have eq: "b = abs n" by simp
    show ?thesis using eq lit_below [OF cond] by auto
  next
    case False
    note cond = False
    have ab: "abs_bound (Num n) = None"
      unfolding abs_bound.simps by (rule if_not_P [OF cond])
    from ab Num.prems show ?thesis by simp
  qed
next
  case (Bin op l r)
  note prems = Bin.prems
  show ?case
  proof (cases "abs_bound l")
    case None
    note eq_l = None
    from eq_l prems show ?thesis by simp
  next
    case (Some bl)
    note eq_l = Some
    show ?thesis
    proof (cases "abs_bound r")
      case None
      note eq_r = None
      from eq_l eq_r prems show ?thesis by simp
    next
      case (Some br)
      note eq_r = Some
      note ih_l = Bin.IH(1) [OF eq_l]
      note ih_r = Bin.IH(2) [OF eq_r]
      from ih_l have ub_l: "abs (eval l) \<le> bl" by blast
      from ih_l have nz_bl: "0 \<le> bl" by blast
      from ih_r have ub_r: "abs (eval r) \<le> br" by blast
      from ih_r have nz_br: "0 \<le> br" by blast
      show ?thesis
      proof (cases "op = ''+''")
        case True
        note op_eq = True
        have cap_eq: "cap (bl + br) = Some b"
          using eq_l eq_r op_eq prems by simp
        from cap_Some [OF cap_eq] have b_eq: "b = bl + br" by blast
        from cap_Some [OF cap_eq] have wall: "bl + br < 2 ^ 31" by blast
        have nz: "0 \<le> bl + br"
          by (rule add_nonneg_nonneg [OF nz_bl nz_br])
        have le1: "abs (eval l + eval r) \<le> abs (eval l) + abs (eval r)"
          by (rule abs_triangle_ineq)
        have le2: "abs (eval l) + abs (eval r) \<le> bl + br"
          by (rule add_mono [OF ub_l ub_r])
        have le: "abs (eval l + eval r) \<le> bl + br"
          by (rule order_trans [OF le1 le2])
        have ev: "eval (Bin op l r) = eval l + eval r"
          unfolding eval.simps by (rule if_P [OF op_eq])
        show ?thesis using b_eq nz wall le ev by auto
      next
        case False
        note not_plus = False
        show ?thesis
        proof (cases "op = ''-''")
          case True
          note op_eq = True
          have cap_eq: "cap (bl + br) = Some b"
            using eq_l eq_r not_plus op_eq prems by simp
          from cap_Some [OF cap_eq] have b_eq: "b = bl + br" by blast
          from cap_Some [OF cap_eq] have wall: "bl + br < 2 ^ 31" by blast
          have nz: "0 \<le> bl + br"
            by (rule add_nonneg_nonneg [OF nz_bl nz_br])
          have le1: "abs (eval l - eval r) \<le> abs (eval l) + abs (eval r)"
            by (rule abs_triangle_ineq4)
          have le2: "abs (eval l) + abs (eval r) \<le> bl + br"
            by (rule add_mono [OF ub_l ub_r])
          have le: "abs (eval l - eval r) \<le> bl + br"
            by (rule order_trans [OF le1 le2])
          have ev: "eval (Bin op l r) = eval l - eval r"
            using not_plus op_eq by simp
          show ?thesis using b_eq nz wall le ev by auto
        next
          case False
          note not_minus = False
          show ?thesis
          proof (cases "op = ''*''")
            case True
            note op_eq = True
            have cap_eq: "cap (bl * br) = Some b"
              using eq_l eq_r not_plus not_minus op_eq prems by simp
            from cap_Some [OF cap_eq] have b_eq: "b = bl * br" by blast
            from cap_Some [OF cap_eq] have wall: "bl * br < 2 ^ 31" by blast
            have nz: "0 \<le> bl * br"
              by (rule mult_nonneg_nonneg [OF nz_bl nz_br])
            have absr: "0 \<le> abs (eval r)" by simp
            have e1: "abs (eval l * eval r) = abs (eval l) * abs (eval r)"
              by (rule abs_mult)
            have e2: "abs (eval l) * abs (eval r) \<le> bl * br"
              by (rule mult_mono [OF ub_l ub_r nz_bl absr])
            have le: "abs (eval l * eval r) \<le> bl * br"
              using e1 e2 by linarith
            have ev: "eval (Bin op l r) = eval l * eval r"
              using not_plus not_minus op_eq by simp
            show ?thesis using b_eq nz wall le ev by auto
          next
            case False
            note not_times = False
            have un: "abs_bound (Bin op l r) = None"
              using eq_l eq_r not_plus not_minus not_times by simp
            from un prems show ?thesis by simp
          qed
        qed
      qed
    qed
  qed
next
  case (UnOp op a)
  note prems = UnOp.prems
  show ?case
  proof (cases "op = ''-''")
    case True
    note op_eq = True
    have ab: "abs_bound a = Some b"
      using op_eq prems by simp
    note ih = UnOp.IH [OF ab]
    from ih have nz: "0 \<le> b" by blast
    from ih have wb: "b < 2 ^ 31" by blast
    from ih have ub: "abs (eval a) \<le> b" by blast
    have ev: "eval (UnOp op a) = - eval a"
      unfolding eval.simps by (rule if_P [OF op_eq])
    show ?thesis using nz wb ub ev by auto
  next
    case False
    note op_ne = False
    have un: "abs_bound (UnOp op a) = None"
      using op_ne by simp
    from un prems show ?thesis by simp
  qed
next
  case (Cond t x y)
  note prems = Cond.prems
  show ?case
  proof (cases "abs_bound t")
    case None
    note eq_t = None
    from eq_t prems show ?thesis by simp
  next
    case (Some bt)
    note eq_t = Some
    show ?thesis
    proof (cases "abs_bound x")
      case None
      note eq_x = None
      from eq_t eq_x prems show ?thesis by simp
    next
      case (Some bx)
      note eq_x = Some
      show ?thesis
      proof (cases "abs_bound y")
        case None
        note eq_y = None
        from eq_t eq_x eq_y prems show ?thesis by simp
      next
        case (Some bv)
        note eq_y = Some
        have inj: "Some (max bx bv) = Some b"
          using eq_t eq_x eq_y prems by simp
        from inj have b_eq: "b = max bx bv" by simp
        note ih_x = Cond.IH(2) [OF eq_x]
        note ih_y = Cond.IH(3) [OF eq_y]
        from ih_x have ub_x: "abs (eval x) \<le> bx" by blast
        from ih_x have nz_x: "0 \<le> bx" by blast
        from ih_x have hi_x: "bx < 2 ^ 31" by blast
        from ih_y have ub_y: "abs (eval y) \<le> bv" by blast
        from ih_y have nz_y: "0 \<le> bv" by blast
        from ih_y have hi_y: "bv < 2 ^ 31" by blast
        have le1: "bx \<le> max bx bv" by (rule max.cobounded1)
        have le2: "bv \<le> max bx bv" by (rule max.cobounded2)
        have nz: "0 \<le> max bx bv"
          by (rule order_trans [OF nz_x le1])
        have wall: "max bx bv < 2 ^ 31"
          using hi_x hi_y by auto
        show ?thesis
        proof (cases "eval t \<noteq> 0")
          case True
          note z = True
          have ev: "eval (Cond t x y) = eval x"
            unfolding eval.simps by (rule if_P [OF z])
          have le: "abs (eval x) \<le> max bx bv"
            by (rule order_trans [OF ub_x le1])
          show ?thesis using b_eq nz wall le ev by auto
        next
          case False
          note z = False
          have ev: "eval (Cond t x y) = eval y"
            unfolding eval.simps by (rule if_not_P [OF z])
          have le: "abs (eval y) \<le> max bx bv"
            by (rule order_trans [OF ub_y le2])
          show ?thesis using b_eq nz wall le ev by auto
        qed
      qed
    qed
  qed
next
  case Refuse
  from Refuse.prems show ?case by simp
qed

section \<open>5. The verdict, as the transform publishes it\<close>

text \<open>The statement layer: \<open>stmt_carries_i32\<close> marks an Assign or Expr
  statement whose expression is an Arith node the estimator accepts, and
  that verdict — keyed (fn_name, stmt index) — is what a renderer reads.\<close>

datatype stmt = SAssign arith | SExpr arith | SOther

fun carries :: "stmt \<Rightarrow> bool" where
  "carries (SAssign e) = is_i32 e"
| "carries (SExpr e) = is_i32 e"
| "carries SOther = False"

fun stmt_value :: "stmt \<Rightarrow> int" where
  "stmt_value (SAssign e) = eval e"
| "stmt_value (SExpr e) = eval e"
| "stmt_value SOther = 0"

text \<open>Pulling the witness out of the verdict, once: \<open>is_i32 e\<close> is
  `abs_bound e = None` being false, and option has exactly two
  constructors.  The statement layer's theorems use this rather than
  repeating the case analysis.\<close>

lemma is_i32_ex:
  assumes "is_i32 e"
  shows "\<exists>b. abs_bound e = Some b"
proof -
  from assms have nb: "abs_bound e \<noteq> None" by (simp add: is_i32_def)
  show ?thesis
  proof (cases "abs_bound e")
    case None with nb show ?thesis by simp
  next
    case (Some v) then show ?thesis by blast
  qed
qed

theorem i32_provable_sound:
  assumes cs: "carries s"
  shows "abs (stmt_value s) < 2 ^ 31"
proof (cases s)
  case (SAssign e)
  note seq = SAssign
  from cs seq have ii: "is_i32 e" by simp
  from ii have ex: "\<exists>b. abs_bound e = Some b" by (rule is_i32_ex)
  from ex obtain b where b: "abs_bound e = Some b" by blast
  from abs_bound_sound [OF b] have ub: "abs (eval e) \<le> b" by blast
  from abs_bound_sound [OF b] have wb: "b < 2 ^ 31" by blast
  have step: "abs (stmt_value (SAssign e)) < 2 ^ 31"
    using ub wb unfolding stmt_value.simps by linarith
  moreover have rewr: "stmt_value s = stmt_value (SAssign e)"
    using seq by simp
  ultimately show ?thesis by auto
next
  case (SExpr e)
  note seq = SExpr
  from cs seq have ii: "is_i32 e" by simp
  from ii have ex: "\<exists>b. abs_bound e = Some b" by (rule is_i32_ex)
  from ex obtain b where b: "abs_bound e = Some b" by blast
  from abs_bound_sound [OF b] have ub: "abs (eval e) \<le> b" by blast
  from abs_bound_sound [OF b] have wb: "b < 2 ^ 31" by blast
  have step: "abs (stmt_value (SExpr e)) < 2 ^ 31"
    using ub wb unfolding stmt_value.simps by linarith
  moreover have rewr: "stmt_value s = stmt_value (SExpr e)"
    using seq by simp
  ultimately show ?thesis by auto
next
  case SOther
  note seq = SOther
  have step: "abs (stmt_value SOther) < 2 ^ 31" by simp
  have rewr: "stmt_value s = stmt_value SOther"
    using seq by simp
  from step rewr show ?thesis by auto
qed

text \<open>And the node-level form, for a renderer that reads the estimator
  directly rather than the statement verdict.\<close>

theorem i32_provable_node_sound:
  assumes "is_i32 e"
  shows "abs (eval e) < 2 ^ 31"
proof -
  from assms have ex: "\<exists>b. abs_bound e = Some b" by (rule is_i32_ex)
  from ex obtain b where b: "abs_bound e = Some b" by blast
  from abs_bound_sound [OF b] have ub: "abs (eval e) \<le> b" by blast
  from abs_bound_sound [OF b] have wb: "b < 2 ^ 31" by blast
  show ?thesis using ub wb by linarith
qed

end
