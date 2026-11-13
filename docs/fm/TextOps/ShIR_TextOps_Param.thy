(*  ShIR_TextOps_Param — the PARAMETER-EXPANSION shapes and the field
    selection of `cut`, as algebra rather than as prose.

    Fourteenth theory of the session (after Base, Strip, SortUniq, Compose,
    Space, Strings, Cut, Field, Cutset, Substitute, Splice, Device, Glob).

    Why this theory exists. Three areas of this slice were justified by prose
    only:

      * the default / alternate / assign shapes `${v:-w}`, `${v-w}`,
        `${v:=w}`, `${v=w}`, `${v:+w}`, `${v+w}` and `${#v}` — the transforms
        argue about them (`default_empty_fold` says `${v:-}` IS `${v}`,
        `copy_propagation` folds `:-` on a known value) but no theory said
        what the shapes denote, so "set but empty" and "unset" could be
        conflated without anything noticing;
      * bash's NEGATIVE slice offsets and lengths (`${v: -3}`, `${v:0:-2}`),
        which `SubStrExtract` explicitly left outside its model;
      * `FieldExtract`'s `vec_field_range` and `output_delimiter` — the last
        entry of the README's open-target list: `-fN-M`, `-f1,3`,
        `--complement` and `-o` splicing carried no obligation at all.

    The point of the first section is that "unset" and "set to the empty
    string" are DIFFERENT values, and that the colon shapes are exactly the
    ones that confuse them (`:-` tests nullness, `-` tests presence). That is
    proved as a table of laws plus a refutation of the conflation, so the
    distinction the renderers must keep is a theorem, not a convention.

    The field section states what `cut` means: the SELECTED fields are a
    sublist of the input's field list in INPUT order (which is why `-f3,1`
    prints fields 1 then 3, not 3 then 1), `--complement` is the missing
    indices with the lengths adding up exactly, and the output delimiter is
    inserted BETWEEN fields so a single selected field gets no delimiter at
    all. `intercalate d (split_on d t) = t` is the other direction: the field
    list of `Split` carries every byte of the text, in order — the per-field
    byte identity `split.node` recorded as missing.

    Out of model, and stated so: IFS as a SET of characters (this session
    models one delimiter / the whitespace class of ShIR_TextOps_Space),
    quoting inside the word (`"${v:-a b}"` does not split), locale-dependent
    classifications, and `${v:?w}` / `${v@Q}`-style operators. Nothing here
    is left unproved-by-omission; everything outside is an explicit boundary.
*)

theory ShIR_TextOps_Param
  imports ShIR_TextOps_Glob
begin

section \<open>Presence: unset and set-but-empty are different values\<close>

text \<open>`presence` is what a parameter expansion reads: `Unset` is a name with
  no value, `Present b` a value that may be the empty string. The two are
  distinct constructors precisely because `${v-w}` and `${v:-w}` disagree on
  `Present []`.\<close>

datatype presence = Unset | Present bytes

fun pval :: "presence \<Rightarrow> bytes" where
  "pval Unset = []"
| "pval (Present b) = b"

fun pnull :: "presence \<Rightarrow> bool" where
  "pnull Unset = True"
| "pnull (Present b) = (b = [])"

text \<open>The colon shapes test NULLNESS (`Unset` or empty), the plain shapes test
  PRESENCE (`Unset` only). That is the whole difference between `:-` and `-`.\<close>

definition def_colon :: "bytes \<Rightarrow> presence \<Rightarrow> bytes" where
  "def_colon w p = (if pnull p then w else pval p)"

definition def_plain :: "bytes \<Rightarrow> presence \<Rightarrow> bytes" where
  "def_plain w p = (case p of Unset \<Rightarrow> w | Present b \<Rightarrow> b)"

definition alt_colon :: "bytes \<Rightarrow> presence \<Rightarrow> bytes" where
  "alt_colon w p = (if pnull p then [] else w)"

definition alt_plain :: "bytes \<Rightarrow> presence \<Rightarrow> bytes" where
  "alt_plain w p = (case p of Unset \<Rightarrow> [] | Present _ \<Rightarrow> w)"

text \<open>`${v:=w}` returns the same VALUE as `${v:-w}` and additionally leaves the
  name non-null, which is why a second `:=` with the same word changes
  nothing (`assign_colon` is the post-state, `def_colon_idempotent` the law).\<close>

definition assign_colon :: "bytes \<Rightarrow> presence \<Rightarrow> presence" where
  "assign_colon w p = Present (def_colon w p)"

definition plen :: "presence \<Rightarrow> nat" where
  "plen p = length (pval p)"

lemma def_colon_value [simp]:
  "\<not> pnull p \<Longrightarrow> def_colon w p = pval p"
  by (simp add: def_colon_def)

lemma def_colon_word [simp]:
  "pnull p \<Longrightarrow> def_colon w p = w"
  by (simp add: def_colon_def)

text \<open>`${v:-}` with an EMPTY word is the value itself — the law the
  `default-empty-fold` transform relies on when it retires the expansion.\<close>

lemma def_colon_empty_word:
  "def_colon [] p = pval p"
  by (cases p) (simp_all add: def_colon_def)

lemma def_colon_nonnull:
  "w \<noteq> [] \<Longrightarrow> \<not> pnull (Present (def_colon w p))"
  by (cases p) (simp_all add: def_colon_def)

lemma def_colon_idempotent:
  "w \<noteq> [] \<Longrightarrow> def_colon w (assign_colon w p) = def_colon w p"
  using def_colon_nonnull by (simp add: assign_colon_def)

lemma assign_colon_value:
  "def_colon w p = pval (assign_colon w p)"
  by (simp add: assign_colon_def)

text \<open>The colon pair is a partition of the two cases: on a non-null value the
  default shape yields the value and the alternate shape yields the word; on a
  null value the default yields the word and the alternate yields nothing.\<close>

lemma colon_pair_on_value:
  "\<not> pnull p \<Longrightarrow> def_colon w p = pval p \<and> alt_colon w p = w"
  by (simp add: alt_colon_def)

lemma colon_pair_on_null:
  "pnull p \<Longrightarrow> def_colon w p = w \<and> alt_colon w p = []"
  by (simp add: alt_colon_def)

text \<open>REFUTED: "a parameter is either set or empty, so `-` and `:-` agree".
  At a set-but-empty value they do not: `:-` takes the word, `-` returns the
  empty value. This is the conflation the renderers must never make.\<close>

theorem colon_minus_is_not_plain:
  assumes nw: "w \<noteq> []"
  shows "def_colon w (Present []) \<noteq> def_plain w (Present [])"
  using nw by (simp add: def_colon_def def_plain_def)

theorem plus_plain_is_not_plus_colon:
  assumes nw: "w \<noteq> []"
  shows "alt_plain w (Present []) \<noteq> alt_colon w (Present [])"
  using nw by (simp add: alt_plain_def alt_colon_def)

text \<open>`${#v}` is zero exactly on a null parameter, which is why a length test
  cannot distinguish unset from empty either — the same distinction, seen from
  the other side.\<close>

lemma plen_unset [simp]: "plen Unset = 0"
  by (simp add: plen_def)

lemma plen_present [simp]: "plen (Present b) = length b"
  by (simp add: plen_def)

lemma plen_zero_iff_null: "plen p = 0 \<longleftrightarrow> pnull p"
  by (cases p) (simp_all add: plen_def)

text \<open>Non-vacuity, at concrete bytes: each shape really moves.\<close>

lemma param_witness:
  "pnull Unset \<and> pnull (Present []) \<and> \<not> pnull (Present [97]) \<and>
   def_colon [98] Unset = [98] \<and>
   def_colon [98] (Present []) = [98] \<and>
   def_colon [98] (Present [97]) = [97] \<and>
   def_plain [98] (Present []) = [] \<and>
   def_plain [98] Unset = [98] \<and>
   alt_colon [98] (Present [97]) = [98] \<and>
   alt_colon [98] (Present []) = [] \<and>
   alt_plain [98] (Present []) = [98] \<and>
   plen (Present [97, 98]) = 2"
  by (simp add: def_colon_def def_plain_def alt_colon_def alt_plain_def plen_def)

section \<open>Slicing: `${v:off}` and `${v:off:len}` with bash's negative forms\<close>

text \<open>bash counts a negative offset from the END and a negative length from the
  END as well, then clamps: `-n` with `n` past the length means the whole
  string, and the length is clamped to what is left. `start_of` is the offset
  (a `bool` says "counted from the end"), `stop_of` the exclusive end used by
  the two-index form, `slice_from` the one-index form.\<close>

fun start_of :: "bytes \<Rightarrow> bool \<Rightarrow> nat \<Rightarrow> nat" where
  "start_of b False off = min off (length b)"
| "start_of b True off = length b - min off (length b)"

definition stop_of :: "bytes \<Rightarrow> nat \<Rightarrow> bool \<Rightarrow> nat \<Rightarrow> nat" where
  "stop_of b s lneg len =
     (if lneg then max s (length b - min len (length b))
      else min (length b) (s + len))"

definition slice_from :: "bytes \<Rightarrow> bool \<Rightarrow> nat \<Rightarrow> bytes" where
  "slice_from b sneg off = drop (start_of b sneg off) b"

definition slice :: "bytes \<Rightarrow> bool \<Rightarrow> nat \<Rightarrow> bool \<Rightarrow> nat \<Rightarrow> bytes" where
  "slice b sneg off lneg len =
     (let s = start_of b sneg off; e = stop_of b s lneg len
      in take (e - s) (drop s b))"

text \<open>The NON-NEGATIVE forms are exactly `SubStrExtract`'s meaning —
  `take length (drop offset text)` — so the negative laws extend a proven model
  rather than replacing it.\<close>

lemma slice_from_nonneg [simp]:
  "slice_from b False off = drop (min off (length b)) b"
  by (simp add: slice_from_def)

lemma slice_from_neg [simp]:
  "slice_from b True off = drop (length b - min off (length b)) b"
  by (simp add: slice_from_def)

lemma slice_nonneg:
  assumes h: "off + len \<le> length b"
  shows "slice b False off False len = take len (drop off b)"
proof -
  have le: "off \<le> length b" using h by arith
  have s: "start_of b False off = off" using le by (simp add: min_absorb2)
  have e: "stop_of b (start_of b False off) False len = off + len"
    using h s by (simp add: stop_of_def min_absorb2)
  show ?thesis using s e by (simp add: slice_def)
qed

lemma slice_from_last_n:
  assumes h: "n \<le> length b"
  shows "slice_from b True n = take n (drop (length b - n) b)"
proof -
  have s: "start_of b True n = length b - n" using h by (simp add: min_absorb2)
  have len: "length (drop (length b - n) b) = n"
  proof -
    have "length (drop (length b - n) b) = length b - (length b - n)"
      by (simp add: length_drop)
    thus ?thesis using h by arith
  qed
  hence "drop (length b - n) b = take n (drop (length b - n) b)"
    by (simp add: take_all)
  thus ?thesis using s by (simp add: slice_from_def)
qed

lemma slice_negative_length:
  assumes h: "n \<le> length b"
  shows "slice b False 0 True n = take (length b - n) b"
proof -
  have s: "start_of b False 0 = 0" by simp
  have e: "stop_of b 0 True n = length b - n"
    using h by (simp add: stop_of_def min_absorb2)
  show ?thesis using s e by (simp add: slice_def)
qed

text \<open>Past the end, both forms clamp instead of failing: a negative offset
  larger than the text is the whole text, and a negative length larger than
  the text leaves nothing.\<close>

lemma slice_from_neg_clamps:
  "slice_from b True (length b + k) = b"
  by (simp add: min_absorb2)

lemma slice_neg_len_clamps:
  assumes h: "length b \<le> n"
  shows "slice b False 0 True n = []"
proof -
  have e: "stop_of b 0 True n = 0" using h by (simp add: stop_of_def min_absorb2)
  show ?thesis using e by (simp add: slice_def)
qed

text \<open>REFUTED: "a negative offset is just zero" and "a negative length is
  ignored". Both are the plausible-but-wrong readings that would make
  `${v: -2}` the identity and `${v:0:-1}` the whole value.\<close>

theorem negative_offset_is_not_zero:
  "slice_from [97, 98, 99] True 2 \<noteq> slice_from [97, 98, 99] False 0"
  by simp

theorem negative_length_is_not_ignored:
  "slice [97, 98, 99] False 0 True 1 \<noteq> slice_from [97, 98, 99] False 0"
  by (simp add: slice_def stop_of_def)

theorem negative_length_is_not_a_prefix:
  "slice [97, 98, 99] False 0 True 1 \<noteq> take 1 [97, 98, 99]"
  by (simp_all add: slice_def stop_of_def)

lemma slice_witness:
  "slice_from [97, 98, 99] True 2 = [98, 99] \<and>
   slice_from [97, 98, 99] False 1 = [98, 99] \<and>
   slice [97, 98, 99] False 0 True 1 = [97, 98] \<and>
   slice [97, 98, 99] False 1 False 1 = [98] \<and>
   slice [97, 98, 99] True 1 False 1 = [99]"
  by (simp add: slice_def stop_of_def)

section \<open>`cut`'s field selection: ranges, input order, complement, `-o`\<close>

text \<open>The field list of a text is `split_on` (ShIR_TextOps_Compose). Joining it
  back with the delimiter recovers the text EXACTLY — every byte appears once,
  in order, and a trailing delimiter is the empty last field. That is the
  per-field byte identity `split.node` recorded as missing.\<close>

fun intercalate :: "nat \<Rightarrow> bytes list \<Rightarrow> bytes" where
  "intercalate d [] = []"
| "intercalate d [x] = x"
| "intercalate d (x # y # zs) = x @ [d] @ intercalate d (y # zs)"

lemma intercalate_split_on:
  "intercalate d (split_on d t) = t"
proof (induction t)
  case Nil
  then show ?case by simp
next
  case (Cons c cs)
  have ne: "split_on d cs \<noteq> []" by (rule split_on_never_nil)
  show ?case
  proof (cases "c = d")
    case True
    then obtain x xs where fs: "split_on d cs = x # xs"
      using ne by (cases "split_on d cs") simp
    have ih: "intercalate d (x # xs) = cs" using Cons.IH fs by simp
    show ?thesis using True fs ih by (cases xs) simp_all
  next
    case False
    then obtain x xs where fs: "split_on d cs = x # xs"
      using ne by (cases "split_on d cs") simp
    have ih: "intercalate d (x # xs) = cs" using Cons.IH fs by simp
    show ?thesis using False fs ih by (cases xs) simp_all
  qed
qed

lemma cut_keeps_trailing_empty_field:
  "split_on 47 [97, 47] = [[97], []]"
  by simp

text \<open>A selector is one `-f` argument: a single index, or a range with an
  optional end (`-fN-M` is `Some`, `-fN-` is `None`). A selector LIST is
  `-f1,3`. `--complement` is `select_not`.\<close>

datatype fsel = FOne nat | FRange nat "nat option"

fun covers :: "fsel \<Rightarrow> nat \<Rightarrow> bool" where
  "covers (FOne n) i = (i = n)"
| "covers (FRange a lo) i =
     (a \<le> i \<and> (case lo of None \<Rightarrow> True | Some b \<Rightarrow> i \<le> b))"

definition sel_idx :: "fsel list \<Rightarrow> nat \<Rightarrow> bool" where
  "sel_idx sels i = (\<exists>s \<in> set sels. covers s i)"

text \<open>The selection is over the INDICES of the field list, in increasing order:
  that is what makes the output follow the input, not the argument.\<close>

definition select :: "fsel list \<Rightarrow> bytes list \<Rightarrow> bytes list" where
  "select sels fs = map (nth fs) (filter (sel_idx sels) [0 ..< length fs])"

definition select_not :: "fsel list \<Rightarrow> bytes list \<Rightarrow> bytes list" where
  "select_not sels fs =
     map (nth fs) (filter (\<lambda>i. \<not> sel_idx sels i) [0 ..< length fs])"

definition splice_fields :: "nat \<Rightarrow> fsel list \<Rightarrow> bytes list \<Rightarrow> bytes" where
  "splice_fields d sels fs = intercalate d (select sels fs)"

lemma map_nth_upt [simp]: "map (nth fs) [0 ..< length fs] = fs"
  by (rule nth_equalityI) simp_all

lemma select_all_range: "select [FRange 0 None] fs = fs"
  by (simp add: select_def sel_idx_def covers.simps)

lemma select_nil [simp]: "select sels [] = []"
  by (simp add: select_def)

lemma select_length_le: "length (select sels fs) \<le> length fs"
proof -
  have "length (filter (sel_idx sels) [0 ..< length fs]) \<le> length [0 ..< length fs]"
    by (rule length_filter_le)
  thus ?thesis by (simp add: select_def)
qed

text \<open>The selected fields are fields of the input — nothing is invented, and
  nothing is duplicated.\<close>

lemma select_subset:
  assumes h: "x \<in> set (select sels fs)"
  shows "x \<in> set fs"
proof -
  from h obtain i where
    i: "i \<in> set (filter (sel_idx sels) [0 ..< length fs])" and xi: "x = fs ! i"
    by (auto simp: select_def)
  have "i < length fs" using i by simp
  thus ?thesis using xi by (simp add: nth_mem)
qed

text \<open>`--complement` is exactly the indices the selection leaves out, so the two
  counts add up to the field count.\<close>

lemma filter_length_partition:
  "length (filter P xs) + length (filter (\<lambda>x. \<not> P x) xs) = length xs"
  by (induction xs) simp_all

lemma complement_lengths:
  "length (select sels fs) + length (select_not sels fs) = length fs"
proof -
  have part: "length (filter (sel_idx sels) [0 ..< length fs])
        + length (filter (\<lambda>i. \<not> sel_idx sels i) [0 ..< length fs])
      = length [0 ..< length fs]"
    by (rule filter_length_partition)
  thus ?thesis using part by (simp add: select_def select_not_def)
qed

lemma complement_nil: "select_not sels [] = []"
  by (simp add: select_not_def)

text \<open>The output delimiter goes BETWEEN the selected fields, which is why one
  selected field has no delimiter at all (a renderer that always appends `-o`
  would emit a trailing separator).\<close>

lemma splice_single:
  "splice_fields d [FOne 0] [x] = x"
  by (simp add: splice_fields_def select_def sel_idx_def covers.simps)

lemma splice_three:
  "splice_fields d [FOne 0, FOne 1, FOne 2] [a, b, c] = a @ [d] @ b @ [d] @ c"
  by (simp add: splice_fields_def select_def sel_idx_def covers.simps)

text \<open>REFUTED, by evaluation, the three readings a `cut` lowering gets wrong:

  1. the fields come out in ARGUMENT order (`-f3,1` printing field 3 first);
  2. a non-contiguous selection behaves like a `take` of the field list;
  3. the output delimiter is appended rather than interleaved.\<close>

theorem order_is_input_not_argument:
  "splice_fields 44 [FOne 2, FOne 1] [[97], [98], [99]] \<noteq> [99, 44, 98]"
  by (simp add: splice_fields_def select_def sel_idx_def covers.simps)

theorem range_is_neither_first_nor_prefix:
  "splice_fields 44 [FRange 1 (Some 2)] [[97], [98], [99]] = [98, 44, 99] \<and>
   splice_fields 44 [FRange 1 (Some 2)] [[97], [98], [99]] \<noteq> [97, 44, 98]"
  by (simp add: splice_fields_def select_def sel_idx_def covers.simps)

theorem output_delimiter_is_not_appended:
  "splice_fields 44 [FOne 0] [[97]] \<noteq> [97, 44]"
  by (simp add: splice_fields_def select_def sel_idx_def covers.simps)

theorem select_all_keeps_every_field:
  "splice_fields 44 [FRange 0 None] [[97], [98]] = [97, 44, 98]"
  by (simp add: splice_fields_def select_def sel_idx_def covers.simps)

end
