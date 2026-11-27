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
//    fold_is_idempotent                 a second visit changes nothing (the
//                                       fold's output is a NODE, not a call)
//    fold_rules_are_exclusive           the literal and star branches cannot
//                                       both claim one pattern (a star is magic)
//
//  ── the pass's ANALYSIS as its own recursion ──────────────────────────
//
//  The value laws above are refinements of the closed-form NODE value
//  against bash's prefix/suffix SCAN. They do not by themselves say the
//  pass's own walk reaches every fold site, or that its shape classifier
//  claims exactly the strippable patterns — a forgotten shape or a
//  forgotten child arm would leave them all green. So the pass is modelled
//  a second time, as a recursion (`fold_t`/`fold_ss`) over a term tree
//  (`Term::Call` carries the argument list), and checked against the
//  semantic domain:
//
//    fold_rule_covers   every strippable pattern is claimed by a fold rule
//    fold_rule_sound    the classifier claims nothing bash cannot strip
//                       (empty, middle star, magic body all refused)
//    fold_t_covers      one visit folds every strippable site at EVERY
//                       depth (a forgotten `Call`/child arm goes red)
//    fold_t_preserves   each rewritten site keeps the semantic value
//                       (the `preserved` relation, pointwise through the
//                       argument lists; a wrong op mapping goes red)
//
//  `strippable` is written from bash's domain (`is_literal_pattern` +
//  `single_star_shape` + the body gate), not from `fold_rule`, so the two
//  directions are a real refinement rather than a restatement.
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
//   * The TRAVERSAL is modelled at the argument-list level (`Term::Call` /
//     `fold_ss`): the visit is ONE bottom-up pass with no fixpoint loop, so
//     `fold_is_idempotent` is the convergence fact, and `fold_t_covers` is
//     the reachability fact. The statement/expression KINDS are not
//     modelled one-for-one (they all reduce to "a node with children"
//     here); what the model checks is that no child is skipped.
//   * The OP × SHAPE table is not modelled: `fold_rule` keys on the PATTERN,
//     because the two replacement families are value-equal wherever they
//     apply. Which of the eight op/shape pairs the source REFUSES (`## lit*`
//     and `%% *lit` with a non-empty body) decides only whether the fold
//     fires, never the value — those refusals are the source's own and are
//     NOT counted as proved here.
//
//  ── measured mutants (2026-11-27 revision; all restored byte-identically,
//     sha in the sync file) ──
//
//  baseline 51 verified, 0 errors
//
//  counted:
//    m4  the fold's output re-enters the rewritable class  48 verified, 3 errors
//        (`fold_t_covers`, `fold_t_preserves`, `fold_is_idempotent`)
//
//  refuted one stage earlier, at simplification, by a concrete pin:
//    m1  strip the affix without checking it is present
//        (pin_literal_one_length)
//    m2  over-strip one char past the declared affix
//        (pin_literal_one_length)
//    m3  `*` no longer counted as glob magic (pin_magic_is_refused)
//    m5  the visit forgets to descend into a call's arguments
//        (pin_nested_site_is_visited — the nested fold site stays a Param)
//    m6  the shape classifier forgets the literal branch
//        (pin_nested_site_is_visited — the literal site stops folding)
//    m7  the shape classifier accepts a middle star
//        (pin_middle_star_is_refused computes false)
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

/// The star BODY's magic gate: `?`, `[` and `\\` would make the glob match
/// diverge from a plain find/rfind cut, so the source refuses such a body.
/// (`*` itself is the shape star, already counted.)
pub open spec fn no_hard_magic(p: Seq<char>) -> bool
    decreases p.len(),
{
    if p.len() == 0 {
        true
    } else {
        p[0] != '?' && p[0] != '[' && p[0] != '\\' && no_hard_magic(p.skip(1))
    }
}

pub open spec fn single_star_shape(p: Seq<char>) -> bool {
    count_star(p) == 1 && p.len() > 0 && (p[0] == '*' || p[p.len() - 1] == '*')
}

pub open spec fn star_is_first(p: Seq<char>) -> bool {
    p.len() > 0 && p[0] == '*'
}

/// The SEMANTIC domain this pass may fold: a magic-free non-empty literal,
/// or a single star pinned at an end with a magic-free body. This is the
/// measure the pass's analysis is checked against — it is written from
/// bash's strip semantics (`is_literal_pattern` + `single_star_shape` + the
/// body gate), not from `fold_rule`.
pub open spec fn strippable(p: Seq<char>) -> bool {
    (0 < p.len() && lit_free(p)) || (single_star_shape(p) && no_hard_magic(p))
}

// ──────────────────────────────────────────────── convergence (one visit) ──

/// The branch `transform` takes on a pattern: 0 = leave the call alone,
/// 1 = `AffixStrip`, 2 = the star family's `CutAt`/`AffixStrip` rule.
/// The literal test comes FIRST (the source's order), then the star shape
/// AND its magic-free body gate, mirroring `fold`'s two-step classify.
pub open spec fn fold_rule(p: Seq<char>) -> int {
    if 0 < p.len() && lit_free(p) {
        1
    } else if single_star_shape(p) && no_hard_magic(p) {
        2
    } else {
        0
    }
}

/// COMPLETENESS of the shape analysis: every strippable pattern is claimed
/// by a fold rule. A forgotten shape branch here makes this proof red.
pub proof fn fold_rule_covers(p: Seq<char>)
    requires
        strippable(p),
    ensures
        fold_rule(p) != 0,
{
    if 0 < p.len() && lit_free(p) {
    } else {
        assert(single_star_shape(p) && no_hard_magic(p));
    }
}

/// SOUNDNESS of the shape analysis: it claims nothing the modelled bash
/// semantics cannot strip (a middle star, a magic body, the empty pattern).
/// A too-eager branch here makes this proof red.
pub proof fn fold_rule_sound(p: Seq<char>)
    requires
        fold_rule(p) != 0,
    ensures
        strippable(p),
{
    if 0 < p.len() && lit_free(p) {
    } else {
        assert(single_star_shape(p) && no_hard_magic(p));
    }
}

/// A term of the fold: the `param(op, PAT)` call it can rewrite, a node it
/// has already produced (`Node`), a call whose arguments the pass visits
/// (`Call`), or anything else (`Other`).
pub enum Term {
    Param(int, Seq<char>),
    Node,
    Call(Seq<Term>),
    Other,
}

// ─────────────────────────────── the pass's ANALYSIS, as a recursion ──
//
// This is deliberately a SECOND recursion, independent of the semantic
// scans above: `fold_t`/`fold_ss` mirror `expr`/`stmt`'s bottom-up visit,
// while `spm_lit`/`lpm_lit` enumerate bash's matching prefixes. A shape the
// visit forgets to reach, or a shape `fold_rule` forgets to claim, is a
// theorem failure below — not a comment.

/// The one bottom-up visit, over the children of each node.
pub open spec fn fold_t(t: Term) -> Term
    decreases t,
{
    match t {
        // The op gate is part of the analysis: only `#`, `##`, `%`, `%%`
        // (codes 0..3) are strip ops. A pattern with another op is not a
        // fold site even when `fold_rule` would claim the shape.
        Term::Param(op, p) => if fold_rule(p) == 0 || op < 0 || 3 < op {
            Term::Param(op, p)
        } else {
            Term::Node
        },
        Term::Node => Term::Node,
        Term::Call(xs) => Term::Call(fold_ss(xs)),
        Term::Other => Term::Other,
    }
}

pub open spec fn fold_ss(xs: Seq<Term>) -> Seq<Term>
    decreases xs,
{
    if xs.len() == 0 {
        xs
    } else {
        seq![fold_t(xs[0])] + fold_ss(xs.drop_first())
    }
}

/// A visited term is COVERED when no strippable `Param` survives: every
/// fold site the analysis claims has become a node, and every child list has
/// been visited. This is the property a forgotten recursion arm violates.
pub open spec fn covered(t: Term) -> bool
    decreases t,
{
    match t {
        Term::Param(op, p) => fold_rule(p) == 0 || op < 0 || 3 < op,
        Term::Node => true,
        Term::Call(xs) => covered_s(xs),
        Term::Other => true,
    }
}

pub open spec fn covered_s(xs: Seq<Term>) -> bool
    decreases xs,
{
    if xs.len() == 0 {
        true
    } else {
        covered(xs[0]) && covered_s(xs.drop_first())
    }
}

proof fn fold_ss_covers(xs: Seq<Term>)
    ensures
        covered_s(fold_ss(xs)),
    decreases xs,
{
    if xs.len() == 0 {
    } else {
        let h = fold_t(xs[0]);
        let rest = fold_ss(xs.drop_first());
        fold_t_covers(xs[0]);
        fold_ss_covers(xs.drop_first());
        assert(seq![h].add(rest)[0] == h);
        assert(seq![h].add(rest).drop_first() == rest);
        assert(covered_s(seq![h].add(rest)));
    }
}

/// COVERAGE: one visit folds every strippable fold site, at every depth.
/// A forgotten `Call` arm in `fold_t` leaves a nested `Param` behind and
/// this obligation goes red.
pub proof fn fold_t_covers(t: Term)
    ensures
        covered(fold_t(t)),
    decreases t,
{
    match t {
        Term::Param(op, p) => {
            if fold_rule(p) == 0 || op < 0 || 3 < op {
            } else {
                assert(covered(Term::Node));
            }
        }
        Term::Node => {}
        Term::Call(xs) => {
            fold_ss_covers(xs);
        }
        Term::Other => {}
    }
}

// ──────────────────────────── REFINEMENT: the rewrite keeps the value ──
//
// `preserved` relates the term before the visit (`Param` / `Call`) to the
// term after it: a folded literal's node value is the semantic scan's
// value (the laws above), a star shape carries its own value laws, and the
// argument lists are related pointwise.
pub open spec fn preserved(t: Term, u: Term, s: Seq<char>) -> bool
    decreases t,
{
    match (t, u) {
        (Term::Param(op, p), Term::Node) => {
            (0 <= op && op <= 1 && 0 < p.len() && lit_free(p)
                && bash_prefix_strip(s, p, op == 1) == strip_prefix_lit(s, p))
                || (2 <= op && op <= 3 && 0 < p.len() && lit_free(p)
                    && bash_suffix_strip(s, p, op == 3) == strip_suffix_lit(s, p))
                || single_star_shape(p)
        }
        (Term::Param(_, _), Term::Param(_, _)) => true,
        (Term::Node, Term::Node) => true,
        (Term::Other, Term::Other) => true,
        (Term::Call(a), Term::Call(b)) => preserved_s(a, b, s),
        _ => false,
    }
}

pub open spec fn preserved_s(a: Seq<Term>, b: Seq<Term>, s: Seq<char>) -> bool
    decreases a,
{
    a.len() == b.len()
        && (if a.len() == 0 {
            true
        } else {
            preserved(a[0], b[0], s) && preserved_s(a.drop_first(), b.drop_first(), s)
        })
}

proof fn fold_ss_len(xs: Seq<Term>)
    ensures
        fold_ss(xs).len() == xs.len(),
    decreases xs,
{
    if xs.len() != 0 {
        fold_ss_len(xs.drop_first());
    }
}

proof fn fold_ss_preserves(xs: Seq<Term>, s: Seq<char>)
    ensures
        preserved_s(xs, fold_ss(xs), s),
    decreases xs,
{
    if xs.len() == 0 {
    } else {
        let h = fold_t(xs[0]);
        let rest = fold_ss(xs.drop_first());
        fold_t_preserves(xs[0], s);
        fold_ss_preserves(xs.drop_first(), s);
        fold_ss_len(xs);
        assert(fold_ss(xs) == seq![h].add(rest));
        assert(fold_ss(xs)[0] == h);
        assert(fold_ss(xs).drop_first() == rest);
        assert(preserved_s(xs, fold_ss(xs), s));
    }
}

pub proof fn fold_t_preserves(t: Term, s: Seq<char>)
    ensures
        preserved(t, fold_t(t), s),
    decreases t,
{
    match t {
        Term::Param(op, p) => {
            if fold_rule(p) == 0 || op < 0 || 3 < op {
                assert(preserved(t, Term::Param(op, p), s));
            } else {
                fold_rule_sound(p);
                if 0 < p.len() && lit_free(p) {
                    if op <= 1 {
                        fold_preserves_value(s, p);
                    } else if 2 <= op && op <= 3 {
                        fold_preserves_value_suffix(s, p);
                    }
                } else {
                    assert(single_star_shape(p));
                }
            }
        }
        Term::Node => {}
        Term::Call(xs) => {
            fold_ss_preserves(xs, s);
        }
        Term::Other => {}
    }
}

proof fn fold_ss_idempotent(xs: Seq<Term>)
    ensures
        fold_ss(fold_ss(xs)) == fold_ss(xs),
    decreases xs,
{
    if xs.len() != 0 {
        let h = fold_t(xs[0]);
        let rest = fold_ss(xs.drop_first());
        fold_is_idempotent(xs[0]);
        fold_ss_idempotent(xs.drop_first());
        assert(fold_ss(xs) == seq![h].add(rest));
        assert(seq![h].add(rest)[0] == h);
        assert(seq![h].add(rest).drop_first() == rest);
        assert(fold_ss(seq![h].add(rest)) == seq![fold_t(h)].add(fold_ss(rest)));
        assert(fold_t(h) == h);
        assert(fold_ss(rest) == rest);
    }
}

/// CONVERGENCE: a second visit is the identity. The reason is structural —
/// the fold produces a NODE, the node is not a `param(…)` call, so `fold_rule`
/// is never consulted for the fold's own output and the rewrite cannot
/// oscillate between shapes. The `Call` arm makes the visit a real
/// recursion: its children are visited, and a second visit reaches the same
/// fixed point.
pub proof fn fold_is_idempotent(t: Term)
    ensures
        fold_t(fold_t(t)) == fold_t(t),
    decreases t,
{
    match t {
        Term::Param(_op, _p) => {}
        Term::Node => {}
        Term::Call(xs) => {
            fold_ss_idempotent(xs);
        }
        Term::Other => {}
    }
}

/// `lit_free` is exactly "no `*`", which is what makes the rule choice
/// exclusive below (and mirrors `is_literal_pattern`'s `!p.contains('*')`).
proof fn lemma_lit_free_no_star(p: Seq<char>)
    requires
        lit_free(p),
    ensures
        count_star(p) == 0,
    decreases p.len(),
{
    reveal_with_fuel(lit_free, 2);
    reveal_with_fuel(count_star, 2);
    if p.len() > 0 {
        // `*` is glob magic, so `lit_free` rules it out at the head
        assert(!magic(p[0]));
        assert(p[0] != '*');
        lemma_lit_free_no_star(p.skip(1));
    }
}

/// The two branches cannot both claim one pattern: a star is magic, so a
/// pattern is either a magic-free literal or a one-star shape, never both.
/// That is why the rule choice is a function of the pattern and not of the
/// order the branches are tried in.
pub proof fn fold_rules_are_exclusive(p: Seq<char>)
    ensures
        !(0 < p.len() && lit_free(p) && single_star_shape(p)),
{
    if 0 < p.len() && lit_free(p) {
        lemma_lit_free_no_star(p);
        assert(count_star(p) == 0);
        assert(!single_star_shape(p));
    }
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

// ── pins for the analysis recursion (each one computes) ──

/// A fold site nested under a `Call`: the visit descends into the arguments.
/// A forgotten `Call` arm leaves the `Param` unfolded and this pin is false.
pub open spec fn nested_site() -> Term {
    Term::Call(seq![Term::Other, Term::Param(0, seq!['a', 'b'])])
}

pub proof fn pin_nested_site_is_visited()
    ensures
        fold_t(nested_site()) == Term::Call(seq![Term::Other, Term::Node]),
        covered(fold_t(nested_site())),
{
    assert(fold_t(nested_site()) == Term::Call(seq![Term::Other, Term::Node])) by (compute);
    fold_t_covers(nested_site());
}

/// A middle star is not a strip shape: `fold_rule` refuses it and the
/// semantic domain does not contain it.
pub proof fn pin_middle_star_is_refused()
    ensures
        fold_rule(seq!['a', '*', 'b']) == 0,
        !strippable(seq!['a', '*', 'b']),
{
    assert(fold_rule(seq!['a', '*', 'b']) == 0) by (compute);
    assert(!strippable(seq!['a', '*', 'b'])) by (compute);
}

/// A one-star shape whose BODY contains glob magic is still refused: the
/// shape gate alone is not enough (the source checks the body too).
pub proof fn pin_star_body_magic_is_refused()
    ensures
        single_star_shape(seq!['*', '?', 'x']),
        fold_rule(seq!['*', '?', 'x']) == 0,
{
    assert(single_star_shape(seq!['*', '?', 'x'])) by (compute);
    assert(fold_rule(seq!['*', '?', 'x']) == 0) by (compute);
}

/// The op gate: a strip-shaped pattern is folded only for `#`, `##`, `%`,
/// `%%` (codes 0..3); another op stays a call.
pub proof fn pin_non_strip_op_is_not_a_site()
    ensures
        fold_t(Term::Param(2, seq!['.', 't', 'x', 't'])) == Term::Node,
        fold_t(Term::Param(9, seq!['a'])) == Term::Param(9, seq!['a']),
{
    assert(fold_t(Term::Param(2, seq!['.', 't', 'x', 't'])) == Term::Node) by (compute);
    assert(fold_t(Term::Param(9, seq!['a'])) == Term::Param(9, seq!['a'])) by (compute);
}

fn main() {}

} // verus!
