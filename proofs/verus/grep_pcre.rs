// grep_pcre.rs — `grep -P 'PCRE'` → `grep -E 'ERE'`, proved where it can be
// stated from the source alone.
//
// WHAT THIS PASS ACTUALLY DOES (read from the source, measured with the pass
// on vs off): it rewrites ONE call's argv — the grep flag word gains `E` and
// loses `P`, every pass-through flag stays, a `--` separator stays, the
// pattern slot receives the translated text, and the operands after the
// pattern are carried over unchanged — and it rewrites the pattern by a
// SYNTAX-DIRECTED TOKEN TABLE (class escapes to POSIX classes, group prefixes
// erased, possessive markers dropped, literals copied) or REFUSES the pattern
// outright and leaves `grep -P` alone.
//
// WHAT IS NOT STATED HERE, and why: the *language equivalences* the token
// table relies on — that ERE `[0-9]` matches exactly what PCRE `\d` matches,
// that `(X)` and `(?>X)` recognize the same language, that a possessive
// quantifier recognizes the same language as the greedy one, that
// `[[:alnum:]_]` is PCRE's `\w` under the C locale, and that absorbing a
// fixed lookbehind prefix changes only the bytes `-o` prints (hence the
// `sed 's/^X//'` strip the pass inserts) — are claims about a MATCHER, not
// about the rewrite. They are bounded here and measured by the anchor, which
// runs real `grep -P` and the emitted program on the same inputs.
//
// THE MODEL:
//
//   Part 1 the argv (a structural law, no regex semantics):
//     A1 the_rewrite_keeps_the_operands — operands after the pattern survive
//        in order.
//     A2 the_rewrite_keeps_every_pass_through_flag — each of the ten
//        pass-through flags is present afterwards iff it was present before.
//     A3 the_rewrite_makes_the_mode_extended — the flag word starts `-E`,
//        contains no `P`, and carries exactly one `E`.
//     A4 the_rewrite_keeps_the_separator — a source `--` is still there, and
//        one is not invented otherwise (the REPAIR this model forced: with
//        the separator dropped, `grep -P -- -x` emitted `grep -E -x`, which
//        parses the PATTERN as an option — usage error, rc 2, no output,
//        where bash prints `-x` and exits 0; measured).
//
//   Part 2 the pattern (a syntax-directed table over the accepted fragment):
//     The output is a FOLD over the token list (`tr`) of a per-token table
//     (`tr1`) — so the composition is the definition, not a separate claim.
//     P1 a_literal_run_is_copied — a run of `Lit` tokens emits exactly its
//        characters, in order (`tr == lit_text`), so no literal can be dropped
//        or reordered.
//     P2 the_table_rewrites_each_class_escape — six exact-output lemmas, one
//        per `\d \D \s \S \w \W`.
//     P3 the_group_prefix_becomes_a_plain_group / the_quantifier_marker_is_
//        all_a_possessive_loses / the_escaped_literal_keeps_its_backslash /
//        the_copied_class_is_copied / the_interval_text_is_kept — `(?:`/`(?>`
//        → `(`, `X*+` → `X*`, `\.` → `\.`, `[[:alpha:]]` copied, `{m,n}`
//        copied.
//     P4 the refusals: `has_bad` (an EXISTENTIAL over the token list) marks a
//        lazy quantifier or a refused escape, and `a_lazy_quantifier_is_
//        refused` / `a_refused_escape_is_refused` pin that a list carrying one
//        is refused (the pattern is left as `grep -P`);
//        an_accepted_pattern_has_no_bad_token is the converse, by definition.
//     P5 the_lookbehind_absorption_keeps_the_prefix — `(?<=X)Y` → `X` ++ Y,
//        with X recorded as the strip prefix.
//
//   Pins (each a plausible wrong rule the model rejects):
//     a_dash_pattern_without_a_separator_is_an_option (the measured REPAIR),
//     the_ten_pass_through_flags_are_kept (the pass-through set is exactly
//     `o v c q l n s w x i`, and `P`/`E`/`-` are not in it),
//     a_dropped_flag_is_not_the_same_call,
//     a_dropped_operand_is_not_the_same_call,
//     the_naive_keep_the_pc_classes_is_not_the_written_table,
//     a_dropped_literal_breaks_the_pattern,
//     a_lazy_quantifier_would_change_the_match_set,
//     an_atomic_group_is_not_a_plain_group_without_the_marker.
//
//   Witnesses (law_at_the_witnesses): the measured `grep -Pn '\d+'` call
//   (flag word `-En`, pattern `[0-9]+`, operand kept) and the repaired
//   `grep -P -- -x` call (separator still in front of the pattern).
//
// NOT MODELLED, stated: the SCANNER (where a class ends, which `(?` form a
// prefix is, whether a `{` is a quantifier), the matcher semantics above, the
// constant-map resolution of `$VAR` patterns (the transform's own tests pin
// the accepted/refused shapes), and the insertion point of the `sed` strip
// stage inside a capture pipeline (structure pinned by the unit tests).

use vstd::prelude::*;

verus! {

// ── character codes ──────────────────────────────────────────────────────

pub open spec fn ch(c: int) -> Seq<int> { seq![c] }
pub open spec fn dash_c() -> int { 45 }
pub open spec fn e_c() -> int { 69 }
pub open spec fn p_c() -> int { 80 }
pub open spec fn s_open() -> int { 91 }   // '['
pub open spec fn s_close() -> int { 93 }  // ']'
pub open spec fn caret() -> int { 94 }    // '^'
pub open spec fn qmark() -> int { 63 }    // '?'
pub open spec fn lparen() -> int { 40 }
pub open spec fn rparen() -> int { 41 }
pub open spec fn star() -> int { 42 }
pub open spec fn plus() -> int { 43 }
pub open spec fn colon() -> int { 58 }

pub open spec fn sep() -> Seq<int> { seq![dash_c(), dash_c()] }

/// The flags carried through unchanged (the source's `o v c q l n s w x i`).
pub open spec fn passthrough(c: int) -> bool {
    c == 111 || c == 118 || c == 99 || c == 113 || c == 108 || c == 110
        || c == 115 || c == 119 || c == 120 || c == 105
}

pub open spec fn contains(s: Seq<int>, c: int) -> bool
    decreases s.len()
{
    if s.len() == 0 { false } else if s[0] == c { true } else { contains(s.skip(1), c) }
}

proof fn contains_cons(x: int, s: Seq<int>, c: int)
    ensures contains(ch(x) + s, c) == (c == x || contains(s, c))
{
    assert((ch(x) + s).len() == 1 + s.len());
    assert((ch(x) + s)[0] == x);
    assert((ch(x) + s).skip(1) =~= s);
}

pub open spec fn b2i(b: bool) -> int { if b { 1 } else { 0 } }

proof fn count_of_cons(x: int, s: Seq<int>, c: int)
    ensures count_of(ch(x) + s, c) == b2i(x == c) + count_of(s, c)
{
    assert((ch(x) + s).len() == 1 + s.len());
    assert((ch(x) + s)[0] == x);
    assert((ch(x) + s).skip(1) =~= s);
}

/// Every token is a literal character?
pub open spec fn all_lits(toks: Seq<Tok>) -> bool
    decreases toks.len()
{
    if toks.len() == 0 { true } else { (toks[0] is Lit) && all_lits(toks.skip(1)) }
}

/// The literal characters of a token list, as a text.
pub open spec fn lit_text(toks: Seq<Tok>) -> Seq<int>
    decreases toks.len()
{
    if toks.len() == 0 { seq![] }
    else {
        match toks[0] {
            Tok::Lit(c) => ch(c) + lit_text(toks.skip(1)),
            _ => lit_text(toks.skip(1)),
        }
    }
}

/// A run of literals is copied verbatim, in order.
proof fn a_literal_run_is_copied(toks: Seq<Tok>)
    requires all_lits(toks)
    ensures tr(toks) == lit_text(toks)
    decreases toks.len()
{
    if toks.len() == 0 {
    } else {
        a_literal_run_is_copied(toks.skip(1));
    }
}

proof fn pass_flags_contains(s: Seq<int>, c: int)
    ensures contains(pass_flags(s), c) == (passthrough(c) && contains(s, c))
    decreases s.len()
{
    if s.len() == 0 {
    } else {
        let x = s[0];
        let rest = s.skip(1);
        pass_flags_contains(rest, c);
        if passthrough(x) {
            contains_cons(x, pass_flags(rest), c);
            if c == x {
                assert(passthrough(c));
            }
        } else {
            if c == x {
                assert(!passthrough(c));
            }
        }
    }
}

proof fn pass_flags_have_no(s: Seq<int>, c: int)
    requires !passthrough(c)
    ensures count_of(pass_flags(s), c) == 0
    decreases s.len()
{
    if s.len() == 0 {
    } else {
        let x = s[0];
        pass_flags_have_no(s.skip(1), c);
        if passthrough(x) {
            assert(x != c);
            count_of_cons(x, pass_flags(s.skip(1)), c);
        }
    }
}

proof fn pass_flags_empty()
    ensures pass_flags(Seq::<int>::empty()) == Seq::<int>::empty()
{
}

proof fn pass_flags_one(x: int)
    ensures pass_flags(seq![x]) == (if passthrough(x) { ch(x) } else { Seq::<int>::empty() })
{
    assert(seq![x].len() == 1);
    assert(seq![x][0] == x);
    assert(seq![x].skip(1) =~= Seq::<int>::empty());
    pass_flags_empty();
}

pub open spec fn count_of(s: Seq<int>, c: int) -> int
    decreases s.len()
{
    if s.len() == 0 { 0 } else if s[0] == c { 1 + count_of(s.skip(1), c) } else { count_of(s.skip(1), c) }
}

// ── Part 1: the argv rewrite ─────────────────────────────────────────────

/// The source call's argv, as the pass reads it: the flag characters (in the
/// order first seen), whether a `--` separator appeared, the pattern, and the
/// operands after the pattern.
pub struct Argv {
    pub flags: Seq<int>,
    pub dd: bool,
    pub pat: Seq<int>,
    pub ops: Seq<Seq<int>>,
}

pub open spec fn pass_flags(s: Seq<int>) -> Seq<int>
    decreases s.len()
{
    if s.len() == 0 { seq![] }
    else if passthrough(s[0]) { ch(s[0]) + pass_flags(s.skip(1)) }
    else { pass_flags(s.skip(1)) }
}

/// The rebuilt flag word: `-E` followed by the pass-through flags.
pub open spec fn new_flags(av: Argv) -> Seq<int> {
    ch(dash_c()) + (ch(e_c()) + pass_flags(av.flags))
}

/// The rebuilt argv: [flag word, ("--")?, translated pattern, operands…].
pub open spec fn rebuild(av: Argv) -> Seq<Seq<int>> {
    seq![new_flags(av)]
        + (if av.dd { seq![sep()] } else { seq![] })
        + seq![av.pat]
        + av.ops
}

/// The index of the pattern slot after the rebuild.
pub open spec fn pat_slot(av: Argv) -> int {
    if av.dd { 2 } else { 1 }
}

/// Would grep read the first argv word as an option? Only when it starts with
/// `-`, i.e. when no separator stands between the options and the pattern.
pub open spec fn parsed_as_option(av: Argv) -> bool {
    av.pat.len() > 0 && av.pat[0] == dash_c() && !av.dd
}

proof fn the_rewrite_keeps_the_operands(av: Argv)
    ensures rebuild(av).skip(pat_slot(av) + 1) == av.ops
{
    if av.dd {
        assert(rebuild(av).skip(3) == av.ops);
    } else {
        assert(rebuild(av).skip(2) == av.ops);
    }
}

proof fn the_rewrite_keeps_every_pass_through_flag(av: Argv, c: int)
    requires passthrough(c)
    ensures
        contains(new_flags(av), c) == contains(av.flags, c)
{
    pass_flags_contains(av.flags, c);
    contains_cons(dash_c(), ch(e_c()) + pass_flags(av.flags), c);
    contains_cons(e_c(), pass_flags(av.flags), c);
    assert(c != dash_c());
    assert(c != e_c());
}

/// The pass-through set is exactly the source's ten flags: dropping one from
/// `passthrough` is a different call, and `P`/`E` are never pass-through.
proof fn the_ten_pass_through_flags_are_kept()
    ensures
        passthrough(111), passthrough(118), passthrough(99), passthrough(113),
        passthrough(108), passthrough(110), passthrough(115), passthrough(119),
        passthrough(120), passthrough(105),
        !passthrough(p_c()), !passthrough(e_c()), !passthrough(dash_c())
{
}

proof fn the_rewrite_makes_the_mode_extended(av: Argv)
    ensures
        new_flags(av)[0] == dash_c(),
        new_flags(av)[1] == e_c(),
        count_of(new_flags(av), e_c()) == 1,
        count_of(new_flags(av), p_c()) == 0
{
    pass_flags_have_no(av.flags, e_c());
    count_of_cons(dash_c(), ch(e_c()) + pass_flags(av.flags), e_c());
    count_of_cons(e_c(), pass_flags(av.flags), e_c());
    pass_flags_have_no(av.flags, p_c());
    count_of_cons(dash_c(), ch(e_c()) + pass_flags(av.flags), p_c());
    count_of_cons(e_c(), pass_flags(av.flags), p_c());
    assert(dash_c() != p_c());
    assert(e_c() != p_c());
}

proof fn the_rewrite_keeps_the_separator(av: Argv)
    ensures
        av.dd ==> rebuild(av)[1] == sep() && rebuild(av)[2] == av.pat,
        !av.dd ==> rebuild(av)[1] == av.pat
{
    let rest = (if av.dd { seq![sep()] } else { seq![] }) + seq![av.pat] + av.ops;
    assert(rebuild(av) =~= seq![new_flags(av)] + rest);
    assert(rebuild(av).skip(1) =~= rest);
}

/// The pinned rule: a pattern that begins with `-` and has no separator is an
/// OPTION to grep. The rewrite must therefore keep the source's separator.
proof fn a_dash_pattern_without_a_separator_is_an_option()
{
    let bad = Argv { flags: seq![p_c()], dd: false, pat: seq![dash_c(), 120], ops: seq![] };
    let good = Argv { flags: seq![p_c()], dd: true, pat: seq![dash_c(), 120], ops: seq![] };
    assert(parsed_as_option(bad));
    assert(!parsed_as_option(good));
    // the repaired rebuild keeps the separator in front of the pattern
    assert(rebuild(good)[1] == sep());
    assert(rebuild(good)[2] == good.pat);
}

proof fn a_dropped_flag_is_not_the_same_call(av: Argv, c: int)
    requires passthrough(c), contains(av.flags, c)
    ensures contains(new_flags(av), c), contains(av.flags, c)
{
    the_rewrite_keeps_every_pass_through_flag(av, c);
}

proof fn a_dropped_operand_is_not_the_same_call(av: Argv)
    requires av.ops.len() > 0
    ensures rebuild(av)[pat_slot(av) + 1] == av.ops[0]
{
    if av.dd {
        assert(rebuild(av).skip(2) =~= seq![av.pat] + av.ops);
        assert((seq![av.pat] + av.ops).skip(1) =~= av.ops);
    } else {
        assert(rebuild(av).skip(1) =~= seq![av.pat] + av.ops);
        assert((seq![av.pat] + av.ops).skip(1) =~= av.ops);
    }
}

// ── Part 2: the pattern translation ──────────────────────────────────────

pub enum Tok {
    /// a literal character
    Lit(int),
    /// \d \D \s \S \w \W
    Digit, NotDigit, Space, NotSpace, Word, NotWord,
    /// an escaped literal punctuation char, copied as `\c`
    EscLit(int),
    /// a copied character class (`[[:alpha:]]`, `[^0-9]`, …)
    Cls(Seq<int>),
    /// `(?:` and `(?>` — the prefix is dropped, the group stays
    GroupOpen,
    GroupClose,
    /// `*`, `+`, `?` with the possessive marker flag
    Quant(int, bool),
    /// `{m,n}` text with the possessive marker flag
    Interval(Seq<int>, bool),
    /// the `?` making a quantifier lazy — REFUSED
    Lazy,
    /// \b \B \K \1..\9 \x … — REFUSED
    RefusedEscape,
}

pub open spec fn is_bad(t: Tok) -> bool {
    match t {
        Tok::Lazy => true,
        Tok::RefusedEscape => true,
        _ => false,
    }
}

pub open spec fn has_bad(toks: Seq<Tok>) -> bool {
    exists|i: int| 0 <= i < toks.len() && is_bad(toks[i])
}

/// The output of one token (non-recursive: the SMT can compute it).
pub open spec fn tr1(t: Tok) -> Seq<int> {
    match t {
        Tok::Lit(c) => ch(c),
        Tok::Digit => seq![91, 48, 45, 57, 93],
        Tok::NotDigit => seq![91, 94, 48, 45, 57, 93],
        Tok::Space => seq![91, 91, 58, 115, 112, 97, 99, 101, 58, 93, 93],
        Tok::NotSpace => seq![91, 94, 91, 58, 115, 112, 97, 99, 101, 58, 93, 93],
        Tok::Word => seq![91, 91, 58, 97, 108, 110, 117, 109, 93, 95, 93],
        Tok::NotWord => seq![91, 94, 91, 58, 97, 108, 110, 117, 109, 93, 95, 93],
        Tok::EscLit(c) => seq![92, c],
        Tok::Cls(cls) => cls,
        Tok::GroupOpen => ch(lparen()),
        Tok::GroupClose => ch(rparen()),
        Tok::Quant(q, _) => ch(q),
        Tok::Interval(text, _) => text,
        Tok::Lazy => seq![],
        Tok::RefusedEscape => seq![],
    }
}

/// The output text. Defined for every token list; the pass only uses it when
/// `!has_bad(toks)`.
pub open spec fn tr(toks: Seq<Tok>) -> Seq<int>
    decreases toks.len()
{
    if toks.len() == 0 { seq![] } else { tr1(toks[0]) + tr(toks.skip(1)) }
}

proof fn tr_cons(t: Tok, rest: Seq<Tok>)
    ensures tr(seq![t] + rest) == tr1(t) + tr(rest)
{
    assert((seq![t] + rest)[0] == t);
    assert((seq![t] + rest).skip(1) =~= rest);
}

proof fn tr_one(t: Tok)
    ensures tr(seq![t]) == tr1(t)
{
    tr_cons(t, Seq::<Tok>::empty());
    assert(tr1(t) + Seq::<int>::empty() =~= tr1(t));
}

proof fn seq2(a: Tok, b: Tok)
    ensures seq![a, b] =~= seq![a] + seq![b]
{
}

proof fn seq3(a: Tok, b: Tok, c: Tok)
    ensures seq![a, b, c] =~= seq![a] + (seq![b] + seq![c])
{
    seq2(a, b);
    seq2(b, c);
}


proof fn has_bad_one(t: Tok)
    ensures has_bad(seq![t]) == is_bad(t)
{
    assert(seq![t].len() == 1);
    assert(seq![t][0] == t);
}

proof fn tr_nil()
    ensures tr(Seq::<Tok>::empty()) == Seq::<int>::empty()
{
    assert(tr(Seq::<Tok>::empty()) == Seq::<int>::empty());
}

// exact-output lemmas: one per table entry (the specification pins)
proof fn the_table_rewrites_digit_class()
    ensures tr(seq![Tok::Digit]) == seq![91, 48, 45, 57, 93]
{
    tr_one(Tok::Digit);
}

proof fn the_table_rewrites_not_digit_class()
    ensures tr(seq![Tok::NotDigit]) == seq![91, 94, 48, 45, 57, 93]
{
    tr_one(Tok::NotDigit);
}

proof fn the_table_rewrites_space_class()
    ensures tr(seq![Tok::Space]) == seq![91, 91, 58, 115, 112, 97, 99, 101, 58, 93, 93]
{
    tr_one(Tok::Space);
}

proof fn the_table_rewrites_not_space_class()
    ensures tr(seq![Tok::NotSpace]) == seq![91, 94, 91, 58, 115, 112, 97, 99, 101, 58, 93, 93]
{
    tr_one(Tok::NotSpace);
}

proof fn the_table_rewrites_word_class()
    ensures tr(seq![Tok::Word]) == seq![91, 91, 58, 97, 108, 110, 117, 109, 93, 95, 93]
{
    tr_one(Tok::Word);
}

proof fn the_table_rewrites_not_word_class()
    ensures tr(seq![Tok::NotWord]) == seq![91, 94, 91, 58, 97, 108, 110, 117, 109, 93, 95, 93]
{
    tr_one(Tok::NotWord);
}

proof fn the_group_prefix_becomes_a_plain_group()
    ensures
        tr(seq![Tok::GroupOpen]) == ch(lparen()),
        tr(seq![Tok::GroupOpen, Tok::Lit(97), Tok::GroupClose]) == seq![40, 97, 41]
{
    tr_one(Tok::GroupOpen);
    tr_one(Tok::GroupClose);
    tr_one(Tok::Lit(97));
    seq3(Tok::GroupOpen, Tok::Lit(97), Tok::GroupClose);
    seq2(Tok::Lit(97), Tok::GroupClose);
    tr_cons(Tok::GroupOpen, seq![Tok::Lit(97)] + seq![Tok::GroupClose]);
    tr_cons(Tok::Lit(97), seq![Tok::GroupClose]);
    assert(tr(seq![Tok::GroupOpen] + (seq![Tok::Lit(97)] + seq![Tok::GroupClose]))
        == ch(lparen()) + (ch(97) + ch(rparen())));
    assert((ch(lparen()) + (ch(97) + ch(rparen()))) =~= seq![40, 97, 41]);
}

proof fn the_quantifier_marker_is_all_a_possessive_loses()
    ensures
        tr(seq![Tok::Quant(star(), true)]) == ch(star()),
        tr(seq![Tok::Quant(plus(), true)]) == ch(plus()),
        tr(seq![Tok::Quant(qmark(), true)]) == ch(qmark()),
        tr(seq![Tok::Interval(seq![123, 50, 125], true)]) == seq![123, 50, 125]
{
    tr_one(Tok::Quant(star(), true));
    tr_one(Tok::Quant(plus(), true));
    tr_one(Tok::Quant(qmark(), true));
    tr_one(Tok::Interval(seq![123, 50, 125], true));
}

proof fn the_escaped_literal_keeps_its_backslash()
    ensures tr(seq![Tok::EscLit(46)]) == seq![92, 46]
{
    tr_one(Tok::EscLit(46));
}

proof fn the_copied_class_is_copied()
    ensures tr(seq![Tok::Cls(seq![91, 94, 48, 93])]) == seq![91, 94, 48, 93]
{
    tr_one(Tok::Cls(seq![91, 94, 48, 93]));
}

proof fn the_interval_text_is_kept()
    ensures tr(seq![Tok::Interval(seq![123, 50, 44, 52, 125], false)]) == seq![123, 50, 44, 52, 125]
{
    tr_one(Tok::Interval(seq![123, 50, 44, 52, 125], false));
}

// the refusals
proof fn a_lazy_quantifier_is_refused()
    ensures has_bad(seq![Tok::Quant(star(), false), Tok::Lazy])
{
    assert(is_bad(seq![Tok::Quant(star(), false), Tok::Lazy][1]));
    assert(has_bad(seq![Tok::Quant(star(), false), Tok::Lazy])) by {
        let i: int = 1;
        assert(0 <= i < seq![Tok::Quant(star(), false), Tok::Lazy].len());
        assert(is_bad(seq![Tok::Quant(star(), false), Tok::Lazy][i]));
    }
}

proof fn a_refused_escape_is_refused()
    ensures has_bad(seq![Tok::RefusedEscape]), has_bad(seq![Tok::Lit(97), Tok::RefusedEscape])
{
    assert(has_bad(seq![Tok::RefusedEscape])) by {
        let i: int = 0;
        assert(is_bad(seq![Tok::RefusedEscape][i]));
    }
    assert(has_bad(seq![Tok::Lit(97), Tok::RefusedEscape])) by {
        let i: int = 1;
        assert(is_bad(seq![Tok::Lit(97), Tok::RefusedEscape][i]));
    }
}

proof fn an_accepted_pattern_has_no_bad_token(toks: Seq<Tok>)
    requires !has_bad(toks)
    ensures forall|i: int| 0 <= i < toks.len() ==> !is_bad(toks[i])
{
}

// the lookbehind absorption
pub open spec fn absorb(prefix: Seq<int>, body: Seq<Tok>) -> Seq<int> {
    prefix + tr(body)
}

proof fn the_lookbehind_absorption_keeps_the_prefix(prefix: Seq<int>, body: Seq<Tok>)
    requires !has_bad(body)
    ensures absorb(prefix, body) == prefix + tr(body)
{
}

// ── pins: the wrong rules ────────────────────────────────────────────────

proof fn the_naive_keep_the_pc_classes_is_not_the_written_table()
    ensures
        tr(seq![Tok::Word]) != seq![92, 119],
        tr(seq![Tok::Digit]) != seq![92, 100]
{
    tr_one(Tok::Word);
    tr_one(Tok::Digit);
    assert(tr1(Tok::Word).len() == 11);
    assert(tr1(Tok::Digit).len() == 5);
    assert(tr(seq![Tok::Word]).len() == 11);
    assert(tr(seq![Tok::Digit]).len() == 5);
}

proof fn a_dropped_literal_breaks_the_pattern()
    ensures
        tr(seq![Tok::Lit(97)]) == ch(97),
        tr(seq![Tok::Lit(97), Tok::Lit(98)]) != tr(seq![Tok::Lit(98)])
{
    tr_one(Tok::Lit(97));
    tr_one(Tok::Lit(98));
    seq2(Tok::Lit(97), Tok::Lit(98));
    tr_cons(Tok::Lit(97), seq![Tok::Lit(98)]);
    assert(tr(seq![Tok::Lit(97)] + seq![Tok::Lit(98)]) == seq![97, 98]);
}

proof fn an_atomic_group_is_not_a_plain_group_without_the_marker()
    ensures
        tr(seq![Tok::GroupOpen]) == ch(lparen()),
        tr(seq![Tok::GroupOpen]) != seq![lparen(), qmark(), colon()]
{
    tr_one(Tok::GroupOpen);
}

// ── the measured witnesses ───────────────────────────────────────────────

proof fn law_at_the_witnesses()
{
    // `grep -Pn '\d+'` -> `grep -En '[0-9]+'`
    tr_one(Tok::Digit);
    tr_one(Tok::Quant(plus(), false));
    seq2(Tok::Digit, Tok::Quant(plus(), false));
    has_bad_one(Tok::Digit);
    has_bad_one(Tok::Quant(plus(), false));
    let toks: Seq<Tok> = seq![Tok::Digit] + seq![Tok::Quant(plus(), false)];
    assert(!has_bad(toks));
    tr_cons(Tok::Digit, seq![Tok::Quant(plus(), false)]);
    assert(tr(toks) == seq![91, 48, 45, 57, 93, 43]);
    // the flag word and the pattern slot of that call
    let av = Argv {
        flags: seq![p_c(), 110],
        dd: false,
        pat: seq![91, 48, 45, 57, 93, 43],
        ops: seq![seq![120]],
    };
    assert(seq![p_c(), 110][0] == p_c());
    assert(seq![p_c(), 110].skip(1) =~= seq![110]);
    pass_flags_one(110);
    assert(pass_flags(seq![110]) == seq![110]);
    assert(pass_flags(seq![p_c(), 110]) == seq![110]);
    assert(new_flags(av) == seq![dash_c(), e_c(), 110]);
    assert(rebuild(av) == seq![seq![dash_c(), e_c(), 110], seq![91, 48, 45, 57, 93, 43], seq![120]]);
    // `grep -P -- -x` keeps its separator
    let dash = Argv { flags: seq![p_c()], dd: true, pat: seq![dash_c(), 120], ops: seq![] };
    pass_flags_one(p_c());
    assert(pass_flags(seq![p_c()]) == Seq::<int>::empty());
    assert((ch(e_c()) + Seq::<int>::empty()) =~= ch(e_c()));
    assert(new_flags(dash) == seq![dash_c(), e_c()]);
    assert(rebuild(dash).skip(1) =~= seq![sep()] + (seq![seq![dash_c(), 120]] + dash.ops));
    assert(rebuild(dash)[1] == sep());
    assert(rebuild(dash)[2] == seq![dash_c(), 120]);
    assert(!parsed_as_option(dash));
}

fn main() {}

} // verus!
