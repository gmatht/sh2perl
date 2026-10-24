(*  ShIR_TextOps — a formal semantics for the text-ops slice of shIR, and
    machine-checked proofs of the lowering rules that slice depends on.

    Part 1 of the slice (see ShIR_TextOps.thy for the session). RECONSTRUCTED 2026-10-24 after the original (uncommitted) file was lost;
    the lemma names and statements follow docs/fm/README.md, which listed
    them. If you have the original, prefer it.

    Scope (Tier 1, pure HOL, no AFP): the EXPRESSION nodes `StrLen`,
    `WordCount` and `RegCount` as sh2perl/src/shir_nodes/{str_len,word_count,
    reg_count}.node declare them, and the `wc` arm of `try_lower_wc` in
    sh2perl/src/transforms/text_ops.rs that emits them. Nothing here models
    shell state, streams or command execution: `wc` is a function on the
    byte string it is handed.

    Why this is worth writing down: `wc -l` once emitted `length(text) + 1`
    for a source whose trailing newline was ALREADY part of `text`, so
    `echo a b | wc -l` rendered 2 instead of 1 — and every renderer
    faithfully reproduced it, because the IR node said "line count" while
    the lowering meant "newlines in text, plus one". `wc_count` below is the
    meaning, `lowered_wc` is what the transform emits, and
    `lowered_wc_sound` proves they agree. `strlen_plus_one_is_not_line_count`
    is the pre-fix emission, stated and refuted.

    Encoding note: the nodes are byte-oriented. A string is a list of byte
    values, so a "character count" is a byte count here; the ASCII guard is
    exactly the condition under which that is the right reading for a
    renderer whose `StrLen` counts codepoints.

    Proof style: structural induction, goals kept in the form the induction
    gives them. Avoid `metis` and avoid `simp`/`auto` on a goal containing
    `dropWhile` over an appended list — both hang the search on this box
    (measured: >10 min on a lemma that takes 0.2 s written this way).

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
*)

theory ShIR_TextOps_Base
  imports Main
begin

section \<open>The domain: byte strings\<close>

type_synonym bytes = "nat list"

definition is_nl :: "nat \<Rightarrow> bool" where
  "is_nl w \<equiv> w = 10"

text \<open>POSIX \<open>[:space:]\<close> — what bash's \<open>IFS\<close> split and \<open>wc -w\<close> use.\<close>

definition is_ws :: "nat \<Rightarrow> bool" where
  "is_ws w \<equiv> w = 32 \<or> w = 9 \<or> w = 10 \<or> w = 11 \<or> w = 12 \<or> w = 13"

definition ascii :: "bytes \<Rightarrow> bool" where
  "ascii s \<equiv> (\<forall>w \<in> set s. w < 128)"

section \<open>The counts, as folds\<close>

text \<open>\<open>-l\<close>: the number of newline bytes. bash counts newline-TERMINATED
  lines, so a final unterminated line is not counted, and a source that ends
  in a newline has one more line than it has line breaks. "Number of
  newlines" is therefore the whole meaning, with no "+1".\<close>

definition nl_count :: "bytes \<Rightarrow> int" where
  "nl_count s \<equiv> int (length (filter is_nl s))"

text \<open>\<open>-w\<close>: count word STARTS — a non-whitespace byte that is either the
  first byte or follows a whitespace byte. This is the fold form of "split
  on runs of whitespace and drop the empties": it needs no intermediate
  list, which keeps the proofs short.\<close>

fun ws_starts :: "bytes \<Rightarrow> bool list" where
  "ws_starts [] = []"
| "ws_starts [c] = [\<not> is_ws c]"
| "ws_starts (c # d # rest) = ((\<not> is_ws c \<and> is_ws d) # ws_starts (d # rest))"

definition word_count :: "bytes \<Rightarrow> int" where
  "word_count s \<equiv> int (length (filter id (ws_starts s)))"

section \<open>wc's own meaning\<close>

text \<open>\<open>wc_count c s\<close> — what `wc c` prints for the byte string \<open>s\<close>, and
  the meaning each count node must denote:

    \<open>-l\<close>  newlines
    \<open>-w\<close>  words
    \<open>-c\<close>  bytes
    \<open>-m\<close>  characters (== bytes in this model; see the encoding note)

  \<open>-c\<close> and \<open>-m\<close> are kept as separate modes on purpose: the lowering is
  only allowed to use one codepoint count for both under the ASCII guard.\<close>

datatype wc_mode = W_lines | W_words | W_bytes | W_chars

definition wc_count :: "wc_mode \<Rightarrow> bytes \<Rightarrow> int" where
  "wc_count c s =
     (case c of
        W_lines \<Rightarrow> nl_count s
      | W_words \<Rightarrow> word_count s
      | W_bytes \<Rightarrow> int (length s)
      | W_chars \<Rightarrow> int (length s))"

section \<open>The lowering, as the transform spells it\<close>

datatype count_node = RegCount_nl | StrLen_n | WordCount_n

text \<open>\<open>lowered_wc m s\<close> — the node `try_lower_wc` emits, as a function of the
  mode and the source text it is handed. The source text is what the stage
  extraction produced, trailing newline included; there is no "trailing
  newline" flag here on purpose, because the bug was a second newline
  invented inside the line arm. A lowering that cannot be written as a
  function of the text alone is the shape this model rules out.\<close>

definition lowered_wc :: "wc_mode \<Rightarrow> bytes \<Rightarrow> count_node option" where
  "lowered_wc m s =
     (case m of
        W_lines \<Rightarrow> Some RegCount_nl
      | W_bytes \<Rightarrow> if ascii s then Some StrLen_n else None
      | W_chars \<Rightarrow> if ascii s then Some StrLen_n else None
      | W_words \<Rightarrow> Some WordCount_n)"

text \<open>\<open>denote\<close> — the semantics of each node: what a conforming renderer
  must print. `render_ext_expr/handlers/all_nodes.rs` implements these
  (`RegCount` → newline count, `StrLen` → length, `WordCount` → word count).\<close>

definition denote :: "count_node \<Rightarrow> bytes \<Rightarrow> int" where
  "denote n s =
     (case n of
        RegCount_nl \<Rightarrow> nl_count s
      | StrLen_n \<Rightarrow> int (length s)
      | WordCount_n \<Rightarrow> word_count s)"

section \<open>Obligation 1: the lowering is sound\<close>

theorem lowered_wc_sound:
  assumes "lowered_wc m s = Some n"
  shows "denote n s = wc_count m s"
  using assms
  by (cases m)
     (auto simp: lowered_wc_def denote_def wc_count_def split: count_node.split if_split_asm)

section \<open>Obligation 2: the newline count is the source, and nothing more\<close>

text \<open>Taken from bash: `echo a b | wc -l` is 1. The source the transform is
  handed is "a b" followed by the newline echo writes.\<close>

lemma echo_a_b_line_count:
  "wc_count W_lines [97, 32, 98, 10] = 1"
  unfolding wc_count_def nl_count_def is_nl_def by simp

text \<open>`echo -n x | wc -l` is 0: no newline, no count, and no node may
  invent one.\<close>

lemma echo_n_x_line_count:
  "wc_count W_lines [120] = 0"
  unfolding wc_count_def nl_count_def is_nl_def by simp

text \<open>The pre-fix emission, stated so that it fails. The transform counted
  `length(text) + 1` where bash counts newlines: on the echo source those
  differ by one, so that emission was not a lowering of `wc_count W_lines`.\<close>

theorem strlen_plus_one_is_not_line_count:
  "int (length [97, 32, 98, 10]) + 1 \<noteq> wc_count W_lines [97, 32, 98, 10]"
  using echo_a_b_line_count by simp

text \<open>And the general shape behind it: any source whose newline count is not
  its length is miscounted by `length + 1`, so the line arm must be the
  count, not a length.\<close>

lemma line_count_is_not_length_plus_one:
  assumes "nl_count s < int (length s) + 1"
  shows "int (length s) + 1 \<noteq> wc_count W_lines s"
  using assms by (simp add: wc_count_def)

section \<open>Obligation 3: the plain-ASCII guard\<close>

text \<open>`wc -c` counts BYTES. The `StrLen` renderers are codepoint lengths
  (`t.length`, `length($t)`, `t.len()`, …), which agree with the byte count
  where bytes and characters have the same cardinality — ASCII. The transform
  refuses the reduction otherwise.\<close>

lemma ascii_nil [simp]: "ascii []" by (simp add: ascii_def)

lemma ascii_cons [simp]: "ascii (w # ws) = (w < 128 \<and> ascii ws)"
  by (simp add: ascii_def)

text \<open>The guard never costs a correct reduction: on ASCII, byte mode IS a
  StrLen.\<close>

lemma ascii_bytes_is_strlen:
  assumes "ascii s"
  shows "lowered_wc W_bytes s = Some StrLen_n"
  using assms by (simp add: lowered_wc_def)

text \<open>And it is not vacuous: a non-ASCII source is REFUSED, not mis-counted.\<close>

lemma non_ascii_bytes_refused:
  assumes "\<not> ascii s"
  shows "lowered_wc W_bytes s = None"
  using assms by (simp add: lowered_wc_def)

text \<open>A concrete non-ASCII byte string — UTF-8 for e-acute — is refused.
  (`printf '\<eacute>' | wc -c` is 2; a codepoint length would say 1.)\<close>

lemma utf8_e_acute_refused:
  "lowered_wc W_bytes [0xC3, 0xA9] = None"
  by (simp add: lowered_wc_def ascii_def)

section \<open>Obligation 4: the word count at the shell's edges\<close>

text \<open>bash's own answers for `wc -w`, as worked lemmas: the word-start fold
  agrees at the boundaries the corpus actually hits (leading, trailing and
  doubled whitespace; newlines count as whitespace).\<close>

lemma words_empty: "wc_count W_words [] = 0"
  unfolding wc_count_def word_count_def ws_starts.simps
  by simp

lemma words_one: "wc_count W_words [97] = 1"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def
  by simp

lemma words_two: "wc_count W_words [97, 32, 98] = 2"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def
  by simp

lemma words_leading_space: "wc_count W_words [32, 32, 97] = 1"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def
  by simp

lemma words_trailing_space: "wc_count W_words [97, 32, 32] = 1"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def
  by simp

lemma words_doubled_space: "wc_count W_words [97, 32, 32, 98] = 2"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def
  by simp

lemma words_newline_separates: "wc_count W_words [97, 10, 98] = 2"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def
  by simp

lemma words_all_space: "wc_count W_words [32, 10, 32] = 0"
  unfolding wc_count_def word_count_def ws_starts.simps is_ws_def
  by simp

end
