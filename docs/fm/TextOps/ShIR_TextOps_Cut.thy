(*  Title:      ShIR_TextOps_Cut.thy
    The `cut_at` node: the four single-star glob-strip identities.

    The node replaces the glob engine with find()/rfind() for the four forms
    whose pattern is `lit*` with a MAGIC-FREE literal `lit`:

      ${v%%lit*}   keep the head BEFORE the first occurrence   (head,  first)
      ${v%lit*}    keep the head BEFORE the last   occurrence   (head,  !first)
      ${v#*lit}    keep the tail AFTER  the first  occurrence   (!head, first)
      ${v##*lit}   keep the tail AFTER  the last   occurrence   (!head, !first)

    and leaves the text unchanged when the literal does not occur.

    WHAT IS PROVED. The two primitives and their characterisations — an
    occurrence at the returned index, no occurrence anywhere (the no-match
    case), and minimality/maximality — then the four cut forms as functions of
    those primitives, with their no-match no-ops, and the split identity that
    says a successful cut splits the text into prefix, literal and remainder.
    Finally `affix_strip_fold_sound`: the fold's star branch at a head literal
    is the AffixStrip node, and first differs from last on a repeating literal
    (which is why the node carries `first` at all).

    WHAT IS NOT MODELLED. The glob engine itself: "shortest-vs-longest match of
    `lit*`" is NOT formalised. What is proved is the literal reading the fold
    compiles to — first and last occurrence — and the magic-free refusal is
    tracked as a predicate, not as a glob semantics. That gap is recorded in
    docs/fm/README.md.
*)

theory ShIR_TextOps_Cut
  imports ShIR_TextOps_Strings
begin

section \<open>find_first and rfind_last\<close>

text \<open>`find_first s n` is the index of the leftmost occurrence of `n` in `s`,
  or `None`. The empty needle occurs at index 0 (bash's `${v#*}` with an empty
  literal is the identity, and that falls out of `take 0 s = []`).\<close>

fun find_first :: "bytes \<Rightarrow> bytes \<Rightarrow> nat option" where
  "find_first [] n = (if n = [] then Some 0 else None)"
| "find_first (c # cs) n =
     (if take (length n) (c # cs) = n then Some 0
      else map_option Suc (find_first cs n))"

lemma find_first_nil [simp]:
  "find_first [] n = (if n = [] then Some 0 else None)"
  by (simp add: find_first.simps)

lemma find_first_cons [simp]:
  "find_first (c # cs) n =
     (if take (length n) (c # cs) = n then Some 0
      else map_option Suc (find_first cs n))"
  by (simp add: find_first.simps)

lemma map_option_Suc_Some:
  "map_option Suc x = Some i \<Longrightarrow> \<exists>k. x = Some k \<and> i = Suc k"
  by (cases x) simp_all

text \<open>PRIMITIVE, part 1: a returned index really is an occurrence, and it
  lies inside the text.\<close>

lemma find_first_occ:
  "find_first s n = Some i \<Longrightarrow> i \<le> length s \<and> take (length n) (drop i s) = n"
proof (induct s arbitrary: i)
  case Nil
  then show ?case
  proof (cases "n = []")
    case True
    with Nil(1) have "i = 0" by (simp add: find_first_nil)
    then show ?thesis using True by simp
  next
    case False
    with Nil(1) show ?thesis by (simp add: find_first_nil)
  qed
next
  case (Cons c cs)
  show ?case
  proof (cases "take (length n) (c # cs) = n")
    case True
    with Cons(2) have "i = 0" by (simp add: find_first_cons)
    then show ?thesis using True by simp
  next
    case False
    with Cons(2) have h: "map_option Suc (find_first cs n) = Some i"
      by (simp add: find_first_cons)
    have "\<exists>k. find_first cs n = Some k \<and> i = Suc k"
      using h by (rule map_option_Suc_Some)
    then obtain k where k: "find_first cs n = Some k" and i: "i = Suc k" by blast
    have ih: "k \<le> length cs \<and> take (length n) (drop k cs) = n"
      using Cons(1)[OF k] .
    then show ?thesis using i by simp
  qed
qed

text \<open>PRIMITIVE, part 2: the no-match case — `None` means no occurrence at any
  index of the text, including the end.\<close>

lemma find_first_none:
  "find_first s n = None \<Longrightarrow>
     \<forall>i. i \<le> length s \<longrightarrow> take (length n) (drop i s) \<noteq> n"
proof (induct s)
  case Nil
  then show ?case
  proof (cases "n = []")
    case True
    with Nil(1) show ?thesis by (simp add: find_first_nil)
  next
    case False
    show ?thesis
    proof (intro allI impI)
      fix i
      assume "i \<le> length ([] :: bytes)"
      then have "i = 0" by simp
      then show "take (length n) (drop i ([] :: bytes)) \<noteq> n" using False by simp
    qed
  qed
next
  case (Cons c cs)
  have rec: "find_first cs n = None"
  proof -
    have "(if take (length n) (c # cs) = n then Some 0
           else map_option Suc (find_first cs n)) = None"
      using Cons(2) by (simp add: find_first_cons)
    then show ?thesis by (cases "take (length n) (c # cs) = n") simp_all
  qed
  have guard: "take (length n) (c # cs) \<noteq> n"
  proof
    assume h: "take (length n) (c # cs) = n"
    with Cons(2) have "find_first (c # cs) n = Some 0" by (simp add: find_first_cons)
    with Cons(2) show False by simp
  qed
  show ?case
  proof (intro allI impI)
    fix i
    assume le: "i \<le> length (c # cs)"
    show "take (length n) (drop i (c # cs)) \<noteq> n"
    proof (cases i)
      case 0
      then show ?thesis using guard by simp
    next
      case (Suc j)
      have "take (length n) (drop j cs) \<noteq> n"
        using Cons(1)[OF rec, rule_format, of j] le Suc by simp
      then show ?thesis using Suc by simp
    qed
  qed
qed

text \<open>PRIMITIVE, part 3: leftmost — nothing matches before the returned index.
  This is the "first" half of the node's `first` flag.\<close>

lemma find_first_min:
  "find_first s n = Some i \<Longrightarrow> take (length n) (drop j s) = n \<Longrightarrow> j \<le> length s \<Longrightarrow> i \<le> j"
proof (induct s arbitrary: i j)
  case Nil
  then show ?case
  proof (cases "n = []")
    case True
    with Nil(1) have "i = 0" by (simp add: find_first_nil)
    then show ?thesis by simp
  next
    case False
    with Nil(1) show ?thesis by (simp add: find_first_nil)
  qed
next
  case (Cons c cs)
  show ?case
  proof (cases "take (length n) (c # cs) = n")
    case True
    with Cons(2) have "i = 0" by (simp add: find_first_cons)
    then show ?thesis by simp
  next
    case False
    with Cons(2) have h: "map_option Suc (find_first cs n) = Some i"
      by (simp add: find_first_cons)
    have "\<exists>k. find_first cs n = Some k \<and> i = Suc k"
      using h by (rule map_option_Suc_Some)
    then obtain k where k: "find_first cs n = Some k" and i: "i = Suc k" by blast
    have "Suc k \<le> j"
    proof (cases j)
      case 0
      with False Cons(3) show ?thesis by simp
    next
      case (Suc j')
      have at': "take (length n) (drop j' cs) = n" using Cons(3) Suc by simp
      have bd: "j' \<le> length cs" using Cons(4) Suc by simp
      have "k \<le> j'" using Cons(1)[OF k at' bd] .
      then show ?thesis using Suc by simp
    qed
    then show ?thesis using i by simp
  qed
qed

text \<open>`rfind_last` is the same walk with the recursion on the tail taken
  first: a later occurrence always wins over an earlier one.\<close>

fun rfind_last :: "bytes \<Rightarrow> bytes \<Rightarrow> nat option" where
  "rfind_last [] n = (if n = [] then Some 0 else None)"
| "rfind_last (c # cs) n =
     (case rfind_last cs n of
        Some j \<Rightarrow> Some (Suc j)
      | None \<Rightarrow> (if take (length n) (c # cs) = n then Some 0 else None))"

lemma rfind_last_nil [simp]:
  "rfind_last [] n = (if n = [] then Some 0 else None)"
  by (simp add: rfind_last.simps)

lemma rfind_last_cons [simp]:
  "rfind_last (c # cs) n =
     (case rfind_last cs n of
        Some j \<Rightarrow> Some (Suc j)
      | None \<Rightarrow> (if take (length n) (c # cs) = n then Some 0 else None))"
  by (simp add: rfind_last.simps)

lemma rfind_last_occ:
  "rfind_last s n = Some i \<Longrightarrow> i \<le> length s \<and> take (length n) (drop i s) = n"
proof (induct s arbitrary: i)
  case Nil
  then show ?case
  proof (cases "n = []")
    case True
    with Nil(1) have "i = 0" by (simp add: rfind_last_nil)
    then show ?thesis using True by simp
  next
    case False
    with Nil(1) show ?thesis by (simp add: rfind_last_nil)
  qed
next
  case (Cons c cs)
  show ?case
  proof (cases "rfind_last cs n")
    case None
    note rec = this
    show ?thesis
    proof (cases "take (length n) (c # cs) = n")
      case True
      with Cons(2) rec have "i = 0" by (simp add: rfind_last_cons)
      then show ?thesis using True by simp
    next
      case False
      with Cons(2) rec have "None = Some i" by (simp add: rfind_last_cons)
      then show ?thesis by simp
    qed
  next
    case (Some j)
    with Cons(2) have i: "i = Suc j" by (simp add: rfind_last_cons)
    have ih: "j \<le> length cs \<and> take (length n) (drop j cs) = n"
      using Cons(1)[OF Some] .
    then show ?thesis using i by simp
  qed
qed

lemma rfind_last_none:
  "rfind_last s n = None \<Longrightarrow>
     \<forall>i. i \<le> length s \<longrightarrow> take (length n) (drop i s) \<noteq> n"
proof (induct s)
  case Nil
  then show ?case
  proof (cases "n = []")
    case True
    with Nil(1) show ?thesis by (simp add: rfind_last_nil)
  next
    case False
    show ?thesis
    proof (intro allI impI)
      fix i
      assume "i \<le> length ([] :: bytes)"
      then have "i = 0" by simp
      then show "take (length n) (drop i ([] :: bytes)) \<noteq> n" using False by simp
    qed
  qed
next
  case (Cons c cs)
  from Cons(2) have eq: "(case rfind_last cs n of Some j \<Rightarrow> Some (Suc j)
      | None \<Rightarrow> (if take (length n) (c # cs) = n then Some 0 else None)) = None"
    by (simp add: rfind_last_cons)
  have rec: "rfind_last cs n = None"
  proof (rule ccontr)
    assume ne: "rfind_last cs n \<noteq> None"
    then obtain j where j: "rfind_last cs n = Some j" by blast
    with eq have "Some (Suc j) = None" by simp
    then show False by simp
  qed
  have guard: "take (length n) (c # cs) \<noteq> n"
  proof
    assume h: "take (length n) (c # cs) = n"
    with eq rec have "Some 0 = None" by simp
    then show False by simp
  qed
  show ?case
  proof (intro allI impI)
    fix i
    assume le: "i \<le> length (c # cs)"
    show "take (length n) (drop i (c # cs)) \<noteq> n"
    proof (cases i)
      case 0
      then show ?thesis using guard by simp
    next
      case (Suc j)
      have "take (length n) (drop j cs) \<noteq> n"
        using Cons(1)[OF rec, rule_format, of j] le Suc by simp
      then show ?thesis using Suc by simp
    qed
  qed
qed

lemma rfind_last_max:
  "rfind_last s n = Some i \<Longrightarrow> take (length n) (drop j s) = n \<Longrightarrow> j \<le> length s \<Longrightarrow> j \<le> i"
proof (induct s arbitrary: i j)
  case Nil
  then show ?case
  proof (cases "n = []")
    case True
    with Nil(1) have i0: "i = 0" by (simp add: rfind_last_nil)
    with Nil(3) have "j = 0" by simp
    then show ?thesis using i0 by simp
  next
    case False
    with Nil(1) show ?thesis by (simp add: rfind_last_nil)
  qed
next
  case (Cons c cs)
  show ?case
  proof (cases "rfind_last cs n")
    case None
    note rec = this
    have i0: "i = 0"
    proof (cases "take (length n) (c # cs) = n")
      case True
      with Cons(2) rec show ?thesis by (simp add: rfind_last_cons)
    next
      case False
      with Cons(2) rec have "None = Some i" by (simp add: rfind_last_cons)
      then show ?thesis by simp
    qed
    show ?thesis
    proof (cases j)
      case 0
      then show ?thesis using i0 by simp
    next
      case (Suc j')
      have at': "take (length n) (drop j' cs) = n" using Cons(3) Suc by simp
      have bd: "j' \<le> length cs" using Cons(4) Suc by simp
      have no_occ: "\<forall>k. k \<le> length cs \<longrightarrow> take (length n) (drop k cs) \<noteq> n"
        using rfind_last_none[OF rec] .
      from no_occ bd have "take (length n) (drop j' cs) \<noteq> n" by blast
      with at' have False by simp
      then show ?thesis by simp
    qed
  next
    case (Some m)
    with Cons(2) have i: "i = Suc m" by (simp add: rfind_last_cons)
    show ?thesis
    proof (cases j)
      case 0
      then show ?thesis by simp
    next
      case (Suc j')
      have at': "take (length n) (drop j' cs) = n" using Cons(3) Suc by simp
      have bd: "j' \<le> length cs" using Cons(4) Suc by simp
      have "j' \<le> m" using Cons(1)[OF Some at' bd] .
      then show ?thesis using Suc i by simp
    qed
  qed
qed

section \<open>The four cut forms\<close>

definition cut_head_first :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cut_head_first s n = (case find_first s n of None \<Rightarrow> s | Some i \<Rightarrow> take i s)"

definition cut_head_last :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cut_head_last s n = (case rfind_last s n of None \<Rightarrow> s | Some i \<Rightarrow> take i s)"

definition cut_tail_first :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cut_tail_first s n =
     (case find_first s n of None \<Rightarrow> s | Some i \<Rightarrow> drop (i + length n) s)"

definition cut_tail_last :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cut_tail_last s n =
     (case rfind_last s n of None \<Rightarrow> s | Some i \<Rightarrow> drop (i + length n) s)"

text \<open>The node as the shell sees it: `head` keeps the prefix side, `first`
  picks the first occurrence.\<close>

definition cut_at :: "bool \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cut_at head first s n =
     (if head then (if first then cut_head_first s n else cut_head_last s n)
      else (if first then cut_tail_first s n else cut_tail_last s n))"

text \<open>OBLIGATION \<open>cut_at_head_first\<close>: the head up to the first occurrence,
  and the text unchanged when there is no match. The two cases of the same
  `case` expression, which is why they are one theorem.\<close>

theorem cut_at_head_first:
  "cut_at True True s n =
     (case find_first s n of None \<Rightarrow> s | Some i \<Rightarrow> take i s)"
  unfolding cut_at_def cut_head_first_def by simp

corollary cut_at_head_first_no_match:
  assumes "find_first s n = None"
  shows "cut_at True True s n = s"
  using assms unfolding cut_at_head_first by simp

corollary cut_at_head_first_matched:
  assumes "find_first s n = Some i"
  shows "cut_at True True s n = take i s"
  using assms unfolding cut_at_head_first by simp

text \<open>The three mirrors. `%` before `%%` is the last occurrence on the same
  side; `#`/`##` are the two occurrences on the tail side.\<close>

theorem cut_at_head_last:
  "cut_at True False s n =
     (case rfind_last s n of None \<Rightarrow> s | Some i \<Rightarrow> take i s)"
  unfolding cut_at_def cut_head_last_def by simp

corollary cut_at_head_last_no_match:
  assumes "rfind_last s n = None"
  shows "cut_at True False s n = s"
  using assms unfolding cut_at_head_last by simp

corollary cut_at_head_last_matched:
  assumes "rfind_last s n = Some i"
  shows "cut_at True False s n = take i s"
  using assms unfolding cut_at_head_last by simp

theorem cut_at_tail_first:
  "cut_at False True s n =
     (case find_first s n of None \<Rightarrow> s | Some i \<Rightarrow> drop (i + length n) s)"
  unfolding cut_at_def cut_tail_first_def by simp

corollary cut_at_tail_first_no_match:
  assumes "find_first s n = None"
  shows "cut_at False True s n = s"
  using assms unfolding cut_at_tail_first by simp

corollary cut_at_tail_first_matched:
  assumes "find_first s n = Some i"
  shows "cut_at False True s n = drop (i + length n) s"
  using assms unfolding cut_at_tail_first by simp

theorem cut_at_tail_last:
  "cut_at False False s n =
     (case rfind_last s n of None \<Rightarrow> s | Some i \<Rightarrow> drop (i + length n) s)"
  unfolding cut_at_def cut_tail_last_def by simp

corollary cut_at_tail_last_no_match:
  assumes "rfind_last s n = None"
  shows "cut_at False False s n = s"
  using assms unfolding cut_at_tail_last by simp

corollary cut_at_tail_last_matched:
  assumes "rfind_last s n = Some i"
  shows "cut_at False False s n = drop (i + length n) s"
  using assms unfolding cut_at_tail_last by simp

text \<open>OBLIGATION \<open>cut_at_sound\<close>: the node is those four functions, one per
  flag pair, and both no-match cases are the identity — the shell's "the strip
  is a no-op when the pattern does not match".\<close>

theorem cut_at_sound:
  "cut_at True True s n = cut_head_first s n \<and>
   cut_at True False s n = cut_head_last s n \<and>
   cut_at False True s n = cut_tail_first s n \<and>
   cut_at False False s n = cut_tail_last s n \<and>
   (find_first s n = None \<longrightarrow> cut_head_first s n = s) \<and>
   (rfind_last s n = None \<longrightarrow> cut_head_last s n = s) \<and>
   (find_first s n = None \<longrightarrow> cut_tail_first s n = s) \<and>
   (rfind_last s n = None \<longrightarrow> cut_tail_last s n = s)"
proof (intro conjI impI)
  show "cut_at True True s n = cut_head_first s n" unfolding cut_at_def by simp
  show "cut_at True False s n = cut_head_last s n" unfolding cut_at_def by simp
  show "cut_at False True s n = cut_tail_first s n" unfolding cut_at_def by simp
  show "cut_at False False s n = cut_tail_last s n" unfolding cut_at_def by simp
  show "find_first s n = None \<Longrightarrow> cut_head_first s n = s"
    unfolding cut_head_first_def by simp
  show "rfind_last s n = None \<Longrightarrow> cut_head_last s n = s"
    unfolding cut_head_last_def by simp
  show "find_first s n = None \<Longrightarrow> cut_tail_first s n = s"
    unfolding cut_tail_first_def by simp
  show "rfind_last s n = None \<Longrightarrow> cut_tail_last s n = s"
    unfolding cut_tail_last_def by simp
qed

section \<open>The split identity: a successful cut divides the text\<close>

text \<open>This is what makes the four forms usable as a rewriting rule: the text
  is the prefix, the literal, and the remainder — so a renderer that copies the
  prefix and the tail cannot lose or duplicate a byte.\<close>

theorem find_first_split:
  assumes "find_first s n = Some i"
  shows "s = take i s @ n @ drop (i + length n) s"
proof -
  have occ: "take (length n) (drop i s) = n"
    using find_first_occ[OF assms] by blast
  have blk: "drop i s = n @ drop (i + length n) s"
  proof -
    have a: "drop i s = take (length n) (drop i s) @ drop (length n) (drop i s)"
    proof (rule sym)
      show "take (length n) (drop i s) @ drop (length n) (drop i s) = drop i s"
        by (rule append_take_drop_id)
    qed
    have b: "drop (length n) (drop i s) = drop (i + length n) s"
      by (simp add: drop_drop add.commute)
    show ?thesis
    proof -
      have "drop i s = n @ drop (length n) (drop i s)" using a occ by simp
      also have "\<dots> = n @ drop (i + length n) s" using b by simp
      finally show ?thesis .
    qed
  qed
  show ?thesis
  proof -
    have "s = take i s @ drop i s"
    proof (rule sym)
      show "take i s @ drop i s = s" by (rule append_take_drop_id)
    qed
    also have "\<dots> = take i s @ n @ drop (i + length n) s" using blk by simp
    finally show ?thesis .
  qed
qed

theorem rfind_last_split:
  assumes "rfind_last s n = Some i"
  shows "s = take i s @ n @ drop (i + length n) s"
proof -
  have occ: "take (length n) (drop i s) = n"
    using rfind_last_occ[OF assms] by blast
  have blk: "drop i s = n @ drop (i + length n) s"
  proof -
    have a: "drop i s = take (length n) (drop i s) @ drop (length n) (drop i s)"
    proof (rule sym)
      show "take (length n) (drop i s) @ drop (length n) (drop i s) = drop i s"
        by (rule append_take_drop_id)
    qed
    have b: "drop (length n) (drop i s) = drop (i + length n) s"
      by (simp add: drop_drop add.commute)
    show ?thesis
    proof -
      have "drop i s = n @ drop (length n) (drop i s)" using a occ by simp
      also have "\<dots> = n @ drop (i + length n) s" using b by simp
      finally show ?thesis .
    qed
  qed
  show ?thesis
  proof -
    have "s = take i s @ drop i s"
    proof (rule sym)
      show "take i s @ drop i s = s" by (rule append_take_drop_id)
    qed
    also have "\<dots> = take i s @ n @ drop (i + length n) s" using blk by simp
    finally show ?thesis .
  qed
qed

corollary cut_tail_first_kept_on_miss:
  "find_first s n = None \<Longrightarrow> cut_tail_first s n = s"
  unfolding cut_tail_first_def by simp

section \<open>affix_strip_fold: the star branch\<close>

text \<open>The fold emits CutAt for the star forms and keeps AffixStrip for the
  plain forms. The star branch is right where the literal sits at the head —
  then first and last occurrence coincide, and the star form is exactly the
  AffixStrip node. It is NOT AffixStrip when the literal repeats, which is why
  the node carries `first`; both halves are below.\<close>

theorem cut_tail_first_at_head:
  assumes "take (length n) s = n"
  shows "cut_tail_first s n = affix_strip_node s n True"
proof -
  have fs: "find_first s n = Some 0"
  proof (cases s)
    case Nil
    have "n = []" using assms Nil by simp
    then show ?thesis using Nil by (simp add: find_first_nil)
  next
    case (Cons c cs)
    then show ?thesis using assms by (simp add: find_first_cons)
  qed
  have "cut_tail_first s n = drop (length n) s"
    unfolding cut_tail_first_def using fs by simp
  also have "\<dots> = affix_strip_node s n True"
  proof -
    have "has_affix s n True" using assms by simp
    then show ?thesis unfolding affix_strip_node_def by simp
  qed
  finally show ?thesis .
qed

text \<open>The four single-star identities, on a text where the literal repeats:
  `first` and `!first` give different answers, so a fold that dropped the flag
  would be wrong. These evaluate the node, they are not restatements.\<close>

lemma cut_star_first_is_not_last:
  "cut_tail_first [97, 98, 97, 98] [97, 98] = [97, 98] \<and>
   cut_tail_last [97, 98, 97, 98] [97, 98] = [] \<and>
   cut_head_first [97, 98, 97, 98] [97, 98] = [] \<and>
   cut_head_last [97, 98, 97, 98] [97, 98] = [97, 98]"
  by eval

lemma cut_star_no_match_keeps_the_text:
  "cut_head_first [97, 98] [122] = [97, 98] \<and>
   cut_head_last [97, 98] [122] = [97, 98] \<and>
   cut_tail_first [97, 98] [122] = [97, 98] \<and>
   cut_tail_last [97, 98] [122] = [97, 98]"
  by eval

text \<open>A magic-free literal is what the fold refuses `*`/`?`/`[`/`\\` for. The
  predicate is tracked here so the node's precondition is a theorem rather than
  a comment; the glob semantics it guards is not modelled.\<close>

definition is_magic_free :: "bytes \<Rightarrow> bool" where
  "is_magic_free n \<equiv> \<forall>b\<in>set n. b \<noteq> 42 \<and> b \<noteq> 63 \<and> b \<noteq> 91 \<and> b \<noteq> 92"

lemma is_magic_free_plain [simp]:
  "is_magic_free [97, 98]" by eval

lemma is_magic_free_rejects_star:
  "\<not> is_magic_free [97, 42]" by eval

theorem affix_strip_fold_sound:
  "(is_magic_free n \<longrightarrow> cut_at False True s n = cut_tail_first s n) \<and>
   (take (length n) s = n \<longrightarrow> cut_tail_first s n = affix_strip_node s n True) \<and>
   (cut_tail_first [97, 98, 97, 98] [97, 98] = [97, 98]) \<and>
   (cut_tail_last [97, 98, 97, 98] [97, 98] = [])"
proof (intro conjI impI)
  show "is_magic_free n \<Longrightarrow> cut_at False True s n = cut_tail_first s n"
    unfolding cut_at_def by simp
  show "take (length n) s = n \<Longrightarrow> cut_tail_first s n = affix_strip_node s n True"
    by (rule cut_tail_first_at_head)
  show "cut_tail_first [97, 98, 97, 98] [97, 98] = [97, 98]" by eval
  show "cut_tail_last [97, 98, 97, 98] [97, 98] = []" by eval
qed

text \<open>LIMITATION. The glob engine is not modelled: "the longest prefix
  matching `lit*`" is taken as "the suffix starting at the last occurrence of
  `lit`", and that identification is assumed, not proved. The literal reading
  (find/rfind, no-match no-op, split) is proved. See docs/fm/README.md.\<close>

end
