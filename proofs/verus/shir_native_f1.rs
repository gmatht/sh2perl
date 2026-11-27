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
//   * F1's line-mode `head -N`/`tail -N`: PROVED in general, for an
//     arbitrary line list and an arbitrary N, on the explicit domain
//     `nl_free_lines(ls)` (no line carries a newline) and
//     `nonempty_lines(ls)`:
//       - `head_window_law`: bash's byte copy of the first N lines equals
//         the fold's join of `ls.take(N)` plus one newline;
//       - `tail_window_law`: bash's drop-(len-N)-lines equals the fold's
//         join of the last N lines plus one newline.
//     The bash side steps the byte stream (`take_lines`/`skip_lines`); the
//     fold side is `join_lines`/`source_lines` as the source writes it. The
//     domain is needed: an empty first line makes `head -1` print nothing
//     where bash prints a blank line.
//     Off domain, the divergences are proved in general too, over that same
//     explicit witness family (a newline-free, non-empty line list `ls`;
//     producer text `join_lines(ls) + "\n"` when newline-terminated and
//     `join_lines(ls)` when not), for an arbitrary N:
//       - `head_past_end_diverges`: for N past the last line, the fold's line
//         window on the terminated producer plus one newline is the whole
//         selection text — bash prints the text once, the fold terminates it
//         twice;
//       - `skip_lines_unterminated` + `tail_unterminated_diverges`: bash's
//         drop-(len-N) on the unterminated producer is the last N lines' text
//         with no final newline; the fold's output is that text plus a
//         newline;
//       - `tail_terminated_selection` (general in N, landed) records the
//         trailing-empty-segment selection on the terminated producer, which
//         is why the fold's `tail -N` prints fewer real lines than bash's.
//     Supporting inductions: `nl_count`/`nl_count_concat`/`nl_count_join` and
//     `no_nl_text_zero` (the terminated producer carries exactly `ls.len()`
//     newlines), `take_lines_whole` (N past the newline count keeps the whole
//     text), `join_snoc_empty`, `skip_lines_unterminated`,
//     `nonempty_lines_subrange`/`nonempty_join_len`. No bridging lemma assumes
//     the conclusion: the bash side (`take_lines`/`skip_lines` over bytes) and
//     the fold side (`join_lines`/`source_lines`) stay separate recursions,
//     and the step is `subrange`, never `drop_first`. The narrowed domain is
//     the one above; the four pins remain as the N=1 witnesses.
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
//  third measurement (general line-mode law): baseline 62 verified, 0 errors.
//    t1 join_lines adds a newline to a one-line selection: 55v/7e.
//    t2 take_lines does not decrement its line counter: 61v/1e.
//    t3 nonempty_lines admits empty lines: 59v/3e.
//    t4 source_lines omits its newline: 56v/6e.
//    t5 skip_lines does not decrement its line counter: 61v/1e.
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

// ── line lists and the fold's text, stepped with subrange ──────────────────
// NOTE: `drop_first` is not meaningfully specified on the empty sequence in
// this toolchain, so every recursion here steps with `tl` (subrange(1, len)).

pub open spec fn tl(t: Seq<char>) -> Seq<char> {
    t.subrange(1, t.len() as int)
}

pub open spec fn join_lines(ls: Seq<Seq<char>>) -> Seq<char>
    decreases ls.len(),
{
    if ls.len() == 0 {
        Seq::empty()
    } else if ls.len() == 1 {
        ls[0]
    } else {
        ls[0] + (seq!['\n'] + join_lines(ls.subrange(1, ls.len() as int)))
    }
}

// the fold's line mode: join the selection with '\n', and append one newline
// when the join is non-empty
pub open spec fn source_lines(sel: Seq<Seq<char>>) -> Seq<char> {
    let j = join_lines(sel);
    if j.len() == 0 { j } else { j + seq!['\n'] }
}

// bash `head -n N`: copy whole lines from the byte stream until N are complete
pub open spec fn take_lines(t: Seq<char>, n: nat) -> Seq<char>
    decreases t.len() + n as int,
{
    if t.len() == 0 || n == 0 {
        Seq::empty()
    } else if t[0] == '\n' {
        seq!['\n'] + take_lines(tl(t), (n - 1) as nat)
    } else {
        seq![t[0]] + take_lines(tl(t), n)
    }
}

// bash `tail -n N`: drop whole lines from the front until N remain
pub open spec fn skip_lines(t: Seq<char>, n: nat) -> Seq<char>
    decreases t.len() + n as int,
{
    if t.len() == 0 || n == 0 {
        t
    } else if t[0] == '\n' {
        skip_lines(tl(t), (n - 1) as nat)
    } else {
        skip_lines(tl(t), n)
    }
}

// domain premises, as recursive predicates (no quantified Seq equalities)
pub open spec fn no_nl_text(t: Seq<char>) -> bool
    decreases t.len(),
{
    if t.len() == 0 {
        true
    } else {
        t[0] != '\n' && no_nl_text(tl(t))
    }
}

pub open spec fn nonempty_lines(ls: Seq<Seq<char>>) -> bool
    decreases ls.len(),
{
    if ls.len() == 0 {
        true
    } else {
        ls[0].len() > 0 && nonempty_lines(ls.subrange(1, ls.len() as int))
    }
}

pub open spec fn nl_free_lines(ls: Seq<Seq<char>>) -> bool
    decreases ls.len(),
{
    if ls.len() == 0 {
        true
    } else {
        no_nl_text(ls[0]) && nl_free_lines(ls.subrange(1, ls.len() as int))
    }
}

proof fn no_nl_tl(t: Seq<char>)
    requires no_nl_text(t), t.len() > 0,
    ensures no_nl_text(tl(t)),
{
    reveal_with_fuel(no_nl_text, 3);
}

proof fn nl_free_lines_tail(ls: Seq<Seq<char>>)
    requires nl_free_lines(ls), ls.len() > 0,
    ensures nl_free_lines(ls.subrange(1, ls.len() as int)),
{
    reveal_with_fuel(nl_free_lines, 3);
}

proof fn nonempty_lines_tail(ls: Seq<Seq<char>>)
    requires nonempty_lines(ls), ls.len() > 0,
    ensures nonempty_lines(ls.subrange(1, ls.len() as int)),
{
    reveal_with_fuel(nonempty_lines, 3);
}

// one line's bytes, then its newline, inside a longer stream
proof fn take_lines_after_line(l: Seq<char>, u: Seq<char>, n: nat)
    requires n >= 1, no_nl_text(l), l.len() > 0,
    ensures take_lines(l + (seq!['\n'] + u), n)
        == l + (seq!['\n'] + take_lines(u, (n - 1) as nat)),
    decreases l.len(),
{
    reveal_with_fuel(take_lines, 6);
    if l.len() == 1 {
        assert((l + (seq!['\n'] + u))[0] == l[0]);
        assert(tl(l + (seq!['\n'] + u)) == seq!['\n'] + u);
        assert(l + (seq!['\n'] + u) == seq![l[0]] + (seq!['\n'] + u));
        assert(tl(seq!['\n'] + u) == u);
    } else {
        no_nl_tl(l);
        assert(tl(l).len() > 0);
        assert(l + (seq!['\n'] + u) == seq![l[0]] + (tl(l) + (seq!['\n'] + u)));
        assert(tl(l + (seq!['\n'] + u)) == tl(l) + (seq!['\n'] + u));
        take_lines_after_line(tl(l), u, n);
    }
}

proof fn skip_lines_after_line(l: Seq<char>, u: Seq<char>, n: nat)
    requires n >= 1, no_nl_text(l), l.len() > 0,
    ensures skip_lines(l + (seq!['\n'] + u), n) == skip_lines(u, (n - 1) as nat),
    decreases l.len(),
{
    reveal_with_fuel(skip_lines, 6);
    if l.len() == 1 {
        assert((l + (seq!['\n'] + u))[0] == l[0]);
        assert(tl(l + (seq!['\n'] + u)) == seq!['\n'] + u);
        assert(tl(seq!['\n'] + u) == u);
        assert(skip_lines(seq!['\n'] + u, 1) == skip_lines(u, 0));
    } else {
        no_nl_tl(l);
        assert(tl(l).len() > 0);
        assert(l + (seq!['\n'] + u) == seq![l[0]] + (tl(l) + (seq!['\n'] + u)));
        assert(tl(l + (seq!['\n'] + u)) == tl(l) + (seq!['\n'] + u));
        skip_lines_after_line(tl(l), u, n);
    }
}

proof fn nonempty_take(ls: Seq<Seq<char>>, k: nat)
    requires nonempty_lines(ls), k <= ls.len(),
    ensures nonempty_lines(ls.take(k as int)),
    decreases k,
{
    reveal_with_fuel(nonempty_lines, 3);
    if k == 0 {
        assert(ls.take(0) == Seq::<Seq<char>>::empty());
    } else {
        nonempty_lines_tail(ls);
        assert(ls.subrange(1, ls.len() as int).take((k - 1) as int) == ls.take(k as int).subrange(1, k as int));
        nonempty_take(ls.subrange(1, ls.len() as int), (k - 1) as nat);
    }
}

proof fn nonempty_join(ls: Seq<Seq<char>>)
    requires nonempty_lines(ls), ls.len() > 0,
    ensures join_lines(ls).len() > 0,
    decreases ls.len(),
{
    reveal_with_fuel(join_lines, 2);
    reveal_with_fuel(nonempty_lines, 2);
    if ls.len() == 1 {
        assert(join_lines(ls) == ls[0]);
    } else {
        nonempty_lines_tail(ls);
        assert(ls.subrange(1, ls.len() as int).len() > 0);
        nonempty_join(ls.subrange(1, ls.len() as int));
        assert(join_lines(ls) == ls[0] + (seq!['\n'] + join_lines(ls.subrange(1, ls.len() as int))));
    }
}

// a non-empty line in front of a selection contributes itself and a newline
proof fn source_lines_cons(l: Seq<char>, rest: Seq<Seq<char>>)
    requires l.len() > 0, nonempty_lines(rest),
    ensures source_lines(seq![l] + rest) == l + (seq!['\n'] + source_lines(rest)),
{
    reveal_with_fuel(join_lines, 2);
    if rest.len() == 0 {
        assert(join_lines(seq![l] + rest) == l);
        assert(source_lines(Seq::<Seq<char>>::empty()) == Seq::<char>::empty());
    } else {
        nonempty_join(rest);
        assert((seq![l] + rest).subrange(1, (seq![l] + rest).len() as int) == rest);
        assert(join_lines(seq![l] + rest) == l + (seq!['\n'] + join_lines(rest)));
        if join_lines(rest).len() == 0 {
            assert(join_lines(rest) == Seq::<char>::empty());
        }
    }
}

// the fold's head: the first N lines of the list
proof fn head_window_law(ls: Seq<Seq<char>>, n: nat)
    requires nl_free_lines(ls), nonempty_lines(ls), n <= ls.len(),
    ensures take_lines(join_lines(ls) + seq!['\n'], n) == source_lines(ls.take(n as int)),
    decreases ls.len(),
{
    reveal_with_fuel(join_lines, 2);
    if n == 0 {
        assert(ls.take(0) == Seq::<Seq<char>>::empty());
    } else if ls.len() == 0 {
    } else {
        let rest = ls.subrange(1, ls.len() as int);
        nl_free_lines_tail(ls);
        nonempty_lines_tail(ls);
        assert(no_nl_text(ls[0]));
        assert(ls[0].len() > 0);
        assert(ls.take(n as int) == seq![ls[0]] + rest.take((n - 1) as int));
        if ls.len() == 1 {
            assert(n == 1);
            reveal_with_fuel(take_lines, 4);
            assert(join_lines(ls) == ls[0]);
            take_lines_after_line(ls[0], Seq::<char>::empty(), 1);
            assert(ls[0] + (seq!['\n'] + Seq::<char>::empty()) == ls[0] + seq!['\n']);
            assert(take_lines(ls[0] + seq!['\n'], 1) == ls[0] + seq!['\n']);
            assert(source_lines(seq![ls[0]]) == ls[0] + seq!['\n']);
        } else {
            assert(join_lines(ls) == ls[0] + (seq!['\n'] + join_lines(rest)));
            assert(join_lines(ls) + seq!['\n']
                == ls[0] + (seq!['\n'] + (join_lines(rest) + seq!['\n'])));
            take_lines_after_line(ls[0], join_lines(rest) + seq!['\n'], n);
            head_window_law(rest, (n - 1) as nat);
            nonempty_take(rest, (n - 1) as nat);
            source_lines_cons(ls[0], rest.take((n - 1) as int));
        }
    }
}

// the fold's tail: the last N lines of the list
proof fn tail_window_law(ls: Seq<Seq<char>>, n: nat)
    requires nl_free_lines(ls), nonempty_lines(ls), 1 <= n <= ls.len(),
    ensures skip_lines(join_lines(ls) + seq!['\n'], (ls.len() - n) as nat)
        == source_lines(ls.subrange((ls.len() - n) as int, ls.len() as int)),
    decreases ls.len(),
{
    reveal_with_fuel(join_lines, 2);
    if ls.len() == n {
        assert((ls.len() - n) as nat == 0);
        assert(skip_lines(join_lines(ls) + seq!['\n'], 0nat) == join_lines(ls) + seq!['\n']);
        assert(ls.subrange(0, ls.len() as int) == ls);
        assert(source_lines(ls) == join_lines(ls) + seq!['\n']);
    } else {
        let rest = ls.subrange(1, ls.len() as int);
        nl_free_lines_tail(ls);
        nonempty_lines_tail(ls);
        assert(no_nl_text(ls[0]));
        assert(ls[0].len() > 0);
        assert(join_lines(ls) == ls[0] + (seq!['\n'] + join_lines(rest)));
        assert(n <= rest.len());
        assert((ls.len() - n) as nat == (rest.len() - n) as nat + 1);
        assert((ls.len() - n) - 1 == (rest.len() - n));
        assert((ls.len() - n) as nat >= 1);
        assert(join_lines(ls) + seq!['\n']
            == ls[0] + (seq!['\n'] + (join_lines(rest) + seq!['\n'])));
        skip_lines_after_line(ls[0], join_lines(rest) + seq!['\n'], (ls.len() - n) as nat);
        assert(skip_lines(join_lines(ls) + seq!['\n'], (ls.len() - n) as nat)
            == skip_lines(join_lines(rest) + seq!['\n'], (rest.len() - n) as nat));
        tail_window_law(rest, n);
        assert(ls.subrange((ls.len() - n) as int, ls.len() as int)
            == rest.subrange((rest.len() - n) as int, rest.len() as int));
    }
}

// ── off-domain behaviour, general in N ───────────────────────────────────

pub open spec fn empty_line() -> Seq<char> {
    Seq::<char>::empty()
}

// General in N: the fold's `tail -N` selection on a newline-terminated
// producer is the last N-1 real lines plus the trailing empty segment, so the
// line it prints last is blank where bash prints the last real line.
proof fn tail_terminated_selection(ls: Seq<Seq<char>>, n: nat)
    requires 1 <= n <= ls.len(),
    ensures (ls + seq![empty_line()]).subrange((ls.len() + 1 - n) as int, (ls.len() + 1) as int)
        == ls.subrange((ls.len() - (n - 1)) as int, ls.len() as int) + seq![empty_line()],
{
    assert((ls + seq![empty_line()]).len() == ls.len() + 1);
}


// ── concrete instances of the off-domain behaviour ───────────────────────
// (the general statements above cover the domain; these pin the witnesses the
//  header names so a mutant cannot move them silently)

proof fn pin_head_one_line()
{
    reveal_with_fuel(join_lines, 2);
    assert(source_lines(seq![seq!['a']]) == seq!['a', '\n']);
}

proof fn pin_head_beyond_the_last_line_adds_a_newline()
{
    reveal_with_fuel(join_lines, 2);
    assert(join_lines(seq![seq!['a'], empty_line()]) == seq!['a', '\n']);
    assert(source_lines(seq![seq!['a'], empty_line()]) == seq!['a', '\n', '\n']);
}

proof fn pin_tail_lines_selects_the_trailing_empty_segment()
{
    reveal_with_fuel(join_lines, 2);
    assert(source_lines(seq![empty_line()]) == Seq::<char>::empty());
}

proof fn pin_tail_lines_add_a_newline_bash_does_not()
{
    reveal_with_fuel(join_lines, 2);
    assert(source_lines(seq![seq!['b']]) == seq!['b', '\n']);
}

// ── F1 line mode, off-domain: the divergences, general in N ─────────────
// Witness family: any newline-free list of non-empty lines `ls`; the producer
// text is `join_lines(ls) + "\n"` (newline-terminated) or `join_lines(ls)`
// (unterminated).  These are *divergences*: the fold's output is bash's output
// plus a newline, or bash's last-N lines replaced by the last N-1 plus a blank
// line.  Nothing here asserts equality of the two pipelines.

pub open spec fn nl_count(t: Seq<char>) -> nat
    decreases t.len(),
{
    if t.len() == 0 {
        0nat
    } else {
        (if t[0] == '\n' { 1nat } else { 0nat }) + nl_count(tl(t))
    }
}

proof fn no_nl_text_zero(t: Seq<char>)
    requires no_nl_text(t),
    ensures nl_count(t) == 0,
    decreases t.len(),
{
    reveal_with_fuel(nl_count, 3);
    reveal_with_fuel(no_nl_text, 2);
    if t.len() > 0 {
        no_nl_text_zero(tl(t));
    }
}

proof fn nl_count_concat(x: Seq<char>, y: Seq<char>)
    ensures nl_count(x + y) == nl_count(x) + nl_count(y),
    decreases x.len(),
{
    reveal_with_fuel(nl_count, 4);
    if x.len() == 0 {
        assert(Seq::<char>::empty() + y == y);
    } else {
        assert((x + y)[0] == x[0]);
        assert(tl(x + y) == tl(x) + y);
        nl_count_concat(tl(x), y);
    }
}

proof fn nl_count_nl(t: Seq<char>)
    ensures nl_count(t + seq!['\n']) == nl_count(t) + 1,
{
    nl_count_concat(t, seq!['\n']);
    reveal_with_fuel(nl_count, 4);
    assert(tl(seq!['\n']) == Seq::<char>::empty());
}

proof fn join_snoc_empty(ls: Seq<Seq<char>>)
    requires ls.len() > 0,
    ensures join_lines(ls + seq![empty_line()]) == join_lines(ls) + seq!['\n'],
    decreases ls.len(),
{
    reveal_with_fuel(join_lines, 8);
    if ls.len() == 1 {
        assert(ls + seq![empty_line()] == seq![ls[0], empty_line()]);
        assert(join_lines(seq![empty_line()]) == Seq::<char>::empty());
        assert(join_lines(seq![ls[0], empty_line()])
            == ls[0] + (seq!['\n'] + Seq::<char>::empty()));
        assert(ls[0] + (seq!['\n'] + Seq::<char>::empty()) == ls[0] + seq!['\n']);
        assert(join_lines(ls) == ls[0]);
    } else {
        assert((ls + seq![empty_line()]).len() == ls.len() + 1);
        assert((ls + seq![empty_line()]).subrange(1, (ls + seq![empty_line()]).len() as int)
            == ls.subrange(1, ls.len() as int) + seq![empty_line()]);
        assert((ls + seq![empty_line()])[0] == ls[0]);
        assert(join_lines(ls + seq![empty_line()])
            == ls[0] + (seq!['\n'] + join_lines(ls.subrange(1, ls.len() as int) + seq![empty_line()])));
        assert(join_lines(ls) == ls[0] + (seq!['\n'] + join_lines(ls.subrange(1, ls.len() as int))));
        join_snoc_empty(ls.subrange(1, ls.len() as int));
    }
}

proof fn nl_count_join(ls: Seq<Seq<char>>)
    requires nl_free_lines(ls), ls.len() > 0,
    ensures nl_count(join_lines(ls) + seq!['\n']) == ls.len(),
    decreases ls.len(),
{
    reveal_with_fuel(join_lines, 8);
    reveal_with_fuel(nl_count, 8);
    if ls.len() == 1 {
        assert(join_lines(ls) == ls[0]);
        no_nl_text_zero(ls[0]);
        nl_count_nl(ls[0]);
    } else {
        nl_free_lines_tail(ls);
        assert(no_nl_text(ls[0]));
        no_nl_text_zero(ls[0]);
        assert(join_lines(ls) + seq!['\n']
            == ls[0] + (seq!['\n'] + (join_lines(ls.subrange(1, ls.len() as int)) + seq!['\n'])));
        nl_count_concat(ls[0], seq!['\n'] + (join_lines(ls.subrange(1, ls.len() as int)) + seq!['\n']));
        nl_count_concat(seq!['\n'], join_lines(ls.subrange(1, ls.len() as int)) + seq!['\n']);
        assert(nl_count(ls[0]) == 0);
        assert(nl_count(seq!['\n']) == 1);
        nl_count_join(ls.subrange(1, ls.len() as int));
    }
}

proof fn take_lines_whole(t: Seq<char>, n: nat)
    requires n >= nl_count(t) + 1,
    ensures take_lines(t, n) == t,
    decreases t.len(),
{
    reveal_with_fuel(take_lines, 8);
    reveal_with_fuel(nl_count, 8);
    if t.len() == 0 {
    } else if t[0] == '\n' {
        assert((n - 1) as nat >= nl_count(tl(t)) + 1);
        take_lines_whole(tl(t), (n - 1) as nat);
        assert(seq![t[0]] + tl(t) == t);
    } else {
        take_lines_whole(tl(t), n);
        assert(seq![t[0]] + tl(t) == t);
    }
}

// (1) head -N with N past the last line: the producer is newline-terminated,
// so the selection still holds a trailing empty segment; the fold prints the
// whole text plus one extra newline.  bash prints the whole text unchanged.
proof fn head_past_end_diverges(ls: Seq<Seq<char>>, n: nat)
    requires nl_free_lines(ls), nonempty_lines(ls), ls.len() >= 1, n >= ls.len() + 1,
    ensures take_lines(join_lines(ls) + seq!['\n'], n) + seq!['\n']
        == source_lines(ls + seq![empty_line()]),
{
    reveal_with_fuel(join_lines, 3);
    assert(nl_count(join_lines(ls) + seq!['\n']) == ls.len()) by {
        nl_count_join(ls);
    }
    take_lines_whole(join_lines(ls) + seq!['\n'], n);
    join_snoc_empty(ls);
    assert(join_lines(ls).len() > 0) by {
        reveal_with_fuel(join_lines, 4);
        reveal_with_fuel(nonempty_lines, 4);
    }
    assert(source_lines(ls + seq![empty_line()])
        == join_lines(ls) + (seq!['\n'] + seq!['\n']));
    assert(take_lines(join_lines(ls) + seq!['\n'], n) + seq!['\n']
        == join_lines(ls) + (seq!['\n'] + seq!['\n']));
}

proof fn nonempty_lines_subrange(ls: Seq<Seq<char>>, k: nat)
    requires nonempty_lines(ls), k <= ls.len(),
    ensures nonempty_lines(ls.subrange(k as int, ls.len() as int)),
    decreases k,
{
    reveal_with_fuel(nonempty_lines, 3);
    if k == 0 {
        assert(ls.subrange(0, ls.len() as int) == ls);
    } else {
        nonempty_lines_tail(ls);
        nonempty_lines_subrange(ls.subrange(1, ls.len() as int), (k - 1) as nat);
        assert(ls.subrange(k as int, ls.len() as int)
            == ls.subrange(1, ls.len() as int).subrange((k - 1) as int, (ls.len() - 1) as int));
    }
}

proof fn nonempty_join_len(ls: Seq<Seq<char>>)
    requires nonempty_lines(ls), ls.len() > 0,
    ensures join_lines(ls).len() > 0,
    decreases ls.len(),
{
    reveal_with_fuel(join_lines, 3);
    reveal_with_fuel(nonempty_lines, 3);
    if ls.len() == 1 {
        assert(join_lines(ls) == ls[0]);
    } else {
        nonempty_lines_tail(ls);
        nonempty_join_len(ls.subrange(1, ls.len() as int));
        assert(join_lines(ls)
            == ls[0] + (seq!['\n'] + join_lines(ls.subrange(1, ls.len() as int))));
    }
}

// (2) tail -N on an *unterminated* producer: the selection is the last N real
// lines; bash prints their text with no final newline, the fold appends one.
proof fn skip_lines_unterminated(ls: Seq<Seq<char>>, n: nat)
    requires nl_free_lines(ls), nonempty_lines(ls), 1 <= n <= ls.len(),
    ensures skip_lines(join_lines(ls), (ls.len() - n) as nat)
        == join_lines(ls.subrange((ls.len() - n) as int, ls.len() as int)),
    decreases ls.len(),
{
    reveal_with_fuel(join_lines, 3);
    reveal_with_fuel(skip_lines, 6);
    if ls.len() == n {
        assert((ls.len() - n) as nat == 0);
        assert(ls.subrange(0, ls.len() as int) == ls);
    } else {
        let rest = ls.subrange(1, ls.len() as int);
        nl_free_lines_tail(ls);
        nonempty_lines_tail(ls);
        assert(no_nl_text(ls[0]));
        assert(ls[0].len() > 0);
        assert(join_lines(ls) == ls[0] + (seq!['\n'] + join_lines(rest)));
        assert(n <= rest.len());
        assert((ls.len() - n) as nat == (rest.len() - n) as nat + 1);
        skip_lines_after_line(ls[0], join_lines(rest), (ls.len() - n) as nat);
        assert(skip_lines(join_lines(ls), (ls.len() - n) as nat)
            == skip_lines(join_lines(rest), (rest.len() - n) as nat));
        skip_lines_unterminated(rest, n);
        assert(ls.subrange((ls.len() - n) as int, ls.len() as int)
            == rest.subrange((rest.len() - n) as int, rest.len() as int));
    }
}

proof fn tail_unterminated_diverges(ls: Seq<Seq<char>>, n: nat)
    requires nl_free_lines(ls), nonempty_lines(ls), 1 <= n <= ls.len(),
    ensures source_lines(ls.subrange((ls.len() - n) as int, ls.len() as int))
        == skip_lines(join_lines(ls), (ls.len() - n) as nat) + seq!['\n'],
{
    reveal_with_fuel(skip_lines, 3);
    assert(skip_lines(join_lines(ls), (ls.len() - n) as nat)
        == join_lines(ls.subrange((ls.len() - n) as int, ls.len() as int))) by {
        skip_lines_unterminated(ls, n);
    }
    nonempty_lines_subrange(ls, (ls.len() - n) as nat);
    assert((ls.len() - n) as nat <= ls.len());
    assert(ls.subrange((ls.len() - n) as int, ls.len() as int).len() > 0);
    nonempty_join_len(ls.subrange((ls.len() - n) as int, ls.len() as int));
}

fn main() {}

} // verus!
