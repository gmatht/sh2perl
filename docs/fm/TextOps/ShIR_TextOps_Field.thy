(*  Title:      ShIR_TextOps_Field.thy
    The `FieldStrip`, `FieldStripEnd` and `FieldGrab` nodes: the field CURSOR.

    These three nodes are the pieces the `str-chop-cursor` transform builds out
    of a front-chop loop:

      x          = FieldGrab(s, off, D)      head field of the tail at `off`
      {s}_fc     = FieldStrip(s, off, D)     absolute offset past that D
      {s}_fc     = FieldStripEnd(s, off, D)  same, but a miss DRAINS to the end
      {s}_fc     = 0                         one init before the first use

    The nodes carry no state: every read is a function of the whole text, the
    absolute byte offset and the delimiter, so the obligations are offset
    arithmetic. That is what this file proves — the three nodes' obligations
    (`field_grab_at_strip`, `field_strip_no_move_on_miss`,
    `field_strip_end_drains_on_miss`), the cursor bounds that make the offsets
    usable (`field_strip_le_length`, `field_strip_gt_off_on_hit`), the
    inverse reading of the two advances, and the honest corner cases (no match,
    no more fields, empty input, empty delimiter).

    THE DELIMITER IS LITERAL, AND EMPTY MEANS NO MATCH. The C arms guard the
    search with `*d` — an empty delimiter never matches, so `FieldGrab` returns
    the whole remaining tail, `FieldStrip` leaves the offset alone and
    `FieldStripEnd` drains. `field_hit` encodes exactly that guard, and
    `field_grab_empty_delim_is_not_a_cut` records why the node cannot simply be
    the cut at the tail: `cut_head_first (drop off s) []` is `[]`, which is not
    what any of the three arms computes.

    WHAT IS ASSUMED, NOT PROVED. The render arms do not clamp the offset: they
    call `strstr(text + off, d)`, so a caller must supply `off <= strlen(text)`.
    That is the transform's read law (init at 0, only `FieldStrip`/`FieldStripEnd`
    write the cursor, both never past the end), not a property of the nodes;
    here it appears as an explicit hypothesis on the arithmetic facts
    (`off <= length s`) rather than being hidden. The C arms also skip the copy
    for a null text pointer and treat undecodable bytes as bytes; neither is
    modelled, and neither is reachable from a shell string.
*)

theory ShIR_TextOps_Field
  imports ShIR_TextOps_Cut
begin

section \<open>The cursor: one offset, three readings\<close>

lemma find_first_empty_needle [simp]:
  "find_first s [] = Some 0"
  by (induct s) (simp_all add: find_first_nil find_first_cons)

text \<open>`field_hit s off d` is the ABSOLUTE index of the first occurrence of `d`
  at or after `off`, or `None` when `d` does not occur there. The empty
  delimiter is the no-match case, matching the arms' `*d` guard: the first
  occurrence of nothing is not a position the shell treats as a delimiter.\<close>

definition field_hit :: "bytes \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> nat option" where
  "field_hit s off d \<equiv>
     (if d = [] then None
      else map_option (\<lambda>i. off + i) (find_first (drop off s) d))"

text \<open>`field_grab` is the head field: everything from `off` up to that
  occurrence, and the whole remaining tail when there is none (bash leaves the
  value alone on a non-match). `field_strip` advances to just PAST the
  delimiter, and leaves the offset where it was on a non-match. `field_strip_end`
  is the guarded pair's variant: on a non-match it advances to the END of the
  text, which is the dropped `s=""` fallback arm.\<close>

definition field_grab :: "bytes \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "field_grab s off d \<equiv>
     (case field_hit s off d of
        None \<Rightarrow> drop off s
      | Some k \<Rightarrow> take (k - off) (drop off s))"

definition field_strip :: "bytes \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> nat" where
  "field_strip s off d \<equiv>
     (case field_hit s off d of None \<Rightarrow> off | Some k \<Rightarrow> k + length d)"

definition field_strip_end :: "bytes \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> nat" where
  "field_strip_end s off d \<equiv>
     (case field_hit s off d of None \<Rightarrow> length s | Some k \<Rightarrow> k + length d)"

section \<open>Where the cursor points\<close>

lemma field_hit_empty_delim [simp]:
  "field_hit s off [] = None"
  unfolding field_hit_def by simp

lemma field_hit_someD:
  assumes h: "field_hit s off d = Some k"
  shows "d \<noteq> [] \<and> (\<exists>i. find_first (drop off s) d = Some i \<and> k = off + i)"
proof (cases "d = []")
  case True
  then show ?thesis using h by (simp add: field_hit_def)
next
  case False
  have h': "map_option (\<lambda>i. off + i) (find_first (drop off s) d) = Some k"
    using h False by (simp add: field_hit_def)
  have "\<exists>i. find_first (drop off s) d = Some i \<and> k = off + i"
    using h' by (cases "find_first (drop off s) d") auto
  with False show ?thesis by blast
qed

text \<open>WHERE, part 1: a returned offset really is an occurrence, and it lies
  inside the text.\<close>

lemma field_hit_occ:
  assumes h: "field_hit s off d = Some k" and bl: "off \<le> length s"
  shows "k \<le> length s \<and> take (length d) (drop k s) = d"
proof -
  from field_hit_someD[OF h] obtain i where
    fi: "find_first (drop off s) d = Some i" and k: "k = off + i" by blast
  have occ: "i \<le> length (drop off s) \<and> take (length d) (drop i (drop off s)) = d"
    using find_first_occ[OF fi] .
  have dd: "drop i (drop off s) = drop (off + i) s"
    by (simp add: drop_drop add.commute)
  have i_le: "i \<le> length s - off" using occ by (simp add: length_drop)
  show ?thesis
  proof (intro conjI)
    show "k \<le> length s" using k i_le bl by arith
    show "take (length d) (drop k s) = d" using occ k dd by simp
  qed
qed

text \<open>WHERE, part 2: the no-match case — `None` means the delimiter does not
  occur anywhere after the cursor, including at the end.\<close>

lemma field_hit_none:
  assumes h: "field_hit s off d = None" and nd: "d \<noteq> []"
  shows "\<forall>i. i \<le> length (drop off s) \<longrightarrow> take (length d) (drop i (drop off s)) \<noteq> d"
proof -
  have fi: "find_first (drop off s) d = None"
    using h nd by (simp add: field_hit_def)
  show ?thesis using find_first_none[OF fi] .
qed

text \<open>WHERE, part 3: leftmost — nothing after the cursor matches earlier.\<close>

lemma drop_sub_add:
  assumes oj: "off \<le> j"
  shows "drop (j - off) (drop off s) = drop j s"
proof -
  have "j - off + off = j" using oj by simp
  then show ?thesis by (simp add: drop_drop)
qed

lemma field_hit_min:
  assumes h: "field_hit s off d = Some k" and at: "take (length d) (drop j s) = d"
    and oj: "off \<le> j"
  shows "k \<le> j"
proof -
  from field_hit_someD[OF h] obtain i where
    nd: "d \<noteq> []" and fi: "find_first (drop off s) d = Some i" and k: "k = off + i" by blast
  have at': "take (length d) (drop (j - off) (drop off s)) = d"
    using at oj by (simp add: drop_sub_add)
  have bd: "j - off \<le> length (drop off s)"
  proof (rule ccontr)
    assume "\<not> j - off \<le> length (drop off s)"
    then have "drop (j - off) (drop off s) = []" by simp
    then have "d = []" using at' by simp
    with nd show False by simp
  qed
  have "i \<le> j - off" using find_first_min[OF fi at' bd] .
  then show ?thesis using k oj by arith
qed

section \<open>FieldGrab: the head field\<close>

lemma field_grab_hit:
  assumes h: "field_hit s off d = Some k"
  shows "field_grab s off d = take (k - off) (drop off s)"
  using h unfolding field_grab_def by simp

lemma field_grab_miss:
  assumes h: "field_hit s off d = None"
  shows "field_grab s off d = drop off s"
  using h unfolding field_grab_def by simp

text \<open>OBLIGATION \<open>field_grab_at_strip\<close>: the grab and the advance are two
  readings of ONE cursor position, not two independent moves. Both are taken at
  the same `off`, and the offset `field_strip` reports is exactly where the
  delimiter that stopped the grab ends — so the tail at `off` is the grabbed
  field, the delimiter, and the tail at the new offset. Nothing is dropped and
  nothing is read twice.

  This is the statement the transform relies on when it replaces
  `x=${s%%D*}; s=${s#*D}` with `x = FieldGrab(s, off, D)` and
  `off = FieldStrip(s, off, D)`: the two nodes compose, and the *next* field is
  the grab at the offset the previous strip reported.\<close>

theorem field_grab_at_strip:
  assumes h: "field_hit s off d = Some k"
  shows "drop off s = field_grab s off d @ d @ drop (field_strip s off d) s"
proof -
  from field_hit_someD[OF h] obtain i where
    fi: "find_first (drop off s) d = Some i" and k: "k = off + i" by blast
  have sp: "drop off s = take i (drop off s) @ d @ drop (i + length d) (drop off s)"
    using find_first_split[OF fi] .
  have grab: "field_grab s off d = take i (drop off s)"
    using field_grab_hit[OF h] k by simp
  have strip: "field_strip s off d = off + i + length d"
    using h k unfolding field_strip_def by simp
  have dd: "drop (off + i + length d) s = drop (i + length d) (drop off s)"
    by (simp add: drop_drop ac_simps)
  show ?thesis using sp grab strip dd by simp
qed

text \<open>NO MORE FIELDS: a cursor at (or past) the end reads the empty field, which
  is the drained-string read. This is also what makes the guarded pair's
  fallback sound — a grab from the end reads `""` exactly like a grab of the
  emptied string.\<close>

lemma field_grab_at_end:
  assumes le: "length s \<le> off"
  shows "field_grab s off d = []"
proof (cases "field_hit s off d")
  case None
  then have "drop off s = []" using le by simp
  then show ?thesis using field_grab_miss[OF None] by simp
next
  case (Some k)
  have "drop off s = []" using le by simp
  then show ?thesis using field_grab_hit[OF Some] by simp
qed

text \<open>The grab invents nothing: it is a prefix of the tail at the cursor.\<close>

lemma field_grab_is_tail_prefix:
  assumes bl: "off \<le> length s"
  shows "field_grab s off d = take (length (field_grab s off d)) (drop off s)"
proof (cases "field_hit s off d")
  case None
  then show ?thesis using field_grab_miss[OF None] by (simp add: take_all)
next
  case (Some k)
  have occ: "k \<le> length s \<and> take (length d) (drop k s) = d" using field_hit_occ[OF Some bl] .
  have "k - off \<le> length s - off" using occ by arith
  then have "k - off \<le> length (drop off s)" by (simp add: length_drop)
  then show ?thesis using field_grab_hit[OF Some] by (simp add: take_all)
qed

section \<open>FieldStrip: advance past the delimiter, or do not move\<close>

text \<open>OBLIGATION \<open>field_strip_no_move_on_miss\<close>: bash's `${s#*D}` is a no-op
  when the pattern does not match, so a non-match must leave the cursor exactly
  where it was — and the paired grab then reads the whole remaining tail. Moving
  the cursor on a miss would silently skip a field; draining it would be the
  other node.\<close>

theorem field_strip_no_move_on_miss:
  assumes h: "field_hit s off d = None"
  shows "field_strip s off d = off"
  using h unfolding field_strip_def by simp

lemma field_strip_hit:
  assumes h: "field_hit s off d = Some k"
  shows "field_strip s off d = k + length d"
  using h unfolding field_strip_def by simp

text \<open>The cursor never moves backwards, and on a hit it strictly advances —
  which is the termination argument for the tokenizer loop: every converted chop
  consumes at least one byte of the original buffer. A miss is the case the
  transform must not loop on (it needs the guard, or `FieldStripEnd`), and
  `field_strip_stuck_iff_miss` says so exactly.\<close>

lemma field_strip_ge_off:
  "off \<le> field_strip s off d"
proof (cases "field_hit s off d")
  case None
  then show ?thesis by (simp add: field_strip_def)
next
  case (Some k)
  from field_hit_someD[OF Some] have "off \<le> k" by arith
  then show ?thesis using field_strip_hit[OF Some] by simp
qed

lemma field_strip_gt_off_on_hit:
  assumes h: "field_hit s off d = Some k"
  shows "off < field_strip s off d"
proof -
  from field_hit_someD[OF h] obtain i where
    nd: "d \<noteq> []" and k: "k = off + i" by blast
  have oj: "off \<le> k" using k by simp
  have "0 < length d" using nd by simp
  then have "k < k + length d" by simp
  then have "off < k + length d" using oj `k < k + length d` by arith
  with field_strip_hit[OF h] show ?thesis by simp
qed

lemma field_strip_stuck_iff_miss:
  "field_strip s off d = off \<longleftrightarrow> field_hit s off d = None"
proof
  assume h: "field_strip s off d = off"
  show "field_hit s off d = None"
  proof (rule ccontr)
    assume "field_hit s off d \<noteq> None"
    then obtain k where sk: "field_hit s off d = Some k" by (cases "field_hit s off d") auto
    have "off < field_strip s off d" using field_strip_gt_off_on_hit[OF sk] .
    then show False using h by simp
  qed
next
  assume h: "field_hit s off d = None"
  then show "field_strip s off d = off" by (rule field_strip_no_move_on_miss)
qed

text \<open>THE CURSOR STAYS IN THE TEXT. With the read law's `off <= length s`, the
  new offset is still inside the buffer, so the next `strstr(text + off, d)` is
  within bounds and the arms need no clamp.\<close>

lemma field_strip_le_length:
  assumes bl: "off \<le> length s"
  shows "field_strip s off d \<le> length s"
proof (cases "field_hit s off d")
  case None
  then show ?thesis using bl by (simp add: field_strip_def)
next
  case (Some k)
  have occ: "k \<le> length s \<and> take (length d) (drop k s) = d"
    using field_hit_occ[OF Some bl] .
  have ld: "length d \<le> length (drop k s)"
  proof -
    have "length d = length (take (length d) (drop k s))" using occ by simp
    also have "\<dots> = min (length d) (length (drop k s))" by (simp add: length_take)
    also have "\<dots> \<le> length (drop k s)" by simp
    finally show ?thesis .
  qed
  have "length d \<le> length s - k" using ld by (simp add: length_drop)
  then have "k + length d \<le> length s" using occ by arith
  then show ?thesis using field_strip_hit[OF Some] by simp
qed

text \<open>PROGRESS: on a hit the unconsumed tail strictly shrinks, which is the
  measure the converted loop decreases.\<close>

lemma field_strip_progress:
  assumes h: "field_hit s off d = Some k" and bl: "off \<le> length s"
  shows "length (drop (field_strip s off d) s) < length (drop off s)"
proof -
  have gt: "off < field_strip s off d" using field_strip_gt_off_on_hit[OF h] .
  have le: "field_strip s off d \<le> length s" using field_strip_le_length[OF bl] .
  have "length (drop (field_strip s off d) s) = length s - field_strip s off d"
    by (simp add: length_drop)
  also have "\<dots> < length s - off" using gt le by arith
  also have "\<dots> = length (drop off s)" by (simp add: length_drop)
  finally show ?thesis .
qed

section \<open>FieldStripEnd: the same advance, draining on a miss\<close>

text \<open>OBLIGATION \<open>field_strip_end_drains_on_miss\<close>: the guarded front-chop's
  fallback arm is `x=$s; s=""`, so on a miss the cursor must go to the END of
  the text. That drain is what makes dropping the guard sound: every later grab
  then reads `""` (`field_grab_after_drain`), exactly as the emptied string
  would.\<close>

theorem field_strip_end_drains_on_miss:
  assumes h: "field_hit s off d = None"
  shows "field_strip_end s off d = length s"
  using h unfolding field_strip_end_def by simp

text \<open>THE TWO ADVANCES ARE INVERSE READINGS OF ONE CURSOR: they agree exactly on
  a hit, and differ exactly on a miss — where one holds the offset and the other
  drains. Neither is derivable from the other, which is why the family has three
  nodes and not two.\<close>

lemma field_strip_end_hit:
  assumes h: "field_hit s off d = Some k"
  shows "field_strip_end s off d = k + length d"
  using h unfolding field_strip_end_def by simp

theorem field_strip_end_agrees_on_hit:
  assumes h: "field_hit s off d = Some k"
  shows "field_strip_end s off d = field_strip s off d"
  using field_strip_end_hit[OF h] field_strip_hit[OF h] by simp

theorem field_strip_end_differs_on_miss:
  assumes h: "field_hit s off d = None" and "off < length s"
  shows "field_strip_end s off d \<noteq> field_strip s off d"
  using field_strip_end_drains_on_miss[OF h] field_strip_no_move_on_miss[OF h] `off < length s`
  by simp

lemma field_strip_le_strip_end:
  assumes bl: "off \<le> length s"
  shows "field_strip s off d \<le> field_strip_end s off d"
proof (cases "field_hit s off d")
  case None
  then show ?thesis using bl by (simp add: field_strip_def field_strip_end_def)
next
  case (Some k)
  then show ?thesis using field_strip_end_hit[OF Some] field_strip_hit[OF Some] by simp
qed

lemma field_strip_end_le_length:
  assumes bl: "off \<le> length s"
  shows "field_strip_end s off d \<le> length s"
proof (cases "field_hit s off d")
  case None
  then show ?thesis by (simp add: field_strip_end_def)
next
  case (Some k)
  have occ: "k \<le> length s \<and> take (length d) (drop k s) = d"
    using field_hit_occ[OF Some bl] .
  have ld: "length d \<le> length (drop k s)"
  proof -
    have "length d = length (take (length d) (drop k s))" using occ by simp
    also have "\<dots> = min (length d) (length (drop k s))" by (simp add: length_take)
    also have "\<dots> \<le> length (drop k s)" by simp
    finally show ?thesis .
  qed
  have "length d \<le> length s - k" using ld by (simp add: length_drop)
  then have "k + length d \<le> length s" using occ by arith
  then show ?thesis using field_strip_end_hit[OF Some] by simp
qed

theorem field_grab_after_drain:
  assumes h: "field_hit s off d = None"
  shows "field_grab s (field_strip_end s off d) d = []"
proof -
  have "field_strip_end s off d = length s" by (rule field_strip_end_drains_on_miss[OF h])
  then show ?thesis by (simp add: field_grab_at_end)
qed

section \<open>Initialisation and the empty delimiter\<close>

text \<open>The cursor is initialised to 0 before the first use, so the first grab is
  the head field of the whole text. The arms rely on that, and on the offset
  never exceeding the buffer.\<close>

lemma field_init_is_zero:
  assumes h: "field_hit s 0 d = Some k"
  shows "s = field_grab s 0 d @ d @ drop (field_strip s 0 d) s"
  using field_grab_at_strip[OF h] by simp

text \<open>AN EMPTY DELIMITER IS NOT A POSITION. The arms guard the search with
  `*d`, so an empty delimiter never matches: the grab is the whole tail, the
  strip does not move and the end-strip drains. The cut at the tail is a
  DIFFERENT answer — `cut_head_first (drop off s) []` is `[]` — which is why the
  node needs its own guard rather than being `CutAt` with an empty literal.\<close>

lemma field_empty_delim_grab [simp]:
  "field_grab s off [] = drop off s"
  unfolding field_grab_def by simp

lemma field_empty_delim_strip [simp]:
  "field_strip s off [] = off"
  unfolding field_strip_def by simp

lemma field_empty_delim_strip_end [simp]:
  "field_strip_end s off [] = length s"
  unfolding field_strip_end_def by simp

theorem field_grab_empty_delim_is_not_a_cut:
  assumes bl: "off < length s"
  shows "field_grab s off [] \<noteq> cut_head_first (drop off s) []"
proof -
  have ne: "drop off s \<noteq> []"
  proof
    assume "drop off s = []"
    then have "length s - off = 0" by (simp add: length_drop)
    then show False using bl by simp
  qed
  have cut: "cut_head_first (drop off s) [] = []"
  proof -
    have "find_first (drop off s) [] = Some 0" by simp
    then show ?thesis unfolding cut_head_first_def by simp
  qed
  have grab: "field_grab s off [] = drop off s" by simp
  show ?thesis using ne cut grab by simp
qed

section \<open>The obligations, named, with the nodes\<close>

text \<open>Collected for the three node files: `FieldGrab`'s `spec:` is
  `field_grab_at_strip` (whose instance at `off = 0` is the first field), and
  `FieldStrip`'s and `FieldStripEnd`'s are the two no-match laws, which is the
  whole difference between them.\<close>

theorem field_nodes_sound:
  "(\<forall>k. field_hit s off d = Some k \<longrightarrow>
      drop off s = field_grab s off d @ d @ drop (field_strip s off d) s) \<and>
   (field_hit s off d = None \<longrightarrow> field_strip s off d = off) \<and>
   (field_hit s off d = None \<longrightarrow> field_strip_end s off d = length s) \<and>
   (off \<le> length s \<longrightarrow> field_strip s off d \<le> length s) \<and>
   (off \<le> length s \<longrightarrow> field_strip_end s off d \<le> length s) \<and>
   (\<forall>k. field_hit s off d = Some k \<longrightarrow> off < field_strip s off d)"
proof (intro conjI allI impI)
  fix k
  assume h: "field_hit s off d = Some k"
  then show "drop off s = field_grab s off d @ d @ drop (field_strip s off d) s"
    by (rule field_grab_at_strip)
  show "off < field_strip s off d" using h by (rule field_strip_gt_off_on_hit)
next
  assume h: "field_hit s off d = None"
  then show "field_strip s off d = off" by (rule field_strip_no_move_on_miss)
  show "field_strip_end s off d = length s" using h by (rule field_strip_end_drains_on_miss)
next
  assume bl: "off \<le> length s"
  then show "field_strip s off d \<le> length s" by (rule field_strip_le_length)
  show "field_strip_end s off d \<le> length s" using bl by (rule field_strip_end_le_length)
qed

end
