(*  ShIR_TextOps_Strings — the STRING-ALGEBRA nodes of the text-ops slice:
    `StringTrim`, `StringAffix` and `AffixStrip`.

    Sixth theory of the session (after Base, Strip, SortUniq, Compose, Space).
    Still no shell state: a string is a byte string and each node is a total
    function on it, so this is the algebraic tier — what the three nodes MEAN,
    and exactly which emits are not their value.

    What is settled here, in order:

      1. the three nodes as functions (`string_trim`, `has_affix`,
         `affix_strip_node`), on top of one generic run-drop
         (`lead_drop`/`trail_drop`/`trim_by`), which is total and defined by
         structural recursion on the byte string;
      2. the algebra: a trim is IDEMPOTENT (all four flag pairs), it removes a
         RUN and nothing else (the interior is untouched), and an affix is
         stable under appending on the side away from the pattern;
      3. `AffixStrip` is `StringAffix` plus a drop — proved as the shape the
         renderers emit (`if affix then slice else t`) — and it strips ONE
         occurrence, so it is NOT `trim_start_matches` and not idempotent;
      4. StringAffix's trailing end: the existential `(\<exists>r. t = r @ p)`
         that `endsWith` computes is the node, and the Perl arm
         (`substr($t, -length($p)) eq $p`) equals it EXACTLY for non-empty
         patterns and fails at the empty pattern, which is a measured fact
         (`perl_suffix_test`) and a proved witness;
      5. the trim classes are MEASURED, not read off the renderers' comments:
         `measure-trim.sh` walks every code point through each renderer and the
         lists below are its transcription. The four coincidences
         (JS = `js_ws`, Python = `py_ws`, Rust = Go = Unicode White_Space) and
         the two new classes (Java's `Character.isWhitespace`, Zig's literal
         " \t\n") are separate tables, so the equalities below are CHECKS;
      6. consequently the source comment claiming Python's `strip()` uses "a
         FIXED whitespace set (space \t \n \r \v \f)" is FALSE, and Python and
         Java therefore have no ASCII guard at all (0x1C is ASCII), while Zig
         is unsound on plain ASCII because it trims three bytes of six;
      7. the ESTree, Go, Rust and Zig arms ignore `leading`/`trailing` (they
         always trim both ends), so a one-sided `StringTrim` would be
         mis-rendered by them. The emitting transforms today only ever build
         `leading: true, trailing: true` (src/transforms/text_ops.rs), so this
         is a latent contract gap and it is stated as such: the renderer
         denotes `trim_by P True True`, whatever the node's flags say.

    Not settled here, deliberately: what to do about any of it. The witnesses
    say the emit is not the node's value and on which strings; whether the fix
    is a refusal, a class change or a new transform is a design decision (see
    the open targets at the end of the file).

    Proof style follows docs/fm/README.md: structural induction (or
    `rev_induct` for the trailing end), explicit cases, no `metis`, no `simp`
    on a cons/append shape where the hypothesis would have to be re-derived,
    and `by eval` for every ground statement.

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
    Measurements: ./measure-trim.sh (this directory), output quoted in words
    where a table alone cannot show it (the `$`-anchored run strip, the empty
    pattern).
*)

theory ShIR_TextOps_Strings
  imports ShIR_TextOps_Space
begin


section \<open>One generic run-drop, and the trim built from it\<close>

text \<open>`lead_drop` is the whole of a leading trim: it is total, it recurses on
  the tail, and it stops at the first byte outside the class. Everything else
  in this file is either this function at another class, its mirror image, or
  the affix algebra. The class is a parameter on purpose — the node uses
  `is_ws`, and each renderer uses its own measured class, so a class difference
  is a congruence lemma away from being a value difference.\<close>

fun lead_drop :: "(nat \<Rightarrow> bool) \<Rightarrow> bytes \<Rightarrow> bytes" where
  "lead_drop P [] = []"
| "lead_drop P (c # cs) = (if P c then lead_drop P cs else c # cs)"

text \<open>The trailing end is the same function on the reversed string. Defining
  it this way (rather than with a look-behind recursion) is what makes its
  coinductive-looking lemmas structural: `rev_induct` is the induction that
  matches it, and every proof below that touches the trailing end uses it.\<close>

definition trail_drop :: "(nat \<Rightarrow> bool) \<Rightarrow> bytes \<Rightarrow> bytes" where
  "trail_drop P s \<equiv> rev (lead_drop P (rev s))"

text \<open>`trim_by` is the node: drop a leading run if `pre`, then a trailing run
  if `suf`, over the result. The two flags are independent, which is exactly
  what four of the six renderers do not implement.\<close>

definition trim_by :: "(nat \<Rightarrow> bool) \<Rightarrow> bool \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "trim_by P pre suf s \<equiv>
     (let t = (if pre then lead_drop P s else s)
      in if suf then trail_drop P t else t)"

definition string_trim :: "bool \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "string_trim pre suf s \<equiv> trim_by is_ws pre suf s"


subsection \<open>Lemmas about the run drop, at any class\<close>

lemma lead_drop_nil [simp]: "lead_drop P [] = []"
  by (rule lead_drop.simps(1))

lemma lead_drop_cons [simp]:
  "lead_drop P (c # cs) = (if P c then lead_drop P cs else c # cs)"
  by (rule lead_drop.simps(2))

text \<open>Dropping a run twice is dropping it once: the result of a drop either
  starts with a byte outside the class or is empty. This is the only reason a
  trim can be idempotent, so it is proved first.\<close>

lemma lead_drop_idem: "lead_drop P (lead_drop P s) = lead_drop P s"
  by (induction s) (simp_all split: if_splits)

lemma lead_drop_head:
  assumes "lead_drop P s \<noteq> []"
  shows "\<not> P (hd (lead_drop P s))"
  using assms by (induction s) (auto split: if_splits)

lemma lead_drop_all_P:
  assumes "\<forall>c \<in> set s. P c"
  shows "lead_drop P s = []"
  using assms by (induction s) simp_all

text \<open>And the converse: a drop that returns nothing has consumed an all-`P`
  string. Together with the head lemma this is the complete characterisation
  of what was removed — a run, not a filter.\<close>

lemma lead_drop_nil_all:
  assumes "lead_drop P s = []"
  shows "\<forall>c \<in> set s. P c"
  using assms by (induction s) (simp_all split: if_splits)

lemma lead_drop_suffix:
  "\<exists>r. s = r @ lead_drop P s"
proof (induction s)
  case Nil
  show ?case by simp
next
  case (Cons a s)
  show ?case
  proof (cases "P a")
    case True
    from Cons.IH obtain r where r: "s = r @ lead_drop P s" by blast
    have eq: "a # s = (a # r) @ lead_drop P (a # s)"
    proof -
      have l: "lead_drop P (a # s) = lead_drop P s" using True by simp
      have "a # s = (a # r) @ lead_drop P s" using r by simp
      thus ?thesis using l by simp
    qed
    from eq show ?thesis by (rule exI)
  next
    case False
    have eq: "a # s = [] @ lead_drop P (a # s)"
    proof -
      have "lead_drop P (a # s) = a # s" using False by simp
      thus ?thesis by simp
    qed
    from eq show ?thesis by (rule exI)
  qed
qed

lemma lead_drop_subset: "set (lead_drop P s) \<subseteq> set s"
  by (induction s) (auto split: if_splits)

text \<open>The trailing-end mirror image of each of the above, by transfer through
  `rev`. Keeping them as separate lemmas means the node-level proofs never
  have to reason about a reversal in the middle of an argument.\<close>

lemma trail_drop_nil [simp]: "trail_drop P [] = []"
  unfolding trail_drop_def by simp

lemma trail_drop_idem: "trail_drop P (trail_drop P s) = trail_drop P s"
  unfolding trail_drop_def by (simp add: lead_drop_idem)

lemma trail_drop_prefix: "\<exists>r. s = trail_drop P s @ r"
proof -
  obtain r where r: "rev s = r @ lead_drop P (rev s)"
    using lead_drop_suffix by blast
  have "s = rev (rev s)" by simp
  also have "\<dots> = rev (r @ lead_drop P (rev s))" using r by (rule arg_cong)
  also have "\<dots> = rev (lead_drop P (rev s)) @ rev r" by (rule rev_append)
  finally show ?thesis unfolding trail_drop_def by blast
qed

lemma trail_drop_nil_all:
  assumes "trail_drop P s = []"
  shows "\<forall>c \<in> set s. P c"
proof -
  have lead: "lead_drop P (rev s) = []"
    using assms unfolding trail_drop_def by simp
  have "\<forall>c \<in> set (rev s). P c" by (rule lead_drop_nil_all) (rule lead)
  thus ?thesis by simp
qed

lemma trail_drop_nonempty:
  assumes "x \<noteq> []" and "\<not> P (hd x)"
  shows "trail_drop P x \<noteq> []"
proof
  assume h: "trail_drop P x = []"
  have all: "\<forall>c \<in> set x. P c" by (rule trail_drop_nil_all) (rule h)
  from assms(1) obtain a as where aas: "x = a # as" by (cases x) auto
  from all aas have "P a" by simp
  with assms(2) aas show False by simp
qed

text \<open>If the first byte is outside the class, stripping the trailing end
  cannot expose it: a trailing drop returns a PREFIX (or nothing, which the
  first-byte premise excludes), so the leading byte survives. This is the step
  that makes the two-sided trim idempotent — the second pass has nothing to do
  at the front because the first pass already stopped at a non-member.\<close>

lemma lead_drop_trail_drop:
  assumes "x \<noteq> []" and "\<not> P (hd x)"
  shows "lead_drop P (trail_drop P x) = trail_drop P x"
proof -
  obtain r where r: "x = trail_drop P x @ r" using trail_drop_prefix by blast
  have ne: "trail_drop P x \<noteq> []" using assms trail_drop_nonempty by blast
  from ne obtain a as where aas: "trail_drop P x = a # as"
    by (cases "trail_drop P x") auto
  have hx: "hd x = a"
  proof -
    have "x = a # (as @ r)" using r aas by simp
    thus ?thesis by simp
  qed
  with aas have "\<not> P a" using assms by simp
  thus ?thesis using aas by simp
qed

subsection \<open>The trim's algebra\<close>

lemma trim_by_none [simp]: "trim_by P False False s = s"
  unfolding trim_by_def by simp

lemma trim_by_pre [simp]: "trim_by P True False s = lead_drop P s"
  unfolding trim_by_def by simp

lemma trim_by_suf [simp]: "trim_by P False True s = trail_drop P s"
  unfolding trim_by_def by simp

lemma trim_by_both [simp]: "trim_by P True True s = trail_drop P (lead_drop P s)"
  unfolding trim_by_def by simp

theorem trim_by_idem_both:
  "trim_by P True True (trim_by P True True s) = trim_by P True True s"
proof (cases "lead_drop P s = []")
  case True
  hence "trim_by P True True s = []" by simp
  thus ?thesis by simp
next
  case False
  have ne: "lead_drop P s \<noteq> []" using False by blast
  have np: "\<not> P (hd (lead_drop P s))" using lead_drop_head ne by blast
  have step: "lead_drop P (trail_drop P (lead_drop P s))
                = trail_drop P (lead_drop P s)"
    by (rule lead_drop_trail_drop) (simp_all add: False np)
  have "trim_by P True True (trim_by P True True s)
          = trail_drop P (trail_drop P (lead_drop P s))" using step by simp
  also have "\<dots> = trail_drop P (lead_drop P s)" by (simp add: trail_drop_idem)
  also have "\<dots> = trim_by P True True s" by simp
  finally show ?thesis .
qed

lemma trim_by_idem_pre:
  "trim_by P True False (trim_by P True False s) = trim_by P True False s"
  by (simp add: lead_drop_idem)

lemma trim_by_idem_suf:
  "trim_by P False True (trim_by P False True s) = trim_by P False True s"
  by (simp add: trail_drop_idem)

text \<open>Class congruence: two classes that agree on every byte of a string trim
  it identically. This is the lemma that turns each "measured class = the
  model's class" table below into a statement about trim VALUES, and the
  contrapositive that makes a single witness refute a rendering.\<close>

lemma lead_drop_cong:
  assumes "\<forall>c \<in> set s. P c = Q c"
  shows "lead_drop P s = lead_drop Q s"
  using assms by (induction s) (simp_all split: if_splits)

lemma trail_drop_cong:
  assumes "\<forall>c \<in> set s. P c = Q c"
  shows "trail_drop P s = trail_drop Q s"
proof -
  have "\<forall>c \<in> set (rev s). P c = Q c" using assms by simp
  hence "lead_drop P (rev s) = lead_drop Q (rev s)" by (rule lead_drop_cong)
  thus ?thesis unfolding trail_drop_def by simp
qed

lemma trim_by_cong:
  assumes agree: "\<forall>c \<in> set s. P c = Q c"
  shows "trim_by P pre suf s = trim_by Q pre suf s"
proof (cases pre)
  case True
  note pre = True
  have lead_eq: "lead_drop P s = lead_drop Q s"
    using agree by (rule lead_drop_cong)
  have agree_lead: "\<forall>c \<in> set (lead_drop P s). P c = Q c"
  proof (rule ballI)
    fix c
    assume "c \<in> set (lead_drop P s)"
    with lead_drop_subset have "c \<in> set s" by blast
    with agree show "P c = Q c" by blast
  qed
  show ?thesis
  proof (cases suf)
    case True
    have "trim_by P pre suf s = trail_drop P (lead_drop P s)"
      using pre True by simp
    also have "\<dots> = trail_drop Q (lead_drop P s)"
      using agree_lead by (rule trail_drop_cong)
    also have "\<dots> = trail_drop Q (lead_drop Q s)" by (simp add: lead_eq)
    also have "\<dots> = trim_by Q pre suf s" using pre True by simp
    finally show ?thesis .
  next
    case False
    have "trim_by P pre suf s = lead_drop P s" using pre False by simp
    also have "\<dots> = lead_drop Q s" using lead_eq .
    also have "\<dots> = trim_by Q pre suf s" using pre False by simp
    finally show ?thesis .
  qed
next
  case False
  note pre = False
  show ?thesis
  proof (cases suf)
    case True
    have "trim_by P pre suf s = trail_drop P s" using pre True by simp
    also have "\<dots> = trail_drop Q s" using agree by (rule trail_drop_cong)
    also have "\<dots> = trim_by Q pre suf s" using pre True by simp
    finally show ?thesis .
  next
    case False
    thus ?thesis using pre by simp
  qed
qed

text \<open>The node: four flag pairs, four unfoldings, and idempotence at each.\<close>

lemma string_trim_none: "string_trim False False s = s"
  unfolding string_trim_def by simp

lemma string_trim_pre: "string_trim True False s = lead_drop is_ws s"
  unfolding string_trim_def by simp

lemma string_trim_suf: "string_trim False True s = trail_drop is_ws s"
  unfolding string_trim_def by simp

theorem string_trim_both:
  "string_trim True True s = trail_drop is_ws (lead_drop is_ws s)"
  unfolding string_trim_def by simp

lemma string_trim_idem_both [simp]:
  "string_trim True True (string_trim True True s) = string_trim True True s"
  unfolding string_trim_def by (subst trim_by_idem_both) simp

lemma string_trim_idem_pre [simp]:
  "string_trim True False (string_trim True False s) = string_trim True False s"
  unfolding string_trim_def by (subst trim_by_idem_pre) simp

lemma string_trim_idem_suf [simp]:
  "string_trim False True (string_trim False True s) = string_trim False True s"
  unfolding string_trim_def by (subst trim_by_idem_suf) simp

theorem string_trim_idem:
  "string_trim pre suf (string_trim pre suf s) = string_trim pre suf s"
proof (cases pre)
  case True
  have peq: "pre = True" using True by simp
  show ?thesis
  proof (cases suf)
    case True
    have seq: "suf = True" using True by simp
    show ?thesis using peq seq by simp
  next
    case False
    have seq: "suf = False" using False by simp
    show ?thesis using peq seq by simp
  qed
next
  case False
  have peq: "pre = False" using False by simp
  show ?thesis
  proof (cases suf)
    case True
    have seq: "suf = True" using True by simp
    show ?thesis using peq seq by simp
  next
    case False
    have seq: "suf = False" using False by simp
    show ?thesis using peq seq by (simp add: string_trim_none)
  qed
qed

text \<open>A trim removes a run from each end and NOTHING ELSE. Stated as an
  equation on a string whose first byte is not whitespace: the leading strip is
  the identity there, so any change has to come from the trailing end only.
  `string_trim_interior_kept` below is the working version of the same claim at
  the node.\<close>

lemma lead_drop_untouched:
  assumes "x \<noteq> []" and "\<forall>c \<in> set x. \<not> P c"
  shows "lead_drop P (x @ y) = x @ y"
proof (cases x)
  case Nil
  with assms show ?thesis by simp
next
  case (Cons a xs)
  from assms(2) Cons have "\<not> P a" by simp
  with Cons show ?thesis by simp
qed

lemma trail_drop_untouched:
  assumes "x \<noteq> []" and "\<forall>c \<in> set x. \<not> P c"
  shows "trail_drop P (y @ x) = y @ x"
proof -
  have rev_all: "\<forall>c \<in> set (rev x). \<not> P c" using assms by simp
  have ne: "rev x \<noteq> []" using assms(1) by (cases x) simp_all
  have step: "lead_drop P (rev x @ rev y) = rev x @ rev y"
    using lead_drop_untouched ne rev_all by blast
  have "trail_drop P (y @ x) = rev (lead_drop P (rev x @ rev y))"
    unfolding trail_drop_def by simp
  also have "\<dots> = rev (rev x @ rev y)" using step by simp
  also have "\<dots> = y @ x" by simp
  finally show ?thesis .
qed

text \<open>Ground instances, all evaluated: the interior survives, each flag keeps
  the end it was not asked to touch, and a multi-byte non-space is a byte to
  the node even when it is a space to an engine.\<close>

lemma string_trim_all_space: "string_trim True True [32, 10, 32] = []"
  by eval

lemma string_trim_interior_kept:
  "string_trim True True [32, 97, 10, 98, 32] = [97, 10, 98]"
  by eval

lemma string_trim_lead_only_keeps_trailing:
  "string_trim True False [32, 97, 32] = [97, 32]"
  by eval

lemma string_trim_trail_only_keeps_leading:
  "string_trim False True [32, 97, 32] = [32, 97]"
  by eval

lemma string_trim_utf8_nbsp_is_not_space:
  "string_trim True True [194, 160, 97, 194, 160] = [194, 160, 97, 194, 160]"
  by eval

text \<open>The Perl trailing idiom is `s/\s+$//`. `$` also matches before a FINAL
  newline, which is the classic over-match; measured (`measure-trim.sh`,
  `perl.txt`) it does not fire here, because a greedy `\s+` that reaches the
  final newline keeps going to the true end. The two ground facts below are
  the model matching the measurement byte for byte: the whole run goes, and a
  run that is not at the end stays.\<close>

lemma perl_trail_strip_removes_the_whole_run:
  "trail_drop perl_ws [97, 32, 10, 10] = [97]"
  by eval

lemma perl_trail_strip_leaves_an_interior_run:
  "trail_drop perl_ws [97, 32, 32, 10, 98] = [97, 32, 32, 10, 98]"
  by eval


section \<open>The trim classes, measured\<close>

text \<open>Each list is the transcription of `measure-trim.sh`'s output for one
  renderer — every code point 0..0x10FFFF pushed through the engine's trim with
  the engine's own version string printed alongside. They are defined here
  separately from the Space theory's `\<open>*_ws_list\<close>` tables on purpose: the
  equalities proved below then say "the class `trim()` removes is the class
  `\\s` matches", which is a fact about two independent measurements, not a
  re-definition.\<close>

definition js_trim_list :: "nat list" where
  "js_trim_list =
     [9, 10, 11, 12, 13, 32, 160, 5760, 8192, 8193, 8194, 8195, 8196,
      8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288,
      65279]"

definition py_trim_list :: "nat list" where
  "py_trim_list =
     [9, 10, 11, 12, 13, 28, 29, 30, 31, 32, 133, 160, 5760, 8192, 8193,
      8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233,
      8239, 8287, 12288]"

definition trim_unicode_list :: "nat list" where
  "trim_unicode_list =
     [9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196,
      8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288]"

text \<open>Java's `String.strip()` is `Character.isWhitespace`, which is a FOURTH
  class: the Unicode set without the non-breaking code points (U+0085, U+00A0,
  U+2007, U+202F) and with the four ASCII separators 0x1C..0x1F. It is
  measured here and has no counterpart in the Space theory, because Java's
  `-w` rendering goes through a helper rather than a regex class.\<close>

definition java_ws_list :: "nat list" where
  "java_ws_list =
     [9, 10, 11, 12, 13, 28, 29, 30, 31, 32, 5760, 8192, 8193, 8194, 8195,
      8196, 8197, 8198, 8200, 8201, 8202, 8232, 8233, 8287, 12288]"

text \<open>Zig is not a measured class of the same kind: the renderer passes the
  LITERAL `" \t\n"` to `std.mem.trim(u8, t, ...)`. The set below is that
  literal, and the point of listing it is that it is three of the six POSIX
  bytes.\<close>

definition zig_ws_list :: "nat list" where
  "zig_ws_list = [9, 10, 32]"

definition js_trim_ws :: "nat \<Rightarrow> bool" where
  "js_trim_ws c \<equiv> c \<in> set js_trim_list"

definition py_trim_ws :: "nat \<Rightarrow> bool" where
  "py_trim_ws c \<equiv> c \<in> set py_trim_list"

definition rust_trim_ws :: "nat \<Rightarrow> bool" where
  "rust_trim_ws c \<equiv> c \<in> set trim_unicode_list"

definition java_ws :: "nat \<Rightarrow> bool" where
  "java_ws c \<equiv> c \<in> set java_ws_list"

definition zig_ws :: "nat \<Rightarrow> bool" where
  "zig_ws c \<equiv> c \<in> set zig_ws_list"

text \<open>The transcription checks. Four renderers coincide with a class the
  Space theory already measured; if a digit in any table above is retyped
  wrongly, one of these list equalities fails at build time.\<close>

lemma js_trim_measured_is_js_ws: "js_trim_list = js_ws_list" by eval

lemma py_trim_measured_is_py_ws: "py_trim_list = py_ws_list" by eval

lemma trim_measured_is_unicode: "trim_unicode_list = unicode_ws_list" by eval

lemma js_trim_ws_is_js_ws [simp]: "js_trim_ws c = js_ws c"
  unfolding js_trim_ws_def js_ws_def by (simp add: js_trim_measured_is_js_ws)

lemma py_trim_ws_is_py_ws [simp]: "py_trim_ws c = py_ws c"
  unfolding py_trim_ws_def py_ws_def by (simp add: py_trim_measured_is_py_ws)

lemma rust_trim_ws_is_unicode [simp]: "rust_trim_ws c = unicode_ws c"
  unfolding rust_trim_ws_def unicode_ws_def by (simp add: trim_measured_is_unicode)

lemma go_trim_measured_is_unicode: "go_ws_list = trim_unicode_list"
  by (simp add: trim_measured_is_unicode go_measured_is_unicode)

text \<open>Java and the POSIX class, as an equation over all `c`: the extra locus
  is the four ASCII separators, and the missing one is the four non-breaking
  code points. The second form is the one a transform rule can use — the
  divergence is entirely inside ASCII.\<close>

lemma java_is_unicode_minus_nonbreaking_plus_separators:
  "java_ws c \<longleftrightarrow>
     (unicode_ws c \<and> c \<noteq> 133 \<and> c \<noteq> 160 \<and> c \<noteq> 8199 \<and> c \<noteq> 8239)
     \<or> (28 \<le> c \<and> c \<le> 31)"
  unfolding java_ws_def unicode_ws_def java_ws_list_def unicode_ws_list_def
  by auto

lemma java_ascii_agrees:
  "c < 128 \<Longrightarrow> (java_ws c \<longleftrightarrow> (is_ws c \<or> (28 \<le> c \<and> c \<le> 31)))"
  unfolding java_ws_def is_ws_def java_ws_list_def by auto

lemma zig_is_the_renderer_literal:
  "zig_ws c \<longleftrightarrow> c \<in> set [32, 9, 10]"
  unfolding zig_ws_def zig_ws_list_def by auto

lemma zig_is_posix_minus_cr_vt_ff:
  "zig_ws c \<longleftrightarrow> (is_ws c \<and> c \<noteq> 11 \<and> c \<noteq> 12 \<and> c \<noteq> 13)"
  unfolding zig_ws_def is_ws_def zig_ws_list_def by auto


section \<open>What each renderer denotes, and where it is not the node\<close>

text \<open>One definition per arm, each `trim_by` at the class the measurement
  gave. The ones that take no flags are the point: `estree`, `go`, `rust` and
  `zig` call a both-ends primitive, so their value is a function of the string
  alone. Perl, Python and Java select the primitive from the flags, which is
  why their models carry the flags.

  Perl appears twice because its class is not a property of the emitted line:
  `perl_ws` is the UTF8-upgraded string model, `perl_bytes_ws` the default
  byte model (Space's `perl_bytes_is_unicode_minus`).\<close>

definition estree_trim :: "bytes \<Rightarrow> bytes" where
  "estree_trim s \<equiv> trim_by js_ws True True s"

definition go_trim :: "bytes \<Rightarrow> bytes" where
  "go_trim s \<equiv> trim_by unicode_ws True True s"

definition rust_trim :: "bytes \<Rightarrow> bytes" where
  "rust_trim s \<equiv> trim_by unicode_ws True True s"

definition zig_trim :: "bytes \<Rightarrow> bytes" where
  "zig_trim s \<equiv> trim_by zig_ws True True s"

definition java_trim :: "bool \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "java_trim pre suf s \<equiv> trim_by java_ws pre suf s"

definition python_trim :: "bool \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "python_trim pre suf s \<equiv> trim_by py_ws pre suf s"

definition perl_trim :: "bool \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "perl_trim pre suf s \<equiv> trim_by perl_ws pre suf s"

definition perl_bytes_trim :: "bool \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "perl_bytes_trim pre suf s \<equiv> trim_by perl_bytes_ws pre suf s"

subsection \<open>The ASCII guard: three renderers have one, three do not\<close>

text \<open>For Perl (both models' ASCII part), Go and Rust, and JS, every byte
  below 128 is a class member exactly when it is a POSIX space, so on an ASCII
  source the rendering IS the node — this is the guard `lowered_wc` uses for
  `-c`/`-m`, available here for `StringTrim`.\<close>

theorem estree_trim_ascii_is_sound:
  assumes "ascii s"
  shows "estree_trim s = string_trim True True s"
proof -
  have agree: "\<forall>c \<in> set s. js_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    with js_ascii_agrees have "js_ws c = posix_ws c" by blast
    thus "js_ws c = is_ws c" by simp
  qed
  have "estree_trim s = trim_by is_ws True True s"
    unfolding estree_trim_def by (rule trim_by_cong) (rule agree)
  thus ?thesis unfolding string_trim_def by simp
qed

theorem go_trim_ascii_is_sound:
  assumes "ascii s"
  shows "go_trim s = string_trim True True s"
proof -
  have agree: "\<forall>c \<in> set s. unicode_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    with unicode_ascii_agrees have "unicode_ws c = posix_ws c" by blast
    thus "unicode_ws c = is_ws c" by simp
  qed
  have "go_trim s = trim_by is_ws True True s"
    unfolding go_trim_def by (rule trim_by_cong) (rule agree)
  thus ?thesis unfolding string_trim_def by simp
qed

theorem rust_trim_ascii_is_sound:
  assumes "ascii s"
  shows "rust_trim s = string_trim True True s"
proof -
  have agree: "\<forall>c \<in> set s. unicode_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    with unicode_ascii_agrees have "unicode_ws c = posix_ws c" by blast
    thus "unicode_ws c = is_ws c" by simp
  qed
  have "rust_trim s = trim_by is_ws True True s"
    unfolding rust_trim_def by (rule trim_by_cong) (rule agree)
  thus ?thesis unfolding string_trim_def by simp
qed

text \<open>Perl is sound under the guard at EVERY flag pair, because its renderer
  is the only one that selects the primitive from the flags. That is the
  strongest statement in this section, and the reason the Perl arm is the one
  the corpus exercises.\<close>

theorem perl_trim_ascii_is_sound:
  assumes "ascii s"
  shows "perl_trim pre suf s = string_trim pre suf s"
  and "perl_bytes_trim pre suf s = string_trim pre suf s"
proof -
  have agree: "\<forall>c \<in> set s. perl_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    with unicode_ascii_agrees have "unicode_ws c = posix_ws c" by blast
    thus "perl_ws c = is_ws c" by simp
  qed
  have agree_bytes: "\<forall>c \<in> set s. perl_bytes_ws c = is_ws c"
  proof (rule ballI)
    fix c
    assume "c \<in> set s"
    with assms have "c < 128" unfolding ascii_def by blast
    hence "perl_bytes_ws c = unicode_ws c"
      using perl_bytes_is_unicode_minus by simp
    with unicode_ascii_agrees `c < 128` have "perl_bytes_ws c = posix_ws c"
      by simp
    thus "perl_bytes_ws c = is_ws c" by simp
  qed
  have "perl_trim pre suf s = trim_by is_ws pre suf s"
    unfolding perl_trim_def by (rule trim_by_cong) (rule agree)
  with string_trim_def show "perl_trim pre suf s = string_trim pre suf s" by simp
  have "perl_bytes_trim pre suf s = trim_by is_ws pre suf s"
    unfolding perl_bytes_trim_def by (rule trim_by_cong) (rule agree_bytes)
  with string_trim_def show "perl_bytes_trim pre suf s = string_trim pre suf s"
    by simp
qed

text \<open>Python has NO such guard, and the reason is not exotic encoding: 0x1C
  is plain ASCII and Python's `strip()` removes it while the shell does not.
  The witness is ASCII on both sides, so no refinement of the `ascii` premise
  can rescue the rendering — the class has to change.\<close>

lemma python_trim_drops_the_separator: "python_trim True True [28, 97, 28] = [97]"
  by eval

theorem python_trim_has_no_ascii_guard:
  "\<exists>s. ascii s \<and> python_trim True True s \<noteq> string_trim True True s"
proof -
  have "ascii [28, 97, 28]" unfolding ascii_def by simp
  moreover have "python_trim True True [28, 97, 28] \<noteq> string_trim True True [28, 97, 28]"
    using python_trim_drops_the_separator by eval
  ultimately show ?thesis by blast
qed

text \<open>The renderer's own comment says Python "removes a FIXED whitespace set
  (space, \t, \n, \r, \v, \f) — bash's default IFS". That set is POSIX space,
  i.e. `is_ws`; the measurement says Python's class is `py_ws`. So the comment
  describes a different function from the one the line calls, and the
  difference is visible on ASCII input.\<close>

definition documented_python_set :: "nat \<Rightarrow> bool" where
  "documented_python_set c \<equiv> c = 32 \<or> c = 9 \<or> c = 10 \<or> c = 13 \<or> c = 11 \<or> c = 12"

lemma documented_python_set_is_posix: "documented_python_set c = is_ws c"
  by (auto simp: documented_python_set_def is_ws_def)

lemma documented_python_set_would_keep_the_separator:
  "trim_by documented_python_set True True [28, 97, 28] = [28, 97, 28]"
  by eval

theorem python_trim_class_is_not_its_source_comment:
  "python_trim True True [28, 97, 28] \<noteq>
   trim_by documented_python_set True True [28, 97, 28]"
  using python_trim_drops_the_separator
    documented_python_set_would_keep_the_separator by simp

text \<open>Java is in the same position as Python for a DIFFERENT reason: it also
  has the four ASCII separators, so a guard on the source's ASCII-ness does not
  help. Its extra set and Python's happen to coincide on ASCII.\<close>

lemma java_trim_drops_the_separator: "java_trim True True [28, 97, 28] = [97]"
  by eval

theorem java_trim_has_no_ascii_guard:
  "\<exists>s. ascii s \<and> java_trim True True s \<noteq> string_trim True True s"
proof -
  have "ascii [28, 97, 28]" unfolding ascii_def by simp
  moreover have "java_trim True True [28, 97, 28] \<noteq> string_trim True True [28, 97, 28]"
    using java_trim_drops_the_separator by eval
  ultimately show ?thesis by blast
qed

text \<open>Zig is worse and in a different direction: it is a SUBSET of POSIX
  space, so it fails to trim the model's own carriages returns. A carriage
  return at either end is a plain ASCII byte, so this arm is unsound on ASCII
  text for a reason no encoding argument touches.\<close>

lemma zig_trim_keeps_the_carriage_return:
  "zig_trim [13, 97, 13] = [13, 97, 13]"
  by eval

lemma string_trim_drops_the_carriage_return:
  "string_trim True True [13, 97, 13] = [97]"
  by eval

theorem zig_trim_is_not_sound_on_ascii:
  "\<exists>s. ascii s \<and> zig_trim s \<noteq> string_trim True True s"
proof -
  have "ascii [13, 97, 13]" unfolding ascii_def by simp
  moreover have "zig_trim [13, 97, 13] \<noteq> string_trim True True [13, 97, 13]"
    using zig_trim_keeps_the_carriage_return string_trim_drops_the_carriage_return
    by simp
  ultimately show ?thesis by blast
qed

subsection \<open>The dropped flags: a latent contract gap, stated as one\<close>

text \<open>The ESTree, Go, Rust and Zig arms take no flags at all: they call
  `trim()`, `TrimSpace`, `trim()`, `trim(u8, t, " \t\n")`. Their value is
  therefore `trim_by P True True`, which cannot equal a one-sided node value
  when the string has whitespace at the end the node was told to keep. The
  emitting transform currently only ever constructs
  `leading: true, trailing: true` (src/transforms/text_ops.rs), so no corpus
  example reaches this; the witnesses are here so that the day one does, the
  obligation is already discharged — and so that a transform which starts
  offering one-sided trims must not offer them to these four backends.\<close>

lemma estree_trim_drops_the_leading_flag:
  "estree_trim [32, 97, 32] \<noteq> string_trim True False [32, 97, 32]"
  by eval

lemma estree_trim_drops_the_trailing_flag:
  "estree_trim [32, 97, 32] \<noteq> string_trim False True [32, 97, 32]"
  by eval

lemma go_trim_drops_the_leading_flag:
  "go_trim [32, 97, 32] \<noteq> string_trim True False [32, 97, 32]"
  by eval

lemma rust_trim_drops_the_leading_flag:
  "rust_trim [32, 97, 32] \<noteq> string_trim True False [32, 97, 32]"
  by eval

theorem flag_blind_renderers_are_wrong_for_one_sided_trims:
  "estree_trim [32, 97, 32] \<noteq> string_trim True False [32, 97, 32]
   \<and> go_trim [32, 97, 32] \<noteq> string_trim True False [32, 97, 32]
   \<and> rust_trim [32, 97, 32] \<noteq> string_trim True False [32, 97, 32]"
  using estree_trim_drops_the_leading_flag go_trim_drops_the_leading_flag
    rust_trim_drops_the_leading_flag by blast

text \<open>And the same four are correct at the flag pair the transform actually
  emits, on ASCII — this is why the corpus is green and the gap is latent
  rather than live.\<close>

theorem flag_blind_renderers_are_right_at_both_ends:
  "estree_trim [32, 97, 32] = string_trim True True [32, 97, 32]
   \<and> go_trim [32, 97, 32] = string_trim True True [32, 97, 32]
   \<and> rust_trim [32, 97, 32] = string_trim True True [32, 97, 32]"
  by eval

text \<open>The non-ASCII witnesses, in the same style as the Space theory's: the
  renderer decodes to code points, the node reads bytes. U+00A0 is the shortest
  such case — JS trims it, the shell sees two bytes that are not spaces.\<close>

theorem js_trim_rendering_is_not_the_model:
  "estree_trim [160, 97, 160] \<noteq> string_trim True True [194, 160, 97, 194, 160]"
  by eval

theorem python_trim_rendering_is_not_the_model:
  "python_trim True True [160, 97, 160] \<noteq> string_trim True True [194, 160, 97, 194, 160]"
  by eval


section \<open>StringAffix: an affix is an existential, and the arms compute it\<close>

text \<open>`has_affix t p pre` is the node's value as a boolean: does `p` occur at
  the front (or the back). The prefix side is written `take (length p) t = p`
  because that is exactly what `startsWith`/`index(t,p)==0` decide, INCLUDING
  the empty pattern (a take of nothing is nothing). The suffix side is written
  `drop (length t - length p) t = p`, which needs no length guard: when `p` is
  longer than `t` the subtraction is zero and `t` cannot equal the longer `p`.
  Both forms are proved equal to the existential below, so the renderers'
  conditionals and the algebra speak the same language.\<close>

definition has_affix :: "bytes \<Rightarrow> bytes \<Rightarrow> bool \<Rightarrow> bool" where
  "has_affix t p pre \<equiv>
     (if pre then take (length p) t = p
      else drop (length t - length p) t = p)"

lemma has_affix_nil_pat [simp]: "has_affix t [] pre"
  unfolding has_affix_def by simp

lemma has_affix_pre [simp]: "has_affix t p True = (take (length p) t = p)"
  unfolding has_affix_def by simp

lemma has_affix_suf [simp]: "has_affix t p False = (drop (length t - length p) t = p)"
  unfolding has_affix_def by simp

text \<open>The empty pattern is an affix of everything, on both sides. That is the
  shell's answer too (`${s#}`/`${s%}` with an empty literal pattern are the
  identity) and it is where the Perl trailing arm diverges — see below.\<close>

lemma has_affix_empty_pattern_both:
  "has_affix t [] True \<and> has_affix t [] False"
  by simp

lemma has_affix_empty_text: "has_affix [] p pre \<longleftrightarrow> p = []"
  by (cases pre) auto

lemma has_affix_too_long:
  assumes lt: "length t < length p"
  shows "\<not> has_affix t p pre"
proof (cases pre)
  case True
  have len: "length (take (length p) t) = length t"
    using lt by (simp add: min_def)
  have neq: "take (length p) t \<noteq> p"
  proof
    assume "take (length p) t = p"
    hence "length (take (length p) t) = length p" by simp
    with len lt show False by simp
  qed
  have peq: "pre = True" using True by simp
  show ?thesis using peq neq by simp
next
  case False
  have peq: "pre = False" using False by simp
  have z: "length t - length p = 0" using lt by simp
  have "drop (length t - length p) t = t" using z by simp
  moreover have "t \<noteq> p"
  proof
    assume "t = p"
    hence "length t = length p" by simp
    with lt show False by simp
  qed
  ultimately show ?thesis using peq by simp
qed

theorem affix_pre_is_the_existential:
  "has_affix t p True \<longleftrightarrow> (\<exists>r. t = p @ r)"
proof
  assume h: "has_affix t p True"
  have base: "t = take (length p) t @ drop (length p) t" by simp
  also have "\<dots> = p @ drop (length p) t" using h by simp
  finally have eq: "t = p @ drop (length p) t" .
  from eq show "\<exists>r. t = p @ r" by (rule exI)
next
  assume "\<exists>r. t = p @ r"
  then obtain r where r: "t = p @ r" ..
  have "take (length p) t = p" using r by simp
  thus "has_affix t p True" by simp
qed

theorem affix_suf_is_the_existential:
  "has_affix t p False \<longleftrightarrow> (\<exists>r. t = r @ p)"
proof
  assume h: "has_affix t p False"
  have base: "t = take (length t - length p) t @ drop (length t - length p) t" by simp
  also have "\<dots> = take (length t - length p) t @ p" using h by simp
  finally have eq: "t = take (length t - length p) t @ p" .
  from eq show "\<exists>r. t = r @ p" by (rule exI)
next
  assume "\<exists>r. t = r @ p"
  then obtain r where r: "t = r @ p" ..
  have d: "drop (length t - length p) t = p"
  proof -
    have "length t = length r + length p" using r by simp
    hence "length t - length p = length r" by simp
    hence "drop (length t - length p) t = drop (length r) (r @ p)"
      using r by simp
    also have "\<dots> = p" by simp
    finally show ?thesis .
  qed
  thus "has_affix t p False" by simp
qed

text \<open>Affix facts that matter to a transform rule: affixing is stable under
  appending on the side that is NOT the pattern's side, so it can be decided on
  a prefix of the text; and a pattern longer than the text is never an
  affix.\<close>

lemma has_affix_short_length:
  assumes "has_affix t p True"
  shows "length p \<le> length t"
proof -
  have h: "take (length p) t = p" using assms by simp
  have "length p = length (take (length p) t)" using h by simp
  also have "\<dots> \<le> length t" by simp
  finally show ?thesis .
qed

lemma has_affix_suf_length:
  assumes "has_affix t p False"
  shows "length p \<le> length t"
proof -
  have h: "drop (length t - length p) t = p" using assms by simp
  have "length p = length (drop (length t - length p) t)" using h by simp
  also have "\<dots> \<le> length t" by simp
  finally show ?thesis .
qed

lemma affix_pre_stable [simp]:
  assumes "has_affix t p True"
  shows "has_affix (t @ w) p True"
proof -
  have h: "take (length p) t = p" using assms by simp
  have le: "length p \<le> length t" using assms by (rule has_affix_short_length)
  have "take (length p) (t @ w) = take (length p) t" using le by simp
  also have "\<dots> = p" using h .
  finally have "take (length p) (t @ w) = p" .
  thus ?thesis by simp
qed

lemma affix_suf_stable [simp]:
  assumes "has_affix t p False"
  shows "has_affix (w @ t) p False"
proof -
  have h: "drop (length t - length p) t = p" using assms by simp
  have le: "length p \<le> length t" using assms by (rule has_affix_suf_length)
  have "length (w @ t) - length p = length w + (length t - length p)" using le by simp
  hence "drop (length (w @ t) - length p) (w @ t)
           = drop (length w + (length t - length p)) (w @ t)" by simp
  also have "\<dots> = drop (length t - length p) t" by simp
  also have "\<dots> = p" using h .
  finally have "drop (length (w @ t) - length p) (w @ t) = p" .
  thus ?thesis by simp
qed

lemma affix_suf_not_stable_at_front [simp]:
  "\<not> has_affix [97, 98, 98] [97] False"
  by simp


section \<open>AffixStrip: the affix test, then a slice\<close>

text \<open>The node's contract is one occurrence: if the pattern is there, remove
  that one copy, otherwise return the text unchanged. The Rust arm is the
  clearest statement of it (`t.strip_prefix(p).unwrap_or(t)`), the ESTree arm
  the clearest statement of its shape (`startsWith ? slice(len) : t`), and the
  definition below is both — which is why `affix_strip_shape` is the obligation
  the renderers discharge.\<close>

definition affix_strip_node :: "bytes \<Rightarrow> bytes \<Rightarrow> bool \<Rightarrow> bytes" where
  "affix_strip_node t p pre \<equiv>
     (if has_affix t p pre
      then (if pre then drop (length p) t else take (length t - length p) t)
      else t)"

lemma affix_strip_nil_pat [simp]: "affix_strip_node t [] pre = t"
  unfolding affix_strip_node_def by simp

lemma affix_strip_no_affix [simp]:
  "\<not> has_affix t p pre \<Longrightarrow> affix_strip_node t p pre = t"
  unfolding affix_strip_node_def by simp

lemma affix_strip_pre [simp]:
  "affix_strip_node t p True =
     (if take (length p) t = p then drop (length p) t else t)"
  unfolding affix_strip_node_def by simp

lemma affix_strip_suf [simp]:
  "affix_strip_node t p False =
     (if drop (length t - length p) t = p then take (length t - length p) t else t)"
  unfolding affix_strip_node_def by simp

text \<open>The shape the arms emit, with the test written as the existential that
  `startsWith`/`endsWith`/`strip_prefix` compute. This is the per-node soundness
  statement for the four consistent renderers: the condition, the slice and the
  fall-through are all the node's.\<close>

theorem affix_strip_shape_pre [simp]:
  "affix_strip_node t p True =
     (if (\<exists>r. t = p @ r) then drop (length p) t else t)"
proof -
  have a: "affix_strip_node t p True =
              (if take (length p) t = p then drop (length p) t else t)"
    by (rule affix_strip_pre)
  have b: "take (length p) t = p \<longleftrightarrow> (\<exists>r. t = p @ r)"
    using affix_pre_is_the_existential by simp
  show ?thesis using a b by simp
qed

theorem affix_strip_shape_suf [simp]:
  "affix_strip_node t p False =
     (if (\<exists>r. t = r @ p) then take (length t - length p) t else t)"
proof -
  have a: "affix_strip_node t p False =
              (if drop (length t - length p) t = p
               then take (length t - length p) t else t)"
    by (rule affix_strip_suf)
  have b: "drop (length t - length p) t = p \<longleftrightarrow> (\<exists>r. t = r @ p)"
    using affix_suf_is_the_existential by simp
  show ?thesis using a b by simp
qed

theorem affix_strip_is_affix_then_slice:
  "affix_strip_node t p pre =
     (if has_affix t p pre
      then (if pre then drop (length p) t else take (length t - length p) t)
      else t)"
  by (simp add: affix_strip_node_def)

lemma affix_strip_pre_length:
  assumes "has_affix t p True"
  shows "length (affix_strip_node t p True) = length t - length p"
proof -
  have "take (length p) t = p" using assms by simp
  hence "affix_strip_node t p True = drop (length p) t" by simp
  thus ?thesis by simp
qed

text \<open>ONE occurrence, which is the whole difference between this node and
  `trim_start_matches`/`strip_prefix`-in-a-loop. Both facts are evaluated:
  three `a`s lose one, and stripping again loses another. A renderer that
  stripped every occurrence would satisfy the single-strip example and fail
  here.\<close>

theorem affix_strip_strips_one_occurrence:
  "affix_strip_node [97, 97, 97] [97] True = [97, 97]
   \<and> affix_strip_node (affix_strip_node [97, 97, 97] [97] True) [97] True = [97]"
  by eval

theorem affix_strip_is_not_idempotent:
  "affix_strip_node (affix_strip_node [97, 97, 97] [97] True) [97] True
     \<noteq> affix_strip_node [97, 97, 97] [97] True"
  by eval

text \<open>The empty pattern strips nothing, on either side — unlike the affix
  TEST, which is true. This is the node behaving correctly where the Perl
  trailing renderer does not (`perl_empty_suffix_rendering_is_not_the_model`).\<close>

lemma affix_strip_empty_pattern_is_identity:
  "affix_strip_node t [] True = t \<and> affix_strip_node t [] False = t"
  by simp

theorem affix_strip_empty_pattern_identity_witness:
  "affix_strip_node [97, 98, 99] [] False = [97, 98, 99]"
  by eval


section \<open>The Perl trailing StringAffix arm, and the empty pattern\<close>

text \<open>The measured facts (`measure-trim.sh`, `perl.txt`): `substr("abc", -length(""))`
  is `"abc"`, so `substr($t, -length($p)) eq $p` is FALSE for the empty pattern,
  while `endsWith("")` is true everywhere and the shell's `${t%}` with an empty
  literal pattern is the identity. The Perl arm is therefore correct exactly on
  the non-empty patterns, which is a property worth having as a theorem rather
  than as a comment: it tells a transform rule that the arm is safe iff the
  pattern is known non-empty.\<close>

definition perl_suffix_test :: "bytes \<Rightarrow> bytes \<Rightarrow> bool" where
  "perl_suffix_test t p \<equiv>
     (if p = [] then t = []
      else drop (length t - length p) t = p)"

theorem perl_suffix_test_is_the_model_for_nonempty_patterns:
  assumes "p \<noteq> []"
  shows "perl_suffix_test t p = has_affix t p False"
  using assms unfolding perl_suffix_test_def has_affix_def by simp

theorem perl_empty_suffix_test_is_not_the_model:
  "perl_suffix_test [97, 98, 99] [] \<noteq> has_affix [97, 98, 99] [] False"
  by eval

theorem perl_suffix_test_differs_exactly_at_the_empty_pattern:
  "\<not> perl_suffix_test [97, 98, 99] []
   \<and> has_affix [97, 98, 99] [] False
   \<and> (\<forall>p. p \<noteq> [] \<longrightarrow> perl_suffix_test [97, 98, 99] p = has_affix [97, 98, 99] p False)"
proof -
  have wit: "\<not> perl_suffix_test [97, 98, 99] []"
    using perl_empty_suffix_test_is_not_the_model by simp
  have model: "has_affix [97, 98, 99] [] False" by simp
  have all: "\<forall>p. p \<noteq> [] \<longrightarrow>
                perl_suffix_test [97, 98, 99] p = has_affix [97, 98, 99] p False"
  proof (rule allI, rule impI)
    fix p :: bytes
    assume "p \<noteq> []"
    thus "perl_suffix_test [97, 98, 99] p = has_affix [97, 98, 99] p False"
      by (rule perl_suffix_test_is_the_model_for_nonempty_patterns)
  qed
  from wit model all show ?thesis by simp
qed

text \<open>Why the leading arm does not have the same hole: `index($t, $p) == 0`
  computes the existential on the prefix side for EVERY pattern, the empty one
  included (`index($t, "")` is 0, which is the correct answer). And the Perl
  AffixStrip arms are correct at the empty pattern too, because a false test
  makes them return the text unchanged, which is the node's answer.\<close>

theorem perl_prefix_test_is_the_model:
  "has_affix t p True \<longleftrightarrow> (\<exists>r. t = p @ r)"
  by (rule affix_pre_is_the_existential)

lemma perl_affix_strip_empty_pattern_is_right:
  "affix_strip_node [97, 98, 99] [] True = [97, 98, 99]
   \<and> affix_strip_node [97, 98, 99] [] False = [97, 98, 99]"
  by eval


section \<open>Summary: what is proved, and what is left to decide\<close>

text \<open>Collected in the form a contract can quote.

  Proved about the NODES (any class, any flags): a trim is a run drop and is
  idempotent at all four flag pairs (`trim_by_idem_*`, `string_trim_idem`), it
  touches nothing but the ends (`lead_drop_untouched`, `trail_drop_untouched`,
  `string_trim_interior_kept`), the affix test is an existential on both sides
  (`affix_pre_is_the_existential`, `affix_suf_is_the_existential`), and
  AffixStrip is that test plus a slice, once, with the empty pattern an
  identity (`affix_strip_shape`, `affix_strip_strips_one_occurrence`,
  `affix_strip_empty_pattern_is_identity`).

  Proved about the EMITS: Perl is sound on ASCII at every flag pair; JS, Go and
  Rust are sound on ASCII at the both-ends pair the transform emits; Python and
  Java have no ASCII guard (0x1C); Zig is unsound on ASCII (CR); the four
  flag-blind arms are unsound for one-sided trims; JS and Python differ from
  the node on U+00A0; and the Perl trailing affix arm differs exactly at the
  empty pattern.

  Left to decide, deliberately: whether the class-based arms should (a) refuse
  `StringTrim` outside ASCII, (b) be narrowed to the measured class of the
  target, or (c) be replaced by an explicit byte-set node that the lowering can
  spell out per backend. Each option needs the class tables above; none is a
  proof obligation of this file, whose obligation is that the current emits are
  not the node's value and on exactly which strings. The natural next slice is
  `StringAffix`/`AffixStrip` in a loop — `${s#p}` inside `read`, where the
  occurrence count interacts with the iteration — which needs a state tier and
  is not attempted here.\<close>

section \<open>Node obligations: the three string-algebra nodes\<close>

text \<open>These are the \<open>spec:\<close> targets the node files cite, named as the
  nodes name them. Each restates an obligation proved above; the named form is
  what makes a node's \<open>spec:\<close> line resolve to a theorem, which
  harness/check-node-specs.pl checks mechanically.\<close>

theorem string_trim_sound:
  "string_trim pre suf s = trim_by is_ws pre suf s"
  unfolding string_trim_def by simp

theorem string_affix_sound:
  "has_affix t p True = (take (length p) t = p) \<and>
   has_affix t p False = (drop (length t - length p) t = p)"
  by simp

theorem affix_strip_sound:
  "(affix_strip_node t p pre =
     (if has_affix t p pre
      then (if pre then drop (length p) t else take (length t - length p) t)
      else t)) \<and>
   (affix_strip_node t p pre =
     (if pre then (if take (length p) t = p then drop (length p) t else t)
      else (if drop (length t - length p) t = p
            then take (length t - length p) t else t)))"
  by simp


end
