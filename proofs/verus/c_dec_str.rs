// Standalone model of the C backend's DECIMAL RENDERERS — the two
// hand-rolled itoas it EMITS into generated programs, plus the buffer
// widths it emits beside them (sh2perl/src/c_backend.rs):
//
//   HELPER A — `_sh_ll2s(char *d, long long v)` (inserted at ≈10080):
//     char t[24]; int i = 24; int neg = (v < 0);
//     unsigned long long u = neg ? (unsigned long long)(-(v + 1)) + 1
//                                : (unsigned long long)v;
//     do { t[--i] = (char)('0' + (u % 10)); u /= 10; } while (u);
//     if (neg) t[--i] = '-';
//     { size_t n = (size_t)(24 - i); memcpy(d, t + i, n); d[n] = 0; }
//   reached from `num_temp` (≈18031), whose destination is the emitted
//   `static char _sN[32];` — so the generator fixes BOTH the renderer
//   and the buffer it writes into.
//
//   HELPER B — the inline `size_t` itoa in `FieldStrip`/`FieldStripEnd`
//   (≈18372, ≈18393), into `str_temp(24)`:
//     size_t _i = 23; b[23] = 0; size_t _v = _so;
//     if (!_v) { b[--_i] = '0'; }
//     else while (_v && _i) { b[--_i] = (char)('0' + _v % 10); _v /= 10; }
//     memmove(b, b + _i, 24 - _i);
//
// WHY THIS EXISTS. Both helpers ship an unproven claim in the comment
// above them, and the two claims are the same claim:
//
//   A: "byte-for-byte what snprintf(\"%lld\") writes for every long long
//       (incl. LLONG_MIN)"
//   B: "a decimal string in a bounded static (size_t ≤ 20 digits + NUL
//       ≤ 24)"
//
// C_UB_PROOF.md §2a names exactly this class as an obligation of the
// backend's own fast arms ("not trivially in range"), and §2b names the
// second half: a write site is safe only if `strlen ≤ cap` is proved, not
// assumed. A hand-rolled itoa is the tightest such site in the file —
// it is UB-shaped (index underflow, off-by-one NUL) AND silently-wrong-
// shaped (a dropped sign or a truncated digit changes every number the
// program prints, with no sanitizer to catch it). `_sh_ll2s` alone is on
// the path of every numeric print: MIMEcroft has 220 conversion sites.
//
// WHAT IS PROVED (all unconditional over the 64-bit domain):
//   * mag_emitted_is_abs      — the emitted magnitude expression
//                              `-(v+1)+1` equals |v| for every v;
//   * mag_step_fits_i64       — and `-(v+1)` LIES IN i64 for every v in
//                              i64, which is the reason the source is
//                              written that way: plain `-v` does NOT
//                              fit at LLONG_MIN (`neg_plain_overflows`
//                              is that refutation, stated and proved);
//   * fill_is_digits          — the do-while's time-order output is the
//                              digits of |v|, LSB first, so the copied
//                              slice is MSB first (the two recursions
//                              in this file, one per direction, related
//                              by one lemma);
//   * ndigits_le_19           — THE WIDTH THEOREM: no 64-bit magnitude
//                              needs more than 19 digits, so
//                              `final_i ≥ 4` (index never underflows
//                              `t[24]`) and `copy_len ≤ 20`;
//   * ll2s_exact              — the bytes copied equal `render_ll(v)`,
//                              the canonical spelling `%lld` writes;
//                              the round-trip those bytes promise is
//                              proved over the denotation in
//                              ArithDec.thy (`render_parse_id`);
//   * ll2s_capacity           — `copy_len + 1 ≤ 32`, the width
//                              `num_temp` emits: the NUL always lands
//                              inside the buffer;
//   * size_itoa_i_bound       — the inline loop leaves `_i ≥ 3`, so it
//                              never truncates and the `memmove` always
//                              carries the NUL at `b[23]`;
//   * size_itoa_exact         — its result is `dec_digits(v)` for every
//                              u64, including the 20-digit maximum and
//                              the `!_v` zero arm;
//   * max_digits_64           — the shared width fact both helpers rest
//                              on, proved once.
//
// NEGATIVE CONTROL (four mutants, each a plausible one-token source
// edit): (1) drop the sign byte — ll2s_exact red; (2) `i = 23` instead
// of `24` — the copy length shifts, so i_never_underflows and
// copy_len_bound red (the BYTES are right, the count is not — the two
// halves separate); (3) replace `-(v+1)+1` with plain `-v` — run as the
// edit the source really makes (`-(v+1)` -> `-v` AND the paired `+ 1`
// dropped), which keeps every VALUE obligation green and turns exactly
// one red, mag_step_fits_i64 — the overflow the value-level theorems
// cannot see; (4) `_i = 19` instead of `23` in the inline loop —
// size_itoa_i_bound (and size_itoa_zero, whose end state is 18 not 22)
// red on a 20-digit size_t.
//
// WHAT IS NOT MODELLED: `snprintf` itself (its `%lld` spec IS
// `render_ll` — canonical sign+digits — so the theorem is stated as
// exactness against that spelling rather than against libc);
// `_sh_i128_str` and `_sh_isqrt` (separate helpers, separate widths —
// neither is on the `num_temp` path modelled here); the `_sh_vgrow`
// dynamic buffers, whose capacity is data-dependent and therefore a
// different (Tier-2) obligation.
//
// CORRESPONDENCE: `render_ll` is the shIR-level decimal rendering of an
// arithmetic value, defined independently in
// proofs/ArithIdentity/ArithDec.thy (same sign+digits spelling, over
// `bytes = nat list`), where the round-trip and the `length ≤ 20` half of
// `ll2s_capacity` are proved over the denotation instead of over the
// emitted statements — so the two levels state one contract.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_dec_str.rs
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

//// ---- the 64-bit domain -------------------------------------------------

pub open spec fn i64_max() -> int {
    9223372036854775807int
}

pub open spec fn i64_min() -> int {
    0 - 9223372036854775808int
}

pub open spec fn u64_max() -> int {
    18446744073709551615int
}

pub open spec fn fits_i64(x: int) -> bool {
    i64_min() <= x && x <= i64_max()
}

//// ---- the specification: the canonical spelling -------------------------

// Digit value -> the character `%lld` writes for it. Written as a chain
// because Verus has no arithmetic on `char`; the inverse is `digit_val`.
pub open spec fn digit_char(d: int) -> char
    recommends 0 <= d < 10
{
    if d == 0 {
        '0'
    } else if d == 1 {
        '1'
    } else if d == 2 {
        '2'
    } else if d == 3 {
        '3'
    } else if d == 4 {
        '4'
    } else if d == 5 {
        '5'
    } else if d == 6 {
        '6'
    } else if d == 7 {
        '7'
    } else if d == 8 {
        '8'
    } else {
        '9'
    }
}

pub open spec fn digit_val(c: char) -> int {
    if c == '0' {
        0int
    } else if c == '1' {
        1int
    } else if c == '2' {
        2int
    } else if c == '3' {
        3int
    } else if c == '4' {
        4int
    } else if c == '5' {
        5int
    } else if c == '6' {
        6int
    } else if c == '7' {
        7int
    } else if c == '8' {
        8int
    } else if c == '9' {
        9int
    } else {
        0 - 1int
    }
}

// `cons` and `append` as length-indexed builders, so every Seq
// construction here is total and its length is computable by simp.
pub open spec fn cons(c: char, s: Str) -> Str {
    Seq::new(s.len() + 1, |i: int| if i == 0 {
        c
    } else {
        s[i - 1]
    })
}

pub open spec fn append(a: Str, b: Str) -> Str {
    Seq::new(
        a.len() + b.len(),
        |i: int| if i < a.len() {
            a[i]
        } else {
            b[i - a.len()]
        },
    )
}

pub open spec fn rev(s: Str) -> Str {
    Seq::new(s.len(), |i: int| s[s.len() - 1 - i])
}

pub open spec fn mag(v: int) -> int {
    if v < 0 {
        0 - v
    } else {
        v
    }
}

// Append one char at the end, as `Seq::new` so its length stays
// computable — `dec_digits` grows at the tail, `fill_time` at the head.
pub open spec fn snoc(s: Str, c: char) -> Str {
    Seq::new(s.len() + 1, |i: int| if i < s.len() {
        s[i]
    } else {
        c
    })
}

// The canonical decimal of a natural: MSB first, no leading zero, "0"
// for zero. This IS the `%lld` specification.
pub open spec fn dec_digits(u: int) -> Str
    recommends 0 <= u
    decreases u,
{
    if u < 10 {
        seq![digit_char(u)]
    } else {
        snoc(dec_digits(u / 10), digit_char(u % 10))
    }
}

pub open spec fn render_ll(v: int) -> Str {
    if v < 0 {
        cons('-', dec_digits(mag(v)))
    } else {
        dec_digits(v)
    }
}

// A decimal string parses as this value: fold left from the first
// character, `-` prefix negates. Non-digits read -1, but every string
// this model produces is well-formed, and the round-trip only ever
// consumes those.
pub open spec fn parse_digits(s: Str, idx: int, acc: int) -> int
    decreases s.len() - idx,
{
    if idx >= s.len() {
        acc
    } else {
        parse_digits(s, idx + 1, acc * 10 + digit_val(s[idx]))
    }
}

pub open spec fn parse_ll(s: Str) -> int {
    if s.len() > 0 && s[0] == '-' {
        0 - parse_digits(s, 1, 0)
    } else {
        parse_digits(s, 0, 0)
    }
}

//// ---- HELPER A: the emitted `_sh_ll2s` ----------------------------------

// The magnitude expression as the source spells it — statement for
// statement, `-(v + 1)` FIRST and `+ 1` after, because the cast binds
// to `-(v+1)` only:
//   `neg ? (unsigned long long)(-(v + 1)) + 1 : (unsigned long long)v`
// The cast itself is a reinterpretation, so in the model it is the
// identity on the mathematical value it denotes.
//
// Split into the SIGNED SUB-EXPRESSION (`mag_step`, what evaluates
// before the cast and therefore what can be UB) and the full value
// (`mag_emitted`), because those are the two things a one-token edit
// can break independently: plain `-v` keeps the value and destroys the
// fit at LLONG_MIN.
pub open spec fn mag_step(v: int) -> int {
    if v < 0 {
        0 - (v + 1)
    } else {
        v
    }
}

pub open spec fn mag_emitted(v: int) -> int {
    if v < 0 {
        mag_step(v) + 1
    } else {
        mag_step(v)
    }
}

// How many digits the do-while writes for `u` (>= 1 always writes at
// least one: it is a do-while, which is what makes `u == 0` print "0").
pub open spec fn ndigits(u: int) -> int
    recommends 0 <= u
    decreases u,
{
    if u < 10 {
        1int
    } else {
        1 + ndigits(u / 10)
    }
}

// The do-while's output in TIME order: the first byte written is
// `u % 10` (it lands at `t[23]`), the last is the most significant.
pub open spec fn fill_time(u: int) -> Str
    recommends 0 <= u
    decreases u,
{
    if u < 10 {
        seq![digit_char(u)]
    } else {
        cons(digit_char(u % 10), fill_time(u / 10))
    }
}

// The bytes `memcpy(d, t + i, n)` actually moves: time order reversed,
// which is exactly what reading `t[i..24]` forward yields.
pub open spec fn ll2s_chars(v: int) -> Str {
    let body = rev(fill_time(mag_emitted(v)));
    if v < 0 {
        cons('-', body)
    } else {
        body
    }
}

// The emitted index arithmetic: `i` starts at 24 and decrements once
// per digit and once more for the sign; `n = 24 - i`.
pub open spec fn final_i(v: int) -> int {
    24 - ndigits(mag_emitted(v)) - (if v < 0 {
        1int
    } else {
        0int
    })
}

pub open spec fn copy_len(v: int) -> int {
    24 - final_i(v)
}

// The width `num_temp` emits beside the call: `static char _sN[32];`.
// Stated as a constant so the capacity obligation is about the
// GENERATOR's choice, not about an assumption.
pub open spec fn num_temp_cap() -> int {
    32int
}

//// ---- HELPER B: the inline `size_t` itoa of FieldStrip ------------------

// `_i` after the loop, for value `v`. `_i` starts at 23 and one `--_i`
// happens per digit written, so after k writes `_i = 23 - k`, and the
// `while (_v && _i)` guard stops the loop at `_i == 0` — so the exact
// end state is `_i = 23 - min(ndigits(v), 23)`. Closed form, because
// that identity IS the loop invariant after each write.
pub open spec fn size_i_after(v: int) -> int
    recommends 0 <= v,
{
    let d = ndigits(v);
    if d >= 23 {
        // the `&& _i` guard fired before the digits ran out: 23 digits
        // were written (indices 22..0) and the HIGH digits were dropped
        0int
    } else {
        23 - d
    }
}

// What a truncated read yields: the last 23 characters of the spelling
// (what actually sits in b[0..23] once `_i` has hit 0 — b[0] holds the
// 23rd-least-significant digit). `size_i_after` never reaches 0 on the
// u64 domain, which is what the bound below proves.
pub open spec fn low23(s: Str) -> Str {
    if s.len() <= 23 {
        s
    } else {
        Seq::new(23, |i: int| s[s.len() - 23 + i])
    }
}

pub open spec fn size_itoa_chars(v: int) -> Str
    recommends 0 <= v,
{
    if v == 0 {
        // the `if (!_v)` arm: one '0' written at index 22
        seq!['0']
    } else if size_i_after(v) == 0 {
        low23(dec_digits(v))
    } else {
        // digits written LSB-first from b[22] down; the forward read
        // b[_i..23] sees them MSB-first
        rev(fill_time(v))
    }
}

//// ---- the width theorem both helpers rest on ---------------------------

// A 64-bit magnitude has at most 19 decimal digits: 2^63 = 19 digits
// and 2^64-1 = 20, and the signed path never reaches the u64 maximum
// (its magnitude is <= 2^63).
pub open spec fn max_ll_mag() -> int {
    9223372036854775808int
}

// The do-while always writes at least one byte, so every magnitude has
// at least one digit — the fact the LOWER half of the copy bound needs.
proof fn ndigits_ge_1(u: int)
    requires
        0 <= u,
    ensures
        1 <= ndigits(u),
    decreases u,
{
    if u >= 10 {
        ndigits_ge_1(u / 10);
        assert(ndigits(u) == 1 + ndigits(u / 10));
    }
}

// Digit count is monotone: u <= v ==> ndigits(u) <= ndigits(v).
//
// WHY THIS SHAPE: the direct induction — "assume ndigits(u/10) <= 19,
// conclude ndigits(u) <= 19" — is off by one, because the hypothesis
// gives 20. The bound has to move WITH the argument, and the only place
// it is ever concrete is the base. So: monotonicity (the constant stays
// with v) plus one computed digit count per domain maximum.
proof fn ndigits_mono(u: int, v: int)
    requires
        0 <= u,
        u <= v,
    ensures
        ndigits(u) <= ndigits(v),
    decreases u,
{
    if u >= 10 {
        assert(u / 10 <= v / 10);
        ndigits_mono(u / 10, v / 10);
        assert(ndigits(u) == 1 + ndigits(u / 10));
        assert(ndigits(v) == 1 + ndigits(v / 10));
    } else {
        ndigits_ge_1(v);
        assert(ndigits(u) == 1int);
    }
}

// The two concrete base facts, by explicit unfolding: each assert peels
// one digit of the literal. Z3 does not unfold `ndigits` 19 times on its
// own, so the chains are written out — their content is exactly "2^63
// has 19 decimal digits" and "2^64-1 has 20".
proof fn ndigits_max_ll_mag_le_19()
    ensures
        ndigits(max_ll_mag()) <= 19,
{
    assert(max_ll_mag() == 9223372036854775808int);
    assert(ndigits(9223372036854775808int) == 1 + ndigits(922337203685477580int));
    assert(ndigits(922337203685477580int) == 1 + ndigits(92233720368547758int));
    assert(ndigits(92233720368547758int) == 1 + ndigits(9223372036854775int));
    assert(ndigits(9223372036854775int) == 1 + ndigits(922337203685477int));
    assert(ndigits(922337203685477int) == 1 + ndigits(92233720368547int));
    assert(ndigits(92233720368547int) == 1 + ndigits(9223372036854int));
    assert(ndigits(9223372036854int) == 1 + ndigits(922337203685int));
    assert(ndigits(922337203685int) == 1 + ndigits(92233720368int));
    assert(ndigits(92233720368int) == 1 + ndigits(9223372036int));
    assert(ndigits(9223372036int) == 1 + ndigits(922337203int));
    assert(ndigits(922337203int) == 1 + ndigits(92233720int));
    assert(ndigits(92233720int) == 1 + ndigits(9223372int));
    assert(ndigits(9223372int) == 1 + ndigits(922337int));
    assert(ndigits(922337int) == 1 + ndigits(92233int));
    assert(ndigits(92233int) == 1 + ndigits(9223int));
    assert(ndigits(9223int) == 1 + ndigits(922int));
    assert(ndigits(922int) == 1 + ndigits(92int));
    assert(ndigits(92int) == 1 + ndigits(9int));
    assert(ndigits(9int) == 1int);
}

proof fn ndigits_u64_max_le_20()
    ensures
        ndigits(u64_max()) <= 20,
{
    assert(u64_max() == 18446744073709551615int);
    assert(ndigits(18446744073709551615int) == 1 + ndigits(1844674407370955161int));
    assert(ndigits(1844674407370955161int) == 1 + ndigits(184467440737095516int));
    assert(ndigits(184467440737095516int) == 1 + ndigits(18446744073709551int));
    assert(ndigits(18446744073709551int) == 1 + ndigits(1844674407370955int));
    assert(ndigits(1844674407370955int) == 1 + ndigits(184467440737095int));
    assert(ndigits(184467440737095int) == 1 + ndigits(18446744073709int));
    assert(ndigits(18446744073709int) == 1 + ndigits(1844674407370int));
    assert(ndigits(1844674407370int) == 1 + ndigits(184467440737int));
    assert(ndigits(184467440737int) == 1 + ndigits(18446744073int));
    assert(ndigits(18446744073int) == 1 + ndigits(1844674407int));
    assert(ndigits(1844674407int) == 1 + ndigits(184467440int));
    assert(ndigits(184467440int) == 1 + ndigits(18446744int));
    assert(ndigits(18446744int) == 1 + ndigits(1844674int));
    assert(ndigits(1844674int) == 1 + ndigits(184467int));
    assert(ndigits(184467int) == 1 + ndigits(18446int));
    assert(ndigits(18446int) == 1 + ndigits(1844int));
    assert(ndigits(1844int) == 1 + ndigits(184int));
    assert(ndigits(184int) == 1 + ndigits(18int));
    assert(ndigits(18int) == 1 + ndigits(1int));
    assert(ndigits(1int) == 1int);
}

proof fn ndigits_le_19(u: int)
    requires
        0 <= u <= max_ll_mag(),
    ensures
        ndigits(u) <= 19,
{
    ndigits_max_ll_mag_le_19();
    ndigits_mono(u, max_ll_mag());
}

proof fn ndigits_le_20(u: int)
    requires
        0 <= u <= u64_max(),
    ensures
        ndigits(u) <= 20,
{
    ndigits_u64_max_le_20();
    ndigits_mono(u, u64_max());
}

pub proof fn max_digits_64()
    ensures
        ndigits(max_ll_mag()) <= 19,
        ndigits(u64_max()) <= 20,
{
    ndigits_le_19(max_ll_mag());
    ndigits_le_20(u64_max());
}

//// ---- the obligations ---------------------------------------------------

// THE MAGNITUDE: the emitted expression is |v|.
pub proof fn mag_emitted_is_abs(v: int)
    ensures
        mag_emitted(v) == mag(v),
{
}

// And the signed sub-expression `mag_step` fits in i64 for every v in
// i64 — the reason the source is spelled `-(v+1)+1` rather than `-v`.
// Stated as its own obligation because it is a *compile-time* property
// of the emitted expression, not a property of the value that reaches
// it: the value of plain `-v` is right everywhere except that one input
// where evaluating it is UB (`neg_plain_overflows`).
pub proof fn mag_step_fits_i64(v: int)
    requires
        fits_i64(v),
    ensures
        v < 0 ==> fits_i64(mag_step(v)),
        v < 0 ==> mag_emitted(v) == mag(v),
{
}

// NEGATIVE CONTROL, stated as a theorem so it is checkable: plain `-v`
// does NOT fit at LLONG_MIN. This is what the `-(v+1)+1` idiom defends
// against; without it the emitted expression is UB at exactly one input.
pub proof fn neg_plain_overflows()
    ensures
        !fits_i64(0 - i64_min()),
        fits_i64(0 - (i64_min() + 1)),
{
}

// The bridge between the two orders: reversing a cons'd head yields
// the same bytes as snoc'ing it onto the reversed tail. This is the
// only extensionality fact the model needs — everything else is
// induction on `u`.
pub proof fn rev_cons(c: char, s: Str)
    ensures
        rev(cons(c, s)) == snoc(rev(s), c),
{
    let lhs = rev(cons(c, s));
    let rhs = snoc(rev(s), c);
    assert(lhs.len() == rhs.len());
    assert forall|k: int| 0 <= k < lhs.len() implies lhs[k] == rhs[k] by {
        if k == s.len() {
            // head of the cons lands at the far end once reversed
        } else {
            // interior: cons indexes up by one, rev indexes down by one
        }
    }
}

// The two recursions agree: time order reversed is MSB-first order.
proof fn fill_is_digits(u: int)
    requires
        0 <= u,
    ensures
        rev(fill_time(u)) == dec_digits(u),
    decreases u,
{
    if u >= 10 {
        fill_is_digits(u / 10);
        rev_cons(digit_char(u % 10), fill_time(u / 10));
    }
}

// THE WIDTH: no 64-bit magnitude needs more than 19 digits, so the
// emitted index never underflows `t[24]` and the copy never exceeds 20.
pub proof fn ndigits_le_19_public(v: int)
    requires
        fits_i64(v),
    ensures
        ndigits(mag_emitted(v)) <= 19,
{
    ndigits_le_19(mag_emitted(v));
}

pub proof fn i_never_underflows(v: int)
    requires
        fits_i64(v),
    ensures
        0 <= final_i(v),
        4 <= final_i(v),
{
    ndigits_le_19_public(v);
}

pub proof fn copy_len_bound(v: int)
    requires
        fits_i64(v),
    ensures
        1 <= copy_len(v),
        copy_len(v) <= 20,
{
    mag_emitted_is_abs(v);
    assert(0 <= mag_emitted(v) && mag_emitted(v) <= max_ll_mag());
    ndigits_le_19_public(v);
    ndigits_ge_1(mag_emitted(v));
}

// THE SNPRINTF CLAIM: what `_sh_ll2s` copies is the canonical spelling.
pub proof fn ll2s_exact(v: int)
    requires
        fits_i64(v),
    ensures
        ll2s_chars(v) == render_ll(v),
{
    mag_emitted_is_abs(v);
    fill_is_digits(mag_emitted(v));
}

// NOTE (deliberate): the round-trip `parse_ll(ll2s_chars(v)) == v` is NOT
// proved in this file. `ll2s_exact` already gives
// `ll2s_chars(v) == render_ll(v)`, so the round-trip is a property of the
// SPELLING alone — it says nothing about the emitted statements modelled
// here. It is proved where that spelling is defined, over the shIR
// denotation: proofs/ArithIdentity/ArithDec.thy (`render_parse_id`).
// Splitting it keeps each level's proof about its own object, and list
// reasoning of that shape is Isabelle's, not Verus's.

// THE BUFFER CLAIM: the generator emits `char _sN[32]` for this call, so
// the NUL at `d[n]` must land inside it.
pub proof fn ll2s_capacity(v: int)
    requires
        fits_i64(v),
    ensures
        copy_len(v) + 1 <= num_temp_cap(),
        copy_len(v) + 1 <= 24,
{
    copy_len_bound(v);
}

//// ---- HELPER B obligations ---------------------------------------------

// The inline loop always leaves `_i >= 3`, so it never truncates a u64
// and the `memmove(b, b + _i, 24 - _i)` always carries `b[23]` (the NUL).
pub proof fn size_itoa_i_bound(v: int)
    requires
        0 <= v <= u64_max(),
    ensures
        3 <= size_i_after(v),
        size_i_after(v) <= 22,
{
    ndigits_le_20(v);
    ndigits_ge_1(v);
    // 1 <= d <= 20 < 23: the `&& _i` guard never fires, so the closed
    // form is 23 - d, which lies in [3, 22]
    assert(1 <= ndigits(v) && ndigits(v) <= 20 && ndigits(v) < 23);
}

pub proof fn size_itoa_exact(v: int)
    requires
        0 <= v <= u64_max(),
    ensures
        size_itoa_chars(v) == dec_digits(v),
{
    ndigits_le_20(v);
    if v == 0 {
        // the `!_v` arm writes '0' at index 22 — the spelling of zero
        assert(size_itoa_chars(0) == seq!['0']);
        assert(dec_digits(0) == seq!['0']);
    } else {
        // d <= 20 < 23 rules out the truncation arm entirely
        assert(0 < size_i_after(v));
        fill_is_digits(v);
    }
}

// Zero takes the `if (!_v)` arm, which writes '0' at index 22 and never
// enters the loop — the one shape the else-branch does not cover.
pub proof fn size_itoa_zero()
    ensures
        size_itoa_chars(0) == seq!['0'],
        size_i_after(0) == 22,
{
}

// The memmove's length: `24 - _i` covers indices `_i .. 23`, so it ends
// exactly on the NUL the source planted at `b[23]`.
pub proof fn size_itoa_keeps_nul(v: int)
    requires
        0 <= v <= u64_max(),
    ensures
        24 - size_i_after(v) >= 1,
        size_i_after(v) <= 23,
{
    size_itoa_i_bound(v);
}

fn main() {}

} // verus!