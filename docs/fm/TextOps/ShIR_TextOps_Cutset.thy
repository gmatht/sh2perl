(*  Title:      ShIR_TextOps_Cutset.thy
    The `cutset_trim` node: trim by a character SET instead of a fixed class.

    This is the generalisation of StringTrim, and it is a second theory rather
    than more rows in the first because the divergence it explains is not about
    the whitespace class at all: the node carries a CUTSET, and the arms
    disagree about what a cutset IS. The model is a byte set (matching this
    session's `bytes = nat list`), which is what the shell and Perl mean; the
    Go and Rust arms take sets of code points, and the witnesses below pin the
    point where the two readings differ.

    WHAT IS PROVED. The node is `trim_by` at the membership test (so run-only
    removal and idempotence follow from ShIR_TextOps_Strings rather than being
    re-proved), the side field is three-valued, and the model's answers on the
    bracket metacharacters and on a multi-byte character.

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
     trim_by (\<lambda>c. c \<in> set cs) (lead_of side) (trail_of side) s"
  unfolding cutset_trim_def cutset_mem_def by simp

theorem cutset_trim_is_trim_by:
  "cutset_trim side cs s = trim_by (cutset_mem cs) (lead_of side) (trail_of side) s"
  unfolding cutset_trim_def by (rule refl)

text \<open>The side field is three-valued, so a renderer's conditional chain over
  both/leading/trailing is exhaustive: no fourth case hides in the node's
  `optional_string`.\<close>

theorem cutset_side_three_valued:
  "side = C_both \<or> side = C_lead \<or> side = C_trail"
  by (cases side) simp_all

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

section \<open>Go and Rust: a set of code points over a byte model\<close>

text \<open>Go's `strings.Trim(s, cutset)` and Rust's `trim_matches(|c|
  cutset.contains(c))` take sets of CODE POINTS; the node's model is bytes. For
  ASCII the two agree, which is why every ASCII witness passes either way. The
  witnesses below are where they cannot: the byte 0xC2 is half of the UTF-8
  encoding of U+00A0 (and is not itself a character), so a byte-wise trim
  removes it and a codepoint-wise trim cannot.\<close>

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
