(*  TextSemantics — the meaning of `TextFilter` (shir_nodes/text_filter.node).

    `TextFilter` is the node a static text pipeline lowers to:

        echo -e "a\nb\nc" | head -2        ->  TextFilter {src, ops = "head 2"}
        seq 1 10 | uniq                    ->  TextFilter {src, ops = "uniq"}
        echo "x:y" | cut -d: -f2 | tail -1 ->  TextFilter {src, ops = "cut 2 ':' ; tail 1"}

    The node's `spec:` line names ONE obligation, `TextSemantics#filter_chain`:
    a `;`-separated op list is a FUNCTION COMPOSITION, so the op list applied to
    a text is the same text as applying the ops one at a time in order.  That
    is what makes the renderer's left-to-right walk sound.

    The ops are modelled over LISTS OF LINES, because that is what every op in
    the list is a function of (`head`, `tail` and `uniq` never see a byte that
    is not part of a line).  Each op's documented edge case is pinned by an
    executable lemma (`by eval`), and the expected value quoted in the comment
    is the MEASURED bash output on this box — so a model change that stops
    matching bash fails the build rather than the corpus.

    Build:  isabelle build -D sh2perl/docs/fm/TextSemantics
*)

theory ShIR_TextSemantics
  imports Main
begin

type_synonym bytes = "nat list"

definition NL :: nat where "NL = 10"
definition SP :: nat where "SP = 32"

section \<open>Text as lines\<close>

text \<open>`lines s` is the maximal runs between newlines, with the empty tail a
  FINAL newline produces DROPPED — exactly the list `head`, `tail` and `uniq`
  iterate over.  (`printf 'a\n' | head -1` prints `a`, not `a` and an empty
  line.)\<close>

fun lines :: "bytes \<Rightarrow> bytes list" where
  "lines [] = []"
| "lines (c # cs) =
     (if c = NL then [] # lines cs
      else case lines cs of
             [] \<Rightarrow> [[c]]
           | l # ls \<Rightarrow> (c # l) # ls)"

lemma lines_final_newline_dropped: "lines [97, NL] = [[97]]" by eval

lemma lines_no_final_newline: "lines [97] = [[97]]" by eval

lemma lines_empty_line_is_a_line: "lines [NL] = [[]]" by eval

lemma lines_two: "lines [97, NL, 98] = [[97], [98]]" by eval

section \<open>The line-shaped ops\<close>

datatype lop = Head nat | Tail nat | Uniq | Rev

fun uniq_adj :: "bytes list \<Rightarrow> bytes list" where
  "uniq_adj [] = []"
| "uniq_adj [x] = [x]"
| "uniq_adj (x # y # ys) = (if x = y then uniq_adj (x # ys) else x # uniq_adj (y # ys))"

text \<open>`head -N`: the first N lines AS THEY ARE (it never adds a trailing
  newline and never truncates the last emitted line).  `tail -N`: the last N
  lines, GNU treating a final partial line as a line.  `uniq`: collapse
  ADJACENT equal lines only.\<close>

fun apply_lop :: "lop \<Rightarrow> bytes list \<Rightarrow> bytes list" where
  "apply_lop (Head n) ls = take n ls"
| "apply_lop (Tail n) ls = drop (length ls - n) ls"
| "apply_lop Uniq ls = uniq_adj ls"
| "apply_lop Rev ls = rev ls"

text \<open>Measured: `printf 'a\nb\nc\n' | head -2` is `a`, `b`;
  `printf 'a\nb\nc' | tail -2` is `b`, `c`;
  `printf 'a\nb\nb\na\n' | uniq` is `a`, `b`, `a` (adjacent only).\<close>

lemma head_two: "apply_lop (Head 2) (lines [97, NL, 98, NL, 99]) = [[97], [98]]"
  by eval

lemma head_beyond_length: "apply_lop (Head 5) (lines [97, NL, 98]) = [[97], [98]]"
  by eval

lemma tail_keeps_final_partial_line:
  "apply_lop (Tail 2) (lines [97, NL, 98, NL, 99]) = [[98], [99]]"
  by eval

lemma uniq_collapses_adjacent_only:
  "apply_lop Uniq (lines [97, NL, 98, NL, 98, NL, 97]) = [[97], [98], [97]]"
  by eval

text \<open>Collapsing a SECOND time changes nothing — the fact the renderer relies on
  when it folds `sort | uniq` into one op list.  Proved through the auxiliary
  law `idem_aux` (prepending a line to a collapsed list is the same as
  prepending to the raw one), which is the shape the recursion can induct on.\<close>

lemma idem_aux: "uniq_adj (x # uniq_adj xs) = uniq_adj (x # xs)"
  by (induction xs arbitrary: x rule: uniq_adj.induct) simp_all

lemma uniq_adj_idempotent: "uniq_adj (uniq_adj xs) = uniq_adj xs"
proof (induction xs rule: uniq_adj.induct)
  case 1 then show ?case by simp
next
  case (2 u) then show ?case by simp
next
  case (3 u v us)
  show ?case
  proof (cases "u = v")
    case True then show ?thesis using 3 idem_aux by simp
  next
    case False
    have "uniq_adj (u # uniq_adj (v # us)) = u # uniq_adj (v # uniq_adj us)"
      using False idem_aux by simp
    moreover have "u # uniq_adj (v # uniq_adj us) = u # uniq_adj (v # us)"
      using idem_aux by simp
    ultimately show ?thesis using False by simp
  qed
qed

section \<open>The composition law the node promises\<close>

fun apply_chain :: "lop list \<Rightarrow> bytes list \<Rightarrow> bytes list" where
  "apply_chain [] t = t"
| "apply_chain (f # fs) t = apply_chain fs (apply_lop f t)"

text \<open>`filter_chain` — the obligation `text_filter.node` cites.  A
  `;`-separated op list (the node's `ops` field) may be split at any point and
  applied left to right; the renderer's walk and a reader's sequential reading
  compute the same text.  This is what lets the lowering emit ops in source
  order without a second, unchecked notion of "order".\<close>

theorem filter_chain: "apply_chain (xs @ ys) t = apply_chain ys (apply_chain xs t)"
  by (induction xs arbitrary: t) simp_all

corollary filter_chain_single:
  "apply_chain [o1, o2] t = apply_chain [o2] (apply_chain [o1] t)"
  using filter_chain by simp

corollary chain_idempotent_uniq:
  "apply_chain (ops @ [Uniq, Uniq]) t = apply_chain (ops @ [Uniq]) t"
  by (simp add: filter_chain uniq_adj_idempotent)

section \<open>The byte-measuring ops\<close>

text \<open>`wc -l` counts NEWLINES: a final line without one is not counted
  (measured: `printf 'a\nb' | wc -l` is 1, `printf 'a\nb\n' | wc -l` is 2).

  `wc -w` splits on the six POSIX isspace bytes only, so a UTF-8 NBSP (194,160)
  is part of a word — MEASURED DIVERGENCE, recorded rather than hidden: this
  box's `wc` is `wc (uutils coreutils) 0.10.0`, which decodes UTF-8 and splits
  at the NBSP, so `printf 'a\302\240b\n' | wc -w` prints **2** here in both the
  C and the UTF-8 locale, while single-byte 160, 194, control and DEL bytes are
  all non-separators (measured: 1 each).  The model below is the documented
  POSIX/GNU rule the node's renderers implement; a corpus example that pipes a
  multi-byte NBSP through `wc -w` would disagree with this box's wc, and that is
  a note for the corpus, not a licence to model uutils.\<close>

definition is_ws :: "nat \<Rightarrow> bool" where
  "is_ws c \<longleftrightarrow> c \<in> {32, 9, 10, 13, 11, 12}"

definition wc_l :: "bytes \<Rightarrow> nat" where
  "wc_l s = length (filter (\<lambda>c. c = NL) s)"

text \<open>The word count is written as an accumulator over an "inside a word" flag:
  a `dropWhile`-based definition is not structurally recursive (Isabelle 2026
  refuses to find a termination order for it), and the flag makes the recursion
  one byte per step.\<close>

fun wc_w_acc :: "bool \<Rightarrow> bytes \<Rightarrow> nat" where
  "wc_w_acc inw [] = (if inw then 1 else 0)"
| "wc_w_acc inw (c # cs) =
     (if is_ws c then (if inw then 1 + wc_w_acc False cs else wc_w_acc False cs)
      else wc_w_acc True cs)"

abbreviation wc_w :: "bytes \<Rightarrow> nat" where "wc_w s \<equiv> wc_w_acc False s"

definition wc_c :: "bytes \<Rightarrow> nat" where "wc_c s = length s"

lemma wc_l_no_final_newline: "wc_l [97, NL, 98] = 1" by eval

lemma wc_l_final_newline: "wc_l [97, NL, 98, NL] = 2" by eval

lemma wc_w_words: "wc_w [97, SP, 98, NL] = 2" by eval

lemma wc_w_nbsp_is_not_whitespace: "wc_w [97, 194, 160, 98] = 1" by eval

lemma wc_w_single_high_byte_not_whitespace: "wc_w [97, 160, 98] = 1" by eval

lemma wc_c_is_byte_count: "wc_c [97, 194, 160] = 3" by eval

section \<open>`cut -d D -f N`\<close>

fun fields :: "nat \<Rightarrow> bytes \<Rightarrow> bytes list" where
  "fields d [] = [[]]"
| "fields d (c # cs) =
     (if c = d then [] # fields d cs
      else case fields d cs of
             [] \<Rightarrow> [[c]]
           | f # fs \<Rightarrow> (c # f) # fs)"

text \<open>A line with no D is printed UNCHANGED (GNU), fields are 1-based, and a
  field past the end is the empty string — `printf 'x:y\n' | cut -d: -f3`
  prints an empty line, it does not print `x:y`.\<close>

definition cut_field :: "nat \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cut_field d n line =
     (if d \<notin> set line then line
      else if n = 0 then []
      else let fs = fields d line in if n \<le> length fs then fs ! (n - 1) else [])"

lemma cut_first_field: "cut_field 58 1 [120, 58, 121] = [120]" by eval

lemma cut_second_field: "cut_field 58 2 [120, 58, 121] = [121]" by eval

lemma cut_no_delimiter_unchanged: "cut_field 58 2 [110, 111] = [110, 111]"
  by eval

lemma cut_past_the_end_is_empty: "cut_field 58 3 [120, 58, 121] = []" by eval

end
