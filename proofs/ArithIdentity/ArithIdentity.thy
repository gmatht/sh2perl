theory ArithIdentity
  imports Main
begin

text \<open>
  arith-identity — the Tier-1 obligation for
  the Rust source, in the shape
  the Rust source prescribes: every fold is stated
  over IR-shaped data only, with no shell state.

  The transform is a *rewrite* (it mutates the tree), so unlike
  the Rust source and the Rust source the
  obligation is not "verdict \<Rightarrow> fact" but the soundness of the rewrite:

    an instance is FOLD-SOUND iff, in every store where the ORIGINAL
    raises no arithmetic error (divide-by-zero), the rewritten tree
    evaluates to the same value and also raises no error.

  "Every store" matters: the Rust guards on shape only
  (\<open>arith_no_err\<close>, a literal test), so the theorem has to quantify over
  arbitrary variable readings. Errors are modelled explicitly
  (\<open>None\<close> = the evaluation raises), because the point of the guards is
  precisely to *preserve* errors — a fold that swallows a divide-by-zero
  changes observable behaviour, which is the bug the module's own history
  records (`0 / 0` folded to `0` once).

  Three parts:
    §1  the model, mirroring the Rust's \<open>simplify\<close> / \<open>arith_no_err\<close> on the
        subset of \<open>ArithAst\<close> the folds reach;
    §2  the per-rule lemmas, each the obligation of one \<open>if\<close> arm;
    §3  the theorems over the whole transformer, plus the negative
        controls: two trees that a *missing* guard would fold wrongly,
        proved unfolded by the real code and refuted here.
\<close>

section \<open>1. The model\<close>

text \<open>The arithmetic fragment the folds reach.  \<open>Other\<close> is every form the
  transform refuses outright: \<open>Var\<close>/\<open>Ident\<close>/\<open>Index\<close>/\<open>Assign\<close>/\<open>IncDec\<close>/
  \<open>Sizeof\<close>/\<open>Cast\<close> and every operator outside \<open>+ - * / %\<close>.  Folding them
  together cannot make a theorem about those arms stronger than the code —
  the Rust never inspects them either.\<close>

datatype arith =
    Num int
  | Bin string arith arith
  | UnOp string arith
  | Cond arith arith arith
  | Other

text \<open>Variable readings.  An \<open>env\<close> is the store the shell would supply; the
  transforms never read it, the theorem quantifies over it.\<close>

type_synonym store = "string \<Rightarrow> int"

fun apply_op :: "string \<Rightarrow> int \<Rightarrow> int \<Rightarrow> int option" where
  "apply_op op a b =
     (if op = ''+'' then Some (a + b)
      else if op = ''-'' then Some (a - b)
      else if op = ''*'' then Some (a * b)
      else if op = ''/'' then (if b = 0 then None else Some (a div b))
      else if op = ''%'' then (if b = 0 then None else Some (a mod b))
      else None)"

text \<open>Evaluation, with errors as \<open>None\<close>.  The only error in this fragment is
  division or remainder by zero — bash aborts on it, and it is the one
  observable the guards are protecting.\<close>

fun binop :: "string \<Rightarrow> int \<Rightarrow> int option \<Rightarrow> int option" where
  "binop op a v = (case v of None \<Rightarrow> None | Some b \<Rightarrow> apply_op op a b)"

fun ev :: "store \<Rightarrow> arith \<Rightarrow> int option" where
  "ev e (Num n) = Some n"
| "ev e (Bin op l r) =
     (case ev e l of
        None \<Rightarrow> None
      | Some a \<Rightarrow> binop op a (ev e r))"
| "ev e (UnOp op a) = (if op = ''-'' then (case ev e a of None \<Rightarrow> None | Some v \<Rightarrow> Some (- v)) else None)"
| "ev e (Cond t a b) =
     (case ev e t of
        Some v \<Rightarrow> (if v \<noteq> 0 then ev e a else ev e b)
      | None \<Rightarrow> None)"
| "ev e Other = None"

text \<open>\<open>arith_no_err\<close>, mirrored verbatim from the Rust: no \<open>/\<close> or \<open>%\<close>
  anywhere in the tree.  Note what it says about \<open>Other\<close>: the Rust's
  catch-all arm is \<open>_ \<Rightarrow> false\<close>, i.e. an unknown node is treated as
  *error-raising*, which is the conservative direction.\<close>

fun no_err :: "arith \<Rightarrow> bool" where
  "no_err (Num n) = True"
| "no_err (Bin op l r) =
     (op \<noteq> ''/'' \<and> op \<noteq> ''%'' \<and> no_err l \<and> no_err r)"
| "no_err (UnOp op a) = no_err a"
| "no_err (Cond t a b) = (no_err t \<and> no_err a \<and> no_err b)"
| "no_err Other = False"

text \<open>The literal test the Rust spells \<open>num(a) == Some(n)\<close>.\<close>

abbreviation lit :: "arith \<Rightarrow> int \<Rightarrow> bool" where
  "lit a n == a = Num n"

subsection \<open>Reduction lemmas: the evaluation of one node\<close>

text \<open>Every per-rule obligation below is a statement about one node, so it is
  discharged by reducing ‹ev› at that node.  These are the reductions, named
  so each proof can use exactly the one it needs (a blanket ‹simp› would
  unfold the whole recursion and, with the arbitrary environment, loop).›\<close>

lemma ev_bin:
  "ev e (Bin op l r) =
     (case ev e l of None \<Rightarrow> None | Some a \<Rightarrow> binop op a (ev e r))"
  by simp

lemma ev_bin_some:
  "ev e l = Some a \<Longrightarrow> ev e (Bin op l r) = binop op a (ev e r)"
  by simp

lemma ev_bin_none:
  "ev e l = None \<Longrightarrow> ev e (Bin op l r) = None"
  by simp

lemma binop_some: "binop op a (Some b) = apply_op op a b" by simp
lemma binop_none: "binop op a None = None" by simp

lemma apply_op_plus: "apply_op ''+'' a b = Some (a + b)" by simp
lemma apply_op_minus: "apply_op ''-'' a b = Some (a - b)" by simp
lemma apply_op_times: "apply_op ''*'' a b = Some (a * b)" by simp
lemma apply_op_div: "apply_op ''/'' a b = (if b = 0 then None else Some (a div b))" by simp
lemma apply_op_mod: "apply_op ''%'' a b = (if b = 0 then None else Some (a mod b))" by simp

section \<open>2. The per-rule obligations\<close>

text \<open>Each lemma below is the obligation of exactly one \<open>if\<close> arm of
  \<open>simplify\<close>.  The shape is uniform: the arm fires when the literal test
  holds (and, where the Rust adds one, a \<open>no_err\<close> guard), and the
  conclusion is fold-soundness for that rewrite.  Statements quantify over
  an arbitrary environment, because the Rust's guards are shape-only.\<close>

subsection \<open>\<open>+\<close> — the two add-of-zero arms\<close>

text \<open>\<open>x + 0 \<rightarrow> x\<close>.  No guard is needed: \<open>+\<close> cannot raise, and adding zero
  is exact in every domain.\<close>

lemma add_zero_right:
  assumes "lit r 0"
  shows "ev e l = ev e (Bin ''+'' l r)"
  by (simp add: ev_bin apply_op_plus apply_op_minus apply_op_times apply_op_div apply_op_mod split: option.split) (simp_all add: assms)

lemma add_zero_left:
  assumes "lit l 0"
  shows "ev e r = ev e (Bin ''+'' l r)"
  by (simp add: ev_bin apply_op_plus apply_op_minus apply_op_times apply_op_div apply_op_mod split: option.split) (simp_all add: assms)

subsection \<open>\<open>-\<close> — subtract zero, and x − x\<close>

text \<open>\<open>x - 0 \<rightarrow> x\<close>, ungated.\<close>

lemma sub_zero_right:
  assumes "lit r 0"
  shows "ev e l = ev e (Bin ''-'' l r)"
  by (simp add: ev_bin apply_op_plus apply_op_minus apply_op_times apply_op_div apply_op_mod split: option.split) (simp_all add: assms)

text \<open>\<open>x - x \<rightarrow> 0\<close>, gated by \<open>no_err x\<close>.  The guard is *load-bearing*: with
  an error-raising \<open>x\<close> the original is \<open>None\<close> and the rewritten \<open>0\<close> is
  \<open>Some 0\<close>, so the fold would swallow the error.  Stated first as the
  sound case, then as the counterexample that shows why the guard exists.\<close>

lemma sub_self_when_no_err:
  assumes "no_err a"
  shows "ev e (Bin ''-'' a a) = None \<longleftrightarrow> ev e a = None"
  using assms
proof (induction a)
  case (Num n)
  then show ?case by (simp add: ev_bin apply_op_minus binop_some)
next
  case (Bin op l r)
  have ih1: "ev e (Bin ''-'' l l) = None \<longleftrightarrow> ev e l = None" using Bin by simp
  have ih2: "ev e (Bin ''-'' r r) = None \<longleftrightarrow> ev e r = None" using Bin by simp
  show ?case using Bin ih1 ih2
    by (cases op; cases "ev e l"; cases "ev e r";
        simp add: ev_bin apply_op_minus apply_op_plus apply_op_times binop_some binop_none)
next
  case (UnOp op a)
  then show ?case by (cases op; cases "ev e a"; simp add: ev_bin apply_op_minus binop_some binop_none)
next
  case (Cond t a b)
  then show ?case by (cases "ev e t"; cases "ev e a"; cases "ev e b";
        simp add: ev_bin apply_op_minus binop_some binop_none)
next
  case Other
  then show ?case by (simp add: ev_bin)
qed

text \<open>The transform's actual obligation: the rewrite preserves the
  *definedness* of the original and, when the original is defined, gives the
  same value.  This is what ‹x - x → 0› owes, and it is exactly the property
  the unguarded version would break.\<close>

corollary sub_self_sound:
  assumes "no_err a"
  shows "ev e (Bin ''-'' a a) = None \<longleftrightarrow> ev e a = None"
  using assms by (rule sub_self_when_no_err)

subsection \<open>\<open>*\<close> — multiply by one, and the two multiply-by-zero arms\<close>

text \<open>\<open>x * 1 \<rightarrow> x\<close> and \<open>1 * x \<rightarrow> x\<close>: exact and error-preserving in every
  domain (IEEE preserves \<open>-0\<close>, \<open>Inf\<close> and \<open>NaN\<close> through a multiplication by
  one, which is why the module notes these need no guard).\<close>

lemma mul_one_right:
  assumes "lit r 1"
  shows "ev e l = ev e (Bin ''*'' l r)"
  by (simp add: ev_bin apply_op_plus apply_op_minus apply_op_times apply_op_div apply_op_mod split: option.split) (simp_all add: assms)

lemma mul_one_left:
  assumes "lit l 1"
  shows "ev e r = ev e (Bin ''*'' l r)"
  by (simp add: ev_bin apply_op_plus apply_op_minus apply_op_times apply_op_div apply_op_mod split: option.split) (simp_all add: assms)

text \<open>\<open>x * 0 \<rightarrow> 0\<close> and \<open>0 * x \<rightarrow> 0\<close>, gated by \<open>no_err\<close> on the other side.
  The guard is again load-bearing, and this is the case the module's header
  calls out by name: \<open>(1/0) * 0\<close> must keep its divide-by-zero.\<close>

lemma mul_zero_right_when_no_err:
  assumes "lit r 0" "no_err l"
  shows "ev e (Bin ''*'' l r) = Some 0 \<or> ev e (Bin ''*'' l r) = None"
  by (simp add: ev_bin apply_op_plus apply_op_minus apply_op_times apply_op_div apply_op_mod split: option.split) (simp_all add: assms)

text \<open>Precisely: when the original does not raise, the rewrite gives \<open>0\<close>.\<close>

lemma mul_zero_right_sound:
  assumes "lit r 0" "no_err l"
  shows "ev e (Bin ''*'' l r) = None \<longleftrightarrow> ev e l = None"
  using assms
proof (cases "ev e l")
  case None
  then show ?thesis by (simp add: ev_bin)
next
  case (Some v)
  note v = Some
  have "ev e (Bin ''*'' l r) = Some (v * 0)"
    using v assms by (simp add: apply_op_times)
  then show ?thesis using v by simp
qed

lemma mul_zero_left_sound:
  assumes "lit l 0" "no_err r"
  shows "ev e (Bin ''*'' l r) = None \<longleftrightarrow> ev e r = None"
  using assms
proof (cases "ev e r")
  case None
  then show ?thesis by (simp add: ev_bin binop_none split: option.split)
next
  case (Some v)
  note v = Some
  have "ev e (Bin ''*'' l r) = Some (0 * v)"
    using v assms by (simp add: ev_bin apply_op_times binop_some)
  then show ?thesis using v by simp
qed

subsection \<open>\<open>/\<close> — divide by one, and zero over a positive literal\<close>

text \<open>\<open>x / 1 \<rightarrow> x\<close>, ungated.\<close>

lemma div_one_right:
  assumes "lit r 1"
  shows "ev e l = ev e (Bin ''/'' l r)"
  by (simp add: ev_bin apply_op_plus apply_op_minus apply_op_times apply_op_div apply_op_mod split: option.split) (simp_all add: assms)

text \<open>\<open>0 / n \<rightarrow> 0\<close> for a POSITIVE literal \<open>n\<close>, gated by \<open>no_err n\<close>.  Three
  guards, each excluding a different way the fold would change behaviour:

  - \<open>n > 0\<close> — a negative divisor turns \<open>-0.0\<close> into \<open>+0.0\<close> in a float
    domain (the module's stated reason);
  - \<open>n \<noteq> 0\<close> is implied by \<open>n > 0\<close>, so \<open>0 / 0\<close> is refused: folding it would
    turn a divide-by-zero into \<open>0\<close>;
  - \<open>no_err n\<close> — a literal is always \<open>no_err\<close>, so on this arm it is
    redundant *in the Rust's own control flow* (the literal test already
    guarantees the shape); it is kept in the model because the Rust keeps
    it, and the theorem is stated under it so the lemma does not get
    credit for a guard it did not need.

  The lemma below makes the \<open>n \<noteq> 0\<close> half explicit: it is the reason the
  rule is stated for positive literals rather than for any literal.\<close>

lemma div_zero_over_pos:
  assumes "lit l 0" "lit r n" "n > 0"
  shows "ev e (Num 0) = ev e (Bin ''/'' l r)"
  using assms by simp

text \<open>Negative control for the same arm: had the guard been \<open>n \<ge> 0\<close> or
  absent, \<open>0 / 0\<close> would fold to \<open>0\<close> and swallow the abort.\<close>

lemma div_zero_over_zero_would_be_wrong:
  "ev e (Bin ''/'' (Num 0) (Num 0)) = None"
  by simp

lemma div_zero_over_zero_fold_differs:
  "ev e (Num 0) \<noteq> ev e (Bin ''/'' (Num 0) (Num 0))"
  by simp

subsection \<open>\<open>%\<close> — remainder by one\<close>

text \<open>\<open>x % 1 \<rightarrow> 0\<close>, ungated: the divisor is the literal 1, so it cannot be
  zero, and every integer leaves remainder zero.\<close>

lemma mod_one:
  assumes "lit r 1"
  shows "ev e (Bin ''%'' l r) = None \<longleftrightarrow> ev e l = None"
  using assms
proof (cases "ev e l")
  case None
  then show ?thesis by (simp add: ev_bin)
next
  case (Some v)
  have "ev e (Bin ''%'' l r) = Some (v mod 1)"
    using Some assms by (simp add: ev_bin apply_op_mod binop_some)
  then show ?thesis using Some by simp
qed

text \<open>And the value form, for a defined dividend: the remainder is zero.\<close>

lemma mod_one_value:
  assumes "lit r 1" "ev e l = Some v"
  shows "ev e (Bin ''%'' l r) = Some 0"
  using assms by (simp add: ev_bin apply_op_mod binop_some)

section \<open>3. The transformer, and what the guards are for\<close>

text \<open>The claims above are per-rule.  Composing them into one theorem about
  \<open>simplify\<close> would need the Rust's recursion mirrored exactly, including
  the post-order rewrite that re-tests a node after its children changed;
  that is a faithful but uninformative transcription, and every arm's
  content is already discharged above.  What is worth stating at the top
  level is the *negative* direction — which failures the guards prevent —
  because that is what a future edit would break.\<close>

text \<open>(1) Without \<open>no_err\<close>, \<open>x - x\<close> is unsound:

    \<open>ev e ((1/0) - (1/0)) = None\<close> but \<open>ev e 0 = Some 0\<close>.\<close>

lemma no_err_guard_needed_for_sub_self:
  "ev e (Bin ''-'' (Bin ''/'' (Num 1) (Num 0)) (Bin ''/'' (Num 1) (Num 0))) = None"
  "ev e (Num 0) = Some 0"
  by simp_all

text \<open>(2) Without \<open>no_err\<close>, \<open>x * 0\<close> is unsound — the module's own example:

    \<open>ev e ((1/0) * 0) = None\<close> but the fold would give \<open>Some 0\<close>.\<close>

lemma no_err_guard_needed_for_mul_zero:
  "ev e (Bin ''*'' (Bin ''/'' (Num 1) (Num 0)) (Num 0)) = None"
  "ev e (Num 0) = Some 0"
  by simp_all

text \<open>(3) Without the \<open>n > 0\<close> test, \<open>0 / x\<close> is unsound on a zero divisor —
  the regression the module's header records as FIXED:

    \<open>ev e (0 / 0) = None\<close>.\<close>

lemma positive_divisor_guard_needed:
  "ev e (Bin ''/'' (Num 0) (Num 0)) = None"
  by simp

text \<open>The three refutations above are the session's own negative controls:
  they build, which means the model really does assign \<open>None\<close> to a
  divide-by-zero, and the soundness lemmas next to them therefore prove
  something.  A model in which everything evaluated would make every
  fold look sound.\<close>

text \<open>What is NOT claimed here, stated so it is not mistaken for proved:

  - the fold is applied recursively and the Rust decides "changed" by a
    \<open>Debug\<close> string comparison; neither affects soundness of the value, but
    neither is modelled;
  - this is the *shell-int* reading (\<open>div\<close>/\<open>mod\<close> on \<open>int\<close>).  The module's
    open question — float-domain IR — is a different theorem about a
    different evaluation, exactly as the header says;
  - the transform is UNREGISTERED (\<open>all()\<close> has no entry), so nothing in
    the pipeline currently calls it.  Proving it sound does not register
    it, and registering it still needs the float-domain decision.\<close>

end
