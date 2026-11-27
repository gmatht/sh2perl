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
//    * `plain_sort_law` — `native_sort_stmt`'s plain branch splits the
//      producer, drops a trailing empty segment, sorts and re-joins. Prove
//      two independent properties of that output: it is nondecreasing under
//      a byte-lexicographic order (`sorted`, `sorted_at`, insertion sort),
//      and it is a permutation of the input (`same_multiset`, count
//      per line). The semantic side is those relations, not a second sort;
//      the pass side is the insertion-sort algorithm itself.
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
//   * F1's line-mode `head -N`/`tail -N`: the fold's split/join computation
//     (`split_nl`/`join_lines`/`source_lines`) is modelled and its emitted
//     bytes are pinned, but the byte-level equality with bash's line copy is
//     NOT proved. The pins show the fold diverges from bash in three cases:
//     `head -N` with N past the last line appends a stray newline; `tail -N`
//     on a newline-terminated producer selects the trailing empty segment
//     and prints nothing; `tail -N` on an unterminated producer adds a
//     newline bash does not print. These are source divergences, recorded
//     rather than assumed away.
//   * `native_literal_subshell_wc_stmt`'s nested literal grep/sed tree:
//     NOT THIS PROOF (that is F2's matcher).
//   * `native_sort_stmt` / `native_sort_file_stmt`'s numeric and human
//     orderings (`-n`, `-h`) and `human_size`: NOT THIS PROOF — the source
//     parses `f64`/human suffixes, a different comparison key. The plain
//     order law covers no flags or `-r`; unknown flags (`-u`, `--reverse`,
//     `-k2`, a file operand) are refused by `plain_sort_flag` as pins.
//     Note the plan text expected the source to refuse `-n`/`-r`; it does
//     not (it implements them), so the refusal pins are for the flags it
//     really refuses.
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
//  first measurement (tr/wc/head-tail): baseline 18 verified, 0 errors.
//  All five go red:
//    m1 tr skips the per-line translation: 17 verified, 1 error at
//       tr_join_law.
//    m2 newline count off by one: 15v/3e (wc_l_law and its pins).
//    m3 join drops the later lines: rejected before verification — join
//       stops being recursive, so join_cons's reveal_with_fuel cannot apply.
//    m4 head takes the suffix: 17v/1e at head_bytes_law.
//    m5 tail clamps to empty instead of the whole input: 16v/2e at
//       tail_bytes_law and pin_tail_clamps.
//  second measurement (sort + line mode): baseline 47 verified, 0 errors.
//  All five go red:
//    s1 insert compares backwards: 42v/5e (sorted/isort proof chain).
//    s2 isort drops the head line: 45v/2e (same_multiset/isort_perm).
//    s3 plain_sort_flag accepts -n: 46v/1e at
//       pin_unknown_sort_flags_keep_the_shell_out.
//    s4 source_lines omits its newline: 44v/3e at the head-line pins.
//    s5 join_lines adds a trailing newline: 43v/4e at the head-line pins.
//  Every mutated file was restored byte-identically and its sha re-checked.
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



// ── sort: byte-order model on lines as sequences of bytes ────────────────

spec fn lex_le(a: Seq<nat>, b: Seq<nat>) -> bool
    decreases a.len() as int + b.len() as int,
{
    if a.len() == 0 {
        true
    } else if b.len() == 0 {
        false
    } else if a[0] < b[0] {
        true
    } else if a[0] > b[0] {
        false
    } else {
        lex_le(a.drop_first(), b.drop_first())
    }
}

proof fn lex_le_refl(a: Seq<nat>)
    ensures lex_le(a, a),
    decreases a.len(),
{
    if a.len() > 0 {
        lex_le_refl(a.drop_first());
    }
}

proof fn lex_le_total(a: Seq<nat>, b: Seq<nat>)
    ensures lex_le(a, b) || lex_le(b, a),
    decreases a.len() as int + b.len() as int,
{
    if a.len() == 0 || b.len() == 0 {
    } else if a[0] == b[0] {
        lex_le_total(a.drop_first(), b.drop_first());
    }
}

proof fn lex_le_trans(a: Seq<nat>, b: Seq<nat>, c: Seq<nat>)
    requires lex_le(a, b), lex_le(b, c),
    ensures lex_le(a, c),
    decreases a.len() as int + b.len() as int,
{
    if a.len() == 0 {
    } else if b.len() == 0 {
    } else if c.len() == 0 {
    } else if a[0] == b[0] {
        if b[0] == c[0] {
            lex_le_trans(a.drop_first(), b.drop_first(), c.drop_first());
        }
    }
}

// sortedness of a line list (adjacent order suffices for total orders)
spec fn sorted(ls: Seq<Seq<nat>>) -> bool
    decreases ls.len(),
{
    if ls.len() <= 1 {
        true
    } else {
        lex_le(ls[0], ls[1]) && sorted(ls.drop_first())
    }
}

spec fn count_of(ls: Seq<Seq<nat>>, x: Seq<nat>) -> nat
    decreases ls.len(),
{
    if ls.len() == 0 {
        0nat
    } else {
        (if ls[0] == x { 1nat } else { 0nat }) + count_of(ls.drop_first(), x)
    }
}

spec fn same_multiset(a: Seq<Seq<nat>>, b: Seq<Seq<nat>>) -> bool {
    forall|x: Seq<nat>| #[trigger] count_of(a, x) == count_of(b, x)
}

proof fn count_concat(a: Seq<Seq<nat>>, b: Seq<Seq<nat>>, x: Seq<nat>)
    ensures count_of(a + b, x) == count_of(a, x) + count_of(b, x),
    decreases a.len(),
{
    if a.len() == 0 {
    } else {
        assert((a + b)[0] == a[0]);
        assert((a + b).drop_first() == a.drop_first() + b);
        count_concat(a.drop_first(), b, x);
    }
}

proof fn count_single(x: Seq<nat>, y: Seq<nat>)
    ensures count_of(seq![x], y) == (if x == y { 1nat } else { 0nat }),
{
    reveal_with_fuel(count_of, 2);
    assert(seq![x][0] == x);
}

// insertion of one line into an already sorted list (the pass's sort step)
spec fn insert(x: Seq<nat>, ls: Seq<Seq<nat>>) -> Seq<Seq<nat>>
    decreases ls.len(),
{
    if ls.len() == 0 {
        seq![x]
    } else if lex_le(x, ls[0]) {
        seq![x] + ls
    } else {
        seq![ls[0]] + insert(x, ls.drop_first())
    }
}

spec fn isort(ls: Seq<Seq<nat>>) -> Seq<Seq<nat>>
    decreases ls.len(),
{
    if ls.len() == 0 {
        Seq::empty()
    } else {
        insert(ls[0], isort(ls.drop_first()))
    }
}

// every element of a sorted list is <= its head (used by insert)
proof fn sorted_head_le_tail(ls: Seq<Seq<nat>>, i: int)
    requires sorted(ls), 0 <= i < ls.len(),
    ensures lex_le(ls[0], ls[i]),
    decreases i,
{
    reveal_with_fuel(sorted, 3);
    if i == 0 {
        lex_le_refl(ls[0]);
    } else {
        assert(lex_le(ls[0], ls[1]));
        assert(ls.drop_first()[0] == ls[1]);
        assert(ls.drop_first()[i - 1] == ls[i]);
        sorted_head_le_tail(ls.drop_first(), i - 1);
        lex_le_trans(ls[0], ls[1], ls[i]);
    }
}

proof fn sorted_tail(ls: Seq<Seq<nat>>)
    requires sorted(ls), ls.len() > 0,
    ensures sorted(ls.drop_first()),
{
    reveal_with_fuel(sorted, 2);
}

proof fn sorted_at(ls: Seq<Seq<nat>>, i: int)
    requires sorted(ls), 0 <= i < ls.len() - 1,
    ensures lex_le(ls[i], ls[i + 1]),
    decreases i,
{
    reveal_with_fuel(sorted, 3);
    if i == 0 {
        assert(ls[0] == ls[0]);
    } else {
        assert(ls.drop_first()[i - 1] == ls[i]);
        assert(ls.drop_first()[i] == ls[i + 1]);
        sorted_at(ls.drop_first(), i - 1);
    }
}

proof fn insert_head_is_min(x: Seq<nat>, ls: Seq<Seq<nat>>)
    requires sorted(ls), ls.len() > 0,
    ensures lex_le(insert(x, ls)[0], ls[0]),
{
    reveal_with_fuel(insert, 2);
    if lex_le(x, ls[0]) {
        assert(insert(x, ls) == seq![x] + ls);
        assert((seq![x] + ls)[0] == x);
        assert(insert(x, ls)[0] == x);
    } else {
        assert(insert(x, ls) == seq![ls[0]] + insert(x, ls.drop_first()));
        assert((seq![ls[0]] + insert(x, ls.drop_first()))[0] == ls[0]);
        assert(insert(x, ls)[0] == ls[0]);
        lex_le_total(x, ls[0]);
        lex_le_refl(ls[0]);
    }
}

proof fn insert_first_no_less(k: Seq<nat>, x: Seq<nat>, ls: Seq<Seq<nat>>)
    requires sorted(ls), ls.len() > 0, lex_le(k, ls[0]), lex_le(k, x),
    ensures lex_le(k, insert(x, ls)[0]),
{
    reveal_with_fuel(insert, 2);
    if lex_le(x, ls[0]) {
        assert(insert(x, ls) == seq![x] + ls);
        assert((seq![x] + ls)[0] == x);
    } else {
        assert(insert(x, ls) == seq![ls[0]] + insert(x, ls.drop_first()));
        assert((seq![ls[0]] + insert(x, ls.drop_first()))[0] == ls[0]);
    }
}

proof fn insert_sorted(x: Seq<nat>, ls: Seq<Seq<nat>>)
    requires sorted(ls),
    ensures sorted(insert(x, ls)),
    decreases ls.len(),
{
    reveal_with_fuel(insert, 2);
    reveal_with_fuel(sorted, 3);
    if ls.len() == 0 {
    } else if lex_le(x, ls[0]) {
        assert(insert(x, ls) == seq![x] + ls);
        assert(sorted(ls));
        assert(lex_le(x, ls[0]));
        assert forall|j: int| 0 <= j < (seq![x] + ls).len() - 1 implies
            #[trigger] lex_le((seq![x] + ls)[j], (seq![x] + ls)[j + 1]) by {
            if j == 0 {
                assert((seq![x] + ls)[0] == x);
                assert((seq![x] + ls)[1] == ls[0]);
            } else {
                assert((seq![x] + ls)[j] == ls[j - 1]);
                assert((seq![x] + ls)[j + 1] == ls[j]);
                sorted_at(ls, j - 1);
            }
        }
        assert(insert(x, ls).drop_first() == ls);
        assert(insert(x, ls)[0] == x);
        assert(insert(x, ls)[1] == ls[0]);
        assert(sorted(ls));
        assert(sorted(insert(x, ls)));
    } else if ls.len() == 1 {
        assert(ls.drop_first().len() == 0);
        assert(insert(x, ls.drop_first()) == seq![x]);
        assert(insert(x, ls) == seq![ls[0]] + seq![x]);
        lex_le_total(x, ls[0]);
        assert(lex_le(ls[0], x));
        assert((seq![ls[0]] + seq![x])[0] == ls[0]);
        assert((seq![ls[0]] + seq![x])[1] == x);
        assert(sorted(insert(x, ls)));
    } else {
        assert(insert(x, ls) == seq![ls[0]] + insert(x, ls.drop_first()));
        sorted_tail(ls);
        insert_sorted(x, ls.drop_first());
        lex_le_total(x, ls[0]);
        assert(insert(x, ls.drop_first()).len() > 0);
        sorted_at(ls, 0);
        assert(lex_le(ls[0], ls[1]));
        assert(ls.drop_first()[0] == ls[1]);
        insert_first_no_less(ls[0], x, ls.drop_first());
        assert(lex_le(ls[0], insert(x, ls.drop_first())[0]));
        assert forall|j: int| 0 <= j < insert(x, ls).len() - 1 implies
            #[trigger] lex_le(insert(x, ls)[j], insert(x, ls)[j + 1]) by {
            if j == 0 {
                assert(insert(x, ls)[0] == ls[0]);
                assert(insert(x, ls)[1] == insert(x, ls.drop_first())[0]);
            } else {
                assert(insert(x, ls)[j] == insert(x, ls.drop_first())[j - 1]);
                assert(insert(x, ls)[j + 1] == insert(x, ls.drop_first())[j]);
                sorted_at(insert(x, ls.drop_first()), j - 1);
            }
        }
        assert(insert(x, ls).drop_first() == insert(x, ls.drop_first()));
        assert(insert(x, ls)[0] == ls[0]);
        assert(insert(x, ls)[1] == insert(x, ls.drop_first())[0]);
        reveal_with_fuel(sorted, 2);
        assert(lex_le(insert(x, ls)[0], insert(x, ls)[1]));
        assert(sorted(insert(x, ls).drop_first()));
        assert(sorted(insert(x, ls)));
    }
}

proof fn isort_sorted(ls: Seq<Seq<nat>>)
    ensures sorted(isort(ls)),
    decreases ls.len(),
{
    if ls.len() == 0 {
    } else {
        isort_sorted(ls.drop_first());
        insert_sorted(ls[0], isort(ls.drop_first()));
    }
}

proof fn insert_perm(x: Seq<nat>, ls: Seq<Seq<nat>>)
    ensures same_multiset(insert(x, ls), seq![x] + ls),
    decreases ls.len(),
{
    if ls.len() == 0 {
    } else if lex_le(x, ls[0]) {
        assert(insert(x, ls) == seq![x] + ls);
    } else {
        assert(insert(x, ls) == seq![ls[0]] + insert(x, ls.drop_first()));
        insert_perm(x, ls.drop_first());
        assert forall|y: Seq<nat>| count_of(insert(x, ls), y) == count_of(seq![x] + ls, y) by {
            count_concat(seq![ls[0]], insert(x, ls.drop_first()), y);
            count_concat(seq![x], ls, y);
            count_concat(seq![x], ls.drop_first(), y);
            count_single(ls[0], y);
            count_single(x, y);
        }
    }
}

proof fn isort_perm(ls: Seq<Seq<nat>>)
    ensures same_multiset(isort(ls), ls),
    decreases ls.len(),
{
    if ls.len() == 0 {
    } else {
        isort_perm(ls.drop_first());
        insert_perm(ls[0], isort(ls.drop_first()));
        assert forall|y: Seq<nat>| count_of(isort(ls), y) == count_of(ls, y) by {
            reveal_with_fuel(count_of, 3);
            count_single(ls[0], y);
            count_concat(seq![ls[0]], isort(ls.drop_first()), y);
            assert(count_of(seq![ls[0]], y) + count_of(ls.drop_first(), y) == count_of(ls, y));
        }
    }
}

// the pass's flag parser: sort accepts only -r; -n/-h/anything else refuse
// the flags native_sort_stmt accepts for the plain-order fold: none at all,
// or -r. -n / -h / -nr / -rn / -hr / -rh take a different comparison and are
// modelled (and refused for this law) as such; anything else is refused by
// the source's `match` and keeps the shell-out.
pub open spec fn plain_sort_flag(w: Seq<char>) -> bool {
    w.len() == 0 || (w.len() == 2 && w[0] == '-' && w[1] == 'r')
}

// the two sides of the plain-order law: the pass insertion-sorts (isort),
// the semantic side is the relation "sorted and a permutation".
proof fn plain_sort_law(ls: Seq<Seq<nat>>)
    ensures sorted(isort(ls)), same_multiset(isort(ls), ls),
{
    isort_sorted(ls);
    isort_perm(ls);
}

proof fn pin_plain_sort_is_byte_order()
{
    let a: Seq<nat> = seq!['1' as nat, '0' as nat];
    let b: Seq<nat> = seq!['9' as nat];
    reveal_with_fuel(isort, 4);
    reveal_with_fuel(insert, 4);
    assert(isort(seq![b, a]) == seq![a, b]);
}

proof fn pin_numeric_order_differs_from_plain_order()
{
    // plain order puts "10" before "9"; numeric order would put 9 first.
    let ten: Seq<nat> = seq!['1' as nat, '0' as nat];
    let nine: Seq<nat> = seq!['9' as nat];
    assert(lex_le(ten, nine));
    assert(!lex_le(nine, ten));
}

proof fn pin_unknown_sort_flags_keep_the_shell_out()
{
    assert(seq!['-', 'u'][1] == 'u');
    assert(seq!['-', 'r'][1] == 'r');
    assert(!plain_sort_flag(seq!['-', 'u']));
    assert(seq!['-', '-', 'r'].len() == 3);
    assert(!plain_sort_flag(seq!['-', '-', 'r']));
    assert(seq!['-', 'k', '2'].len() == 3);
    assert(!plain_sort_flag(seq!['-', 'k', '2']));
    assert(seq!['f', 'i', 'l', 'e'].len() == 4);
    assert(!plain_sort_flag(seq!['f', 'i', 'l', 'e']));
    // -n and -h are accepted by the source but take a different comparison;
    // they are not plain-order flags.
    assert(!plain_sort_flag(seq!['-', 'n']));
    assert(!plain_sort_flag(seq!['-', 'h']));
    assert(plain_sort_flag(seq!['-', 'r']));
}

// ── line-mode head/tail: the source's split/join vs bash's byte windows ──

// the split the fold performs: Rust `str::split('\n')` on a newline-
// terminated text recovers the lines and one trailing empty segment.
pub open spec fn split_from(t: Seq<char>, cur: Seq<char>) -> Seq<Seq<char>>
    decreases t.len(),
{
    if t.len() == 0 {
        seq![cur]
    } else if t[0] == '\n' {
        seq![cur] + split_from(t.drop_first(), Seq::<char>::empty())
    } else {
        split_from(t.drop_first(), cur + seq![t[0]])
    }
}

pub open spec fn split_nl(t: Seq<char>) -> Seq<Seq<char>> {
    split_from(t, Seq::<char>::empty())
}

pub open spec fn join_lines(ls: Seq<Seq<char>>) -> Seq<char>
    decreases ls.len(),
{
    if ls.len() == 0 {
        Seq::empty()
    } else if ls.len() == 1 {
        ls[0]
    } else {
        ls[0] + (seq!['\n'] + join_lines(ls.drop_first()))
    }
}

// the fold's line mode: join the selection, append one newline if non-empty
pub open spec fn source_lines(sel: Seq<Seq<char>>) -> Seq<char> {
    let j = join_lines(sel);
    if j.len() == 0 { j } else { j + seq!['\n'] }
}

// bash `head -n N`: copy whole lines from the byte stream until N complete
// concrete facts about the fold's line mode. The fold splits the producer on
// '\n' (keeping a trailing empty segment) and then joins the selection with
// '\n', appending one newline if the join is non-empty. These pins record what
// that emits; the byte-level equality with bash is NOT proved (see header).
proof fn pin_head_one_line()
{
    reveal_with_fuel(join_lines, 2);
    assert(source_lines(seq![seq!['a']]) == seq!['a', '\n']);
}

proof fn pin_head_beyond_the_last_line_adds_a_newline()
{
    reveal_with_fuel(join_lines, 2);
    // "echo a | head -5": split gives ["a", ""], head keeps both, the join is
    // "a\n" and the fold appends another newline: "a\n\n".
    assert(join_lines(seq![seq!['a'], Seq::<char>::empty()]) == seq!['a', '\n']);
    assert(source_lines(seq![seq!['a'], Seq::<char>::empty()]) == seq!['a', '\n', '\n']);
}

proof fn pin_tail_lines_selects_the_trailing_empty_segment()
{
    reveal_with_fuel(join_lines, 2);
    // "echo a | tail -1": split gives ["a", ""], tail -1 keeps the empty last
    // segment, so the fold prints nothing (bash prints "a\n").
    assert(source_lines(seq![Seq::<char>::empty()]) == Seq::<char>::empty());
}

proof fn pin_tail_lines_add_a_newline_bash_does_not()
{
    reveal_with_fuel(join_lines, 2);
    // "printf 'a\nb' | tail -1": split gives ["a", "b"]; bash prints "b"
    // (unterminated), the fold prints "b\n".
    assert(source_lines(seq![seq!['b']]) == seq!['b', '\n']);
}


fn main() {}

} // verus!
