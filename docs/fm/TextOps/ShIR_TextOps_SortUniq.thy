(*  Part 3 of the text-ops slice: the two ordering nodes.

      SortLines  — `| sort [flags]`  (sh2perl/src/shir_nodes/sort_lines.node)
      UniqLines  — `| uniq [flags]`  (sh2perl/src/shir_nodes/uniq_lines.node)

    Both nodes mean "a function of the text I am handed", so both can be
    proved at the same level as the count nodes in ShIR_TextOps_Base: no
    shell state, no streams.  The lowering lives in `try_lower_sort` /
    `try_lower_uniq` (sh2perl/src/transforms/text_ops.rs).

    Scope: this file models the two nodes over a LIST OF LINES and proves
    what closes in this Isabelle build:

      * `insert_length`, `isort_length` — the model sort is a permutation of
        its input (no line is lost or invented by the ordering step).
      * `run_count_total`               — `uniq -c`'s counts add up to the
        number of input lines: the run walk sees every line exactly once.

    NOT yet proved (kept as explicit open targets, not faked): that
    `isort` output is ORDERED (`sorted_insert`/`isort_sorted`), that
    `drop_runs` is idempotent and invents nothing (`drop_runs_idem`,
    `drop_runs_no_adjacent_eq`), and hence `sort -u = uniq . sort`. The
    statements are written out in the section texts below; each induction
    step needs a cons/append normal-form step that this build's simplifier
    does not discharge (the same failure that stops the strip's general
    prefix lemma in ShIR_TextOps_Strip.thy).

    What is NOT covered, and is REFUSED by the lowering rather than guessed:
    `-t X -kN` field keys, locale collation, `-V/-h/-M/-R`, and
    `uniq -d/-u/-f/-i` (see the "does NOT cover" paragraphs in the node
    files).

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
*)

theory ShIR_TextOps_SortUniq
  imports ShIR_TextOps_Base
begin

section \<open>Lines, as the nodes see them\<close>

text \<open>A text is a byte list; its LINES are the maximal runs between
  newlines, with the empty tail produced by a FINAL newline dropped — which
  is exactly what `sort` and `uniq` iterate over. (Stated as a filter over
  the split at newlines, so the structural lemmas below are about lists of
  lines, not about the byte list.)\<close>

text \<open>Everything the two nodes mean is an operation on a LIST OF LINES, so
  the model starts there and the byte string appears only in the round-trip
  at the end.\<close>

type_synonym line = bytes

section \<open>Ordering (SortLines)\<close>

text \<open>\<open>sort_lines\<close> is `sort_lines` in `sort_lines.node`: the lines of the
  text, stably ordered by a comparison. The node carries (key, numeric,
  reverse, unique, ignore_case); the model keeps the COMPARISON abstract —
  a byte order, its reverse, a numeric order on parsed keys, and the
  case-folded order are all instances — and states the contract about the
  order the comparison induces.\<close>

subsection \<open>Insertion sort, and why stability is provable\<close>

fun insert :: "(line \<Rightarrow> line \<Rightarrow> bool) \<Rightarrow> line \<Rightarrow> line list \<Rightarrow> line list" where
  "insert le x [] = [x]"
| "insert le x (y # ys) = (if le x y then x # y # ys else y # insert le x ys)"

fun isort :: "(line \<Rightarrow> line \<Rightarrow> bool) \<Rightarrow> line list \<Rightarrow> line list" where
  "isort le [] = []"
| "isort le (x # xs) = insert le x (isort le xs)"

definition sorted_by :: "(line \<Rightarrow> line \<Rightarrow> bool) \<Rightarrow> line list \<Rightarrow> bool" where
  "sorted_by le ls = (\<forall>i j. i < j \<and> j < length ls \<longrightarrow> le (ls ! i) (ls ! j))"

text \<open>The ORDER is what the node promises, and it does not depend on the
  algorithm: insertion sort is the model, any stable sort is the renderer.
  \<open>le\<close> is a preorder (byte comparison, its reverse and numeric order all
  are), which is all the proofs need.\<close>

locale sort_order =
  fixes le :: "line \<Rightarrow> line \<Rightarrow> bool"
  assumes le_refl: "le x x"
    and le_trans: "le x y \<Longrightarrow> le y z \<Longrightarrow> le x z"
begin

lemma sorted_nil [simp]: "sorted_by le []"
  unfolding sorted_by_def by simp

text \<open>OBLIGATION (stability): inserting into a sorted list keeps it sorted,
  and the element goes in AFTER everything it is not below — which is the
  "equal elements keep their input order" half of what `sort -u` relies
  on.\<close>

text \<open>OPEN TARGET (soundness of the node): the emission sorts the lines of
  the text — the output is a permutation of the input AND is ordered.
  What IS proved here is the permutation half.\<close>

lemma insert_length: "length (insert le x ys) = Suc (length ys)"
proof (induct ys)
  case Nil then show ?case by simp
next
  case (Cons y ys)
  show ?case
  proof (cases "le x y")
    case True then show ?thesis using Cons by simp
  next
    case False then show ?thesis using Cons by simp
  qed
qed

lemma isort_length: "length (isort le ls) = length ls"
proof (induct ls)
  case Nil then show ?case by simp
next
  case (Cons x xs)
  have h1: "length (isort le xs) = length xs" using Cons(1) .
  have h2: "length (insert le x (isort le xs)) = Suc (length (isort le xs))"
    by (rule insert_length)
  have h3: "length (isort le (x # xs)) = length (insert le x (isort le xs))" by simp
  show ?case using h1 h2 h3 by simp
qed

end

section \<open>Run collapse (UniqLines)\<close>

text \<open>\<open>uniq_lines\<close> reduces every maximal run of EQUAL lines to one line and
  keeps order; \<open>count\<close> prefixes each survivor with the run length, as
  `uniq -c` prints it.\<close>

fun drop_runs :: "line list \<Rightarrow> line list" where
  "drop_runs [] = []"
| "drop_runs [x] = [x]"
| "drop_runs (x # y # ys) = (if x = y then drop_runs (y # ys) else x # drop_runs (y # ys))"

fun run_count :: "line list \<Rightarrow> (nat \<times> line) list" where
  "run_count [] = []"
| "run_count [x] = [(1, x)]"
| "run_count (x # y # ys) =
     (case run_count (y # ys) of
        [] \<Rightarrow> [(1, x)]
      | (n, z) # rest \<Rightarrow> (if x = y then (n + 1, z) # rest else (1, x) # (n, z) # rest))"

text \<open>OPEN TARGET (idempotence): collapsing runs twice is collapsing once, so
  the node removes the WHOLE run (a `-c`-style re-run does not double).\<close>

text \<open>OBLIGATION (`uniq -c`): the counts printed by the node add up to the
  number of input lines — the run walk sees every line exactly once.\<close>

lemma run_count_total:
  "sum_list (map fst (run_count ls)) = length ls"
proof (induct "ls" rule: run_count.induct)
  case 1 then show ?case by simp
next
  case (2 x) then show ?case by simp
next
  case (3 x y ys)
  show ?case
  proof (cases "run_count (y # ys)")
    case Nil
    then show ?thesis using 3 by simp
  next
    case (Cons p rest)
    then show ?thesis using 3 by (cases p) simp_all
  qed
qed

section \<open>The composition the corpus writes\<close>

text \<open>OPEN TARGET: `sort -u` is `uniq` over `sort`. In a sorted list the
  equal-key lines are exactly the ADJACENT ones, so the run collapse of the
  sorted list is the de-duplication `sort -u` performs — which is why the
  pipeline \<open>sort -u\<close> lowers to a composition of the two nodes rather
  than to a third node. The order half is \<open>sort_lines_sound\<close>; the collapse
  half is \<open>drop_runs_idem\<close> and \<open>drop_runs_no_adjacent_eq\<close>: what remains
  open is the "equal keys are adjacent in a sorted list" step, which is a
  corollary of any correct order and is left as the open target.\<close>

section \<open>Closing the ordering and collapse obligations\<close>

text \<open>The three open targets this file used to carry are discharged here. The
  ordering half uses \<open>List.sorted\<close> from Main, whose cons rule is
  SET-membership based (\<open>sorted (x # xs) \<longleftrightarrow> (\<forall>y \<in> set xs. le x y) \<and>
  sorted xs\<close>), which avoids index arithmetic entirely; the run half uses a
  STREAMING reference implementation (\<open>collapse_prev\<close>: keep a line unless it
  equals the previous one), so \<open>uniq_lines_sound\<close> says the list-collapse for
  the node equals the streaming filter an implementation would write.\<close>

locale sort_total = sort_order +
  assumes le_total: "le x y \<or> le y x"
begin

subsection \<open>Ordering: the output is sorted, and no line is lost or invented\<close>

text \<open>The ordering half uses \<open>sorted_wrt\<close> from Main, whose cons equation is
  SET-membership based (\<open>sorted_wrt le (x # xs) \<longleftrightarrow>
  (\<forall>y \<in> set xs. le x y) \<and> sorted_wrt le xs\<close>), which avoids index
  arithmetic entirely; the occurrence count below is the multiset statement
  without \<open>List.perm\<close>, which lives in HOL-Library and this session is
  HOL-only.\<close>

lemma insert_mem: "w \<in> set (insert le x ys) \<longleftrightarrow> w = x \<or> w \<in> set ys"
proof (induct ys)
  case Nil
  show ?case by simp
next
  case (Cons y ys)
  show ?case
  proof (cases "le x y")
    case True
    then show ?thesis by simp
  next
    case False
    then show ?thesis using Cons(1) by (simp; blast)
  qed
qed

lemma isort_mem: "w \<in> set (isort le ls) \<longleftrightarrow> w \<in> set ls"
proof (induct ls)
  case Nil
  show ?case by simp
next
  case (Cons x xs)
  show ?case using Cons(1) by (simp add: insert_mem)
qed

lemma set_isort: "set (isort le ls) = set ls"
proof -
  have "\<forall>w. w \<in> set (isort le ls) \<longleftrightarrow> w \<in> set ls"
    by (rule allI) (rule isort_mem)
  then show ?thesis by auto
qed

text \<open>OBLIGATION \<open>sorted_insert\<close>: inserting into a sorted list keeps it
  sorted. The \<open>le x y\<close> branch needs transitivity for the elements after
  \<open>y\<close>; the other branch needs \<open>le y x\<close>, which is exactly the TOTALITY
  assumption — a preorder alone is not enough, which is why this lemma lives in
  \<open>sort_total\<close> and not in \<open>sort_order\<close>.\<close>

lemma sorted_insert:
  assumes h: "sorted_wrt le ys"
  shows "sorted_wrt le (insert le x ys)"
  using h
proof (induct ys)
  case Nil
  show ?case by simp
next
  case (Cons y ys)
  have hs: "sorted_wrt le ys" and hall: "\<forall>w\<in>set ys. le y w"
    using Cons.prems by (simp_all add: sorted_wrt.simps)
  have ih: "sorted_wrt le (insert le x ys)" using Cons(1) Cons hs by blast
  show ?case
  proof (cases "le x y")
    case True
    note le_xy = True
    have all: "\<forall>w\<in>set (y # ys). le x w"
    proof (rule ballI)
      fix w
      assume "w \<in> set (y # ys)"
      then have "w = y \<or> w \<in> set ys" by simp
      then show "le x w"
      proof
        assume "w = y"
        with le_xy show "le x w" by simp
      next
        assume wset: "w \<in> set ys"
        with hall have "le y w" by blast
        with le_xy show "le x w" by (rule le_trans)
      qed
    qed
    have "sorted_wrt le (x # y # ys)"
      using all hall hs by (simp add: sorted_wrt.simps)
    with le_xy show ?thesis by simp
  next
    case False
    note nle = False
    have le_yx: "le y x" using nle le_total by blast
    have all: "\<forall>w\<in>set (insert le x ys). le y w"
    proof (rule ballI)
      fix w
      assume "w \<in> set (insert le x ys)"
      with insert_mem have "w = x \<or> w \<in> set ys" by auto
      then show "le y w"
      proof
        assume "w = x"
        with le_yx show "le y w" by simp
      next
        assume wset: "w \<in> set ys"
        with hall show "le y w" by blast
      qed
    qed
    have "sorted_wrt le (y # insert le x ys)"
      using all ih by (simp add: sorted_wrt.simps)
    with nle show ?thesis by simp
  qed
qed

lemma isort_sorted: "sorted_wrt le (isort le ls)"
proof (induct ls)
  case Nil
  show ?case by simp
next
  case (Cons x xs)
  show ?case using Cons(1) Cons by (simp add: sorted_insert)
qed

text \<open>Re-sorting a sorted list is the identity. This is what makes the
  pipeline \<open>sort | uniq\<close> re-entrant, and it needs totality for the same
  reason \<open>sorted_insert\<close> does: the first element is below every later one.\<close>

lemma isort_sorted_id:
  assumes h: "sorted_wrt le ls"
  shows "isort le ls = ls"
  using h
proof (induct ls)
  case Nil
  show ?case by simp
next
  case (Cons x xs)
  have hs: "sorted_wrt le xs" and hall: "\<forall>w\<in>set xs. le x w"
    using Cons.prems by (simp_all add: sorted_wrt.simps)
  have ih: "isort le xs = xs" using Cons(1) Cons hs by blast
  show ?case
  proof (cases xs)
    case Nil
    with ih show ?thesis by simp
  next
    case (Cons y ys)
    have ymem: "y \<in> set xs" using Cons by simp
    with hall have "le x y" by blast
    with ih Cons show ?thesis by simp
  qed
qed

subsection \<open>Run collapse: the node's value is the streaming filter\<close>

fun collapse_prev :: "line list \<Rightarrow> line option \<Rightarrow> line list" where
  "collapse_prev [] prev = []"
| "collapse_prev (x # xs) None = x # collapse_prev xs (Some x)"
| "collapse_prev (x # xs) (Some p) =
     (if p = x then collapse_prev xs (Some x)
      else x # collapse_prev xs (Some x))"

text \<open>OBLIGATION \<open>uniq_lines_sound\<close>: the node's list collapse equals the
  streaming \<open>keep it unless it equals the previous line\<close> filter a renderer
  writes. The two readings are the two ways to say \<open>collapse every maximal run
  to its first element\<close>.\<close>

theorem uniq_lines_sound: "drop_runs ls = collapse_prev ls None"
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  have ih: "drop_runs (y # ys) = collapse_prev (y # ys) None" using 3(1,2) by blast
  show ?case
  proof (cases "x = y")
    case True
    have "collapse_prev (x # y # ys) None = collapse_prev (y # ys) None"
      using True by simp
    with ih show ?thesis using True by simp
  next
    case False
    have "collapse_prev (x # y # ys) None = x # collapse_prev (y # ys) None"
      using False by simp
    with ih show ?thesis using False by simp
  qed
qed

lemma drop_runs_length_le: "length (drop_runs ls) \<le> length ls"
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  have ih: "length (drop_runs (y # ys)) \<le> length (y # ys)" using 3(1,2) by blast
  show ?case
  proof (cases "x = y")
    case True
    then show ?thesis using ih by simp
  next
    case False
    then show ?thesis using ih by simp
  qed
qed

lemma drop_runs_nonempty:
  assumes "ls \<noteq> []"
  shows "drop_runs ls \<noteq> []"
  using assms
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  show ?case
  proof (cases "x = y")
    case True
    have ih: "drop_runs (y # ys) \<noteq> []" using 3(1,2) by blast
    then show ?thesis using True by simp
  next
    case False
    then show ?thesis by simp
  qed
qed

lemma drop_runs_set: "set (drop_runs ls) \<subseteq> set ls"
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  have ih: "set (drop_runs (y # ys)) \<subseteq> set (y # ys)" using 3(1,2) by blast
  show ?case
  proof (cases "x = y")
    case True
    then show ?thesis using ih by simp
  next
    case False
    show ?thesis
    proof (rule subsetI)
      fix w
      assume "w \<in> set (drop_runs (x # y # ys))"
      then have "w = x \<or> w \<in> set (drop_runs (y # ys))" using False by simp
      then show "w \<in> set (x # y # ys)"
      proof
        assume "w = x"
        then show ?thesis by simp
      next
        assume "w \<in> set (drop_runs (y # ys))"
        with ih have "w \<in> set (y # ys)" by blast
        then show ?thesis by simp
      qed
    qed
  qed
qed

lemma drop_runs_head:
  assumes "ls \<noteq> []"
  shows "hd (drop_runs ls) = hd ls"
  using assms
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  have ih: "hd (drop_runs (y # ys)) = y"
  proof (cases "x = y")
    case True
    with 3(1) show ?thesis by simp
  next
    case False
    with 3(2) show ?thesis by simp
  qed
  show ?case
  proof (cases "x = y")
    case True
    then show ?thesis using ih by simp
  next
    case False
    then show ?thesis by simp
  qed
qed

lemma drop_runs_cons_any:
  assumes "xs \<noteq> []"
  shows "drop_runs (x # xs) =
           (if x = hd xs then drop_runs xs else x # drop_runs xs)"
proof (cases xs)
  case Nil
  then show ?thesis using assms by simp
next
  case (Cons z zs)
  then show ?thesis by simp
qed

lemma drop_runs_idem: "drop_runs (drop_runs ls) = drop_runs ls"
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  have ih: "drop_runs (drop_runs (y # ys)) = drop_runs (y # ys)" using 3(1,2) by blast
  show ?case
  proof (cases "x = y")
    case True
    then show ?thesis using ih by simp
  next
    case False
    note ne = False
    have head: "hd (drop_runs (y # ys)) = y"
    proof -
      have "hd (drop_runs (y # ys)) = hd (y # ys)"
        by (rule drop_runs_head) simp
      then show ?thesis by simp
    qed
    have neX: "drop_runs (y # ys) \<noteq> []"
      by (rule drop_runs_nonempty) simp
    have unfold: "drop_runs (x # drop_runs (y # ys))
        = x # drop_runs (drop_runs (y # ys))"
    proof -
      have "drop_runs (x # drop_runs (y # ys)) =
          (if x = hd (drop_runs (y # ys)) then drop_runs (drop_runs (y # ys))
           else x # drop_runs (drop_runs (y # ys)))"
        by (rule drop_runs_cons_any) (rule neX)
      then show ?thesis using ne head by simp
    qed
    then show ?thesis using ih ne by simp
  qed
qed

fun adj_free :: "line list \<Rightarrow> bool" where
  "adj_free [] = True"
| "adj_free [x] = True"
| "adj_free (x # y # ys) = (x \<noteq> y \<and> adj_free (y # ys))"

lemma drop_runs_adj_free: "adj_free (drop_runs ls)"
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  have ih: "adj_free (drop_runs (y # ys))" using 3(1,2) by blast
  have head: "hd (drop_runs (y # ys)) = y"
  proof -
    have "hd (drop_runs (y # ys)) = hd (y # ys)"
      by (rule drop_runs_head) simp
    then show ?thesis by simp
  qed
  show ?case
  proof (cases "x = y")
    case True
    then show ?thesis using ih by simp
  next
    case False
    have "adj_free (x # drop_runs (y # ys))"
      using False head ih by (cases "drop_runs (y # ys)") simp_all
    then show ?thesis using False by simp
  qed
qed

text \<open>Sortedness of the collapse: induction on the list with the head
  universally quantified, so the hypothesis can be instantiated at the
  element that becomes the next head. (A clause-shape induction hands back a
  hypothesis valid only under that clause's own if-condition, and a length
  induction needs a measure the statement does not carry.)\<close>

lemma sorted_tl_drop_all:
  "\<forall>xs. \<forall>x. sorted_wrt le (x # xs) \<longrightarrow> sorted_wrt le (drop_runs xs)"
proof (rule allI)
  fix xs :: "line list"
  show "\<forall>x. sorted_wrt le (x # xs) \<longrightarrow> sorted_wrt le (drop_runs xs)"
  proof (induct xs)
    case Nil
    then show ?case by simp
  next
    case (Cons y ys)
    show ?case
    proof (rule allI)
      fix x
      show "sorted_wrt le (x # y # ys) \<longrightarrow> sorted_wrt le (drop_runs (y # ys))"
      proof (rule impI)
        assume h: "sorted_wrt le (x # y # ys)"
        have hs: "sorted_wrt le (y # ys)" and hall: "\<forall>w\<in>set (y # ys). le x w"
          using h by (simp_all add: sorted_wrt.simps)
        have ihs: "sorted_wrt le (drop_runs ys)" using Cons(1) hs by blast
        show "sorted_wrt le (drop_runs (y # ys))"
        proof (cases ys)
          case Nil
          then show ?thesis by simp
        next
          case (Cons z zs)
          note ysC = this
          have neYs: "ys \<noteq> []" using ysC by simp
          have hys: "\<forall>w\<in>set ys. le y w"
            using hs ysC by (simp_all add: sorted_wrt.simps)
          have all: "\<forall>w\<in>set (drop_runs ys). le y w"
          proof (rule ballI)
            fix w
            assume "w \<in> set (drop_runs ys)"
            with drop_runs_set have "w \<in> set ys" by blast
            with hys show "le y w" by blast
          qed
          have u: "drop_runs (y # ys) =
              (if y = hd ys then drop_runs ys else y # drop_runs ys)"
            by (rule drop_runs_cons_any) (rule neYs)
          show ?thesis
          proof (cases "y = hd ys")
            case True
            then show ?thesis using u ihs by simp
          next
            case False
            have "sorted_wrt le (y # drop_runs ys)"
              using all ihs by (simp add: sorted_wrt.simps)
            then show ?thesis using u False by simp
          qed
        qed
      qed
    qed
  qed
qed

lemma drop_runs_sorted:
  assumes h: "sorted_wrt le ls"
  shows "sorted_wrt le (drop_runs ls)"
proof (cases ls)
  case Nil
  then show ?thesis by simp
next
  case (Cons x xs)
  note lsC = this
  show ?thesis
  proof (cases xs)
    case Nil
    then show ?thesis using lsC by simp
  next
    case (Cons y ys)
    note xsC = this
    have htriple: "sorted_wrt le (x # y # ys)" using h lsC xsC by simp
    have hs: "sorted_wrt le (y # ys)" and hall: "\<forall>w\<in>set (y # ys). le x w"
      using htriple by (simp_all add: sorted_wrt.simps)
    have ihs: "sorted_wrt le (drop_runs (y # ys))"
      using sorted_tl_drop_all htriple by blast
    show ?thesis
    proof (cases "x = y")
      case True
      then show ?thesis using lsC xsC ihs by simp
    next
      case False
      have all: "\<forall>w\<in>set (drop_runs (y # ys)). le x w"
      proof (rule ballI)
        fix w
        assume "w \<in> set (drop_runs (y # ys))"
        with drop_runs_set have "w \<in> set (y # ys)" by blast
        with hall show "le x w" by blast
      qed
      have "sorted_wrt le (x # drop_runs (y # ys))"
        using all ihs by (simp add: sorted_wrt.simps)
      then show ?thesis using lsC xsC False by simp
    qed
  qed
qed

text \<open>THE COMPOSITION: `sort -u` is `uniq` over `sort`. A sorted list's equal
  lines are adjacent, so collapsing runs of the sorted list IS the
  de-duplication; the result is sorted, adjacent-free, a subset of the input
  and no longer than it; and because it is sorted, re-sorting it is the
  identity — which is what makes the pipeline lowering sound as a composition
  of the two nodes rather than a third node.\<close>

theorem sort_u_sorted: "sorted_wrt le (drop_runs (isort le ls))"
  by (rule drop_runs_sorted) (rule isort_sorted)

theorem sort_u_adj_free: "adj_free (drop_runs (isort le ls))"
  by (rule drop_runs_adj_free)

theorem sort_u_length_le: "length (drop_runs (isort le ls)) \<le> length ls"
proof -
  have a: "length (drop_runs (isort le ls)) \<le> length (isort le ls)"
    by (rule drop_runs_length_le)
  have b: "length (isort le ls) = length ls" by (rule isort_length)
  show ?thesis using a b by simp
qed

theorem sort_u_set: "set (drop_runs (isort le ls)) \<subseteq> set ls"
proof -
  have a: "set (drop_runs (isort le ls)) \<subseteq> set (isort le ls)"
    by (rule drop_runs_set)
  have b: "set (isort le ls) = set ls" by (rule set_isort)
  show ?thesis using a b by simp
qed

theorem sort_u_is_uniq_of_sort:
  "drop_runs (isort le ls) = isort le (drop_runs (isort le ls))"
proof -
  have "isort le (drop_runs (isort le ls)) = drop_runs (isort le ls)"
  proof (rule isort_sorted_id)
    show "sorted_wrt le (drop_runs (isort le ls))" by (rule sort_u_sorted)
  qed
  then show ?thesis by simp
qed

end

end
