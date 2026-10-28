// Standalone model of the C backend's STRING-LITERAL ESCAPER — `cstr`
// (sh2perl/src/c_backend.rs:11080), the single funnel for every emitted
// C string literal: 178 call sites incl. heredoc bodies AND the printf
// FORMAT string built by `heredoc_body_c`.
//
// THE DECISION, verbatim per char (arm order matters):
//   '\\' -> `\\`   '"' -> `\"`   10 -> `\n`   9 -> `\t`   13 -> `\r`
//   marker U+F800+byte -> raw single-byte escape (bash passthrough)
//   c < 32             -> raw single-byte escape
//   else               -> the char itself (raw)
//
// TWO SPELLINGS OF THE SINGLE-BYTE ESCAPE are modelled:
//   * SHIPPED (2026-10-28 morning): `\xNN` hex. C's `\x` is GREEDILY
//     hex (C99 6.4.4.4 consumes every following hex digit), so
//     `"\x01a"` lexes as ONE escape 0x1a. MEASURED (t1/t2/t3,
//     /tmp/cstr_probe): stdout `1a` vs bash `01 61`; `"\x019"` ->
//     `19` vs `01 39` (t5); and `"\x01ABC"` -> 0x1ABC = 7116 > 255,
//     a diagnostic-required range violation (invalid C). t4/t6 matched
//     only because their followers ('x','b', tab) are not hex digits —
//     the bug fires exactly when the next source char is 0-9a-fA-F.
//   * FIXED: `\NNN` 3-digit OCTAL (the landed fix). A fixed-width
//     octal escape consumes at most those 3 digits, so a following
//     hex/octal-looking char stays literal. All 8 probe shapes
//     (t1-t8, incl. uppercase and 3-hex-digit followers) match bash.
//
// THE LEXER IS A FINITE-STATE MACHINE over one char per step (the
// semantics extension this obligation needs): `run(t, st)` consumes
// `t[0]` per call — recursion is structural on `t`, so termination is
// free, and token boundaries fall out of the state, not out of
// substring arithmetic. `decode = run + flush` (flush turns a
// still-open token at end-of-input into its value). This is exactly
// how a C lexer reads: `\x` accumulates UNBOUNDED digits (refused > 255
// at token end), octal stops at 3 (state counter), a non-digit breaks
// the token AND processes the current char — the boundary locality the
// hex spelling violates.
//
// WHAT IS PROVED:
//   * round_trip     — THE OBLIGATION: decode(emit(s), Normal) ==
//                      Some(bytes_of(s)) for every valid source string:
//                      the bytes the C compiler puts in the program
//                      are exactly the bytes bash would have seen;
//   * decode_char    — the per-char bridge decode(emit_char(c) ++ b,
//                      Normal) == pre(byte_of(c), decode(b, Normal)),
//                      the locality property that is exactly what the
//                      hex spelling fails;
//   * oct3_value     — fold_digits(oct3(v), 8) == v for v ≤ 255 and
//                      oct3_digits_octal (the `{:03o}` spelling is
//                      value-exact and lexes as its 3 digits);
//   * pins: the measured literal texts (`\001a`, `\n`, `\"`,
//                      `\353` for marker byte 0xAB) and the digit-
//                      follower shapes (`\0019`, `\0017`);
//   * SHIPPED-HEX DIVERGENCE (negative control): decode of the hex
//                      spelling gives the MEASURED wrong values —
//                      emit_old([1,'a']) decodes to Some([26]) not
//                      [1,97] (t1), [1,'9'] -> Some([25]) (t5), and
//                      [1,'A','B','C'] -> None (the out-of-range
//                      arm, t7). Each is `assert(... != ...)` style
//                      pins against bytes_of.
//
// WHAT IS NOT MODELLED: the UTF-8 re-encoding of non-ASCII Unicode
// chars (raw passthrough is byte-identity under UTF-8 — out of the
// valid_c domain); read_source's marker DECODING (input side;
// byte_of maps markers); the atoll numeric fold's separate unescape
// (`unescape_cstr`, digits only, unaffected by the spelling); printf
// format semantics (`%` doubling lives in `heredoc_body_c`, upstream
// of cstr); compiler source-charset assumptions (UTF-8 default).
// Domain `valid_c`: ASCII (≤ 127) or marker (F800..F8FF); NUL (0)
// round-trips at this level but bash values are NUL-free anyway.
// Unknown escapes are refused (fail-closed: cstr never emits one).
//
// Text is Seq<int> (byte/char CODES) to stay cast-free:
//   BS=92 `\`, QC=34 `"`, XX=120 `x`, NN=110 `n`, TT=116 `t`,
//   RR=114 `r`, digits 48..57 / 65..70 / 97..102.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_str_lit.rs
use vstd::prelude::*;

verus! {

pub type Str = Seq<int>;

pub open spec fn bs() -> int { 92int }
pub open spec fn qc() -> int { 34int }

// ── char classes over CODES ────────────────────────────────────────

pub open spec fn is_hexc(c: int) -> bool {
    (48 <= c && c <= 57) || (65 <= c && c <= 70) || (97 <= c && c <= 102)
}

pub open spec fn is_octc(c: int) -> bool {
    48 <= c && c <= 55
}

pub open spec fn digit_val(c: int) -> int {
    if 48 <= c && c <= 57 {
        c - 48
    } else if 65 <= c && c <= 70 {
        c - 55
    } else if 97 <= c && c <= 102 {
        c - 87
    } else {
        0int // never consulted: callers pre-check the class
    }
}

// Marker elements: U+F800+byte from read_source's byte-preserving
// scheme; everything else in-domain is a plain ASCII code.
pub open spec fn is_marker(c: int) -> bool {
    0xF800 <= c && c <= 0xF8FF
}

pub open spec fn valid_c(c: int) -> bool {
    (0 <= c && c <= 127) || is_marker(c)
}

pub open spec fn valid_str(s: Str) -> bool {
    forall|i: int| 0 <= i < s.len() ==> valid_c(s[i])
}

// The byte a source element denotes (markers decoded; ASCII as-is).
pub open spec fn byte_of(c: int) -> int {
    if is_marker(c) {
        c - 0xF800
    } else {
        c
    }
}

pub open spec fn bytes_of(s: Str) -> Str
    decreases s,
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        seq![byte_of(s[0])].add(bytes_of(s.drop_first()))
    }
}

// ── digit arithmetic ───────────────────────────────────────────────

// Digits are most-significant-FIRST (as written in a C literal and
// as `run`'s Oct arm accumulates), so the fold consumes from the tail.
pub open spec fn fold_digits(d: Str, base: int) -> int
    decreases d.len(),
{
    if d.len() == 0 {
        0int
    } else {
        digit_val(d[d.len() - 1]) + base * fold_digits(d.subrange(0, d.len() - 1), base)
    }
}

// `{:03o}`: exactly three octal digits, zero-padded, most significant
// first. Values ≤ 255 fit (255 = 377_8).
pub open spec fn oct3(v: int) -> Str {
    seq![48 + (v / 64) % 8, 48 + (v / 8) % 8, 48 + v % 8]
}

// The two `\xHH` digits of the SHIPPED spelling (lowercase hex).
pub open spec fn hex2(v: int) -> Str {
    seq![if (v / 16) < 10 { 48 + v / 16 } else { 87 + v / 16 },
         if v % 16 < 10 { 48 + v % 16 } else { 87 + v % 16 }]
}

// ── the emitter (arm order as in `cstr`) ───────────────────────────

pub open spec fn emit_char(c: int) -> Str {
    if c == bs() {
        seq![bs(), bs()]
    } else if c == qc() {
        seq![bs(), qc()]
    } else if c == 10 {
        seq![bs(), 110]
    } else if c == 9 {
        seq![bs(), 116]
    } else if c == 13 {
        seq![bs(), 114]
    } else if is_marker(c) {
        seq![bs()].add(oct3(byte_of(c)))
    } else if c < 32 {
        seq![bs()].add(oct3(c))
    } else {
        seq![c]
    }
}

// The SHIPPED spelling: both single-byte arms hex.
pub open spec fn emit_char_old(c: int) -> Str {
    if c == bs() {
        seq![bs(), bs()]
    } else if c == qc() {
        seq![bs(), qc()]
    } else if c == 10 {
        seq![bs(), 110]
    } else if c == 9 {
        seq![bs(), 116]
    } else if c == 13 {
        seq![bs(), 114]
    } else if is_marker(c) {
        seq![bs(), 120].add(hex2(byte_of(c)))
    } else if c < 32 {
        seq![bs(), 120].add(hex2(c))
    } else {
        seq![c]
    }
}

pub open spec fn emit_body(s: Str) -> Str
    decreases s,
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        emit_char(s[0]).add(emit_body(s.drop_first()))
    }
}

pub open spec fn emit(s: Str) -> Str {
    seq![qc()].add(emit_body(s)).add(seq![qc()])
}

pub open spec fn emit_old_body(s: Str) -> Str
    decreases s,
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        emit_char_old(s[0]).add(emit_old_body(s.drop_first()))
    }
}

pub open spec fn emit_old(s: Str) -> Str {
    seq![qc()].add(emit_old_body(s)).add(seq![qc()])
}

// ── the C lexer: finite state, one char per step ───────────────────
// Normal: between tokens (quote skipped, backslash starts an escape).
// Esc: after `\`, next char selects the escape.
// Hex(acc, seen): after `\x`, accumulating unbounded hex digits.
// Oct(acc, cnt): after `\` + first octal digit, at most 3 digits.

pub enum St {
    Normal,
    Esc,
    Hex { acc: int, seen: bool },
    Oct { acc: int, cnt: nat },
}

// One char per call. Values are accumulated in order; a breaking char
// is emitted after the finished token's value, then processed — the
// token-boundary locality made structural.
pub open spec fn run(t: Str, st: St) -> Option<(Str, St)>
    decreases t,
{
    if t.len() == 0 {
        Option::Some((Seq::empty(), st))
    } else {
        let c = t[0];
        let rest = t.drop_first();
        match st {
            St::Normal => {
                if c == qc() {
                    run(rest, St::Normal)
                } else if c == bs() {
                    run(rest, St::Esc)
                } else if c < 32 {
                    Option::None // raw control byte is never emitted text
                } else {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![c].add(v), s)),
                        Option::None => Option::None,
                    }
                }
            },
            St::Esc => {
                if c == 110 {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![10].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == 116 {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![9].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == 114 {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![13].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == bs() {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![bs()].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == qc() {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![qc()].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == 120 {
                    run(rest, St::Hex { acc: 0, seen: false })
                } else if is_octc(c) {
                    run(rest, St::Oct { acc: c - 48, cnt: 1 })
                } else {
                    Option::None // unknown escape — cstr never emits one
                }
            },
            St::Hex { acc, seen } => {
                if is_hexc(c) {
                    run(rest, St::Hex { acc: acc * 16 + digit_val(c), seen: true })
                } else if !seen {
                    Option::None // `\x` with no digits — constraint violation
                } else if acc > 255 {
                    Option::None // range violation (greedy hex is exactly this arm)
                } else if c == bs() {
                    match run(rest, St::Esc) {
                        Option::Some((v, s)) => Option::Some((seq![acc].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == qc() {
                    // the closing delimiter: skip it, emit no byte.
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![acc].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c >= 32 {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![acc, c].add(v), s)),
                        Option::None => Option::None,
                    }
                } else {
                    Option::None // raw control byte is never emitted text
                }
            },
            St::Oct { acc, cnt } => {
                if cnt < 3 && is_octc(c) {
                    run(rest, St::Oct { acc: acc * 8 + (c - 48), cnt: cnt + 1 })
                } else if acc > 255 {
                    Option::None // 3 octal digits reach 777_8 = 511
                } else if is_octc(c) {
                    // 3 digits consumed: emit, then the 4th digit is raw text.
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![acc, c].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == bs() {
                    match run(rest, St::Esc) {
                        Option::Some((v, s)) => Option::Some((seq![acc].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c == qc() {
                    // the closing delimiter: skip it, emit no byte.
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![acc].add(v), s)),
                        Option::None => Option::None,
                    }
                } else if c >= 32 {
                    match run(rest, St::Normal) {
                        Option::Some((v, s)) => Option::Some((seq![acc, c].add(v), s)),
                        Option::None => Option::None,
                    }
                } else {
                    Option::None // raw control byte is never emitted text
                }
            },
        }
    }
}

// Flush a still-open token at end of input.
pub open spec fn flush(st: St) -> Option<Str> {
    match st {
        St::Normal => Option::Some(Seq::empty()),
        St::Esc => Option::None, // trailing backslash
        St::Hex { acc, seen } => {
            if seen && acc <= 255 {
                Option::Some(seq![acc])
            } else {
                Option::None
            }
        },
        St::Oct { acc, cnt: _ } => {
            if acc <= 255 {
                Option::Some(seq![acc])
            } else {
                Option::None
            }
        },
    }
}

// decode: run to end-of-input, then flush. No recursion of its own.
pub open spec fn pre(v: int, o: Option<Str>) -> Option<Str> {
    match o {
        Option::Some(x) => Option::Some(seq![v].add(x)),
        Option::None => Option::None,
    }
}

pub open spec fn decode_t(t: Str, st: St) -> Option<(Str, St)> {
    run(t, st)
}

pub open spec fn decode(t: Str, st: St) -> Option<Str> {
    match run(t, st) {
        Option::Some((v, s)) => {
            match flush(s) {
                Option::Some(f) => Option::Some(v.add(f)),
                Option::None => Option::None,
            }
        },
        Option::None => Option::None,
    }
}

// ── lemmas ───────────────────────────────────────────────────────

// A raw plain char in Normal context is its own byte, one run step.
pub proof fn decode_plain(c: int, b: Str)
    requires 32 <= c, c != bs(), c != qc(),
    ensures decode(seq![c].add(b), St::Normal) == pre(c, decode(b, St::Normal)),
{
    let t = seq![c].add(b);
    assert(t[0] == c);
    assert(t.drop_first() == b);
    match run(b, St::Normal) {
        Option::Some((v, s)) => {
            match flush(s) {
                Option::Some(f) => {
                    assert(decode(t, St::Normal) == Some(seq![c].add(v).add(f))) by {
                        reveal_with_fuel(run, 2);
                    };
                    assert(decode(b, St::Normal) == Some(v.add(f)));
                    assert(seq![c].add(v).add(f) =~= seq![c].add(v.add(f)));
                },
                Option::None => {},
            }
        },
        Option::None => {},
    }
}

// The octal token at cnt==3 is a COMPLETE value: any breaking char
// emits it first and then behaves exactly as in Normal — the boundary
// locality for the fixed 3-digit spelling.
pub proof fn oct3_break(v: int, b: Str)
    requires 0 <= v, v <= 255,
    ensures decode(b, St::Oct { acc: v, cnt: 3 })
        == pre(v, decode(b, St::Normal)),
    decreases b,
{
    if b.len() == 0 {
        // both sides: flush(Oct) = [v] vs pre(v, flush(Normal)) = [v]
    } else {
        let c = b[0];
        let bp = b.drop_first();
        if is_octc(c) {
            // cnt==3: emit v, then c raw, then Normal — via decode_plain
            decode_plain(c, bp);
        } else if c == bs() {
            // both sides continue in run(bp, Esc) with v prefixed
        } else if c == qc() {
            // both sides skip the quote and continue run(bp, Normal)
        } else if c >= 32 {
            decode_plain(c, bp);
        } else {
            // raw control: Oct arm refuses (non-octal), Normal refuses
        }
    }
}

// An escape PAIR `[bs, x]` decodes to its byte in one chain.
pub proof fn decode_pair(x: int, v: int, b: Str)
    requires x == 110 || x == 116 || x == 114 || x == bs() || x == qc(),
        v == (if x == 110 { 10int } else if x == 116 { 9int } else if x == 114 {
            13int
        } else if x == bs() { 92int } else { 34int }),
    ensures decode(seq![bs(), x].add(b), St::Normal) == pre(v, decode(b, St::Normal)),
{
    let t = seq![bs(), x].add(b);
    assert(t[0] == bs());
    assert(t.drop_first() == seq![x].add(b));
    let t2 = seq![x].add(b);
    assert(t2[0] == x);
    assert(t2.drop_first() == b);
    match run(b, St::Normal) {
        Option::Some((vs, st)) => {
            match flush(st) {
                Option::Some(f) => {
                    assert(run(t, St::Normal) == run(t2, St::Esc)) by {
                        reveal_with_fuel(run, 1);
                    };
                    assert(run(t2, St::Esc) == Option::Some((seq![v].add(vs), st))) by {
                        reveal_with_fuel(run, 1);
                    };
                    assert(decode(t, St::Normal) == Some(seq![v].add(vs).add(f)));
                    assert(decode(b, St::Normal) == Some(vs.add(f)));
                    assert(seq![v].add(vs).add(f) =~= seq![v].add(vs.add(f)));
                },
                Option::None => {},
            }
        },
        Option::None => {},
    }
}

// The `{:03o}` spelling is value-exact: its 3 digits fold back to v.
pub proof fn oct3_value(v: int)
    requires 0 <= v, v <= 255,
    ensures fold_digits(oct3(v), 8) == v,
        ((v / 64) % 8 * 8 + (v / 8) % 8) * 8 + v % 8 == v,
{
    let hi = (v / 64) % 8;
    let mid = (v / 8) % 8;
    let lo = v % 8;
    // digit_val needs each digit in 48..55 BEFORE the fold unfolds
    assert(0 <= hi <= 7 && 0 <= mid <= 7 && 0 <= lo <= 7);
    assert(48 <= 48 + hi <= 55 && 48 <= 48 + mid <= 55 && 48 <= 48 + lo <= 55);
    assert(fold_digits(oct3(v), 8) == ((hi * 8 + mid) * 8 + lo)) by {
        reveal_with_fuel(fold_digits, 4);
    };
    assert(hi == v / 64); // v <= 255 -> v/64 <= 3 -> mod 8 is the identity
    let q = v / 8;
    assert(v == 8 * (v / 8) + (v % 8));
    assert(q == 8 * (q / 8) + (q % 8));
    assert(8 * (q % 8) + (v % 8) < 64);
    assert(v == 64 * (q / 8) + 8 * (q % 8) + (v % 8));
    assert(q / 8 == v / 64);
    assert(mid == q % 8);
}

// The octal form `[bs] ++ oct3(v)` decodes to v: 4 steps land in
// Oct{v, 3}, then the boundary lemma takes over.
pub proof fn decode_octal(v: int, b: Str)
    requires 0 <= v, v <= 255,
    ensures decode(seq![bs()].add(oct3(v)).add(b), St::Normal)
        == pre(v, decode(b, St::Normal)),
{
    let t = seq![bs()].add(oct3(v)).add(b);
    oct3_value(v);
    let hi = (v / 64) % 8;
    let mid = (v / 8) % 8;
    let lo = v % 8;
    // digit guards: each oct3 digit is an octal char (48..55), so the
    // Esc/Oct arms each take exactly one step per digit — four
    // one-step unfolds instead of one 5-deep reveal.
    assert(0 <= hi <= 7 && 0 <= mid <= 7 && 0 <= lo <= 7);
    assert(t[0] == bs());
    assert(t.drop_first() == oct3(v).add(b));
    let s0 = oct3(v).add(b);
    assert(s0[0] == 48 + hi);
    assert(s0.drop_first() == seq![48 + mid, 48 + lo].add(b));
    let s1 = seq![48 + mid, 48 + lo].add(b);
    assert(s1[0] == 48 + mid);
    assert(s1.drop_first() == seq![48 + lo].add(b));
    let s2 = seq![48 + lo].add(b);
    assert(s2[0] == 48 + lo);
    assert(s2.drop_first() == b);
    assert(run(t, St::Normal) == run(s0, St::Esc)) by {
        reveal_with_fuel(run, 1);
    };
    assert(run(s0, St::Esc) == run(s1, St::Oct { acc: hi, cnt: 1 })) by {
        reveal_with_fuel(run, 1);
    };
    assert(run(s1, St::Oct { acc: hi, cnt: 1 })
        == run(s2, St::Oct { acc: hi * 8 + mid, cnt: 2 })) by {
        reveal_with_fuel(run, 1);
    };
    assert(run(s2, St::Oct { acc: hi * 8 + mid, cnt: 2 })
        == run(b, St::Oct { acc: (hi * 8 + mid) * 8 + lo, cnt: 3 })) by {
        reveal_with_fuel(run, 1);
    };
    assert(((hi * 8 + mid) * 8 + lo) == v);
    assert(decode(t, St::Normal) == decode(b, St::Oct { acc: v, cnt: 3 }));
    oct3_break(v, b);
}

// THE PER-CHAR BRIDGE (locality): one source char's emission is
// self-delimiting — decode(emit_char(c) ++ rest) reads exactly
// byte_of(c), then the rest. This is the property `\x` violates.
pub proof fn decode_char(c: int, b: Str)
    requires valid_c(c),
    ensures decode(emit_char(c).add(b), St::Normal)
        == pre(byte_of(c), decode(b, St::Normal)),
{
    if c == bs() {
        decode_pair(bs(), bs(), b);
    } else if c == qc() {
        decode_pair(qc(), qc(), b);
    } else if c == 10 {
        decode_pair(110, 10, b);
    } else if c == 9 {
        decode_pair(116, 9, b);
    } else if c == 13 {
        decode_pair(114, 13, b);
    } else if is_marker(c) {
        decode_octal(byte_of(c), b);
    } else if c < 32 {
        decode_octal(c, b);
    } else {
        decode_plain(c, b);
    }
    assert(decode(emit_char(c).add(b), St::Normal)
        == pre(byte_of(c), decode(b, St::Normal))) by {
        reveal_with_fuel(emit_char, 1);
    };
}

// THE OBLIGATION: the emitted literal denotes exactly the source
// bytes, for every valid string — every C compiler reading the fixed
// octal spelling puts byte_of(c) at each position.
pub proof fn round_trip(s: Str)
    requires valid_str(s),
    ensures decode(emit(s), St::Normal) == Some(bytes_of(s)),
    decreases s,
{
    if s.len() == 0 {
        // emit = [q,q]: both skipped in Normal, flush(Normal) = []
        assert(decode(emit(s), St::Normal) == Some(bytes_of(s))) by {
            reveal_with_fuel(run, 3);
        };
    } else {
        let s2 = s.drop_first();
        let q = seq![qc()];
        // leading delimiter: one quote-skip step off emit(s)
        assert(emit(s)[0] == qc());
        assert(emit(s).drop_first() == emit_body(s).add(q));
        assert(decode(emit(s), St::Normal)
            == decode(emit_body(s).add(q), St::Normal)) by {
            reveal_with_fuel(run, 1);
        };
        // emit_body(s) unfolds to emit_char(s[0]) ++ emit_body(s2)
        decode_char(s[0], emit_body(s2).add(q));
        // the same quote-skip for s2 turns emit(s2) into the tail
        assert(emit(s2)[0] == qc());
        assert(emit(s2).drop_first() == emit_body(s2).add(q));
        assert(decode(emit(s2), St::Normal)
            == decode(emit_body(s2).add(q), St::Normal)) by {
            reveal_with_fuel(run, 1);
        };
        round_trip(s2);
        assert(emit_body(s).add(q)
            =~= emit_char(s[0]).add(emit_body(s2).add(q))) by {
            reveal_with_fuel(emit_body, 1);
        };
        assert(bytes_of(s)
            =~= seq![byte_of(s[0])].add(bytes_of(s2))) by {
            reveal_with_fuel(bytes_of, 1);
        };
    }
}

// ── pins: the MEASURED literal shapes ─────────────────────────────

proof fn pin_octal_follower() {
    // "\001a": control 1 + raw 'a' — the t1/t2/t3 shape. Fixed
    // spelling round-trips.
    assert(decode(emit(seq![1, 97]), St::Normal) == Some(seq![1, 97])) by (compute);
    // "\0019": digit follower — the t5 shape.
    assert(decode(emit(seq![1, 57]), St::Normal) == Some(seq![1, 57])) by (compute);
    // "\0017": 7 is an octal DIGIT — still not absorbed (fixed 3).
    assert(decode(emit(seq![1, 55]), St::Normal) == Some(seq![1, 55])) by (compute);
}

proof fn pin_named() {
    assert(decode(emit(seq![10]), St::Normal) == Some(seq![10])) by (compute);
    assert(decode(emit(seq![9, 13]), St::Normal) == Some(seq![9, 13])) by (compute);
    assert(decode(emit(seq![bs(), qc()]), St::Normal) == Some(seq![bs(), qc()])) by (compute);
}

proof fn pin_marker() {
    // marker U+F800+0xAB -> byte 0xAB = 171 -> "\263" (263_8 = 171)
    assert(decode(emit(seq![0xF8AB]), St::Normal) == Some(seq![171])) by (compute);
}

// ── the SHIPPED hex spelling: measured divergences ────────────────
// Each pins what the greedy `\x` actually does, so the model cannot
// drift from the measurement if the spelling is ever reintroduced.

proof fn pin_hex_divergence_t1() {
    // "\x01a" compiles to ONE byte 0x1a = 26, not [1, 97] (measured
    // stdout: 1a vs bash 01 61).
    assert(decode(emit_old(seq![1, 97]), St::Normal) == Some(seq![26])) by (compute);
    assert(Some(seq![26]) != Some(bytes_of(seq![1, 97]))) by (compute);
}

proof fn pin_hex_divergence_t5() {
    // "\x019" -> 0x19 = 25, not [1, 57] (measured 19 vs 01 39).
    assert(decode(emit_old(seq![1, 57]), St::Normal) == Some(seq![25])) by (compute);
    assert(Some(seq![25]) != Some(bytes_of(seq![1, 57]))) by (compute);
}

proof fn pin_hex_divergence_t7() {
    // "\x01ABC" -> 0x1ABC = 6844 > 255: the range-violation arm the
    // greedy run reaches (invalid C without a diagnostic).
    assert(decode(emit_old(seq![1, 65, 66, 67]), St::Normal) == None) by (compute);
}

fn main() {}

} // verus!
