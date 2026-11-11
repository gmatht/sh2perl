(*  ShIR_TextOps_Space — WHERE the renderers' whitespace class differs from
    the shell's, stated exactly.

    Part 5 of the session (after Base, Strip, SortUniq, Compose).

    Why this file exists. `wc -w`'s meaning in Base is `word_count`, built on
    `is_ws` — POSIX space, the six ASCII bytes 9 10 11 12 13 32, which is what
    bash's `IFS` splitting uses. Five of the eight renderers do not run that
    class: Perl runs `split(/\\s+/)`, ESTree `split(/\\s+/)`, Python
    `str.split()` (no argument), Rust `split_whitespace()`, Go
    `strings.Fields`. Only C and Java (the transition scans) and the Sh
    backend stay byte-oriented. So `array_len_split_is_word_count` in Compose
    is about a class the renderers do not all use, and Compose says so in an
    open target: `\\s` is not POSIX space.

    This file closes that target the only way it can be closed honestly: by
    MEASURING each engine's class on this box and proving what the difference
    is, exactly, and what it costs. It does not model a regex engine, and it
    does not claim any engine is wrong in general — it states which byte
    strings make the emitted expression disagree with `wc_count W_words`, and
    proves the disagreement on one witness per class.

    Provenance (do not trust these tables after an upgrade; re-run the
    script): docs/fm/TextOps/measure-space.sh, which walks all of
    U+0000..U+10FFFF and prints the code points each engine calls whitespace.
    Measured on 2026-11-11 with

        perl 5.38.2 (x86_64-linux-gnu-thread-multi)
        node v22.23.3
        python 3.12.3
        rustc 1.90.0
        go (unicode.IsSpace)

    and the byte-string side measured with bash's own `wc -w`.

    What is proved:

      1. every engine's class CONTAINS POSIX space, so an engine never splits
         less than the shell does — the counts can only go up
      2. exactly where each one differs: `js_extra_locus` and friends name the
         code point sets, as equations, not prose
      3. on pure ASCII (c < 128) JS, Perl, Rust and Go agree with POSIX, so
         their `wc -w` rendering is sound on ASCII input and can only go wrong
         on a non-ASCII source — this is the analogue of the `ascii` guard the
         `-c`/`-m` lowering uses, and `WordCount_n` has none
      4. PYTHON IS THE EXCEPTION AND IT IS NOT A NON-ASCII PROBLEM:
         `str.isspace()` includes 0x1C..0x1F (FS GS RS US), so
         `len(t.split())` overcounts a PLAIN ASCII source
      5. a witness per divergence, both sides evaluated: `a<NUL-delimited>`...
         with NBSP and with 0x1C, the engine counts 2 and the shell 1
      6. consequently the emitted `ArrayLen(Split t)` is NOT
         `wc_count W_words t` for those renderers, which is the falsification
         Compose's open target asked for — so the transform is unsound there,
         not merely unproved

    Where the fix belongs is a separate question, deliberately not answered
    here: either the renderers stop using `\\s` for `-w`, or the transform
    refuses `-w` on sources that may contain the extra code points. Both need
    this file's locus statement; neither is a proof obligation.

    Proof style follows docs/fm/README.md: no `metis`, no `simp` on a
    cons/append shape, `by eval` for ground statements, explicit `auto` with
    the definitions in hand for the finite set equations.

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
*)

theory ShIR_TextOps_Space
  imports ShIR_TextOps_Compose
begin


section \<open>The measured classes\<close>

text \<open>Each list is a transcription of one engine's measured class, defined
  independently so that the relations between them (below) are checks and not
  re-definitions. The lists are sorted, and the ASCII part is first, so the
  eye can compare the six-byte prefix against \<open>is_ws\<close>.\<close>

definition posix_ws_list :: "nat list" where
  "posix_ws_list = [9, 10, 11, 12, 13, 32]"

text \<open>Unicode White_Space, as measured through rust's
  \<open>char::is_whitespace\<close>. Go's \<open>unicode.IsSpace\<close> and Perl's
  \<open>\\s\<close> under \<open>use feature "unicode_strings"\<close> measured the same
  set; \<open>rust_is_unicode\<close> and friends below are that claim as a proof
  obligation rather than a remark.\<close>

definition unicode_ws_list :: "nat list" where
  "unicode_ws_list =
     [9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196,
      8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288]"

text \<open>ECMAScript \<open>/\\s/\<close>: the Unicode set, plus U+FEFF (the BOM/zwnbsp,
  which JS treats as whitespace and Unicode does not), minus U+0085 NEL
  (which JS does not).\<close>

definition js_ws_list :: "nat list" where
  "js_ws_list =
     [9, 10, 11, 12, 13, 32, 160, 5760, 8192, 8193, 8194, 8195, 8196,
      8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288,
      65279]"

text \<open>Python's \<open>str.isspace()\<close>, which is what \<open>str.split()\<close> with no
  argument splits on: the Unicode set, plus the four ASCII separators
  0x1C..0x1F.\<close>

definition py_ws_list :: "nat list" where
  "py_ws_list =
     [9, 10, 11, 12, 13, 28, 29, 30, 31, 32, 133, 160, 5760, 8192, 8193,
      8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233,
      8239, 8287, 12288]"

text \<open>Go's \<open>unicode.IsSpace\<close> — measured identical to the Unicode set.\<close>

definition go_ws_list :: "nat list" where
  "go_ws_list =
     [9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196,
      8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288]"

text \<open>Perl's \<open>\\s\<close> with \<open>use feature "unicode_strings"\<close> (or any UTF8-flagged
  subject): measured identical to the Unicode set.\<close>

definition perl_ws_list :: "nat list" where
  "perl_ws_list =
     [9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196,
      8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288]"

text \<open>Perl's \<open>\\s\<close> in the DEFAULT (byte) string model: the same set with
  U+0085 and U+00A0 dropped. This is not a different regex — it is the same
  `\\s` applied to a string perl has not upgraded, which is why the same
  generated line can be right or wrong depending on what the data looks like
  at run time. That is the worst case for a codegen claim: the class is not a
  property of the emitted code.\<close>

definition perl_bytes_ws_list :: "nat list" where
  "perl_bytes_ws_list =
     [9, 10, 11, 12, 13, 32, 5760, 8192, 8193, 8194, 8195, 8196,
      8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288]"

definition posix_ws :: "nat \<Rightarrow> bool" where
  "posix_ws c \<equiv> c \<in> set posix_ws_list"

definition unicode_ws :: "nat \<Rightarrow> bool" where
  "unicode_ws c \<equiv> c \<in> set unicode_ws_list"

definition js_ws :: "nat \<Rightarrow> bool" where
  "js_ws c \<equiv> c \<in> set js_ws_list"

definition py_ws :: "nat \<Rightarrow> bool" where
  "py_ws c \<equiv> c \<in> set py_ws_list"

definition go_ws :: "nat \<Rightarrow> bool" where
  "go_ws c \<equiv> c \<in> set go_ws_list"

definition perl_ws :: "nat \<Rightarrow> bool" where
  "perl_ws c \<equiv> c \<in> set perl_ws_list"

definition perl_bytes_ws :: "nat \<Rightarrow> bool" where
  "perl_bytes_ws c \<equiv> c \<in> set perl_bytes_ws_list"


section \<open>The transcriptions agree with the model's POSIX class\<close>

lemma posix_lists_agree [simp]: "posix_ws c = is_ws c"
  unfolding posix_ws_def posix_ws_list_def is_ws_def by auto

text \<open>The three engines that use the Unicode property measured the same
  set. These are list equalities, so they are a transcription check: if any of
  the four tables above is retyped wrongly, one of these fails.\<close>

lemma go_measured_is_unicode: "go_ws_list = unicode_ws_list" by eval

lemma perl_unicode_measured_is_unicode: "perl_ws_list = unicode_ws_list" by eval

lemma go_ws_is_unicode [simp]: "go_ws c = unicode_ws c"
  unfolding go_ws_def unicode_ws_def by (simp add: go_measured_is_unicode)

lemma perl_ws_is_unicode [simp]: "perl_ws c = unicode_ws c"
  unfolding perl_ws_def unicode_ws_def by (simp add: perl_unicode_measured_is_unicode)


section \<open>Exactly where each class differs\<close>

text \<open>The lemma names say it: JS carries the BOM and not NEL; Python adds
  the four ASCII separators; Perl in its byte model LOSES NBSP and NEL. These
  are equations over all \<open>c\<close>, so "where" is a proved predicate, not a
  paragraph. Notice that POSIX is not one of the ranks: every engine's class
  is a SUPERSET of it, so an engine never splits less than the shell does.\<close>

lemma posix_subset_all [simp]:
  assumes "posix_ws c"
  shows "js_ws c \<and> py_ws c \<and> perl_ws c \<and> perl_bytes_ws c \<and> go_ws c
        \<and> unicode_ws c"
proof -
  from assms have "c = 9 \<or> c = 10 \<or> c = 11 \<or> c = 12 \<or> c = 13 \<or> c = 32"
    unfolding posix_ws_def posix_ws_list_def is_ws_def by simp
  thus "js_ws c \<and> py_ws c \<and> perl_ws c \<and> perl_bytes_ws c \<and> go_ws c
        \<and> unicode_ws c"
    unfolding js_ws_def py_ws_def perl_ws_def perl_bytes_ws_def go_ws_def
      unicode_ws_def js_ws_list_def py_ws_list_def perl_ws_list_def
      perl_bytes_ws_list_def go_ws_list_def unicode_ws_list_def
    by (elim disjE) simp_all
qed

lemma js_extra_locus:
  "js_ws c \<and> \<not> posix_ws c
     \<longleftrightarrow> c \<in> set [160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198,
                     8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288,
                     65279]"
  unfolding js_ws_def posix_ws_def js_ws_list_def posix_ws_list_def
  by auto

lemma py_extra_locus:
  "py_ws c \<and> \<not> posix_ws c
     \<longleftrightarrow> c \<in> set [28, 29, 30, 31, 133, 160, 5760, 8192, 8193, 8194,
                     8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232,
                     8233, 8239, 8287, 12288]"
  unfolding py_ws_def posix_ws_def py_ws_list_def posix_ws_list_def
  by auto

lemma perl_extra_locus:
  "perl_ws c \<and> \<not> posix_ws c
     \<longleftrightarrow> c \<in> set [133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197,
                     8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287,
                     12288]"
  unfolding perl_ws_def posix_ws_def perl_ws_list_def posix_ws_list_def
  by auto

lemma go_extra_locus:
  "go_ws c \<and> \<not> posix_ws c
     \<longleftrightarrow> c \<in> set [133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197,
                     8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287,
                     12288]"
  unfolding go_ws_def posix_ws_def go_ws_list_def posix_ws_list_def
  by auto

lemma perl_bytes_extra_locus:
  "perl_bytes_ws c \<and> \<not> posix_ws c
     \<longleftrightarrow> c \<in> set [5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199,
                     8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288]"
  unfolding perl_bytes_ws_def posix_ws_def perl_bytes_ws_list_def
    posix_ws_list_def
  by auto

text \<open>And the converse direction, which is what makes the two loci a
  complete answer: the byte-model Perl class is the Unicode one minus the two
  code points the upgrade decides, and Python's is the Unicode one plus the
  four ASCII separators.\<close>

lemma perl_bytes_is_unicode_minus:
  "perl_bytes_ws c \<longleftrightarrow> unicode_ws c \<and> c \<noteq> 133 \<and> c \<noteq> 160"
  unfolding perl_bytes_ws_def unicode_ws_def perl_bytes_ws_list_def
    unicode_ws_list_def
  by auto

lemma py_is_unicode_plus_ascii_separators:
  "py_ws c \<longleftrightarrow> (unicode_ws c \<or> 28 \<le> c \<and> c \<le> 31)"
  unfolding py_ws_def unicode_ws_def py_ws_list_def unicode_ws_list_def
  by auto

lemma js_is_unicode_plus_bom_minus_nel:
  "js_ws c \<longleftrightarrow> (unicode_ws c \<and> c \<noteq> 133) \<or> c = 65279"
  unfolding js_ws_def unicode_ws_def js_ws_list_def unicode_ws_list_def
  by auto

text \<open>Read the other way round, this says POSIX space is exactly the ASCII part
  of Unicode White_Space — the six bytes that are left when the model asks
  about a byte string.\<close>

lemma posix_is_the_ascii_part_of_unicode:
  "posix_ws c \<longleftrightarrow> (unicode_ws c \<and> c < 128)"
  unfolding unicode_ws_def posix_ws_def unicode_ws_list_def posix_ws_list_def
  by auto


section \<open>The ASCII guard: what it does and does not buy\<close>

text \<open>`lowered_wc` only lowers `-c`/`-m` when the source is `ascii`, because
  the renderers' `StrLen` counts code points and the node counts bytes. The
  word mode has no such guard — Base's `lowered_wc_sound` fires for every
  source. What follows is the guard such a rule WOULD need: for JS, Perl,
  Rust and Go the classes coincide with POSIX below 128, so an ASCII source is
  safe; for Python it is not, because the divergence starts at 0x1C.\<close>

lemma js_ascii_agrees: "c < 128 \<Longrightarrow> js_ws c \<longleftrightarrow> posix_ws c"
  unfolding js_ws_def posix_ws_def js_ws_list_def posix_ws_list_def
  by auto

lemma perl_bytes_ascii_agrees:
  "c < 128 \<Longrightarrow> perl_bytes_ws c \<longleftrightarrow> posix_ws c"
  unfolding perl_bytes_ws_def posix_ws_def perl_bytes_ws_list_def
    posix_ws_list_def
  by auto

lemma unicode_ascii_agrees:
  "c < 128 \<Longrightarrow> unicode_ws c \<longleftrightarrow> posix_ws c"
  unfolding unicode_ws_def posix_ws_def unicode_ws_list_def posix_ws_list_def
  by auto

lemma py_ascii_extra_is_separators:
  "c < 128 \<Longrightarrow> (py_ws c \<longleftrightarrow> (posix_ws c \<or> 28 \<le> c \<and> c \<le> 31))"
  unfolding py_ws_def posix_ws_def py_ws_list_def posix_ws_list_def
  by auto

text \<open>The separator bytes are ASCII, so the `ascii` guard of
  `lowered_wc`'s `StrLen` arm would NOT have stopped the Python divergence even
  if `WordCount_n` had been given the same guard: 0x1C is below 128.\<close>

lemma separator_bytes_are_ascii: "ascii [28, 29, 30, 31]"
  by (simp add: ascii_def)


section \<open>Counting by a class: the parameterised word scan\<close>

text \<open>Base fixes the class inside `ws_starts`; Compose re-derives the same count
  as a look-BEHIND scan (`ws_scan`, `nwords`), which is the form that suits a
  `Split`. To ask what a renderer computes, the class has to be a parameter, so
  the scan is repeated here over an arbitrary predicate. `nwords_by is_ws` is
  then Base's `word_count`, which is how every claim below stays a claim about
  the quantity the node means rather than about a convenient variant of it.\<close>

fun ws_scan_by :: "(nat \<Rightarrow> bool) \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> nat" where
  "ws_scan_by P prev [] = 0"
| "ws_scan_by P prev (c # cs) =
     (if (\<not> P c \<and> prev) then 1 else 0) + ws_scan_by P (P c) cs"

definition nwords_by :: "(nat \<Rightarrow> bool) \<Rightarrow> bytes \<Rightarrow> nat" where
  "nwords_by P s \<equiv> ws_scan_by P True s"

lemma ws_scan_by_posix:
  "ws_scan_by is_ws prev s = ws_scan prev s"
  by (induction s arbitrary: prev) (simp_all add: ws_scan.simps)

lemma nwords_by_posix:
  "int (nwords_by is_ws s) = word_count s"
  by (simp add: nwords_by_def ws_scan_by_posix word_count_is_scan)

text \<open>The congruence that turns an ASCII-agreement lemma into a counting
  lemma: if two classes agree on every byte of a string, they count it the
  same. Read contrapositively it is what makes a single witness enough to
  refute a rendering.\<close>

lemma ws_scan_by_cong:
  assumes "\<forall>c \<in> set s. P c = Q c"
  shows "ws_scan_by P prev s = ws_scan_by Q prev s"
  using assms
  by (induction s arbitrary: prev) simp_all

lemma nwords_by_cong:
  assumes "\<forall>c \<in> set s. P c = Q c"
  shows "nwords_by P s = nwords_by Q s"
  using assms unfolding nwords_by_def by (rule ws_scan_by_cong)

theorem nwords_by_posix_is_model:
  "int (nwords_by is_ws s) = wc_count W_words s"
  unfolding wc_count_def using nwords_by_posix by simp


section \<open>The consequences at the node: the emitted expression is not the node's value\<close>

text \<open>Two witnesses, one per kind of divergence. Both sides are evaluated: the
  left is what the emitted expression computes (a measured class, over the
  string the engine sees), the right is what the shell prints — the node's
  meaning over the BYTES it reads.

  The NBSP witness needs the encoding spelled out, because that is exactly why
  `wc -w` is right here: bash reads the two UTF-8 bytes 0xC2 0xA0, neither of
  which is a POSIX space, so it sees ONE word; JS decodes to U+00A0, which its
  regex `\\s` matches, so it sees TWO.\<close>

definition utf8_nbsp :: bytes where
  "utf8_nbsp = [194, 160]"

lemma utf8_nbsp_has_no_whitespace_byte:
  "\<not> is_ws 194" "\<not> is_ws 160"
  unfolding is_ws_def by simp_all

lemma shell_nbsp_is_one_word:
  "wc_count W_words [97, 194, 160, 98] = 1"
proof -
  have a: "\<not> is_ws 97" unfolding is_ws_def by simp
  have b: "\<not> is_ws 194" unfolding is_ws_def by simp
  have c: "\<not> is_ws 160" unfolding is_ws_def by simp
  have d: "\<not> is_ws 98" unfolding is_ws_def by simp
  from a b c d show ?thesis
    unfolding wc_count_def word_count_def ws_starts.simps by simp
qed

lemma js_counts_nbsp_as_separator:
  "nwords_by js_ws [97, 160, 98] = 2"
  by (simp add: nwords_by_def ws_scan_by.simps js_ws_def js_ws_list_def)

theorem js_nbsp_rendering_is_not_the_model:
  "int (nwords_by js_ws [97, 160, 98]) \<noteq> wc_count W_words [97, 194, 160, 98]"
  using js_counts_nbsp_as_separator shell_nbsp_is_one_word by simp

text \<open>The second witness needs no encoding at all, and that is the point: 0x1C
  is ASCII. A unit separator between two letters is one word to bash and two to
  Python's `str.split()`, so a proof that a rendering is right on ASCII sources
  is not available for that renderer.\<close>

lemma shell_separator_is_one_word:
  "wc_count W_words [97, 28, 98] = 1"
proof -
  have a: "\<not> is_ws 97" unfolding is_ws_def by simp
  have b: "\<not> is_ws 28" unfolding is_ws_def by simp
  have c: "\<not> is_ws 98" unfolding is_ws_def by simp
  from a b c show ?thesis
    unfolding wc_count_def word_count_def ws_starts.simps by simp
qed

lemma python_counts_separator_as_space:
  "nwords_by py_ws [97, 28, 98] = 2"
  by (simp add: nwords_by_def ws_scan_by.simps py_ws_def py_ws_list_def)

theorem python_separator_rendering_is_not_the_model:
  "int (nwords_by py_ws [97, 28, 98]) \<noteq> wc_count W_words [97, 28, 98]"
  using python_counts_separator_as_space shell_separator_is_one_word by simp

text \<open>For contrast, the engines that DO agree on ASCII get the general
  statement rather than a witness: with `js_ws` the emitted count equals the
  node's value on every ASCII source. That is why the JS renderer is only
  exposed on non-ASCII text, and Perl in its byte model on even less
  (`perl_bytes_extra_locus` still carries U+1680 and the U+2000 block).\<close>

theorem js_ascii_rendering_is_sound:
  assumes "ascii s"
  shows "int (nwords_by js_ws s) = wc_count W_words s"
proof -
  have agree: "\<forall>c \<in> set s. js_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    with js_ascii_agrees have "js_ws c = posix_ws c" by blast
    thus "js_ws c = is_ws c" by simp
  qed
  have "nwords_by js_ws s = nwords_by is_ws s"
    by (rule nwords_by_cong) (rule agree)
  with nwords_by_posix_is_model show ?thesis by simp
qed

theorem go_ascii_rendering_is_sound:
  assumes "ascii s"
  shows "int (nwords_by go_ws s) = wc_count W_words s"
proof -
  have agree: "\<forall>c \<in> set s. go_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    with unicode_ascii_agrees have "unicode_ws c = posix_ws c" by blast
    thus "go_ws c = is_ws c" by simp
  qed
  have "nwords_by go_ws s = nwords_by is_ws s"
    by (rule nwords_by_cong) (rule agree)
  with nwords_by_posix_is_model show ?thesis by simp
qed

theorem perl_unicode_ascii_rendering_is_sound:
  assumes "ascii s"
  shows "int (nwords_by perl_ws s) = wc_count W_words s"
proof -
  have agree: "\<forall>c \<in> set s. perl_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    with unicode_ascii_agrees have "unicode_ws c = posix_ws c" by blast
    thus "perl_ws c = is_ws c" by simp
  qed
  have "nwords_by perl_ws s = nwords_by is_ws s"
    by (rule nwords_by_cong) (rule agree)
  with nwords_by_posix_is_model show ?thesis by simp
qed

text \<open>The same statement does NOT hold for Python, and the witness above is
  why: the guard's own premise (`ascii`) is satisfied by the counterexample, so
  no refinement of a guard on the source fixes that renderer — the class has to
  change.\<close>

theorem python_has_no_ascii_guard:
  "\<exists>s. ascii s \<and> int (nwords_by py_ws s) \<noteq> wc_count W_words s"
  using python_separator_rendering_is_not_the_model
  by (rule_tac exI[where x="[97, 28, 98]"]) (simp add: ascii_def)

text \<open>All four results in one statement, which is the form a transform rule
  or a contract needs: the ASCII premise is sufficient for three measured
  classes and insufficient for the fourth, and the fourth is refuted by an
  ASCII string.\<close>

theorem rendering_is_sound_exactly_for_the_posix_classes:
  "(\<forall>s. ascii s \<longrightarrow> int (nwords_by js_ws s) = wc_count W_words s)
   \<and> (\<forall>s. ascii s \<longrightarrow> int (nwords_by go_ws s) = wc_count W_words s)
   \<and> (\<forall>s. ascii s \<longrightarrow> int (nwords_by perl_ws s) = wc_count W_words s)
   \<and> (\<not> (\<forall>s. ascii s \<longrightarrow> int (nwords_by py_ws s) = wc_count W_words s))"
  using js_ascii_rendering_is_sound go_ascii_rendering_is_sound
    perl_unicode_ascii_rendering_is_sound python_has_no_ascii_guard
  by blast

text \<open>What this does NOT decide, deliberately: whether the fix is to refuse
  `-w` on non-ASCII sources, to narrow each renderer to `[\t\n\v\f\r ]`, or to
  count bytes instead of words. Each option needs the two `*_extra_locus`
  lists above, and none of them is a proof obligation of this file — the
  obligation discharged here is that the current emit is not the node's value,
  and exactly on which strings.\<close>

end
