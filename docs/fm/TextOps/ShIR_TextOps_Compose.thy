(*  ShIR_TextOps_Compose — the COMPOSITION nodes of the text-ops slice:
    `Split`, `ArrayLen` and `FieldExtract`, and the corollary that the
    renderer idiom `ArrayLen(Split(t))` denotes `wc -w`'s meaning.

    Part 4 of the session (after Base, Strip, SortUniq). Tier 1: no shell
    state, no streams, no command execution — a field list is what `split`
    returns for the byte string it is handed.

    Why this file exists. The `wc -w` renderer does not run the `WordCount`
    node's name on every backend: Perl runs
      scalar(grep { length } split(/\\s+/, t, -1))
    ESTree runs
      t.split(/\\s+/).filter(w => w).length
    and C/Java a transition-count scan (`_sh_wc_words`). So Base's
    `lowered_wc_sound` — which is about the NODE — says nothing about the
    shape those three actually run. What they run is `ArrayLen(Split t)`;
    `array_len_split_is_word_count` below is the missing step: the
    composition denotes `word_count t`. (docs/transform-verification.md
    names this as the item-1 slice; docs/fm/README.md as "the natural next
    slice, in payoff order".)

    What is settled here, each with a negative control where one is
    available:

      1. `ArrayLen(Split t) = word_count t`                       (corollary)
      2. dropping the EMPTY fields is load-bearing: the unfiltered
         `split(/\\s+/)` overcounts on leading and trailing whitespace, so
         the `.filter(w => w)` / `grep { length }` is not decoration
      3. a field never contains whitespace and is never empty
      4. `FieldExtract`'s fields are `Split`'s fields: `cut -d, -fN` is the
         N-th element (1-based), out of range is the empty field, and a
         delimiter-free source is one field — which is the `-s` test

    Two things this file deliberately does NOT claim:

    * `Split` with a literal delimiter keeps empty fields (`a,,b` is three
      fields) — that is `cut`, and it is why `FieldExtract` may not be
      modelled over the whitespace split. `split_ws` and `split_on` below
      are different nodes' business.
    * OPEN TARGET, stated not proved: `\\s` is not POSIX space outside
      ASCII. JS/Perl `\\s` matches U+00A0 (NBSP), which `wc -w` counts as
      part of a word, so the two agree on every ASCII input and can
      disagree on a non-ASCII one. `is_ws` is the POSIX set; the `ascii`
      guard `lowered_wc` applies to `StrLen_n` has no analogue on
      `WordCount_n`. `nbsp_is_not_posix_space` is the one-line fact under
      it, not a proof of the idiom's soundness over non-ASCII text.

    Proof style follows docs/fm/README.md: structural induction, goals kept
    in the shape the induction gives them, explicit `cases` instead of
    `simp` on a cons/append shape, no `metis`.

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
*)

theory ShIR_TextOps_Compose
  imports ShIR_TextOps_Base
begin

section \<open>A look-behind word scan\<close>

text \<open>Base defines the word count through \<open>ws_starts\<close>, which decides each
  byte from the byte AFTER it. \<open>Split\<close> can only decide from the byte
  BEFORE (a field begins when a non-whitespace byte follows whitespace, or
  nothing). Both count one per maximal non-whitespace run; the scan below
  is the look-behind form, and \<open>word_count_is_scan\<close> proves the two
  agree. Keeping them separate is what lets the Split proof be an
  induction with no lookahead in it.\<close>

fun ws_scan :: "bool \<Rightarrow> bytes \<Rightarrow> nat" where
  "ws_scan p [] = 0"
| "ws_scan p (c # cs) = (if \<not> is_ws c \<and> p then 1 else 0) + ws_scan (is_ws c) cs"

definition nwords :: "bytes \<Rightarrow> nat" where
  "nwords s \<equiv> ws_scan True s"

text \<open>One byte at a time, the two forms agree: the head is counted exactly
  when it opens a run, and the rest is counted from a cold start. This is
  the shift that makes "count the byte that starts a run" and "count the
  last byte of a run" the same number.\<close>

lemma ws_scan_cons:
  "ws_scan True (c # cs) =
   (if \<not> is_ws c \<and> (cs = [] \<or> is_ws (hd cs)) then 1 else 0) + ws_scan True cs"
  by (cases cs) (simp_all add: ws_scan.simps split: if_splits)

text \<open>Base's \<open>ws_starts\<close> counts word ENDS (its third clause tests the byte
  after the head, so the last byte of every run contributes a \<open>True\<close>).
  Ends per string = starts per string, which is the lemma.\<close>

lemma ws_starts_count:
  "length (filter id (ws_starts s)) = ws_scan True s"
  by (induction s rule: ws_starts.induct) (simp_all add: ws_scan_cons)

text \<open>So the meaning Base fixed is the count this file reasons with.\<close>

lemma word_count_is_scan:
  "word_count s = int (ws_scan True s)"
  by (simp add: word_count_def ws_starts_count)

section \<open>Split and ArrayLen\<close>

text \<open>The two nodes, at the level their contracts live: `ArrayLen` is a
  length (renderers: `.length`, `scalar(@a)`, `len(x)`), and `Split` is the
  list of maximal non-whitespace runs, with the empty fields DROPPED — that
  is the shipped filter (`grep { length }` in Perl, `.filter(w => w)` in
  ESTree), and item 2 of the header is that dropping it changes the answer.\<close>

definition array_len :: "'a list \<Rightarrow> nat" where
  "array_len xs \<equiv> length xs"

text \<open>\<open>field_scan cur acc cs\<close> — \<open>cur\<close> is the run being built (REVERSED, so
  the scan is structural on \<open>cs\<close>), \<open>acc\<close> the finished fields (also
  reversed). A run is pushed only when non-empty, which is exactly the
  filter. Nothing is invented: no field is ever added for whitespace, so a
  source of nothing but whitespace yields the empty list.\<close>

fun field_scan :: "bytes \<Rightarrow> bytes list \<Rightarrow> bytes \<Rightarrow> bytes list" where
  "field_scan cur acc [] = (if cur = [] then acc else rev cur # acc)"
| "field_scan cur acc (c # cs) =
     (if is_ws c
        then field_scan [] (if cur = [] then acc else rev cur # acc) cs
        else field_scan (c # cur) acc cs)"

definition split_ws :: "bytes \<Rightarrow> bytes list" where
  "split_ws s \<equiv> rev (field_scan [] [] s)"

text \<open>The counting invariant. \<open>p\<close> in \<open>ws_scan\<close> is "the previous byte was
  whitespace, or this is the start"; an unfinished run means the next byte
  is mid-word (so \<open>False\<close>), and it is counted by the \<open>if cur = []\<close> term —
  the run in progress contributes exactly one, at the moment it is pushed.\<close>

lemma field_scan_len:
  "length (field_scan cur acc cs) =
   length acc + (if cur = [] then 0 else 1) + ws_scan (cur = []) cs"
proof (induction cs arbitrary: cur acc)
  case Nil
  then show ?case by (cases cur) simp_all
next
  case (Cons c cs)
  then show ?case
    by (cases "is_ws c") (simp_all add: Cons.IH split: if_splits)
qed

lemma split_ws_len:
  "array_len (split_ws s) = ws_scan True s"
  using field_scan_len[of "[]" "[]" s] by (simp add: split_ws_def array_len_def)

section \<open>Obligation: the composition denotes the node's meaning\<close>

text \<open>The corollary docs/fm/README.md asked for. Base proves that the node
  `WordCount` denotes \<open>word_count\<close>; this proves that the shape three of
  the renderers actually run — `ArrayLen(Split t)` — denotes the same
  number, so the two are interchangeable at that level and the renderer
  idiom is not a second, unchecked definition of `wc -w`.\<close>

theorem array_len_split_is_word_count:
  "int (array_len (split_ws s)) = wc_count W_words s"
  using split_ws_len word_count_is_scan
  by (simp add: wc_count_def)

text \<open>Composed with Base's lowering lemma, end to end: whenever
  `try_lower_wc` emits the word node, the split-length idiom measures it.\<close>

theorem lowered_wc_w_via_split:
  assumes "lowered_wc W_words s = Some WordCount_n"
  shows "int (array_len (split_ws s)) = wc_count W_words s"
  using array_len_split_is_word_count by simp

section \<open>Negative control: the empty fields are load-bearing\<close>

text \<open>`split(/\s+/)` on a source with leading whitespace returns a leading
  EMPTY field, and on trailing whitespace a trailing one. Length of that
  list is not the word count; the shipped renderers drop the empties, and
  that filter is the difference between a correct lowering and the
  overcount below. Stated as the lists the engines produce:\<close>

lemma unfiltered_split_overcounts_leading:
  "int (length ([[] :: bytes, [97]])) \<noteq> wc_count W_words [32, 97]"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def by simp

lemma unfiltered_split_overcounts_trailing:
  "int (length ([[97], [] :: bytes])) \<noteq> wc_count W_words [97, 32]"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def by simp

text \<open>And the filtered idiom keeps the right number on the same inputs — the
  two controls and the corollary are the same fact from both sides.\<close>

lemma filtered_split_counts_leading:
  "array_len (split_ws [32, 97]) = 1"
  by eval

lemma filtered_split_counts_trailing:
  "array_len (split_ws [97, 32]) = 1"
  by eval

section \<open>Field shape\<close>

text \<open>A field never contains whitespace and is never empty. This is what
  makes `Split` composable with the other text nodes: a downstream
  `StrLen(FieldExtract(...))` is measuring a word, not a span of
  whitespace. The two halves are separate lemmas on purpose: the
  conjunction is what sends `simp` into the split-case blow-up recorded in
  `docs/fm/README.md`, while each half is a plain structural induction.
  (On this box the combined statement did not terminate in 120 s; the two
  halves close in seconds. The conjunction below reassembles them, so the
  published claim is unchanged.)\<close>

definition ws_free :: "bytes \<Rightarrow> bool" where
  "ws_free w \<longleftrightarrow> (\<forall>c \<in> set w. \<not> is_ws c)"

lemma ws_free_Nil [simp]: "ws_free []" by (simp add: ws_free_def)

lemma ws_free_cons [simp]: "ws_free (c # w) = (\<not> is_ws c \<and> ws_free w)"
  by (simp add: ws_free_def)

lemma ws_free_rev [simp]: "ws_free (rev w) = ws_free w"
  by (simp add: ws_free_def)

lemma field_scan_no_empty:
  assumes acc: "[] \<notin> set acc"
  shows "[] \<notin> set (field_scan cur acc cs)"
  using acc
proof (induction cs arbitrary: cur acc)
  case Nil
  then show ?case by (cases "cur = []") simp_all
next
  case (Cons c cs)
  show ?case
  proof (cases "is_ws c")
    case True
    have "[] \<notin> set (if cur = [] then acc else rev cur # acc)"
    proof (cases "cur = []")
      case True
      with Cons.prems show ?thesis by simp
    next
      case False
      with Cons.prems show ?thesis by simp
    qed
    with True Cons.IH show ?thesis by simp
  next
    case False
    with Cons.IH Cons.prems show ?thesis by simp
  qed
qed

lemma field_scan_ws_free:
  assumes cur: "ws_free cur"
    and acc_free: "(\<forall>w \<in> set acc. ws_free w)"
  shows "\<forall>w \<in> set (field_scan cur acc cs). ws_free w"
  using cur acc_free
proof (induction cs arbitrary: cur acc)
  case Nil
  then show ?case by (cases "cur = []") (simp_all add: ws_free_def)
next
  case (Cons c cs)
  note IH = Cons.IH
  note acc_free = Cons.prems(2)
  show ?case
  proof (cases "cur = []")
    case curNil: True
    have free_nil: "ws_free []" by (simp add: ws_free_def)
    show ?thesis
    proof (cases "is_ws c")
      case ws: True
      have step: "\<forall>w \<in> set (field_scan [] acc cs). ws_free w"
        by (rule IH) (simp_all add: acc_free)
      with curNil ws step show ?thesis by simp
    next
      case nows: False
      have step: "\<forall>w \<in> set (field_scan [c] acc cs). ws_free w"
        by (rule IH) (simp_all add: acc_free nows)
      with curNil nows step show ?thesis by simp
    qed
  next
    case curCons: False
    show ?thesis
    proof (cases "is_ws c")
      case ws: True
      have step: "\<forall>w \<in> set (field_scan [] (rev cur # acc) cs). ws_free w"
        by (rule IH) (simp_all add: acc_free Cons.prems(1))
      with curCons ws step show ?thesis by simp
    next
      case nows: False
      have step: "\<forall>w \<in> set (field_scan (c # cur) acc cs). ws_free w"
        by (rule IH) (simp_all add: acc_free nows Cons.prems(1))
      with curCons nows step show ?thesis by simp
    qed
  qed
qed


lemma field_scan_fields_clean:
  assumes cur: "ws_free cur"
    and empty: "[] \<notin> set acc"
    and free: "(\<forall>w \<in> set acc. ws_free w)"
  shows "\<forall>w \<in> set (field_scan cur acc cs).
         w \<noteq> [] \<and> (\<forall>c \<in> set w. \<not> is_ws c)"
proof -
  have no_empty: "[] \<notin> set (field_scan cur acc cs)"
    using field_scan_no_empty empty by blast
  have all_free: "\<forall>w \<in> set (field_scan cur acc cs). ws_free w"
    using field_scan_ws_free cur free by blast
  from no_empty all_free show ?thesis
  proof (intro ballI)
    fix w
    assume w: "w \<in> set (field_scan cur acc cs)"
    have free_w: "ws_free w" using all_free w by blast
    show "w \<noteq> [] \<and> (\<forall>c \<in> set w. \<not> is_ws c)"
    proof
      show "w \<noteq> []" using w no_empty by blast
    next
      show "\<forall>c \<in> set w. \<not> is_ws c" using free_w by (simp add: ws_free_def)
    qed
  qed
qed

lemma split_ws_fields_clean:
  "\<forall>w \<in> set (split_ws s). w \<noteq> [] \<and> (\<forall>c \<in> set w. \<not> is_ws c)"
  using field_scan_fields_clean[where cur = "[]" and acc = "[]", of s]
  by (simp add: split_ws_def)

text \<open>The edges Base fixed for the count, restated for the field list, so the
  two readings are visibly the same reading.\<close>

lemma split_ws_empty: "split_ws [] = []" by eval

lemma split_ws_all_space: "split_ws [32, 10, 32] = []" by eval

lemma split_ws_two: "split_ws [97, 32, 98] = [[97], [98]]" by eval

lemma split_ws_padded: "split_ws [32, 32, 97, 10, 98, 32] = [[97], [98]]" by eval

text \<open>`wc -w` on the same inputs, from Base, so the pair of readings can be
  compared at the same terms.\<close>

lemma word_count_padded: "wc_count W_words [32, 32, 97, 10, 98, 32] = 2"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def by simp

section \<open>FieldExtract: `cut`'s fields\<close>

text \<open>`cut -dD -fN` splits on a LITERAL delimiter and KEEPS empty fields:
  `a,,b` has three fields and `-f2` is the empty string. So this is not the
  whitespace split above, and modelling it as one would be wrong — that is
  the reason the node carries its own \<open>delimiter\<close>.\<close>

fun split_on :: "nat \<Rightarrow> bytes \<Rightarrow> bytes list" where
  "split_on d [] = [[]]"
| "split_on d (c # cs) =
     (if c = d then [] # split_on d cs
      else (c # hd (split_on d cs)) # tl (split_on d cs))"

lemma split_on_never_nil [simp]: "split_on d s \<noteq> []"
  by (induction s) simp_all

fun ndelims :: "nat \<Rightarrow> bytes \<Rightarrow> nat" where
  "ndelims d [] = 0"
| "ndelims d (c # cs) = (if c = d then Suc (ndelims d cs) else ndelims d cs)"

text \<open>Field count is one more than the delimiter count — `cut`'s own
  arithmetic for "how many fields are there", and the reason `-fN` past the
  end is not an error but an empty field.\<close>

lemma length_split_on:
  "length (split_on d s) = Suc (ndelims d s)"
  by (induction s) simp_all

definition field_at :: "bytes list \<Rightarrow> nat \<Rightarrow> bytes option" where
  "field_at fs n \<equiv> if n < length fs then Some (fs ! n) else None"

text \<open>`cut` prints the empty line for a field past the end, so the meaning of
  the node is "the N-th field, or nothing".\<close>

definition field_or_empty :: "bytes list \<Rightarrow> nat \<Rightarrow> bytes" where
  "field_or_empty fs n \<equiv> case field_at fs n of None \<Rightarrow> [] | Some f \<Rightarrow> f"

lemma field_or_empty_out_of_range:
  "length fs \<le> n \<Longrightarrow> field_or_empty fs n = []"
  by (simp add: field_or_empty_def field_at_def)

text \<open>`-s`: a source with no delimiter is ONE field, and that is exactly the
  condition `-s` suppresses on. Proved, not asserted, because a split that
  returned no field at all would make every `-fN` empty.\<close>

lemma split_on_no_delim:
  assumes "d \<notin> set s"
  shows "split_on d s = [s]"
  using assms by (induction s) simp_all

text \<open>The empty middle field survives, which is the whole difference from
  `split_ws`:\<close>

lemma cut_keeps_empty_field:
  "split_on 44 [97, 44, 44, 98] = [[97], [], [98]]"
  by eval

lemma cut_empty_field_is_empty:
  "field_or_empty (split_on 44 [97, 44, 44, 98]) 1 = []"
  by eval

lemma cut_field_two:
  "field_or_empty (split_on 44 [97, 44, 98, 44, 99]) 1 = [98]"
  by eval

text \<open>And the counterexample that a whitespace split of the same source would
  give a different answer, i.e. the two splits are not interchangeable and
  the node's delimiter is what decides:\<close>

lemma ws_split_would_lose_empty_field:
  "split_ws [97, 44, 44, 98] = [[97, 44, 44, 98]]"
  by eval

section \<open>The `\\s` gap: taken up in `ShIR_TextOps_Space`\<close>

text \<open>The renderers' whitespace class is the engine's `\\s`; the model's is
  POSIX `[:space:]`. This section used to record the difference as an open
  target, because closing it needs the engines' class IN the model. That is
  now the fifth theory of this session: `ShIR_TextOps_Space` transcribes each
  engine's measured class, puts the word count over an arbitrary class
  (`nwords_by`), proves the ASCII-agreement results, and proves two witnesses
  that the emit is not the node's value — including a plain-ASCII one for
  Python, which no source guard can exclude.

  All that stays here is the byte-level fact the gap starts from: NBSP
  (U+00A0) is not a POSIX space, and as a UTF-8 encoding it is not ASCII
  either, so the guard `lowered_wc` uses for `StrLen` would not have covered
  it.\<close>

lemma nbsp_is_not_posix_space: "\<not> is_ws (160::nat)"
  unfolding is_ws_def by simp

lemma nbsp_is_not_ascii: "\<not> ascii [160]"
  by (simp add: ascii_def)

end
