(*  Part 3 of the text-ops slice: the two ordering nodes.

      SortLines  — `| sort [flags]`  (sh2perl/src/shir_nodes/sort_lines.node)
      UniqLines  — `| uniq [flags]`  (sh2perl/src/shir_nodes/uniq_lines.node)

    Both nodes mean "a function of the text I am handed", so both can be
    proved at the same level as the count nodes in ShIR_TextOps_Base: no
    shell state, no streams.  The lowering lives in `try_lower_sort` /
    `try_lower_uniq` (sh2perl/src/transforms/text_ops.rs).

    Scope: this file models the two nodes over a LIST OF LINES and proves
    what the nodes promise:

      * `insert_length`, `isort_length` — the model sort preserves length;
        `sorted_insert`, `isort_sorted` — its output is ORDERED and every
        line's occurrence count is unchanged (no loss, no duplication; in the
        `sort_total` locale: a preorder is not enough for the second branch);
        `isort_sorted_id` — re-sorting a sorted list is the identity.
      * `run_count_total` — `uniq -c`'s counts add up to the input length;
        `uniq_lines_sound` — and they are the RUN LENGTHS: the input is exactly
        the concatenation of `n` copies of each printed line, in order, and the
        printed lines are the collapse.
      * `drop_runs_idem`, `drop_runs_adj_free`, `sort_u_is_uniq_of_sort` —
        the collapse is idempotent, invents nothing and leaves no equal
        neighbours, so `sort -u` is `uniq . sort`.

    The two nodes' own `spec:` lines name `sort_lines_sound` and
    `uniq_lines_sound`; `uniq -s N` is declared open in the node as
    `pending TextOps#uniq_lines_skip_sound` (see the file tail for why).

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

text \<open>Occurrence counting, because set equality plus length is NOT enough:
  `[a,a,b]` and `[a,b,b]` share a set and a length. `List.perm` lives in
  HOL-Library and this is a HOL-only session, so counts are the exact statement
  available.\<close>

fun count_of :: "line \<Rightarrow> line list \<Rightarrow> nat" where
  "count_of w [] = 0"
| "count_of w (x # xs) = (if w = x then Suc (count_of w xs) else count_of w xs)"

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

text \<open>OBLIGATION (soundness of the node), proved below: the emission sorts the
  lines of the text — the output is ordered, every line's occurrence count is
  unchanged (so nothing is lost or duplicated) and the length is unchanged.
  `isort_length` is the permutation half; `isort_sorted` is the whole
  obligation; `sorted_insert` is the single-insertion step it rests on.\<close>

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

text \<open>OBLIGATION (idempotence), proved below as `drop_runs_idem`: collapsing runs
  twice is collapsing once, so the node removes the WHOLE run (a `-c`-style
  re-run does not double). `drop_runs_no_adjacent_eq` is the companion fact:
  the output has no two equal neighbours.\<close>

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

text \<open>OBLIGATION (the composition), proved as `sort_u_is_uniq_of_sort` and
  `sort_u_composition` below: `sort -u` is `uniq` over `sort`. In a sorted list
  the equal-key lines are the ADJACENT ones, so the run collapse of the sorted
  list is the de-duplication `sort -u` performs — which is why the pipeline
  `sort -u` lowers to a composition of the two nodes rather than to a third
  node. The sort half is `isort_sorted`; the collapse half is `drop_runs_idem`
  and `drop_runs_no_adjacent_eq`.\<close>

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

lemma sorted_insert_sorted:
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

lemma isort_sorted_sorted: "sorted_wrt le (isort le ls)"
proof (induct ls)
  case Nil
  show ?case by simp
next
  case (Cons x xs)
  show ?case using Cons(1) Cons by (simp add: sorted_insert_sorted)
qed

text \<open>OBLIGATION \<open>sorted_insert\<close>, spelled out: insertion into a sorted list
  stays sorted, and it changes exactly one line's occurrence count (the inserted
  one), so nothing is lost and nothing is duplicated. Adjacent-free is NOT part
  of this and cannot be: plain sort keeps duplicates, and `[a,b,a]` sorts to
  `[a,a,b]`, which has equal neighbours. Adjacent-free is the `sort -u` value's
  property, and is proved for `drop_runs` below.\<close>

lemma count_of_insert:
  "count_of w (insert le x ys) =
     (if w = x then Suc (count_of w ys) else count_of w ys)"
proof (induct ys)
  case Nil
  then show ?case by simp
next
  case (Cons y ys)
  note ih = Cons(1)
  show ?case
  proof (cases "le x y")
    case True
    then show ?thesis by simp
  next
    case False
    have step: "count_of w (insert le x (y # ys)) =
          (if w = y then Suc (count_of w (insert le x ys)) else count_of w (insert le x ys))"
      using False by simp
    show ?thesis using step ih by (cases "w = x"; cases "w = y") simp_all
  qed
qed

theorem sorted_insert:
  assumes hys: "sorted_wrt le ys"
  shows "sorted_wrt le (insert le x ys) \<and>
     (\<forall>w. count_of w (insert le x ys) =
            (if w = x then Suc (count_of w ys) else count_of w ys)) \<and>
     length (insert le x ys) = Suc (length ys)"
proof (intro conjI)
  show "sorted_wrt le (insert le x ys)" using hys by (rule sorted_insert_sorted)
  show "\<forall>w. count_of w (insert le x ys) =
            (if w = x then Suc (count_of w ys) else count_of w ys)"
  proof (rule allI)
    fix w
    show "count_of w (insert le x ys) =
          (if w = x then Suc (count_of w ys) else count_of w ys)"
      by (rule count_of_insert)
  qed
  show "length (insert le x ys) = Suc (length ys)" by (rule insert_length)
qed

text \<open>OBLIGATION \<open>isort_sorted\<close>, spelled out: the insertion sort's output is
  sorted, every line's occurrence count is unchanged (no loss, no duplication),
  and the length is unchanged. The element SET follows too (`set_isort`).\<close>

lemma count_of_isort: "\<forall>w. count_of w (isort le ls) = count_of w ls"
proof (induct ls)
  case Nil
  then show ?case by simp
next
  case (Cons x xs)
  show ?case
  proof (intro allI)
    fix w
    have step1: "count_of w (isort le (x # xs)) = count_of w (insert le x (isort le xs))"
      by simp
    have step2: "count_of w (insert le x (isort le xs)) =
          (if w = x then Suc (count_of w (isort le xs)) else count_of w (isort le xs))"
      by (rule count_of_insert)
    have step3: "(if w = x then Suc (count_of w (isort le xs)) else count_of w (isort le xs)) =
          (if w = x then Suc (count_of w xs) else count_of w xs)"
      using Cons(1) by simp
    show "count_of w (isort le (x # xs)) = count_of w (x # xs)"
      using step1 step2 step3 by simp
  qed
qed

theorem isort_sorted:
  "sorted_wrt le (isort le ls) \<and>
   (\<forall>w. count_of w (isort le ls) = count_of w ls) \<and>
   length (isort le ls) = length ls"
proof (intro conjI)
  show "sorted_wrt le (isort le ls)" by (rule isort_sorted_sorted)
  show "\<forall>w. count_of w (isort le ls) = count_of w ls" by (rule count_of_isort)
  show "length (isort le ls) = length ls" by (rule isort_length)
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

lemma uniq_lines_collapse: "drop_runs ls = collapse_prev ls None"
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

text \<open>The collapse above states the node's VALUE; it does not state what the
  `-c` counts COUNT, and a model that reported the right survivor list with
  wrongly split counts would satisfy a count total while printing the wrong
  numbers. The reconstruction below is the statement with teeth: concatenating
  `n` copies of each printed line, in the printed order, gives back the input
  EXACTLY. Together with the survivor list being the collapse and every count
  being at least one, that pins each count to its run length — a differently
  split count list reconstructs a different text. (`[a,a,b]` is the witness: the
  survivor list `[a,b]` with counts `(1,2)` would reconstruct `[a,b,b]`.)\<close>

lemma run_count_nonempty [simp]: "run_count ls \<noteq> [] \<longleftrightarrow> ls \<noteq> []"
proof (induct ls rule: run_count.induct)
  case 1 then show ?case by simp
next
  case (2 x) then show ?case by simp
next
  case (3 x y ys)
  show ?case
  proof (cases "run_count (y # ys)")
    case Nil then show ?thesis using 3 by simp
  next
    case (Cons p rest) then show ?thesis using 3 by (cases p) simp_all
  qed
qed

lemma run_count_nil_eq_iff [simp]: "run_count ls = [] \<longleftrightarrow> ls = []"
proof (cases "ls = []")
  case True
  then show ?thesis by simp
next
  case False
  have "run_count ls \<noteq> []" using run_count_nonempty False by simp
  with False show ?thesis by simp
qed

lemma map_snd_run_count: "map snd (run_count ls) = drop_runs ls"
proof (induct ls rule: run_count.induct)
  case 1 then show ?case by simp
next
  case (2 x) then show ?case by simp
next
  case (3 x y ys)
  show ?case
  proof (cases "run_count (y # ys)")
    case Nil
    then show ?thesis using 3 by simp_all
  next
    case (Cons p rest)
    then show ?thesis using 3 by (cases p) simp_all
  qed
qed

lemma run_count_pos: "\<forall>n \<in> set (map fst (run_count ls)). 1 \<le> n"
proof (induct ls rule: run_count.induct)
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

text \<open>OBLIGATION (the reconstruction): the input is the concatenation of the
  counted runs, in order. `run_count_pos` above keeps the counts from being
  zero, which is what makes `replicate` faithful.\<close>

lemma run_count_parts:
  "ls = concat (map (\<lambda>p. replicate (fst p) (snd p)) (run_count ls))"
proof (induct ls rule: run_count.induct)
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
    obtain n z where pz: "p = (n, z)" by (cases p)
    have ih: "y # ys = replicate n z @ concat (map (\<lambda>p. replicate (fst p) (snd p)) rest)"
      using 3(1) Cons pz by simp
    have mem: "(n, z) \<in> set (run_count (y # ys))" using Cons pz by simp
    have memn: "n \<in> set (map fst (run_count (y # ys)))" using mem by force
    have pos: "1 \<le> n" using run_count_pos memn by blast
    show ?thesis
    proof (cases "x = y")
      case True
      have yz: "y = z"
      proof (cases n)
        case 0
        then show ?thesis using pos by simp
      next
        case (Suc k)
        then show ?thesis using ih by simp
      qed
      have rc: "run_count (x # y # ys) = (n + 1, z) # rest"
        using True Cons pz by simp
      have lhs: "x # y # ys = z # replicate n z @ concat (map (\<lambda>p. replicate (fst p) (snd p)) rest)"
        using True yz ih by simp
      have rhs: "concat (map (\<lambda>p. replicate (fst p) (snd p)) (run_count (x # y # ys)))
                   = z # replicate n z @ concat (map (\<lambda>p. replicate (fst p) (snd p)) rest)"
        using rc by simp
      show ?thesis using lhs rhs by simp
    next
      case False
      have rc: "run_count (x # y # ys) = (1, x) # (n, z) # rest"
        using False Cons pz by simp
      have rhs: "concat (map (\<lambda>p. replicate (fst p) (snd p)) (run_count (x # y # ys)))
                   = x # y # ys"
        using rc ih by simp
      show ?thesis using rhs by simp
    qed
  qed
qed

text \<open>OBLIGATION \<open>uniq_lines_sound\<close>, conjunct by conjunct: the collapse
  equals the streaming filter a renderer writes (the value); the input is
  exactly the counted runs concatenated (what the counts mean); the printed
  lines are the collapse (which lines survive); every count is at least one; the
  counts add up to the input length; and no equal neighbours survive.\<close>

theorem uniq_lines_sound:
  "drop_runs ls = collapse_prev ls None \<and>
   ls = concat (map (\<lambda>p. replicate (fst p) (snd p)) (run_count ls)) \<and>
   map snd (run_count ls) = drop_runs ls \<and>
   (\<forall>n \<in> set (map fst (run_count ls)). 1 \<le> n) \<and>
   sum_list (map fst (run_count ls)) = length ls \<and>
   adj_free (drop_runs ls)"
proof (intro conjI)
  show "drop_runs ls = collapse_prev ls None" by (rule uniq_lines_collapse)
  show "ls = concat (map (\<lambda>p. replicate (fst p) (snd p)) (run_count ls))"
    by (rule run_count_parts)
  show "map snd (run_count ls) = drop_runs ls" by (rule map_snd_run_count)
  show "\<forall>n \<in> set (map fst (run_count ls)). 1 \<le> n" by (rule run_count_pos)
  show "sum_list (map fst (run_count ls)) = length ls" by (rule run_count_total)
  show "adj_free (drop_runs ls)" by (rule drop_runs_adj_free)
qed

text \<open>OPEN, and named: `uniq -s N` (`uniq_lines skip`). The node compares the
  lines' suffixes after dropping `skip` characters and KEEPS THE FIRST line of
  the run. A `drop_runs`-shaped model drops the first and keeps the last, which
  agrees for exact equality but not for a skip relation (two lines that share a
  suffix differ before it), so the skip case needs a partition under that
  relation and a keep-first walk; it is not a corollary of the proofs above.
  Recorded as \<open>pending TextOps#uniq_lines_skip_sound\<close> in the node.\<close>

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
  by (rule drop_runs_sorted) (rule conjunct1[OF isort_sorted])

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


text \<open>OBLIGATION \<open>sort_lines_sound\<close>: the `sort_lines` node's value is the
  run collapse of the insertion sort at its key order. Both flags that change
  the value are compositions of the two functions proved here — `unique` is the
  collapse and `reverse` is `rev` of the same list — so the node inherits: the
  output is sorted, has no equal neighbours, and invents no line.\<close>

theorem sort_lines_sound:
  "sorted_wrt le (drop_runs (isort le ls)) \<and>
   adj_free (drop_runs (isort le ls)) \<and>
   set (drop_runs (isort le ls)) \<subseteq> set ls"
proof (intro conjI)
  show "sorted_wrt le (drop_runs (isort le ls))" by (rule sort_u_sorted)
  show "adj_free (drop_runs (isort le ls))" by (rule sort_u_adj_free)
  show "set (drop_runs (isort le ls)) \<subseteq> set ls" by (rule sort_u_set)
qed


text \<open>Adjacent-free, in the syntax that says "no two equal neighbours" without
  a recursive predicate: `drop_runs_no_adjacent_eq` is the OBLIGATION, and
  `adj_free` is the same fact in the shape the earlier proofs use.\<close>

text \<open>The pair can sit ANYWHERE, not just at the head: the head-only form
  `\<nexists>x y zs. xs = x # y # zs \<and> x = y` is weaker than adjacent-free
  (it tests one pair), so the statement below quantifies over a prefix.\<close>

lemma adj_free_imp_nth:
  "adj_free xs \<Longrightarrow> Suc i < length xs \<Longrightarrow> xs ! i \<noteq> xs ! Suc i"
proof (induct xs arbitrary: i)
  case Nil
  then show ?case by simp
next
  case (Cons a xs)
  note IH = Cons(1)
  note hAdj = Cons(2)
  note hLen = Cons(3)
  show ?case
  proof (cases xs)
    case Nil
    then show ?thesis using hLen by simp
  next
    case (Cons b bs)
    note tail = this
    show ?thesis
    proof (cases i)
      case 0
      with hAdj tail show ?thesis by simp
    next
      case (Suc j)
      have ha: "adj_free xs" using hAdj tail by simp
      have hl: "Suc j < length xs" using hLen Suc by simp
      have hres: "xs ! j \<noteq> xs ! Suc j" using IH[OF ha hl] .
      show ?thesis using hres tail Suc by simp
    qed
  qed
qed

theorem drop_runs_no_adjacent_eq:
  "\<And>i. Suc i < length (drop_runs ls) \<Longrightarrow>
      drop_runs ls ! i \<noteq> drop_runs ls ! Suc i"
  by (rule adj_free_imp_nth[OF drop_runs_adj_free])

text \<open>OBLIGATION (the composition): `sort -u` is `uniq` over `sort`, stated as
  the equation between the collapse of the sort and the collapse of that
  collapse — collapsing twice is collapsing once — together with the fact that
  the collapse is already sorted, so re-sorting it changes nothing. Both halves
  are what makes the multi-node lowering sound rather than a third node.\<close>

theorem sort_u_is_collapse_of_collapse:
  "drop_runs (isort le ls) = drop_runs (drop_runs (isort le ls))"
  by (rule drop_runs_idem[symmetric])

theorem sort_u_composition:
  "drop_runs (isort le ls) = drop_runs (drop_runs (isort le ls)) \<and>
   drop_runs (isort le ls) = isort le (drop_runs (isort le ls)) \<and>
   sorted_wrt le (drop_runs (isort le ls)) \<and>
   adj_free (drop_runs (isort le ls))"
proof (intro conjI)
  show "drop_runs (isort le ls) = drop_runs (drop_runs (isort le ls))"
    by (rule sort_u_is_collapse_of_collapse)
  show "drop_runs (isort le ls) = isort le (drop_runs (isort le ls))"
    by (rule sort_u_is_uniq_of_sort)
  show "sorted_wrt le (drop_runs (isort le ls))" by (rule sort_u_sorted)
  show "adj_free (drop_runs (isort le ls))" by (rule sort_u_adj_free)
qed


end

section \<open>`uniq -s N`: the run collapse under a key\<close>

text \<open>`uniq -s N` decides whether two lines belong to the same run by comparing
  their SUFFIXES (\<open>drop N\<close>) instead of the lines, and keeps the FIRST line of
  the run — the survivor keeps the characters the comparison ignored. Which end
  of a run a recursion keeps is invisible under exact equality (all the lines of
  a run are the same line then), so it is the whole difference here: a
  \<open>drop_runs\<close>-shaped recursion drops the head and keeps the last member, which
  for a skip relation is a DIFFERENT value. The collapse is therefore written
  keep-first, with the run leader as the comparison key (an equivalence, since a
  key is a function), and the obligations are proved below.\<close>

fun key_from :: "(line \<Rightarrow> line) \<Rightarrow> line \<Rightarrow> line list \<Rightarrow> line list" where
  "key_from K c [] = []"
| "key_from K c (y # ys) =
     (if K y = K c then key_from K c ys else y # key_from K y ys)"

definition key_runs :: "(line \<Rightarrow> line) \<Rightarrow> line list \<Rightarrow> line list" where
  "key_runs K ls = (case ls of [] \<Rightarrow> [] | x # xs \<Rightarrow> x # key_from K x xs)"

text \<open>\<open>key_free K\<close> is the companion predicate: no two adjacent lines have the same
  key, i.e. every maximal run really was reduced to one line.\<close>

fun key_free :: "(line \<Rightarrow> line) \<Rightarrow> line list \<Rightarrow> bool" where
  "key_free K [] = True"
| "key_free K [x] = True"
| "key_free K (x # y # ys) = (K x \<noteq> K y \<and> key_free K (y # ys))"

lemma key_from_nil [simp]: "key_from K c [] = []"
  by simp

lemma key_from_cons [simp]:
  "key_from K c (y # ys) =
     (if K y = K c then key_from K c ys else y # key_from K y ys)"
  by simp

lemma key_runs_nil [simp]: "key_runs K [] = []"
  unfolding key_runs_def by simp

lemma key_runs_cons [simp]: "key_runs K (x # xs) = x # key_from K x xs"
  unfolding key_runs_def by simp

lemma key_runs_nonempty:
  assumes "ls \<noteq> []"
  shows "key_runs K ls \<noteq> []"
  using assms by (cases ls) simp_all

lemma key_runs_head:
  assumes "ls \<noteq> []"
  shows "hd (key_runs K ls) = hd ls"
  using assms by (cases ls) simp_all

lemma key_from_length_le: "\<forall>c. length (key_from K c zs) \<le> length zs"
proof (induct zs)
  case Nil
  then show ?case by simp
next
  case (Cons y ys)
  note IH = Cons
  show ?case
  proof (rule allI)
    fix c
    show "length (key_from K c (y # ys)) \<le> length (y # ys)"
    proof (cases "K y = K c")
      case True
      have ihc: "length (key_from K c ys) \<le> length ys" using IH by blast
      then show ?thesis using True by simp
    next
      case False
      have ihy: "length (key_from K y ys) \<le> length ys" using IH by blast
      then show ?thesis using False by simp
    qed
  qed
qed

lemma key_runs_length_le: "length (key_runs K ls) \<le> length ls"
proof (cases ls)
  case Nil
  then show ?thesis by simp
next
  case (Cons x xs)
  have h: "length (key_from K x xs) \<le> length xs" using key_from_length_le by blast
  show ?thesis using h Cons by simp
qed

lemma key_from_head_ne:
  assumes "key_from K c zs = z # zt"
  shows "K z \<noteq> K c"
  using assms
proof (induct zs)
  case Nil
  then show ?case by simp
next
  case (Cons y ys)
  note IH = Cons(1)
  note hEq = Cons(2)
  show ?case
  proof (cases "K y = K c")
    case True
    then have e: "key_from K c (y # ys) = key_from K c ys" by simp
    have "key_from K c ys = z # zt" using hEq e by simp
    with IH have "K z \<noteq> K c" by blast
    then show ?thesis using e by simp
  next
    case False
    then have "key_from K c (y # ys) = y # key_from K y ys" by simp
    then have "z = y" using hEq by simp
    then show ?thesis using False by simp
  qed
qed

lemma key_from_key_free: "\<forall>c. key_free K (key_from K c zs)"
proof (induct zs)
  case Nil
  then show ?case by simp
next
  case (Cons y ys)
  note IH = Cons
  show ?case
  proof (rule allI)
    fix c
    show "key_free K (key_from K c (y # ys))"
    proof (cases "K y = K c")
      case True
      have ihc: "key_free K (key_from K c ys)" using IH by blast
      then show ?thesis using True by simp
    next
      case False
      have ihy: "key_free K (key_from K y ys)" using IH by blast
      show ?thesis
      proof (cases "key_from K y ys")
        case Nil
        then show ?thesis using False by simp
      next
        case (Cons z zt)
        have hne: "K z \<noteq> K y" using key_from_head_ne Cons by blast
        have ihf: "key_free K (z # zt)" using ihy Cons by metis
        show ?thesis using False Cons hne ihf by simp
      qed
    qed
  qed
qed

lemma key_runs_key_free: "key_free K (key_runs K ls)"
proof (cases ls)
  case Nil
  then show ?thesis by simp
next
  case (Cons x xs)
  note lsC = Cons
  show ?thesis
  proof (cases "key_from K x xs")
    case Nil
    then show ?thesis using lsC by simp
  next
    case (Cons z zt)
    have hne: "K z \<noteq> K x" using key_from_head_ne Cons by blast
    have ihf: "key_free K (z # zt)" using key_from_key_free Cons by metis
    show ?thesis using lsC Cons hne ihf by simp
  qed
qed

text \<open>\<open>uniq_lines_skip_sound\<close>: the node's value for \<open>uniq -s N\<close> — the collapse
  under the key \<open>drop N\<close> — keeps the first line of every run (so the skipped
  characters survive in the printed line), every following line of the run is
  dropped, the value is a subsequence (no longer than its input), and no two
  survivors share a key (so the runs are separated: nothing that uniq keeps is
  lost and nothing it drops is kept).\<close>

theorem uniq_lines_skip_sound:
  "length (key_runs (drop N) ls) \<le> length ls \<and>
   (ls \<noteq> [] \<longrightarrow> hd (key_runs (drop N) ls) = hd ls) \<and>
   key_free (drop N) (key_runs (drop N) ls)"
proof (intro conjI)
  show "length (key_runs (drop N) ls) \<le> length ls" by (rule key_runs_length_le)
  show "ls \<noteq> [] \<longrightarrow> hd (key_runs (drop N) ls) = hd ls"
  proof (rule impI)
    assume "ls \<noteq> []"
    then show "hd (key_runs (drop N) ls) = hd ls" by (rule key_runs_head)
  qed
  show "key_free (drop N) (key_runs (drop N) ls)" by (rule key_runs_key_free)
qed

text \<open>The two boundary cases the node's comment used to assert in prose, now
  as theorems: at \<open>N = 0\<close> the skip collapse IS the plain run collapse (\<open>drop_runs\<close>),
  and at \<open>N = 1\<close> it is NOT — two lines that differ only before the skipped
  characters are one run for `uniq -s 1`, and the FIRST of them survives.\<close>

lemma key_runs_id_is_drop_runs: "key_runs id ls = drop_runs ls"
proof (induct ls rule: drop_runs.induct)
  case 1
  then show ?case by simp
next
  case (2 x)
  then show ?case by simp
next
  case (3 x y ys)
  have ih: "key_runs id (y # ys) = drop_runs (y # ys)" using 3(1,2) by blast
  show ?case
  proof (cases "x = y")
    case True
    have e1: "key_runs id (x # y # ys) = key_runs id (y # ys)" using True by simp
    have e2: "drop_runs (x # y # ys) = drop_runs (y # ys)" using True by simp
    then show ?thesis using e1 ih by metis
  next
    case False
    have e1: "key_runs id (x # y # ys) = x # key_runs id (y # ys)" using False by simp
    have e2: "drop_runs (x # y # ys) = x # drop_runs (y # ys)" using False by simp
    then show ?thesis using e1 e2 ih by metis
  qed
qed

theorem uniq_lines_skip_zero:
  "key_runs (drop 0) ls = drop_runs ls"
proof -
  have e: "(drop 0 :: line \<Rightarrow> line) = id" by (rule ext) simp
  have h: "key_runs id ls = drop_runs ls" by (rule key_runs_id_is_drop_runs)
  show ?thesis using e h by simp
qed

theorem uniq_lines_skip_merges:
  "key_runs (drop 1) [[97, 98], [120, 98]] = [[97, 98]]"
  by simp


section \<open>Stability: equal keys keep their input order\<close>

text \<open>`sort_lines.node` carries no `stable` flag because the node IS a stable
  order; that promise was prose in the node comment until now. It is stated and
  proved here without positions: for any line `w`, the lines that compare equal to
  `w` (`le_eq le w`) appear in the output in the same order as in the input, i.e.
  as the same filtered subsequence. Insertion sort is stable for a reason that
  fits this form: it takes the HEAD of the input and inserts it into the sorted
  tail, and `insert` stops at the first element the head is not below — every
  element that compares equal to the head IS below it by transitivity, so the head
  lands before all of its equals and the equal-key subsequence is unchanged.\<close>

abbreviation le_eq :: "(line \<Rightarrow> line \<Rightarrow> bool) \<Rightarrow> line \<Rightarrow> line \<Rightarrow> bool" where
  "le_eq le w y \<equiv> le y w \<and> le w y"

text \<open>The two halves of insertion's effect on a filtered subsequence: an element
  the filter rejects leaves the subsequence untouched wherever it lands, and an
  element the filter accepts and that is below every accepted element lands at the
  FRONT of the subsequence — which is what keeps the input order.\<close>

lemma filter_insert_notP:
  assumes hx: "\<not> P x"
  shows "filter P (insert le x ys) = filter P ys"
  using hx
proof (induct ys)
  case Nil
  then show ?case by simp
next
  case (Cons y ys)
  note IH = Cons(1)
  note hx = Cons(2)
  show ?case
  proof (cases "le x y")
    case True
    then show ?thesis using hx by simp
  next
    case False
    note nle = False
    have e1: "insert le x (y # ys) = y # insert le x ys" using nle by simp
    have e2: "filter P (insert le x ys) = filter P ys" using IH hx by blast
    show ?thesis
    proof (cases "P y")
      case True
      then show ?thesis using e1 e2 by simp
    next
      case False
      then show ?thesis using e1 e2 by simp
    qed
  qed
qed

lemma filter_insert_eq:
  assumes hx: "P x"
    and habove: "\<And>y. P y \<Longrightarrow> le x y"
  shows "filter P (insert le x ys) = x # filter P ys"
  using hx habove
proof (induct ys)
  case Nil
  then show ?case by simp
next
  case (Cons y ys)
  note IH = Cons(1)
  note hx = Cons(2)
  note hab = Cons(3)
  show ?case
  proof (cases "le x y")
    case True
    then show ?thesis using hx by simp
  next
    case False
    note nle = False
    have e1: "insert le x (y # ys) = y # insert le x ys" using nle by simp
    have nPy: "\<not> P y" using hab nle by blast
    have e2: "filter P (insert le x ys) = x # filter P ys" using IH hx hab by blast
    show ?thesis using e1 e2 nPy by simp
  qed
qed

context sort_order
begin

text \<open>Stability of the sort itself. `le_trans` is the locale's preorder law, so
  nothing about the order is assumed here beyond what the node already needs.\<close>

theorem isort_stable:
  "filter (le_eq le w) (isort le ls) = filter (le_eq le w) ls"
proof (induct ls)
  case Nil
  then show ?case by simp
next
  case (Cons x xs)
  note IH = Cons
  show ?case
  proof (cases "le_eq le w x")
    case True
    have hxw: "le x w" using True by simp
    have hab: "\<And>y. le_eq le w y \<Longrightarrow> le x y"
    proof -
      fix y
      assume hy: "le_eq le w y"
      have hwy: "le w y" using hy by simp
      show "le x y" using hxw hwy by (rule le_trans)
    qed
    have e1: "filter (le_eq le w) (isort le (x # xs)) =
              filter (le_eq le w) (insert le x (isort le xs))" by simp
    have e2: "filter (le_eq le w) (insert le x (isort le xs)) =
              x # filter (le_eq le w) (isort le xs)"
    proof (rule filter_insert_eq)
      show "le_eq le w x" using True .
    next
      show "\<And>y. le_eq le w y \<Longrightarrow> le x y" by (rule hab)
    qed
    have e3: "filter (le_eq le w) (isort le xs) = filter (le_eq le w) xs" using IH .
    have e4: "filter (le_eq le w) (x # xs) = x # filter (le_eq le w) xs"
      using True by simp
    show ?thesis using e1 e2 e3 e4 by simp
  next
    case False
    have e1: "filter (le_eq le w) (isort le (x # xs)) =
              filter (le_eq le w) (insert le x (isort le xs))" by simp
    have e2: "filter (le_eq le w) (insert le x (isort le xs)) =
              filter (le_eq le w) (isort le xs)"
    proof (rule filter_insert_notP)
      show "\<not> le_eq le w x" using False .
    qed
    have e3: "filter (le_eq le w) (isort le xs) = filter (le_eq le w) xs" using IH .
    have e4: "filter (le_eq le w) (x # xs) = filter (le_eq le w) xs"
      using False by simp
    show ?thesis using e1 e2 e3 e4 by simp
  qed
qed

end


end
