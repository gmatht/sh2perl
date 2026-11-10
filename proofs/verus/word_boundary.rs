// THE WORD-BOUNDARY MODEL — `echo x $$` vs `echo x$$`, reduced to the rule
// that decides where one shell word ends and the next begins.
//
// THE CLAIM (the one harness/check_ast.pl pins as
// `ast_dollardollar_arg_boundary` / `ast_dollardollar_word_internal`): the
// two sources must produce DIFFERENT argument vectors. `x $$` is two words
// (the literal `x`, then the PID expansion); `x$$` is ONE word (the literal
// `x` followed by the PID inside the same word, printed without a space).
// bash prints `x 1234` vs `x1234`, so an AST that cannot tell them apart is
// a miscompile waiting for the first renderer that assumes adjacency — the
// class the check describes ("the parser cuts `$$` off the end").
//
// THE RULE, from the live parser (`sh2perl/src/parser/words.rs`). A word is
// built by two mechanisms, and BOTH have to hold for two tokens to share a
// word:
//
//   G1  the combine loop in `parse_word_inner` (~:530-620) continues over a
//       fixed token set; `Token::Space` / `Token::Tab` are NOT in it, so a
//       whitespace token ends the word (the loop `_ => break` arm), and the
//       caller skips the whitespace and starts a fresh word.
//   G2  `merge_contiguous_quoted_fragments` (~:180) glues a following
//       `$-expansion` part into the current word only when the fragments are
//       BYTE-ADJACENT: `next_start != prev_end` breaks out. A gap therefore
//       separates words even without a whitespace token.
//
// The model is the boundary predicate those two produce:
//
//   starts_word(s, i)  ==  s[i] is not whitespace
//                       && NOT (s[i-1] is not whitespace && adjacent(s[i-1], s[i]))
//
// and the argument count of a command is `words(s)` = the number of such
// positions. The observation the model is built on is that `dump_ast` prints
// one `WORD:` line per argument, so the check's `/^WORD:/` count IS
// `words(s)` (the literal bridge, exercised by the Rust test at the bottom of
// `words.rs` and by the gate).
//
// WHAT IS PROVED HERE:
//   * glued_is_one_word / spaced_is_two_words / the_argument_vectors_differ —
//     the two sources of the check, on the rule above, give 1 vs 2 words.
//   * lemma_whitespace_ends_the_word / lemma_gap_ends_the_word — the two
//     mechanisms as separate properties. Each is what a regression drops:
//       whitespace_is_load_bearing — with G1 removed (`starts_word_nows`)
//         the spaced source also gets ONE word: the two become
//         indistinguishable, which is the whole bug.
//       expansion_split_is_load_bearing — with the old "always cut `$$` off
//         the word" behaviour (`starts_word_bug`) the glued source also gets
//         TWO words, again indistinguishable.
//     Both mutants are stated as proved FACTS about the mutant rule, so the
//     controls fail if someone re-derives the rule the other way.
//   * the_word_keeps_its_text / the_split_keeps_the_pid — the merge and the
//     boundary are both value-preserving: the glued word's text is the
//     concatenation of its parts (`x` then the PID), and the spaced form's
//     second word is the PID on its own. Distinguishability therefore is not
//     bought by dropping or duplicating characters.
//
// NOT MODELLED: the real `Token` enum (only Ident / `$-expansion` / Space
// kinds appear), the combine loop's full continuation set (test-operator
// tokens, keywords, escapes — none can be `$$`), quoting, `$var` vs `$$` key
// resolution, and the renderer (the gate's `beh_dollardollar_*` cases compare
// the emitted program against bash, which is the behaviour tie).
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 word_boundary.rs

use vstd::prelude::*;

verus! {

/// Token kinds relevant to a word boundary. `Expansion` stands for every
/// `$`-expansion the merge arm accepts (`$$`, `$var`, `${x}`, ...); `Space`
/// for the whitespace tokens the combine loop refuses to consume.
#[derive(PartialEq, Eq, Structural)]
pub enum Kind {
    Ident,
    Expansion,
    Space,
}

/// One lexer token. `text` is the source text it covers (so a boundary can be
/// shown value-preserving); the concrete `$$` text is the PID, represented by
/// the single character 'P' — the obligation is about the boundary, not the
/// digit count.
pub struct Tok {
    pub k: Kind,
    pub start: int,
    pub end: int,
    pub text: Seq<char>,
}

// ---------------------------------------------------------------- the rule

spec fn adjacent(a: Tok, b: Tok) -> bool {
    a.end == b.start
}

/// Would the parser extend the word that ends at token `j` with token `j+1`?
/// G1: a whitespace token is never part of a word, so nothing continues out of
/// one (the combine loop's `_ => break`, and the whitespace the caller then
/// skips). G2: a non-adjacent token is not merged either (the `next_start !=
/// prev_end` break in `merge_contiguous_quoted_fragments`).
spec fn continues(s: Seq<Tok>, j: int) -> bool {
    if j < 0 || j + 1 >= s.len() {
        false
    } else {
        s[j].k != Kind::Space && s[j + 1].k != Kind::Space && adjacent(s[j], s[j + 1])
    }
}

/// Does token `i` begin a new word? Not whitespace itself, and the previous
/// token does not continue into it.
spec fn starts_word(s: Seq<Tok>, i: int) -> bool {
    0 <= i < s.len() && s[i].k != Kind::Space && !continues(s, i - 1)
}

/// The argument count: how many positions begin a word. `dump_ast` prints one
/// `WORD:` line per argument, so this is the quantity check_ast.pl counts.
spec fn count(s: Seq<Tok>, i: int) -> int
    decreases s.len() - i
{
    if i >= s.len() {
        0
    } else if starts_word(s, i) {
        1 + count(s, i + 1)
    } else {
        count(s, i + 1)
    }
}

spec fn words(s: Seq<Tok>) -> int {
    count(s, 0)
}

/// The text of the word beginning at `i`: its own text, then every token that
/// continues into it. Nothing is skipped and nothing is duplicated, so if the
/// merge is wrong this is where it shows.
spec fn word_text(s: Seq<Tok>, i: int) -> Seq<char>
    decreases s.len() - i
{
    if i < 0 || i >= s.len() {
        Seq::empty()
    } else if continues(s, i) {
        s[i].text + word_text(s, i + 1)
    } else {
        s[i].text
    }
}

// ------------------------------------------------- the two concrete sources

spec fn tok_x() -> Tok {
    Tok { k: Kind::Ident, start: 0, end: 1, text: seq!['x'] }
}

/// `$$` glued to `x`: starts where `x` ends.
spec fn tok_pid_glued() -> Tok {
    Tok { k: Kind::Expansion, start: 1, end: 3, text: seq!['P'] }
}

spec fn tok_space() -> Tok {
    Tok { k: Kind::Space, start: 1, end: 2, text: seq![' '] }
}

/// `$$` after the space: starts one byte later.
spec fn tok_pid_spaced() -> Tok {
    Tok { k: Kind::Expansion, start: 2, end: 4, text: seq!['P'] }
}

/// `echo x$$` — no whitespace token between the two (the lexer emits
/// Identifier("x") then DollarDollar).
spec fn glued() -> Seq<Tok> {
    seq![tok_x(), tok_pid_glued()]
}

/// `echo x $$` — the lexer emits Identifier("x"), Space, DollarDollar.
spec fn spaced() -> Seq<Tok> {
    seq![tok_x(), tok_space(), tok_pid_spaced()]
}

// ------------------------------------------------------------ the claim

proof fn glued_is_one_word()
    ensures words(glued()) == 1,
{
    // `compute` unfolds the concrete token sequences: Verus does not do it on
    // its own because `count`/`word_text` are recursive.
    assert(words(glued()) == 1) by (compute);
}

proof fn spaced_is_two_words()
    ensures words(spaced()) == 2,
{
    assert(words(spaced()) == 2) by (compute);
}

/// The check's real claim: the two sources cannot share an AST argument list.
proof fn the_argument_vectors_differ()
    ensures words(glued()) != words(spaced()),
{
    assert(words(glued()) == 1) by (compute);
    assert(words(spaced()) == 2) by (compute);
}

// ------------------------------------------------- the two mechanisms

/// G1 on its own: a token after a whitespace token always starts a word.
proof fn lemma_whitespace_ends_the_word(s: Seq<Tok>, j: int)
    requires
        0 <= j < s.len(),
        s[j].k == Kind::Space,
        j + 1 < s.len(),
        s[j + 1].k != Kind::Space,
    ensures
        starts_word(s, j + 1),
{
}

/// G2 on its own: a token after a byte gap always starts a word, whitespace or
/// not.
proof fn lemma_gap_ends_the_word(s: Seq<Tok>, j: int)
    requires
        0 <= j,
        j + 1 < s.len(),
        s[j].k != Kind::Space,
        s[j + 1].k != Kind::Space,
        !adjacent(s[j], s[j + 1]),
    ensures
        starts_word(s, j + 1),
{
}

/// A word's text is exactly its own text followed by every continuation's:
/// the merge never drops or repeats a character.
proof fn the_word_keeps_its_text()
    ensures
        word_text(glued(), 0) == seq!['x', 'P'],
        word_text(glued(), 0) == glued()[0].text + glued()[1].text,
{
    assert(word_text(glued(), 0) == seq!['x', 'P']) by (compute);
    assert(glued()[0].text + glued()[1].text == seq!['x', 'P']) by (compute);
}

/// The spaced form keeps the PID as the (whole) text of the second word.
proof fn the_split_keeps_the_pid()
    ensures
        starts_word(spaced(), 2),
        word_text(spaced(), 2) == seq!['P'],
        word_text(spaced(), 0) == seq!['x'],
{
}

// -------------------------------------------------------------- controls

/// MUTANT of the rule: drop G1 (whitespace no longer ends the word). Both
/// sources then yield ONE word — indistinguishable, i.e. the miscompile the
/// check exists to catch. Stating this as a proved fact about the mutant keeps
/// the control honest: if the guard stops being load-bearing, this proof is
/// what changes.
spec fn starts_word_nows(s: Seq<Tok>, i: int) -> bool {
    if i < 0 || i >= s.len() {
        false
    } else if i == 0 {
        s[i].k != Kind::Space
    } else {
        s[i].k != Kind::Space && !adjacent(s[i - 1], s[i])
    }
}

spec fn count_nows(s: Seq<Tok>, i: int) -> int
    decreases s.len() - i
{
    if i >= s.len() {
        0
    } else if starts_word_nows(s, i) {
        1 + count_nows(s, i + 1)
    } else {
        count_nows(s, i + 1)
    }
}

proof fn whitespace_is_load_bearing()
    ensures
        count_nows(spaced(), 0) == count_nows(glued(), 0),
        count_nows(spaced(), 0) == 1,
{
    assert(count_nows(spaced(), 0) == 1) by (compute);
    assert(count_nows(glued(), 0) == 1) by (compute);
}

/// MUTANT of the rule: the shipped bug, where an expansion token is ALWAYS cut
/// into its own argument (`starts_word_bug` accepts a word start on any
/// `Expansion`, ignoring adjacency). The glued source then also gets TWO
/// words, so `x$$` and `x $$` parse identically — the exact failure the check
/// pins.
spec fn starts_word_bug(s: Seq<Tok>, i: int) -> bool {
    if i < 0 || i >= s.len() {
        false
    } else if s[i].k == Kind::Space {
        false
    } else if i == 0 {
        true
    } else {
        s[i].k == Kind::Expansion || s[i - 1].k == Kind::Space || !adjacent(s[i - 1], s[i])
    }
}

spec fn count_bug(s: Seq<Tok>, i: int) -> int
    decreases s.len() - i
{
    if i >= s.len() {
        0
    } else if starts_word_bug(s, i) {
        1 + count_bug(s, i + 1)
    } else {
        count_bug(s, i + 1)
    }
}

proof fn expansion_split_is_load_bearing()
    ensures
        count_bug(glued(), 0) == count_bug(spaced(), 0),
        count_bug(glued(), 0) == 2,
{
    assert(count_bug(glued(), 0) == 2) by (compute);
    assert(count_bug(spaced(), 0) == 2) by (compute);
}

fn main() {}

} // verus!
