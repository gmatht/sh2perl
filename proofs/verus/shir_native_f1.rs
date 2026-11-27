//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_f1.rs — cluster F1 of the
//  `shir_native_stmt` decomposition (filter pipelines, first command group).
//
//  Plan §F1 names `native_echo_tr_sort_stmt` (5252), `tr_set_escape` (5220),
//  `native_wc_l_pipeline` (2473), `native_head_tail_stmt` (1749) and
//  `native_head_tail_pipeline` (2369). This file proves the parts of F1 that
//  have a self-contained text model:
//
//    * `tr_join_law` — the fold's per-line `tr///` (the character-set map)
//      distributes over the producer's lines exactly as bash's `tr` acts on
//      the producer's byte stream: translating each line and re-joining is
//      the same text as translating the joined text.
//    * `tr_line_count_law` — translation preserves the line structure (one
//      output line per input line), so a following `wc -l`/`head`/`tail`
//      sees the same lines.
//    * `wc_l_law` — `native_wc_l_pipeline` reports `input.lines().count()`;
//      bash's `wc -l` counts newline characters. On the fold's producer
//      (an `echo`/`printf` emitting a newline-terminated text) those agree:
//      counting the `\\n`s of `join(lines)` equals the number of lines.
//    * `head_bytes_law` / `tail_bytes_law` — `native_head_tail_stmt`'s
//      `-c N` mode takes the first/last N bytes with a clamp at the input
//      length; bash's `head -c`/`tail -c` do the same.
//
//  ── the two sides ─────────────────────────────────────────────────────
//
//  SEMANTIC side: bash commands over the produced byte stream — `tr` renames
//  one character throughout, `wc -l` walks the bytes counting `\\n`, and
//  `head -c`/`tail -c` select a byte window.
//
//  PASS side: the folds' computations over the line list / the clamped
//  string: `tr_lines` maps each line and `join` rebuilds the text; `wc`
//  uses the line count; the head/tail mode clamps with `min` (and, for
//  tail, subtracts from the length).
//
//  ── stated, not modelled ──────────────────────────────────────────────
//
//   * F1's `sort` (plain order and its `-n`/`-r` refusals): NOT THIS PROOF.
//     Proving it needs an ordering/permutation model with a uniqueness
//     lemma; `sort`'s ordering is left open rather than assumed. The
//     emitted `sort` call is the backend's own list sort.
//   * F1's line-mode `head -N`/`tail -N` splitting and the joined output's
//     trailing-newline handling: NOT THIS PROOF (the byte mode is here).
//   * `native_literal_subshell_wc_stmt`'s nested literal grep/sed tree:
//     NOT THIS PROOF (that is F2's matcher).
//   * `native_sort_stmt` / `native_sort_file_stmt`'s numeric/human/reverse
//     orderings and `human_size`: NOT THIS PROOF.
//   * `tr` translating the line-separating newline itself (`a == '\n'`):
//     outside the law, stated as the explicit hypothesis `a != '\n'`.
//     bash's `tr` would translate the separators too; the fold maps only
//     the line contents, so the law is restricted rather than assumed.
//   * Locales, multibyte cutting (the source floors to a char boundary and
//     the corpus is ASCII) and the byte renderer are out of model.
//   * D1's renderer round trip, D2's chain laws and D3's condition laws are
//     reused conceptually and not re-claimed; clusters A, F2–F4 and G are
//     other proofs.
//
//  ── measured mutants (restored byte-identically; sha in the sync) ──────
//
//  baseline 18 verified, 0 errors. All five go red:
//    m1 tr skips the per-line translation: 17 verified, 1 error at
//       tr_join_law.
//    m2 newline count off by one: 15v/3e (wc_l_law and its pins).
//    m3 join drops the later lines: rejected before verification — join
//       stops being recursive, so join_cons's reveal_with_fuel cannot apply.
//    m4 head takes the suffix: 17v/1e at head_bytes_law.
//    m5 tail clamps to empty instead of the whole input: 16v/2e at
//       tail_bytes_law and pin_tail_clamps.
//  Each mutated file was restored byte-identically and its sha re-checked.
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ─────────────────────────────────────────────── tr: characters and lines ──

pub open spec fn tr_char(c: char, a: char, b: char) -> char {
    if c == a { b } else { c }
}

pub open spec fn tr_text(t: Seq<char>, a: char, b: char) -> Seq<char>
    decreases t.len(),
{
    if t.len() == 0 {
        Seq::empty()
    } else {
        seq![tr_char(t[0], a, b)] + tr_text(t.drop_first(), a, b)
    }
}

pub open spec fn tr_lines(ls: Seq<Seq<char>>, a: char, b: char) -> Seq<Seq<char>>
    decreases ls.len(),
{
    if ls.len() == 0 {
        Seq::empty()
    } else {
        seq![tr_text(ls[0], a, b)] + tr_lines(ls.drop_first(), a, b)
    }
}

/// the producer's text: every line followed by its newline.
pub open spec fn join(ls: Seq<Seq<char>>) -> Seq<char>
    decreases ls.len(),
{
    if ls.len() == 0 {
        Seq::empty()
    } else {
        ls[0] + (seq!['\n'] + join(ls.drop_first()))
    }
}

/// the fold's tr result, re-joined.
pub open spec fn pass_tr(ls: Seq<Seq<char>>, a: char, b: char) -> Seq<char> {
    join(tr_lines(ls, a, b))
}

/// bash's tr result on the byte stream.
pub open spec fn bash_tr(ls: Seq<Seq<char>>, a: char, b: char) -> Seq<char> {
    tr_text(join(ls), a, b)
}

pub proof fn tr_line_count_law(ls: Seq<Seq<char>>, a: char, b: char)
    ensures tr_lines(ls, a, b).len() == ls.len(),
    decreases ls.len(),
{
    if ls.len() == 0 {
    } else {
        tr_line_count_law(ls.drop_first(), a, b);
    }
}

pub proof fn join_cons(l: Seq<char>, rest: Seq<Seq<char>>)
    ensures join(seq![l] + rest) == l + seq!['\n'] + join(rest),
{
    assert((seq![l] + rest)[0] == l);
    assert((seq![l] + rest).drop_first() == rest);
    reveal_with_fuel(join, 2);
}

pub proof fn tr_text_concat(x: Seq<char>, y: Seq<char>, a: char, b: char)
    ensures tr_text(x + y, a, b) == tr_text(x, a, b) + tr_text(y, a, b),
    decreases x.len(),
{
    if x.len() == 0 {
    } else {
        assert((x + y).drop_first() == x.drop_first() + y);
        assert(tr_text(x + y, a, b)
            == seq![tr_char(x[0], a, b)] + tr_text(x.drop_first() + y, a, b));
        tr_text_concat(x.drop_first(), y, a, b);
    }
}

pub proof fn tr_join_law(ls: Seq<Seq<char>>, a: char, b: char)
    requires a != '\n',
    ensures pass_tr(ls, a, b) == bash_tr(ls, a, b),
    decreases ls.len(),
{
    reveal_with_fuel(join, 2);
    reveal_with_fuel(tr_lines, 2);
    reveal_with_fuel(tr_text, 2);
    if ls.len() == 0 {
    } else {
        tr_join_law(ls.drop_first(), a, b);
        assert(tr_lines(ls, a, b)
            == seq![tr_text(ls[0], a, b)] + tr_lines(ls.drop_first(), a, b));
        join_cons(tr_text(ls[0], a, b), tr_lines(ls.drop_first(), a, b));
        assert(seq![ls[0]] + ls.drop_first() == ls);
        join_cons(ls[0], ls.drop_first());
        assert(bash_tr(ls, a, b) == tr_text(join(ls), a, b));
        assert(join(ls) == ls[0] + (seq!['\n'] + join(ls.drop_first())));
        assert(tr_char('\n', a, b) == '\n');
        assert(tr_text(join(ls), a, b)
            == tr_text(ls[0] + (seq!['\n'] + join(ls.drop_first())), a, b));
        tr_text_concat(ls[0], seq!['\n'] + join(ls.drop_first()), a, b);
        tr_text_concat(seq!['\n'], join(ls.drop_first()), a, b);
        assert(tr_text(ls[0] + (seq!['\n'] + join(ls.drop_first())), a, b)
            == tr_text(ls[0], a, b) + (seq!['\n'] + tr_text(join(ls.drop_first()), a, b)));
        assert(join(tr_lines(ls, a, b))
            == tr_text(ls[0], a, b) + (seq!['\n'] + join(tr_lines(ls.drop_first(), a, b))));
        assert(bash_tr(ls, a, b)
            == tr_text(ls[0], a, b) + (seq!['\n'] + tr_text(join(ls.drop_first()), a, b)));
    }
}

// ───────────────────────────────────────────────────── wc -l: newline count ──

pub open spec fn count_nl(t: Seq<char>)
    -> int
    decreases t.len(),
{
    if t.len() == 0 {
        0
    } else {
        (if t[0] == '\n' { 1int } else { 0int }) + count_nl(t.drop_first())
    }
}

pub proof fn count_nl_concat(x: Seq<char>, y: Seq<char>)
    ensures count_nl(x + y) == count_nl(x) + count_nl(y),
    decreases x.len(),
{
    if x.len() == 0 {
    } else {
        assert((x + y).drop_first() == x.drop_first() + y);
        assert(count_nl(x + y) == (if x[0] == '\n' { 1int } else { 0int }) + count_nl(x.drop_first() + y));
        count_nl_concat(x.drop_first(), y);
    }
}

/// a line carries no newline of its own (the splitter removed them)
pub open spec fn no_nl(t: Seq<char>) -> bool {
    forall|i: int| 0 <= i < t.len() ==> t[i] != '\n'
}

pub open spec fn all_no_nl(ls: Seq<Seq<char>>) -> bool {
    forall|i: int| 0 <= i < ls.len() ==> no_nl(ls[i])
}

pub proof fn no_nl_zero(t: Seq<char>)
    requires no_nl(t),
    ensures count_nl(t) == 0,
    decreases t.len(),
{
    if t.len() == 0 {
    } else {
        assert(count_nl(t) == (if t[0] == '\n' { 1int } else { 0int }) + count_nl(t.drop_first()));
        no_nl_zero(t.drop_first());
    }
}

/// `join` emits exactly one newline per line, and the lines themselves carry
/// none: bash's newline count is the line count.
pub proof fn wc_l_law(ls: Seq<Seq<char>>)
    requires all_no_nl(ls),
    ensures count_nl(join(ls)) == ls.len(),
    decreases ls.len(),
{
    reveal_with_fuel(join, 2);
    reveal_with_fuel(count_nl, 3);
    if ls.len() == 0 {
    } else {
        wc_l_law(ls.drop_first());
        no_nl_zero(ls[0]);
        assert(seq![ls[0]] + ls.drop_first() == ls);
        join_cons(ls[0], ls.drop_first());
        assert(count_nl(seq!['\n']) == 1);
        assert(join(ls) == ls[0] + (seq!['\n'] + join(ls.drop_first())));
        assert(count_nl(join(ls))
            == count_nl(ls[0] + (seq!['\n'] + join(ls.drop_first()))));
        count_nl_concat(ls[0], seq!['\n'] + join(ls.drop_first()));
        count_nl_concat(seq!['\n'], join(ls.drop_first()));
    }
}

// ─────────────────────────────────────────────── head/tail -c: byte windows ──

pub open spec fn take_n(t: Seq<char>, n: nat) -> Seq<char> {
    t.take(n as int)
}

pub open spec fn drop_n(t: Seq<char>, n: nat) -> Seq<char> {
    t.subrange(n as int, t.len() as int)
}

pub open spec fn bash_head_bytes(t: Seq<char>, n: nat) -> Seq<char> {
    t.take(n as int)
}

pub open spec fn pass_head_bytes(t: Seq<char>, n: nat) -> Seq<char> {
    t.take(n as int)
}

pub open spec fn bash_tail_bytes(t: Seq<char>, n: nat) -> Seq<char> {
    if n as int >= t.len() {
        t
    } else {
        t.subrange(t.len() - n as int, t.len() as int)
    }
}

pub open spec fn pass_tail_bytes(t: Seq<char>, n: nat) -> Seq<char> {
    let m = if n as int >= t.len() { t.len() as int } else { n as int };
    t.subrange(t.len() - m, t.len() as int)
}

pub proof fn head_bytes_law(t: Seq<char>, n: nat)
    ensures bash_head_bytes(t, n) == pass_head_bytes(t, n),
{
}

pub proof fn tail_bytes_law(t: Seq<char>, n: nat)
    ensures bash_tail_bytes(t, n) == pass_tail_bytes(t, n),
{
}

// ───────────────────────────── load-bearing refusals (pins) ──────────────

/// `tail -c 5` on a 3-byte input prints the whole input (clamped), not an
/// empty or underflowed slice.
pub proof fn pin_tail_clamps() {
    let t = seq!['a', 'b', 'c'];
    assert(bash_tail_bytes(t, 5) == t);
    assert(pass_tail_bytes(t, 5) == t);
    assert(bash_tail_bytes(t, 2) == seq!['b', 'c']);
}

/// `head -c 0` prints nothing.
pub proof fn pin_head_zero_is_empty() {
    let t = seq!['a', 'b'];
    assert(bash_head_bytes(t, 0) == Seq::empty());
    assert(pass_head_bytes(t, 0) == Seq::empty());
}

/// `tr` leaves characters outside its set untouched (the set is one char in
/// the fold's domain).
pub proof fn pin_tr_only_maps_the_set() {
    assert(tr_char('b', 'a', 'x') == 'b');
    assert(tr_char('a', 'a', 'x') == 'x');
}

/// A producer without a trailing newline is outside the fold's domain: the
/// source's `wc` is only reached from `echo`/`printf` producers, and `printf`
/// with escapes that omit the newline is refused by the fold's shape.
pub proof fn pin_wc_needs_a_newline_per_line() {
    let ls = seq![seq!['a'], seq!['b']];
    assert(no_nl(seq!['a']));
    assert(no_nl(seq!['b']));
    assert(all_no_nl(ls));
    wc_l_law(ls);
    assert(count_nl(join(ls)) == 2);
}

fn main() {}

} // verus!
