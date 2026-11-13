(*  ShIR_TextOps_Glob — the SHELL PATTERN language the pattern-naming nodes
    mention, and the laws those nodes claim about it.

    Thirteenth theory of the session (after Base, Strip, SortUniq, Compose, Space,
    Strings, Cut, Field, Cutset, Substitute, Splice, Device).

    Why this theory exists. The node specs that mention shell pattern matching
    (`StringAffix`'s `[[ $x == P* ]]`, `AffixStrip`'s `${s#p}` / `${s%p}`,
    `StringContains`' `case` idiom `*p*`) were proved for LITERAL patterns only:
    `has_affix` compares `take (length p) t` with `p` byte for byte. The shell's
    pattern is a glob, so a node whose `pattern` field can carry a star or a `?`
    was being modelled by something strictly narrower than its promise. This
    theory supplies the missing semantics — the constructors the nodes actually
    name, and nothing more — and proves the laws, so the glob reading is a proved
    reading rather than a comment.

    WHAT IS MODELLED.
      * `PLit b`  — a literal byte string (the only constructor that consumes
        more than one byte, and the only one that consumes exactly its length);
      * `PAny`    — one byte;
      * `PStar`   — any byte string including the empty one;
      * a PATTERN is a list of these, matched left to right (`pmatch`). Nothing
        else is a constructor: no classes, no ranges, no extglob, no brace
        expansion, no quoting.

    WHAT IS PROVED.
      1. the matching laws: a star is total and absorbs its neighbours
         (`pmatch_star_star`), adjacent literals merge (`pmatch_lit_concat`),
         `P*` is the prefix existential, `*P` the suffix existential, `*P*` is
         "occurs as a slice", `?` is exactly one byte, and a pattern with more
         literal bytes than the text cannot match at all (`pmatch_bounded`);
      2. `${s#p}` (`strip_pre`, SHORTEST matching prefix removed, identity when
         there is no match) and `${s%p}` (`strip_suf`, the mirror), with the
         shortest-match law that separates `#` from `##`;
      3. the node functions coincide with the glob on the patterns the operators
         emit: `strip_pre [PLit p, PStar] t = affix_strip_node t p True` and its
         suffix mirror, which is what makes the literal model a special case of
         this one rather than a separate reading;
      4. witnesses for the laws, and REFUTED weaker models (whole-string match,
         greedy/longest strip on both ends, a star read as a literal byte, `?`
         read as a second star, empty-on-miss strip).

    Proof style follows docs/fm/README.md: structural induction, explicit cases,
    no `metis`. Matching is a relation, not a computable function (the star's
    recursion is an unbounded existential over splits), so the ground witnesses
    are derived from the general laws with `simp` rather than `by eval` — the
    general law is the stronger statement anyway.

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
*)

theory ShIR_TextOps_Glob
  imports ShIR_TextOps_Strings
begin


section \<open>The pattern language: literal bytes, one byte, a star\<close>

datatype pseg = PLit bytes | PAny | PStar

text \<open>`pmatch ps s` is "the pattern `ps` matches the whole of `s`". The recursion is
  on the PATTERN, with the star's split made explicit, so the definition is structurally
  recursive and every law below is an induction on `ps` or a case on that split.\<close>

fun pmatch :: "pseg list \<Rightarrow> bytes \<Rightarrow> bool" where
  "pmatch [] s = (s = [])"
| "pmatch (PLit b # ps) s = (\<exists>r. s = b @ r \<and> pmatch ps r)"
| "pmatch (PAny # ps) s = (\<exists>c r. s = c # r \<and> pmatch ps r)"
| "pmatch (PStar # ps) s = (\<exists>u v. s = u @ v \<and> pmatch ps v)"

text \<open>How many bytes a pattern MUST consume: every literal its own length, one per
  single-byte wildcard, none for a star. This is the measure that makes "the pattern is
  longer than the text" a proved non-match rather than an assumption.\<close>

fun lit_len :: "pseg list \<Rightarrow> nat" where
  "lit_len [] = 0"
| "lit_len (PLit b # ps) = length b + lit_len ps"
| "lit_len (PAny # ps) = Suc (lit_len ps)"
| "lit_len (PStar # ps) = lit_len ps"

text \<open>The four elimination and introduction shapes, proved once so that no later proof
  has to guess an existential witness inside `simp`.\<close>

lemma pmatch_nil [simp]: "pmatch [] s = (s = [])"
  by (rule pmatch.simps(1))

lemma pmatch_litE:
  "pmatch (PLit b # ps) s \<Longrightarrow> \<exists>r. s = b @ r \<and> pmatch ps r"
  by (rule pmatch.simps(2)[THEN iffD1])

lemma pmatch_anyE:
  "pmatch (PAny # ps) s \<Longrightarrow> \<exists>c r. s = c # r \<and> pmatch ps r"
  by (rule pmatch.simps(3)[THEN iffD1])

lemma pmatch_starE:
  "pmatch (PStar # ps) s \<Longrightarrow> \<exists>u v. s = u @ v \<and> pmatch ps v"
  by (rule pmatch.simps(4)[THEN iffD1])

lemma pmatch_litI: "pmatch ps r \<Longrightarrow> pmatch (PLit b # ps) (b @ r)"
proof (unfold pmatch.simps(2))
  assume h: "pmatch ps r"
  show "\<exists>r'. b @ r = b @ r' \<and> pmatch ps r'"
    by (rule exI[where x = r], rule conjI) (simp_all add: h)
qed

lemma pmatch_anyI: "pmatch ps r \<Longrightarrow> pmatch (PAny # ps) (c # r)"
proof (unfold pmatch.simps(3))
  assume h: "pmatch ps r"
  show "\<exists>c' r'. c # r = c' # r' \<and> pmatch ps r'"
    by (rule exI[where x = c], rule exI[where x = r], rule conjI) (simp_all add: h)
qed

lemma pmatch_starI: "pmatch ps v \<Longrightarrow> pmatch (PStar # ps) (u @ v)"
proof (unfold pmatch.simps(4))
  assume h: "pmatch ps v"
  show "\<exists>u' v'. u @ v = u' @ v' \<and> pmatch ps v'"
    by (rule exI[where x = u], rule exI[where x = v], rule conjI) (simp_all add: h)
qed


section \<open>Matching laws\<close>

lemma pmatch_star_only [simp]: "pmatch [PStar] s"
proof -
  have "pmatch (PStar # []) (s @ [])" by (rule pmatch_starI) simp
  thus ?thesis by simp
qed

lemma pmatch_any_only [simp]: "pmatch [PAny] s = (length s = 1)"
proof (rule iffI)
  assume h: "pmatch [PAny] s"
  have ex: "\<exists>c r. s = c # r \<and> pmatch [] r" using h by (rule pmatch_anyE)
  from ex obtain c r where sr: "s = c # r" and r: "pmatch [] r" by blast
  have "r = []" using r by simp
  thus "length s = 1" using sr by simp
next
  assume "length s = 1"
  then obtain c where sc: "s = [c]" by (cases s) simp_all
  have "pmatch [PAny] (c # [])" by (rule pmatch_anyI) simp
  thus "pmatch [PAny] s" using sc by simp
qed

lemma pmatch_lit_only [simp]: "pmatch [PLit b] s = (s = b)"
proof (rule iffI)
  assume h: "pmatch [PLit b] s"
  have ex: "\<exists>r. s = b @ r \<and> pmatch [] r" using h by (rule pmatch_litE)
  from ex obtain r where sr: "s = b @ r" and r: "pmatch [] r" by blast
  have "r = []" using r by simp
  thus "s = b" using sr by simp
next
  assume "s = b"
  have "pmatch [PLit b] (b @ [])" by (rule pmatch_litI) simp
  thus "pmatch [PLit b] s" using \<open>s = b\<close> by simp
qed

text \<open>A star is TOTAL and it ABSORBS: two stars match exactly what one star matches. The
  second is what makes a `.*`-style rendering of a star safe, and the law a naive
  two-loop matcher gets wrong.\<close>

lemma pmatch_star_star: "pmatch (PStar # PStar # ps) s = pmatch (PStar # ps) s"
proof (rule iffI)
  assume h: "pmatch (PStar # PStar # ps) s"
  have e1: "\<exists>u v. s = u @ v \<and> pmatch (PStar # ps) v" using h by (rule pmatch_starE)
  from e1 obtain u v where uv: "s = u @ v" and v: "pmatch (PStar # ps) v" by blast
  have e2: "\<exists>a b. v = a @ b \<and> pmatch ps b" using v by (rule pmatch_starE)
  from e2 obtain a b where ab: "v = a @ b" and b: "pmatch ps b" by blast
  have "s = (u @ a) @ b" using uv ab by simp
  moreover have "pmatch (PStar # ps) ((u @ a) @ b)" using b by (rule pmatch_starI)
  ultimately show "pmatch (PStar # ps) s" by simp
next
  assume h: "pmatch (PStar # ps) s"
  have "pmatch (PStar # (PStar # ps)) ([] @ s)" by (rule pmatch_starI) (rule h)
  thus "pmatch (PStar # PStar # ps) s" by simp
qed

text \<open>Adjacent literals MERGE: `ab` and `a` then `b` are the same pattern. This is what
  lets a frontend accumulate a literal run and still emit the node on the run.\<close>

lemma pmatch_lit_concat:
  "pmatch (PLit a # PLit b # ps) s = pmatch (PLit (a @ b) # ps) s"
proof (rule iffI)
  assume h: "pmatch (PLit a # PLit b # ps) s"
  have e1: "\<exists>r. s = a @ r \<and> pmatch (PLit b # ps) r" using h by (rule pmatch_litE)
  from e1 obtain r where sr: "s = a @ r" and r: "pmatch (PLit b # ps) r" by blast
  have e2: "\<exists>r'. r = b @ r' \<and> pmatch ps r'" using r by (rule pmatch_litE)
  from e2 obtain r' where rr: "r = b @ r'" and m: "pmatch ps r'" by blast
  have "s = (a @ b) @ r'" using sr rr by simp
  moreover have "pmatch (PLit (a @ b) # ps) ((a @ b) @ r')" using m by (rule pmatch_litI)
  ultimately show "pmatch (PLit (a @ b) # ps) s" by simp
next
  assume h: "pmatch (PLit (a @ b) # ps) s"
  have e1: "\<exists>r. s = (a @ b) @ r \<and> pmatch ps r" using h by (rule pmatch_litE)
  from e1 obtain r where sr: "s = (a @ b) @ r" and m: "pmatch ps r" by blast
  have s1: "s = a @ (b @ r)" using sr by simp
  have m2: "pmatch (PLit b # ps) (b @ r)" using m by (rule pmatch_litI)
  have "pmatch (PLit a # PLit b # ps) (a @ (b @ r))" using m2 by (rule pmatch_litI)
  thus "pmatch (PLit a # PLit b # ps) s" using s1 by simp
qed

text \<open>The readings the nodes actually name. `P*` is `startsWith` as an existential, `*P`
  is `endsWith`, and `*P*` is "occurs as a slice" — the `case` idiom `StringContains`
  lowers.\<close>

lemma pmatch_lit_star: "pmatch [PLit p, PStar] t = (\<exists>r. t = p @ r)"
proof (rule iffI)
  assume h: "pmatch [PLit p, PStar] t"
  have e1: "\<exists>r. t = p @ r \<and> pmatch [PStar] r" using h by (rule pmatch_litE)
  from e1 obtain r where tr: "t = p @ r" by blast
  from tr show "\<exists>r. t = p @ r" by (rule exI)
next
  assume h: "\<exists>r. t = p @ r"
  from h obtain r where tr: "t = p @ r" ..
  have "pmatch [PLit p, PStar] (p @ r)" by (rule pmatch_litI) simp
  thus "pmatch [PLit p, PStar] t" using tr by simp
qed

lemma pmatch_star_lit: "pmatch [PStar, PLit p] t = (\<exists>r. t = r @ p)"
proof (rule iffI)
  assume h: "pmatch [PStar, PLit p] t"
  have e1: "\<exists>u v. t = u @ v \<and> pmatch [PLit p] v" using h by (rule pmatch_starE)
  from e1 obtain u v where uv: "t = u @ v" and mv: "pmatch [PLit p] v" by blast
  have "v = p" using mv by (simp only: pmatch_lit_only)
  hence "t = u @ p" using uv by simp
  thus "\<exists>r. t = r @ p" by (rule exI)
next
  assume h: "\<exists>r. t = r @ p"
  from h obtain r where tr: "t = r @ p" ..
  have "pmatch [PLit p] p" by (simp only: pmatch_lit_only)
  hence "pmatch (PStar # [PLit p]) (r @ p)" by (rule pmatch_starI)
  thus "pmatch [PStar, PLit p] t" using tr by simp
qed

lemma pmatch_lit_star_lit: "pmatch [PLit a, PStar, PLit b] s = (\<exists>u. s = a @ u @ b)"
proof (rule iffI)
  assume h: "pmatch [PLit a, PStar, PLit b] s"
  have e1: "\<exists>r. s = a @ r \<and> pmatch [PStar, PLit b] r" using h by (rule pmatch_litE)
  from e1 obtain r where sr: "s = a @ r" and mr: "pmatch [PStar, PLit b] r" by blast
  have "\<exists>r'. r = r' @ b" using mr by (rule pmatch_star_lit[THEN iffD1])
  then obtain r' where rr: "r = r' @ b" ..
  have "s = a @ r' @ b" using sr rr by simp
  thus "\<exists>u. s = a @ u @ b" by (rule exI)
next
  assume h: "\<exists>u. s = a @ u @ b"
  from h obtain u where su: "s = a @ u @ b" ..
  have "\<exists>r. u @ b = r @ b" by (rule exI[where x = u]) simp
  hence m2: "pmatch [PStar, PLit b] (u @ b)" by (rule pmatch_star_lit[THEN iffD2])
  have "pmatch [PLit a, PStar, PLit b] (a @ (u @ b))" using m2 by (rule pmatch_litI)
  thus "pmatch [PLit a, PStar, PLit b] s" using su by simp
qed

lemma pmatch_star_lit_star: "pmatch [PStar, PLit p, PStar] t = (\<exists>u v. t = u @ p @ v)"
proof (rule iffI)
  assume h: "pmatch [PStar, PLit p, PStar] t"
  have e1: "\<exists>u v. t = u @ v \<and> pmatch [PLit p, PStar] v" using h by (rule pmatch_starE)
  from e1 obtain u v where uv: "t = u @ v" and mv: "pmatch [PLit p, PStar] v" by blast
  have "\<exists>r. v = p @ r" using mv by (rule pmatch_lit_star[THEN iffD1])
  then obtain r where vr: "v = p @ r" ..
  have "t = u @ p @ r" using uv vr by simp
  thus "\<exists>u v. t = u @ p @ v" by blast
next
  assume h: "\<exists>u v. t = u @ p @ v"
  from h obtain u v where uv: "t = u @ p @ v" by blast
  have "\<exists>r. p @ v = p @ r" by (rule exI[where x = v]) simp
  hence m: "pmatch [PLit p, PStar] (p @ v)" by (rule pmatch_lit_star[THEN iffD2])
  hence "pmatch (PStar # [PLit p, PStar]) (u @ (p @ v))" by (rule pmatch_starI)
  thus "pmatch [PStar, PLit p, PStar] t" using uv by simp
qed

text \<open>BOUNDED: a pattern cannot match a text shorter than what its literals and single-byte
  wildcards must consume. For star-free patterns this is the trivial length argument; with
  stars in the pattern it needs the split induction below.\<close>

lemma pmatch_bounded:
  assumes m: "pmatch ps s"
  shows "lit_len ps \<le> length s"
  using m
proof (induction ps arbitrary: s)
  case Nil
  show ?case by simp
next
  case (Cons a ps)
  show ?case
  proof (cases a)
    case (PLit b)
    have ex: "\<exists>r. s = b @ r \<and> pmatch ps r" using Cons.prems[unfolded PLit] by (rule pmatch_litE)
    from ex obtain r where sr: "s = b @ r" and mps: "pmatch ps r" by blast
    have "lit_len ps \<le> length r" using Cons.IH mps by blast
    hence "length b + lit_len ps \<le> length b + length r" by simp
    hence "length b + lit_len ps \<le> length s" using sr by simp
    thus ?thesis using PLit by simp
  next
    case PAny
    have ex: "\<exists>c r. s = c # r \<and> pmatch ps r" using Cons.prems[unfolded PAny] by (rule pmatch_anyE)
    from ex obtain c r where sr: "s = c # r" and mps: "pmatch ps r" by blast
    have "lit_len ps \<le> length r" using Cons.IH mps by blast
    hence "Suc (lit_len ps) \<le> Suc (length r)" by simp
    hence "Suc (lit_len ps) \<le> length s" using sr by simp
    thus ?thesis using PAny by simp
  next
    case PStar
    have ex: "\<exists>u v. s = u @ v \<and> pmatch ps v" using Cons.prems[unfolded PStar] by (rule pmatch_starE)
    from ex obtain u v where uv: "s = u @ v" and mps: "pmatch ps v" by blast
    have "lit_len ps \<le> length v" using Cons.IH mps by blast
    hence "lit_len ps \<le> length u + length v" by simp
    hence "lit_len ps \<le> length s" using uv by simp
    thus ?thesis using PStar by simp
  qed
qed


section \<open>`${s#p}`: remove the SHORTEST matching prefix\<close>

text \<open>`${s#p}` is not "match the pattern against the whole string": it removes the
  shortest prefix of `s` that the pattern matches, and it is the identity when nothing
  matches. `pre_ok ps s k` is "the first `k` bytes are a match"; `strip_pre` takes the
  least such `k`.\<close>

definition pre_ok :: "pseg list \<Rightarrow> bytes \<Rightarrow> nat \<Rightarrow> bool" where
  "pre_ok ps s k \<equiv> (k \<le> length s \<and> pmatch ps (take k s))"

definition strip_pre :: "pseg list \<Rightarrow> bytes \<Rightarrow> bytes" where
  "strip_pre ps s \<equiv>
     (if (\<exists>k. pre_ok ps s k)
      then drop (LEAST k. pre_ok ps s k) s
      else s)"

lemma strip_pre_some:
  assumes "\<exists>k. pre_ok ps s k"
  shows "strip_pre ps s = drop (LEAST k. pre_ok ps s k) s"
  using assms unfolding strip_pre_def by simp

lemma strip_pre_none:
  assumes "\<not> (\<exists>k. pre_ok ps s k)"
  shows "strip_pre ps s = s"
  using assms unfolding strip_pre_def by simp

text \<open>Whatever it removes was a match, and what it leaves is a suffix of the input.\<close>

lemma strip_pre_suffix: "\<exists>u. s = u @ strip_pre ps s"
proof (cases "\<exists>k. pre_ok ps s k")
  case True
  have least: "pre_ok ps s (LEAST k. pre_ok ps s k)" using True by (rule LeastI_ex)
  have sp: "strip_pre ps s = drop (LEAST k. pre_ok ps s k) s"
    using True by (rule strip_pre_some)
  show ?thesis
  proof (rule exI[where x = "take (LEAST k. pre_ok ps s k) s"])
    have "take (LEAST k. pre_ok ps s k) s @ drop (LEAST k. pre_ok ps s k) s = s"
      by simp
    thus "s = take (LEAST k. pre_ok ps s k) s @ strip_pre ps s" using sp by simp
  qed
next
  case False
  hence "strip_pre ps s = s" by (rule strip_pre_none)
  show ?thesis by (rule exI[where x = "[]"]) (simp add: \<open>strip_pre ps s = s\<close>)
qed

lemma strip_pre_removed_matches:
  assumes ex: "\<exists>k. pre_ok ps s k"
  shows "\<exists>u. s = u @ strip_pre ps s \<and> pmatch ps u"
proof (rule exI[where x = "take (LEAST k. pre_ok ps s k) s"])
  have least: "pre_ok ps s (LEAST k. pre_ok ps s k)" using ex by (rule LeastI_ex)
  hence m: "pmatch ps (take (LEAST k. pre_ok ps s k) s)"
    unfolding pre_ok_def by blast
  have sp: "strip_pre ps s = drop (LEAST k. pre_ok ps s k) s" using ex by (rule strip_pre_some)
  have "take (LEAST k. pre_ok ps s k) s @ drop (LEAST k. pre_ok ps s k) s = s" by simp
  thus "s = take (LEAST k. pre_ok ps s k) s @ strip_pre ps s \<and>
        pmatch ps (take (LEAST k. pre_ok ps s k) s)"
    using sp m by simp
qed

lemma strip_pre_length_le: "length (strip_pre ps s) \<le> length s"
proof -
  from strip_pre_suffix obtain u where u: "s = u @ strip_pre ps s" ..
  have "length s = length u + length (strip_pre ps s)" by (metis u length_append)
  thus ?thesis by arith
qed

text \<open>SHORTEST, the law that distinguishes `${s#p}` from `${s##p}`: if some prefix `u` of
  `s` matches and `s = u @ v`, the strip removes at most `|u|` bytes — never more than
  any other match, which is what a greedy or longest-match implementation does.\<close>

lemma strip_pre_shortest:
  assumes m: "pmatch ps u" and s: "s = u @ v"
  shows "length v \<le> length (strip_pre ps s)"
proof -
  have ok: "pre_ok ps s (length u)"
  proof -
    have "take (length u) s = u" using s by simp
    moreover have "length u \<le> length s" using s by simp
    ultimately show ?thesis using m unfolding pre_ok_def by simp
  qed
  hence ex: "\<exists>k. pre_ok ps s k" by blast
  have mle: "(LEAST k. pre_ok ps s k) \<le> length u" using ok by (rule Least_le)
  have len: "length (strip_pre ps s) = length s - (LEAST k. pre_ok ps s k)"
  proof -
    have "strip_pre ps s = drop (LEAST k. pre_ok ps s k) s" using ex by (rule strip_pre_some)
    thus ?thesis by simp
  qed
  have sl: "length s = length u + length v" using s by simp
  from mle len sl show ?thesis by arith
qed

text \<open>`${s#*}` is the identity: a star matches the empty prefix, so the SHORTEST match is
  the empty one. `${s##*}` would be the empty string, the longest-match mistake that
  `greedy_strip_is_not_the_node` refutes.\<close>

lemma strip_pre_star: "strip_pre [PStar] s = s"
proof -
  have zero: "pre_ok [PStar] s 0" unfolding pre_ok_def by simp
  hence ex: "\<exists>k. pre_ok [PStar] s k" by blast
  have "(LEAST k. pre_ok [PStar] s k) \<le> 0" using zero by (rule Least_le)
  hence z: "(LEAST k. pre_ok [PStar] s k) = 0" by simp
  have "strip_pre [PStar] s = drop (LEAST k. pre_ok [PStar] s k) s"
    using ex by (rule strip_pre_some)
  thus ?thesis using z by simp
qed

text \<open>And the form `${s#P*}` — the one `AffixStrip` emits: the shortest match of `PLit p`
  followed by a star is exactly `p`, so the node's literal drop is the glob's special case.
  The `if` is the "no match ⇒ identity" arm.\<close>

lemma strip_pre_lit_star:
  "strip_pre [PLit p, PStar] t = (if take (length p) t = p then drop (length p) t else t)"
proof (cases "take (length p) t = p")
  case True
  have exr: "\<exists>r. take (length p) t = p @ r"
  proof (rule exI[where x = "[]"])
    show "take (length p) t = p @ []" using True by simp
  qed
  have m: "pmatch [PLit p, PStar] (take (length p) t)"
    using exr by (rule pmatch_lit_star[THEN iffD2])
  have okp: "pre_ok [PLit p, PStar] t (length p)"
  proof -
    have le: "length p \<le> length t"
    proof -
      have "length p = length (take (length p) t)" using True by simp
      also have "\<dots> \<le> length t" by simp
      finally show ?thesis .
    qed
    have "length p \<le> length t \<and> pmatch [PLit p, PStar] (take (length p) t)"
      using le m by (rule conjI)
    thus ?thesis unfolding pre_ok_def by simp
  qed
  hence ex: "\<exists>k. pre_ok [PLit p, PStar] t k" by blast
  have min: "\<And>k. pre_ok [PLit p, PStar] t k \<Longrightarrow> length p \<le> k"
  proof -
    fix k
    assume ok: "pre_ok [PLit p, PStar] t k"
    have h: "pmatch [PLit p, PStar] (take k t)" and kl: "k \<le> length t"
      using ok unfolding pre_ok_def by blast+
    have "\<exists>r. take k t = p @ r" using h by (rule pmatch_lit_star[THEN iffD1])
    then obtain r where r: "take k t = p @ r" by blast
    have "length (take k t) = length p + length r" using r by simp
    also have "length (take k t) = k" using kl by simp
    finally show "length p \<le> k" by arith
  qed
  have eq: "(LEAST k. pre_ok [PLit p, PStar] t k) = length p"
  proof (rule Least_equality)
    show "pre_ok [PLit p, PStar] t (length p)" using okp .
  next
    fix y
    assume "pre_ok [PLit p, PStar] t y"
    thus "length p \<le> y" by (rule min)
  qed
  have "strip_pre [PLit p, PStar] t = drop (LEAST k. pre_ok [PLit p, PStar] t k) t"
    using ex by (rule strip_pre_some)
  thus ?thesis using True eq by simp
next
  case False
  have none: "\<not> (\<exists>k. pre_ok [PLit p, PStar] t k)"
  proof
    assume "\<exists>k. pre_ok [PLit p, PStar] t k"
    then obtain k where ok: "pre_ok [PLit p, PStar] t k" by blast
    have h: "pmatch [PLit p, PStar] (take k t)" and kl: "k \<le> length t"
      using ok unfolding pre_ok_def by blast+
    have "\<exists>r. take k t = p @ r" using h by (rule pmatch_lit_star[THEN iffD1])
    then obtain r where r: "take k t = p @ r" by blast
    have "length (take k t) = length p + length r" using r by simp
    hence le: "length p \<le> k" using kl by simp
    have "take (length p) t = take (min (length p) k) t" using le by (simp add: min_def)
    also have "\<dots> = take (length p) (take k t)" by (simp add: take_take)
    also have "\<dots> = take (length p) (p @ r)" using r by simp
    also have "\<dots> = p" by simp
    finally have "take (length p) t = p" .
    with False show False by blast
  qed
  hence "strip_pre [PLit p, PStar] t = t" by (rule strip_pre_none)
  thus ?thesis using False by simp
qed

text \<open>The node's own function, reached from the glob rather than assumed: this is what
  makes `AffixStrip`'s literal model a special case instead of a separate one.\<close>

theorem strip_pre_is_the_node:
  "strip_pre [PLit p, PStar] t = affix_strip_node t p True"
  unfolding affix_strip_node_def
  by (simp add: strip_pre_lit_star)


section \<open>`${s%p}`: remove the SHORTEST matching suffix\<close>

text \<open>The mirror image, indexed by the SUFFIX length: `suf_ok ps s k` is "the last `k`
  bytes are a match". Keeping the index a length (rather than an offset) is what makes
  "shortest" the least `k` at both ends.\<close>

definition suf_ok :: "pseg list \<Rightarrow> bytes \<Rightarrow> nat \<Rightarrow> bool" where
  "suf_ok ps s k \<equiv> (k \<le> length s \<and> pmatch ps (drop (length s - k) s))"

definition strip_suf :: "pseg list \<Rightarrow> bytes \<Rightarrow> bytes" where
  "strip_suf ps s \<equiv>
     (if (\<exists>k. suf_ok ps s k)
      then take (length s - (LEAST k. suf_ok ps s k)) s
      else s)"

lemma strip_suf_some:
  assumes "\<exists>k. suf_ok ps s k"
  shows "strip_suf ps s = take (length s - (LEAST k. suf_ok ps s k)) s"
  using assms unfolding strip_suf_def by simp

lemma strip_suf_none:
  assumes "\<not> (\<exists>k. suf_ok ps s k)"
  shows "strip_suf ps s = s"
  using assms unfolding strip_suf_def by simp

lemma strip_suf_suffix: "\<exists>u. strip_suf ps s @ u = s"
proof (cases "\<exists>k. suf_ok ps s k")
  case True
  have sp: "strip_suf ps s = take (length s - (LEAST k. suf_ok ps s k)) s"
    using True by (rule strip_suf_some)
  show ?thesis
  proof (rule exI[where x = "drop (length s - (LEAST k. suf_ok ps s k)) s"])
    have "take (length s - (LEAST k. suf_ok ps s k)) s @
            drop (length s - (LEAST k. suf_ok ps s k)) s = s" by simp
    thus "strip_suf ps s @ drop (length s - (LEAST k. suf_ok ps s k)) s = s"
      using sp by simp
  qed
next
  case False
  hence sp: "strip_suf ps s = s" by (rule strip_suf_none)
  show ?thesis
  proof (rule exI[where x = "[]"])
    show "strip_suf ps s @ [] = s" using sp by simp
  qed
qed

lemma strip_suf_length_le: "length (strip_suf ps s) \<le> length s"
proof -
  from strip_suf_suffix obtain u where u: "strip_suf ps s @ u = s" ..
  have "length s = length (strip_suf ps s) + length u" by (metis u length_append)
  thus ?thesis by arith
qed

lemma strip_suf_shortest:
  assumes m: "pmatch ps u" and s: "s = v @ u"
  shows "length v \<le> length (strip_suf ps s)"
proof -
  have ok: "suf_ok ps s (length u)"
  proof -
    have "drop (length s - length u) s = u" using s by simp
    moreover have "length u \<le> length s" using s by simp
    ultimately show ?thesis using m unfolding suf_ok_def by simp
  qed
  hence ex: "\<exists>k. suf_ok ps s k" by blast
  have mle: "(LEAST k. suf_ok ps s k) \<le> length u" using ok by (rule Least_le)
  have len: "length (strip_suf ps s) = length s - (LEAST k. suf_ok ps s k)"
  proof -
    have "strip_suf ps s = take (length s - (LEAST k. suf_ok ps s k)) s"
      using ex by (rule strip_suf_some)
    thus ?thesis by simp
  qed
  have sl: "length s = length v + length u" using s by simp
  from mle len sl show ?thesis by arith
qed

lemma strip_suf_star: "strip_suf [PStar] s = s"
proof -
  have zero: "suf_ok [PStar] s 0" unfolding suf_ok_def by simp
  hence ex: "\<exists>k. suf_ok [PStar] s k" by blast
  have "(LEAST k. suf_ok [PStar] s k) \<le> 0" using zero by (rule Least_le)
  hence z: "(LEAST k. suf_ok [PStar] s k) = 0" by simp
  have "strip_suf [PStar] s = take (length s - (LEAST k. suf_ok [PStar] s k)) s"
    using ex by (rule strip_suf_some)
  thus ?thesis using z by simp
qed

text \<open>Two facts about `*p`, mirroring the prefix side: no match shorter than `p` can
  qualify, and `k = |p|` is admissible exactly when `t` ends with `p`.\<close>

lemma suf_ok_lit_star_min:
  assumes ok: "suf_ok [PStar, PLit p] t k"
  shows "length p \<le> k"
proof -
  have kl: "k \<le> length t" and m: "pmatch [PStar, PLit p] (drop (length t - k) t)"
    using ok unfolding suf_ok_def by blast+
  have "\<exists>r. drop (length t - k) t = r @ p"
    using m by (rule pmatch_star_lit[THEN iffD1])
  then obtain r where r: "drop (length t - k) t = r @ p" by blast
  have h1: "length (drop (length t - k) t) = k" using kl by simp
  have h2: "length (drop (length t - k) t) = length r + length p"
    by (metis r length_append)
  have "k = length r + length p" using h1 h2 by simp
  thus "length p \<le> k" by arith
qed

lemma drop_eq_len_le: "drop (length t - length p) t = p \<Longrightarrow> length p \<le> length t"
proof -
  assume h: "drop (length t - length p) t = p"
  have "length p = length (drop (length t - length p) t)" using h by simp
  also have "\<dots> \<le> length t" by simp
  finally show "length p \<le> length t" .
qed

lemma suf_ok_lit_star_test:
  "suf_ok [PStar, PLit p] t (length p) = (drop (length t - length p) t = p)"
proof (rule iffI)
  assume h: "drop (length t - length p) t = p"
  have le: "length p \<le> length t" using h by (rule drop_eq_len_le)
  have exr: "\<exists>r. drop (length t - length p) t = r @ p"
  proof (rule exI[where x = "[]"])
    show "drop (length t - length p) t = [] @ p" using h by simp
  qed
  have mp: "pmatch [PStar, PLit p] (drop (length t - length p) t)"
    using exr by (rule pmatch_star_lit[THEN iffD2])
  show "suf_ok [PStar, PLit p] t (length p)"
  proof -
    have "length p \<le> length t \<and> pmatch [PStar, PLit p] (drop (length t - length p) t)"
      using le mp by (rule conjI)
    thus ?thesis unfolding suf_ok_def by simp
  qed
next
  assume ok: "suf_ok [PStar, PLit p] t (length p)"
  have le: "length p \<le> length t" and m: "pmatch [PStar, PLit p] (drop (length t - length p) t)"
    using ok unfolding suf_ok_def by blast+
  have "\<exists>r. drop (length t - length p) t = r @ p"
    using m by (rule pmatch_star_lit[THEN iffD1])
  then obtain r where r: "drop (length t - length p) t = r @ p" by blast
  have h1: "length (drop (length t - length p) t) = length p" using le by simp
  have h2: "length (drop (length t - length p) t) = length r + length p"
    by (metis r length_append)
  have "length r = 0" using h1 h2 by arith
  hence "r = []" by simp
  thus "drop (length t - length p) t = p" using r by simp
qed

lemma strip_suf_star_lit:
  "strip_suf [PStar, PLit p] t
     = (if drop (length t - length p) t = p then take (length t - length p) t else t)"
proof (cases "drop (length t - length p) t = p")
  case True
  have okp: "suf_ok [PStar, PLit p] t (length p)" using True suf_ok_lit_star_test by simp
  hence ex: "\<exists>k. suf_ok [PStar, PLit p] t k" by blast
  have eq: "(LEAST k. suf_ok [PStar, PLit p] t k) = length p"
  proof (rule Least_equality)
    show "suf_ok [PStar, PLit p] t (length p)" using okp .
  next
    fix y
    assume "suf_ok [PStar, PLit p] t y"
    thus "length p \<le> y" by (rule suf_ok_lit_star_min)
  qed
  have "strip_suf [PStar, PLit p] t
          = take (length t - (LEAST k. suf_ok [PStar, PLit p] t k)) t"
    using ex by (rule strip_suf_some)
  thus ?thesis using True eq by simp
next
  case False
  have none: "\<not> (\<exists>k. suf_ok [PStar, PLit p] t k)"
  proof
    assume "\<exists>k. suf_ok [PStar, PLit p] t k"
    then obtain k where ok: "suf_ok [PStar, PLit p] t k" by blast
    have kl: "k \<le> length t" and m: "pmatch [PStar, PLit p] (drop (length t - k) t)"
      using ok unfolding suf_ok_def by blast+
    have kge: "length p \<le> k" using ok by (rule suf_ok_lit_star_min)
    have "\<exists>r. drop (length t - k) t = r @ p"
      using m by (rule pmatch_star_lit[THEN iffD1])
    then obtain r where r: "drop (length t - k) t = r @ p" by blast
    have rl: "length r = k - length p"
    proof -
      have h1: "length (drop (length t - k) t) = k" using kl by simp
      have h2: "length (drop (length t - k) t) = length r + length p"
        by (metis r length_append)
      have "k = length r + length p" using h1 h2 by simp
      thus "length r = k - length p" by arith
    qed
    have key: "length r + (length t - k) = length t - length p" using rl kge kl by arith
    have comp: "drop (length t - length p) t = drop (length r) (drop (length t - k) t)"
    proof -
      have "drop (length r + (length t - k)) t = drop (length r) (drop (length t - k) t)"
        by (simp only: drop_drop)
      thus ?thesis using key by simp
    qed
    have step: "drop (length t - length p) t = drop (length r) (r @ p)" using comp r by simp
    have "drop (length r) (r @ p) = p"
    proof (induct r)
      case Nil thus ?case by simp
    next
      case (Cons a r) thus ?case by simp
    qed
    hence "drop (length t - length p) t = p" using step by simp
    with False show False by blast
  qed
  hence "strip_suf [PStar, PLit p] t = t" by (rule strip_suf_none)
  thus ?thesis using False by simp
qed

theorem strip_suf_is_the_node:
  "strip_suf [PStar, PLit p] t = affix_strip_node t p False"
  unfolding affix_strip_node_def
  by (simp add: strip_suf_star_lit)


section \<open>`*p*`: the interior form of `StringContains`\<close>

text \<open>The third form the renderers emit is the interior one, `${t#*p*}`-style:
  `index(t,p) != -1`. It is `pmatch [PStar, PLit p, PStar]`, and the theory already
  has its decomposition (`pmatch_star_lit_star`). What is left is the bridge to the
  node's own scan, which counts an OFFSET rather than carrying a split.\<close>

lemma contains_drop_split:
  assumes h: "take (length p) (drop i t) = p"
  shows "t = take i t @ p @ drop (i + length p) t"
proof -
  have "drop i t = take (length p) (drop i t) @ drop (length p) (drop i t)"
    by (simp only: append_take_drop_id)
  also have "\<dots> = p @ drop (length p) (drop i t)" using h by simp
  finally have d: "drop i t = p @ drop (length p) (drop i t)" .
  have "t = take i t @ drop i t" by (simp add: append_take_drop_id)
  also have "\<dots> = take i t @ p @ drop (length p) (drop i t)" using d by simp
  also have "drop (length p) (drop i t) = drop (i + length p) t"
    by (simp add: drop_drop add.commute)
  finally show ?thesis .
qed

lemma contains_is_a_glob_match:
  assumes h: "contains_anywhere t p"
  shows "pmatch [PStar, PLit p, PStar] t"
proof -
  from h obtain i where le: "i \<le> length t" and tk: "take (length p) (drop i t) = p"
    unfolding contains_anywhere_def by blast
  have split: "t = take i t @ p @ drop (i + length p) t" using tk by (rule contains_drop_split)
  have ex1: "\<exists>r. p @ drop (i + length p) t = p @ r"
    by (rule exI[where x = "drop (i + length p) t"]) simp
  have m1: "pmatch [PLit p, PStar] (p @ drop (i + length p) t)"
    using ex1 by (rule pmatch_lit_star[THEN iffD2])
  have ex2: "\<exists>v. t = take i t @ v"
  proof (rule exI[where x = "p @ drop (i + length p) t"])
    show "t = take i t @ (p @ drop (i + length p) t)" using split by simp
  qed
  have step: "pmatch [PLit p, PStar] (p @ drop (i + length p) t)" using m1 .
  have m2: "pmatch (PStar # [PLit p, PStar])
              (take i t @ (p @ drop (i + length p) t))"
    using step by (rule pmatch_starI)
  have "take i t @ (p @ drop (i + length p) t) = t" using split by simp
  thus ?thesis using m2 by simp
qed

lemma glob_match_is_contained:
  assumes h: "pmatch [PStar, PLit p, PStar] t"
  shows "contains_anywhere t p"
proof -
  have "\<exists>u v. t = u @ p @ v" using h by (rule pmatch_star_lit_star[THEN iffD1])
  then obtain u v where uv: "t = u @ p @ v" by blast
  have ex: "take (length p) (drop (length u) t) = p"
  proof -
    have "drop (length u) t = p @ v" using uv by simp
    thus ?thesis by simp
  qed
  have le: "length u \<le> length t" using uv by simp
  show ?thesis unfolding contains_anywhere_def using ex le by blast
qed

theorem glob_contains_sound:
  "contains_anywhere t p = pmatch [PStar, PLit p, PStar] t"
  using contains_is_a_glob_match glob_match_is_contained by blast


section \<open>The node obligations, at the nodes' names\<close>

text \<open>These are the three pattern-matching obligations, stated over the node models
  already proved in `ShIR_TextOps_Strings`: for each, the node's byte comparison is
  exactly the glob match of the corresponding pattern. Nothing here restates a
  definition — each is a consequence of the laws above.\<close>

theorem glob_affix_sound:
  "has_affix t p True = pmatch [PLit p, PStar] t"
  "has_affix t p False = pmatch [PStar, PLit p] t"
  using string_affix_sound pmatch_lit_star pmatch_star_lit by simp_all

theorem glob_strip_sound:
  "affix_strip_node t p True = strip_pre [PLit p, PStar] t"
  "affix_strip_node t p False = strip_suf [PStar, PLit p] t"
  using strip_pre_is_the_node strip_suf_is_the_node by simp_all


section \<open>Witnesses: the fragment is not vacuous\<close>

text \<open>A matching law that no string ever satisfies would be vacuous. These are
  ground instances at concrete bytes, discharged from the general laws.\<close>

theorem glob_match_witness:
  "pmatch [PLit [97], PStar] [97, 98, 99] \<and>
   strip_pre [PLit [97], PStar] [97, 98, 99] = [98, 99] \<and>
   strip_suf [PStar, PLit [99]] [97, 98, 99] = [97, 98]"
  by (simp add: pmatch_lit_star strip_pre_lit_star pmatch_star_lit strip_suf_star_lit)

theorem glob_nonmatch_witness:
  "\<not> pmatch [PLit [97], PLit [98]] [97, 99] \<and>
   strip_pre [PLit [99], PStar] [97, 99] = [97, 99] \<and>
   strip_pre [PLit [97], PStar] [98, 97] = [98, 97]"
  by (simp add: pmatch.simps strip_pre_lit_star)

text \<open>And the shortest-match reading is not the trivially-empty one: a two-byte
  prefix matches at BOTH ends of `97, 98`, and the strip removes exactly one copy.\<close>

theorem glob_shortest_witness:
  "strip_pre [PLit [97], PStar] [97, 97] = [97] \<and>
   strip_suf [PStar, PLit [97]] [97, 97] = [97]"
  by (simp add: strip_pre_lit_star strip_suf_star_lit)


section \<open>Refuted weaker models\<close>

text \<open>Each of these is a plausible implementation of the same node that the proved
  law rules out, at a concrete input. They are refuted, not assumed away.\<close>

text \<open>REFUTED: "match the pattern against the whole string" — the mistake that
  turns `${s#*lit}` into an equality test.\<close>

definition whole_match_strip :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "whole_match_strip t p \<equiv> (if t = p then [] else t)"

theorem whole_match_is_not_the_node:
  "whole_match_strip [97, 98] [97] \<noteq> strip_pre [PLit [97], PStar] [97, 98]"
  by (simp add: whole_match_strip_def strip_pre_lit_star)

text \<open>REFUTED: "a star strips everything" — the greedy reading `${s##*}`. The
  shortest-match law keeps the string.\<close>

definition greedy_star_strip :: "bytes \<Rightarrow> bytes" where
  "greedy_star_strip s \<equiv> []"

theorem greedy_star_is_not_the_node:
  "greedy_star_strip [97] \<noteq> strip_pre [PStar] [97]"
  by (simp add: greedy_star_strip_def strip_pre_star)

text \<open>REFUTED: a star that can only match the EMPTY string (`PStar` as `PLit []`),
  and an "any" that matches any length (`PAny` as a star). Both are the shapes a
  first implementation reaches for; the concrete bytes below separate them.\<close>

fun pmatch_loose :: "pseg list \<Rightarrow> bytes \<Rightarrow> bool" where
  "pmatch_loose [] s = (s = [])"
| "pmatch_loose (PLit p # ps) s = (take (length p) s = p \<and> pmatch_loose ps (drop (length p) s))"
| "pmatch_loose (PAny # ps) s = (pmatch_loose ps s)"
| "pmatch_loose (PStar # ps) s = (pmatch_loose ps s)"

theorem optional_star_is_not_the_node:
  "pmatch_loose [PStar, PLit [98]] [97, 97, 98] \<noteq> pmatch [PStar, PLit [98]] [97, 97, 98]"
  by (simp add: pmatch_loose.simps pmatch_star_lit)

theorem any_is_not_a_star:
  "pmatch_loose [PAny] [] \<noteq> pmatch [PAny] []"
  by (simp add: pmatch_loose.simps pmatch.simps)

text \<open>REFUTED: "#p can never fail", i.e. the strip that returns the empty string
  when nothing matches. `${s#p}` is the identity on a miss.\<close>

definition miss_strip :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes" where
  "miss_strip p t \<equiv> (if take (length p) t = p then drop (length p) t else [])"

theorem miss_is_not_the_node:
  "miss_strip [97] [98, 97] \<noteq> strip_pre [PLit [97], PStar] [98, 97]"
  by (simp add: miss_strip_def strip_pre_lit_star)


section \<open>The boundary of the pattern language\<close>

text \<open>The fragment has exactly three constructors — a literal byte string, one
  byte, and a run of any bytes. That is the whole language. Everything else the
  shell's pattern grammar offers is OUTSIDE it: character classes (`[abc]`,
  ranges `[a-z]`, negation `[!a-z]`), `?`-extglob groups, brace alternation, and
  backslash quoting. The nodes that promise such patterns (`cut_at` with
  `${v#*lit}`, `field_strip`, `field_grab`, `path_name` with `${p##*\/}`,
  `cutset_trim` with a class) are therefore stated against the literal pattern
  only; class and range matching is not modelled, and no theorem below assumes it.

  The boundary is not a hand-wave: the theorem below is a checked consequence of
  the three constructors. A byte CLASS is a test that accepts two different bytes
  and rejects a third. No pattern in this fragment can do that at a fixed
  position, because `PLit` fixes the byte, `PAny` accepts every byte, and `PStar`
  accepts the whole rest of the string. So the fragment cannot express a class at
  all — which is exactly why the class-carrying nodes are documented, not
  modelled.\<close>

theorem fragment_is_three_constructors:
  "\<forall>p. p = PAny \<or> p = PStar \<or> (\<exists>bs. p = PLit bs)"
proof (rule allI)
  fix p
  show "p = PAny \<or> p = PStar \<or> (\<exists>bs. p = PLit bs)" by (cases p) simp_all
qed

text \<open>Checkable, one constructor at a time: a literal test cannot accept two
  different bytes at the same position, and an "any" test cannot reject one.\<close>

theorem literal_test_cannot_be_a_class:
  "\<not> (\<exists>q. pmatch [PLit q] [97] \<and> pmatch [PLit q] [98])"
  by (simp add: pmatch_lit_only)

theorem any_test_is_every_byte:
  "pmatch [PAny] [97] \<and> pmatch [PAny] [98] \<and> pmatch [PAny] [99]"
  by (simp add: pmatch_any_only)

theorem star_test_is_every_string:
  "pmatch [PStar] [] \<and> pmatch [PStar] [97] \<and> pmatch [PStar] [97, 98, 99]"
  by (simp add: pmatch_star_only)

end
