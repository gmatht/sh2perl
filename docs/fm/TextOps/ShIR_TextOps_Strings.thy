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


section \<open>RepeatStr: repetition, and the slice\<close>

text \<open>The `RepeatStr` node (emitted for the `yes X | head -n K` idiom) is a
  repetition: the renderers spell it `"C" x N`, `.repeat(N)`, `repeat()`. It is
  defined here the way a loop would compute it, and proved equal to the
  library reading `concat (replicate n s)` — so the backends' three spellings
  are the same value, and the node's length is arithmetic, not a guess.\<close>

fun repeat_str_node :: "nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "repeat_str_node 0 s = []"
| "repeat_str_node (Suc k) s = s @ repeat_str_node k s"

theorem repeat_str_is_concat:
  "repeat_str_node n s = concat (replicate n s)"
  by (induct n) simp_all

theorem repeat_str_sound:
  "repeat_str_node n s = concat (replicate n s) \<and>
   length (repeat_str_node n s) = n * length s \<and>
   repeat_str_node 0 s = [] \<and>
   repeat_str_node 1 s = s \<and>
   repeat_str_node n [] = []"
proof (intro conjI)
  show "repeat_str_node n s = concat (replicate n s)" by (rule repeat_str_is_concat)
  show "length (repeat_str_node n s) = n * length s" by (induct n) simp_all
  show "repeat_str_node 0 s = []" by simp
  show "repeat_str_node 1 s = s" by simp
  show "repeat_str_node n [] = []" by (induct n) simp_all
qed

text \<open>The `SubStrExtract` node (`expr substr`, `${var:N:M}`) reads a bounded
  slice. The definition is the byte-at-a-time scan a C arm performs; the
  theorem identifies it with `take len (drop off s)`, which is what the JS,
  Perl and Rust arms compute, so the arms agree on the value and on the
  clipping: a slice shorter than `len` is the whole tail, and an offset past
  the end is the empty string. Negative offsets (bash's `${v: -3}`) are NOT
  modelled here; only the non-negative index forms are.\<close>

fun substr_extract :: "bytes \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> bytes" where
  "substr_extract [] off len = []"
| "substr_extract (c # cs) off len =
     (if off = 0 then (if len = 0 then [] else c # substr_extract cs 0 (len - 1))
      else substr_extract cs (off - 1) len)"

theorem substr_extract_is_take_drop:
  "substr_extract s off len = take len (drop off s)"
proof (induct s arbitrary: off len)
  case Nil
  then show ?case by simp
next
  case (Cons c cs)
  note IH = Cons(1)
  show ?case
  proof (cases off)
    case 0
    note off0 = this
    show ?thesis
    proof (cases len)
      case 0
      with off0 show ?thesis by simp
    next
      case (Suc m)
      with off0 IH show ?thesis by simp
    qed
  next
    case (Suc k)
    with IH show ?thesis by simp
  qed
qed

theorem substr_extract_sound:
  "substr_extract s off len = take len (drop off s) \<and>
   length (substr_extract s off len) \<le> len \<and>
   substr_extract s off 0 = [] \<and>
   substr_extract s 0 len = take len s \<and>
   (length s \<le> off \<longrightarrow> substr_extract s off len = [])"
proof (intro conjI)
  show "substr_extract s off len = take len (drop off s)"
    by (rule substr_extract_is_take_drop)
  show "length (substr_extract s off len) \<le> len"
    by (simp add: substr_extract_is_take_drop)
  show "substr_extract s off 0 = []"
    by (simp add: substr_extract_is_take_drop)
  show "substr_extract s 0 len = take len s"
    by (simp add: substr_extract_is_take_drop)
  show "length s \<le> off \<longrightarrow> substr_extract s off len = []"
  proof (intro impI)
    assume h: "length s \<le> off"
    have "drop off s = []" using h by simp
    then show "substr_extract s off len = []"
      by (simp add: substr_extract_is_take_drop)
  qed
qed


section \<open>CaseTransform: casing as a byte table, and its ASCII guard\<close>

text \<open>`CaseTransform` is emitted for `tr a-z A-Z`, `${var^}`, `${var^^}`,
  `${var,}` and `${var,,}`, and rendered as `lc`/`uc`, `toLowerCase`, `tolower`.
  The model here is the BYTE table, because this session's strings are bytes.
  That is not a limitation of convenience: it is what the node means in the
  shell, and it is where several renderers stop being faithful. The rules
  proved below are exactly the ones a renderer may depend on for ASCII input;
  what the model does NOT cover is stated at the end of the section, and the
  node comment repeats it.\<close>

definition up_byte :: "nat \<Rightarrow> nat" where
  "up_byte c = (if 97 \<le> c \<and> c \<le> 122 then c - 32 else c)"

definition down_byte :: "nat \<Rightarrow> nat" where
  "down_byte c = (if 65 \<le> c \<and> c \<le> 90 then c + 32 else c)"

definition case_map :: "bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "case_map up s = map (if up then up_byte else down_byte) s"

lemma up_byte_idem [simp]: "up_byte (up_byte c) = up_byte c"
proof (cases "97 \<le> c \<and> c \<le> 122")
  case True
  have h1: "up_byte c = c - 32" using True by (simp add: up_byte_def; arith)
  have h3: "\<not> (97 \<le> c - 32 \<and> c - 32 \<le> 122)" using True by arith
  have h4: "up_byte (c - 32) = c - 32" using h3 by (simp add: up_byte_def; arith)
  show ?thesis using h1 h4 by metis
next
  case False
  have h1: "up_byte c = c" using False by (simp add: up_byte_def; arith)
  then show ?thesis by simp
qed

lemma down_byte_idem [simp]: "down_byte (down_byte c) = down_byte c"
proof (cases "65 \<le> c \<and> c \<le> 90")
  case True
  have h1: "down_byte c = c + 32" using True by (simp add: down_byte_def; arith)
  have h3: "\<not> (65 \<le> c + 32 \<and> c + 32 \<le> 90)" using True by arith
  have h4: "down_byte (c + 32) = c + 32" using h3 by (simp add: down_byte_def; arith)
  show ?thesis using h1 h4 by metis
next
  case False
  have h1: "down_byte c = c" using False by (simp add: down_byte_def; arith)
  then show ?thesis by simp
qed

lemma up_then_down [simp]: "down_byte (up_byte c) = down_byte c"
proof (cases "97 \<le> c \<and> c \<le> 122")
  case True
  have hu: "up_byte c = c - 32" using True by (simp add: up_byte_def; arith)
  have nlow: "\<not> (65 \<le> c \<and> c \<le> 90)" using True by arith
  have dc: "down_byte c = c" using nlow by (simp add: down_byte_def; arith)
  have inr: "65 \<le> c - 32 \<and> c - 32 \<le> 90" using True by arith
  have step: "down_byte (c - 32) = c"
  proof -
    have "down_byte (c - 32) = (c - 32) + 32" using inr by (simp add: down_byte_def)
    then show ?thesis using True by arith
  qed
  show ?thesis using hu dc step by metis
next
  case False
  have hu: "up_byte c = c" using False by (simp add: up_byte_def; arith)
  then show ?thesis by simp
qed

lemma down_then_up [simp]: "up_byte (down_byte c) = up_byte c"
proof (cases "65 \<le> c \<and> c \<le> 90")
  case True
  have hd: "down_byte c = c + 32" using True by (simp add: down_byte_def; arith)
  have nup: "\<not> (97 \<le> c \<and> c \<le> 122)" using True by arith
  have uc: "up_byte c = c" using nup by (simp add: up_byte_def; arith)
  have inr: "97 \<le> c + 32 \<and> c + 32 \<le> 122" using True by arith
  have step: "up_byte (c + 32) = c"
  proof -
    have "up_byte (c + 32) = (c + 32) - 32" using inr by (simp add: up_byte_def)
    then show ?thesis using True by arith
  qed
  show ?thesis using hd uc step by metis
next
  case False
  have hd: "down_byte c = c" using False by (simp add: down_byte_def; arith)
  then show ?thesis by simp
qed

text \<open>Casing a text never changes its length, is idempotent, and the two
  directions compose to the inner one — which is why a renderer may run `lc` and
  `uc` in either order without inventing a third case. Non-ASCII bytes are FIXED
  POINTS: that is the guard the arms rely on, and the divergence below.\<close>

lemma case_map_length [simp]: "length (case_map up s) = length s"
  by (simp add: case_map_def)

lemma case_map_idem [simp]: "case_map up (case_map up s) = case_map up s"
  by (induct s) (simp_all add: case_map_def)

lemma case_map_down_of_up [simp]: "case_map False (case_map True s) = case_map False s"
  by (induct s) (simp_all add: case_map_def)

lemma case_map_up_of_down [simp]: "case_map True (case_map False s) = case_map True s"
  by (induct s) (simp_all add: case_map_def)

lemma case_map_non_ascii [simp]: "128 \<le> c \<Longrightarrow> case_map up [c] = [c]"
proof -
  assume h: "128 \<le> c"
  have nlow: "\<not> (97 \<le> c \<and> c \<le> 122)" using h by arith
  have nup: "\<not> (65 \<le> c \<and> c \<le> 90)" using h by arith
  have u: "up_byte c = c" using nlow by (simp add: up_byte_def; arith)
  have d: "down_byte c = c" using nup by (simp add: down_byte_def; arith)
  show ?thesis using u d by (simp add: case_map_def)
qed

theorem case_transform_sound:
  "length (case_map up s) = length s \<and>
   case_map up (case_map up s) = case_map up s \<and>
   case_map True (case_map False s) = case_map True s \<and>
   case_map False (case_map True s) = case_map False s \<and>
   (\<forall>c. 128 \<le> c \<longrightarrow> case_map up [c] = [c])"
proof (intro conjI)
  show "length (case_map up s) = length s" by simp
  show "case_map up (case_map up s) = case_map up s" by simp
  show "case_map True (case_map False s) = case_map True s" by simp
  show "case_map False (case_map True s) = case_map False s" by simp
  show "\<forall>c. 128 \<le> c \<longrightarrow> case_map up [c] = [c]" using case_map_non_ascii by blast
qed

lemma case_map_ascii_witness:
  "case_map True [97, 122, 65, 32] = [65, 90, 65, 32] \<and>
   case_map False [65, 90, 97, 126] = [97, 122, 97, 126]"
  by eval

lemma case_map_utf8_witness:
  "case_map True [195, 169] = [195, 169] \<and>
   case_map False [195, 137] = [195, 137]"
  by eval

text \<open>WHAT IS OUT OF MODEL, precisely. The model cases ASCII letters only, so
  every byte above 127 is a fixed point: `case_map True [0xC3, 0xA9]` is the
  UTF-8 form of U+00E9 (e-acute) and the model leaves it alone. Rust's
  `to_uppercase`, JS `toUpperCase` and Perl's `uc` under `use utf8` case-map the
  CODE POINT instead, and would return U+00C9 (`[0xC3, 0x89]`). The two readings
  therefore differ on exactly the inputs where a multi-byte character carries a
  case: an ASCII-guarded renderer is faithful, an unguarded one is not. Bash's
  own `${var^^}` is locale-dependent in the same way, which is why the node does
  not promise a language's full casing table. The byte-level laws above are what
  a guarded renderer may assume.\<close>

section \<open>CharTranslate: the `tr` node\<close>

text \<open>`CharTranslate` covers `tr SET1 SET2` and its `-d` (delete) and `-s`
  (squeeze) modes. The three modes are modelled separately and composed by the
  node's flags: translation (SET2 supplied), deletion (no SET2, drop the SET1
  bytes), and squeezing (collapse runs of the SET2 bytes). `tr`'s target rule is
  the one that surprises people — if SET2 is SHORTER than SET1, its LAST member
  is repeated — so the table is built by `tr_expand`, and the key law is that
  equal-length sets make the whole thing a per-byte map, which is what the cheap
  renderers assume. NOT modelled: `tr`'s range syntax `a-z`, character classes
  (`[:upper:]`), escapes (`\n`, `\055`) and the complement `-c` — the node's
  SET fields are already-expanded byte strings, and range expansion is the
  lowering's job, not the node's.\<close>

fun tr_expand :: "bytes \<Rightarrow> bytes \<Rightarrow> (nat \<times> nat) list" where
  "tr_expand [] to = []"
| "tr_expand (f # fs) [] = (f, f) # tr_expand fs []"
| "tr_expand (f # fs) (t # ts) = (f, t) # tr_expand fs (if ts = [] then [t] else ts)"

definition tr_translate :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "tr_translate src to s =
     map (\<lambda>c. case map_of (tr_expand src to) c of Some t \<Rightarrow> t | None \<Rightarrow> c) s"

definition tr_delete :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "tr_delete src s = filter (\<lambda>c. c \<notin> set src) s"

fun squeeze_runs :: "(nat \<Rightarrow> bool) \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes" where
  "squeeze_runs P prev [] = []"
| "squeeze_runs P prev (c # cs) =
     (if P c \<and> prev then squeeze_runs P prev cs
      else c # squeeze_runs P (P c) cs)"

definition squeeze :: "(nat \<Rightarrow> bool) \<Rightarrow> bytes \<Rightarrow> bytes" where
  "squeeze P s = squeeze_runs P False s"

lemma tr_expand_equal:
  "length src = length to \<Longrightarrow> tr_expand src to = zip src to"
proof (induct src arbitrary: to)
  case Nil
  then show ?case by (cases to) simp_all
next
  case (Cons f fs)
  note IH = Cons(1)
  note len0 = Cons(2)
  show ?case
  proof (cases to)
    case Nil
    then show ?thesis using len0 by simp
  next
    case (Cons t ts)
    note to_case = this
    have len: "length fs = length ts" using len0 to_case by simp
    show ?thesis
    proof (cases ts)
      case Nil
      then have fs_nil: "fs = []" using len by simp
      then show ?thesis using to_case Nil by simp
    next
      case (Cons t' ts')
      then show ?thesis using IH len to_case by simp
    qed
  qed
qed

lemma map_of_self_eq: "map_of (map (\<lambda>f. (f, f)) xs) c = Some t \<Longrightarrow> t = c"
  by (induct xs) (auto split: if_split_asm)

lemma tr_expand_nil_to [simp]: "tr_expand src [] = map (\<lambda>f. (f, f)) src"
  by (induct src) simp_all

lemma tr_translate_nil_to [simp]: "tr_translate src [] s = s"
proof -
  have id: "\<And>c. (case map_of (map (\<lambda>f. (f, f)) src) c of Some t \<Rightarrow> t | None \<Rightarrow> c) = c"
  proof -
    fix c
    show "(case map_of (map (\<lambda>f. (f, f)) src) c of Some t \<Rightarrow> t | None \<Rightarrow> c) = c"
    proof (cases "map_of (map (\<lambda>f. (f, f)) src) c")
      case None
      then show ?thesis by simp
    next
      case (Some t)
      then have "t = c" by (rule map_of_self_eq)
      then show ?thesis using Some by simp
    qed
  qed
  show ?thesis by (simp add: tr_translate_def id)
qed

lemma tr_translate_length [simp]: "length (tr_translate src to s) = length s"
  by (simp add: tr_translate_def)

lemma tr_translate_nil_from [simp]: "tr_translate [] to s = s"
  by (simp add: tr_translate_def)

theorem tr_translate_is_byte_map:
  "length src = length to \<Longrightarrow>
     tr_translate src to s =
       map (\<lambda>c. case map_of (zip src to) c of Some t \<Rightarrow> t | None \<Rightarrow> c) s"
  unfolding tr_translate_def using tr_expand_equal by simp

lemma tr_expand_short_target:
  "tr_expand [97, 98, 99] [120, 121] = [(97, 120), (98, 121), (99, 121)]"
  by eval

lemma tr_delete_set: "set (tr_delete src s) = set s - set src"
  by (auto simp: tr_delete_def)

lemma tr_delete_length_le [simp]: "length (tr_delete src s) \<le> length s"
  by (simp add: tr_delete_def)

lemma tr_delete_nil_from [simp]: "tr_delete [] s = s"
  by (simp add: tr_delete_def)

lemma tr_delete_witness:
  "tr_delete [97] [98, 97, 97, 99] = [98, 99] \<and>
   tr_delete [] [97, 98] = [97, 98] \<and>
   tr_delete [97, 98] [97, 98, 99] = [99]"
  by eval

lemma squeeze_runs_subset [simp]: "set (squeeze_runs P prev s) \<subseteq> set s"
  by (induct s arbitrary: prev) (auto split: if_split)

lemma squeeze_runs_length_le [simp]: "length (squeeze_runs P prev s) \<le> length s"
proof (induct s arbitrary: prev)
  case Nil
  then show ?case by simp
next
  case (Cons c cs)
  note IH = Cons(1)
  show ?case
  proof (cases "P c \<and> prev")
    case True
    have eq: "length (squeeze_runs P prev (c # cs)) = length (squeeze_runs P prev cs)"
      using True by simp
    have le1: "length (squeeze_runs P prev cs) \<le> length cs" using IH by metis
    have "length (squeeze_runs P prev (c # cs)) \<le> length cs" using eq le1 by simp
    then show ?thesis by simp
  next
    case False
    have eq: "length (squeeze_runs P prev (c # cs)) =
              Suc (length (squeeze_runs P (P c) cs))" using False by simp
    have le1: "length (squeeze_runs P (P c) cs) \<le> length cs" using IH by metis
    have "Suc (length (squeeze_runs P (P c) cs)) \<le> Suc (length cs)" using le1 by simp
    then show ?thesis using eq by simp
  qed
qed

lemma squeeze_head_nonmember:
  "prev \<Longrightarrow> squeeze_runs P prev s = [] \<or>
            (\<exists>x xs. squeeze_runs P prev s = x # xs \<and> \<not> P x)"
proof (induct s arbitrary: prev)
  case Nil
  then show ?case by simp
next
  case (Cons c cs)
  note IH = Cons(1)
  note hprev = Cons(2)
  show ?case
  proof (cases "P c \<and> prev")
    case True
    then have eq: "squeeze_runs P prev (c # cs) = squeeze_runs P prev cs" by simp
    have disj: "squeeze_runs P prev cs = [] \<or>
          (\<exists>x xs. squeeze_runs P prev cs = x # xs \<and> \<not> P x)" using IH hprev by simp
    then show ?thesis using eq by metis
  next
    case False
    have np: "\<not> P c" using False hprev by blast
    have keep: "squeeze_runs P prev (c # cs) = c # squeeze_runs P (P c) cs" using False by simp
    have ex: "\<exists>x xs. squeeze_runs P prev (c # cs) = x # xs \<and> \<not> P x"
      using keep np by blast
    then show ?thesis by (rule disjI2)
  qed
qed

fun no_run :: "(nat \<Rightarrow> bool) \<Rightarrow> bytes \<Rightarrow> bool" where
  "no_run P [] = True"
| "no_run P [_] = True"
| "no_run P (x # y # zs) = (\<not> (P x \<and> P y) \<and> no_run P (y # zs))"

lemma squeeze_no_run: "no_run P (squeeze_runs P prev s)"
proof (induct s arbitrary: prev)
  case Nil
  then show ?case by simp
next
  case (Cons c cs)
  note IH = Cons(1)
  show ?case
  proof (cases "P c \<and> prev")
    case True
    have eq: "squeeze_runs P prev (c # cs) = squeeze_runs P prev cs" using True by simp
    then show ?thesis using IH by simp
  next
    case False
    have keep: "squeeze_runs P prev (c # cs) = c # squeeze_runs P (P c) cs" using False by simp
    have tail_ok: "no_run P (squeeze_runs P (P c) cs)" using IH by simp
    show ?thesis
    proof (cases "squeeze_runs P (P c) cs")
      case Nil
      then show ?thesis using keep by simp
    next
      case (Cons d ds)
      note tail = this
      have pc_imp: "P c \<Longrightarrow> \<not> P d"
      proof -
        assume pc: "P c"
        have eq: "squeeze_runs P True cs = d # ds" using tail pc by simp
        have hh: "squeeze_runs P True cs = [] \<or>
                  (\<exists>x xs. squeeze_runs P True cs = x # xs \<and> \<not> P x)"
          by (rule squeeze_head_nonmember) simp
        have "\<not> P d"
        proof (rule ccontr)
          assume pd: "\<not> \<not> P d"
          have ne: "squeeze_runs P True cs \<noteq> []" using eq by simp
          with hh obtain x xs where cd: "squeeze_runs P True cs = x # xs"
            and npx: "\<not> P x" by blast
          have "x = d" using cd eq by simp
          then show False using npx pd by simp
        qed
        then show "\<not> P d" .
      qed
      have h1: "\<not> (P c \<and> P d)" using pc_imp by blast
      have h2: "no_run P (d # ds)" using tail tail_ok by simp
      have ge: "squeeze_runs P prev (c # cs) = c # d # ds" using keep tail by simp
      show ?thesis using ge h1 h2 by (simp add: no_run.simps)
    qed
  qed
qed

lemma squeeze_runs_replicate_member [simp]:
  "P c \<Longrightarrow> squeeze_runs P prev (replicate n c) =
     (if n = 0 then [] else if prev then [] else [c])"
proof (induct n arbitrary: prev)
  case 0
  then show ?case by simp
next
  case (Suc m)
  note IH = this
  show ?case
  proof (cases prev)
    case True
    then show ?thesis using IH by simp
  next
    case False
    then show ?thesis using IH by simp
  qed
qed

lemma squeeze_runs_replicate_nonmember [simp]:
  "\<not> P c \<Longrightarrow> squeeze_runs P prev (replicate n c) = replicate n c"
  by (induct n arbitrary: prev) simp_all

lemma squeeze_witness:
  "squeeze (\<lambda>c. c = 97) [97, 97, 97, 98, 97] = [97, 98, 97] \<and>
   squeeze (\<lambda>c. c = 97) [98, 98] = [98, 98] \<and>
   squeeze (\<lambda>c. c = 97) [] = [] \<and>
   squeeze (\<lambda>c. c = 97) [97, 98, 97, 97] = [97, 98, 97]"
  by eval

theorem char_translate_sound:
  "length (tr_translate src to s) = length s \<and>
   (length src = length to \<longrightarrow>
      tr_translate src to s =
        map (\<lambda>c. case map_of (zip src to) c of Some t \<Rightarrow> t | None \<Rightarrow> c) s) \<and>
   tr_translate [] to s = s \<and>
   tr_translate src [] s = s \<and>
   set (tr_delete src s) = set s - set src \<and>
   length (tr_delete src s) \<le> length s \<and>
   tr_delete [] s = s \<and>
   set (squeeze (\<lambda>c. c \<in> set to) s) \<subseteq> set s \<and>
   no_run (\<lambda>c. c \<in> set to) (squeeze (\<lambda>c. c \<in> set to) s) \<and>
   length (squeeze (\<lambda>c. c \<in> set to) s) \<le> length s"
proof (intro conjI)
  show "length (tr_translate src to s) = length s" by simp
  show "length src = length to \<longrightarrow>
        tr_translate src to s =
          map (\<lambda>c. case map_of (zip src to) c of Some t \<Rightarrow> t | None \<Rightarrow> c) s"
    using tr_translate_is_byte_map by blast
  show "tr_translate [] to s = s" by simp
  show "tr_translate src [] s = s" by simp
  show "set (tr_delete src s) = set s - set src" by (rule tr_delete_set)
  show "length (tr_delete src s) \<le> length s" by simp
  show "tr_delete [] s = s" by simp
  show "set (squeeze (\<lambda>c. c \<in> set to) s) \<subseteq> set s"
    by (simp add: squeeze_def)
  show "no_run (\<lambda>c. c \<in> set to) (squeeze (\<lambda>c. c \<in> set to) s)"
    by (simp add: squeeze_def squeeze_no_run)
  show "length (squeeze (\<lambda>c. c \<in> set to) s) \<le> length s" by (simp add: squeeze_def)
qed


section \<open>CharExtract: the `cut -c` window, in bytes\<close>

text \<open>`CharExtract` is the positional node behind `cut -cN-M` / `cut -bN-M`
  (and `${v:N:M}`-shaped windows). The node's window is modelled as start/length
  over BYTES, which is what `cut -b` means and what `byte_mode = true` selects, and
  it is the reading this session can decide: the value is the slice of the input
  at that offset, so it is contiguous, it never invents a byte, and a window past
  the end is empty. The laws below are the ones a renderer may rely on.

  WHAT IS OUT OF MODEL, precisely. (1) `cut -c` counts CHARACTERS where this model
  counts bytes: on multi-byte UTF-8 the two windows differ, and the byte window can
  return a continuation byte with no lead byte (witness `[195, 169, 97]` at offset 1,
  length 1 is `[169]`), which is not a character at all. A renderer is faithful to
  `byte_mode = false` only with a character-aware arm, so the node documents the
  flag rather than promising both. (2) `cut`'s FIELD and RANGE syntax — `-d`,
  comma-separated lists (`-c1,3,5`), open ranges (`-c3-`) and disjoint selections —
  belongs to the lowering, not to the node: this node is ONE contiguous window, and
  a selection of several disjoint windows is several nodes. (3) Negative offsets
  are not modelled either.\\<close>

definition char_extract :: "bytes \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> bytes" where
  "char_extract s start len = substr_extract s start len"

lemma char_extract_is_window: "char_extract s start len = take len (drop start s)"
  unfolding char_extract_def by (rule substr_extract_is_take_drop)

lemma char_extract_contiguous: "\<exists>r z. s = r @ char_extract s start len @ z"
proof -
  have win: "char_extract s start len = take len (drop start s)"
    by (rule char_extract_is_window)
  have dflat: "drop len (drop start s) = drop (len + start) s"
    by (simp add: drop_drop)
  have flat: "drop (len + start) s = drop len (drop start s)"
    using dflat by (rule sym)
  have mid: "take len (drop start s) @ drop (len + start) s = drop start s"
  proof -
    have base: "take len (drop start s) @ drop len (drop start s) = drop start s"
      by (rule append_take_drop_id)
    show ?thesis
    proof (subst flat)
      show "take len (drop start s) @ drop len (drop start s) = drop start s"
        by (rule base)
    qed
  qed
  have eq: "s = take start s @ take len (drop start s) @ drop (len + start) s"
    by (simp add: mid)
  show ?thesis
  proof (rule exI[where x = "take start s"], rule exI[where x = "drop (len + start) s"])
    show "s = take start s @ char_extract s start len @ drop (len + start) s"
    proof (subst char_extract_is_window)
      show "s = take start s @ take len (drop start s) @ drop (len + start) s"
        by (rule eq)
    qed
  qed
qed

lemma char_extract_subset: "set (char_extract s start len) \<subseteq> set s"
proof -
  have s1: "set (take len (drop start s)) \<subseteq> set (drop start s)"
    by (rule set_take_subset)
  have s2: "set (drop start s) \<subseteq> set s" by (rule set_drop_subset)
  show ?thesis
  proof (subst char_extract_is_window)
    show "set (take len (drop start s)) \<subseteq> set s"
      using s1 s2 by (rule subset_trans)
  qed
qed

lemma char_extract_shift:
  "char_extract s start len = char_extract (drop start s) 0 len"
  by (simp add: char_extract_is_window)

theorem char_extract_sound:
  "char_extract s start len = take len (drop start s) \<and>
   length (char_extract s start len) \<le> len \<and>
   length (char_extract s start len) \<le> length s \<and>
   (len = 0 \<longrightarrow> char_extract s start len = []) \<and>
   (length s \<le> start \<longrightarrow> char_extract s start len = []) \<and>
   (\<exists>r z. s = r @ char_extract s start len @ z) \<and>
   set (char_extract s start len) \<subseteq> set s"
proof (intro conjI)
  show "char_extract s start len = take len (drop start s)"
    by (rule char_extract_is_window)
  show "length (char_extract s start len) \<le> len" by (simp add: char_extract_is_window)
  show "length (char_extract s start len) \<le> length s" by (simp add: char_extract_is_window)
  show "len = 0 \<longrightarrow> char_extract s start len = []" by (simp add: char_extract_is_window)
  show "length s \<le> start \<longrightarrow> char_extract s start len = []"
    by (simp add: char_extract_is_window)
  show "\<exists>r z. s = r @ char_extract s start len @ z" by (rule char_extract_contiguous)
  show "set (char_extract s start len) \<subseteq> set s" by (rule char_extract_subset)
qed

lemma char_extract_witness:
  "char_extract [97, 98, 99, 100, 101] 1 3 = [98, 99, 100] \<and>
   char_extract [97, 98, 99] 2 5 = [99] \<and>
   char_extract [97, 98, 99] 9 2 = [] \<and>
   char_extract [97, 98, 99] 1 0 = []"
  by eval

lemma char_extract_utf8_witness:
  "char_extract [195, 169, 97] 1 1 = [169] \<and>
   char_extract [195, 169, 97] 0 2 = [195, 169]"
  by eval


section \<open>PathName: GNU `basename`/`dirname`, in bytes\<close>

text \<open>`PathName` carries a `which` flag ("basename" or "dirname") and answers
  the GNU COMMAND semantics, which is not the same as bash's parameter strip: the
  renderer in `render_ext_expr/handlers/all_nodes.rs` is documented as "EXACT GNU
  basename/dirname semantics, verified against bash on trailing-slash, all-slash,
  no-slash and multi-slash inputs", and the transform notes record why the naive
  forms are wrong (`s|.*/||` and `split("/").pop()` yield "" for `a/b/`, and
  `split('/').slice(0,-1).join('/')` yields "" for a slashless word instead of
  "."). The model below is that rule over bytes, separator 47 = "/": strip
  trailing separators while more than one byte remains, an all-separator path is
  "/", `basename` is what follows the last separator, and `dirname` is the prefix
  before the last separator, re-stripped, with "" becoming "/" and a slashless
  path becoming ".".

  WHAT IS OUT OF MODEL, precisely. (1) One separator only: "/" (47). Windows
  "\\" and drive letters, and host separator classes, are not modelled; the
  node's `which` flag says nothing about platform. (2) No filesystem: this is the
  pure string function, so newline stripping, `basename -a` multi-argument and
  `basename -s` suffix removal are outside the node (the lowering passes one
  word). (3) No `.`/`..` resolution and no symlink or existence semantics — GNU
  basename/dirname do not resolve either, so the model is faithful rather than
  incomplete, but it is stated because renderers built on `std::path` DO
  normalise some of that (`Path::file_name` of "a/.." is None), which is why the
  node does not promise a host path library. (4) The generic Ext lowering in
  `shir.rs` renders `split('/').pop()` / `split('/').slice(0,-1).join('/')`,
  which disagrees with this rule on trailing-slash and slashless inputs; the
  handler is what is modelled here, and that mismatch is a lowering question,
  not a semantic one.\<close>

abbreviation sep :: nat where "sep \<equiv> 47"

fun lead_strip :: "bytes \<Rightarrow> bytes" where
  "lead_strip [] = []"
| "lead_strip (c # cs) = (if c = sep then lead_strip cs else c # cs)"

fun lead_until :: "bytes \<Rightarrow> bytes" where
  "lead_until [] = []"
| "lead_until (c # cs) = (if c = sep then [] else c # lead_until cs)"

fun drop_thru :: "bytes \<Rightarrow> bytes" where
  "drop_thru [] = []"
| "drop_thru (c # cs) = (if c = sep then c # cs else drop_thru cs)"

definition bname :: "bytes \<Rightarrow> bytes" where
  "bname p = (let q = rev (lead_strip (rev p)) in
     if q = [] then (if p = [] then [] else [sep])
     else if q = [sep] then [sep]
     else rev (lead_until (rev q)))"

definition dname :: "bytes \<Rightarrow> bytes" where
  "dname p = (if p \<noteq> [] \<and> set p \<subseteq> {sep} then [sep]
     else let q = rev (lead_strip (rev p)) in
       if sep \<notin> set q then [46]
       else let d = rev (lead_strip (rev (rev (drop_thru (rev q))))) in
         if d = [] then [sep] else d)"

lemma lead_strip_nosep: "sep \<notin> set p \<Longrightarrow> lead_strip p = p"
  by (induct p) auto

lemma lead_strip_all: "set p \<subseteq> {sep} \<Longrightarrow> lead_strip p = []"
  by (induct p) auto

lemma lead_strip_empty: "lead_strip p = [] \<Longrightarrow> set p \<subseteq> {sep}"
proof (induct p)
  case Nil
  then show ?case by simp
next
  case (Cons a p)
  show ?case
  proof (cases "a = sep")
    case True
    with Cons(2) have "lead_strip p = []" by simp
    with Cons(1) have "set p \<subseteq> {sep}" by simp
    with True show ?thesis by simp
  next
    case False
    with Cons(2) have "a # p = []" by simp
    then show ?thesis by simp
  qed
qed

lemma lead_strip_subset: "set (lead_strip p) \<subseteq> set p"
  by (induct p) auto

lemma lead_until_nosep: "sep \<notin> set p \<Longrightarrow> lead_until p = p"
  by (induct p) auto

lemma lead_until_no_sep: "sep \<notin> set (lead_until p)"
  by (induct p) auto

lemma lead_until_subset: "set (lead_until p) \<subseteq> set p"
  by (induct p) auto

lemma drop_thru_subset: "set (drop_thru p) \<subseteq> set p"
  by (induct p) auto

lemma drop_thru_present: "sep \<in> set p \<Longrightarrow> drop_thru p \<noteq> [] \<and> hd (drop_thru p) = sep"
  by (induct p) auto

lemma all_sep_imp_mem: "p \<noteq> [] \<Longrightarrow> set p \<subseteq> {sep} \<Longrightarrow> sep \<in> set p"
proof -
  assume np: "p \<noteq> []" and sub: "set p \<subseteq> {sep}"
  obtain a p' where hp: "p = a # p'" using np by (cases p) auto
  with sub have "a = sep" by auto
  with hp show ?thesis by simp
qed

lemma bname_nosep: "sep \<notin> set p \<Longrightarrow> bname p = p"
proof -
  assume h: "sep \<notin> set p"
  have r: "sep \<notin> set (rev p)" using h by simp
  have q2: "rev (lead_strip (rev p)) = p"
    using r by (simp add: lead_strip_nosep)
  have q3: "rev (lead_until (rev p)) = p"
    using r by (simp add: lead_until_nosep)
  show ?thesis
  proof (cases "p = []")
    case True
    then show ?thesis by (simp add: bname_def)
  next
    case False
    have ps: "p \<noteq> [sep]"
    proof
      assume "p = [sep]"
      with h show False by simp
    qed
    have qne: "rev (lead_strip (rev p)) \<noteq> [sep]" using q2 ps by simp
    have eq: "bname p = rev (lead_until (rev (rev (lead_strip (rev p)))))"
      using False ps qne q2 by (simp add: bname_def)
    show ?thesis using eq q2 q3 by simp
  qed
qed

lemma bname_all_sep: "p \<noteq> [] \<Longrightarrow> set p \<subseteq> {sep} \<Longrightarrow> bname p = [sep]"
proof -
  assume np: "p \<noteq> []" and sub: "set p \<subseteq> {sep}"
  have r: "set (rev p) \<subseteq> {sep}" using sub by simp
  have "lead_strip (rev p) = []" using r by (rule lead_strip_all)
  then have q0: "rev (lead_strip (rev p)) = []" by simp
  then show ?thesis using np by (simp add: bname_def)
qed

lemma lead_until_subset_rev: "set (rev (lead_until (rev q))) \<subseteq> set q"
proof -
  have a: "set (rev (lead_until (rev q))) \<subseteq> set (rev q)"
  proof (subst set_rev)
    show "set (lead_until (rev q)) \<subseteq> set (rev q)" by (rule lead_until_subset)
  qed
  have b: "set (rev q) \<subseteq> set q" by simp
  from a b show ?thesis by (rule subset_trans)
qed

lemma lead_strip_subset_rev: "set (rev (lead_strip (rev p))) \<subseteq> set p"
proof -
  have a: "set (rev (lead_strip (rev p))) \<subseteq> set (rev p)"
  proof (subst set_rev)
    show "set (lead_strip (rev p)) \<subseteq> set (rev p)" by (rule lead_strip_subset)
  qed
  have b: "set (rev p) \<subseteq> set p" by simp
  from a b show ?thesis by (rule subset_trans)
qed

lemma bname_subset: "set (bname p) \<subseteq> set p"
proof (cases "p = []")
  case True
  then show ?thesis by (simp add: bname_def)
next
  case False
  note np = \<open>p \<noteq> []\<close>
  show ?thesis
  proof (cases "rev (lead_strip (rev p)) = []")
    case True
    then have z: "lead_strip (rev p) = []" by simp
    have sub: "set (rev p) \<subseteq> {sep}" using z by (rule lead_strip_empty)
    have sp: "set p \<subseteq> {sep}" using sub by simp
    have "sep \<in> set p" using np sp by (rule all_sep_imp_mem)
    then show ?thesis by (simp add: bname_def np True)
  next
    case False
    note nq = \<open>rev (lead_strip (rev p)) \<noteq> []\<close>
    show ?thesis
    proof (cases "rev (lead_strip (rev p)) = [sep]")
      case True
      then have "sep \<in> set (rev (lead_strip (rev p)))" by simp
      with lead_strip_subset_rev have "sep \<in> set p" by blast
      then show ?thesis by (simp add: bname_def np True)
    next
      case False
      note ns = \<open>rev (lead_strip (rev p)) \<noteq> [sep]\<close>
      have inner: "set (rev (lead_until (rev (rev (lead_strip (rev p)))))) \<subseteq> set p"
      proof (rule subset_trans[of _ "set (rev (lead_strip (rev p)))"])
        show "set (rev (lead_until (rev (rev (lead_strip (rev p))))))
              \<subseteq> set (rev (lead_strip (rev p)))"
          by (rule lead_until_subset_rev)
        show "set (rev (lead_strip (rev p))) \<subseteq> set p"
          by (rule lead_strip_subset_rev)
      qed
      have hb: "bname p = rev (lead_until (rev (rev (lead_strip (rev p)))))"
        using np nq ns by (simp add: bname_def)
      then show ?thesis using inner by simp
    qed
  qed
qed

lemma dname_nosep: "sep \<notin> set p \<Longrightarrow> dname p = [46]"
proof -
  assume h: "sep \<notin> set p"
  have r: "sep \<notin> set (rev p)" using h by simp
  have q2: "rev (lead_strip (rev p)) = p"
    using r by (simp add: lead_strip_nosep)
  have ns: "sep \<notin> set (rev (lead_strip (rev p)))" using q2 h by simp
  show ?thesis
  proof (cases "p \<noteq> [] \<and> set p \<subseteq> {sep}")
    case True
    from True have np: "p \<noteq> []" and sp: "set p \<subseteq> {sep}" by simp_all
    have "sep \<in> set p" using np sp by (rule all_sep_imp_mem)
    with h show ?thesis by blast
  next
    case False
    with ns show ?thesis by (simp add: dname_def)
  qed
qed

lemma dname_all_sep: "p \<noteq> [] \<Longrightarrow> set p \<subseteq> {sep} \<Longrightarrow> dname p = [sep]"
  by (simp add: dname_def)

lemma dname_nonempty: "dname p \<noteq> []"
proof (cases "p \<noteq> [] \<and> set p \<subseteq> {sep}")
  case True
  then show ?thesis by (simp add: dname_def)
next
  case False
  show ?thesis
  proof (cases "sep \<in> set (rev (lead_strip (rev p)))")
    case True
    then show ?thesis using False by (simp add: dname_def)
  next
    case False
    then show ?thesis by (simp add: dname_def False)
  qed
qed

theorem path_name_sound:
  "(sep \<notin> set p \<longrightarrow> bname p = p) \<and>
   (p \<noteq> [] \<and> set p \<subseteq> {sep} \<longrightarrow> bname p = [sep]) \<and>
   set (bname p) \<subseteq> set p \<and>
   (sep \<notin> set p \<longrightarrow> dname p = [46]) \<and>
   (p \<noteq> [] \<and> set p \<subseteq> {sep} \<longrightarrow> dname p = [sep]) \<and>
   dname p \<noteq> [] \<and>
   dname [] = [46]"
proof (intro conjI)
  show "sep \<notin> set p \<longrightarrow> bname p = p" by (simp add: bname_nosep)
  show "p \<noteq> [] \<and> set p \<subseteq> {sep} \<longrightarrow> bname p = [sep]"
    by (auto intro: bname_all_sep)
  show "set (bname p) \<subseteq> set p" by (rule bname_subset)
  show "sep \<notin> set p \<longrightarrow> dname p = [46]" by (simp add: dname_nosep)
  show "p \<noteq> [] \<and> set p \<subseteq> {sep} \<longrightarrow> dname p = [sep]"
    by (auto intro: dname_all_sep)
  show "dname p \<noteq> []" by (rule dname_nonempty)
  show "dname [] = [46]" by (simp add: dname_def)
qed

lemma path_name_witness:
  "bname [47, 117, 115, 114, 47, 98, 105, 110] = [98, 105, 110] \<and>
   dname [47, 117, 115, 114, 47, 98, 105, 110] = [47, 117, 115, 114] \<and>
   bname [97, 47, 98, 47] = [98] \<and>
   dname [97, 47, 98] = [97] \<and>
   bname [97, 98, 99] = [97, 98, 99] \<and>
   dname [97, 98, 99] = [46]"
  by eval

lemma path_name_edge_witness:
  "bname [] = [] \<and>
   dname [] = [46] \<and>
   bname [47, 47, 47] = [47] \<and>
   dname [47, 47, 47] = [47] \<and>
   bname [47] = [47] \<and>
   dname [47] = [47] \<and>
   bname [46, 98, 97, 115, 104, 114, 99] = [46, 98, 97, 115, 104, 114, 99] \<and>
   dname [46, 98, 97, 115, 104, 114, 99] = [46]"
  by eval

lemma path_name_utf8_witness:
  "bname [47, 195, 169] = [195, 169] \<and>
   dname [47, 195, 169] = [47]"
  by eval

section \<open>TakeLines: head/tail as an LF-budget cut\<close>

text \<open>`TakeLines(text, count, from_end, bytes)` is the node behind `head -n` /
  `tail -n`; it also carries a byte mode for `head -c`/`tail -c`, which the source
  lowering refuses and falls back on. The renderers are the specification and agree
  on one shape — split on LF KEEPING empty fields (Perl `split(/\n/, $t, -1)`, JS
  `split('\n')`, Java `split("\n", -1)`), take the first (or last) N fields, and
  rejoin with LF, adding no terminator (`n <= 0` gives the empty text everywhere).
  At the byte level that is exactly: cut the text at the Nth LF and drop that LF.
  `cut` below is that function, and the line count is the LF count plus one, so no
  list inverse is needed anywhere.

  THE CONSEQUENCES THAT MATTER, each proved below.
  (1) The last kept line's terminator is NOT retained: `cut 2 "a\nb\nc" = "a\nb"`,
  and when the input ends in LF the trailing empty field is itself a line, so
  `cut 2 "a\nb\n" = "a\nb"` while `cut 3 "a\nb\n" = "a\nb\n"`. The emitting
  statement's newline flag supplies a terminator; this node never adds one.
  (2) Empty input is ONE empty line: `nlines [] = 1`, `nlines [LF] = 2`.
  (3) A text with fewer LFs than the count comes back unchanged
  (`count_lf s < n \<Longrightarrow> cut n s = s`), and otherwise the text is the output followed by
  a dropped terminator and a remainder (`s = cut n s @ LF # r`) — the truncation
  law `head` needs. The line count of the output is `min n (nlines s)`.
  (4) Only LF terminates. CR, CRLF and NUL are ordinary bytes: a CRLF line keeps
  its CR and a NUL passes through, as the witnesses show.

  WHAT IS NOT PROMISED, precisely. (a) Negative and non-numeric counts: the
  lowering parses an i64 and the arms disagree on negatives (`head -n -5` means
  "all but the last five lines" to GNU and nothing like that to these renderers),
  so the model takes a nat; huge counts are fine because they simply hit the
  unchanged-input case. (b) Byte mode is modelled as a plain slice (`take n s`,
  `drop (length s - n) s`) and only its length bound is claimed; the lowering
  refuses byte mode today. (c) Encoding beyond byte pass-through: bytes are
  compared with 10 only, so UTF-8 and invalid UTF-8 pass through unchanged and no
  character counting is claimed. (d) Host/process behaviour of `head`/`tail`
  (files, `-q`, `-v`, SIGPIPE, exit status) is outside a pure text function.
  (e) The from-end arm is modelled by `drop_lf`: dropping `k` terminators is
  proved to remove exactly `k` LFs (`count_lf (drop_lf k s) = count_lf s - k`), and
  a text with fewer lines than the count comes back unchanged. The byte-level
  statement that the from-end output is a SUFFIX of the input is not claimed here;
  it needs an associativity witness for the appended prefix, and it is stated as an
  open refinement rather than assumed.\<close>

abbreviation nl :: nat where "nl \<equiv> 10"

fun count_lf :: "bytes \<Rightarrow> nat" where
  "count_lf [] = 0"
| "count_lf (c # cs) = (if c = nl then Suc (count_lf cs) else count_lf cs)"

abbreviation nlines :: "bytes \<Rightarrow> nat" where "nlines s \<equiv> Suc (count_lf s)"

fun cut :: "nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "cut 0 s = []"
| "cut (Suc k) [] = []"
| "cut (Suc k) (c # cs) =
     (if c = nl then (if k = 0 then [] else nl # cut k cs)
      else c # cut (Suc k) cs)"

abbreviation take_lines :: "nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "take_lines n s \<equiv> cut n s"

fun drop_lf :: "nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "drop_lf 0 s = s"
| "drop_lf (Suc k) [] = []"
| "drop_lf (Suc k) (c # cs) = (if c = nl then drop_lf k cs else drop_lf (Suc k) cs)"

abbreviation last_lines :: "nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "last_lines n s \<equiv> drop_lf (nlines s - n) s"

abbreviation take_bytes :: "nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "take_bytes n s \<equiv> take n s"

abbreviation last_bytes :: "nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "last_bytes n s \<equiv> drop (length s - n) s"

lemma cut_zero: "cut 0 s = []"
  by simp

lemma cut_nil: "cut n [] = []"
  by (induct n) simp_all

lemma cut_prefix: "\<exists>r. s = cut n s @ r"
  apply (induct s arbitrary: n)
   apply (case_tac n)
    apply simp
   apply simp
  apply (rename_tac a list n)
  apply (case_tac n)
   apply simp
  apply (rename_tac k)
  apply (case_tac "a = nl")
   apply (case_tac k)
    apply simp
   apply simp
  apply simp
  done

lemma cut_length_le: "length (cut n s) \<le> length s"
proof -
  obtain r where r: "s = cut n s @ r" using cut_prefix by blast
  have eq: "length s = length (cut n s) + length r" using r by (metis length_append)
  then show ?thesis by simp
qed

lemma cut_count: "count_lf (cut n s) = min (n - 1) (count_lf s)"
  apply (induct s arbitrary: n)
   apply (case_tac n)
    apply simp
   apply simp
  apply (rename_tac a list n)
  apply (case_tac n)
   apply simp
  apply (rename_tac k)
  apply (case_tac "a = nl")
   apply (case_tac k)
    apply simp
   apply (simp add: min_def)
  apply (simp add: min_def)
  done

lemma cut_all: "count_lf s < n \<Longrightarrow> cut n s = s"
  apply (induct s arbitrary: n)
   apply (case_tac n)
    apply simp
   apply simp
  apply (rename_tac a list n)
  apply (case_tac n)
   apply simp
  apply (rename_tac k)
  apply (case_tac "a = nl")
   apply (case_tac k)
    apply simp
   apply simp
  apply simp
  done

lemma min_shift: "0 < n \<Longrightarrow> min n (Suc c) = Suc (min (n - 1) c)"
  by (induct n) (simp_all add: min_def)

lemma cut_linecount: "0 < n \<Longrightarrow> nlines (cut n s) = min n (nlines s)"
proof -
  assume n0: "0 < n"
  have c: "count_lf (cut n s) = min (n - 1) (count_lf s)" by (rule cut_count)
  have m: "min n (nlines s) = Suc (min (n - 1) (count_lf s))"
    using n0 by (simp add: min_shift)
  from c show ?thesis using m by simp
qed

lemma cut_terminator: "0 < n \<Longrightarrow> n \<le> count_lf s \<Longrightarrow> \<exists>r. s = cut n s @ nl # r"
  apply (induct s arbitrary: n)
   apply (case_tac n)
    apply simp
   apply simp
  apply (rename_tac a list n)
  apply (case_tac n)
   apply simp
  apply (rename_tac k)
  apply (case_tac "a = nl")
   apply (case_tac k)
    apply simp
   apply simp
  apply simp
  done

lemma drop_lf_count: "count_lf (drop_lf k s) = count_lf s - k"
  apply (induct s arbitrary: k)
   apply (case_tac k)
    apply simp
   apply simp
  apply (rename_tac a list k)
  apply (case_tac k)
   apply simp
  apply (rename_tac j)
  apply (case_tac "a = nl")
   apply simp
  apply simp
  done

lemma last_lines_unchanged: "count_lf s < n \<Longrightarrow> last_lines n s = s"
proof -
  assume h: "count_lf s < n"
  have "nlines s - n = 0" using h by (simp add: Suc_leI)
  then show ?thesis by simp
qed

theorem take_lines_sound:
  "cut 0 s = [] \<and>
   nlines [] = 1 \<and>
   nlines [nl] = 2 \<and>
   (\<exists>r. s = cut n s @ r) \<and>
   length (cut n s) \<le> length s \<and>
   (count_lf s < n \<longrightarrow> cut n s = s) \<and>
   count_lf (cut n s) = min (n - 1) (count_lf s) \<and>
   (0 < n \<longrightarrow> nlines (cut n s) = min n (nlines s)) \<and>
   (0 < n \<and> n \<le> count_lf s \<longrightarrow> (\<exists>r. s = cut n s @ nl # r)) \<and>
   count_lf (drop_lf k s) = count_lf s - k \<and>
   (count_lf s < n \<longrightarrow> last_lines n s = s) \<and>
   take_bytes n s = take n s \<and>
   length (last_bytes n s) \<le> n"
proof (intro conjI)
  show "cut 0 s = []" by (rule cut_zero)
  show "nlines [] = 1" by simp
  show "nlines [nl] = 2" by simp
  show "\<exists>r. s = cut n s @ r" by (rule cut_prefix)
  show "length (cut n s) \<le> length s" by (rule cut_length_le)
  show "count_lf s < n \<longrightarrow> cut n s = s" by (simp add: cut_all)
  show "count_lf (cut n s) = min (n - 1) (count_lf s)" by (rule cut_count)
  show "0 < n \<longrightarrow> nlines (cut n s) = min n (nlines s)" by (simp add: cut_linecount)
  show "0 < n \<and> n \<le> count_lf s \<longrightarrow> (\<exists>r. s = cut n s @ nl # r)"
    by (simp add: cut_terminator)
  show "count_lf (drop_lf k s) = count_lf s - k" by (rule drop_lf_count)
  show "count_lf s < n \<longrightarrow> last_lines n s = s" by (simp add: last_lines_unchanged)
  show "take_bytes n s = take n s" by simp
  show "length (last_bytes n s) \<le> n" by simp
qed

lemma take_lines_witness:
  "cut 2 [108, 105, 110, 101, 49, 10, 108, 105, 110, 101, 50, 10, 108, 105, 110, 101, 51]
     = [108, 105, 110, 101, 49, 10, 108, 105, 110, 101, 50] \<and>
   cut 1 [108, 105, 110, 101, 49, 10, 108, 105, 110, 101, 50]
     = [108, 105, 110, 101, 49] \<and>
   cut 9 [108, 105, 110, 101, 49, 10, 108, 105, 110, 101, 50]
     = [108, 105, 110, 101, 49, 10, 108, 105, 110, 101, 50]"
  by eval

lemma take_lines_edge_witness:
  "cut 1 [] = [] \<and>
   nlines [] = 1 \<and>
   cut 0 [97, 10, 98] = [] \<and>
   cut 2 [97, 10, 98, 10] = [97, 10, 98] \<and>
   cut 3 [97, 10, 98, 10] = [97, 10, 98, 10] \<and>
   nlines [97, 10, 98, 10] = 3 \<and>
   cut 1 [97, 10] = [97] \<and>
   last_lines 1 [97, 10, 98, 10] = []"
  by eval

lemma take_lines_bytes_witness:
  "cut 1 [97, 13, 10, 98] = [97, 13] \<and>
   count_lf [97, 13, 10, 98] = 1 \<and>
   cut 1 [195, 169, 10, 98] = [195, 169] \<and>
   cut 1 [97, 0, 10, 98] = [97, 0]"
  by eval


section \<open>MapLiteral: a dict literal over parallel key/value lists\<close>

text \<open>`MapLiteral{keys, values}` is the anonymous dictionary VALUE in expression
  position (`map[K]V{...}`, a JS object literal). The renderer
  (`src/render_ext_estree/handlers/map_literal.rs`) is the specification: if
  `length keys \<noteq> length values` it returns `None` and the `sh2.*` fallback runs;
  otherwise it emits an ObjectExpression whose properties are the zipped pairs, in
  order, with keys marked `computed` — i.e. each key expression's RUNTIME value is
  stringified by the host object model before it becomes a property name.

  So the model is an association list of (key bytes, value bytes) pairs, plus one
  ABSTRACT function @{term st} standing for the host's key-stringification rule.
  What the node means is @{term map_lookup}: the last value whose stringified key
  equals the stringified query, and `None` when no key matches.

  WHAT IS PROVED (each below, over byte lists and an arbitrary @{term st}):
  (1) the entries ARE the zipped pairs, so the property list has `min` of the two
  lengths — the renderer's guard turns that into "as many properties as there are
  keys";
  (2) the structural law: appending one pair either wins over everything before it
  (when its stringified key matches the query) or leaves the earlier answer
  untouched — so a DUPLICATE key is resolved last-wins, which is what a JS object
  literal does, and a miss stays a miss;
  (3) consequently a matching final key yields exactly its own value even when
  earlier keys stringify identically, and a non-matching final key yields the
  earlier lookup;
  (4) a list none of whose keys stringify to the query answers `None`.

  WHAT IS NOT PROMISED, precisely. (a) The stringification rule itself is abstract:
  the host maps numbers to decimal text, `true`/`false`, `-0` to `"0"`, `NaN`,
  arrays/objects to `"[object Object]"`-like text, and `st` may therefore map two
  DISTINCT key expressions to the same property name — that collision is exactly why
  (2) is stated, and the concrete rule is not modelled. (b) Property ORDER and
  iteration are not promised: JS objects reorder integer-like property names, so the
  laws here are about LOOKUP, never about "the i-th property". (c) Key and value
  EXPRESSION evaluation order and side effects are outside a pure function; the model
  takes already-evaluated byte values. (d) The length-mismatch refusal is the
  renderer's guard, not a theorem here — it is a refuse>guess branch whose
  alternative is the unchanged `sh2.*` fallback. (e) Bytes only: no Unicode
  normalization or locale rules, so `st` is a byte function.\<close>

type_synonym kv = "bytes \<times> bytes"

abbreviation map_entries :: "bytes list \<Rightarrow> bytes list \<Rightarrow> kv list" where
  "map_entries ks vs \<equiv> zip ks vs"

fun map_lookup :: "(bytes \<Rightarrow> bytes) \<Rightarrow> bytes \<Rightarrow> kv list \<Rightarrow> bytes option" where
  "map_lookup st q [] = None"
| "map_lookup st q (p # r) =
     (if st (fst p) = st q then
        (case map_lookup st q r of None \<Rightarrow> Some (snd p) | Some w \<Rightarrow> Some w)
      else map_lookup st q r)"

lemma map_lookup_nil: "map_lookup st q [] = None"
  by simp

lemma map_lookup_snoc:
  "map_lookup st q (ps @ [(k, v)]) = (if st k = st q then Some v else map_lookup st q ps)"
  by (induct ps) (simp_all split: option.split)

lemma map_lookup_miss:
  "(\<And>p. p \<in> set ps \<Longrightarrow> st (fst p) \<noteq> st q) \<Longrightarrow> map_lookup st q ps = None"
  by (induct ps) (simp_all split: option.split)

lemma map_lookup_last_wins:
  "length ks = length vs \<Longrightarrow> st k = st q \<Longrightarrow>
     map_lookup st q (map_entries (ks @ [k]) (vs @ [v])) = Some v"
proof -
  assume len: "length ks = length vs"
  assume hit: "st k = st q"
  have zip: "map_entries (ks @ [k]) (vs @ [v]) = map_entries ks vs @ [(k, v)]"
    using len by (simp add: zip_append)
  show ?thesis using zip hit by (simp add: map_lookup_snoc)
qed

lemma map_lookup_passes:
  "length ks = length vs \<Longrightarrow> st k \<noteq> st q \<Longrightarrow>
     map_lookup st q (map_entries (ks @ [k]) (vs @ [v])) = map_lookup st q (map_entries ks vs)"
proof -
  assume len: "length ks = length vs"
  assume miss: "st k \<noteq> st q"
  have zip: "map_entries (ks @ [k]) (vs @ [v]) = map_entries ks vs @ [(k, v)]"
    using len by (simp add: zip_append)
  show ?thesis using zip miss by (simp add: map_lookup_snoc)
qed

theorem map_literal_sound:
  "map_lookup st q [] = None \<and>
   map_entries ks vs = zip ks vs \<and>
   length (map_entries ks vs) = min (length ks) (length vs) \<and>
   map_lookup st q (ps @ [(k, v)]) = (if st k = st q then Some v else map_lookup st q ps) \<and>
   ((\<forall>p \<in> set ps. st (fst p) \<noteq> st q) \<longrightarrow> map_lookup st q ps = None) \<and>
   (length ks = length vs \<and> st k = st q \<longrightarrow>
      map_lookup st q (map_entries (ks @ [k]) (vs @ [v])) = Some v) \<and>
   (length ks = length vs \<and> st k \<noteq> st q \<longrightarrow>
      map_lookup st q (map_entries (ks @ [k]) (vs @ [v])) = map_lookup st q (map_entries ks vs))"
proof (intro conjI)
  show "map_lookup st q [] = None" by (rule map_lookup_nil)
  show "map_entries ks vs = zip ks vs" by simp
  show "length (map_entries ks vs) = min (length ks) (length vs)" by (simp add: length_zip)
  show "map_lookup st q (ps @ [(k, v)]) = (if st k = st q then Some v else map_lookup st q ps)"
    by (rule map_lookup_snoc)
  show "(\<forall>p \<in> set ps. st (fst p) \<noteq> st q) \<longrightarrow> map_lookup st q ps = None"
    by (auto simp: map_lookup_miss)
  show "length ks = length vs \<and> st k = st q \<longrightarrow>
          map_lookup st q (map_entries (ks @ [k]) (vs @ [v])) = Some v"
    by (simp add: map_lookup_snoc)
  show "length ks = length vs \<and> st k \<noteq> st q \<longrightarrow>
          map_lookup st q (map_entries (ks @ [k]) (vs @ [v])) = map_lookup st q (map_entries ks vs)"
    by (simp add: map_lookup_snoc)
qed

lemma map_literal_witness:
  "map_lookup id [107] (map_entries [[107], [97]] [[49], [50]]) = Some [49] \<and>
   map_lookup id [107] (map_entries [[107], [107]] [[49], [50]]) = Some [50] \<and>
   map_lookup id [122] (map_entries [[107], [97]] [[49], [50]]) = None"
  by eval

lemma map_literal_collision_witness:
  "map_lookup (\<lambda>b. if b = [107] then [75] else b) [107]
     (map_entries [[107], [75]] [[49], [50]]) = Some [50] \<and>
   map_lookup (\<lambda>b. if b = [107] then [75] else b) [107]
     (map_entries [[75], [107]] [[49], [50]]) = Some [50]"
  by eval


section \<open>RegexpFind: the leftmost match, as a string\<close>

text \<open>`RegexpFind{text, pattern}` is a leftmost regex match returned AS A STRING, and
  the empty string when nothing matches (`sed`/`grep -o` family, Go's
  `re.FindString`). Every backend renders it by extracting the match from whatever its
  regex engine reports, so the obligation is stated over an ABSTRACT matcher rather
  than over one engine's answers: @{term m} reports, for a subject, the leftmost
  match's DECOMPOSITION `(prefix, matched text, suffix)`, or `None`.

  The node's value is then the middle component (`\<lambda>(p, v, q). v`, and `[]` for
  `None`), and the laws below are the ones a renderer actually relies on:
  (1) no match yields the empty string;
  (2) a reported match yields exactly the reported matched text — the node never
  re-derives, re-encodes or normalises it;
  (3) the empty string comes back exactly when there is no match OR the match is
  empty, so "" is not ambiguous evidence of a miss;
  (4) for any matcher that reports a GENUINE decomposition (`s = prefix @ match @
  suffix`), the returned bytes occur in the subject — which is what makes any
  `take`/`drop`-based rendering of the node sound.

  NOT PROMISED, precisely. (a) LEFTMOST-ness itself: the abstraction hands back the
  leftmost match, so a matcher that skipped an earlier match is not caught here; the
  engine's search order, its alternation preference and its greediness are the
  engine's contract, not this one. (b) The regex DIALECT (ERE/RE2-class char classes,
  anchors, quantifiers, groups), case folding, locale and Unicode rules: `pattern` is
  a string evaluated by the host engine and is not modelled. (c) Capture groups and
  backreferences are not part of this node's value. (d) `text` is an expression, so
  its evaluation (and any side effect) is outside a pure function; the model takes
  already-evaluated bytes. (e) Bytes only: no encoding, no Unicode normalisation.\<close>

abbreviation rf_val ::
  "(bytes \<Rightarrow> (bytes \<times> bytes \<times> bytes) option) \<Rightarrow> bytes \<Rightarrow> bytes" where
  "rf_val m s \<equiv> (case m s of None \<Rightarrow> [] | Some (p, v, q) \<Rightarrow> v)"

abbreviation rf_genuine ::
  "(bytes \<Rightarrow> (bytes \<times> bytes \<times> bytes) option) \<Rightarrow> bytes \<Rightarrow> bool" where
  "rf_genuine m s \<equiv> (\<forall>p v q. m s = Some (p, v, q) \<longrightarrow> s = p @ v @ q)"

lemma rf_no_match: "m s = None \<Longrightarrow> rf_val m s = []"
  by simp

lemma rf_match: "m s = Some (p, v, q) \<Longrightarrow> rf_val m s = v"
  by simp

lemma rf_empty_iff:
  "rf_val m s = [] \<longleftrightarrow> m s = None \<or> (\<exists>p q. m s = Some (p, [], q))"
  by (cases "m s") (auto split: prod.split)

lemma rf_occurs:
  "rf_genuine m s \<Longrightarrow> \<exists>p q. s = p @ rf_val m s @ q"
  by (cases "m s") (auto split: prod.split)

theorem regexp_find_sound:
  "rf_genuine m s \<longrightarrow>
     (m s = None \<longrightarrow> rf_val m s = []) \<and>
     (\<forall>p v q. m s = Some (p, v, q) \<longrightarrow> rf_val m s = v) \<and>
     (rf_val m s = [] \<longleftrightarrow> m s = None \<or> (\<exists>p q. m s = Some (p, [], q))) \<and>
     (\<exists>p q. s = p @ rf_val m s @ q)"
  by (intro impI conjI allI rf_occurs) (simp_all add: rf_empty_iff)

lemma regexp_find_witness:
  "rf_val (\<lambda>s. if s = [97, 98, 99] then Some ([], [98], [99]) else None) [97, 98, 99] = [98] \<and>
   rf_val (\<lambda>s. if s = [97, 98, 99] then Some ([97], [98], [99]) else None) [97, 98, 99] = [98] \<and>
   rf_val (\<lambda>s. None) [97, 98, 99] = [] \<and>
   rf_val (\<lambda>s. Some ([], [], s)) [97] = []"
  by simp


section \<open>RegSub: the substitution engine, non-global and global\<close>

text \<open>`RegSub{text, pattern, replacement, global, line_mode}` is `sed 's/p/r/'`:
  the renderer (`src/render_ext_expr/handlers/all_nodes.rs`, arm `reg_sub`) emits one
  substitution per line-shaped call — Perl `$t =~ s/p/r/` with the `g` modifier when
  `global`, JS `replace` with `"g"`, Python `re.sub(..., count=1|0)`, and Java through
  the `shRegSub`/`shRegSubAll` helper that expands sed's `&` and `\\N` itself.

  TWO FACTS ABOUT THE TREE DECIDE WHAT CAN BE STATED HONESTLY.
  (1) The lowering only builds this node for a subject that is a provably SINGLE-LINE
  literal (`transforms/text_ops.rs`: `single_line_literal` requires a string literal
  with no LF, because the node is a whole-string application and would otherwise match
  across newlines — a different function). A variable or multi-line subject falls back
  to the original command.
  (2) `line_mode` is DECLARED in the node and set by the proposed lowering in the
  backend worktrees, but NO renderer in this tree reads it. So the per-line arm is
  modelled at the abstraction level below and is not claimed to be what any backend
  emits today; the whole-text arms are.

  The model is therefore: an abstract matcher @{term m} reporting a leftmost match as a
  DECOMPOSITION `(prefix, matched, suffix)` or `None`, plus an abstract replacement
  @{term R} applied to the matched text. `R` is where sed's replacement LANGUAGE lives
  (`&` the match, `\N` a group), so the concrete syntax is the abstraction's contract
  rather than a dialect's; the per-line arm is `per_line` = map over `lines_of` and
  `join_lf`, the split keeping empty fields and the join re-inserting exactly one LF
  between lines.

  WHAT IS PROVED (each below, for an arbitrary matcher and replacement):
  (1) non-global is the one-step fold — the unmatched prefix, then `R` of the match,
  then the suffix untouched — and `rs_all ... 1` is exactly that;
  (2) global is the fuel-indexed fold over CONSECUTIVE decompositions of the successive
  suffixes, starting from 0 fuel at the identity;
  (3) a subject the matcher cannot match comes back byte-identical, non-global and
  global, at any fuel;
  (4) if the matcher reports genuine decompositions and `R` is the identity, the whole
  global fold is the identity — the engine invents and loses nothing;
  (5) for a subject with no LF (exactly the class the lowering builds the node for) the per-line
  fold equals the whole-subject substitution, so the two arms cannot disagree there.

  NOT PROMISED, precisely. (a) Greediness, alternation preference and search order —
  the matcher hands back the leftmost match, and an engine that chose a different one
  is not caught. (b) The regex DIALECT (ERE/RE2 classes, anchors, quantifiers,
  groups), case folding, locale and Unicode rules: `pattern` is a string the host
  engine interprets. (c) The replacement syntax itself (`&`, `\N`, `\\&`, `\\n`):
  `R` abstracts it, so a concrete expansion is not proved here — the Substitute theory
  is where `expand` lives. (d) Empty-match advance: the emitted code advances by the
  engine's own `find()` after a match, so a zero-length match's advance rule belongs to
  the engine; the fold here consumes whatever suffix the matcher reports. (e) The
  TRAILING NEWLINE of the source is the text's own business (the lowering's
  `trailing_newline` guard); the model takes the bytes it is given. (f) Argument
  evaluation order and side effects of `text`, and bytes-only encoding.\<close>

type_synonym rmatcher = "bytes \<Rightarrow> (bytes \<times> bytes \<times> bytes) option"

definition rs_first :: "rmatcher \<Rightarrow> (bytes \<Rightarrow> bytes) \<Rightarrow> bytes \<Rightarrow> bytes" where
  "rs_first m R s \<equiv> (case m s of None \<Rightarrow> s | Some (p, v, q) \<Rightarrow> p @ R v @ q)"

fun rs_all :: "rmatcher \<Rightarrow> (bytes \<Rightarrow> bytes) \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "rs_all m R 0 s = s"
| "rs_all m R (Suc k) s =
     (case m s of None \<Rightarrow> s | Some (p, v, q) \<Rightarrow> p @ R v @ rs_all m R k q)"

lemma rs_first_miss: "m s = None \<Longrightarrow> rs_first m R s = s"
  by (simp add: rs_first_def)

lemma rs_first_hit: "m s = Some (p, v, q) \<Longrightarrow> rs_first m R s = p @ R v @ q"
  by (simp add: rs_first_def)

lemma rs_all_miss: "m s = None \<Longrightarrow> rs_all m R k s = s"
  by (induct k) (simp_all split: prod.split)

lemma rs_all_one: "rs_all m R (Suc 0) s = rs_first m R s"
  by (cases "m s") (auto split: prod.split simp: rs_first_def)

lemma rs_all_identity:
  assumes gen: "\<And>t p v q. m t = Some (p, v, q) \<Longrightarrow> t = p @ v @ q"
    and idR: "\<And>v. R v = v"
  shows "rs_all m R k s = s"
proof (induct k arbitrary: s)
  case 0
  then show ?case by simp
next
  case (Suc k)
  show ?case
  proof (cases "m s")
    case None
    then show ?thesis by (simp add: rs_all_miss)
  next
    case (Some z)
    then obtain p v q where z: "z = (p, v, q)"
      by (cases z) (auto split: prod.split)
    have sz: "s = p @ v @ q" using Some z by (simp add: gen)
    have ih: "rs_all m R k q = q" using Suc.hyps Some z by simp
    have rv: "R v = v" using idR by simp
    show ?thesis using Some z sz ih rv by simp
  qed
qed

fun split_lf :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes list \<Rightarrow> bytes list" where
  "split_lf [] cur acc = rev (cur # acc)"
| "split_lf (c # cs) cur acc =
     (if c = nl then split_lf cs [] (cur # acc) else split_lf cs (cur @ [c]) acc)"

definition lines_of :: "bytes \<Rightarrow> bytes list" where
  "lines_of s \<equiv> split_lf s [] []"

lemma split_lf_no_lf:
  "(\<And>c. c \<in> set s \<Longrightarrow> c \<noteq> nl) \<Longrightarrow> split_lf s cur acc = rev acc @ [cur @ s]"
  by (induct s arbitrary: cur acc) (simp_all add: append_assoc)

lemma lines_of_no_lf:
  "(\<And>c. c \<in> set s \<Longrightarrow> c \<noteq> nl) \<Longrightarrow> lines_of s = [s]"
  by (simp add: lines_of_def split_lf_no_lf)

fun join_lf :: "bytes list \<Rightarrow> bytes" where
  "join_lf [] = []"
| "join_lf (l # r) = l @ (case r of [] \<Rightarrow> [] | _ \<Rightarrow> nl # join_lf r)"

lemma join_lf_single [simp]: "join_lf [l] = l"
  by simp

definition per_line :: "(bytes \<Rightarrow> bytes) \<Rightarrow> bytes \<Rightarrow> bytes" where
  "per_line f s \<equiv> join_lf (map f (lines_of s))"

lemma per_line_single:
  "(\<And>c. c \<in> set s \<Longrightarrow> c \<noteq> nl) \<Longrightarrow> per_line f s = f s"
  by (simp add: per_line_def lines_of_no_lf)

theorem reg_sub_sound:
  "rs_first m R s = (case m s of None \<Rightarrow> s | Some (p, v, q) \<Rightarrow> p @ R v @ q) \<and>
   rs_all m R 0 s = s \<and>
   rs_all m R (Suc k) s =
     (case m s of None \<Rightarrow> s | Some (p, v, q) \<Rightarrow> p @ R v @ rs_all m R k q) \<and>
   (m s = None \<longrightarrow> rs_first m R s = s \<and> rs_all m R k s = s) \<and>
   rs_all m R (Suc 0) s = rs_first m R s \<and>
   ((\<forall>t p v q. m t = Some (p, v, q) \<longrightarrow> t = p @ v @ q) \<and> (\<forall>v. R v = v)
      \<longrightarrow> rs_all m R k s = s) \<and>
   ((\<forall>c \<in> set s. c \<noteq> nl) \<longrightarrow> per_line f s = f s)"
proof (intro conjI)
  show "rs_first m R s = (case m s of None \<Rightarrow> s | Some (p, v, q) \<Rightarrow> p @ R v @ q)"
    by (simp add: rs_first_def)
  show "rs_all m R 0 s = s" by simp
  show "rs_all m R (Suc k) s =
          (case m s of None \<Rightarrow> s | Some (p, v, q) \<Rightarrow> p @ R v @ rs_all m R k q)"
    by (cases "m s") (auto split: prod.split)
  show "m s = None \<longrightarrow> rs_first m R s = s \<and> rs_all m R k s = s"
    by (auto intro: rs_first_miss rs_all_miss)
  show "rs_all m R (Suc 0) s = rs_first m R s" by (rule rs_all_one)
  show "(\<forall>t p v q. m t = Some (p, v, q) \<longrightarrow> t = p @ v @ q) \<and> (\<forall>v. R v = v)
          \<longrightarrow> rs_all m R k s = s"
    by (auto intro: rs_all_identity)
  show "(\<forall>c \<in> set s. c \<noteq> nl) \<longrightarrow> per_line f s = f s"
    by (auto intro!: per_line_single)
qed

lemma reg_sub_witness:
  "rs_all (\<lambda>s. case s of [] \<Rightarrow> None | (c # r) \<Rightarrow> Some ([], [c], r)) (\<lambda>v. [118]) 3 [97, 98, 99]
     = [118, 118, 118] \<and>
   rs_all (\<lambda>s. case s of [] \<Rightarrow> None | (c # r) \<Rightarrow> Some ([], [c], r)) (\<lambda>v. [118]) 1 [97, 98, 99]
     = [118, 98, 99] \<and>
   rs_first (\<lambda>s. None) (\<lambda>v. [118]) [97, 98] = [97, 98] \<and>
   rs_all (\<lambda>s. None) (\<lambda>v. [118]) 5 [97, 98] = [97, 98] \<and>
   rs_all (\<lambda>s. case s of [] \<Rightarrow> None | (c # r) \<Rightarrow> Some ([], [c], r)) (\<lambda>v. []) 2 [97, 98] = []"
  by eval

lemma reg_sub_line_witness:
  "per_line (\<lambda>s. s @ [33]) [97, 98] = [97, 98, 33] \<and>
   lines_of [97, 98] = [[97, 98]]"
  by eval


section \<open>ReadLine, ElementRead, FieldRead, TypeAssert: readback laws\<close>

text \<open>Four nodes that looked like pure effects but each carry a real denotational
  contract, and each contract is what a renderer has to preserve.

  \<^bold>\<open>ReadLine\<close> (declared `effect: I/O`) is "one line from standard input, the
  value WITHOUT its trailing newline, EOF gives the empty string". At the byte level
  that is exactly the first feature of the LF model above: the value is the text cut
  at the first LF with that LF dropped (@{term cut} at count 1), so it contains NO LF
  at all — the chomp law, which is what a caller relies on when it compares or
  re-terminates the line. A subject with no LF is read whole and unchanged (that is
  the EOF case, since nothing was terminated), and a subject with an LF splits into
  the value plus that terminator plus a remainder. What is NOT promised is anything
  about the DEVICE: blocking, partial reads, a closed descriptor, the file pointer's
  position, or a final line with no terminator being reported as EOF — those belong to
  the runtime's stream, not to this function. Neither is any encoding, or `read` with
  several variables (the subset reads one; IFS splitting belongs to a different
  lowering).

  \<^bold>\<open>ElementRead\<close> (`m["key"]` on a literal) and \<^bold>\<open>FieldRead\<close> (`p.x`)
  both read one entry out of a composite VALUE, and the file for each says so: the key
  "stringifies like the host object model", and a numeric field name is rendered as an
  INDEX. So both land on the MapLiteral association list: an element read of a query
  is @{term map_lookup} over the zipped entries, and a field read is the same thing
  with the field name's bytes as the query — which makes the two nodes one function at
  two spellings, proved below. The readback law that matters is that reading a key
  that occurs LAST in a literal yields that last value, so a duplicate key reads back
  last-wins exactly as the literal's own lookup does. What is NOT promised: the
  stringification rule (abstract @{term st}, as for `map_literal`), the ORDER of
  enumeration or of property storage (JS reorders integer-like names), indexing
  ARITHMETIC on arrays (`arr[i]` as an offset, out-of-range behaviour), missing-key
  policy beyond "no such entry" (`null` vs `undef` vs an error is the host's), and
  tuple-index reads, which `field_read` says are not this node.

  \<^bold>\<open>TypeAssert\<close> is documented as a CHECKED PASSTHROUGH: the value flows through
  unchanged and `kind` only names the expected dynamic kind. The law is therefore that
  the node's value is its operand for EVERY kind, and that two different kinds give the
  same value — no coercion, which is the property a backend could most easily break.
  What is NOT promised: that the check actually FIRES (the IR is dynamically typed, so
  the assertion is the frontend's guard plus whatever a backend chooses to validate),
  the comma-ok boolean (the frontend emits the `typeof` comparison itself), and the
  `kind` vocabulary beyond the five pinned strings.\<close>

definition rl_line :: "bytes \<Rightarrow> bytes" where
  "rl_line s \<equiv> cut 1 s"

lemma count_lf_snoc_nl: "count_lf (r @ [nl]) = Suc (count_lf r)"
  by (induct r) simp_all

lemma rl_line_clean: "count_lf (rl_line s) = 0"
  by (simp add: rl_line_def cut_count)

theorem read_line_sound:
  "count_lf (rl_line s) = 0 \<and>
   (count_lf s = 0 \<longrightarrow> rl_line s = s) \<and>
   (0 < count_lf s \<longrightarrow> (\<exists>r. s = rl_line s @ nl # r))"
proof (intro conjI)
  show "count_lf (rl_line s) = 0" by (rule rl_line_clean)
  show "count_lf s = 0 \<longrightarrow> rl_line s = s"
    by (simp add: rl_line_def cut_all)
  show "0 < count_lf s \<longrightarrow> (\<exists>r. s = rl_line s @ nl # r)"
    by (simp add: rl_line_def cut_terminator)
qed

lemma read_line_witness:
  "rl_line [97, 10, 98] = [97] \<and>
   rl_line [97, 98] = [97, 98] \<and>
   rl_line [] = [] \<and>
   count_lf (rl_line [97, 10]) = 0"
  by eval

definition er_read ::
  "(bytes \<Rightarrow> bytes) \<Rightarrow> bytes \<Rightarrow> bytes list \<Rightarrow> bytes list \<Rightarrow> bytes option" where
  "er_read st q ks vs \<equiv> map_lookup st q (map_entries ks vs)"

definition fr_read ::
  "(bytes \<Rightarrow> bytes) \<Rightarrow> bytes \<Rightarrow> bytes list \<Rightarrow> bytes list \<Rightarrow> bytes option" where
  "fr_read st name ks vs \<equiv> er_read st name ks vs"

lemma er_read_last:
  "length ks = length vs \<Longrightarrow> st k = st q \<Longrightarrow>
     er_read st q (ks @ [k]) (vs @ [v]) = Some v"
  by (simp add: er_read_def map_lookup_snoc)

lemma fr_read_is_er: "fr_read st name ks vs = er_read st name ks vs"
  by (simp add: fr_read_def)

theorem element_read_sound:
  "er_read st q ks vs = map_lookup st q (map_entries ks vs) \<and>
   (length ks = length vs \<and> st k = st q \<longrightarrow>
      er_read st q (ks @ [k]) (vs @ [v]) = Some v) \<and>
   ((\<forall>p \<in> set (map_entries ks vs). st (fst p) \<noteq> st q) \<longrightarrow>
      er_read st q ks vs = None)"
proof (intro conjI)
  show "er_read st q ks vs = map_lookup st q (map_entries ks vs)"
    by (simp add: er_read_def)
  show "length ks = length vs \<and> st k = st q \<longrightarrow>
          er_read st q (ks @ [k]) (vs @ [v]) = Some v"
    by (auto intro: er_read_last)
  show "(\<forall>p \<in> set (map_entries ks vs). st (fst p) \<noteq> st q) \<longrightarrow>
          er_read st q ks vs = None"
    by (auto simp: er_read_def intro: map_lookup_miss)
qed

theorem field_read_sound:
  "fr_read st name ks vs = er_read st name ks vs \<and>
   (length ks = length vs \<and> st k = st name \<longrightarrow>
      fr_read st name (ks @ [k]) (vs @ [v]) = Some v)"
proof (intro conjI)
  show "fr_read st name ks vs = er_read st name ks vs" by (rule fr_read_is_er)
  show "length ks = length vs \<and> st k = st name \<longrightarrow>
          fr_read st name (ks @ [k]) (vs @ [v]) = Some v"
    by (simp add: fr_read_def er_read_def map_lookup_snoc)
qed

lemma element_read_witness:
  "er_read id [107] [[107], [107]] [[49], [50]] = Some [50] \<and>
   er_read id [107] [[107], [97]] [[49], [50]] = Some [49] \<and>
   er_read id [122] [[107]] [[49]] = None"
  by eval

lemma field_read_witness:
  "fr_read id [120] [[120]] [[49]] = Some [49] \<and>
   fr_read id [120] [[120], [120]] [[49], [50]] = Some [50]"
  by eval

definition ta_val :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "ta_val k v \<equiv> v"

theorem type_assert_sound:
  "ta_val k v = v \<and> ta_val k1 v = ta_val k2 v"
  by (simp add: ta_val_def)


section \<open>CountedFor, Spread, AddressOf/Deref, CloneDeep: value-level laws\<close>

text \<open>Five more parked-as-effect nodes whose VALUE-level contract the model here can
  state honestly. Each law below is one a plausible broken implementation violates.

  \<^bold>\<open>CountedFor\<close> (declared `effect: control flow`) is a counted loop with an
  initializer, a step and a terminal condition; the loop variable's sequence is
  arithmetic and needs no model of the body's store effect. @{term counted_vals}
  generates the values the loop variable takes, and the laws pin exactly what an
  off-by-one or a dropped increment would break: the sequence's LENGTH is the iteration
  budget, its n-th element is `init + n * step`, so the first value is the initializer
  and successive values differ by the step. What is NOT promised: the body's effect (no
  store model here), overflow and the host integer width (the model is mathematical
  `int`), the terminal condition's own evaluation cost, and `break`/`continue`, which the
  node does not carry.

  \<^bold>\<open>Spread\<close> (`f(args...)`, `...arr`) flattens its operand into the argument or
  element vector; @{term spread_seq} is that flattening. The laws are the ones a
  broken flattening violates: singletons flatten to themselves (so spreading a list of
  one-element groups is the identity), the flattened LENGTH is the sum of the parts'
  lengths, appending argument groups concatenates their flattened vectors (so order is
  preserved up to where the spread sits, and a spread in the middle does not reorder
  its neighbours), and duplicate elements survive a flatten — a multiset property. What
  is NOT promised: where a spread is LEGAL (`SpreadElement` / varargs position only — the
  node says a frontend that emits it elsewhere gets a generator error), the host's
  arity/type checking of the resulting call, and evaluation order of the operand.

  \<^bold>\<open>AddressOf\<close> and \<^bold>\<open>Deref\<close> are the two halves of one contract: an address
  denotes a CELL and a dereference reads it. A minimal heap (a total function from
  address to bytes, with @{term hwrite} the functional update and @{term hread} the
  application) is enough to state it, and the laws are the roundtrips a backend that
  rendered `&x`/`*p` as the value itself would fail: a write through an address followed
  by a read of that address yields exactly the stored bytes, writing back what was read
  leaves the heap untouched, and writing a DIFFERENT cell does not disturb this one. The
  node files say value-only backends pass the operand through instead (snapshot
  semantics), which is the second half of `deref_sound`'s first conjunct. What is NOT
  promised: allocation/freshness (addresses are given, not minted), aliasing freedom in
  the source language, nil-dereference detection (the frontend's guards), and writes
  THROUGH a dereference, which `deref`'s file says has no node by design.

  \<^bold>\<open>CloneDeep\<close> copies a cell; the two laws are what a shallow copy breaks: the copy's
  VALUE equals the source's, and mutating the copy through a different address leaves the
  source's value unchanged. The address being different is a side condition here, not a
  theorem, and that is the honest boundary: minting a fresh address is the allocator's
  job, and a heap where two addresses could denote one cell would need an alias model
  this theory does not have. What is NOT promised: sharing INSIDE the copy (a deep copy
  of a nested structure sharing a sub-cell), cycle handling, and the lifetime/GC story.\<close>

fun counted_vals :: "int \<Rightarrow> int \<Rightarrow> nat \<Rightarrow> int list" where
  "counted_vals init step 0 = []"
| "counted_vals init step (Suc k) = init # counted_vals (init + step) step k"

lemma cv_length: "length (counted_vals init step k) = k"
  by (induct k arbitrary: init) simp_all

lemma cv_nth: "n < k \<Longrightarrow> nth (counted_vals init step k) n = init + of_nat n * step"
  apply (induct k arbitrary: init n)
   apply simp
  apply (rename_tac k init n)
  apply (case_tac n)
   apply simp
  apply (rename_tac m)
  apply (simp add: algebra_simps)
  done

theorem counted_for_sound:
  "length (counted_vals init step k) = k \<and>
   (\<forall>n. n < k \<longrightarrow> nth (counted_vals init step k) n = init + of_nat n * step) \<and>
   (\<forall>n. Suc n < k \<longrightarrow>
      nth (counted_vals init step k) (Suc n) = nth (counted_vals init step k) n + step)"
proof (intro conjI)
  show "length (counted_vals init step k) = k" by (rule cv_length)
  show "\<forall>n. n < k \<longrightarrow> nth (counted_vals init step k) n = init + of_nat n * step"
    by (auto simp: cv_nth)
  show "\<forall>n. Suc n < k \<longrightarrow>
          nth (counted_vals init step k) (Suc n) = nth (counted_vals init step k) n + step"
    by (auto simp: cv_nth algebra_simps)
qed

lemma counted_for_witness:
  "counted_vals 1 2 4 = [1, 3, 5, 7] \<and>
   nth (counted_vals 5 (- 2) 3) 2 = 1 \<and>
   length (counted_vals 0 3 5) = 5"
  by eval

abbreviation spread_seq :: "'a list list \<Rightarrow> 'a list" where
  "spread_seq ls \<equiv> concat ls"

theorem spread_sound:
  "spread_seq (map (\<lambda>x. [x]) xs) = xs \<and>
   length (spread_seq ls) = sum_list (map length ls) \<and>
   spread_seq (a @ b) = spread_seq a @ spread_seq b \<and>
   spread_seq [[[49]], [[49]]] = [[49], [49]]"
proof (intro conjI)
  show "spread_seq (map (\<lambda>x. [x]) xs) = xs" by simp
  show "length (spread_seq ls) = sum_list (map length ls)" by (induct ls) simp_all
  show "spread_seq (a @ b) = spread_seq a @ spread_seq b" by simp
  show "spread_seq [[[49]], [[49]]] = [[49], [49]]" by simp
qed

type_synonym heap = "nat \<Rightarrow> bytes"

definition hread :: "nat \<Rightarrow> heap \<Rightarrow> bytes" where
  "hread a h \<equiv> h a"

definition hwrite :: "nat \<Rightarrow> bytes \<Rightarrow> heap \<Rightarrow> heap" where
  "hwrite a v h \<equiv> h(a := v)"

definition clone_cell :: "nat \<Rightarrow> nat \<Rightarrow> heap \<Rightarrow> heap" where
  "clone_cell a b h \<equiv> h(b := h a)"

theorem address_of_sound:
  "hread a (hwrite a v h) = v \<and> hwrite a (hread a h) h = h"
  by (simp add: hread_def hwrite_def)

theorem deref_sound:
  "hread a (hwrite a v h) = v \<and>
   (b \<noteq> a \<longrightarrow> hread a (hwrite b v h) = hread a h)"
  by (simp add: hread_def hwrite_def)

theorem clone_deep_sound:
  "hread b (clone_cell a b h) = hread a h \<and>
   (b \<noteq> a \<longrightarrow> hread a (hwrite b v (clone_cell a b h)) = hread a h)"
  by (simp add: hread_def hwrite_def clone_cell_def)

lemma store_witness:
  "hread 0 (hwrite 0 [49] (\<lambda>n. [])) = [49] \<and>
   hread 1 (clone_cell 0 1 (\<lambda>n. if n = 0 then [49] else [])) = [49] \<and>
   hread 0 (hwrite 1 [50] (clone_cell 0 1 (\<lambda>n. if n = 0 then [49] else []))) = [49]"
  by eval


section \<open>WalkDir: the visited path set\<close>

text \<open>`WalkDir` visits a directory subtree. Its contract is not an effect: the visited
  paths are a pure function of the tree, so the law is about WHICH paths appear. The model
  is the minimal one that makes the traversal total without cheating — a tree is a leaf
  (@{term DFile}) or a directory holding an ASSOCIATION LIST of (name, child) pairs, paths
  are lists of names from the root (so there is no `..` component to step above the root
  with and no absolute form to escape it), and the entry set is defined by a SET-LEVEL
  recursion (@{term entries_set}) that never mentions the fold's order or its prefixing
  mechanism.

  THE LAW, and why each conjunct bites:
  (1) THE VISITED SET IS THE ENTRY SET: @{term "set (walk t) = entries_set t"}. A walker
  that DROPS a child fails it (the child's own path is missing); one that emits BARE NAMES
  for descendants fails it (the entry set holds `n # r` for a real child `n`, so `c` is not
  an entry of the root when `b/c` is); one that steps ABOVE the root — the empty path, or a
  first name that is no child — fails it.
  (2) NO EMPTY PATH IS VISITED: @{term "[] \<notin> set (walk t)"}, so neither the root itself
  nor anything above it is ever emitted.
  (3) EVERY ENTRY DECLINES INTO A REAL CHILD: an entry is `n # r` for a child `(n, t)` of the
  directory, with `r` empty (the child itself) or an entry of that child — the "none escapes
  the root" statement in structural form, since each step goes strictly deeper into an
  existing child.
  (4) THE EMISSION COUNT IS THE ENTRY COUNT (@{term entries_of}): together with (1) this is
  what pins "exactly once each" at the value level — a duplicate emission raises the count
  above the entry count, and by (1) every entry must still appear, so a duplicate necessarily
  costs a dropped entry, which (1) then forbids.
  (5) ORDER IS FREE, not unstated: the laws never mention order, and that is shown by
  running the fold with the child list REVERSED (@{term walk_rev}) and proving the visited
  set is unchanged. So a renderer may take any fold order over the same children.

  OPEN REFINEMENT, named precisely. The INJECTIVE form @{term "distinct (walk t)"} — "each
  entry emitted at most once" as a property of the list rather than via (1)+(4) — is not
  proved here: it needs the per-child DISJOINTNESS lemma (paths from distinct child names
  cannot coincide, which requires a `hd`-tagging argument over the child list) and its
  well-formedness hypothesis @{term wf_tree} (distinct child names). The count-plus-set
  pair above carries the practical content; the injective statement is the refinement.

  NOT PROMISED. (a) The traversal ORDER itself (depth-first here) and the order of
  `opendir` results. (b) Symlinks and CYCLES: this is a finite tree, so a cyclic filesystem
  object cannot be represented at all; a real walker needs a visited-inode set and a
  cycle policy, which are not modelled. (c) Metadata, permissions, entry name encoding, IO
  errors, and laziness (this fold materialises the list).\<close>

datatype dtree = DFile | DDir "(bytes \<times> dtree) list"

fun wf_tree :: "dtree \<Rightarrow> bool" where
  "wf_tree DFile = True"
| "wf_tree (DDir cs) = (distinct (map fst cs) \<and> Ball (set cs) (\<lambda>q. wf_tree (snd q)))"

fun walk :: "dtree \<Rightarrow> bytes list list" where
  "walk DFile = []"
| "walk (DDir cs) = concat (map (\<lambda>q. map ((#) (fst q)) ([] # walk (snd q))) cs)"

fun walk_rev :: "dtree \<Rightarrow> bytes list list" where
  "walk_rev DFile = []"
| "walk_rev (DDir cs) = concat (map (\<lambda>q. map ((#) (fst q)) ([] # walk_rev (snd q))) (rev cs))"

fun entries_set :: "dtree \<Rightarrow> bytes list set" where
  "entries_set DFile = {}"
| "entries_set (DDir cs) =
     (\<Union>q \<in> set cs. insert (fst q # []) ((\<lambda>r. fst q # r) ` entries_set (snd q)))"

fun entries_of :: "dtree \<Rightarrow> nat" where
  "entries_of DFile = 0"
| "entries_of (DDir cs) = length cs + sum_list (map (\<lambda>q. entries_of (snd q)) cs)"

lemma len_concat_map: "length (concat (map g cs)) = sum_list (map (\<lambda>c. length (g c)) cs)"
  by (induct cs) simp_all

lemma sum_list_suc_map: "sum_list (map (\<lambda>c. Suc (f c)) cs) = length cs + sum_list (map f cs)"
  by (induct cs) simp_all

lemma sum_list_map_cong:
  assumes h: "\<And>x. x \<in> set cs \<Longrightarrow> f x = g x"
  shows "sum_list (map f cs) = sum_list (map g cs)"
  using h by (induct cs) simp_all

lemma sum_list_rev_map:
  fixes f :: "'a \<Rightarrow> 'a :: comm_monoid_add"
  shows "sum_list (map f (rev cs)) = sum_list (map f cs)"
  by (induct cs) (simp_all add: ac_simps)

lemma walk_length: "length (walk t) = entries_of t"
  apply (induct t rule: walk.induct)
   apply simp
  apply (rename_tac cs)
  apply (simp add: len_concat_map sum_list_suc_map)
  apply (rule sum_list_map_cong)
  apply (simp add: Ball_def)
  done

lemma mem_child_part:
  "p \<in> set (map ((#) n) ([] # xs)) \<longleftrightarrow> (\<exists>r. p = n # r \<and> (r = [] \<or> r \<in> set xs))"
  by (auto simp: set_map)

lemma walk_set: "set (walk t) = entries_set t"
  apply (induct t rule: walk.induct)
   apply simp
  apply (rename_tac cs)
  apply (simp add: set_concat mem_child_part set_map)
  done

lemma entries_no_nil: "[] \<notin> entries_set t"
  apply (induct t rule: walk.induct)
   apply simp
  apply (rename_tac cs)
  apply auto
  done

lemma walk_nil_not: "[] \<notin> set (walk t)"
  by (simp add: walk_set entries_no_nil)

lemma entries_decline:
  "p \<in> entries_set (DDir cs) \<Longrightarrow>
     (\<exists>q \<in> set cs. \<exists>r. p = fst q # r \<and> (r = [] \<or> r \<in> entries_set (snd q)))"
  by auto

lemma walk_rev_set: "set (walk_rev t) = set (walk t)"
  apply (induct t rule: walk.induct)
   apply simp
  apply (rename_tac cs)
  apply (simp add: set_concat)
  done

theorem walk_dir_sound:
  "set (walk t) = entries_set t \<and>
   ([] \<notin> set (walk t)) \<and>
   (\<forall>p \<in> entries_set (DDir cs).
      (\<exists>q \<in> set cs. \<exists>r. p = fst q # r \<and> (r = [] \<or> r \<in> entries_set (snd q)))) \<and>
   length (walk t) = entries_of t \<and>
   set (walk_rev t) = set (walk t)"
proof (intro conjI)
  show "set (walk t) = entries_set t" by (rule walk_set)
  show "[] \<notin> set (walk t)" by (rule walk_nil_not)
  show "\<forall>p \<in> entries_set (DDir cs).
          (\<exists>q \<in> set cs. \<exists>r. p = fst q # r \<and> (r = [] \<or> r \<in> entries_set (snd q)))"
    by (auto intro: entries_decline)
  show "length (walk t) = entries_of t" by (rule walk_length)
  show "set (walk_rev t) = set (walk t)" by (rule walk_rev_set)
qed

(* a directory holding a file and a subdirectory, plus an empty directory *)
lemma walk_dir_witness:
  "walk (DDir [([100], DFile), ([101], DDir [([102], DFile)])])
     = [[[100]], [[101]], [[101], [102]]] \<and>
   walk (DDir [([100], DDir [])]) = [[[100]]] \<and>
   walk (DDir []) = [] \<and>
   entries_of (DDir [([100], DFile), ([101], DDir [([102], DFile)])]) = 3 \<and>
   set (walk_rev (DDir [([100], DFile), ([101], DDir [([102], DFile)])]))
     = set (walk (DDir [([100], DFile), ([101], DDir [([102], DFile)])]))"
  by eval

text \<open>The four defective traversals below are plausible implementation slips, not straw men:
  copying an entry instead of the head element (dup), skipping subdirectories as if they were
  unreadable (drop), prefixing only one level and concatenating deeper names (bare), and
  reporting the containing directory before descending (up). Each is falsified below on the
  same witness trees @{thm walk_dir_witness} is checked on, against the same definitions
  (see the closing text block: "Why the conjuncts bite").\<close>

definition w_nested :: dtree where
  "w_nested \<equiv> DDir [([100], DFile), ([101], DDir [([102], DFile)])]"

definition w_emptydir :: dtree where
  "w_emptydir \<equiv> DDir [([100], DDir [])]"

definition w_root :: dtree where
  "w_root \<equiv> DDir []"

fun child_names :: "dtree \<Rightarrow> bytes list" where
  "child_names DFile = []"
| "child_names (DDir cs) = map fst cs"

fun is_file :: "dtree \<Rightarrow> bool" where
  "is_file DFile = True"
| "is_file (DDir _) = False"

fun self_of :: "dtree \<Rightarrow> bytes list list" where
  "self_of DFile = [[]]"
| "self_of (DDir _) = []"

fun walk_dup :: "dtree \<Rightarrow> bytes list list" where
  "walk_dup DFile = []"
| "walk_dup (DDir cs) = concat (map (\<lambda>q. map ((#) (fst q)) ([] # [] # walk_dup (snd q))) cs)"

fun walk_drop :: "dtree \<Rightarrow> bytes list list" where
  "walk_drop DFile = []"
| "walk_drop (DDir cs) = concat (map (\<lambda>q. map ((#) (fst q)) (self_of (snd q))) cs)"

fun walk_bare :: "dtree \<Rightarrow> bytes list list" where
  "walk_bare DFile = []"
| "walk_bare (DDir cs) = concat (map (\<lambda>q. (fst q # []) # walk_bare (snd q)) cs)"

fun walk_up :: "dtree \<Rightarrow> bytes list list" where
  "walk_up DFile = []"
| "walk_up (DDir cs) = [] # concat (map (\<lambda>q. map ((#) (fst q)) ([] # walk_up (snd q))) cs)"

text \<open>What the GOOD walker emits on the witness trees. Every defect witness below is stated
  against these same lists and against the law's own counters, never against a bespoke one.\<close>

lemma walk_nested_emits: "walk w_nested = [[[100]], [[101]], [[101], [102]]]"
  by eval

lemma walk_emptydir_emits: "walk w_emptydir = [[[100]]]"
  by eval

lemma walk_root_emits: "walk w_root = []"
  by eval

lemma entries_of_nested: "entries_of w_nested = 3"
  by eval

text \<open>(1) DUPLICATE EMITTER breaks conjunct (4), the emission count. The law's own
  @{const entries_of} is the counter.\<close>

lemma dup_breaks_count: "length (walk_dup w_nested) \<noteq> entries_of w_nested"
  by eval

lemma dup_breaks_count_shallow: "length (walk_dup w_emptydir) \<noteq> entries_of w_emptydir"
  by eval

text \<open>(2) DROPPING WALKER breaks conjunct (1), visited set = entry set. The missed entry is a
  real entry of the subtree (the file inside the subdirectory), so it is an omission, not a
  difference in presentation.\<close>

lemma drop_breaks_entry:
  "[[101], [102]] \<in> set (walk w_nested) \<and> [[101], [102]] \<notin> set (walk_drop w_nested)"
proof
  show "[[101], [102]] \<in> set (walk w_nested)" using walk_nested_emits by auto
next
  have d: "walk_drop w_nested = [[[100]]]" by eval
  show "[[101], [102]] \<notin> set (walk_drop w_nested)" using d by auto
qed

lemma drop_breaks_set: "set (walk_drop w_nested) \<noteq> entries_set w_nested"
proof -
  have g: "[[101], [102]] \<in> set (walk w_nested)" using walk_nested_emits by auto
  with walk_set have e: "[[101], [102]] \<in> entries_set w_nested" by auto
  have n: "[[101], [102]] \<notin> set (walk_drop w_nested)" using drop_breaks_entry by blast
  from e n show ?thesis by auto
qed

lemma drop_empties_subtree: "walk_drop w_emptydir = []"
  by eval

lemma drop_breaks_set_shallow: "set (walk_drop w_emptydir) \<noteq> entries_set w_emptydir"
proof -
  have g: "[[100]] \<in> set (walk w_emptydir)" using walk_emptydir_emits by auto
  with walk_set have e: "[[100]] \<in> entries_set w_emptydir" by auto
  have n: "[[100]] \<notin> set (walk_drop w_emptydir)" using drop_empties_subtree by auto
  from e n show ?thesis by auto
qed

text \<open>(3) BARE-NAME WALKER breaks conjunct (3), declines-into-a-real-child. Its emitted
  @{term "[[102]]"} does not start with a child name of the root. The same check PASSES for
  the good walker on the same tree, which is what keeps this witness from being a property no
  walker could satisfy.\<close>

lemma walk_prefix_ok:
  "\<forall>p \<in> set (walk w_nested). hd p \<in> set (child_names w_nested)"
proof -
  have c: "child_names w_nested = [[100], [101]]" by eval
  show ?thesis using walk_nested_emits c by auto
qed

lemma bare_breaks_prefix:
  "\<exists>p \<in> set (walk_bare w_nested). hd p \<notin> set (child_names w_nested)"
proof -
  have b: "walk_bare w_nested = [[[100]], [[101]], [[102]]]" by eval
  have c: "child_names w_nested = [[100], [101]]" by eval
  show ?thesis using b c by auto
qed

lemma bare_keeps_own_paths: "walk_bare w_emptydir = [[[100]]]"
  by eval

text \<open>(4) STEP-ABOVE-THE-ROOT WALKER breaks conjunct (2), no empty path: it also emits the
  containing directory. The good walker satisfies that conjunct on the same tree.\<close>

lemma up_breaks_no_empty:
  "[] \<in> set (walk_up w_emptydir) \<and> [] \<notin> set (walk w_emptydir)"
proof
  have u: "walk_up w_emptydir = [[], [[100]], [[100]]]" by eval
  show "[] \<in> set (walk_up w_emptydir)" using u by auto
next
  show "[] \<notin> set (walk w_emptydir)" using walk_emptydir_emits by auto
qed

lemma up_breaks_root: "[] \<in> set (walk_up w_root)"
proof -
  have u: "walk_up w_root = [[]]" by eval
  show ?thesis using u by auto
qed

text \<open>The bundle: one theorem that names, per conjunct, the defect walker and the witness
  that falsifies it.\<close>

theorem walk_dir_defects_bite:
  "length (walk_dup w_nested) \<noteq> entries_of w_nested \<and>
   set (walk_drop w_nested) \<noteq> entries_set w_nested \<and>
   (\<exists>p \<in> set (walk_bare w_nested). hd p \<notin> set (child_names w_nested)) \<and>
   [] \<in> set (walk_up w_emptydir)"
  using dup_breaks_count drop_breaks_set bare_breaks_prefix up_breaks_no_empty by auto

text \<open>Why the conjuncts bite, mechanically. Four deliberately defective walkers sit next to
  the good one, and for each a lemma falsifies a NAMED conjunct of the walk_dir_sound theorem
  on the same concrete witness trees the good law is checked on, comparing against the very
  definitions that law uses (entries_of, entries_set, walk, child_names) — so the separation
  is not circular:
  \<^item> @{const walk_dup} emits each entry twice. @{thm dup_breaks_count}: the emission-count
    conjunct fails (6 emitted against @{term "entries_of w_nested"} = 3 on @{const w_nested}).
  \<^item> @{const walk_drop} silently drops directory children. @{thm drop_breaks_set}: the entry
    @{term "[[101], [102]]"} is never visited, so visited-set = entry-set fails;
    @{thm drop_breaks_entry} records it against the good walker's own emission list, and
    @{thm drop_breaks_set_shallow} is the same defect on @{const w_emptydir}.
  \<^item> @{const walk_bare} root-prefixes a child's OWN path but emits its descendants bare.
    @{thm bare_breaks_prefix} exhibits @{term "[[102]]"}, whose head is no child name of the
    root, so declines-into-a-real-child fails; @{thm walk_prefix_ok} shows the good walker
    passes that same check on that same tree.
  \<^item> @{const walk_up} prepends the empty path — the parent of the root.
    @{thm up_breaks_no_empty} shows no-empty-path fails for it while the good walker still
    satisfies it on that tree; @{thm up_breaks_root} is the same defect on the empty
    directory.
  The four are collected by @{thm walk_dir_defects_bite}.\<close>


end
