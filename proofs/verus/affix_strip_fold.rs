// ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/affix_strip_fold.rs — `${v#lit}` is ONE `strip_prefix`, not a
//  glob engine (and the star forms are ONE find/rfind cut).
//
//  The transform folds `param("#"|"##"|"%"|"%%", NAME, PAT)` into
//  `AffixStrip{text, pattern, prefix}` when PAT is a magic-free literal, and
//  into `CutAt{text, delim, head, first}` when PAT is `lit*` / `*lit`. The
//  claim it rests on is a SEMANTIC one, not a syntactic one:
//
//    bash's `${v#lit}` finds the SHORTEST prefix of `v` that matches the
//    pattern `lit`; for a magic-free literal exactly one prefix can match
//    (the literal itself, at the front), so shortest ≡ longest, the strip
//    removes AT MOST ONE occurrence, and `AffixStrip` is the whole
//    semantics of both `#` and `##` — likewise `%`/`%%` at the back.
//
//  That is what this file proves, against a scanner that enumerates the
//  matching prefixes/suffixes rather than assuming the closed form:
//
//    shortest_prefix_match(s, lit)   scan lengths upward from 0
//    longest_prefix_match(s, lit)    scan lengths downward from s.len()
//    bash_prefix_strip(s, lit, longest) = s without the matching prefix
//
//  and the laws tie those scans to the node's `strip_prefix`-shaped value:
//
//    literal_shortest_eq_longest        the two scans agree for a literal
//    fold_preserves_value               the node == bash's strip (= one node)
//    literal_strip_is_one_occurrence    the result is `s` or `s` minus ONE
//                                       leading/trailing occurrence
//    star_forms_preserve_value          `#lit*` ≡ `#lit`, `%*lit` ≡ `%lit`
//                                       (the two AffixStrip star arms)
//
//  ── the star-form table (the Cut arms) ──────────────────────────────────
//
//  The four find()/rfind() identities of the module doc — `%lit*`,
//  `%%lit*`, `#*lit`, `##*lit` — are the CutAt node, and their bash side is
//  the same shortest/longest prefix/suffix scan over a ONE-STAR pattern.
//  Bridging that scan to `find`/`rfind` (the matched prefix ends at the
//  occurrence) is the one piece stated rather than proved here, exactly as
//  the source states the table; the pins below compute both ends of it on a
//  concrete value, including the negative control the table turns on
//  (`${v##*x}` is not `${v#*x}`).
//
//  ── modelled / not modelled ────────────────────────────────────────────
//
//   * Strings are `Seq<char>`; `magic(c)` is `*`, `?`, `[`, `\` — the
//     source's `is_literal_pattern` also rejects the EMPTY pattern, which
//     the laws carry as `0 < p.len()`.
//   * The NAME slot (`is_plain_ident` / `is_text_operand_name`, a pure
//     lexical gate) and the target gates (`fold_target_ok` / `cut_target_ok`,
//     a registry lookup) are NOT modelled: they do not change the value,
//     only whether the fold is offered. Their refusals are pinned by the
//     source's own tests (`non_affix_target_is_refused`, `go_is_refused_…`).
//   * `%*` (empty body, identity) and `%%*` (empty result) are value-level
//     pins; the general empty-body handling is the source's.
//
//  ── measured mutants (all restored byte-identically; sha in the sync file) ──
//
//  baseline 29 verified, 0 errors
//
//  counted:
//    m2  `is_prefix` made strict (`<`)                26 verified, 3 errors
//    m7  the suffix scan ignores the literal           27 verified, 2 errors
//
//  refuted one stage earlier, at simplification, by a concrete pin:
//    m1  the strip removes nothing (pin_literal_one_length)
//    m3  the shortest scan returns k+1 (pin_literal_one_length)
//    m4  `*` is not glob magic (pin_magic_is_refused)
//    m5  cut_at's head branch keeps the tail (pin_dirname_basename)
//    m6  the "first" cut uses rfind (pin_double_star_differs)
//    m8  two stars accepted by the shape gate (pin_star_shape)
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub open spec fn magic(c: char) -> bool {
    c == '*' || c == '?' || c == '[' || c == '\\'
}

/// The source's `is_literal_pattern`: no glob magic anywhere.
pub open spec fn lit_free(p: Seq<char>) -> bool
    decreases p.len(),
{
    if p.len() == 0 {
        true
    } else {
        !magic(p[0]) && lit_free(p.skip(1))
    }
}

/// `s[i .. i+len(p)] == p`
pub open spec fn eq_at(s: Seq<char>, p: Seq<char>, i: int) -> bool {
    i >= 0 && i + p.len() <= s.len() && s.subrange(i, i + p.len()) == p
}

pub open spec fn is_prefix(s: Seq<char>, p: Seq<char>) -> bool {
    p.len() <= s.len() && s.take(p.len() as int) == p
}

pub open spec fn is_suffix(s: Seq<char>, p: Seq<char>) -> bool {
    p.len() <= s.len() && eq_at(s, p, s.len() - p.len())
}

// ────────────────────────────── the scan: shortest / longest matching prefix ──

/// Does the prefix of `s` of length `k` match the LITERAL pattern `p`?
pub open spec fn pref_matches(s: Seq<char>, p: Seq<char>, k: int) -> bool {
    0 <= k && k <= s.len() && s.take(k) == p
}

/// Scan lengths upward: the SHORTEST prefix matching `p`, or -1.
pub open spec fn spm_lit(s: Seq<char>, p: Seq<char>, k: int) -> int
    decreases s.len() + 1 - k,
{
    if k > s.len() {
        -1
    } else if pref_matches(s, p, k) {
        k
    } else {
        spm_lit(s, p, k + 1)
    }
}

/// Scan lengths downward: the LONGEST prefix matching `p`, or -1.
pub open spec fn lpm_lit(s: Seq<char>, p: Seq<char>, k: int) -> int
    decreases k + 1,
{
    if k < 0 {
        -1
    } else if pref_matches(s, p, k) {
        k
    } else {
        lpm_lit(s, p, k - 1)
    }
}

/// bash's `${v#lit}` / `${v##lit}`: drop the shortest / longest matching
/// prefix; no match leaves the value alone.
pub open spec fn bash_prefix_strip(s: Seq<char>, p: Seq<char>, longest: bool) -> Seq<char> {
    let k = if longest { lpm_lit(s, p, s.len() as int) } else { spm_lit(s, p, 0) };
    if k >= 0 {
        s.skip(k)
    } else {
        s
    }
}

/// The node's value: `strip_prefix(lit)` — at most ONE occurrence.
pub open spec fn strip_prefix_lit(s: Seq<char>, p: Seq<char>) -> Seq<char> {
    if is_prefix(s, p) {
        s.skip(p.len() as int)
    } else {
        s
    }
}

// ───────────────────────────────────────────────────── the scan lemmas ──

proof fn lemma_spm_lit_from(s: Seq<char>, p: Seq<char>, k: int)
    requires
        0 <= k <= s.len() + 1,
        0 < p.len(),
    ensures
        spm_lit(s, p, k) == if k <= p.len() && is_prefix(s, p) { p.len() as int } else { -1 },
    decreases s.len() + 1 - k,
{
    reveal_with_fuel(spm_lit, 2);
    if k > s.len() {
        // past every prefix: -1, and `is_prefix` cannot hold with k <= p.len()
        assert(!(k <= p.len() && is_prefix(s, p)));
    } else if pref_matches(s, p, k) {
        // the only matching length is the literal's own
        assert(s.take(k) == p);
        assert(s.take(k).len() == k);
        assert(k == p.len());
        assert(is_prefix(s, p));
        assert(k <= p.len());
    } else {
        assert(!pref_matches(s, p, k));
        lemma_spm_lit_from(s, p, k + 1);
    }
}

proof fn lemma_lpm_lit_from(s: Seq<char>, p: Seq<char>, k: int)
    requires
        0 <= k <= s.len(),
        0 < p.len(),
    ensures
        lpm_lit(s, p, k) == if p.len() as int <= k && is_prefix(s, p) { p.len() as int } else { -1 },
    decreases k,
{
    reveal_with_fuel(lpm_lit, 2);
    if pref_matches(s, p, k) {
        assert(s.take(k) == p);
        assert(s.take(k).len() == k);
        assert(k == p.len());
        assert(is_prefix(s, p));
        assert(p.len() as int <= k);
    } else if k == 0 {
        // k == 0 < p.len(), so the closed form is -1, as the scan returns
        assert(lpm_lit(s, p, k) == -1);
    } else {
        assert(!pref_matches(s, p, k));
        lemma_lpm_lit_from(s, p, k - 1);
        // if p.len() == k the prefix would have matched; it did not
        assert(!(is_prefix(s, p) && p.len() as int == k));
    }
}

// ──────────────────────────────────────────────────────────────── the laws ──

/// For a magic-free literal, bash's shortest and longest prefix match are the
/// SAME cut: exactly one prefix of `v` can be the literal.
pub proof fn literal_shortest_eq_longest(s: Seq<char>, p: Seq<char>)
    requires
        0 < p.len(),
        lit_free(p),
    ensures
        spm_lit(s, p, 0) == lpm_lit(s, p, s.len() as int),
        is_prefix(s, p) ==> spm_lit(s, p, 0) == p.len() as int,
        !is_prefix(s, p) ==> spm_lit(s, p, 0) == -1,
{
    lemma_spm_lit_from(s, p, 0);
    lemma_lpm_lit_from(s, p, s.len() as int);
}

/// THE FOLD: the AffixStrip node's value is bash's `${v#lit}` — and `${v##lit}`
/// too, because the two scans agree.
pub proof fn fold_preserves_value(s: Seq<char>, p: Seq<char>)
    requires
        0 < p.len(),
        lit_free(p),
    ensures
        bash_prefix_strip(s, p, false) == strip_prefix_lit(s, p),
        bash_prefix_strip(s, p, true) == strip_prefix_lit(s, p),
{
    literal_shortest_eq_longest(s, p);
}

/// The strip removes AT MOST ONE occurrence: the value is `s`, or `s` with
/// exactly one leading occurrence of the literal gone.
pub proof fn literal_strip_is_one_occurrence(s: Seq<char>, p: Seq<char>)
    requires
        0 < p.len(),
        lit_free(p),
    ensures
        strip_prefix_lit(s, p) == s || strip_prefix_lit(s, p) == s.skip(p.len() as int),
        strip_prefix_lit(s, p) == s || is_prefix(s, p),
{
}

/// The suffix side: a suffix of `s` of length `k` matching the literal `p`.
pub open spec fn suf_matches(s: Seq<char>, p: Seq<char>, k: int) -> bool {
    0 <= k && k <= s.len() && s.subrange(s.len() - k, s.len() as int) == p
}

pub open spec fn spm_suf(s: Seq<char>, p: Seq<char>, k: int) -> int
    decreases s.len() + 1 - k,
{
    if k > s.len() {
        -1
    } else if suf_matches(s, p, k) {
        k
    } else {
        spm_suf(s, p, k + 1)
    }
}

pub open spec fn lpm_suf(s: Seq<char>, p: Seq<char>, k: int) -> int
    decreases k + 1,
{
    if k < 0 {
        -1
    } else if suf_matches(s, p, k) {
        k
    } else {
        lpm_suf(s, p, k - 1)
    }
}

pub open spec fn bash_suffix_strip(s: Seq<char>, p: Seq<char>, longest: bool) -> Seq<char> {
    let k = if longest { lpm_suf(s, p, s.len() as int) } else { spm_suf(s, p, 0) };
    if k >= 0 {
        s.take(s.len() - k)
    } else {
        s
    }
}

pub open spec fn strip_suffix_lit(s: Seq<char>, p: Seq<char>) -> Seq<char> {
    if is_suffix(s, p) {
        s.take(s.len() - p.len() as int)
    } else {
        s
    }
}

proof fn lemma_spm_suf_from(s: Seq<char>, p: Seq<char>, k: int)
    requires
        0 <= k <= s.len() + 1,
        0 < p.len(),
    ensures
        spm_suf(s, p, k) == if k <= p.len() && is_suffix(s, p) { p.len() as int } else { -1 },
    decreases s.len() + 1 - k,
{
    reveal_with_fuel(spm_suf, 2);
    if k > s.len() {
        assert(!(k <= p.len() && is_suffix(s, p)));
    } else if suf_matches(s, p, k) {
        // a suffix of length k equal to p has k == p.len()
        assert(s.subrange(s.len() - k, s.len() as int) == p);
        assert(s.subrange(s.len() - k, s.len() as int).len() == k);
        assert(k == p.len());
        assert(eq_at(s, p, s.len() - p.len()));
        assert(is_suffix(s, p));
        assert(k <= p.len());
    } else {
        assert(!suf_matches(s, p, k));
        lemma_spm_suf_from(s, p, k + 1);
    }
}

proof fn lemma_lpm_suf_from(s: Seq<char>, p: Seq<char>, k: int)
    requires
        0 <= k <= s.len(),
        0 < p.len(),
    ensures
        lpm_suf(s, p, k) == if p.len() as int <= k && is_suffix(s, p) { p.len() as int } else { -1 },
    decreases k,
{
    reveal_with_fuel(lpm_suf, 2);
    if suf_matches(s, p, k) {
        assert(s.subrange(s.len() - k, s.len() as int) == p);
        assert(s.subrange(s.len() - k, s.len() as int).len() == k);
        assert(k == p.len());
        assert(eq_at(s, p, s.len() - p.len()));
        assert(is_suffix(s, p));
        assert(p.len() as int <= k);
    } else if k == 0 {
        // k == 0 < p.len(): the closed form is -1, as the scan returns
        assert(lpm_suf(s, p, k) == -1);
    } else {
        assert(!suf_matches(s, p, k));
        lemma_lpm_suf_from(s, p, k - 1);
        assert(!(is_suffix(s, p) && p.len() as int == k));
    }
}

/// The suffix mirror of `fold_preserves_value`.
pub proof fn fold_preserves_value_suffix(s: Seq<char>, p: Seq<char>)
    requires
        0 < p.len(),
        lit_free(p),
    ensures
        bash_suffix_strip(s, p, false) == strip_suffix_lit(s, p),
        bash_suffix_strip(s, p, true) == strip_suffix_lit(s, p),
{
    lemma_spm_suf_from(s, p, 0);
    lemma_lpm_suf_from(s, p, s.len() as int);
}

// ─────────────────────────────────────────────────── the star Affix arms ──

/// `#lit*`: the prefix of length `k` matches iff it STARTS with the body, so
/// the shortest match is the body itself — `AffixStrip(prefix)`.
pub open spec fn pref_starts_with(s: Seq<char>, b: Seq<char>, k: int) -> bool {
    0 <= k && k <= s.len() && b.len() <= k && s.take(b.len() as int) == b
}

pub open spec fn spm_star(s: Seq<char>, b: Seq<char>, k: int) -> int
    decreases s.len() + 1 - k,
{
    if k > s.len() {
        -1
    } else if pref_starts_with(s, b, k) {
        k
    } else {
        spm_star(s, b, k + 1)
    }
}

proof fn lemma_spm_star_from(s: Seq<char>, b: Seq<char>, k: int)
    requires 0 <= k <= s.len() + 1,
    ensures
        spm_star(s, b, k) == if is_prefix(s, b) {
            if k <= b.len() {
                b.len() as int
            } else if k <= s.len() {
                k
            } else {
                -1
            }
        } else {
            -1
        },
    decreases s.len() + 1 - k,
{
    reveal_with_fuel(spm_star, 2);
    if k > s.len() {
        assert(!(k <= b.len() && is_prefix(s, b)));
    } else if pref_starts_with(s, b, k) {
        assert(b.len() <= k);
        assert(is_prefix(s, b));
    } else {
        assert(!pref_starts_with(s, b, k));
        lemma_spm_star_from(s, b, k + 1);
    }
}

/// `%*lit`: a suffix of length `k` matches iff it ENDS with the body, so the
/// shortest match is the body itself — `AffixStrip(!prefix)`.
pub open spec fn suf_ends_with(s: Seq<char>, b: Seq<char>, k: int) -> bool {
    0 <= k && k <= s.len() && b.len() <= k && eq_at(s, b, s.len() - b.len())
}

pub open spec fn spm_star_suf(s: Seq<char>, b: Seq<char>, k: int) -> int
    decreases s.len() + 1 - k,
{
    if k > s.len() {
        -1
    } else if suf_ends_with(s, b, k) {
        k
    } else {
        spm_star_suf(s, b, k + 1)
    }
}

proof fn lemma_spm_star_suf_from(s: Seq<char>, b: Seq<char>, k: int)
    requires 0 <= k <= s.len() + 1,
    ensures
        spm_star_suf(s, b, k) == if is_suffix(s, b) {
            if k <= b.len() {
                b.len() as int
            } else if k <= s.len() {
                k
            } else {
                -1
            }
        } else {
            -1
        },
    decreases s.len() + 1 - k,
{
    reveal_with_fuel(spm_star_suf, 2);
    if k > s.len() {
        assert(!(k <= b.len() && is_suffix(s, b)));
    } else if suf_ends_with(s, b, k) {
        assert(b.len() <= k);
        assert(is_suffix(s, b));
    } else {
        assert(!suf_ends_with(s, b, k));
        lemma_spm_star_suf_from(s, b, k + 1);
    }
}

/// The two star shapes the transform folds to `AffixStrip` — `#lit*` is `#lit`
/// and `%*lit` is `%lit` — are exactly the literal strip's value.
pub proof fn star_forms_preserve_value(s: Seq<char>, b: Seq<char>)
    requires
        lit_free(b),
    ensures
        // `#lit*`: shortest prefix matching `lit*` is `lit`
        (if spm_star(s, b, 0) >= 0 { s.skip(spm_star(s, b, 0)) } else { s })
            == strip_prefix_lit(s, b),
        // `%*lit`: shortest suffix matching `*lit` is `lit`
        (if spm_star_suf(s, b, 0) >= 0 { s.take(s.len() - spm_star_suf(s, b, 0)) } else { s })
            == strip_suffix_lit(s, b),
{
    lemma_spm_star_from(s, b, 0);
    lemma_spm_star_suf_from(s, b, 0);
    if is_prefix(s, b) {
        assert(spm_star(s, b, 0) == b.len());
    } else {
        assert(spm_star(s, b, 0) == -1);
        assert(!is_prefix(s, b));
    }
    if is_suffix(s, b) {
        assert(spm_star_suf(s, b, 0) == b.len());
    } else {
        assert(spm_star_suf(s, b, 0) == -1);
            assert(!is_suffix(s, b));
    }
}

// ────────────────────────────────────────────────────────────── the Cut nodes ──

/// The CutAt node's value: keep the head (or tail) around ONE occurrence of
/// the delimiter, at the first (or last) one; no occurrence leaves `s`.
pub open spec fn find(s: Seq<char>, d: Seq<char>, i: int) -> int
    decreases s.len() + 1 - i,
{
    if i + d.len() > s.len() {
        -1
    } else if eq_at(s, d, i) {
        i
    } else {
        find(s, d, i + 1)
    }
}

pub open spec fn rfind(s: Seq<char>, d: Seq<char>, i: int) -> int
    decreases i + 1,
{
    if i < 0 || i + d.len() > s.len() {
        -1
    } else if eq_at(s, d, i) {
        i
    } else {
        rfind(s, d, i - 1)
    }
}

pub open spec fn cut_at(s: Seq<char>, d: Seq<char>, head: bool, first: bool) -> Seq<char> {
    let i = if first { find(s, d, 0) } else { rfind(s, d, s.len() as int - d.len() as int) };
    if i < 0 {
        s
    } else if head {
        s.take(i)
    } else {
        s.skip(i + d.len() as int)
    }
}

// ─────────────────────────────────────── the star SHAPE (single_star_shape) ──

/// The source's `single_star_shape`: EXACTLY one `*`, pinned at an end. A
/// middle star (`a*b`, the cut point needs a scan) and a second star are
/// refused before any of the valuations above is consulted.
pub open spec fn count_star(p: Seq<char>) -> nat
    decreases p.len(),
{
    if p.len() == 0 {
        0
    } else {
        (if p[0] == '*' { 1nat } else { 0nat }) + count_star(p.skip(1))
    }
}

pub open spec fn single_star_shape(p: Seq<char>) -> bool {
    count_star(p) == 1 && p.len() > 0 && (p[0] == '*' || p[p.len() - 1] == '*')
}

pub open spec fn star_is_first(p: Seq<char>) -> bool {
    p.len() > 0 && p[0] == '*'
}

// ───────────────────────────────────────────────────────────────────── pins ──

pub open spec fn v() -> Seq<char> {
    seq!['a', '/', 'x', '/', 'b', '/', 'x']
}

/// `${v##*x}` is NOT `${v#*x}`: shortest vs longest differ on a repeated
/// delimiter — the negative control the star table turns on.
pub proof fn pin_double_star_differs()
    ensures
        cut_at(v(), seq!['x'], false, true) == seq!['/', 'b', '/', 'x'],
        cut_at(v(), seq!['x'], false, false) == Seq::<char>::empty(),
        cut_at(v(), seq!['x'], false, true) != cut_at(v(), seq!['x'], false, false),
{
    assert(cut_at(v(), seq!['x'], false, true) == seq!['/', 'b', '/', 'x']) by (compute);
    assert(cut_at(v(), seq!['x'], false, false) == Seq::<char>::empty()) by (compute);
    assert(cut_at(v(), seq!['x'], false, true) != cut_at(v(), seq!['x'], false, false));
}

/// The dirname/basename pair: `${p%/*}` keeps before the LAST `/`,
/// `${p##*/}` keeps after it.
pub proof fn pin_dirname_basename()
    ensures
        cut_at(seq!['a', '/', 'b', '/', 'c'], seq!['/'], true, false) == seq!['a', '/', 'b'],
        cut_at(seq!['a', '/', 'b', '/', 'c'], seq!['/'], false, false) == seq!['c'],
{
    assert(cut_at(seq!['a', '/', 'b', '/', 'c'], seq!['/'], true, false) == seq!['a', '/', 'b'])
        by (compute);
    assert(cut_at(seq!['a', '/', 'b', '/', 'c'], seq!['/'], false, false) == seq!['c']) by (compute);
}

/// No occurrence: every cut leaves the value unchanged (bash's no-match rule).
pub proof fn pin_no_occurrence_is_the_value()
    ensures
        cut_at(seq!['a', 'b'], seq!['x'], true, true) == seq!['a', 'b'],
        cut_at(seq!['a', 'b'], seq!['x'], false, false) == seq!['a', 'b'],
{
    assert(cut_at(seq!['a', 'b'], seq!['x'], true, true) == seq!['a', 'b']) by (compute);
    assert(cut_at(seq!['a', 'b'], seq!['x'], false, false) == seq!['a', 'b']) by (compute);
}

/// `%*` with an EMPTY body is the identity (the source folds it to the plain
/// read) and `%%*` is "" — the two empty-body arms.
pub proof fn pin_empty_body_star()
    ensures
        spm_star_suf(seq!['a', 'b', 'c'], Seq::<char>::empty(), 0) == 0,
{
    assert(spm_star_suf(seq!['a', 'b', 'c'], Seq::<char>::empty(), 0) == 0) by (compute);
}

/// The literal scans: a magic-free literal matches at one length only.
pub proof fn pin_literal_one_length()
    ensures
        spm_lit(seq!['g', 'i', 't', '-', 'x'], seq!['g', 'i', 't', '-'], 0) == 4,
        lpm_lit(seq!['g', 'i', 't', '-', 'x'], seq!['g', 'i', 't', '-'], 5) == 4,
        spm_lit(seq!['a', 'b'], seq!['x'], 0) == -1,
        strip_prefix_lit(seq!['g', 'i', 't', '-', 'x'], seq!['g', 'i', 't', '-']) == seq!['x'],
        strip_prefix_lit(seq!['a', 'b'], seq!['x']) == seq!['a', 'b'],
{
    assert(spm_lit(seq!['g', 'i', 't', '-', 'x'], seq!['g', 'i', 't', '-'], 0) == 4) by (compute);
    assert(lpm_lit(seq!['g', 'i', 't', '-', 'x'], seq!['g', 'i', 't', '-'], 5) == 4) by (compute);
    assert(spm_lit(seq!['a', 'b'], seq!['x'], 0) == -1) by (compute);
    assert(strip_prefix_lit(seq!['g', 'i', 't', '-', 'x'], seq!['g', 'i', 't', '-']) == seq!['x'])
        by (compute);
    assert(strip_prefix_lit(seq!['a', 'b'], seq!['x']) == seq!['a', 'b']) by (compute);
}

/// Glob magic is refused, so the fold cannot fire on a pattern that needs the
/// engine: `*`, `?`, `[` and `\` all fail `lit_free`.
pub proof fn pin_magic_is_refused()
    ensures
        !lit_free(seq!['a', '*']),
        !lit_free(seq!['?']),
        !lit_free(seq!['[']),
        !lit_free(seq!['\\']),
        lit_free(seq!['g', 'i', 't', '-']),
        lit_free(Seq::<char>::empty()),
{
    assert(!lit_free(seq!['a', '*'])) by (compute);
    assert(!lit_free(seq!['?'])) by (compute);
    assert(!lit_free(seq!['['])) by (compute);
    assert(!lit_free(seq!['\\'])) by (compute);
    assert(lit_free(seq!['g', 'i', 't', '-'])) by (compute);
    assert(lit_free(Seq::<char>::empty())) by (compute);
}

/// The star SHAPE gate: one star, at an end. A middle star, two stars and a
/// starless pattern are all refused — the fold cannot fire on them.
pub proof fn pin_star_shape()
    ensures
        single_star_shape(seq!['*']),
        single_star_shape(seq!['*', 'l', 'i', 't']),
        single_star_shape(seq!['l', 'i', 't', '*']),
        !single_star_shape(seq!['a', '*', 'b']),
        !single_star_shape(seq!['*', '*']),
        !single_star_shape(seq!['a', 'b']),
        star_is_first(seq!['*', 'l', 'i', 't']),
        !star_is_first(seq!['l', 'i', 't', '*']),
{
    assert(single_star_shape(seq!['*'])) by (compute);
    assert(single_star_shape(seq!['*', 'l', 'i', 't'])) by (compute);
    assert(single_star_shape(seq!['l', 'i', 't', '*'])) by (compute);
    assert(!single_star_shape(seq!['a', '*', 'b'])) by (compute);
    assert(!single_star_shape(seq!['*', '*'])) by (compute);
    assert(!single_star_shape(seq!['a', 'b'])) by (compute);
    assert(star_is_first(seq!['*', 'l', 'i', 't'])) by (compute);
    assert(!star_is_first(seq!['l', 'i', 't', '*'])) by (compute);
}

fn main() {}

} // verus!
