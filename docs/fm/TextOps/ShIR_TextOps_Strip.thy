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


section \<open>The node's meaning, stated forward (obligation `newline_strip_sound`)\<close>

text \<open>`newline_strip.node` cites `TextOps#newline_strip_sound`, and until this
  section no theory contained that name. The three lemmas above are about the
  reversed string; the node is about the text as handed to it, so the
  obligation is a definition and one equation:

    newline_strip s  ==  the text with its TRAILING newline run removed

  written as `take (length s - lead_nl_count (rev s)) s` — a prefix of the
  input whose length is `length s` minus the size of the trailing run. That
  form is deliberately a `take`/`drop` equation rather than a cons/append
  rewrite: the box trap documented in `docs/fm/README.md` is about rewriting
  `a # (x @ y)` under a hypothesis, and this statement never does.\<close>

fun lead_nl_count :: "bytes \<Rightarrow> nat" where
  "lead_nl_count [] = 0"
| "lead_nl_count (c # cs) = (if c = 10 then Suc (lead_nl_count cs) else 0)"

lemma lead_nl_count_nil [simp]: "lead_nl_count [] = 0"
  by (rule lead_nl_count.simps(1))

lemma lead_nl_count_cons [simp]:
  "lead_nl_count (c # cs) = (if c = 10 then Suc (lead_nl_count cs) else 0)"
  by (rule lead_nl_count.simps(2))

text \<open>One more defining fact about the reversal, which is where the count
  meets the function: stripping the leading run of `u` KEEPS a suffix of `u`,
  of length `length u - lead_nl_count u`. (The open target that used to be
  called `rev_drop_last_nl_prefix` is this equation — the name was about the
  node keeping a PREFIX of its input, which is `newline_strip_prefix` below.)\<close>

lemma rev_drop_last_nl_length:
  "length (rev_drop_last_nl u) = length u - lead_nl_count u"
proof (induct u)
  case Nil
  show ?case by simp
next
  case (Cons a u)
  show ?case
  proof (cases "a = 10")
    case True
    have ih: "length (rev_drop_last_nl u) = length u - lead_nl_count u"
      using Cons(1) .
    show ?thesis using True ih by simp
  next
    case False
    show ?thesis using False by simp
  qed
qed

lemma rev_drop_last_nl_suffix:
  "rev_drop_last_nl u = drop (lead_nl_count u) u"
proof (induct u)
  case Nil
  show ?case by simp
next
  case (Cons a u)
  show ?case
  proof (cases "a = 10")
    case True
    have ih: "rev_drop_last_nl u = drop (lead_nl_count u) u" using Cons(1) .
    have "rev_drop_last_nl (a # u) = drop (lead_nl_count u) u"
      using True ih by simp
    with True show ?thesis by simp
  next
    case False
    show ?thesis using False by simp
  qed
qed

lemma rev_drop_last_nl_stable:
  assumes "u \<noteq> []" and "hd u \<noteq> 10"
  shows "rev_drop_last_nl u = u"
proof (cases u)
  case Nil
  with assms show ?thesis by simp
next
  case (Cons a rest)
  with assms show ?thesis by simp
qed

definition newline_strip :: "bytes \<Rightarrow> bytes" where
  "newline_strip s \<equiv> rev (rev_drop_last_nl (rev s))"

lemma newline_strip_nil [simp]: "newline_strip [] = []"
  unfolding newline_strip_def by simp

lemma newline_strip_append [simp]:
  "newline_strip (xs @ [x]) = (if x = 10 then newline_strip xs else xs @ [x])"
  unfolding newline_strip_def by simp

text \<open>THE OBLIGATION the node file names: the node removes exactly the
  trailing newline run, and nothing else.\<close>

theorem newline_strip_sound:
  "newline_strip s = take (length s - lead_nl_count (rev s)) s"
proof -
  have drop_eq: "rev_drop_last_nl (rev s) = drop (lead_nl_count (rev s)) (rev s)"
    by (rule rev_drop_last_nl_suffix)
  have "newline_strip s = rev (drop (lead_nl_count (rev s)) (rev s))"
    unfolding newline_strip_def using drop_eq by simp
  also have "\<dots> = take (length s - lead_nl_count (rev s)) s"
    by (simp add: rev_drop)
  finally show ?thesis .
qed

lemma newline_strip_length:
  "length (newline_strip s) = length s - lead_nl_count (rev s)"
  using newline_strip_sound by simp

lemma newline_strip_prefix:
  "\<exists>k. newline_strip s = take k s"
  using newline_strip_sound by blast

text \<open>The removal is a RUN: the last kept byte is not a newline (unless
  nothing is kept), which is what separates this node from a one-newline
  chomp.\<close>

lemma newline_strip_keep:
  "newline_strip s = [] \<or> last (newline_strip s) \<noteq> 10"
proof (cases "rev_drop_last_nl (rev s) = []")
  case True
  then show ?thesis unfolding newline_strip_def by simp
next
  case False
  have hd: "hd (rev_drop_last_nl (rev s)) \<noteq> 10"
    using rev_drop_last_nl_keep False by blast
  have "last (newline_strip s) = hd (rev_drop_last_nl (rev s))"
    unfolding newline_strip_def by (simp add: last_rev)
  with hd show ?thesis by simp
qed

text \<open>Idempotence: stripping a stripped text changes nothing, because the
  kept part no longer ends in a newline (or is empty).\<close>

lemma newline_strip_idem:
  "newline_strip (newline_strip s) = newline_strip s"
proof (cases "rev_drop_last_nl (rev s) = []")
  case True
  have "newline_strip s = []" unfolding newline_strip_def using True by simp
  then show ?thesis by simp
next
  case False
  have hd: "hd (rev_drop_last_nl (rev s)) \<noteq> 10"
    using rev_drop_last_nl_keep False by blast
  have stable: "rev_drop_last_nl (rev_drop_last_nl (rev s))
                   = rev_drop_last_nl (rev s)"
    by (rule rev_drop_last_nl_stable) (simp_all add: False hd)
  have "newline_strip (newline_strip s)
          = rev (rev_drop_last_nl (rev_drop_last_nl (rev s)))"
    unfolding newline_strip_def by simp
  also have "\<dots> = rev (rev_drop_last_nl (rev s))" by (simp add: stable)
  finally show ?thesis unfolding newline_strip_def .
qed

end
