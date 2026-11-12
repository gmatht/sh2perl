(*  ShIR_TextOps_Splice — the denotational semantics of the `StrSplice`
    node and a machine-checked proof of the C backend's capacity-guarded
    FUSED STORE that consumes it.

      StrSplice(base, start, end, repl)   (sh2perl/src/shir_nodes/str_splice.node)
      folded by `str-splice-fold`          (sh2perl/src/transforms/str_splice_fold.rs)
      consumed by `try_splice_arr_store`   (sh2perl/src/c_backend.rs)

    `StrSplice` is a C-TARGETED node (the §11 marketplace offer: only the
    C renderer has the value arm and the capacity-guarded store arm), and
    until this file its `spec:` line read `none (unstated)`. It is the
    first node whose spec covers a RENDER-side fast path rather than only
    the fold that produced it.

    Why a proof: the fused store writes the splice result STRAIGHT INTO
    the destination element's own buffer — three memmoves in place of two
    substring temps, a join temp and a store copy (MIMEcroft's per-pixel
    row rebuild over a 2560-byte framebuffer row). An in-place write is
    correct only under two conditions the renderer enforces by hand:
      (1) the destination's allocation must already hold the result —
          the `strnlen(_slot, res) == res` guard; and
      (2) the base string and the repl must not ALIAS the slot — the
          `splice_pure` refusal of array reads inside the repl.
    This file states both as theorems and refutes the schedule when (2)
    is dropped (`aliasing_breaks_the_schedule`), so neither condition is
    a comment any more.

    What is proved (all over byte lists; offsets are `int` exactly as the
    emission's `long long` arithmetic, clamped by the same expression):
      * pe_case_pos / pe_case_neg     — the clamp the emission computes
          (`(_s < 0) ? _bl + _s : _s; clamp to [0,_bl]`) as two equations;
      * bash_slice_suffix             — the fold's suffix part denotes
          "from `end` to the string's end" (`drop (sb b k) b`);
      * bash_slice_prefix0            — the fold's prefix part denotes
          "the first `start` bytes" (`take (pe b s) b`);
      * str_splice_fold_sound         — THE FOLD OBLIGATION: the
          Interpolate the fold replaces denotes exactly the node's
          meaning `take (pe b s) b @ repl @ drop (sb b k) b`;
      * memmove_zero                  — a zero-length move is the
          identity (the emission's `if (_rl)` guard);
      * sched_prefix / fused_store_correct — THE STORE OBLIGATION: under
          the emission's own arithmetic and length facts, the schedule's
          first `res` bytes are exactly the node's meaning;
      * guard_iff_fits                — the guard `strnlen(slot,res)==res`
          is exactly "the slot's allocation holds res+1 bytes", which is
          what makes the terminator write `slot[res] = 0` in bounds;
      * aliasing_breaks_the_schedule  — the NEGATIVE result: with the
          repl reading the live buffer (the alias `splice_pure` refuses),
          the schedule's output differs from the node's meaning on a
          concrete instance — the invariant is load-bearing.

    NOT modelled (stated, not papered over): the rest of the backend's
    eligibility routing (array-kind checks, key shapes) — those are
    refuse>guess branches whose every alternative is the unchanged plain
    path, so they can only make the store NOT fire, never fire wrongly;
    and the store's effect on the ARRAY's length bookkeeping, which the
    plain path (`_sh_arr_set`) owns in both arms.

    Build:  isabelle build -D sh2perl/docs/fm/TextOps
*)

theory ShIR_TextOps_Splice
  imports ShIR_TextOps_Base
begin

section \<open>Offsets, clamped exactly as the emission clamps them\<close>

text \<open>The C emission (verbatim arithmetic, `long long` throughout):

  @{text "long long _pe = (_s < 0) ? _bl + _s : _s;
          if (_pe < 0) _pe = 0; if (_pe > _bl) _pe = _bl;"}

Both offsets — the prefix's length (`start`) and the suffix's begin
(`end`) — go through this same expression, so one clamped function models
both.\<close>

definition clamp_off :: "int \<Rightarrow> int \<Rightarrow> int" where
  "clamp_off bl x = (if x < 0 then bl + x else x)"

definition clamped :: "int \<Rightarrow> int \<Rightarrow> nat" where
  "clamped bl x = nat (min (max (clamp_off bl x) 0) bl)"

abbreviation pe :: "bytes \<Rightarrow> int \<Rightarrow> nat" where
  "pe b s \<equiv> clamped (int (length b)) s"

abbreviation sb :: "bytes \<Rightarrow> int \<Rightarrow> nat" where
  "sb b k \<equiv> clamped (int (length b)) k"

subsection \<open>The clamp, as two equations\<close>

lemma pe_case_pos: "0 \<le> (s::int) \<Longrightarrow> pe b s = min (length b) (nat s)"
  unfolding clamped_def clamp_off_def
  by (simp add: max_absorb1 min_def nat_le_iff le_nat_iff)

lemma pe_case_neg_pos: "(s::int) < 0 \<Longrightarrow> 0 \<le> int (length b) + s \<Longrightarrow>
    pe b s = nat (int (length b) + s)"
  unfolding clamped_def clamp_off_def
  by (simp add: max_def min_def min_absorb2)

lemma pe_case_neg_zero: "(s::int) < 0 \<Longrightarrow> int (length b) + s \<le> 0 \<Longrightarrow>
    pe b s = 0"
  unfolding clamped_def clamp_off_def
  by (simp add: max_def min_def)

lemma pe_le_length: "pe b s \<le> length b"
  unfolding clamped_def
  by (simp add: nat_le_iff min.coboundedI2)

lemma int_len_nonneg: "0 \<le> int (length b)"
  by simp

section \<open>The node's meaning\<close>

text \<open>`@{term splice_val}` is `str_splice.node`'s declared meaning: the
base's prefix `[0, start)`, then the repl, then the base's suffix from
`end` — both offsets clamped as above.\<close>

definition splice_val :: "bytes \<Rightarrow> bytes \<Rightarrow> int \<Rightarrow> int \<Rightarrow> bytes" where
  "splice_val b r s k = take (pe b s) b @ r @ drop (sb b k) b"

section \<open>The Interpolate the fold replaces\<close>

text \<open>The fold's two operands are bash slices: the prefix is
`${b:0:$start}` — offset 0, length `start` (a NEGATIVE length is a real
end-count in bash) — and the suffix is `${b:$end}` — from the clamped
`end` to the string's end. `@{term end_of}` is that end index, and the
slice is the byte range `[st, end_of)`.\<close>

definition end_of :: "bytes \<Rightarrow> nat \<Rightarrow> int option \<Rightarrow> nat" where
  "end_of b st len =
     (case len of
        None \<Rightarrow> length b
      | Some n \<Rightarrow> if 0 \<le> n then min (length b) (st + nat n)
                   else max st (nat (int (length b) + n)))"

definition bash_slice :: "bytes \<Rightarrow> int \<Rightarrow> int option \<Rightarrow> bytes" where
  "bash_slice b off len =
     take (end_of b (pe b off) len - pe b off) (drop (pe b off) b)"

lemma bash_slice_suffix: "bash_slice b k None = drop (sb b k) b"
  unfolding bash_slice_def end_of_def
  by (simp add: take_all pe_le_length)

lemma bash_slice_prefix0: "bash_slice b 0 (Some s) = take (pe b s) b"
proof (cases "0 \<le> s")
  case True
  then show ?thesis
    unfolding bash_slice_def end_of_def
    by (simp add: pe_case_pos int_len_nonneg)
next
  case False
  then have neg0: "s < 0" by simp
  show ?thesis
  proof (cases "int (length b) + s \<le> 0")
    case True
    then show ?thesis
      unfolding bash_slice_def end_of_def clamped_def clamp_off_def
      by (simp add: max_def split: if_split)
  next
    case False
    then have s_ge: "0 \<le> int (length b) + s" by linarith
    show ?thesis
      unfolding bash_slice_def end_of_def clamped_def clamp_off_def
      using neg0 s_ge by (simp add: max_def split: if_split)
  qed
qed

subsection \<open>THE FOLD OBLIGATION\<close>

text \<open>`str-splice-fold` replaces
`Interpolate[slice(b,0,start), repl, slice(b,end,"")]` — under its guards
(both slices read the SAME base, the prefix starts at literal 0, the
suffix runs to the end) — with `StrSplice{base=b,start,end,repl}`. The
fold preserves meaning: the replaced interpolation denotes exactly
@{term splice_val}.\<close>

theorem str_splice_fold_sound:
  "bash_slice b 0 (Some s) @ r @ bash_slice b k None = splice_val b r s k"
  unfolding splice_val_def
  by (simp add: bash_slice_prefix0 bash_slice_suffix)

section \<open>The C backend's fused store, as the emission writes it\<close>

text \<open>The schedule, verbatim from `try_splice_arr_store` (with
@{text "_res"} = `pe + rl + (bl - sb)`):

  @{text "memmove(_slot, _row, _pe);
          if (_rl) memmove(_slot + _pe, _rp, _rl);
          memmove(_slot + _pe + _rl, _row + _sb, _bl - _sb);
          _slot[_res] = 0;"}

`@{term "memmove dst src n buf"}` models C `memmove(buf+dst, src, n)` over
a byte buffer. The sources are modelled as SEPARATE lists: that is the
backend's disjointness invariant (the base is a scalar copy, the repl is
`splice_pure`-checked to contain no array read), and
`aliasing_breaks_the_schedule` below shows the invariant is
load-bearing. (The emission's `let`-style temporaries `_t1/_t2/_t3` are
written as the same nested composition here — Isabelle term syntax does
not chain `let … in let …`, and the terms are identical.)\<close>

definition memmove :: "nat \<Rightarrow> bytes \<Rightarrow> nat \<Rightarrow> bytes \<Rightarrow> bytes" where
  "memmove dst src n buf = take dst buf @ take n src @ drop (dst + n) buf"

definition sched :: "bytes \<Rightarrow> bytes \<Rightarrow> bytes \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> bytes" where
  "sched buf row rp pe' rl' sbn' bl =
     memmove (pe' + rl') (drop sbn' row) (bl - sbn')
       (memmove pe' rp rl' (memmove 0 row pe' buf))"

subsection \<open>Moves: length preservation and prefix content\<close>

lemma memmove_zero: "memmove dst src 0 buf = buf"
  unfolding memmove_def
  by (simp add: append_take_drop_id)

lemma memmove_length: "dst + n \<le> length buf \<Longrightarrow> n \<le> length src \<Longrightarrow>
    length (memmove dst src n buf) = length buf"
  unfolding memmove_def
  by (simp add: length_take min_absorb2)
lemma memmove_prefix: "dst + n \<le> length buf \<Longrightarrow> n \<le> length src \<Longrightarrow>
    take (dst + n) (memmove dst src n buf) = take dst buf @ take n src"
proof -
  assume le1: "dst + n \<le> length buf"
  assume le2: "n \<le> length src"
  have len_a: "length (take dst buf @ take n src) = dst + n"
    using le1 le2 by (simp add: min_absorb2)
  have "take (dst + n) (take dst buf @ take n src @ drop (dst + n) buf)
        = take (dst + n) (take dst buf @ take n src)"
    using le1 le2 by (simp add: take_append len_a)
  also have "take (dst + n) (take dst buf @ take n src) = take dst buf @ take n src"
    by (simp add: take_all len_a)
  finally show ?thesis unfolding memmove_def .
qed

subsection \<open>The schedule: first `res` bytes are the splice\<close>

text \<open>The single capacity hypothesis below is the emission's own: the
slot's allocation holds `res` bytes plus the terminator it writes at
index `res`. Every length fact the three moves need follows from it.\<close>

theorem sched_prefix:
  assumes len_row: "length row = bl"
    and sb_ok: "sbn' \<le> bl"
    and pe'_le: "pe' \<le> bl"
    and cap_ok: "pe' + length rp + (bl - sbn') < length buf"
  shows "take (pe' + length rp + (bl - sbn')) (sched buf row rp pe' (length rp) sbn' bl)
         = take pe' row @ rp @ drop sbn' row"
proof -
  let ?rl' = "length rp"
  let ?t1 = "memmove 0 row pe' buf"
  let ?t2 = "memmove pe' rp (length rp) ?t1"
  let ?t3 = "memmove (pe' + length rp) (drop sbn' row) (bl - sbn') ?t2"
  have pe_le_buf: "pe' \<le> length buf"
    using cap_ok by linarith
  have pe_rl_le_buf: "pe' + length rp \<le> length buf"
    using cap_ok by linarith
  have t1_len: "length ?t1 = length buf"
    using pe_le_buf pe'_le len_row by (simp add: memmove_def min_absorb2)
  have t1_pref: "take pe' ?t1 = take pe' row"
    using pe_le_buf pe'_le len_row by (simp add: memmove_def min_absorb2)
  have pe_le_t1: "pe' \<le> length ?t1"
    using pe_le_buf t1_len by simp
  have t2_len: "length ?t2 = length ?t1"
    using pe_le_t1 pe_le_buf pe_rl_le_buf pe'_le len_row by (simp add: memmove_def min_absorb2)
  have t2_pref: "take (pe' + length rp) ?t2 = take pe' ?t1 @ take (length rp) rp"
    using pe_le_t1 pe_le_buf pe_rl_le_buf pe'_le len_row by (simp add: memmove_def min_absorb2)
  have pe_rl_le_t2: "pe' + length rp \<le> length ?t2"
    using pe_rl_le_buf t2_len t1_len by simp
  have src_len: "bl - sbn' \<le> length (drop sbn' row)"
    using sb_ok len_row by simp
  have t3_pref: "take (pe' + length rp + (bl - sbn')) ?t3
                 = take (pe' + length rp) ?t2 @ take (bl - sbn') (drop sbn' row)"
    using pe_rl_le_t2 src_len len_row sb_ok pe'_le pe_le_buf pe_rl_le_buf
    by (simp add: memmove_def min_absorb2)
  have tail: "take (bl - sbn') (drop sbn' row) = drop sbn' row"
    using sb_ok len_row by (simp add: take_all)
  have body: "take (pe' + length rp + (bl - sbn'))
                 (sched buf row rp pe' (length rp) sbn' bl)
              = take (pe' + length rp) ?t2 @ drop sbn' row"
    by (simp add: sched_def t3_pref tail)
  have mid: "take (pe' + length rp) ?t2 = take pe' row @ rp"
    by (simp add: t2_pref t1_pref)
  from body mid show ?thesis by simp
qed

theorem fused_store_correct:
  assumes len_row: "length row = bl"
    and sb_ok: "sbn' \<le> bl"
    and pe'_le: "pe' \<le> bl"
    and cap_ok: "pe' + length rp + (bl - sbn') < length buf"
    and pe_def: "pe' = pe row s"
    and sb_def: "sbn' = sb row k"
  shows "take (pe' + length rp + (bl - sbn'))
              (sched buf row rp pe' (length rp) sbn' bl)
         = splice_val row rp s k"
proof -
  have h: "take (pe' + length rp + (bl - sbn'))
               (sched buf row rp pe' (length rp) sbn' bl)
           = take pe' row @ rp @ drop sbn' row"
    by (rule sched_prefix[OF assms(1-4)])
  show ?thesis unfolding splice_val_def using h pe_def sb_def by simp
qed

subsection \<open>The capacity guard\<close>

text \<open>The guard is `strnlen(_slot, _res) == _res`: the slot's string has
no NUL inside its first `res` bytes. Modelled over the slot's string
@{term s0} (the NUL sits at index `length s0`), the guard says exactly
`res \<le> length s0` — and under the backend's allocation invariant
(the slot's allocation holds the string plus its terminator) that is
equivalent to "the allocation holds `res + 1` bytes", which is what
makes the terminator write `slot[res] = 0` in bounds. A slot whose
string is SHORTER than `res` fails the guard, and the emission then runs
the plain `_sh_arr_set` path — the fallback is what prevents the
out-of-bounds write, so a mis-sized slot is a slow store, never UB.\<close>

definition splice_guard :: "bytes \<Rightarrow> nat \<Rightarrow> bool" where
  "splice_guard s0 res \<longleftrightarrow> res \<le> length s0"

lemma guard_iff_fits:
  "splice_guard s0 res \<longleftrightarrow> res + 1 \<le> length s0 + 1"
  unfolding splice_guard_def by simp

lemma guard_implies_in_bounds:
  assumes "splice_guard s0 res" and "length s0 + 1 \<le> cap"
  shows "res + 1 \<le> cap"
  using assms unfolding splice_guard_def by linarith

lemma short_slot_refused:
  assumes "length s0 < res"
  shows "\<not> splice_guard s0 res \<and> res + 1 > length s0 + 1"
  using assms unfolding splice_guard_def by simp

subsection \<open>The disjointness invariant is load-bearing\<close>

text \<open>The aliased schedule: the repl source is NOT a separate buffer
but a window into the LIVE slot buffer, read at the moment the second
move runs — exactly what a repl holding an array-element pointer would
do, and exactly what `splice_pure`'s refusal of array reads prevents.\<close>

definition sched_alias :: "bytes \<Rightarrow> bytes \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> nat \<Rightarrow> bytes" where
  "sched_alias buf row rp_off bl pe' rl' sbn' =
     memmove (pe' + rl') (drop sbn' row) (bl - sbn')
       (memmove pe' (drop rp_off (memmove 0 row pe' buf)) rl'
         (memmove 0 row pe' buf))"

text \<open>Concrete instance: slot buffer 8 bytes, base @{text "9,10,11"}
(bl = 3), repl aliased at offset 2 of the live buffer (originally bytes
3,4), pe = 3, rl = 2, sb = 1, res = 7. The correct result splices the
ORIGINAL repl; the aliased schedule splices whatever move 1 left at the
alias's window.\<close>

lemma aliasing_breaks_the_schedule:
  "take 7 (sched_alias [1,2,3,4,5,6,7,8] [9,10,11] 2 3 2 1 3)
   \<noteq> take 3 [9,10,11] @ [3,4] @ drop 1 [9,10,11]"
  unfolding sched_alias_def memmove_def
  by simp

section \<open>What the node spec now covers\<close>

text \<open>Between @{thm str_splice_fold_sound} (the fold preserves the
Interpolate's meaning) and @{thm fused_store_correct} (the C fast path's
output is that meaning, in bounds, under the guard) the whole lifecycle
of this node is pinned at the denotational level: what it means, that the
fold is sound, and that the only backend consuming it renders that
meaning. The remaining hand-enforced conditions — the capacity guard and
repl disjointness — are theorems here (`guard_iff_fits`,
`aliasing_breaks_the_schedule`), so a future renderer that weakens either
check contradicts a proved statement rather than a comment.\<close>

end
