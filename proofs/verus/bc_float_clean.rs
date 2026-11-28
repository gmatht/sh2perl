//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/bc_float_clean.rs — the trailing `+ 0.0` strip removes
//  EXACTLY the additive identity, and nothing else.
//
//  `transforms/bc_float_clean.rs` rewrites an `echo "scale=K; EXPR" | bc`
//  capture in place: a bc argument text whose LAST additive term is the
//  literal `0.0` loses that term, when the remainder still carries a
//  decimal (the shader backend's float verdict). The transform is pure text
//  surgery — `rsplit_once('+')`, a `trim()` and a trailing-`trim_end()` — so
//  the claim it needs is a STRUCTURAL one:
//
//    whatever the strip removes is exactly `ws* '+' ws* '0.0' ws*`, attached
//    to a prefix that does not end in whitespace;
//    the arithmetic content of that remove is one term of value 0.
//
//  This file proves that against an INDEPENDENT recogniser: a five-state DFA
//  over the removed span, written from the grammar rather than from the
//  pass's cursor arithmetic. `strip_add_zero` (the analysis, transcribed
//  from `strip_trailing_add_zero`) and `is_identity_tail` (the DFA) are
//  different recursions; the law says the first only ever deletes what the
//  second accepts.
//
//    strip_removes_only_the_identity     Some(p) ⇒ s = p ++ DFA-accepted
//                                        tail, p not ending in whitespace
//    identity_tail_is_one_zero_term      the accepted tail is exactly
//                                        `+ 0.0`: one additive term, value 0
//    add_zero_is_the_identity            over the value model: ev(a + 0) = ev(a)
//    strip_preserves_the_value           both halves, in one statement
//
//  ── what is modelled, and what is not ──────────────────────────────────
//
//  * The transform's two gates are modelled as their own predicates and
//    pinned: (i) `is_identity_tail` (was the trailing term really `0.0`?),
//    (ii) `has_dot` on the STRIPPED text (a decimal must remain, or the
//    backend's float verdict changes — a RENDERER contract, refused).
//  * `strip_add_zero` is written over the whole string with explicit
//    cursors (`trim_len`, `last_plus`, `skip_ws`) instead of the source's
//    `rsplit_once`/`trim_end`; the equivalence is the point of the law, not
//    an assumption, for the `+ 0.0` shapes.
//  * The float ARITHMETIC is not modelled: the value model is a list of
//    integer TERMS (`total`), and the identity `x + 0 = x` is proved there
//    as `total(a.push(0), 0) = total(a, 0)`. bc's
//    `x + 0.0 == x` for finite decimals is the stated arithmetic fact — it
//    is the same identity, over a domain Verus has no reals for.
//  * Out of model, stated: the IR traversal and the `echo … | bc` pipeline
//    SHAPE check (`clean_pipeline`: `exec`/`builtin` echo, two stages, the
//    second `exec bc`, exactly one echo arg), the `Interpolate` last-part
//    rule, and the renderer's float verdict itself (only its input
//    predicate, `has_dot`, is modelled).
//
//  ── measured mutants (see the README + sync file) ──────────────────────
//
//  baseline 32 verified, 0 errors.
//
//  ONE VACUITY was found here by mutation and fixed in this file: the DFA
//  and the strip's cursor arithmetic shared `is_ws`, so narrowing that
//  charset to ' ' left the file GREEN (measured: 27v/0e on the pre-fix
//  proof) — both sides moved together, and the proof then said nothing about
//  the tabs/newlines `trim_end()` really trims.  FIX: `is_ws_sem` /
//  `all_ws_sem` are the DFA's own transcription, bridged by
//  `is_ws_sem_is_is_ws` / `all_ws_sem_iff`; `pin_ws_charset_covers_tabs`
//  pins the shared value, and an un-pinned narrowing now fails a law (M0).
//
//  Counted — a law fails at verification with a real count:
//    M0  the DFA's charset loses '\r' (the vacuity's  31v/1e  all_ws_sem_iff
//        law-level twin; the pin covers only '\t')
//    M1  DFA Trail stops accepting whitespace         31v/1e  lemma_run_ws
//    M2  strip drops its `s[j+2] == '0'` check         31v/1e  strip_removes_
//        only_the_identity (the DFA never reaches Trail)
//    M3  the term model's `total` base case is 1       31v/1e  lemma_total_push
//    M4  strip accepts a signed `-0.0` tail            31v/1e  strip_removes_
//        only_the_identity (the deleted span is not a zero term)
//    M6  last_plus stops at an adjacent '+' only       31v/1e  lemma_last_plus
//    M11 DFA Trail also accepts '+'                    31v/1e  lemma_run_consumes
//        (an OVER-wide trail: the audit's one-term law is what catches it,
//         not the one-way strip ⇒ DFA law)
//  Pin-level (rejected at simplification, no summary line, recorded as pin
//  evidence and NOT as a law): M0' the DFA charset loses '\t'
//  (pin_ws_charset_covers_tabs), M5 DFA BeforePlus stops skipping whitespace
//  (pin_dfa_agrees), M7 DFA Trail accepts any character, M8 trim_start does
//  not trim, M9 skip_ws skips nothing, M10 the whitespace before '+' is kept
//  (all pin_strip_shapes).  M7–M10 were re-measured on these bytes, not
//  carried over.  Every mutant was restored byte-identically (or measured on
//  a /tmp copy) and the sha256 re-checked.
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ────────────────────────────────────────────────────────── text primitives ──

pub open spec fn is_ws(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r'
}

/// The whitespace charset as the DFA sees it — its own transcription, not
/// the strip's `is_ws`. With one shared predicate, narrowing the charset to
/// ' ' left the file GREEN (measured: 27v/0e) because the strip-side cursors
/// and the recogniser moved together; the bridge law below is what now
/// catches that.
pub open spec fn is_ws_sem(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r'
}

pub open spec fn is_digit(c: char) -> bool {
    '0' <= c && c <= '9'
}

pub open spec fn all_ws_sem(s: Seq<char>, i: int, e: int) -> bool
    decreases e - i,
{
    if i >= e {
        true
    } else {
        is_ws_sem(s[i]) && all_ws_sem(s, i + 1, e)
    }
}

pub proof fn is_ws_sem_is_is_ws(c: char)
    ensures
        is_ws_sem(c) == is_ws(c),
{
}

pub proof fn all_ws_sem_iff(s: Seq<char>, i: int, e: int)
    ensures
        all_ws_sem(s, i, e) == all_ws(s, i, e),
    decreases e - i,
{
    if i < e {
        is_ws_sem_is_is_ws(s[i]);
        all_ws_sem_iff(s, i + 1, e);
    }
}

pub open spec fn all_ws(s: Seq<char>, i: int, e: int) -> bool
    decreases e - i,
{
    if i >= e {
        true
    } else {
        is_ws(s[i]) && all_ws(s, i + 1, e)
    }
}

pub open spec fn ends_ws(s: Seq<char>) -> bool {
    (s.len() as int) > 0 && is_ws(s[(s.len() as int) - 1])
}

pub open spec fn has_dot(s: Seq<char>, i: int) -> bool
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) {
        false
    } else {
        s[i] == '.' || has_dot(s, i + 1)
    }
}

/// First index of the trailing whitespace run — the source's `trim_end()`.
pub open spec fn trim_start(s: Seq<char>, i: int) -> int
    decreases i + 1,
{
    if i <= 0 {
        0
    } else if is_ws(s[i - 1]) {
        trim_start(s, i - 1)
    } else {
        i
    }
}

pub open spec fn trim_len(s: Seq<char>) -> int {
    trim_start(s, (s.len() as int))
}

/// Index of the LAST `+` before `i` — the source's `rsplit_once('+')`.
pub open spec fn last_plus(s: Seq<char>, i: int) -> int
    decreases i + 1,
{
    if i <= 0 {
        -1
    } else if s[i - 1] == '+' {
        i - 1
    } else {
        last_plus(s, i - 1)
    }
}

pub open spec fn skip_ws(s: Seq<char>, i: int) -> int
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) || !is_ws(s[i]) {
        i
    } else {
        skip_ws(s, i + 1)
    }
}

// ─────────────────────────────────────────────────────────── the analysis ──

/// `strip_trailing_add_zero`, transcribed over whole-string cursors.
pub open spec fn strip_add_zero(s: Seq<char>) -> Option<Seq<char>> {
    let e = trim_len(s);
    let i = last_plus(s, e);
    if i < 0 {
        None
    } else {
        let j = skip_ws(s, i + 1);
        if j + 3 <= e && s[j] == '0' && s[j + 1] == '.' && s[j + 2] == '0' && all_ws(s, j + 3, e) {
            Some(s.subrange(0, trim_start(s, i)))
        } else {
            None
        }
    }
}

// ────────────────────────────────────────────────────────── the DFA side ──

/// `ws*  '+'  ws*  '0'  '.'  '0'  ws*` — one additive term, and it is zero.
pub enum Q {
    BeforePlus,
    AfterPlus,
    SeeZero,
    SeeDot,
    Trail,
}

pub open spec fn dfa(q: Q, c: char) -> Option<Q> {
    match q {
        Q::BeforePlus => if is_ws_sem(c) {
            Some(Q::BeforePlus)
        } else if c == '+' {
            Some(Q::AfterPlus)
        } else {
            None
        },
        Q::AfterPlus => if is_ws_sem(c) {
            Some(Q::AfterPlus)
        } else if c == '0' {
            Some(Q::SeeZero)
        } else {
            None
        },
        Q::SeeZero => if c == '.' { Some(Q::SeeDot) } else { None },
        Q::SeeDot => if c == '0' { Some(Q::Trail) } else { None },
        Q::Trail => if is_ws_sem(c) { Some(Q::Trail) } else { None },
    }
}

pub open spec fn run(s: Seq<char>, i: int, e: int, q: Q) -> Option<Q>
    decreases e - i,
{
    if i >= e {
        Some(q)
    } else {
        match dfa(q, s[i]) {
            None => None,
            Some(q2) => run(s, i + 1, e, q2),
        }
    }
}

pub open spec fn is_identity_tail(s: Seq<char>, p: int, e: int) -> bool {
    run(s, p, e, Q::BeforePlus) == Some(Q::Trail)
}

// ────────────────────────────────────────────────────────────── the lemmas ──

proof fn lemma_trim_start(s: Seq<char>, i: int)
    requires 0 <= i <= (s.len() as int),
    ensures
        all_ws(s, trim_start(s, i), i),
        trim_start(s, i) <= i,
        trim_start(s, trim_start(s, i)) == trim_start(s, i),
        trim_start(s, i) > 0 ==> !is_ws(s[trim_start(s, i) - 1]),
    decreases i,
{
    if i <= 0 {
        assert(trim_start(s, i) == 0);
        assert(all_ws(s, 0, 0));
    } else if is_ws(s[i - 1]) {
        lemma_trim_start(s, i - 1);
        let k = trim_start(s, i - 1);
        assert(trim_start(s, i) == k);
        lemma_all_ws_push(s, k, i - 1);
        assert(all_ws(s, k, i));
    } else {
        assert(trim_start(s, i) == i);
        assert(all_ws(s, i, i));
    }
}

/// Extending a whitespace run by one whitespace character.
proof fn lemma_all_ws_push(s: Seq<char>, i: int, e: int)
    requires
        0 <= i <= e < (s.len() as int),
        all_ws(s, i, e),
        is_ws(s[e]),
    ensures all_ws(s, i, e + 1),
    decreases e - i,
{
    if i >= e {
        assert(i == e);
        assert(is_ws(s[i]));
        assert(all_ws(s, i + 1, e + 1));
        assert(all_ws(s, i, e + 1));
    } else {
        lemma_all_ws_push(s, i + 1, e);
    }
}

proof fn lemma_last_plus(s: Seq<char>, i: int)
    requires 0 <= i <= (s.len() as int),
    ensures
        last_plus(s, i) >= 0 ==> s[last_plus(s, i)] == '+',
        last_plus(s, i) >= 0 ==> last_plus(s, i) < i,
        forall|k: int| last_plus(s, i) < k < i ==> s[k] != '+',
    decreases i,
{
    if i <= 0 {
        assert(last_plus(s, i) == -1);
    } else if s[i - 1] == '+' {
        assert(last_plus(s, i) == i - 1);
        assert(s[i - 1] == '+');
    } else {
        lemma_last_plus(s, i - 1);
        assert(last_plus(s, i) == last_plus(s, i - 1));
        assert forall|k: int| last_plus(s, i) < k < i implies s[k] != '+' by {
            if k < i - 1 {
            } else {
                assert(k == i - 1);
            }
        };
    }
}

proof fn lemma_skip_ws(s: Seq<char>, i: int)
    requires 0 <= i <= (s.len() as int),
    ensures
        i <= skip_ws(s, i) <= (s.len() as int),
        all_ws(s, i, skip_ws(s, i)),
        skip_ws(s, i) < (s.len() as int) ==> !is_ws(s[skip_ws(s, i)]),
    decreases (s.len() as int) - i,
{
    if i >= (s.len() as int) || !is_ws(s[i]) {
        assert(skip_ws(s, i) == i);
        assert(all_ws(s, i, i));
    } else {
        lemma_skip_ws(s, i + 1);
        assert(skip_ws(s, i) == skip_ws(s, i + 1));
        assert(all_ws(s, i, skip_ws(s, i + 1)));
    }
}

/// A whitespace run leaves a whitespace-accepting state where it was.
proof fn lemma_run_ws(s: Seq<char>, i: int, e: int, q: Q)
    requires
        all_ws_sem(s, i, e),
        (q == Q::BeforePlus || q == Q::AfterPlus || q == Q::Trail),
    ensures run(s, i, e, q) == Some(q),
    decreases e - i,
{
    if i >= e {
    } else {
        assert(dfa(q, s[i]) == Some(q));
        lemma_run_ws(s, i + 1, e, q);
    }
}

/// One transition, showing the cursor advance.
proof fn lemma_run_step(s: Seq<char>, i: int, e: int, q: Q, q2: Q)
    requires
        0 <= i < e <= (s.len() as int),
        dfa(q, s[i]) == Some(q2),
    ensures run(s, i, e, q) == run(s, i + 1, e, q2),
{
}

/// Concatenating two accepted runs.
proof fn lemma_run_split(s: Seq<char>, i: int, m: int, e: int, q: Q, qm: Q, qf: Q)
    requires
        0 <= i <= m <= e <= (s.len() as int),
        run(s, i, m, q) == Some(qm),
        run(s, m, e, qm) == Some(qf),
    ensures run(s, i, e, q) == Some(qf),
    decreases m - i,
{
    if i >= m {
        assert(q == qm);
    } else {
        match dfa(q, s[i]) {
            None => {
                assert(run(s, i, m, q) == None);
            }
            Some(q2) => {
                lemma_run_split(s, i + 1, m, e, q2, qm, qf);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────── the laws ──

/// THE SOUNDNESS LAW: whatever `strip_add_zero` deletes is exactly the DFA's
/// identity tail, attached to a prefix that does not end in whitespace.
pub proof fn strip_removes_only_the_identity(s: Seq<char>, p: Seq<char>)
    requires strip_add_zero(s) == Some(p),
    ensures
        (p.len() as int) <= trim_len(s),
        s.subrange(0, (p.len() as int)) == p,
        !ends_ws(p),
        is_identity_tail(s, (p.len() as int), trim_len(s)),
{
    let e = trim_len(s);
    let i = last_plus(s, e);
    assert(i >= 0);
    lemma_trim_start(s, (s.len() as int));
    assert(trim_len(s) == trim_start(s, (s.len() as int)));
    lemma_last_plus(s, e);
    assert(i < e);
    let k = trim_start(s, i);
    lemma_trim_start(s, i);
    assert(trim_len(s) == e);
    assert(last_plus(s, trim_len(s)) >= 0);
    assert(strip_add_zero(s) == Some(s.subrange(0, trim_start(s, last_plus(s, trim_len(s))))));
    assert(p == s.subrange(0, k));
    assert(k <= i);
    assert(k >= 0);
    assert(i < e);
    assert(e <= (s.len() as int));
    assert(0 <= k <= (s.len() as int));
    assert((s.subrange(0, k).len() as int) == k);
    assert((p.len() as int) == k);
    assert(trim_start(s, k) == k);
    lemma_skip_ws(s, i + 1);
    let j = skip_ws(s, i + 1);
    assert(i + 1 <= j);
    assert(j + 3 <= e);
    assert(s[i] == '+');
    assert(s.subrange(0, (p.len() as int)) == p);
    // the prefix does not end in whitespace
    if ends_ws(p) {
        assert((p.len() as int) > 0);
        assert(is_ws(p[(p.len() as int) - 1]));
        assert(p[(p.len() as int) - 1] == s[k - 1]);
        assert(false);
    }
    // and the removed span walks the DFA: ws* '+' ws* '0' '.' '0' ws*
    assert(all_ws(s, k, i));
    assert(all_ws(s, i + 1, j));
    assert(all_ws(s, j + 3, e));
    // the DFA's own charset agrees with the strip's on these spans
    all_ws_sem_iff(s, k, i);
    all_ws_sem_iff(s, i + 1, j);
    all_ws_sem_iff(s, j + 3, e);
    assert(dfa(Q::BeforePlus, s[i]) == Some(Q::AfterPlus));
    assert(dfa(Q::AfterPlus, s[j]) == Some(Q::SeeZero));
    assert(dfa(Q::SeeZero, s[j + 1]) == Some(Q::SeeDot));
    assert(dfa(Q::SeeDot, s[j + 2]) == Some(Q::Trail));
    lemma_run_ws(s, k, i, Q::BeforePlus);
    lemma_run_ws(s, i + 1, j, Q::AfterPlus);
    lemma_run_ws(s, j + 3, e, Q::Trail);
    // run over '0' '.' '0'
    lemma_run_step(s, j, e, Q::AfterPlus, Q::SeeZero);
    lemma_run_step(s, j + 1, e, Q::SeeZero, Q::SeeDot);
    lemma_run_step(s, j + 2, e, Q::SeeDot, Q::Trail);
    assert(run(s, j, e, Q::AfterPlus) == Some(Q::Trail));
    // AfterPlus from i+1 through j is whitespace, then the zero tail
    lemma_run_split(s, i + 1, j, e, Q::AfterPlus, Q::AfterPlus, Q::Trail);
    assert(run(s, i + 1, e, Q::AfterPlus) == Some(Q::Trail));
    lemma_run_step(s, i, e, Q::BeforePlus, Q::AfterPlus);
    assert(run(s, i, e, Q::BeforePlus) == Some(Q::Trail));
    // and the prefix whitespace keeps BeforePlus
    lemma_run_split(s, k, i, e, Q::BeforePlus, Q::BeforePlus, Q::Trail);
    assert(run(s, k, e, Q::BeforePlus) == Some(Q::Trail));
    assert(run(s, (p.len() as int), e, Q::BeforePlus) == Some(Q::Trail));
}

/// The DFA-accepted tail carries exactly one additive term, and it is zero:
/// its non-whitespace characters are `+`, `0`, `.`, `0` in that order.
pub open spec fn nonws(s: Seq<char>, i: int, e: int) -> Seq<char>
    decreases e - i,
{
    if i >= e {
        Seq::empty()
    } else if is_ws_sem(s[i]) {
        nonws(s, i + 1, e)
    } else {
        seq![s[i]].add(nonws(s, i + 1, e))
    }
}

/// What each state still has to consume, once whitespace is dropped.
pub open spec fn expected(q: Q) -> Seq<char> {
    match q {
        Q::BeforePlus => seq!['+', '0', '.', '0'],
        Q::AfterPlus => seq!['0', '.', '0'],
        Q::SeeZero => seq!['.', '0'],
        Q::SeeDot => seq!['0'],
        Q::Trail => Seq::empty(),
    }
}

proof fn lemma_run_consumes(s: Seq<char>, i: int, e: int, q: Q)
    requires 0 <= i <= e <= (s.len() as int),
    ensures run(s, i, e, q) == Some(Q::Trail) ==> nonws(s, i, e) == expected(q),
    decreases e - i,
{
    if i >= e {
        if q != Q::Trail {
            assert(run(s, i, e, q) == Some(q));
            assert(run(s, i, e, q) != Some(Q::Trail));
        }
    } else {
        let c = s[i];
        match dfa(q, c) {
            None => {
                assert(run(s, i, e, q) == None);
            }
            Some(q2) => {
                lemma_run_consumes(s, i + 1, e, q2);
                if is_ws_sem(c) {
                    // whitespace keeps the expected remainder equal
                    assert(expected(q) == expected(q2)) by {
                        match q {
                            Q::BeforePlus => {},
                            Q::AfterPlus => {},
                            Q::Trail => {},
                            _ => {
                                // no accepting state reaches Trail through whitespace
                                assert(false);
                            }
                        }
                    };
                } else {
                    assert(nonws(s, i, e) == seq![c].add(nonws(s, i + 1, e)));
                    assert(seq![c].add(expected(q2)) == expected(q));
                }
            }
        }
    }
}

pub proof fn identity_tail_is_one_zero_term(s: Seq<char>, p: int, e: int)
    requires
        0 <= p <= e <= (s.len() as int),
        is_identity_tail(s, p, e),
    ensures nonws(s, p, e) == seq!['+', '0', '.', '0'],
{
    lemma_run_consumes(s, p, e, Q::BeforePlus);
}

/// The value model: an additive expression is its list of TERMS, summed.
/// The literal `0.0` is the term `0`, so appending it cannot change the
/// total — which is exactly what the strip relies on.
pub open spec fn total(a: Seq<int>, i: int) -> int
    decreases a.len() - i,
{
    if i >= a.len() {
        0
    } else {
        a[i] + total(a, i + 1)
    }
}

proof fn lemma_total_push(a: Seq<int>, x: int, i: int)
    requires 0 <= i <= a.len(),
    ensures total(a.push(x), i) == total(a, i) + x,
    decreases a.len() - i,
{
    if i >= a.len() {
        if i == a.len() {
            assert((a.push(x).len() as int) == (a.len() as int) + 1);
            assert(i + 1 >= (a.push(x).len() as int));
            assert(total(a.push(x), i + 1) == 0);
            assert(total(a.push(x), i) == a.push(x)[i] + 0);
            assert(a.push(x)[i] == x);
            assert(total(a, i) == 0);
        }
    } else {
        assert(a.push(x)[i] == a[i]);
        lemma_total_push(a, x, i + 1);
    }
}

/// `x + 0 == x`, over the term model.
pub proof fn add_zero_is_the_identity(a: Seq<int>)
    ensures total(a.push(0), 0) == total(a, 0),
{
    lemma_total_push(a, 0, 0);
}

/// Both structural halves in one statement, for the transform: the removed
/// text is the DFA's identity tail, and that tail is one `+ 0.0` term. (The
/// value half is `add_zero_is_the_identity`, which takes no string.)
pub proof fn strip_preserves_the_value(s: Seq<char>, p: Seq<char>)
    requires strip_add_zero(s) == Some(p),
    ensures
        is_identity_tail(s, (p.len() as int), trim_len(s)),
        nonws(s, (p.len() as int), trim_len(s)) == seq!['+', '0', '.', '0'],
{
    strip_removes_only_the_identity(s, p);
    assert((p.len() as int) <= trim_len(s));
    lemma_trim_start(s, (s.len() as int));
    assert(trim_len(s) <= (s.len() as int));
    identity_tail_is_one_zero_term(s, (p.len() as int), trim_len(s));
}

// ───────────────────────────────────────────────────────────────────── pins ──

pub open spec fn t3() -> Seq<char> {
    seq!['3']
}

/// The shapes the source's own tests exercise, plus every boundary the
/// cursor arithmetic can get wrong.
pub proof fn pin_strip_shapes()
    ensures
        strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0']) == Some(t3()),
        strip_add_zero(seq!['3', '+', '0', '.', '0']) == Some(t3()),
        strip_add_zero(
            seq!['s', 'c', 'a', 'l', 'e', '=', '2', ';', ' ', '3', '.', '5', ' ', '+', ' ', '0', '.', '0'],
        ) == Some(
            seq!['s', 'c', 'a', 'l', 'e', '=', '2', ';', ' ', '3', '.', '5'],
        ),
        // trailing whitespace on the whole text is trimmed too
        strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', ' ', ' ']) == Some(t3()),
        // whitespace between '+' and '0.0'
        strip_add_zero(seq!['3', '+', ' ', ' ', '0', '.', '0']) == Some(t3()),
{
    assert(strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0']) == Some(t3())) by (compute);
    assert(strip_add_zero(seq!['3', '+', '0', '.', '0']) == Some(t3())) by (compute);
    assert(strip_add_zero(seq![
        's', 'c', 'a', 'l', 'e', '=', '2', ';', ' ', '3', '.', '5', ' ', '+', ' ', '0', '.', '0',
    ]) == Some(seq!['s', 'c', 'a', 'l', 'e', '=', '2', ';', ' ', '3', '.', '5'])) by (compute);
    assert(strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', ' ', ' ']) == Some(t3()))
        by (compute);
    assert(strip_add_zero(seq!['3', '+', ' ', ' ', '0', '.', '0']) == Some(t3())) by (compute);
}

/// REFUSALS: a trailing term that is not `0.0`, a `+` that is not the last
/// top-level one, a negative zero, and shapes with no additive term at all.
pub proof fn pin_strip_refusals()
    ensures
        strip_add_zero(seq!['3', ' ', '+', ' ', '0']) == None,
        strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', '0']) == None,
        strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', ' ', '+', ' ', '2']) == None,
        strip_add_zero(seq!['3', ' ', '+', ' ', '-', '0', '.', '0']) == None,
        strip_add_zero(seq!['3', '.', '5']) == None,
        strip_add_zero(seq!['0', '.', '0']) == None,
        strip_add_zero(Seq::<char>::empty()) == None,
{
    assert(strip_add_zero(seq!['3', ' ', '+', ' ', '0']) == None) by (compute);
    assert(strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', '0']) == None) by (compute);
    assert(strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', ' ', '+', ' ', '2']) == None)
        by (compute);
    assert(strip_add_zero(seq!['3', ' ', '+', ' ', '-', '0', '.', '0']) == None) by (compute);
    assert(strip_add_zero(seq!['3', '.', '5']) == None) by (compute);
    assert(strip_add_zero(seq!['0', '.', '0']) == None) by (compute);
    assert(strip_add_zero(Seq::<char>::empty()) == None) by (compute);
}

/// THE SECOND GATE: the strip only fires when the REMAINDER still has a
/// decimal — the only-decimal case is refused because the shader backend's
/// float verdict would change (a renderer contract, not a value claim).
pub proof fn pin_the_decimal_gate()
    ensures
        strip_add_zero(seq!['1', ' ', '+', ' ', '0', '.', '0']) == Some(seq!['1']),
        !has_dot(seq!['1'], 0),
        strip_add_zero(
            seq!['1', '.', '5', ' ', '+', ' ', '0', '.', '0'],
        ) == Some(seq!['1', '.', '5']),
        has_dot(seq!['1', '.', '5'], 0),
{
    assert(strip_add_zero(seq!['1', ' ', '+', ' ', '0', '.', '0']) == Some(seq!['1'])) by (compute);
    assert(!has_dot(seq!['1'], 0)) by (compute);
    assert(strip_add_zero(seq!['1', '.', '5', ' ', '+', ' ', '0', '.', '0'])
        == Some(seq!['1', '.', '5'])) by (compute);
    assert(has_dot(seq!['1', '.', '5'], 0)) by (compute);
}

/// The DFA on the same spans: the analysis and the recogniser agree here.
pub proof fn pin_dfa_agrees()
    ensures
        is_identity_tail(seq!['3', ' ', '+', ' ', '0', '.', '0'], 1, 7),
        !is_identity_tail(seq!['3', ' ', '+', ' ', '0'], 1, 5),
        !is_identity_tail(seq!['3', ' ', '+', ' ', '0', '.', '0', ' ', '+', ' ', '2'], 1, 11),
        nonws(seq!['3', ' ', '+', ' ', '0', '.', '0'], 1, 7) == seq!['+', '0', '.', '0'],
{
    assert(is_identity_tail(seq!['3', ' ', '+', ' ', '0', '.', '0'], 1, 7)) by (compute);
    assert(!is_identity_tail(seq!['3', ' ', '+', ' ', '0'], 1, 5)) by (compute);
    assert(!is_identity_tail(seq!['3', ' ', '+', ' ', '0', '.', '0', ' ', '+', ' ', '2'], 1, 11))
        by (compute);
    assert(nonws(seq!['3', ' ', '+', ' ', '0', '.', '0'], 1, 7) == seq!['+', '0', '.', '0'])
        by (compute);
}

/// One pass removes ONE occurrence: `3 + 0.0 + 0.0` keeps its inner identity.
pub proof fn pin_one_occurrence_per_pass()
    ensures
        strip_add_zero(seq![
            '3', ' ', '+', ' ', '0', '.', '0', ' ', '+', ' ', '0', '.', '0',
        ]) == Some(seq!['3', ' ', '+', ' ', '0', '.', '0']),
{
    assert(strip_add_zero(seq![
        '3', ' ', '+', ' ', '0', '.', '0', ' ', '+', ' ', '0', '.', '0',
    ]) == Some(seq!['3', ' ', '+', ' ', '0', '.', '0'])) by (compute);
}

/// The whitespace charset: Rust's `trim_end()` trims a tab as readily as a
/// space, so the strip and the DFA must agree about it. This is the pin for
/// the vacuity that used to exist here (one shared `is_ws`).
pub proof fn pin_ws_charset_covers_tabs()
    ensures
        strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', '\t']) == Some(t3()),
        is_identity_tail(seq!['3', ' ', '+', ' ', '0', '.', '0', '\t'], 1, 7),
        is_ws('\t'),
        is_ws_sem('\t'),
        !is_ws_sem('x'),
        !is_ws('%'),
{
    assert(strip_add_zero(seq!['3', ' ', '+', ' ', '0', '.', '0', '\t']) == Some(t3())) by (compute);
    assert(is_identity_tail(seq!['3', ' ', '+', ' ', '0', '.', '0', '\t'], 1, 7)) by (compute);
    assert(is_ws('\t')) by (compute);
    assert(is_ws_sem('\t')) by (compute);
    assert(!is_ws_sem('x')) by (compute);
    assert(!is_ws('%')) by (compute);
}

/// The value model on a concrete expression: appending the zero term keeps
/// the total — bc's `x + 0.0 == x` over the integer model the proof uses.
/// This is the pin behind the executable `bc` comparison in the generated
/// test suite (the real `bc` run is in `src/transforms/bc_float_pin_tests.rs`).
pub proof fn pin_bc_value_preserved()
    ensures
        total(seq![3, 5].push(0), 0) == total(seq![3, 5], 0),
{
    add_zero_is_the_identity(seq![3, 5]);
}

fn main() {}

} // verus!
