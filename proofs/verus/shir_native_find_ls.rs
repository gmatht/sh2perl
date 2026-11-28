// F3 — `find` / `head`: what the emitted Perl means.
//
// The folds in `src/transforms/shir_native_stmt.rs`:
//   native_find_stmt (6286)   `find DIR -name PAT [| head -N]`
//   native_ls_stmt   (4625)   `ls -1 P1 .. P8 || echo LIT`
//
// They do not emit `find`. They emit a recursive Perl walk whose matcher is
//     `elsif (substr($__e, -{N}) eq {Q}) { push @__out, $__p }`
// with N = `PAT.len()` after the leading `*` is stripped, whose word-list
// parser is the `while i < w0.len()` loop, and whose `head -N` collects the
// whole traversal and splices.
//
// Each law is stated against a SEMANTIC side that is not the pass's own
// predicate: an indexwise suffix test, a grammar over the token list, and a
// pointwise prefix. The pass's shapes (a `substr`-window recursion, a
// mutable-accumulator loop, a drop-last recursion) are written separately, so a
// forgotten case or an over-broad accept goes red.
//
// Principal results:
//   perl_substr_neg_is_the_suffix_glob    the matcher is `*.sf`, exactly, for
//                                         every name — including names shorter
//                                         than the pattern, where Perl clamps
//                                         the offset to 0
//   find_accepts_iff_shape                the fold's accept set is the declared
//                                         grammar: one dir word, `-name` pairs
//   find_accepts_a_greedy_multi_pattern   d-F3-1: `-name a -name b` is accepted
//                                         and matched by b ALONE, where `find`
//                                         requires both
//   head_take_is_the_first_n              collect-then-splice == the first N
//
// Supporting lemmas and who consumes them:
//   lemma_last_n_len / lemma_last_n_index  → perl_substr_neg_is_the_suffix_glob
//   lemma_take_first_step                 → head_take_is_the_first_n
//
// `ls` is NOT proved here: its listing is a permutation of a sorted readdir, so
// its law is the sort/permutation law plus the `|| echo` status wiring — both
// proved in `shir_native_pipelines.rs` (F4 §5 and §6, d-F4-3). Out of model
// here: mtime/permission predicates (the fold refuses them), readdir order
// beyond the given order, and `find`'s own symlink guard.

use vstd::prelude::*;

verus! {

// ─────────────────────────────────────────── §1  the `-name` glob matcher ──

/// Perl's `substr($e, -n)`: the last `n` characters and — the case a
/// transcription loses — the WHOLE string when `n` exceeds its length.
pub open spec fn last_n(e: Seq<int>, n: int) -> Seq<int>
    decreases n,
{
    if n <= 0 || e.len() == 0 {
        Seq::empty()
    } else {
        last_n(e.drop_last(), n - 1).push(e.last())
    }
}

/// **The semantic side**: `sf` sits at the end of `e`, read off by index. This
/// is what `-name '*.sf'` means, and it knows nothing about `substr`.
pub open spec fn suffix_at(sf: Seq<int>, e: Seq<int>) -> bool {
    sf.len() <= e.len() && forall|i: int| 0 <= i < sf.len() ==> #[trigger] e[e.len() - sf.len()
        + i] == sf[i]
}

pub proof fn lemma_last_n_len(e: Seq<int>, n: int)
    ensures
        last_n(e, n).len() == (if n <= 0 {
            0
        } else if n <= e.len() as int {
            n
        } else {
            e.len() as int
        }),
    decreases n,
{
    if n <= 0 || e.len() == 0 {
    } else {
        lemma_last_n_len(e.drop_last(), n - 1);
        assert(e.drop_last().len() == e.len() - 1);
    }
}

/// Element `k` of the window is element `len - n + k` of the string: the point
/// of the negative offset, and where an off-by-one would hide.
pub proof fn lemma_last_n_index(e: Seq<int>, n: int, k: int)
    requires
        n <= e.len() as int,
        0 <= k < n,
    ensures
        last_n(e, n)[k] == e[e.len() - n + k],
    decreases n,
{
    if n <= 0 || e.len() == 0 {
    } else {
        lemma_last_n_len(e.drop_last(), n - 1);
        assert(e.drop_last().len() == e.len() - 1);
        if k == n - 1 {
            // the appended last element lands at index n-1
        } else {
            lemma_last_n_index(e.drop_last(), n - 1, k);
        }
    }
}

/// **The matcher law.** `substr($e, -len(sf)) eq sf` is the suffix test, for
/// every name — including names shorter than the pattern, where Perl clamps the
/// offset to 0 and compares the whole name (so the emitted matcher never
/// selects a name it should not).
pub proof fn perl_substr_neg_is_the_suffix_glob(sf: Seq<int>, e: Seq<int>)
    ensures
        last_n(e, sf.len() as int) == sf <==> suffix_at(sf, e),
{
    lemma_last_n_len(e, sf.len() as int);
    if sf.len() as int <= e.len() {
        if last_n(e, sf.len() as int) == sf {
            assert forall|i: int| 0 <= i < sf.len() implies (#[trigger] e[e.len() - sf.len()
                + i] == sf[i]) by {
                lemma_last_n_index(e, sf.len() as int, i);
            }
        }
        if suffix_at(sf, e) {
            assert forall|i: int| 0 <= i < sf.len() as int implies (#[trigger] last_n(
                e,
                sf.len() as int,
            )[i] == sf[i]) by {
                lemma_last_n_index(e, sf.len() as int, i);
            }
            assert(last_n(e, sf.len() as int) =~= sf);
        }
    } else {
        // over-long pattern: the fold compares the whole name, so acceptance
        // would need the name to equal the longer pattern
        assert(last_n(e, sf.len() as int).len() == e.len());
    }
}

// ──────────────────────────────────────────── §2  the word-list grammar ────

/// A `find` word as the fold classifies it: a literal, `-name`, or another
/// `-flag` (which it refuses).
pub enum Tok {
    Word(int),
    NameFlag,
    OtherFlag,
}

pub open spec fn is_word(t: Tok) -> bool {
    match t {
        Tok::Word(_) => true,
        _ => false,
    }
}

pub open spec fn is_name(t: Tok) -> bool {
    match t {
        Tok::NameFlag => true,
        _ => false,
    }
}

pub open spec fn is_other(t: Tok) -> bool {
    match t {
        Tok::OtherFlag => true,
        _ => false,
    }
}

pub open spec fn tok_word(t: Tok) -> int {
    match t {
        Tok::Word(w) => w,
        _ => -1,
    }
}

/// **The semantic grammar**: read the token list as ITEMS — one bare literal
/// (the directory) plus one or more `-name <pattern>` pairs, in any order. This
/// reading never fails fast: it counts what it saw and decides at the end, which
/// is what makes it a different function from the fold's `(dir, pat)` loop.
pub open spec fn items(ws: Seq<Tok>, i: int, dirs: int, pats: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() as int {
        dirs == 1 && pats >= 1
    } else if is_name(ws[i]) {
        i + 1 < ws.len() as int && is_word(ws[i + 1]) && items(ws, i + 2, dirs, pats + 1)
    } else if is_word(ws[i]) {
        items(ws, i + 1, dirs + 1, pats)
    } else {
        false
    }
}

/// `items` only looks at whether a pattern has been seen, never at the count.
pub proof fn lemma_items_pats_pos(ws: Seq<Tok>, i: int, dirs: int, p1: int, p2: int)
    requires
        0 <= i <= ws.len(),
        p1 >= 1,
        p2 >= 1,
    ensures
        items(ws, i, dirs, p1) <==> items(ws, i, dirs, p2),
    decreases ws.len() - i,
{
    if i >= ws.len() as int {
    } else if is_name(ws[i]) {
        if i + 1 < ws.len() as int && is_word(ws[i + 1]) {
            lemma_items_pats_pos(ws, i + 2, dirs, p1 + 1, p2 + 1);
        }
    } else if is_word(ws[i]) {
        lemma_items_pats_pos(ws, i + 1, dirs + 1, p1, p2);
    } else {
    }
}

/// Two directory words can never be the grammar, however the tail reads.
pub proof fn lemma_items_two_dirs(ws: Seq<Tok>, i: int, dirs: int, pats: int)
    requires
        0 <= i <= ws.len(),
        dirs >= 2,
    ensures
        !items(ws, i, dirs, pats),
    decreases ws.len() - i,
{
    if i >= ws.len() as int {
    } else if is_name(ws[i]) {
        if i + 1 < ws.len() as int && is_word(ws[i + 1]) {
            lemma_items_two_dirs(ws, i + 2, dirs, pats + 1);
        }
    } else if is_word(ws[i]) {
        lemma_items_two_dirs(ws, i + 1, dirs + 1, pats);
    } else {
    }
}

pub open spec fn grammar(ws: Seq<Tok>) -> bool {
    items(ws, 0, 0, 0)
}

/// The fold's parser: the `while i < w0.len()` loop with its `(dir, pat)`
/// accumulator, transcribed as a recursion — including the OVERWRITE of `pat`
/// on a second `-name`, which is d-F3-1 below.
pub open spec fn parse(
    ws: Seq<Tok>,
    i: int,
    dir: Option<int>,
    pat: Option<int>,
) -> Option<(int, int)>
    decreases ws.len() - i,
{
    if i >= ws.len() as int {
        match (dir, pat) {
            (Some(d), Some(p)) => Some((d, p)),
            _ => None,
        }
    } else if is_name(ws[i]) {
        if i + 1 >= ws.len() as int || !is_word(ws[i + 1]) {
            None
        } else {
            parse(ws, i + 2, dir, Some(tok_word(ws[i + 1])))
        }
    } else if is_other(ws[i]) {
        None
    } else if dir.is_some() {
        None
    } else {
        parse(ws, i + 1, Some(tok_word(ws[i])), pat)
    }
}

pub open spec fn parse_ok(ws: Seq<Tok>, i: int, dir: Option<int>, pat: Option<int>) -> bool {
    match parse(ws, i, dir, pat) {
        Some(_) => true,
        None => false,
    }
}

pub open spec fn accepts(ws: Seq<Tok>) -> bool {
    parse_ok(ws, 0, None, None)
}

/// **The accept-set law.** The fold accepts exactly the declared grammar: it
/// refuses a second directory word, a stray flag and a `-name` with no pattern,
/// and it accepts both `DIR -name P` and `-name P DIR`.
pub proof fn find_accepts_iff_grammar(ws: Seq<Tok>, i: int, dir: Option<int>, pat: Option<int>)
    requires
        0 <= i <= ws.len(),
    ensures
        parse_ok(ws, i, dir, pat) <==> items(
            ws,
            i,
            if dir.is_some() { 1int } else { 0int },
            if pat.is_some() { 1int } else { 0int },
        ),
    decreases ws.len() - i,
{
    if i >= ws.len() as int {
        // the parser wants both accumulators; the grammar wants one literal
        // and at least one pattern
    } else if is_name(ws[i]) {
        if i + 1 >= ws.len() as int || !is_word(ws[i + 1]) {
            // both refuse: no pattern after `-name`
        } else {
            find_accepts_iff_grammar(ws, i + 2, dir, Some(tok_word(ws[i + 1])));
            if pat.is_some() {
                lemma_items_pats_pos(
                    ws,
                    i + 2,
                    if dir.is_some() { 1int } else { 0int },
                    1,
                    (if pat.is_some() { 1int } else { 0int }) + 1,
                );
            }
        }
    } else if is_other(ws[i]) {
        // both refuse: an unmodelled flag
    } else {
        if dir.is_some() {
            // both refuse: a second directory word
            lemma_items_two_dirs(ws, i + 1, 2, if pat.is_some() { 1int } else { 0int });
        } else {
            find_accepts_iff_grammar(ws, i + 1, Some(tok_word(ws[i])), pat);
        }
    }
}

pub proof fn find_accepts_is_the_grammar(ws: Seq<Tok>)
    ensures
        accepts(ws) <==> grammar(ws),
{
    find_accepts_iff_grammar(ws, 0, None, None);
}

/// The five-token list of a two-pattern `find`, and what the fold makes of it.
pub open spec fn two_pattern(dir: int, a: int, b: int) -> Seq<Tok> {
    seq![Tok::Word(dir), Tok::NameFlag, Tok::Word(a), Tok::NameFlag, Tok::Word(b)]
}

/// **d-F3-1.** A second `-name` OVERWRITES the first, so `find DIR -name a
/// -name b` is accepted and matched by `b` alone, while a real `find` requires
/// both. The law is the divergence, with a witness name that `b` matches and
/// `a` does not.
pub proof fn find_accepts_a_greedy_multi_pattern(dir: int, a: int, b: int, name: Seq<int>)
    requires
        suffix_at(seq![b], name),
        !suffix_at(seq![a], name),
    ensures
        accepts(two_pattern(dir, a, b)),
        // the fold's recorded pattern is the LAST one
        parse(two_pattern(dir, a, b), 0, None, None) == Some((dir, b)),
        // the fold selects this name...
        last_n(name, 1) == seq![b],
        // ...while `find` (both patterns conjoined) does not
        !(last_n(name, 1) == seq![a] && last_n(name, 1) == seq![b]),
{
    perl_substr_neg_is_the_suffix_glob(seq![a], name);
    perl_substr_neg_is_the_suffix_glob(seq![b], name);
    assert(grammar(two_pattern(dir, a, b))) by (compute);
    find_accepts_is_the_grammar(two_pattern(dir, a, b));
    assert(parse(two_pattern(dir, a, b), 0, None, None) == Some((dir, b))) by (compute);
}

// ─────────────────────────────────────────── §3  `find ... | head -N` ─────

/// The fold collects the WHOLE traversal and splices: `@__out[0 .. $n-1]`.
pub open spec fn take_first(xs: Seq<int>, n: int) -> Seq<int> {
    let m = if n < xs.len() as int { n } else { xs.len() as int };
    xs.subrange(0, m)
}

/// **The semantic side**: the first `n` results, read front-first. A different
/// recursion from the fold's splice (which slices a whole collected list), so
/// the law says something.
pub open spec fn first_n(xs: Seq<int>, n: int) -> Seq<int>
    decreases xs.len(),
{
    if n <= 0 || xs.len() == 0 {
        Seq::empty()
    } else {
        seq![xs[0]] + first_n(xs.skip(1), n - 1)
    }
}

/// one element peeled off the front on each side
pub proof fn lemma_take_first_step(xs: Seq<int>, n: int)
    requires
        0 < n,
        0 < xs.len(),
    ensures
        take_first(xs, n) == seq![xs[0]] + take_first(xs.skip(1), n - 1),
{
}

/// **The head law.** `head -N` SHORT-CIRCUITS in the shell (the reader closes
/// the pipe after N lines); the fold instead collects the whole finite walk and
/// splices, which is the same list only because the traversal order is fixed and
/// finite. That equivalence is this law.
pub proof fn head_take_is_the_first_n(xs: Seq<int>, n: int)
    requires
        0 <= n,
    ensures
        take_first(xs, n) == first_n(xs, n),
    decreases xs.len(),
{
    if n <= 0 || xs.len() == 0 {
    } else {
        lemma_take_first_step(xs, n);
        head_take_is_the_first_n(xs.skip(1), n - 1);
    }
}


// ────────────────────────────────────────────────── §4  pins ──────────────

/// pin: `*.sh` selects a name that ends in `.sh` — the emitted
/// `substr($e, -3) eq q{.sh}` and the glob agree on the ordinary case.
pub proof fn pin_a_dotted_glob_selects_the_suffix(name: Seq<int>)
    requires
        suffix_at(seq![46int, 115int, 104int], name),
    ensures
        last_n(name, 3) == seq![46int, 115int, 104int],
{
    perl_substr_neg_is_the_suffix_glob(seq![46int, 115int, 104int], name);
}

/// pin: a name SHORTER than the pattern is not selected — Perl clamps the
/// offset to 0 and compares the whole name, so `sh` never matches `*.sh`.
pub proof fn pin_a_short_name_is_not_selected()
    ensures
        last_n(seq![115int, 104int], 3) == seq![115int, 104int],
        last_n(seq![115int, 104int], 3) != seq![46int, 115int, 104int],
{
    assert(last_n(seq![115int, 104int], 3) == seq![115int, 104int]) by (compute);
    assert(last_n(seq![115int, 104int], 3) != seq![46int, 115int, 104int]) by (compute);
}

/// pin: `find DIR -name a -name b` keeps b, the last pattern (d-F3-1).
pub proof fn pin_two_name_flags_keep_the_last()
    ensures
        parse(two_pattern(7int, 1int, 2int), 0, None, None) == Some((7int, 2int)),
        accepts(two_pattern(7int, 1int, 2int)),
{
    assert(parse(two_pattern(7int, 1int, 2int), 0, None, None) == Some((7int, 2int))) by (compute);
    assert(grammar(two_pattern(7int, 1int, 2int))) by (compute);
    find_accepts_is_the_grammar(two_pattern(7int, 1int, 2int));
}

/// pin: `head -2` of a four-line walk is the first two lines.
pub proof fn pin_head_two_of_four()
    ensures
        take_first(seq![11int, 12int, 13int, 14int], 2) == first_n(
            seq![11int, 12int, 13int, 14int],
            2,
        ),
{
    head_take_is_the_first_n(seq![11int, 12int, 13int, 14int], 2);
    assert(first_n(seq![11int, 12int, 13int, 14int], 2) == seq![11int, 12int]) by (compute);
}

} // verus!

fn main() {}
