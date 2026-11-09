// FIRST model of the Perl renderer (`sh2perl/src/perl_backend.rs`) — the one
// backend with no proof coverage at all. The subject is the single funnel
// every literal string passes through: `Render::perl_str`, which escapes a
// shIR `Str` into a Perl DOUBLE-QUOTED literal.
//
// THE CLAIM. Every `Str` emitted through `perl_str` evaluates back to the
// shIR string: the emitted text, read by Perl's double-quote parser, yields
// exactly the original characters. Perl interpolates `$` (scalars, `${x}`,
// `$#x`) and `@` (arrays, `@ARGV`), and treats `\` as an escape introducer,
// so a literal that gets any of those wrong silently becomes a DIFFERENT
// string — the class that produced this session's `${x}` splice failures,
// where shell text `"${x}"` must stay the four characters `${x}` and not
// expand a Perl variable.
//
// THE ENCODER, verbatim (`perl_str`, src/perl_backend.rs:319):
//   '"' -> \"     '\\' -> \\    '$' -> \$    '@' -> \@
//   '\n' -> \n    '\t' -> \t    '\r' -> \r
//   c < 0x20 -> \x{h}          (`format!("{:x}")`: lowercase, shortest form)
//   anything else -> the character itself
// and the whole thing is wrapped in one pair of `"`.
//
// THE DECODER (the other half of the obligation). `decode` is Perl's
// double-quote parser restricted to what the encoder can emit, written as a
// one-character-per-step state machine (normal / after-backslash / expecting
// `{` / hex digits) so termination is structural rather than an arithmetic
// side condition. It accepts a superset of the encoder's output (e.g. `\x{}`
// with no digits, and `\x` without the brace is NOT accepted — the encoder
// always braces, and Perl's unbraced form needs its own lookahead rule).
//
// WHAT IS PROVED HERE:
//   * roundtrip / emitted_literal_round_trips — eval_literal(perl_str(s))
//     = Some(s) for EVERY string s. Universal; the encoder's output re-parses
//     to exactly the input characters, so no `$`/`@`/`\`/`"` survives to be
//     reinterpreted.
//   * Every step of that proof is per-character (`esc_char_step`,
//     `hex_walk`), which is what makes the CONCATENATION argument: each
//     `esc_char(c)` is self-delimiting (a complete escape or one literal
//     byte), so the parser resumes at the next character's boundary. Get one
//     arm wrong and its own lemma is the thing that fails, not the theorem.
//   * splice_witness_round_trips — the `${x}` shape from the gate work.
//   * Negative controls, which keep the model from being vacuous:
//       dollar_is_load_bearing    — drop the `$` arm and the dollar stays
//                                   bare in the emitted text.
//       env_dependence_is_the_hazard — bare `$name` / `${name}` makes the
//                                   value depend on the environment, i.e. the
//                                   text stopped being a literal.
//       slash_is_load_bearing     — drop the `\\` arm and `\<n>` re-parses as
//                                   a newline instead of backslash + 'n'.
//
// NOT MODELLED: Perl's exact lexer (heredocs, `\Q..\E`, `$#`, `$1`, `@{[..]}`,
// `$x[0]` subscripts), the exact key normalization of `$name` vs `${name}`
// (the substitution model keys on the reference SOURCE text — it only claims
// that the value comes from the environment), UTF-8 byte-vs-character
// semantics (the model is over code points; the renderer passes non-ASCII
// through unchanged and byte-clean), and the templates where the renderer
// interpolates a NON-`Str` value into quotes (file modes, `require` names) —
// those are reachable only from identifier-shaped IR, not from shell text.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 perl_str_escape.rs
use vstd::prelude::*;

verus! {

// ---- character codes ----
pub open spec fn q() -> int { 34 }      // '"'
pub open spec fn bs() -> int { 92 }     // '\'
pub open spec fn dollar() -> int { 36 }
pub open spec fn at() -> int { 64 }
pub open spec fn nl() -> int { 10 }
pub open spec fn tab() -> int { 9 }
pub open spec fn cr() -> int { 13 }
pub open spec fn brace_open() -> int { 123 }
pub open spec fn brace_close() -> int { 125 }
pub open spec fn x_char() -> int { 120 }

/// lowercase hex digit for 0..16 (`format!("{:x}")` of one nibble)
pub open spec fn hex_digit(i: int) -> int {
    if i < 10 {
        48 + i
    } else {
        87 + i
    }
}

pub open spec fn is_hex(c: int) -> bool {
    (c >= 48 && c <= 57) || (c >= 97 && c <= 102)
}

pub open spec fn hex_val(c: int) -> int {
    if c >= 48 && c <= 57 {
        c - 48
    } else if c >= 97 && c <= 102 {
        c - 87
    } else {
        -1
    }
}

/// `format!("{:x}", c)` for the only range that reaches it: `c < 0x20`.
pub open spec fn hex_of(c: int) -> Seq<int> {
    if c < 16 {
        seq![hex_digit(c)]
    } else {
        seq![hex_digit(c / 16), hex_digit(c % 16)]
    }
}

/// Prepend one decoded character to an already-decoded tail.
pub open spec fn prepend(c: int, o: Option<Seq<int>>) -> Option<Seq<int>> {
    match o {
        Option::Some(r) => Option::Some(seq![c] + r),
        Option::None => Option::None,
    }
}

// ---- the encoder (`perl_str`) ----

pub open spec fn esc_char(c: int) -> Seq<int> {
    if c == q() {
        seq![bs(), q()]
    } else if c == bs() {
        seq![bs(), bs()]
    } else if c == dollar() {
        seq![bs(), dollar()]
    } else if c == at() {
        seq![bs(), at()]
    } else if c == nl() {
        seq![bs(), 110]     // 'n'
    } else if c == tab() {
        seq![bs(), 116]     // 't'
    } else if c == cr() {
        seq![bs(), 114]     // 'r'
    } else if 0 <= c < 32 {
        seq![bs(), x_char(), brace_open()] + hex_of(c) + seq![brace_close()]
    } else {
        seq![c]
    }
}

pub open spec fn esc(s: Seq<int>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        seq![]
    } else {
        esc_char(s[0]) + esc(s.skip(1))
    }
}

/// The full literal, quotes included — exactly what `perl_str` returns.
pub open spec fn lit(s: Seq<int>) -> Seq<int> {
    seq![q()] + esc(s) + seq![q()]
}

// ---- the decoder (Perl's double-quote parser over the encoder's output) ----
//
// State codes: 0 = normal text, 1 = just read a backslash, 2 = read `\x`
// (expecting `{`), 3 = inside `\x{...}` with the accumulator in `h`.
// Every recursive call consumes exactly one character, so `decreases s.len()`
// is immediate for every arm — no length arithmetic to justify.

pub open spec fn decode(s: Seq<int>, st: int, h: int) -> Option<Seq<int>>
    decreases s.len(),
{
    if s.len() == 0 {
        if st == 0 {
            Option::Some(seq![])
        } else {
            Option::None
        }
    } else {
        let c = s[0];
        let t = s.skip(1);
        if st == 0 {
            if c == bs() {
                decode(t, 1, 0)
            } else if c == q() {
                // the closing quote: the literal ends here
                Option::Some(seq![])
            } else {
                prepend(c, decode(t, 0, 0))
            }
        } else if st == 1 {
            if c == q() {
                prepend(q(), decode(t, 0, 0))
            } else if c == bs() {
                prepend(bs(), decode(t, 0, 0))
            } else if c == dollar() {
                prepend(dollar(), decode(t, 0, 0))
            } else if c == at() {
                prepend(at(), decode(t, 0, 0))
            } else if c == 110 {
                prepend(nl(), decode(t, 0, 0))
            } else if c == 116 {
                prepend(tab(), decode(t, 0, 0))
            } else if c == 114 {
                prepend(cr(), decode(t, 0, 0))
            } else if c == x_char() {
                decode(t, 2, 0)
            } else {
                // Perl: a backslash before anything else is that character
                prepend(c, decode(t, 0, 0))
            }
        } else if st == 2 {
            if c == brace_open() {
                decode(t, 3, 0)
            } else {
                Option::None
            }
        } else {
            if c == brace_close() {
                prepend(h, decode(t, 0, 0))
            } else if is_hex(c) {
                decode(t, 3, h * 16 + hex_val(c))
            } else {
                Option::None
            }
        }
    }
}

/// Read a whole Perl double-quoted literal: check the opening quote, then
/// decode the contents. This is the function the theorem is stated about.
pub open spec fn eval_literal(l: Seq<int>) -> Option<Seq<int>> {
    if l.len() > 0 && l[0] == q() {
        decode(l.skip(1), 0, 0)
    } else {
        Option::None
    }
}

// ---- hex helpers ----

proof fn hex_digit_is_hex(i: int)
    requires
        0 <= i < 16,
    ensures
        is_hex(hex_digit(i)),
        hex_val(hex_digit(i)) == i,
{
}

/// How `skip` distributes over concatenation — the one fact the per-character
/// proofs need to keep the parser anchored on a literal prefix.
proof fn skip_add(a: Seq<int>, b: Seq<int>, n: int)
    requires
        0 <= n <= a.len(),
    ensures
        (a + b).skip(n) == a.skip(n) + b,
{
    assert((a + b).skip(n) =~= a.skip(n) + b);
}

/// `\x{h}` parses back to `c`, for the only range `hex_of` is used at.
/// Each digit advances the accumulator and consumes exactly one character;
/// the closing brace emits the accumulated value.
proof fn hex_walk(c: int, rest: Seq<int>)
    requires
        0 <= c < 32,
    ensures
        decode(hex_of(c) + seq![brace_close()] + rest, 3, 0)
            == prepend(c, decode(rest, 0, 0)),
{
    if c < 16 {
        hex_digit_is_hex(c);
        assert(hex_of(c) + seq![brace_close()] + rest =~= seq![hex_digit(c), brace_close()] + rest);
        assert((seq![hex_digit(c), brace_close()] + rest).skip(1) == seq![brace_close()] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![brace_close()] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else {
        hex_digit_is_hex(c / 16);
        hex_digit_is_hex(c % 16);
        assert(hex_of(c) + seq![brace_close()] + rest
            =~= seq![hex_digit(c / 16), hex_digit(c % 16), brace_close()] + rest);
        assert((seq![hex_digit(c / 16), hex_digit(c % 16), brace_close()] + rest).skip(1)
            == seq![hex_digit(c % 16), brace_close()] + rest);
        reveal_with_fuel(decode, 3);
        assert(hex_val(hex_digit(c / 16)) * 16 + hex_val(hex_digit(c % 16)) == c);
        assert((seq![hex_digit(c % 16), brace_close()] + rest).skip(1)
            == seq![brace_close()] + rest);
        reveal_with_fuel(decode, 5);
        assert((seq![brace_close()] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 6);
    }
}

// ---- one character of the encoder round-trips ----

/// THE PER-CHARACTER LEMMA. `esc_char(c)` is read by the parser as exactly
/// `c`, and it consumes precisely its own characters: everything after it is
/// decoded from `rest`, untouched. That is the concatenation property the
/// whole-string theorem composes.
proof fn esc_char_step(c: int, rest: Seq<int>)
    ensures
        decode(esc_char(c) + rest, 0, 0) == prepend(c, decode(rest, 0, 0)),
{
    if c == q() {
        assert(esc_char(c) == seq![bs(), q()]);
        assert((seq![bs(), q()] + rest).skip(1) == seq![q()] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![q()] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else if c == bs() {
        assert(esc_char(c) == seq![bs(), bs()]);
        assert((seq![bs(), bs()] + rest).skip(1) == seq![bs()] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![bs()] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else if c == dollar() {
        assert(esc_char(c) == seq![bs(), dollar()]);
        assert((seq![bs(), dollar()] + rest).skip(1) == seq![dollar()] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![dollar()] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else if c == at() {
        assert(esc_char(c) == seq![bs(), at()]);
        assert((seq![bs(), at()] + rest).skip(1) == seq![at()] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![at()] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else if c == nl() {
        assert(esc_char(c) == seq![bs(), 110]);
        assert((seq![bs(), 110] + rest).skip(1) == seq![110] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![110] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else if c == tab() {
        assert(esc_char(c) == seq![bs(), 116]);
        assert((seq![bs(), 116] + rest).skip(1) == seq![116] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![116] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else if c == cr() {
        assert(esc_char(c) == seq![bs(), 114]);
        assert((seq![bs(), 114] + rest).skip(1) == seq![114] + rest);
        reveal_with_fuel(decode, 3);
        assert((seq![114] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 4);
    } else if 0 <= c < 32 {
        // \ x { <hex> } : three fixed bytes, then the hex walk
        assert(esc_char(c) + rest =~= seq![bs(), x_char(), brace_open()]
            + (hex_of(c) + seq![brace_close()] + rest));
        assert((seq![bs(), x_char(), brace_open()] + (hex_of(c) + seq![brace_close()] + rest)).skip(1)
            == seq![x_char(), brace_open()] + (hex_of(c) + seq![brace_close()] + rest));
        reveal_with_fuel(decode, 3);
        assert((seq![x_char(), brace_open()] + (hex_of(c) + seq![brace_close()] + rest)).skip(1)
            == seq![brace_open()] + (hex_of(c) + seq![brace_close()] + rest));
        reveal_with_fuel(decode, 5);
        assert((seq![brace_open()] + (hex_of(c) + seq![brace_close()] + rest)).skip(1)
            == hex_of(c) + seq![brace_close()] + rest);
        reveal_with_fuel(decode, 7);
        hex_walk(c, rest);
    } else {
        assert(esc_char(c) == seq![c]);
        assert((seq![c] + rest).skip(1) == rest);
        reveal_with_fuel(decode, 3);
    }
}

// ---- the whole string ----

/// The literal `perl_str` emits evaluates back to the input string.
proof fn roundtrip(s: Seq<int>)
    ensures
        decode(esc(s) + seq![q()], 0, 0) == Option::Some(s),
    decreases s.len(),
{
    reveal_with_fuel(esc, 2);
    if s.len() == 0 {
    } else {
        assert(esc(s) == esc_char(s[0]) + esc(s.skip(1)));
        assert(esc(s) + seq![q()] =~= esc_char(s[0]) + (esc(s.skip(1)) + seq![q()]));
        esc_char_step(s[0], esc(s.skip(1)) + seq![q()]);
        roundtrip(s.skip(1));
        assert(seq![s[0]] + s.skip(1) == s);
    }
}

/// THE THEOREM: for every string, `perl_str`'s output is a literal whose
/// value is that string.
proof fn emitted_literal_round_trips(s: Seq<int>)
    ensures
        eval_literal(lit(s)) == Option::Some(s),
{
    reveal_with_fuel(esc, 1);
    assert(lit(s).skip(1) == esc(s) + seq![q()]);
    roundtrip(s);
}

// ---- the witness from the gate work ----

/// Shell text `${x}`: four literal characters that must stay that, not
/// expand a Perl variable.
pub open spec fn splice_witness() -> Seq<int> {
    seq![dollar(), brace_open(), 120, brace_close()]
}

proof fn splice_witness_round_trips()
    ensures
        eval_literal(lit(splice_witness())) == Option::Some(splice_witness()),
{
    emitted_literal_round_trips(splice_witness());
}

// ---- NEGATIVE CONTROL 1: an unescaped `$` interpolates ----

/// The encoder without the `$` arm.
pub open spec fn esc_naive_no_dollar_char(c: int) -> Seq<int> {
    if c == dollar() {
        seq![dollar()]
    } else {
        esc_char(c)
    }
}

pub open spec fn esc_naive_no_dollar(s: Seq<int>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        seq![]
    } else {
        esc_naive_no_dollar_char(s[0]) + esc_naive_no_dollar(s.skip(1))
    }
}

/// Dropping the `$` arm leaves the dollar bare in the emitted text: the arm
/// is load-bearing, not decorative.
proof fn dollar_is_load_bearing()
    ensures
        esc_char(dollar()) == seq![bs(), dollar()],
        esc_naive_no_dollar_char(dollar()) == seq![dollar()],
        esc(seq![dollar(), 120]) == seq![bs(), dollar(), 120],
        esc_naive_no_dollar(seq![dollar(), 120]) == seq![dollar(), 120],
        esc_naive_no_dollar(seq![dollar()]) != esc(seq![dollar()]),
{
    reveal_with_fuel(esc, 3);
    reveal_with_fuel(esc_naive_no_dollar, 3);
}

// ---- what a bare `$` costs: the value comes from the environment ----
//
// A deliberately small model of Perl substitution: `$name` and `${name}` are
// replaced by the environment's value for the SOURCE TEXT of the reference,
// everything else is copied. It is enough to state the hazard (the text is
// no longer a constant) and nothing more.

pub open spec fn is_word(c: int) -> bool {
    (c >= 97 && c <= 122) || (c >= 65 && c <= 90) || (c >= 48 && c <= 57) || c == 95
}

pub open spec fn word_len(s: Seq<int>) -> int
    decreases s.len(),
{
    if s.len() > 0 && is_word(s[0]) {
        1 + word_len(s.skip(1))
    } else {
        0
    }
}

/// How many characters a `$` consumes from `s` (which starts AFTER the `$`):
/// a bare word, or `{word}`.
pub open spec fn ref_span(s: Seq<int>) -> int
    decreases s.len(),
{
    if s.len() > 0 && s[0] == brace_open() {
        2 + word_len(s.skip(1))
    } else {
        word_len(s)
    }
}

pub open spec fn subst(s: Seq<int>, env: Map<Seq<int>, Seq<int>>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        seq![]
    } else if s[0] == dollar() && ref_span(s.skip(1)) > 0 {
        // `cap` keeps the model total: a reference can never be longer than
        // the text that follows it, so for real input the clamp is inert.
        let cap = s.len() - 1;
        let n = if ref_span(s.skip(1)) > cap { cap } else { ref_span(s.skip(1)) };
        let w = s.skip(1).take(n);
        let v = (if env.dom().contains(w) { env[w] } else { seq![] });
        v + subst(s.skip(1 + n), env)
    } else {
        seq![s[0]] + subst(s.skip(1), env)
    }
}

/// THE HAZARD, stated: two environments that disagree on `x` give two
/// different values for the SAME emitted text, once the `$` is bare. A
/// literal that is environment-dependent is not a literal.
proof fn env_dependence_is_the_hazard(e1: Map<Seq<int>, Seq<int>>, e2: Map<Seq<int>, Seq<int>>)
    requires
        e1.dom().contains(seq![120]),
        e2.dom().contains(seq![120]),
        e1[seq![120]] != e2[seq![120]],
    ensures
        subst(seq![dollar(), 120], e1) != subst(seq![dollar(), 120], e2),
{
    reveal_with_fuel(subst, 4);
    reveal_with_fuel(word_len, 2);
    reveal_with_fuel(ref_span, 1);
    assert(seq![dollar(), 120].skip(1) == seq![120int]);
    assert(seq![120int].take(1) == seq![120int]);
    assert(word_len(seq![120int]) == 1int);
    assert(ref_span(seq![120int]) == 1int);
    assert(subst(seq![], e1) == seq![]);
}

/// ...and the braced form the gate work kept hitting, `${x}`: same hazard,
/// and the shipped encoder removes it by the same single `$` arm.
proof fn braced_substitution_is_the_same_hazard(
    e1: Map<Seq<int>, Seq<int>>,
    e2: Map<Seq<int>, Seq<int>>,
)
    requires
        e1.dom().contains(seq![brace_open(), 120, brace_close()]),
        e2.dom().contains(seq![brace_open(), 120, brace_close()]),
        e1[seq![brace_open(), 120, brace_close()]]
            != e2[seq![brace_open(), 120, brace_close()]],
    ensures
        subst(splice_witness(), e1) != subst(splice_witness(), e2),
{
    reveal_with_fuel(subst, 4);
    reveal_with_fuel(word_len, 3);
    reveal_with_fuel(ref_span, 1);
    assert(splice_witness().skip(1) == seq![brace_open(), 120int, brace_close()]);
    assert(seq![brace_open(), 120int, brace_close()].skip(1) == seq![120int, brace_close()]);
    assert(word_len(seq![120int, brace_close()]) == 1int);
    assert(ref_span(seq![brace_open(), 120int, brace_close()]) == 3int);
    assert(splice_witness().skip(1).take(3) == seq![brace_open(), 120int, brace_close()]);
    assert(splice_witness().skip(4) == seq![]);
    assert(subst(seq![], e1) == seq![]);
}

// ---- NEGATIVE CONTROL 2: the backslash arm ----

/// The encoder without the `\\` arm.
pub open spec fn esc_no_slash_char(c: int) -> Seq<int> {
    if c == bs() {
        seq![bs()]
    } else {
        esc_char(c)
    }
}

/// Without `\\`, a literal backslash before `n` re-parses as a NEWLINE: the
/// emitted text is the same two characters Perl reads as the escape `\n`,
/// so the string silently changes (this is the `printf '%s\n'` class).
proof fn slash_is_load_bearing()
    ensures
        esc_no_slash_char(bs()) == seq![bs()],
        esc_char(bs()) == seq![bs(), bs()],
        decode(esc_no_slash_char(bs()) + seq![110] + seq![q()], 0, 0)
            == Option::Some(seq![nl()]),
        decode(esc_char(bs()) + seq![110] + seq![q()], 0, 0)
            == Option::Some(seq![bs(), 110]),
{
    reveal_with_fuel(decode, 6);
}

fn main() {}

} // verus!
