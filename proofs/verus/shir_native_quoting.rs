//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_quoting.rs — cluster H of the
//  `shir_native_stmt` decomposition: literal quoting/escaping round-trips.
//
//  `transforms/shir_native_stmt.rs` does not emit raw text into its target
//  syntaxes; every literal goes through an escape function first:
//
//    perl_quote              → a Perl double-quoted literal
//    tr_set_escape           → the inside of a Perl `tr///` set
//    decode_capture_escapes  → `$(printf …)` capture text (`\n \t \r \\`)
//    decode_echo_escapes     → `echo -e` output (`\a \b \f \n \r \t \v \\ \0`)
//    regex_escape_literal    → a literal inside a grep BRE
//
//  Each of those is a QUOTE function for one target syntax. The law cluster
//  H owes is the inverse property of each, written against a parser that
//  knows only the target syntax and shares no code with the encoder:
//
//    escape_roundtrip(mode, s):
//        unquote(mode, quote(mode, s)) == Some(s)
//
//  and one instantiation per target so a reader can cite the syntax:
//
//    perl_double_quote_roundtrip   captures_escape_roundtrip
//    tr_set_roundtrip              echo_escape_roundtrip
//    bre_escape_roundtrip
//
//  ── the model ──────────────────────────────────────────────────────────
//
//  All five syntaxes share one shape: a character either stays literal or is
//  written as `\` + a KEY character. `needs(mode, c)` is exactly the source's
//  match arm list, `key(mode, c)` the character written after the backslash,
//  and `dec(mode, k)` the target syntax's own reading of that escape — the
//  semantic side. `quote` is the left-to-right encoder; `unquote` is an
//  independent left-to-right parser over `dec` that never consults `needs`
//  or `key`. `key_is_inverse` is the only bridge, and it is stated as an
//  implication so a mode that escapes nothing needs no case.
//
//  The parser REFUSES an unknown escape (`dec` → None) in every syntax
//  except BRE, where POSIX says `\x` is `x`. That refusal is load-bearing:
//  it is what makes `decode_echo_escapes`/`decode_capture_escapes` reject
//  input they cannot represent rather than guess, and it is pinned.
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * `perl_quote` writes the surrounding `"` itself; the round-trip is
//     proved for the BODY (the escaped text between the delimiters), and
//     `pin_perl_double_quote_is_escaped` proves the body cannot terminate
//     the literal. Wrapping a proven body in two literal quotes is the
//     renderer's trivial contract.
//   * `render_test_text`/`render_flag_substitution_test` are RENDERERS, not
//     quote functions: their law is cluster D's test-expression round trip
//     (`eval(parse(render(t))) = eval(t)`), not an inverse of an escape
//     table. They are named in the plan's H list but their semantic side is
//     D's grammar, so this file does not claim them; see the README section.
//   * For `decode_echo_escapes`/`decode_capture_escapes` the encoder is the
//     canonical inverse of the subset `dec` accepts. The source only
//     DECODES; the law proved is that its accepted subset is exactly
//     invertible, which is the property the folds rely on when they emit an
//     escaped literal for a captured value.
//   * `static_pipeline_truth` is a grep truth value over literal text, not a
//     quoting function; it belongs to cluster F2 and is not claimed here.
//
//  ── measured mutants (restored byte-identically; sha in the sync file) ──
//
//  baseline 19 verified, 0 errors
//
//  counted (the mutation compiles and the proof goes red, 18v/1e):
//    m2  Perl decodes \n as a tab                    (key_is_inverse/round trip)
//    m4  echo accepts an unknown \q                   (pin_echo_unknown_escape_refused)
//    m5  tr decodes \0 as the letter O                (key_is_inverse/round trip)
//
//  refuted one stage earlier, at simplification, by a concrete pin:
//    m1  the encoder drops the tail of the string     (pin_perl_dollar_is_escaped)
//    m3  Perl stops escaping `$`                      (pin_perl_dollar_is_escaped)
//    m6  BRE refuses every escape                     (pin_bre_metacharacters_are_escaped)
//
//  Six mutants measured; each restored byte-identically (sha256 in the sync
//  file). The three counted ones report 18 verified, 1 error.
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub enum Mode {
    Perl,
    Tr,
    Capture,
    Echo,
    Bre,
}

/// The characters the source's escape match arm rewrites in this syntax.
pub open spec fn needs(mode: Mode, c: char) -> bool {
    match mode {
        Mode::Perl => c == '"' || c == '\\' || c == '$' || c == '@'
            || c == '\n' || c == '\t' || c == '\r',
        Mode::Tr => c == '\n' || c == '\t' || c == '\r' || c == '\x0c'
            || c == '\0' || c == '\\',
        Mode::Capture => c == '\n' || c == '\t' || c == '\r' || c == '\\',
        Mode::Echo => c == '\x07' || c == '\x08' || c == '\x0c' || c == '\n'
            || c == '\r' || c == '\t' || c == '\x0b' || c == '\\' || c == '\0',
        Mode::Bre => c == '\\' || c == '.' || c == '+' || c == '*' || c == '?'
            || c == '(' || c == ')' || c == '[' || c == ']' || c == '{'
            || c == '}' || c == '^' || c == '$' || c == '|',
    }
}

/// The character written after the backslash (`c` itself for Perl/BRE,
/// where the escape is a quoting escape rather than a named control).
pub open spec fn key(mode: Mode, c: char) -> char {
    match mode {
        Mode::Perl => if c == '\n' { 'n' } else if c == '\t' { 't' }
            else if c == '\r' { 'r' } else { c },
        Mode::Tr => if c == '\n' { 'n' } else if c == '\t' { 't' }
            else if c == '\r' { 'r' } else if c == '\x0c' { 'f' }
            else if c == '\0' { '0' } else { '\\' },
        Mode::Capture => if c == '\n' { 'n' } else if c == '\t' { 't' }
            else if c == '\r' { 'r' } else { '\\' },
        Mode::Echo => if c == '\x07' { 'a' } else if c == '\x08' { 'b' }
            else if c == '\x0c' { 'f' } else if c == '\n' { 'n' }
            else if c == '\r' { 'r' } else if c == '\t' { 't' }
            else if c == '\x0b' { 'v' } else if c == '\0' { '0' }
            else { '\\' },
        Mode::Bre => c,
    }
}

/// The target syntax's reading of `\` + `k`. None = the syntax refuses it.
pub open spec fn dec(mode: Mode, k: char) -> Option<char> {
    match mode {
        Mode::Perl => if k == 'n' { Some('\n') } else if k == 't' { Some('\t') }
            else if k == 'r' { Some('\r') } else if k == '"' { Some('"') }
            else if k == '\\' { Some('\\') } else if k == '$' { Some('$') }
            else if k == '@' { Some('@') } else { None },
        Mode::Tr => if k == 'n' { Some('\n') } else if k == 't' { Some('\t') }
            else if k == 'r' { Some('\r') } else if k == 'f' { Some('\x0c') }
            else if k == '0' { Some('\0') } else if k == '\\' { Some('\\') }
            else { None },
        Mode::Capture => if k == 'n' { Some('\n') } else if k == 't' { Some('\t') }
            else if k == 'r' { Some('\r') } else if k == '\\' { Some('\\') }
            else { None },
        Mode::Echo => if k == 'a' { Some('\x07') } else if k == 'b' { Some('\x08') }
            else if k == 'f' { Some('\x0c') } else if k == 'n' { Some('\n') }
            else if k == 'r' { Some('\r') } else if k == 't' { Some('\t') }
            else if k == 'v' { Some('\x0b') } else if k == '0' { Some('\0') }
            else if k == '\\' { Some('\\') } else { None },
        Mode::Bre => Some(k),
    }
}

/// One encoded character: `\` + key when `needs`, the literal otherwise.
pub open spec fn esc(mode: Mode, c: char) -> Seq<char> {
    if needs(mode, c) {
        seq!['\\', key(mode, c)]
    } else {
        seq![c]
    }
}

pub open spec fn quote(mode: Mode, s: Seq<char>) -> Seq<char>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        esc(mode, s[0]).add(quote(mode, s.drop_first()))
    }
}

pub open spec fn prepend(c: char, rest: Option<Seq<char>>) -> Option<Seq<char>> {
    match rest {
        Some(r) => Some(seq![c].add(r)),
        None => None,
    }
}

/// The parser: reads `\` + key through `dec`, literals through unchanged.
/// An unknown escape or a trailing `\` is refused.
pub open spec fn unquote(mode: Mode, t: Seq<char>) -> Option<Seq<char>>
    decreases t.len(),
{
    if t.len() == 0 {
        Some(Seq::empty())
    } else if t[0] == '\\' {
        if t.len() < 2 {
            None
        } else {
            match dec(mode, t[1]) {
                Some(c) => prepend(c, unquote(mode, t.subrange(2, t.len() as int))),
                None => None,
            }
        }
    } else {
        prepend(t[0], unquote(mode, t.subrange(1, t.len() as int)))
    }
}

/// The semantic bridge: whenever the encoder escapes `c`, the target syntax's
/// own reading of the key it wrote is `c`.
pub proof fn key_is_inverse(mode: Mode, c: char)
    ensures needs(mode, c) ==> dec(mode, key(mode, c)) == Some(c),
{
    match mode {
        Mode::Perl => {
            if needs(Mode::Perl, c) {
                if c == '"' {
                    assert(dec(Mode::Perl, key(Mode::Perl, c)) == Some(c));
                } else if c == '\\' {
                    assert(dec(Mode::Perl, key(Mode::Perl, c)) == Some(c));
                } else if c == '$' {
                    assert(dec(Mode::Perl, key(Mode::Perl, c)) == Some(c));
                } else if c == '@' {
                    assert(dec(Mode::Perl, key(Mode::Perl, c)) == Some(c));
                } else if c == '\n' {
                    assert(dec(Mode::Perl, key(Mode::Perl, c)) == Some(c));
                } else if c == '\t' {
                    assert(dec(Mode::Perl, key(Mode::Perl, c)) == Some(c));
                } else if c == '\r' {
                    assert(dec(Mode::Perl, key(Mode::Perl, c)) == Some(c));
                }
            }
        }
        Mode::Tr => {
            if needs(Mode::Tr, c) {
                if c == '\n' {
                    assert(dec(Mode::Tr, key(Mode::Tr, c)) == Some(c));
                } else if c == '\t' {
                    assert(dec(Mode::Tr, key(Mode::Tr, c)) == Some(c));
                } else if c == '\r' {
                    assert(dec(Mode::Tr, key(Mode::Tr, c)) == Some(c));
                } else if c == '\x0c' {
                    assert(dec(Mode::Tr, key(Mode::Tr, c)) == Some(c));
                } else if c == '\0' {
                    assert(dec(Mode::Tr, key(Mode::Tr, c)) == Some(c));
                } else if c == '\\' {
                    assert(dec(Mode::Tr, key(Mode::Tr, c)) == Some(c));
                }
            }
        }
        Mode::Capture => {
            if needs(Mode::Capture, c) {
                if c == '\n' {
                    assert(dec(Mode::Capture, key(Mode::Capture, c)) == Some(c));
                } else if c == '\t' {
                    assert(dec(Mode::Capture, key(Mode::Capture, c)) == Some(c));
                } else if c == '\r' {
                    assert(dec(Mode::Capture, key(Mode::Capture, c)) == Some(c));
                } else if c == '\\' {
                    assert(dec(Mode::Capture, key(Mode::Capture, c)) == Some(c));
                }
            }
        }
        Mode::Echo => {
            if needs(Mode::Echo, c) {
                if c == '\x07' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\x08' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\x0c' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\n' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\r' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\t' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\x0b' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\\' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                } else if c == '\0' {
                    assert(dec(Mode::Echo, key(Mode::Echo, c)) == Some(c));
                }
            }
        }
        Mode::Bre => {
            assert(dec(Mode::Bre, key(Mode::Bre, c)) == Some(c));
        }
    }
}

/// One encoder step, consumed by the parser.
pub proof fn unquote_esc(mode: Mode, c: char, t: Seq<char>)
    ensures unquote(mode, esc(mode, c).add(t)) == prepend(c, unquote(mode, t)),
{
    key_is_inverse(mode, c);
    if needs(mode, c) {
        assert(esc(mode, c) == seq!['\\', key(mode, c)]);
        assert((seq!['\\', key(mode, c)].add(t)).subrange(2, 2 + t.len() as int) == t);
    } else {
        assert(esc(mode, c) == seq![c]);
        assert(c != '\\');
        assert((seq![c].add(t)).subrange(1, 1 + t.len() as int) == t);
    }
}

/// The general round-trip lemma, with an arbitrary suffix so the induction
/// goes through.
pub proof fn quote_append(mode: Mode, s: Seq<char>, t: Seq<char>)
    ensures unquote(mode, quote(mode, s).add(t))
        == (match unquote(mode, t) { Some(r) => Some(s.add(r)), None => None }),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        assert(quote(mode, s) == esc(mode, s[0]).add(quote(mode, s.drop_first())));
        assert(quote(mode, s).add(t)
            == esc(mode, s[0]).add(quote(mode, s.drop_first()).add(t)));
        quote_append(mode, s.drop_first(), t);
        unquote_esc(mode, s[0], quote(mode, s.drop_first()).add(t));
    }
}

/// Cluster H's law, parameterised by target syntax.
pub proof fn escape_roundtrip(mode: Mode, s: Seq<char>)
    ensures unquote(mode, quote(mode, s)) == Some(s),
{
    quote_append(mode, s, Seq::empty());
}

// ─────────────────────────────── one citation per target syntax ──────────

pub proof fn perl_double_quote_roundtrip(s: Seq<char>)
    ensures unquote(Mode::Perl, quote(Mode::Perl, s)) == Some(s),
{
    escape_roundtrip(Mode::Perl, s);
}

pub proof fn tr_set_roundtrip(s: Seq<char>)
    ensures unquote(Mode::Tr, quote(Mode::Tr, s)) == Some(s),
{
    escape_roundtrip(Mode::Tr, s);
}

pub proof fn captures_escape_roundtrip(s: Seq<char>)
    ensures unquote(Mode::Capture, quote(Mode::Capture, s)) == Some(s),
{
    escape_roundtrip(Mode::Capture, s);
}

pub proof fn echo_escape_roundtrip(s: Seq<char>)
    ensures unquote(Mode::Echo, quote(Mode::Echo, s)) == Some(s),
{
    escape_roundtrip(Mode::Echo, s);
}

pub proof fn bre_escape_roundtrip(s: Seq<char>)
    ensures unquote(Mode::Bre, quote(Mode::Bre, s)) == Some(s),
{
    escape_roundtrip(Mode::Bre, s);
}

// ───────────────────────────── load-bearing refusals (pins) ──────────────

/// A `$` must not reach the Perl literal raw: it would interpolate at the
/// renderer, changing the value the fold claimed to have proved.
pub proof fn pin_perl_dollar_is_escaped() {
    assert(quote(Mode::Perl, seq!['$', 'x']) == seq!['\\', '$', 'x']) by (compute);
}

/// A `"` in the body must be escaped or it would end the literal early.
pub proof fn pin_perl_double_quote_is_escaped() {
    assert(quote(Mode::Perl, seq!['"']) == seq!['\\', '"']) by (compute);
    assert(quote(Mode::Perl, seq!['"'])[0] == '\\');
}

/// `tr///` is not a string: a raw control character (newline here) is a
/// physical line break that ends the operator, so the escape is mandatory.
pub proof fn pin_tr_newline_is_escaped() {
    assert(quote(Mode::Tr, seq!['\n']) == seq!['\\', 'n']) by (compute);
    assert(quote(Mode::Tr, seq!['\n'])[1] == 'n');
}

/// `echo -e` refuses an escape it does not know, rather than guessing.
pub proof fn pin_echo_unknown_escape_refused() {
    assert(dec(Mode::Echo, 'q') == None::<char>);
    assert(dec(Mode::Echo, 'z') == None::<char>);
}

/// The `$(printf …)` decoder has the same refusal.
pub proof fn pin_capture_unknown_escape_refused() {
    assert(dec(Mode::Capture, 'q') == None::<char>);
    assert(dec(Mode::Capture, 'x') == None::<char>);
}

/// A BRE metacharacter must be escaped to mean itself; the resulting body
/// carries the backslash and is read back as the same character.
pub proof fn pin_bre_metacharacters_are_escaped() {
    assert(quote(Mode::Bre, seq!['.']) == seq!['\\', '.']) by (compute);
    assert(quote(Mode::Bre, seq!['*', 'a', '^']) == seq!['\\', '*', 'a', '\\', '^']) by (compute);
    assert(unquote(Mode::Bre, seq!['\\', '.']) == Some(seq!['.'])) by (compute);
}

/// Nothing is escaped by accident: a plain letter passes through unchanged
/// in every syntax.
pub proof fn pin_plain_letters_pass_through() {
    assert(quote(Mode::Perl, seq!['a', '1', '/']) == seq!['a', '1', '/']) by (compute);
    assert(quote(Mode::Echo, seq!['a', '1', '/']) == seq!['a', '1', '/']) by (compute);
    assert(quote(Mode::Bre, seq!['a', '1', '/']) == seq!['a', '1', '/']) by (compute);
}

fn main() {}

} // verus!
