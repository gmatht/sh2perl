(*  Title:      ShIR_TextOps_Substitute.thy
    The `substitute` node: a leftmost, non-overlapping rewriter.

    WHAT IS MODELLED. Two engines, one per value the node's `global` flag can
    take: `subst_all` replaces every non-overlapping match, `subst_first` only
    the first. Both walk the text with an explicit FUEL budget, ask a matcher
    for the length of the leftmost match at the current position, and emit the
    replacement of that matched text. A zero-length answer is the node's "no
    match": one byte is copied and the walk continues — POSIX's
    empty-match-advances-one-byte rule, which is why `s/x*/-/g` terminates —
    and every step consumes at least one byte, so `length s` units of fuel
    suffice (`subst_all_extra`, `subst_first_extra`, and the corollaries
    `substitute_fuel_*`). That is the termination obligation, discharged as
    arithmetic rather than assumed.

    The two functions are separate on purpose: a single function carrying a
    `global` boolean puts an `if` inside the recursion, which makes the fuel
    induction split on the flag and then fail to match its own hypothesis.

    WHAT IS NOT MODELLED. The regular expression syntax itself, the line-domain
    bookkeeping (split at '\n', re-join, restore the final newline), and the
    `\N` back-reference. The matcher is an abstract parameter, so the engines
    are proved for EVERY matcher; compiling a BRE/ERE into one is out of scope
    and is recorded as a limitation in docs/fm/README.md.

    The REPLACEMENT language is modelled: `expand` reads sed's replacement
    string, where `&` is the matched text, `\N` a capture from the matcher's
    report, `\&` and `\\` the literals. Because the engines take the
    replacement as an arbitrary function of the matched text, that language is
    an INSTANCE, and `substitute_expand_leftmost` states the leftmost,
    non-overlapping step for it. What stays abstract is the matcher's report —
    which captures a compiled regex produces is a property of the compiler, not
    of the engine.
*)

theory ShIR_TextOps_Substitute
  imports ShIR_TextOps_Base
begin

section \<open>Substitute: a leftmost, non-overlapping rewriter\<close>

text \<open>The match length is clipped to the text, so an over-eager matcher cannot
  make the engine consume bytes that do not exist.\<close>

definition clipped :: "(bytes \<Rightarrow> nat) \<Rightarrow> bytes \<Rightarrow> nat" where
  "clipped m s = min (m s) (length s)"

lemma clipped_le: "clipped m s \<le> length s"
  unfolding clipped_def by simp

lemma clipped_nil [simp]: "clipped m [] = 0"
  unfolding clipped_def by simp

text \<open>Not a simp rule on purpose: it rewrites the very condition the equations
  test, which turns a reduction into a case split.\<close>

lemma clipped_zero_iff:
  "clipped m s = 0 \<longleftrightarrow> m s = 0 \<or> s = []"
  unfolding clipped_def by (auto simp: min_def)

section \<open>The two engines\<close>

fun subst_all ::
  "(bytes \<Rightarrow> nat) \<Rightarrow> (bytes \<Rightarrow> bytes) \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> bytes"
where
  "subst_all m r 0 s = s"
| "subst_all m r (Suc n) [] = []"
| "subst_all m r (Suc n) (c # cs) =
     (if clipped m (c # cs) = 0
      then c # subst_all m r n cs
      else r (take (clipped m (c # cs)) (c # cs)) @
           subst_all m r n (drop (clipped m (c # cs)) (c # cs)))"

fun subst_first ::
  "(bytes \<Rightarrow> nat) \<Rightarrow> (bytes \<Rightarrow> bytes) \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> bytes"
where
  "subst_first m r 0 s = s"
| "subst_first m r (Suc n) [] = []"
| "subst_first m r (Suc n) (c # cs) =
     (if clipped m (c # cs) = 0
      then c # subst_first m r n cs
      else r (take (clipped m (c # cs)) (c # cs)) @
           drop (clipped m (c # cs)) (c # cs))"

text \<open>The node's value: the fuel is the text length, which suffices because
  every step consumes at least one byte.\<close>

definition substitute ::
  "(bytes \<Rightarrow> nat) \<Rightarrow> (bytes \<Rightarrow> bytes) \<Rightarrow> bool \<Rightarrow> bytes \<Rightarrow> bytes"
where
  "substitute m r g s =
     (if g then subst_all m r (length s) s else subst_first m r (length s) s)"

section \<open>The two steps, per engine\<close>

lemma subst_all_nil [simp]: "subst_all m r (Suc n) [] = []"
  by simp

lemma subst_all_advance:
  assumes "clipped m (c # cs) = 0"
  shows "subst_all m r (Suc n) (c # cs) = c # subst_all m r n cs"
  using assms by simp

lemma subst_all_take:
  assumes "clipped m (c # cs) = Suc k"
  shows "subst_all m r (Suc n) (c # cs) =
           r (take (Suc k) (c # cs)) @
           subst_all m r n (drop (Suc k) (c # cs))"
  using assms by simp

lemma subst_first_nil [simp]: "subst_first m r (Suc n) [] = []"
  by simp

lemma subst_first_advance:
  assumes "clipped m (c # cs) = 0"
  shows "subst_first m r (Suc n) (c # cs) = c # subst_first m r n cs"
  using assms by simp

lemma subst_first_take:
  assumes "clipped m (c # cs) = Suc k"
  shows "subst_first m r (Suc n) (c # cs) =
           r (take (Suc k) (c # cs)) @ drop (Suc k) (c # cs)"
  using assms by simp

section \<open>The termination obligation: extra fuel never changes the answer\<close>

text \<open>If it could, `s/x*/-/g` would be defined by a loop. Each step consumes at
  least one byte, so induction on the budget is all that is needed; the
  statement is generalized over the text because the recursive call is on
  `drop (Suc k) s`, which is shorter but is not a subterm.\<close>

lemma subst_all_extra:
  "\<forall>s. length s \<le> n \<longrightarrow> subst_all m r (n + k) s = subst_all m r n s"
proof (induct n)
  case 0
  show ?case
  proof (intro allI impI)
    fix s :: bytes
    assume "length s \<le> 0"
    then have s0: "s = []" by simp
    show "subst_all m r (0 + k) s = subst_all m r 0 s"
      by (cases k) (simp_all add: s0)
  qed
next
  case (Suc n)
  show ?case
  proof (intro allI impI)
    fix s :: bytes
    assume len: "length s \<le> Suc n"
    show "subst_all m r (Suc n + k) s = subst_all m r (Suc n) s"
    proof (cases s)
      case Nil
      then show ?thesis by simp
    next
      case (Cons c cs)
      note sC = this
      have lcs: "length cs \<le> n" using len sC by simp
      have ihcs: "subst_all m r (n + k) cs = subst_all m r n cs"
        using Suc(1)[rule_format, of cs] lcs by simp
      show ?thesis
      proof (cases "clipped m (c # cs) = 0")
        case True
        then show ?thesis using ihcs sC by simp
      next
        case False
        obtain j where j: "clipped m (c # cs) = Suc j"
          using False by (cases "clipped m (c # cs)") auto
        have lj: "j \<le> length cs"
        proof -
          have h2: "clipped m (c # cs) \<le> length (c # cs)" by (rule clipped_le)
          have h3: "Suc j \<le> Suc (length cs)" using j h2 by simp
          then show ?thesis by simp
        qed
        have ldrop: "length (drop (Suc j) (c # cs)) \<le> n"
        proof -
          have "length (drop (Suc j) (c # cs)) = length cs - j"
            by (simp add: length_drop)
          with lj lcs show ?thesis by simp
        qed
        have ihd: "subst_all m r (n + k) (drop (Suc j) (c # cs)) =
                   subst_all m r n (drop (Suc j) (c # cs))"
          using Suc(1)[rule_format, of "drop (Suc j) (c # cs)"] ldrop by simp
        show ?thesis using j ihd sC by simp
      qed
    qed
  qed
qed

corollary substitute_all_fuel_suffices:
  "subst_all m r (length s + k) s = subst_all m r (length s) s"
  using subst_all_extra[rule_format, of s "length s"] by simp

lemma subst_all_any_fuel:
  assumes "length s \<le> n"
  shows "subst_all m r n s = subst_all m r (length s) s"
proof -
  have "subst_all m r (length s + (n - length s)) s = subst_all m r (length s) s"
    using subst_all_extra[rule_format, of s, where n = "length s", where k = "n - length s"]
    by simp
  with assms show ?thesis by simp
qed

lemma subst_first_extra:
  "\<forall>s. length s \<le> n \<longrightarrow> subst_first m r (n + k) s = subst_first m r n s"
proof (induct n)
  case 0
  show ?case
  proof (intro allI impI)
    fix s :: bytes
    assume "length s \<le> 0"
    then have s0: "s = []" by simp
    show "subst_first m r (0 + k) s = subst_first m r 0 s"
      by (cases k) (simp_all add: s0)
  qed
next
  case (Suc n)
  show ?case
  proof (intro allI impI)
    fix s :: bytes
    assume len: "length s \<le> Suc n"
    show "subst_first m r (Suc n + k) s = subst_first m r (Suc n) s"
    proof (cases s)
      case Nil
      then show ?thesis by simp
    next
      case (Cons c cs)
      note sC = this
      have lcs: "length cs \<le> n" using len sC by simp
      have ihcs: "subst_first m r (n + k) cs = subst_first m r n cs"
        using Suc(1)[rule_format, of cs] lcs by simp
      show ?thesis
      proof (cases "clipped m (c # cs) = 0")
        case True
        then show ?thesis using ihcs sC by simp
      next
        case False
        show ?thesis using False sC by simp
      qed
    qed
  qed
qed

corollary substitute_first_fuel_suffices:
  "subst_first m r (length s + k) s = subst_first m r (length s) s"
  using subst_first_extra[rule_format, of s "length s"] by simp

section \<open>Identity on no match\<close>

lemma subst_all_no_match:
  "\<forall>i. i < length s \<longrightarrow> clipped m (drop i s) = 0 \<Longrightarrow>
     subst_all m r (length s) s = s"
proof (induct s)
  case Nil
  then show ?case by simp
next
  case (Cons c cs)
  have h0: "clipped m (c # cs) = 0"
    using Cons(2)[rule_format, of 0] by simp
  have ih: "subst_all m r (length cs) cs = cs"
  proof (rule Cons(1))
    show "\<forall>i. i < length cs \<longrightarrow> clipped m (drop i cs) = 0"
    proof (intro allI impI)
      fix i
      assume "i < length cs"
      have step: "clipped m (drop (Suc i) (c # cs)) = 0"
        using Cons(2)[rule_format, of "Suc i"] \<open>i < length cs\<close> by simp
      then show "clipped m (drop i cs) = 0" by simp
    qed
  qed
  show ?case using h0 ih by (simp add: subst_all_advance)
qed

lemma subst_first_no_match:
  "\<forall>i. i < length s \<longrightarrow> clipped m (drop i s) = 0 \<Longrightarrow>
     subst_first m r (length s) s = s"
proof (induct s)
  case Nil
  then show ?case by simp
next
  case (Cons c cs)
  have h0: "clipped m (c # cs) = 0"
    using Cons(2)[rule_format, of 0] by simp
  have ih: "subst_first m r (length cs) cs = cs"
  proof (rule Cons(1))
    show "\<forall>i. i < length cs \<longrightarrow> clipped m (drop i cs) = 0"
    proof (intro allI impI)
      fix i
      assume "i < length cs"
      have step: "clipped m (drop (Suc i) (c # cs)) = 0"
        using Cons(2)[rule_format, of "Suc i"] \<open>i < length cs\<close> by simp
      then show "clipped m (drop i cs) = 0" by simp
    qed
  qed
  show ?case using h0 ih by (simp add: subst_first_advance)
qed

section \<open>OBLIGATION \<open>substitute_sound\<close>\<close>

text \<open>What the node promises, in three parts. (1) a no-match position copies
  exactly one byte and moves on — the empty-match advance; (2) a match of
  length \<open>Suc k\<close> emits the replacement of the matched text and, in the
  global case, continues at \<open>drop (Suc k) s\<close>, so matches are leftmost,
  non-overlapping and never retried at the same position; (3) with no match
  anywhere the value is the input.\<close>

theorem substitute_sound:
  "(\<forall>c cs. clipped m (c # cs) = 0 \<longrightarrow>
       substitute m r g (c # cs) = c # substitute m r g cs) \<and>
   (\<forall>k c cs. clipped m (c # cs) = Suc k \<longrightarrow>
       substitute m r g (c # cs) =
         r (take (Suc k) (c # cs)) @
         (if g then substitute m r g (drop (Suc k) (c # cs))
          else drop (Suc k) (c # cs))) \<and>
   ((\<forall>i. i < length s \<longrightarrow> clipped m (drop i s) = 0) \<longrightarrow>
       substitute m r g s = s)"
proof (intro conjI allI impI)
  fix c cs
  assume h: "clipped m (c # cs) = 0"
  show "substitute m r g (c # cs) = c # substitute m r g cs"
  proof (cases g)
    case True
    have "subst_all m r (Suc (length cs)) (c # cs) =
          c # subst_all m r (length cs) cs"
      using h by (simp add: subst_all_advance)
    then show ?thesis unfolding substitute_def using True by simp
  next
    case False
    have "subst_first m r (Suc (length cs)) (c # cs) =
          c # subst_first m r (length cs) cs"
      using h by (simp add: subst_first_advance)
    then show ?thesis unfolding substitute_def using False by simp
  qed
next
  fix k c cs
  assume h: "clipped m (c # cs) = Suc k"
  show "substitute m r g (c # cs) =
        r (take (Suc k) (c # cs)) @
        (if g then substitute m r g (drop (Suc k) (c # cs))
         else drop (Suc k) (c # cs))"
  proof (cases g)
    case True
    have step: "subst_all m r (Suc (length cs)) (c # cs) =
          r (take (Suc k) (c # cs)) @
          subst_all m r (length cs) (drop (Suc k) (c # cs))"
      using h by (rule subst_all_take)
    have bridge: "subst_all m r (length cs) (drop (Suc k) (c # cs)) =
          subst_all m r (length (drop (Suc k) (c # cs))) (drop (Suc k) (c # cs))"
    proof (rule subst_all_any_fuel)
      show "length (drop (Suc k) (c # cs)) \<le> length cs" by simp
    qed
    show ?thesis unfolding substitute_def using True step bridge by simp
  next
    case False
    have "subst_first m r (Suc (length cs)) (c # cs) =
          r (take (Suc k) (c # cs)) @ drop (Suc k) (c # cs)"
      using h by (rule subst_first_take)
    then show ?thesis unfolding substitute_def using False by simp
  qed
next
  assume h: "\<forall>i. i < length s \<longrightarrow> clipped m (drop i s) = 0"
  show "substitute m r g s = s"
  proof (cases g)
    case True
    have "subst_all m r (length s) s = s" using h by (rule subst_all_no_match)
    then show ?thesis unfolding substitute_def using True by simp
  next
    case False
    have "subst_first m r (length s) s = s" using h by (rule subst_first_no_match)
    then show ?thesis unfolding substitute_def using False by simp
  qed
qed

text \<open>The empty-match rule in its own right: a zero-length answer is not a
  match that retries, it is a byte copied. This is the one obligation in the
  node that cannot be hand-waved, because a wrong answer here is a loop.\<close>

lemma substitute_empty_match_advances:
  assumes "m (c # cs) = 0"
  shows "subst_first m r (Suc n) (c # cs) = c # subst_first m r n cs"
  using assms by (simp add: clipped_zero_iff subst_first_advance)

text \<open>A matcher that answers zero on every nonempty text makes the node the
  identity — the form a renderer checks its compiled matcher against.\<close>

corollary substitute_no_match_is_identity:
  assumes h: "\<And>t. t \<noteq> [] \<Longrightarrow> m t = 0"
  shows "substitute m r g s = s"
proof -
  have step: "\<forall>i. i < length s \<longrightarrow> clipped m (drop i s) = 0"
  proof (intro allI impI)
    fix i
    assume "i < length s"
    then have "drop i s \<noteq> []" by simp
    then show "clipped m (drop i s) = 0" using h by (simp add: clipped_zero_iff)
  qed
  show ?thesis
  proof (cases g)
    case True
    have "subst_all m r (length s) s = s" using step by (rule subst_all_no_match)
    then show ?thesis unfolding substitute_def using True by simp
  next
    case False
    have "subst_first m r (length s) s = s" using step by (rule subst_first_no_match)
    then show ?thesis unfolding substitute_def using False by simp
  qed
qed

section \<open>\<open>&\<close> is the matched text\<close>

text \<open>sed's replacement language is not modelled: the engine takes an arbitrary
  function of the matched text, which is the most general form. The one
  expansion worth stating is \<open>&\<close>, the matched text itself; a replacement
  that is exactly \<open>&\<close> must therefore be the identity.\<close>

definition amp_repl :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "amp_repl rep mt = concat (map (\<lambda>b. if b = 38 then mt else [b]) rep)"

lemma amp_repl_nil [simp]: "amp_repl [] mt = []"
  unfolding amp_repl_def by simp

lemma amp_repl_amp [simp]: "amp_repl [38] mt = mt"
  unfolding amp_repl_def by simp

lemma amp_repl_plain [simp]: "amp_repl [97] mt = [97]"
  unfolding amp_repl_def by simp

theorem substitute_amp_is_the_identity:
  assumes "clipped m (c # cs) = Suc k"
  shows "substitute m (amp_repl [38]) False (c # cs) = c # cs"
proof -
  have "subst_first m (amp_repl [38]) (length (c # cs)) (c # cs) =
        take (Suc k) (c # cs) @ drop (Suc k) (c # cs)"
    using assms by (simp add: subst_first_take)
  then show ?thesis unfolding substitute_def by simp
qed

text \<open>The interior of a non-global substitution is untouched: the head is the
  replacement, the tail is the input's own tail. A renderer that assembled the
  tail out of re-scanned pieces would violate this.\<close>

corollary substitute_first_tail_is_untouched:
  assumes "clipped m (c # cs) = Suc k"
  shows "subst_first m r (length (c # cs)) (c # cs) =
           r (take (Suc k) (c # cs)) @ drop (Suc k) (c # cs)"
  using assms by (simp add: subst_first_take)

section \<open>The replacement language: \<open>&\<close> and \<open>\N\<close>\<close>

text \<open>sed's replacement string is parsed here rather than assumed: `&` becomes
  the matched text, `\N` (a digit) becomes capture `N` of the matcher's report,
  and `\&`, `\\` are the escaped literals. An out-of-range or unknown escape is
  copied through unchanged, which is what keeps this a TOTAL function: no
  replacement string can make the node undefined. Captures come from the same
  abstract matcher that reports the match length — the engine never invents
  one, so this does not assume a regex engine exists.\<close>

fun expand :: "bytes \<Rightarrow> bytes list \<Rightarrow> bytes \<Rightarrow> bytes" where
  "expand [] caps mt = []"
| "expand (b # r) caps mt =
     (if b = 38 then mt @ expand r caps mt
      else if b = 92 then
        (case r of
           [] \<Rightarrow> [92]
         | d # r' \<Rightarrow>
             (if d = 92 then [92] @ expand r' caps mt
              else if d = 38 then [92, 38] @ expand r' caps mt
              else if 48 \<le> d \<and> d \<le> 57 \<and> d - 48 < length caps
                then caps ! (d - 48) @ expand r' caps mt
                else [92, d] @ expand r' caps mt))
      else b # expand r caps mt)"

lemma expand_amp [simp]: "expand [38] caps mt = mt"
  by simp

lemma expand_escaped_amp [simp]: "expand [92, 38] caps mt = [92, 38]"
  by simp

lemma expand_backslash [simp]: "expand [92, 92] caps mt = [92]"
  by simp

text \<open>`\N` is the Nth capture. Guarded by the report's length, so a
  replacement that names a capture the matcher did not report copies the two
  bytes through instead of indexing out of range.\<close>

lemma expand_backref [simp]:
  "48 \<le> d \<Longrightarrow> d \<le> 57 \<Longrightarrow> d - 48 < length caps \<Longrightarrow>
     expand [92, d] caps mt = caps ! (d - 48)"
  by simp

theorem substitute_amp_replaces_only_amp:
  assumes "clipped m (c # cs) = Suc k"
  shows "subst_first m (amp_repl rep) (length (c # cs)) (c # cs) =
           amp_repl rep (take (Suc k) (c # cs)) @ drop (Suc k) (c # cs)"
  using assms by (simp add: subst_first_take)

text \<open>OBLIGATION, engine side: the node's leftmost, non-overlapping step holds
  for the sed replacement language, for every matcher and both flag values.
  This is `substitute_sound`'s match conjunct instantiated at
  \<open>r := expand rep caps\<close>; the empty-match advance and the fuel bound are the
  same theorems, since neither mentions the replacement.\<close>

theorem substitute_expand_leftmost:
  "clipped m (c # cs) = Suc k \<Longrightarrow>
     substitute m (expand rep caps) g (c # cs) =
       expand rep caps (take (Suc k) (c # cs)) @
       (if g then substitute m (expand rep caps) g (drop (Suc k) (c # cs))
        else drop (Suc k) (c # cs))"
  by (metis conjunct2[OF substitute_sound])

corollary substitute_expand_empty_match_advances:
  assumes "m (c # cs) = 0"
  shows "substitute m (expand rep caps) g (c # cs) = c # substitute m (expand rep caps) g cs"
proof (cases g)
  case True
  have "subst_all m (expand rep caps) (length (c # cs)) (c # cs)
          = c # subst_all m (expand rep caps) (length cs) cs"
    using assms by (simp add: clipped_zero_iff subst_all_advance)
  then show ?thesis unfolding substitute_def using True by simp
next
  case False
  have "subst_first m (expand rep caps) (length (c # cs)) (c # cs)
          = c # subst_first m (expand rep caps) (length cs) cs"
    using assms by (simp add: clipped_zero_iff subst_first_advance)
  then show ?thesis unfolding substitute_def using False by simp
qed

text \<open>LIMITATION, recorded rather than hidden. (a) The node's LINE domain is
  not modelled: the node substitutes each line of the text (split at '\n', the
  empty tail after a final newline dropped, re-joined, final newline restored),
  and nothing here mentions that split. Making it commute with the engine needs
  a matcher that cannot cross a newline — a property of the compiled regex, not
  of the engine — so it is recorded as
  \<open>pending TextOps#substitute_line_domain_sound\<close> in the node rather than
  assumed. (b) The BRE/ERE syntax and its leftmost-longest rule are not modelled;
  the matcher reports a match length and a capture list, and WHICH regexes
  produce which reports is the compiler's obligation. (c) The replacement
  language IS modelled above (`&`, `\N`, `\&`, `\\`, with out-of-range escapes
  copied through). The engines are the part every renderer shares; the two
  absences are named, and neither is papered over.\<close>

end
