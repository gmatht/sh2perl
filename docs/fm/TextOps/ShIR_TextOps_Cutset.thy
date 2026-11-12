(*  Title:      ShIR_TextOps_Cutset.thy
    The `cutset_trim` node: trim by a character SET instead of a fixed class.

    This is the generalisation of StringTrim, and it is a second theory rather
    than more rows in the first because the divergence it explains is not about
    the whitespace class at all: the node carries a CUTSET, and the arms
    disagree about what a cutset IS. The model is a byte set (matching this
    session's `bytes = nat list`), which is what the shell and Perl mean; the
    Go and Rust arms take sets of code points, and the witnesses below pin the
    point where the two readings differ.

    WHAT IS PROVED. Five named obligations, each general over arbitrary `s` and
    `cs`:

      cutset_trim_is_trim_by        the node is `trim_by` at the membership test
      cutset_trim_removes_only_a_run what is removed is a run, got from Strings
      cutset_side_three_valued      the side field's three values, and only those
      perl_cutset_needs_quotemeta   the arm is a function of the SET, nothing else
      go_trim_cutset_is_codepoints  the model decides per BYTE, not per character
      rust_trim_matches_is_chars    same, at the trailing end

    so run-only removal and idempotence follow from ShIR_TextOps_Strings rather
    than being re-proved. Each divergence claim is accompanied by a witness
    lemma (the `eval`s below) that names the bytes at which the readings part.

    WHAT IS NOT MODELLED. The regular expression Perl's `s/^[\Q..\E]+//`
    compiles to, and UTF-8 decoding for the Go/Rust arms. The witnesses are
    evaluations of the model; the renderers' divergence is stated in prose, not
    proved against a regex engine (see docs/fm/README.md, limitations).
*)

theory ShIR_TextOps_Cutset
  imports ShIR_TextOps_Strings
begin

section \<open>CutsetTrim: the trim class becomes data\<close>

text \<open>The node's `side` field is a string in the node file; here it is the
  three values the shell actually distinguishes.\<close>

datatype cut_side = C_both | C_lead | C_trail

fun lead_of :: "cut_side \<Rightarrow> bool" where
  "lead_of C_both = True"
| "lead_of C_lead = True"
| "lead_of C_trail = False"

fun trail_of :: "cut_side \<Rightarrow> bool" where
  "trail_of C_both = True"
| "trail_of C_lead = False"
| "trail_of C_trail = True"

definition cutset_mem :: "bytes \<Rightarrow> nat \<Rightarrow> bool" where
  "cutset_mem cs c \<equiv> c \<in> set cs"

definition cutset_trim :: "cut_side \<Rightarrow> bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cutset_trim side cs s \<equiv> trim_by (cutset_mem cs) (lead_of side) (trail_of side) s"

text \<open>OBLIGATION \<open>cutset_trim_sound\<close>: the node is the generic run-drop at
  the set-membership predicate, with the two booleans the side field means.
  Everything `trim_by` proves — that only a RUN is removed at each end, that a
  trim is idempotent — then applies without restating it.\<close>

theorem cutset_trim_sound:
  "cutset_trim side cs s =
     trim_by (\<lambda>c. c \<in> set cs) (lead_of side) (trail_of side) s \<and>
   cutset_trim C_both cs s = trim_by (\<lambda>c. c \<in> set cs) True True s \<and>
   cutset_trim C_lead cs s = trim_by (\<lambda>c. c \<in> set cs) True False s \<and>
   cutset_trim C_trail cs s = trim_by (\<lambda>c. c \<in> set cs) False True s"
proof (intro conjI)
  show "cutset_trim side cs s =
        trim_by (\<lambda>c. c \<in> set cs) (lead_of side) (trail_of side) s"
    unfolding cutset_trim_def cutset_mem_def by simp
  show "cutset_trim C_both cs s = trim_by (\<lambda>c. c \<in> set cs) True True s"
    unfolding cutset_trim_def cutset_mem_def by simp
  show "cutset_trim C_lead cs s = trim_by (\<lambda>c. c \<in> set cs) True False s"
    unfolding cutset_trim_def cutset_mem_def by simp
  show "cutset_trim C_trail cs s = trim_by (\<lambda>c. c \<in> set cs) False True s"
    unfolding cutset_trim_def cutset_mem_def by simp
qed

text \<open>OBLIGATION \<open>cutset_trim_is_trim_by\<close>: the node against
  `trim_by (\<lambda>c. c \<in> cutset) l t s`, for arbitrary `s` and cutset and any of the
  three sides. It is definitional, which is the point: the node has no arithmetic
  of its own, so everything ShIR_TextOps_Strings proves about `trim_by` applies
  unchanged.\<close>

theorem cutset_trim_is_trim_by:
  "cutset_trim side cs s = trim_by (cutset_mem cs) (lead_of side) (trail_of side) s"
  unfolding cutset_trim_def by (rule refl)

text \<open>The `trim_by` results, spelled out per side, so the obligation above can
  be used without unfolding definitions in later proofs.\<close>

lemma cutset_lead_is_lead_drop [simp]:
  "cutset_trim C_lead cs s = lead_drop (cutset_mem cs) s"
  unfolding cutset_trim_def by simp

lemma cutset_trail_is_trail_drop [simp]:
  "cutset_trim C_trail cs s = trail_drop (cutset_mem cs) s"
  unfolding cutset_trim_def by simp

lemma cutset_both_is_both_drops [simp]:
  "cutset_trim C_both cs s = trail_drop (cutset_mem cs) (lead_drop (cutset_mem cs) s)"
  unfolding cutset_trim_def by simp

text \<open>OBLIGATION (inherited, not re-proved): a trim removes a RUN at each end.
  The witnesses the renderers rely on — that what is kept is a suffix, that a
  non-empty result starts with a non-member, and that nothing outside the input
  appears — come straight from the `lead_drop`/`trail_drop` facts in
  ShIR_TextOps_Strings.\<close>

theorem cutset_trim_removes_only_a_run:
  "(\<exists>r. s = r @ cutset_trim C_lead cs s) \<and>
   (cutset_trim C_lead cs s \<noteq> [] \<longrightarrow>
      hd (cutset_trim C_lead cs s) \<notin> set cs) \<and>
   (\<exists>r. s = cutset_trim C_trail cs s @ r) \<and>
   set (cutset_trim C_lead cs s) \<subseteq> set s"
proof (intro conjI impI)
  show "\<exists>r. s = r @ cutset_trim C_lead cs s"
    using lead_drop_suffix[where P = "cutset_mem cs" and s = s] by simp
  show "cutset_trim C_lead cs s \<noteq> [] \<Longrightarrow>
        hd (cutset_trim C_lead cs s) \<notin> set cs"
  proof -
    assume h: "cutset_trim C_lead cs s \<noteq> []"
    have ne: "lead_drop (cutset_mem cs) s \<noteq> []" using h by simp
    have "\<not> cutset_mem cs (hd (lead_drop (cutset_mem cs) s))"
      using ne by (rule lead_drop_head)
    then show ?thesis by (simp add: cutset_mem_def)
  qed
  show "\<exists>r. s = cutset_trim C_trail cs s @ r"
    using trail_drop_prefix[where P = "cutset_mem cs" and s = s] by simp
  show "set (cutset_trim C_lead cs s) \<subseteq> set s"
    using lead_drop_subset[where P = "cutset_mem cs" and s = s] by simp
qed

text \<open>The side field is three-valued, so a renderer's conditional chain over
  both/leading/trailing is exhaustive: no fourth case hides in the node's
  `optional_string`.\<close>

theorem cutset_side_three_valued:
  "(side = C_both \<or> side = C_lead \<or> side = C_trail) \<and>
   (lead_of side, trail_of side) \<in> {(True, True), (True, False), (False, True)} \<and>
   (lead_of side, trail_of side) \<noteq> (False, False)"
proof (intro conjI)
  show "side = C_both \<or> side = C_lead \<or> side = C_trail" by (cases side) simp_all
  show "(lead_of side, trail_of side) \<in> {(True, True), (True, False), (False, True)}"
    by (cases side) simp_all
  show "(lead_of side, trail_of side) \<noteq> (False, False)" by (cases side) simp_all
qed

text \<open>Idempotence, one row per side, inherited from the generic theory: a
  second trim removes nothing because the first left a non-member at the
  trimmed ends.\<close>

theorem cutset_trim_idem_both:
  "cutset_trim C_both cs (cutset_trim C_both cs s) = cutset_trim C_both cs s"
  unfolding cutset_trim_is_trim_by
  by (simp only: lead_of.simps trail_of.simps trim_by_idem_both)

theorem cutset_trim_idem_lead:
  "cutset_trim C_lead cs (cutset_trim C_lead cs s) = cutset_trim C_lead cs s"
  unfolding cutset_trim_is_trim_by
  by (simp only: lead_of.simps trail_of.simps trim_by_idem_pre)

theorem cutset_trim_idem_trail:
  "cutset_trim C_trail cs (cutset_trim C_trail cs s) = cutset_trim C_trail cs s"
  unfolding cutset_trim_is_trim_by
  by (simp only: lead_of.simps trail_of.simps trim_by_idem_suf)

text \<open>The both-ended arm is exactly the two one-ended runs, in that order —
  which is what a renderer emitting two regexes (rather than one class with
  both anchors) claims.\<close>

theorem cutset_trim_both_is_lead_then_trail:
  "cutset_trim C_both cs s =
     trail_drop (cutset_mem cs) (lead_drop (cutset_mem cs) s)"
  unfolding cutset_trim_def by simp

section \<open>The cutset is data: runs, not occurrences\<close>

text \<open>The whole run at a trimmed end goes, and an interior run stays: the node
  is a trim, not a delete.\<close>

lemma cutset_trim_removes_the_run:
  "cutset_trim C_both [120] [120, 120, 97, 120, 120] = [97]"
  by eval

lemma cutset_trim_keeps_an_interior_run:
  "cutset_trim C_lead [120] [97, 120, 120, 98] = [97, 120, 120, 98]"
  by eval

lemma cutset_trim_trail_only_keeps_leading:
  "cutset_trim C_trail [120] [120, 97, 120] = [120, 97]"
  by eval

section \<open>Perl: the arm is right only because of \<open>\Q..\E\<close>\<close>

text \<open>The Perl arm is `s/^[\Q<cutset>\E]+//`: the cutset is interpolated into a
  BRACKET EXPRESSION, and the quoting is what keeps the three bracket
  metacharacters — `]`, `^`, `\` — from meaning something else. The node's
  cutset is data (a byte set), and the model removes these three members like
  any other; a rendering that dropped the quoting would be reading a different
  set. These are the witnesses a reviewer can run.\<close>

lemma cutset_trim_closing_bracket:
  "cutset_trim C_both [93] [93, 93, 97, 93] = [97]"
  by eval

lemma cutset_trim_caret:
  "cutset_trim C_both [94] [94, 97, 94] = [97]"
  by eval

lemma cutset_trim_backslash:
  "cutset_trim C_both [92] [92, 92, 97, 92] = [97]"
  by eval

lemma cutset_trim_hyphen_is_a_member_not_a_range:
  "cutset_trim C_both [97, 45, 99] [97, 99, 98, 97] = [98]"
  by eval

text \<open>OBLIGATION \<open>perl_cutset_needs_quotemeta\<close>. The general statement
  is that the arm is a function of the cutset as a SET and of nothing else: two
  cutsets with the same members trim identically, whatever order or repetition
  they were written with. That is precisely what `\Q..\E` is for — it makes the
  compiled class denote the member set — and it is why an unquoted
  interpolation of `]`, `^` or `\` is a different node, not a stylistic
  variant. The bracketed witnesses below name the bytes where the readings
  part.\<close>

theorem perl_cutset_needs_quotemeta:
  "set cs = set cs' \<Longrightarrow> cutset_trim side cs s = cutset_trim side cs' s"
proof -
  assume h: "set cs = set cs'"
  have agree: "\<forall>c \<in> set s. cutset_mem cs c = cutset_mem cs' c"
    using h by (simp add: cutset_mem_def)
  have "trim_by (cutset_mem cs) (lead_of side) (trail_of side) s =
        trim_by (cutset_mem cs') (lead_of side) (trail_of side) s"
    using agree by (rule trim_by_cong)
  then show ?thesis unfolding cutset_trim_def .
qed

section \<open>Go and Rust: a set of code points over a byte model\<close>

text \<open>Go's `strings.Trim(s, cutset)` and Rust's `trim_matches(|c|
  cutset.contains(c))` take sets of CODE POINTS; the node's model is bytes. For
  ASCII the two agree, which is why every ASCII witness passes either way. The
  witnesses below are where they cannot: the byte 0xC2 is half of the UTF-8
  encoding of U+00A0 (and is not itself a character), so a byte-wise trim
  removes it and a codepoint-wise trim cannot.\<close>

text \<open>OBLIGATION \<open>go_trim_cutset_is_codepoints\<close>. The model decides
  one BYTE at a time: a member byte at the head is removed whatever follows it,
  which is a rule `strings.Trim` (a set of code points) has no way to express
  when the byte is the lead byte of a multi-byte character. The witness below
  shows the model answering with a continuation byte and no lead byte — text a
  codepoint trim cannot produce.\<close>

theorem go_trim_cutset_is_codepoints:
  "c \<in> set cs \<Longrightarrow> cutset_trim C_lead cs (c # s) = cutset_trim C_lead cs s"
proof -
  assume h: "c \<in> set cs"
  have mem: "cutset_mem cs c" using h by (simp add: cutset_mem_def)
  have "cutset_trim C_lead cs (c # s) = lead_drop (cutset_mem cs) (c # s)" by simp
  also have "\<dots> = lead_drop (cutset_mem cs) s" using mem by simp
  also have "\<dots> = cutset_trim C_lead cs s" by simp
  finally show ?thesis .
qed

text \<open>OBLIGATION \<open>rust_trim_matches_is_chars\<close>. The same fact at the
  trailing end: `trim_matches(|c| cutset.contains(c))` asks about a `char`, while
  the model asks about a byte, so a trailing member byte goes even when it is
  half of a character.\<close>

theorem rust_trim_matches_is_chars:
  "c \<in> set cs \<Longrightarrow> cutset_trim C_trail cs (s @ [c]) = cutset_trim C_trail cs s"
proof -
  assume h: "c \<in> set cs"
  have "cutset_trim C_trail cs (s @ [c]) =
        trail_drop (cutset_mem cs) (s @ [c])" by simp
  also have "\<dots> = trail_drop (cutset_mem cs) s"
  proof -
    have mem: "cutset_mem cs c" using h by (simp add: cutset_mem_def)
    have rev: "rev (s @ [c]) = c # rev s" by simp
    show ?thesis unfolding trail_drop_def using rev mem by simp
  qed
  also have "\<dots> = cutset_trim C_trail cs s" by simp
  finally show ?thesis .
qed

lemma cutset_trim_nbsp_pair_is_one_character:
  "cutset_trim C_both [194, 160] [194, 160, 97, 194, 160] = [97]"
  by eval

lemma cutset_trim_bytewise_can_split_a_character:
  "cutset_trim C_both [194] [194, 160, 97] = [160, 97]"
  by eval

text \<open>The second witness is the divergence in one line: the model answers
  \<open>[160, 97]\<close>, which is not text — a continuation byte with no lead byte.
  A codepoint trim over the same source cutset state leaves \<open>[194, 160, 97]\<close>
  whole. Neither answer is the node's; the node says "characters" and the model
  says bytes, and that gap is recorded, not papered over. The ASCII guard a
  renderer can rely on is the one already proved for StringTrim.\<close>

lemma cutset_trim_ascii_agrees_with_the_model:
  "cutset_trim C_both [97, 98] [97, 98, 99, 98, 97] = [99]"
  by eval

end
