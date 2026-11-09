theory ArithDec
  imports ArithWidth
begin

text \<open>
  Section 5 of the arithmetic denotation — the DECIMAL RENDERING of a
  value, added for the C backend's hand-rolled itoas
  (sh2perl/src/c_backend.rs: `_sh_ll2s`, reached from `num_temp`, and
  the inline `size_t` itoa of `FieldStrip`/`FieldStripEnd`).

  `ArithIdentity.thy` gives the shell-int reading of a tree;
  `ArithWidth.thy` adds the machine-width reading of the same tree.
  This file adds the third thing the C renderer needs: the BYTES a
  value is printed as, and the contract those bytes have with the
  buffer they are copied into.

  Why it belongs to the denotation rather than to a comment: both
  helpers ship an unproven claim in the source above them —
  `_sh_ll2s` claims "byte-for-byte what snprintf(\"%lld\") writes for
  every long long (incl. LLONG_MIN)", and the inline loop claims "a
  decimal string in a bounded static (size_t \<le> 20 digits + NUL \<le> 24)".
  C_UB_PROOF.md \<open>§2a\<close>/\<open>§2b\<close> name exactly this class as an obligation:
  a write site is safe only if `strlen \<le> cap` is PROVED. Here the
  claim is stated over the denotation:

    render v         — the canonical spelling of v (sign + digits),
                       the `%lld` specification;
    parse            — the fold that reads such a byte string back;
    parse_render_id  — the round trip: every value the renderer
                       prints is read back as itself;
    render_len_le_20 — the WIDTH half of `ll2s_capacity`: no i64
                       spelling exceeds 20 bytes, so the NUL the
                       generator places after it lands inside the
                       `static char _sN[32]` it also emits;
    dec_bytes_u64_le_20 — the same for the u64 domain of the inline
                       FieldStrip loop (20 digits \<le> 23 slots before
                       the `_i` guard would ever fire).

  CORRESPONDENCE: the Verus model of the emitted STATEMENTS (index
  arithmetic, the memcpy length, the `--_i` guard) is
  sh2perl/proofs/verus/c_dec_str.rs. Its `render_ll`/`dec_digits`/
  `parse_ll` are the same three functions as `render`/`dec_bytes`/
  `parse` here, one level up: over `Seq<char>` with `'0' + d` instead
  of over byte lists with `48 + d`. `ll2s_exact` (Verus) says the
  bytes the copy moves equal `render_ll(v)`; `parse_render_id` (here)
  says those bytes carry the value. Splitting it this way keeps each
  level's proof about its own object — statement-level index
  arithmetic is Verus's, spelling-level list reasoning is Isabelle's.

  DEFINITION NOTE (the loop that is easy to walk into): `dec_bytes`'s
  raw equation is UNCONDITIONAL — `dec_bytes ?u` matches at every
  depth, so declaring `dec_bytes.simps [simp]` makes any `simp` on a
  symbolic argument unfold `u`, `u div 10`, `u div 100`, ... forever
  (measured: a `by (induction u) auto` that should take 0.2 s hangs
  the build past 600 s). The rewrite is therefore registered in its
  CONDITIONAL forms only — `dec_bytes_ge10` fires when `10 \<le> u` is
  provable and `dec_bytes_small` when `u < 10` is, which stops the
  chain at the first level whose guard simp cannot settle. Single
  unfolds use `step_rule` (`by (rule dec_bytes.simps)`, never simp).

  Byte encoding: a string is a list of byte values (same convention
  as docs/fm/TextOps), digits as 48..57, '-' as 45.

  Build:  isabelle build -D sh2perl/proofs/ArithIdentity
\<close>

type_synonym bytes = "nat list"

section \<open>The spelling\<close>

text \<open>The canonical decimal of a natural: MSB first, no leading zero —
  the do-while of the emitted `_sh_ll2s`, written as a total function.
  The divisor guard is also the termination condition:
  `u div 10 < u` for `u \<ge> 10` (`div_less_dividend`).\<close>

function dec_bytes :: "nat \<Rightarrow> bytes" where
  "dec_bytes u = (if u < 10 then [48 + u]
                  else dec_bytes (u div 10) @ [48 + u mod 10])"
  by pat_completeness auto
termination
  by (relation "measure (\<lambda>u. u)") (auto simp: div_less_dividend)

text \<open>The one-step unfold — `rule`, so it fires exactly once and never
  recurses into the recursive call on the right.\<close>

lemma step_rule:
  "dec_bytes u = (if u < 10 then [48 + u]
                  else dec_bytes (u div 10) @ [48 + u mod 10])"
  by (rule dec_bytes.simps)

text \<open>The two conditional equations. The proofs are STRUCTURED on
  purpose: `unfolding step_rule by simp` never terminates, because as a
  rewrite rule `step_rule`'s free `u` is schematic and re-fires at every
  depth (`dec_bytes (u div 10)`, `dec_bytes ((u div 10) div 10)`, ...),
  and simp chases an unfolding tower that never reaches a fixpoint.
  Here `rule` instantiates the equation once at the top and `trans`
  glues it to a one-step `if` reduction, so no recursive subterm is
  ever offered a rewrite.\<close>

lemma dec_bytes_ge10: "10 \<le> u \<Longrightarrow> dec_bytes u = dec_bytes (u div 10) @ [48 + u mod 10]"
proof -
  assume le: "10 \<le> u"
  have a: "dec_bytes u = (if u < 10 then [48 + u]
                         else dec_bytes (u div 10) @ [48 + u mod 10])"
    by (rule dec_bytes.simps)
  have b: "(if u < 10 then [48 + u]
            else dec_bytes (u div 10) @ [48 + u mod 10])
           = dec_bytes (u div 10) @ [48 + u mod 10]"
    using le by simp
  show ?thesis by (rule trans [OF a b])
qed

lemma dec_bytes_small: "u < 10 \<Longrightarrow> dec_bytes u = [48 + u]"
proof -
  assume lt: "u < 10"
  have a: "dec_bytes u = (if u < 10 then [48 + u]
                         else dec_bytes (u div 10) @ [48 + u mod 10])"
    by (rule dec_bytes.simps)
  have b: "(if u < 10 then [48 + u]
            else dec_bytes (u div 10) @ [48 + u mod 10]) = [48 + u]"
    using lt by simp
  show ?thesis by (rule trans [OF a b])
qed

declare dec_bytes_ge10 [simp] dec_bytes_small [simp]

text \<open>Drop the RAW equation from the simpset: the conditional pair above
  is the whole registered interface. Measured on 2026-10-27: with the
  raw rule present, `by (simp add: parse_dec_bytes)` on
  `parse (render v) = v` unfolds `dec_bytes (nat (- v))` at every depth
  (the guard `nat (- v) < 10` is never provable) and spins past 90 s;
  with `simp del` it completes.\<close>

declare dec_bytes.simps [simp del]

text \<open>`render` is the whole `%lld` specification: canonical sign
  (only for negatives, no `+`) followed by the digits.\<close>

definition render :: "int \<Rightarrow> bytes" where
  "render v = (if v < 0 then 45 # dec_bytes (nat (- v))
               else dec_bytes (nat v))"

section \<open>The reader\<close>

text \<open>A decimal string folds left from 0; `48 + d` is the byte of digit
  `d`. Non-digits would read as garbage, which is why
  `dec_bytes_digits` below pins the spelling's byte set — the round
  trip only ever consumes bytes this pair produces.\<close>

definition parse_digits :: "bytes \<Rightarrow> int" where
  "parse_digits s = foldl (\<lambda>a c. a * 10 + int c - 48) 0 s"

definition parse :: "bytes \<Rightarrow> int" where
  "parse s = (if s \<noteq> [] \<and> hd s = 45 then - parse_digits (tl s)
              else parse_digits s)"

section \<open>Every spelling is well-formed\<close>

lemma dec_bytes_length_ge_1 [simp]: "1 \<le> length (dec_bytes u)"
proof (cases "u < 10")
  case True
  then show ?thesis by simp
next
  case False
  then have 10: "10 \<le> u" by simp
  have "dec_bytes u = dec_bytes (u div 10) @ [48 + u mod 10]"
    using 10 by simp
  then show ?thesis by simp
qed

lemma dec_bytes_digits: "set (dec_bytes u) \<subseteq> {48..57}"
proof (induction u rule: less_induct)
  case (less u)
  show ?case
  proof (cases "u < 10")
    case True
    then show ?thesis by simp
  next
    case False
    then have 10: "10 \<le> u" by simp
    have rec: "u div 10 < u" using 10 by (simp add: div_less_dividend)
    have step: "dec_bytes u = dec_bytes (u div 10) @ [48 + u mod 10]"
      using 10 by simp
    show ?thesis
      unfolding step
      using less.IH [OF rec] 10 by auto
  qed
qed

section \<open>The round trip\<close>

text \<open>Reading a spelling back yields the value it was built from. The
  step case is one `foldl_append`: the append in `dec_bytes` is exactly
  a left fold composed.\<close>

lemma parse_dec_bytes: "parse_digits (dec_bytes u) = int u"
proof (induction u rule: less_induct)
  case (less u)
  show ?case
  proof (cases "u < 10")
    case True
    then show ?thesis by (simp add: parse_digits_def)
  next
    case False
    then have 10: "10 \<le> u" by simp
    have rec: "u div 10 < u" using 10 by (simp add: div_less_dividend)
    have "parse_digits (dec_bytes u)
          = (\<lambda>a c. a * 10 + int c - 48)
              (parse_digits (dec_bytes (u div 10))) (48 + u mod 10)"
      using 10 by (simp add: parse_digits_def)
    also have "parse_digits (dec_bytes (u div 10)) = int (u div 10)"
      by (rule less.IH [OF rec])
    also have "(\<lambda>a c. a * 10 + int c - 48) (int (u div 10)) (48 + u mod 10)
               = int (u div 10) * 10 + int (u mod 10)"
      by simp
    also have "int (u div 10) * 10 + int (u mod 10)
               = int (u div 10 * 10 + u mod 10)"
      by (simp only: of_nat_add of_nat_mult)
    also have "int (u div 10 * 10 + u mod 10) = int u"
      by (simp add: div_mult_mod_eq)
    finally show ?thesis .
  qed
qed

text \<open>`parse`'s case split asks whether the spelling STARTS with the
  sign byte 45. It never does: the first byte is a digit byte (48..57),
  and the list is never empty. Two lemmas so `simp` can decide the
  condition instead of unfolding the do-while.\<close>

lemma dec_bytes_ne_Nil: "dec_bytes u \<noteq> []"
proof (cases "dec_bytes u = []")
  case True
  then have "length (dec_bytes u) = 0" by simp
  with dec_bytes_length_ge_1 [of u] show ?thesis by simp
next
  case False
  then show ?thesis by simp
qed

lemma dec_bytes_hd45: "hd (dec_bytes u) \<noteq> 45"
proof -
  have nz: "dec_bytes u \<noteq> []" by (rule dec_bytes_ne_Nil)
  from nz have "hd (dec_bytes u) \<in> set (dec_bytes u)"
    by (rule hd_in_set)
  moreover have "set (dec_bytes u) \<subseteq> {48..57}"
    by (rule dec_bytes_digits)
  ultimately show ?thesis by auto
qed

lemma parse_render_id: "parse (render v) = v"
proof (cases "v < 0")
  case True
  then have lt: "v < 0" by simp
  have r: "render v = 45 # dec_bytes (nat (- v))"
    unfolding render_def by (simp add: lt)
  have p: "parse (45 # dec_bytes (nat (- v))) = - parse_digits (dec_bytes (nat (- v)))"
    unfolding parse_def by simp
  from r p lt show "parse (render v) = v"
    by (simp add: parse_dec_bytes)
next
  case False
  then have ge: "\<not> v < 0" by simp
  have r: "render v = dec_bytes (nat v)"
    unfolding render_def by (simp add: ge)
  have p: "parse (dec_bytes (nat v)) = parse_digits (dec_bytes (nat v))"
    unfolding parse_def
    using dec_bytes_ne_Nil [of "nat v"] dec_bytes_hd45 [of "nat v"] by simp
  from r p ge show "parse (render v) = v"
    by (simp add: parse_dec_bytes)
qed

section \<open>The width theorems\<close>

text \<open>Digit count is monotone — the same shape as the Verus model's
  `ndigits_mono`, and for the same reason: the direct induction on
  `u` with a constant bound is off by one (the hypothesis gives
  `bound + 1`), so the bound travels with `v` and only the base is
  ever concrete.\<close>

lemma dec_bytes_mono:
  assumes "u \<le> v"
  shows "length (dec_bytes u) \<le> length (dec_bytes v)"
  using assms
proof (induction u arbitrary: v rule: less_induct)
  case (less u v)
  show ?case
  proof (cases "u < 10")
    case True
    then have lu: "length (dec_bytes u) = 1"
      by (simp add: dec_bytes_small)
    show ?thesis unfolding lu
      by (rule dec_bytes_length_ge_1)
  next
    case False
    then have 10: "10 \<le> u" by simp
    from less.prems False have v10: "10 \<le> v" by simp
    have rec: "u div 10 < u" using 10 by (simp add: div_less_dividend)
    have step: "dec_bytes u = dec_bytes (u div 10) @ [48 + u mod 10]"
      using 10 by simp
    have step': "dec_bytes v = dec_bytes (v div 10) @ [48 + v mod 10]"
      by (simp add: v10 less.prems)
    have le: "u div 10 \<le> v div 10"
      using less.prems by (simp add: div_le_mono)
    have lu: "length (dec_bytes u) = length (dec_bytes (u div 10)) + 1"
      unfolding step by simp
    have lv: "length (dec_bytes v) = length (dec_bytes (v div 10)) + 1"
      unfolding step' by simp
    have "length (dec_bytes (u div 10)) \<le> length (dec_bytes (v div 10))"
      using less.IH [OF rec le] .
    then show ?thesis unfolding lu lv by simp
  qed
qed

text \<open>The two concrete digit counts, computed rather than inducted —
  2^63 has 19 decimal digits and 2^64-1 has 20. The conditional
  equations decide every guard on a LITERAL, so one `simp` walks the
  19/20 levels in one pass. `by eval` was the original proof here and
  SPINS (measured 2026-10-27: >90 s vs <1 s for `simp`); the code
  path behind `eval` is not worth chasing when `simp` decides it.\<close>

lemma dec_bytes_len_i64_max: "length (dec_bytes 9223372036854775808) = 19"
  by simp

lemma dec_bytes_len_u64_max: "length (dec_bytes 18446744073709551615) = 20"
  by simp

lemma dec_bytes_length_le_19:
  assumes "u \<le> 9223372036854775808"
  shows "length (dec_bytes u) \<le> 19"
proof -
  have "length (dec_bytes u) \<le> length (dec_bytes 9223372036854775808)"
    by (rule dec_bytes_mono [OF assms])
  also have "length (dec_bytes 9223372036854775808) = 19"
    by (rule dec_bytes_len_i64_max)
  finally show ?thesis .
qed

lemma dec_bytes_u64_le_20:
  assumes "u \<le> 18446744073709551615"
  shows "length (dec_bytes u) \<le> 20"
proof -
  have "length (dec_bytes u) \<le> length (dec_bytes 18446744073709551615)"
    by (rule dec_bytes_mono [OF assms])
  also have "length (dec_bytes 18446744073709551615) = 20"
    by (rule dec_bytes_len_u64_max)
  finally show ?thesis .
qed

text \<open>THE WIDTH HALF OF `ll2s_capacity`: an i64 spelling is at most
  20 bytes, so the NUL the generator writes at `d[n]` lands inside the
  `static char _sN[32]` it emits beside the call — proved, not
  assumed. This is the denotation-level statement of what c_dec_str.rs
  proves at statement level (`copy_len + 1 \<le> 32`).\<close>

lemma nat_le_i64_max:
  assumes "0 \<le> x" and "x \<le> (9223372036854775808::int)"
  shows "nat x \<le> (9223372036854775808::nat)"
  using assms by simp

lemma render_len_le_20:
  assumes "fits64 v"
  shows "length (render v) \<le> 20"
proof (cases "v < 0")
  case True
  then have lt: "v < 0" by simp
  have nv0: "0 \<le> - v" using lt by simp
  have nv1: "- v \<le> (9223372036854775808::int)"
    using assms unfolding fits64_def i64_min_def by simp
  from nv0 nv1 have "nat (- v) \<le> (9223372036854775808::nat)"
    by (rule nat_le_i64_max)
  then have le: "length (dec_bytes (nat (- v))) \<le> 19"
    by (rule dec_bytes_length_le_19)
  show ?thesis using lt le by (simp add: render_def)
next
  case False
  then have ge: "\<not> v < 0" by simp
  have pv0: "0 \<le> v" using ge by simp
  have pv1: "v \<le> (9223372036854775807::int)"
    using assms unfolding fits64_def i64_max_def half_u64_def by simp
  then have pv2: "v \<le> (9223372036854775808::int)" by simp
  from pv0 pv2 have "nat v \<le> (9223372036854775808::nat)"
    by (rule nat_le_i64_max)
  then have le: "length (dec_bytes (nat v)) \<le> 19"
    by (rule dec_bytes_length_le_19)
  show ?thesis using ge le by (simp add: render_def)
qed

text \<open>A negative control in the same shape as ArithWidth's
  `machine_wrap_changes_the_reading`: the bound is genuinely about the
  DOMAIN, not about all ints — an unbounded value has an unbounded
  spelling (10^22 has 23 digits), which is why the generator's buffer
  widths are paired with the tier that guarantees the value fits. The
  buffer is 32 and the width theorem is 20; without the domain the
  pair is false.\<close>

lemma dec_bytes_len_10p22: "length (dec_bytes 10000000000000000000000) = 23"
  by simp

lemma render_len_unbounded: "20 < length (render (10000000000000000000000::int))"
  using dec_bytes_len_10p22 by (simp add: render_def)

end