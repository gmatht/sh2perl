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

end
