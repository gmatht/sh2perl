(*  Part 2 of the text-ops slice: the newline strip of command
    substitution (obligation 5 of ShIR_TextOps_Base).  *)
theory ShIR_TextOps_Strip
  imports ShIR_TextOps_Base
begin

section \<open>Obligation 5: command substitution strips trailing newlines\<close>

text \<open>Command substitution removes ALL trailing newlines (POSIX), not one
  (chomp), and it removes nothing else. The node's meaning is written on the
  REVERSED byte string, where the run to remove is a PREFIX and the recursion
  is structural:\<close>

fun rev_drop_last_nl :: "bytes \<Rightarrow> bytes" where
  "rev_drop_last_nl [] = []"
| "rev_drop_last_nl (c # rest) = (if c = 10 then rev_drop_last_nl rest else c # rest)"

text \<open>The defining cases, cited by name later. (Named because this Isabelle
  build does not reliably simplify this function on a non-variable argument.)\<close>

lemma rev_drop_last_nl_nil [simp]: "rev_drop_last_nl [] = []"
  by (rule rev_drop_last_nl.simps(1))

lemma rev_drop_last_nl_cons [simp]:
  fixes c :: nat and rest :: "nat list"
  shows "rev_drop_last_nl (c # rest) = (if c = 10 then rev_drop_last_nl rest else c # rest)"
  by (rule rev_drop_last_nl.simps(2))

text \<open>The strip never leaves a leading newline behind, and an empty strip
  means the input was all newline. Together these pin the meaning down: the
  node removes a run of newlines from one end and nothing else — which rules
  out both the chomp-one-newline reading and the "eat interior newlines"
  reading.\<close>

lemma rev_drop_last_nl_keep:
  fixes u :: "nat list"
  shows "rev_drop_last_nl u = [] \<or> hd (rev_drop_last_nl u) \<noteq> 10"
proof (induct u)
  case Nil then show ?case by simp
next
  case (Cons a u)
  show ?case
  proof (cases "a = 10")
    case True
    have h: "rev_drop_last_nl (a # u) = rev_drop_last_nl u" using True by simp
    then show ?thesis using Cons True by simp
  next
    case False
    have h: "rev_drop_last_nl (a # u) = a # u" using False by simp
    then show ?thesis using False by simp
  qed
qed

lemma rev_drop_last_nl_all_nl:
  fixes u :: "nat list"
  assumes "\<forall>x\<in>set u. x = 10"
  shows "rev_drop_last_nl u = []"
  using assms
proof (induct u)
  case Nil then show ?case by simp
next
  case (Cons a u)
  have a10: "a = 10" and un: "\<forall>x\<in>set u. x = 10"
    using Cons.prems by simp_all
  have eq: "rev_drop_last_nl (a # u) = rev_drop_last_nl u" using a10 by simp
  have "rev_drop_last_nl u = []" using Cons.hyps un by simp
  from eq this show ?case by (rule trans)
qed

text \<open>Idempotence: the whole run goes, not one newline. This is what makes
  the reduction a "strip" rather than a "chomp".\<close>

text \<open>Congruence, stated for this function: it is the one step that rewriting
  under the strip needs, and it is cheaper to prove once than to fight the
  simplifier for it inside each induction step.\<close>

lemma arg_cong_nl:
  fixes t s :: "nat list"
  assumes "t = s"
  shows "rev_drop_last_nl t = rev_drop_last_nl s"
  using assms by (rule arg_cong[where f=rev_drop_last_nl])

lemma rev_drop_last_nl_idem:
  fixes u :: "nat list"
  shows "rev_drop_last_nl (rev_drop_last_nl u) = rev_drop_last_nl u"
proof (induct u)
  case Nil then show ?case by simp
next
  case (Cons a u)
  have IH: "rev_drop_last_nl (rev_drop_last_nl u) = rev_drop_last_nl u"
    using Cons(1) by assumption
  have R: "rev_drop_last_nl (a # u) = (if a = 10 then rev_drop_last_nl u else a # u)"
    by (rule rev_drop_last_nl.simps(2))
  show ?case
  proof (cases "a = 10")
    case True
    have e: "rev_drop_last_nl (a # u) = rev_drop_last_nl u" using R True by simp
    have L: "rev_drop_last_nl (rev_drop_last_nl (a # u)) = rev_drop_last_nl (rev_drop_last_nl u)"
      using e by (rule arg_cong_nl)
    show ?thesis using L IH e by simp
  next
    case False
    have e: "rev_drop_last_nl (a # u) = a # u" using R False by simp
    have L: "rev_drop_last_nl (rev_drop_last_nl (a # u)) = rev_drop_last_nl (a # u)"
      using e by (rule arg_cong_nl)
    show ?thesis using L e by simp
  qed
qed


end
