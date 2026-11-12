(*  Title:      ShIR_TextOps_Cutset.thy
    The `cutset_trim` node: trim by a character SET instead of a fixed class.

    This is the generalisation of StringTrim, and it is a second theory rather
    than more rows in the first because the divergence it explains is not about
    the whitespace class at all: the node carries a CUTSET, and the arms
    disagree about what a cutset IS. The model is a byte set (matching this
    session's `bytes = nat list`), which is what the shell and Perl mean; the
    Go and Rust arms take sets of code points, and the witnesses below pin the
    point where the two readings differ.

    WHAT IS PROVED. The node's `spec:` is `cutset_trim_is_trim_by` (the node
    IS the generic trim at the membership test) together with
    `cutset_trim_sound` (the same identity with the side field unfolded); the
    other obligations are general over arbitrary `s` and `cs`:

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

text \<open>MEMBER-SET INVARIANCE. The arm is a function of the cutset as a SET and of
  nothing else: two cutsets with the same members trim identically, whatever
  order or repetition they were written with. That is precisely what `\Q..\E`
  is for — it makes the compiled class denote the member set — and it is why an
  unquoted interpolation of `]`, `^` or `\` is a different node, not a
  stylistic variant. This lemma states only the invariance, because invariance
  on its own is true of ANY set-based reading (it would survive the quoting
  being dropped); the divergence proper is proved by comparing against the
  unquoted bracket reading in the next section.\<close>

theorem cutset_trim_depends_only_on_members:
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

text \<open>BYTE GRANULARITY. The model decides one BYTE at a time: a member byte at the
  head is removed whatever follows it, which is a rule `strings.Trim` (a set of
  code points) has no way to express when the byte is the lead byte of a
  multi-byte character. This lemma is the general fact, named for what it says;
  the codepoint comparison is proved with the decoder in the next section, where
  the witness shows the model answering with a continuation byte and no lead
  byte — text a codepoint trim cannot produce.\<close>

theorem cutset_trim_lead_is_bytewise:
  "c \<in> set cs \<Longrightarrow> cutset_trim C_lead cs (c # s) = cutset_trim C_lead cs s"
proof -
  assume h: "c \<in> set cs"
  have mem: "cutset_mem cs c" using h by (simp add: cutset_mem_def)
  have "cutset_trim C_lead cs (c # s) = lead_drop (cutset_mem cs) (c # s)" by simp
  also have "\<dots> = lead_drop (cutset_mem cs) s" using mem by simp
  also have "\<dots> = cutset_trim C_lead cs s" by simp
  finally show ?thesis .
qed

text \<open>TRAILING BYTE GRANULARITY, named `cutset_trim_trail_is_bytewise`. The same
  fact at the trailing end: `trim_matches(|c| cutset.contains(c))` asks about a
  `char`, while the model asks about a byte, so a trailing member byte goes even
  when it is half of a character.\<close>

theorem cutset_trim_trail_is_bytewise:
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


section \<open>The unquoted bracket reading: what \Q..\E is protecting\<close>

text \<open>A toy but honest reading of a bracket BODY when the body is interpolated
  WITHOUT `\Q..\E`. It is a syntax, not a trim: `]` closes the class, a leading
  `^` negates it, a backslash escapes the next byte, and `a-b` is a RANGE. The
  quoted reading is the byte set the node carries. The theorem at the end of
  this section compares the two on named bodies.\<close>

fun uq_body :: "bytes \<Rightarrow> bytes" where
  "uq_body [] = []"
| "uq_body (c # cs) =
     (if c = 93 then []
      else if c = 92 then (case cs of [] \<Rightarrow> [92] | d # ds \<Rightarrow> d # uq_body ds)
      else c # uq_body cs)"

fun uq_scan :: "bytes \<Rightarrow> nat \<Rightarrow> bool" where
  "uq_scan [] c = False"
| "uq_scan [x] c = (c = x)"
| "uq_scan [x, y] c = (c = x \<or> c = y)"
| "uq_scan (x # y # z # rest) c =
     (if y = 45 then (c = x \<or> (x \<le> c \<and> c \<le> z) \<or> uq_scan rest c)
      else (c = x \<or> uq_scan (y # z # rest) c))"

definition uq_class :: "bytes \<Rightarrow> nat \<Rightarrow> bool" where
  "uq_class body c \<equiv>
     (case body of [] \<Rightarrow> False
      | b0 # rest \<Rightarrow>
          (if b0 = 94 then \<not> uq_scan (uq_body rest) c
           else uq_scan (uq_body body) c))"

definition uq_trim :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "uq_trim body s \<equiv> trim_by (uq_class body) True True s"

text \<open>The quoted arm's set is `set body`; the unquoted reading is `uq_class
  body`. They differ on exactly the bytes the quoting exists to protect, and the
  difference is visible in the trimmed text — so a rendering that dropped
  `\Q..\E` would compute a different node, and this theorem would become false
  if the two readings were identified.\<close>

theorem perl_cutset_needs_quotemeta:
  "((93::nat) \<in> set [93] \<and> \<not> uq_class [93] 93 \<and>
     cutset_trim C_both [93] [93, 97] \<noteq> uq_trim [93] [93, 97]) \<and>
   ((98::nat) \<notin> set [94, 97] \<and> uq_class [94, 97] 98 \<and>
     cutset_trim C_both [94, 97] [98] \<noteq> uq_trim [94, 97] [98]) \<and>
   ((92::nat) \<in> set [92, 93] \<and> \<not> uq_class [92, 93] 92 \<and>
     cutset_trim C_both [92, 93] [92, 93, 97] \<noteq> uq_trim [92, 93] [92, 93, 97]) \<and>
   ((98::nat) \<notin> set [97, 45, 99] \<and> uq_class [97, 45, 99] 98 \<and>
     cutset_trim C_both [97, 45, 99] [98] \<noteq> uq_trim [97, 45, 99] [98])"
  by (simp add: cutset_trim_def uq_trim_def uq_class_def
                cutset_mem_def trim_by_def trail_drop_def)

text \<open>Row by row: `]` closes the class immediately, so the unquoted reading
  denotes the EMPTY set while the node's cutset contains `]`; `^` in first
  position negates, so the unquoted class contains every byte except `a`; a
  backslash escapes the `]`, so the unquoted class loses the backslash itself;
  and `a-c` is a range, so `b` is in the class although it is not in the cutset.\<close>

section \<open>The codepoint reading: a toy decoder and a char-wise trim\<close>

text \<open>Go's `strings.Trim` and Rust's `trim_matches(|c| cutset.contains(c))`
  quantify over CODE POINTS. This section models the two-byte subset of UTF-8
  honestly (a lead byte 0xC2..0xDF followed by a continuation byte 0x80..0xBF)
  and defines a trim over the decoded characters: a character is removable only
  if ALL of its bytes are in the cutset. That is the reading the arms implement;
  it is not the node's byte model.\<close>

datatype uchar = UAscii nat | UPair nat nat

fun decode2 :: "bytes \<Rightarrow> uchar list option" where
  "decode2 [] = Some []"
| "decode2 [a] = (if 128 \<le> a \<and> a \<le> 191 then None else Some [UAscii a])"
| "decode2 (a # b # s) =
     (if 194 \<le> a \<and> a \<le> 223 then
        (if 128 \<le> b \<and> b \<le> 191
         then (case decode2 s of None \<Rightarrow> None | Some us \<Rightarrow> Some (UPair a b # us))
         else None)
      else if 128 \<le> a \<and> a \<le> 191 then None
      else (case decode2 (b # s) of None \<Rightarrow> None | Some us \<Rightarrow> Some (UAscii a # us)))"

fun encode2 :: "uchar list \<Rightarrow> bytes" where
  "encode2 [] = []"
| "encode2 (UAscii a # us) = a # encode2 us"
| "encode2 (UPair a b # us) = a # b # encode2 us"

fun in_cutset :: "bytes \<Rightarrow> uchar \<Rightarrow> bool" where
  "in_cutset cs (UAscii a) = (a \<in> set cs)"
| "in_cutset cs (UPair a b) = (a \<in> set cs \<and> b \<in> set cs)"

fun cp_drop :: "(uchar \<Rightarrow> bool) \<Rightarrow> uchar list \<Rightarrow> uchar list" where
  "cp_drop P [] = []"
| "cp_drop P (u # us) = (if P u then cp_drop P us else u # us)"

definition cp_trim_lead :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cp_trim_lead cs s \<equiv>
     (case decode2 s of None \<Rightarrow> s | Some us \<Rightarrow> encode2 (cp_drop (in_cutset cs) us))"

definition cp_trim_trail :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cp_trim_trail cs s \<equiv>
     (case decode2 s of None \<Rightarrow> s
      | Some us \<Rightarrow> encode2 (rev (cp_drop (in_cutset cs) (rev us))))"

text \<open>A char-wise trim returns a character sequence — that is the whole
  difference from the byte model, and it is what makes the witnesses below
  one-way.\<close>

lemma cp_trim_lead_is_a_character_sequence:
  "decode2 s = Some us \<Longrightarrow>
     cp_trim_lead cs s = encode2 (cp_drop (in_cutset cs) us)"
  unfolding cp_trim_lead_def by simp

lemma cp_trim_trail_is_a_character_sequence:
  "decode2 s = Some us \<Longrightarrow>
     cp_trim_trail cs s = encode2 (rev (cp_drop (in_cutset cs) (rev us)))"
  unfolding cp_trim_trail_def by simp

text \<open>The cutset `{0xC2}` is half of U+00A0. The node's byte model removes that
  lead byte and answers `[0xA0, 0x61]` — a continuation byte with no lead byte,
  which does not decode at all — while the codepoint reading cannot remove half
  a character and leaves the text intact. The two readings therefore disagree on
  a concrete witness, and the theorem records both answers plus the fact that
  the byte model's answer has no character sequence behind it.\<close>

theorem go_trim_cutset_is_codepoints:
  "decode2 [194, 160, 97] = Some [UPair 194 160, UAscii 97] \<and>
   cutset_trim C_lead [194] [194, 160, 97] = [160, 97] \<and>
   decode2 [160, 97] = None \<and>
   cp_trim_lead [194] [194, 160, 97] = [194, 160, 97]"
  by eval

text \<open>The same at the trailing end, with the cutset `{0xA0}` — the
  CONTINUATION byte of the same character. The byte model trims it and leaves
  `[0x61, 0xC2]`; the codepoint reading cannot, and keeps the character.\<close>

theorem rust_trim_matches_is_chars:
  "decode2 [97, 194, 160] = Some [UAscii 97, UPair 194 160] \<and>
   cutset_trim C_trail [160] [97, 194, 160] = [97, 194] \<and>
   cp_trim_trail [160] [97, 194, 160] = [97, 194, 160]"
  by eval

text \<open>WHAT IS AND IS NOT MODELLED. Modelled: a bracket-body syntax (closing
  `]`, leading `^` negation, backslash escape, `a-b` ranges) and the two-byte
  subset of UTF-8 with a char-wise trim. Not modelled: Perl's real regex
  compiler, the full UTF-8 repertoire (3- and 4-byte sequences), and the
  renderers' own error behaviour on undecodable input — where `cp_trim_*` here
  returns the argument unchanged, the real arms would treat the bytes as
  replacement characters. The divergence theorems above are propositions about
  the two models, and the assumptions are listed in docs/fm/README.md.\<close>

end
