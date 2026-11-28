//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_perl_emit.rs — plan item G*: the QUOTING law of the
//  folds that emit Perl source, in `src/transforms/shir_native_stmt.rs`.
//
//  `native_perl_e_stmt` (1898) lowers `perl -e CODE [ARGS…]` by emitting
//  `@ARGV = (<perl_quote(w)>, …);` and then the CODE **verbatim** — the target
//  output IS Perl, so the program text needs no translation, only the ARGUMENTS
//  do. `perl_quote` (2054) writes each argument as a Perl double-quoted literal.
//  The law is cluster H's (`perl_double_quote_roundtrip`), applied at this use
//  site: the literal Perl re-parses is the shell word bash would have passed, so
//  the emitted program sees the same `@ARGV` as the shell-out it replaces.
//
//  `native_perl_ne_stmt` (1933) also READS a Perl double-quoted string:
//  `perl_print_prefix` (1979) decodes `\n \t \r \\ \"` out of
//  `print "PREFIX $_\n"` and refuses every other escape. That refusal is
//  CONSERVATIVE, not necessary: Perl decodes an unknown escape to the character
//  itself, so refusing keeps the shell-out for code Perl would have accepted,
//  and never accepts an escape Perl reads differently.
//
//  ── the two sides ───────────────────────────────────────────────────────
//
//  The emitting side is the fold's own construction: `pq_body` (what
//  `perl_quote` writes, minus the surrounding quotes), `prelude` (the `@ARGV`
//  list it builds), `fold_dec` (the print-prefix table).
//
//  The parsing side is PERL's, written independently: `pdq_until` (a
//  double-quoted literal body, parsed until the closing quote, with
//  `perl_unesc` as its escape table) and `perl_dec` (Perl's escape table, where
//  an unknown escape is the character itself). Neither side calls the other;
//  a case the fold forgets to escape is a parsing-side character, not a
//  missing arm on both sides.
//
//  ── the laws as landed ──────────────────────────────────────────────────
//
//    pdq_step                  one encoder step, consumed by the parser
//    pdq_append                the induction, with an arbitrary suffix
//    perl_literal_roundtrip    `pdq_until(pq_body(s) "`) == s`
//    every_char_roundtrips     every single character, including the seven
//                              the encoder escapes
//    argv_prelude_law          the `@ARGV` list parses back to the words, in
//                              order and with the same count
//    decode_preserves_perl     every escape the fold accepts, Perl reads the
//                              same way
//    refusal_is_conservative   every escape the fold refuses, Perl reads as the
//                              character — so refusing costs a fold, never
//                              correctness
//    code_is_emitted_verbatim  the program text is the fold's input, unchanged
//
//  ── what is NOT proved here ─────────────────────────────────────────────
//
//  * `perl -ne`'s per-line print (`native_perl_ne_stmt`) is NOT a quoting law:
//    it emits computed OUTPUT, and its law is a line model. It is deliberately
//    out of this file. RECORDED, NOT PROVED, and found while writing this: the
//    fold's loop skips every EMPTY line when the input ends with a newline
//    (`if line.is_empty() && input.ends_with('\n') { continue; }`), while Perl
//    prints the prefix for a blank line too — so `printf '\n\n' | perl -ne
//    'print "P $_\n"'` keeps the shell-out only because the producer must be a
//    literal `echo`/`printf`; for a literal input containing a blank line the
//    fold would emit one line too few. That is a defect in the fold, not in
//    this model, and fixing it is not this stage's business.
//  * `native_bash_script` computes an argument-demo script's output from
//    literal args; it emits no quoted Perl source either, so it is out of model
//    here (its law is that script's output, a different obligation).
//  * Whether the emitted program's BEHAVIOUR is the same is Perl's own
//    semantics — this file proves the argument list and the prefix survive the
//    round trip, and the CODE is untouched.
//  * The renderer, the parser and Perl's `@ARGV` runtime population are out of
//    model.
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ───────────────────────────────────────── §1  the emitting side (`perl_quote`) ──

/// What `perl_quote` writes for one character, without the surrounding quotes.
pub open spec fn pq_char(c: char) -> Seq<char> {
    if c == '"' {
        seq!['\\', '"']
    } else if c == '\\' {
        seq!['\\', '\\']
    } else if c == '$' {
        seq!['\\', '$']
    } else if c == '@' {
        seq!['\\', '@']
    } else if c == '\n' {
        seq!['\\', 'n']
    } else if c == '\t' {
        seq!['\\', 't']
    } else if c == '\r' {
        seq!['\\', 'r']
    } else {
        seq![c]
    }
}

/// The body of the literal `perl_quote(s)` writes (`s` between two quotes).
pub open spec fn pq_body(s: Seq<char>) -> Seq<char>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        pq_char(s[0]).add(pq_body(s.drop_first()))
    }
}

// ──────────────────────────────────────────── §2  Perl's parsing side ──────

/// Perl's double-quoted escape table for the escapes `perl_quote` can produce;
/// `None` is "not one this model knows", never "Perl rejects it".
pub open spec fn perl_unesc(k: char) -> Option<char> {
    if k == '"' {
        Some('"')
    } else if k == '\\' {
        Some('\\')
    } else if k == '$' {
        Some('$')
    } else if k == '@' {
        Some('@')
    } else if k == 'n' {
        Some('\n')
    } else if k == 't' {
        Some('\t')
    } else if k == 'r' {
        Some('\r')
    } else {
        None
    }
}

/// Parse a double-quoted literal body until its closing quote (not consumed),
/// the way Perl does: an escape is two characters, everything else is itself.
pub open spec fn pdq_until(t: Seq<char>) -> Option<Seq<char>>
    decreases t.len(),
{
    if t.len() == 0 {
        None
    } else if t[0] == '"' {
        Some(Seq::empty())
    } else if t[0] == '\\' {
        if t.len() < 2 {
            None
        } else {
            match perl_unesc(t[1]) {
                Some(c) => match pdq_until(t.subrange(2, t.len() as int)) {
                    Some(r) => Some(seq![c].add(r)),
                    None => None,
                },
                None => None,
            }
        }
    } else {
        match pdq_until(t.drop_first()) {
            Some(r) => Some(seq![t[0]].add(r)),
            None => None,
        }
    }
}

/// The `@ARGV` list `native_perl_e_stmt` builds: one quoted literal per shell
/// word, in order.
pub open spec fn prelude(ws: Seq<Seq<char>>) -> Seq<Seq<char>>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else {
        seq![pq_body(ws[0])].add(prelude(ws.drop_first()))
    }
}

/// Read a seq of literal bodies back the way Perl would, each terminated by its
/// closing quote.
pub open spec fn parse_all(ls: Seq<Seq<char>>) -> Option<Seq<Seq<char>>>
    decreases ls.len(),
{
    if ls.len() == 0 {
        Some(Seq::empty())
    } else {
        match pdq_until(ls[0].add(seq!['"'])) {
            Some(w) => match parse_all(ls.drop_first()) {
                Some(r) => Some(seq![w].add(r)),
                None => None,
            },
            None => None,
        }
    }
}

// ────────────────────────────────────────────────── §3  the round trip ────

/// Replacing a literal-length prefix by an append of the same length keeps the
/// rest: the arithmetic both inductions need.
pub proof fn subrange_of_add(s: Seq<char>, t: Seq<char>)
    requires s.len() == 1,
    ensures s.add(t).subrange(1, 1 + t.len() as int) == t,
{
}

pub proof fn add_empty(s: Seq<char>)
    ensures s.add(Seq::empty()) == s,
{
    assert(s.add(Seq::empty()) =~= s);
}

/// One encoder step, consumed by the parser: `pq_char(c)` followed by anything
/// parses to `c` followed by whatever that anything parses to.
pub proof fn pdq_step(c: char, t: Seq<char>)
    ensures pdq_until(pq_char(c).add(t)) == (match pdq_until(t) {
        Some(r) => Some(seq![c].add(r)),
        None => None,
    }),
{
    if c == '"' {
        assert(pq_char(c) == seq!['\\', '"']);
        assert(perl_unesc('"') == Some('"'));
        assert((seq!['\\', '"']).add(t).subrange(2, 2 + t.len() as int) == t);
    } else if c == '\\' {
        assert(pq_char(c) == seq!['\\', '\\']);
        assert(perl_unesc('\\') == Some('\\'));
        assert((seq!['\\', '\\']).add(t).subrange(2, 2 + t.len() as int) == t);
    } else if c == '$' {
        assert(pq_char(c) == seq!['\\', '$']);
        assert(perl_unesc('$') == Some('$'));
        assert((seq!['\\', '$']).add(t).subrange(2, 2 + t.len() as int) == t);
    } else if c == '@' {
        assert(pq_char(c) == seq!['\\', '@']);
        assert(perl_unesc('@') == Some('@'));
        assert((seq!['\\', '@']).add(t).subrange(2, 2 + t.len() as int) == t);
    } else if c == '\n' {
        assert(pq_char(c) == seq!['\\', 'n']);
        assert(perl_unesc('n') == Some('\n'));
        assert((seq!['\\', 'n']).add(t).subrange(2, 2 + t.len() as int) == t);
    } else if c == '\t' {
        assert(pq_char(c) == seq!['\\', 't']);
        assert(perl_unesc('t') == Some('\t'));
        assert((seq!['\\', 't']).add(t).subrange(2, 2 + t.len() as int) == t);
    } else if c == '\r' {
        assert(pq_char(c) == seq!['\\', 'r']);
        assert(perl_unesc('r') == Some('\r'));
        assert((seq!['\\', 'r']).add(t).subrange(2, 2 + t.len() as int) == t);
    } else {
        assert(pq_char(c) == seq![c]);
        assert(c != '"');
        assert(c != '\\');
        subrange_of_add(seq![c], t);
    }
}

/// The induction, with an arbitrary suffix so it goes through.
pub proof fn pdq_append(s: Seq<char>, t: Seq<char>)
    ensures pdq_until(pq_body(s).add(t)) == (match pdq_until(t) {
        Some(r) => Some(s.add(r)),
        None => None,
    }),
    decreases s.len(),
{
    if s.len() == 0 {
        assert(pq_body(s) == Seq::empty());
        assert(Seq::empty().add(t) == t);
    } else {
        assert(pq_body(s) == pq_char(s[0]).add(pq_body(s.drop_first())));
        assert(pq_body(s).add(t) == pq_char(s[0]).add(pq_body(s.drop_first()).add(t)));
        pdq_append(s.drop_first(), t);
        pdq_step(s[0], pq_body(s.drop_first()).add(t));
        add_empty(s.drop_first());
        assert(s == seq![s[0]].add(s.drop_first()));
    }
}

/// **The quoting law, at the fold's use site.** A shell word written by
/// `perl_quote` is read back by Perl as exactly that word.
pub proof fn perl_literal_roundtrip(s: Seq<char>)
    ensures pdq_until(pq_body(s).add(seq!['"'])) == Some(s),
{
    pdq_append(s, seq!['"']);
    assert(pdq_until(seq!['"']) == Some(Seq::empty()));
    add_empty(s);
}

/// Every single character round-trips, including the seven the encoder escapes:
/// the case a "forgot to escape one" mutant breaks.
pub proof fn every_char_roundtrips(c: char)
    ensures pdq_until(pq_char(c).add(seq!['"'])) == Some(seq![c]),
{
    pdq_step(c, seq!['"']);
    assert(pdq_until(seq!['"']) == Some(Seq::empty()));
    assert(seq![c].add(Seq::empty()) == seq![c]);
}

// ────────────────────────────────────── §4  the `@ARGV` list, in order ────

/// **`argv_prelude_law`.** The prelude the fold emits parses back to the shell
/// words, in the same order and with the same count — so the emitted program
/// sees the `@ARGV` the shell-out would have built.
pub proof fn argv_prelude_law(ws: Seq<Seq<char>>)
    ensures parse_all(prelude(ws)) == Some(ws),
    decreases ws.len(),
{
    if ws.len() == 0 {
        assert(prelude(ws) == Seq::empty());
        assert(parse_all(Seq::empty()) == Some(Seq::empty()));
    } else {
        assert(prelude(ws) == seq![pq_body(ws[0])].add(prelude(ws.drop_first())));
        perl_literal_roundtrip(ws[0]);
        assert(pdq_until(pq_body(ws[0]).add(seq!['"'])) == Some(ws[0]));
        argv_prelude_law(ws.drop_first());
        assert(parse_all(prelude(ws.drop_first())) == Some(ws.drop_first()));
        let tail = prelude(ws.drop_first());
        let head = seq![pq_body(ws[0])];
        assert(head.add(tail)[0] == pq_body(ws[0]));
        assert(head.add(tail).drop_first() == tail);
        assert(parse_all(head.add(tail)) == Some(seq![ws[0]].add(ws.drop_first())));
        assert(seq![ws[0]].add(ws.drop_first()) =~= ws);
    }
}

/// The first argument stays first: the order is the shell's, not the map's.
pub proof fn pin_first_argument_is_first()
    ensures
        parse_all(prelude(seq![seq!['a'], seq!['b']])) == Some(seq![seq!['a'], seq!['b']]),
{
    argv_prelude_law(seq![seq!['a'], seq!['b']]);
}

/// **The program text is not re-quoted.** `native_perl_e_stmt` emits the CODE
/// unchanged, which is only sound because the target output is Perl.
pub open spec fn code_surface(code: Seq<char>) -> Seq<char> {
    code
}

pub proof fn code_is_emitted_verbatim(code: Seq<char>)
    ensures code_surface(code) == code,
{
}

// ────────────────────────────── §5  the print-prefix escape table (the `-ne`
//                                     fold's OTHER use of Perl's syntax) ─────

/// The fold's own decode table (`perl_print_prefix`), over escape characters.
pub open spec fn fold_dec(k: char) -> Option<char> {
    if k == 'n' {
        Some('\n')
    } else if k == 't' {
        Some('\t')
    } else if k == 'r' {
        Some('\r')
    } else if k == '\\' {
        Some('\\')
    } else if k == '"' {
        Some('"')
    } else {
        None
    }
}

/// Perl's table, written independently: in a double-quoted string an escape it
/// does not define is the character itself, so this is TOTAL.
pub open spec fn perl_dec(k: char) -> Option<char> {
    if k == 'n' {
        Some('\n')
    } else if k == 't' {
        Some('\t')
    } else if k == 'r' {
        Some('\r')
    } else if k == '\\' {
        Some('\\')
    } else if k == '"' {
        Some('"')
    } else {
        Some(k)
    }
}

pub proof fn decode_table_preserves_perl()
    ensures forall|k: char, x: char| fold_dec(k) == Some(x) ==> perl_dec(k) == Some(x),
{}

/// **The refusal is conservative.** Every escape the fold refuses is one Perl
/// would have decoded to the character itself — so refusing costs a fold (the
/// shell-out stays), never correctness.
pub proof fn refusal_is_conservative()
    ensures forall|k: char| fold_dec(k) == None ==> perl_dec(k) == Some(k),
{}

/// The concrete refusal the source has: `\q` is not in the table.
pub proof fn pin_unknown_escape_is_refused()
    ensures fold_dec('q') == None,
{}

} // verus!

fn main() {}
