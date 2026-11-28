// F4 — `xargs` and `comm`: what the emitted Perl means.
//
// The folds in `src/transforms/shir_native_stmt.rs`:
//   native_echo_xargs_stmt (5290)   `echo CONTENT | xargs [flags] echo A B ..`
//   native_comm_stmt (6756)         `comm -12 F1 F2`
//   native_cat_file_pipeline (2616) producer + slurp stages over a literal blob
//   native_diff_files_stmt (5827)   `diff F1 F2 [> OUT]`
//
// Two-mode modelling, as in `shir_native_files_invariant.rs`: the SHELL side is
// written from `sh`/GNU semantics (`xargs` splits stdin into words, `echo` joins
// its arguments with single spaces and one newline, `comm` is a two-pointer
// merge), the PASS side is the emitted Perl text. The laws live where the two
// agree; where they do not, the divergence is PROVED (d-F4-1, d-F4-2) rather
// than left as a comment.
//
// Proved here (94 obligations, 0 errors):
//   xargs_appends_the_input_word — the general agreement law: `echo CONTENT |
//                                xargs echo A B` joins A, B, CONTENT exactly the
//                                way `echo` joins the command's argv with the
//                                stdin word appended. Stated on the index-level
//                                join with `push`, so the induction needs only
//                                the `push` index axioms and concat associativity
//                                (the earlier `skip`-based formulation is what
//                                stranded — see the note below).
//   xargs_splits_on_whitespace   d-F4-1: content with a double space prints one
//                                space through the shell and two through the fold
//   xargs_empty_content_diverges d-F4-2: `echo "" | xargs echo A` — the fold
//                                emits `A<space>`, the shell emits `A`
//   comm_12_one_common_line      the emitted `-12` walk prints an equal line once
//                                and advances BOTH cursors (the double advance,
//                                which is what makes duplicates print once)
//   diff_identical_inputs_are_silent — d-F4-5: on equal files the backtrack only
//                                takes the prefix-match branch, so no hunk is
//                                opened and nothing is printed; the DP diagonal
//                                is the file length (lemma_lcs_identical), which
//                                is what makes that branch always reachable.
//   sort_is_a_sorted_permutation — d-F4-3: the emitted `sort @lines` output is
//                                ORDERED (`sorted`, the adjacent-pair model) and
//                                a PERMUTATION of its input (`cnt`, the multiset
//                                count model); `pass_sort`/`ins` are the pass's
//                                insertion step, and neither `sorted` nor `cnt`
//                                is derived from them. Pins: a two-line file
//                                sorts to [A,B] and a three-line file keeps its
//                                duplicate's multiplicity.
//   ls_fallback_prints_iff_missing — d-F4-3: the `ls` fold's status accumulator
//                                (`scan_rc`, the emitted `$__ls_rc` loop) equals
//                                the semantic status (`ls_rc` from the OR-fold
//                                `any_missing`), and the pipeline's `|| echo`
//                                guard (`prints_echo`) fires exactly when an
//                                operand is missing. Pins: a missing operand
//                                prints, an all-present list stays silent.
//   sort_n_is_a_sorted_key_permutation — d-F4-3b: the `-n` fold's output is
//                                ordered by `num_key` (the line's decimal value,
//                                0 for a line that is not a number — the
//                                `unwrap_or(0.0)` fallback) and holds the input's
//                                lines with their multiplicities. `num_key` is
//                                built from `is_int_lit`/`digits_val`, not from
//                                `ins_key`. Pins: `10`/`9` come out 9-then-10
//                                (byte order would leave them the other way),
//                                and `1`/`01` keep their input order (stability).
//   sort_r_is_a_descending_permutation — d-F4-3b: byte-order sort then the
//                                emitted `lines.reverse()` leaves no ascending
//                                adjacent pair (`desc_lex`) and is a permutation.
//                                `rev` is its own `push` recursion, and the law
//                                is about the composition, not about `rev` alone.
//   sort_nr_is_a_descending_key_permutation — d-F4-3b: the numeric sort reversed
//                                is key-descending and a permutation. The glued
//                                `-nr`/`-rn` spelling and the fold's refusal are
//                                checked by `decode_flags` pins.
//
// NOT proved, stated plainly rather than papered over:
//   * the general `comm -12` intersection law over two distinct sorted files —
//     needs ordered-membership invariants for the two-pointer walk (d-F4-4).
//   * diff's output beyond silence — d-F4-5b: the converse ("a change always
//     prints") and the byte-level hunks. Both need an invariant relating the
//     backtrack's cursors to prefix equality (`a[0..i] == b[0..j]`), i.e.
//     subrange-composition facts this proof does not establish; the general
//     statement was left unproved rather than restated so it agrees with itself.
//   * `sort`'s float spellings (`1.5`, `1e3`), `-h`'s `human_size` key, and
//     `-u`/`-k`/`-t` (those three are refused by the fold itself): §7 proves
//     `-n` over the integer fragment of `parse::<f64>` and `-r` over byte
//     order, and nothing is claimed for the rest — d-F4-3c. The byte-level join
//     of the sorted lines (separator/trailing newline) is the
//     `join_sep`/`echo_bytes` model of §1, not restated for sort.
//
// The xargs law's first formulation recursed with `skip` and needed
// `(ws + [c]).skip(1) == ws.skip(1) + [c]`; that rewrite does not discharge
// here, so the law is stated with `push` and a `push`-index induction instead —
// the same claim, a formulation the solver accepts.

use vstd::prelude::*;

verus! {

// ──────────────────────────────────────── §1  words, separators, newlines ──

pub type Word = Seq<int>;

/// `echo`'s argument join, as an index walk: a single space between consecutive
/// words (index recursion, so it reduces without a `skip` axiom).
pub open spec fn join_at(ws: Seq<Word>, sep: int, i: int) -> Seq<int>
    decreases ws.len() - i,
{
    if i >= ws.len() as int {
        Seq::empty()
    } else if i == ws.len() - 1 {
        ws[i]
    } else {
        ws[i] + seq![sep] + join_at(ws, sep, i + 1)
    }
}

pub open spec fn join_sep(ws: Seq<Word>, sep: int) -> Seq<int> {
    join_at(ws, sep, 0)
}

/// What `echo` writes for its arguments.
pub open spec fn echo_bytes(ws: Seq<Word>) -> Seq<int> {
    join_sep(ws, 32) + seq![10int]
}

/// **d-F4-1.** `xargs` splits stdin on ANY run of blanks and `echo` then joins
/// the pieces with ONE space. The fold copies the content through verbatim, so a
/// double space survives the fold and collapses through the shell. Witness: the
/// words `a` and `b` (bytes 97, 98) written as the content `a  b`.
pub proof fn xargs_splits_on_whitespace()
    ensures
        // the shell: xargs' words are `a`, `b`; echo joins them with one space
        echo_bytes(seq![seq![97int], seq![98int]]) == seq![97int, 32int, 98int, 10int],
        join_sep(seq![seq![97int], seq![98int]], 32) == seq![97int, 32int, 98int],
        // the fold copies the content's own two spaces through: different bytes
        seq![97int, 32int, 32int, 98int, 10int] != seq![97int, 32int, 98int, 10int],
{
    assert(echo_bytes(seq![seq![97int], seq![98int]]) == seq![97int, 32int, 98int, 10int]) by (compute);
    assert(join_sep(seq![seq![97int], seq![98int]], 32) == seq![97int, 32int, 98int]) by (compute);
    assert(seq![97int, 32int, 32int, 98int, 10int] != seq![97int, 32int, 98int, 10int]) by (compute);
}

/// **d-F4-2.** `echo "" | xargs echo A` runs `echo A` (no stdin word), while the
/// fold always inserts its own separator: `join(' ', A), " ", "", "\n"`.
/// Witness: the command word `A` (byte 65).
pub proof fn xargs_empty_content_diverges()
    ensures
        // shell: `echo A`
        echo_bytes(seq![seq![65int]]) == seq![65int, 10int],
        // fold: `A` . " " . "" . "\n"
        join_sep(seq![seq![65int]], 32) + seq![32int] + Seq::<int>::empty() + seq![10int] != seq![65int, 10int],
{
    assert(echo_bytes(seq![seq![65int]]) == seq![65int, 10int]) by (compute);
    assert(join_sep(seq![seq![65int]], 32) + seq![32int] + Seq::<int>::empty() + seq![10int]
        != seq![65int, 10int]) by (compute);
}

pub proof fn lemma_concat_assoc(a: Seq<int>, b: Seq<int>, c: Seq<int>)
    ensures
        a + (b + c) == (a + b) + c,
{
}

/// **The xargs law.** `echo CONTENT | xargs echo A B` appends the stdin word to
/// the command's arguments, and `echo` joins the result. Formulated on the
/// index-level join with `push`, so the induction needs only the `push` index
/// axioms and concat associativity — not the skip/concat rewrite that stranded
/// the first attempt.
pub proof fn xargs_appends_the_input_word(cmd: Seq<Word>, content: Word, i: int)
    requires
        cmd.len() >= 1,
        0 <= i <= cmd.len(),
    ensures
        join_at(cmd.push(content), 32, i) == (if i < cmd.len() {
            join_at(cmd, 32, i) + seq![32int]
        } else {
            Seq::<int>::empty()
        }) + content,
    decreases cmd.len() - i,
{
    if i >= cmd.len() as int {
        // the appended word is the last one: the fold stops here
        assert(cmd.push(content).len() == cmd.len() + 1);
        assert(cmd.push(content)[i] == content);
    } else {
        assert(cmd.push(content)[i] == cmd[i]);
        assert(cmd.push(content).len() == cmd.len() + 1);
        xargs_appends_the_input_word(cmd, content, i + 1);
        if i + 1 < cmd.len() as int {
            lemma_concat_assoc(
                join_at(cmd, 32, i + 1) + seq![32int],
                content,
                Seq::<int>::empty(),
            );
        }
    }
}

// ────────────────────────────────────────── §2  `comm -12 F1 F2` ──────────

/// The emitted two-pointer walk: equal lines are printed and BOTH cursors move;
/// the smaller line's cursor moves alone. `-12` is the only flag the fold takes.
pub open spec fn merge(a: Seq<Word>, b: Seq<Word>, i: int, j: int) -> Seq<Word>
    decreases (a.len() - i) + (b.len() - j),
{
    if i >= a.len() as int || j >= b.len() as int {
        Seq::empty()
    } else if lex_lt(a[i], b[j]) {
        merge(a, b, i + 1, j)
    } else if lex_lt(b[j], a[i]) {
        merge(a, b, i, j + 1)
    } else {
        seq![a[i]] + merge(a, b, i + 1, j + 1)
    }
}

/// Lexicographic order on a line's bytes — what `cmp` compares.
pub open spec fn lex_lt(x: Word, y: Word) -> bool
    decreases x.len(),
{
    if x.len() == 0 {
        y.len() > 0
    } else if y.len() == 0 {
        false
    } else if x[0] != y[0] {
        x[0] < y[0]
    } else {
        lex_lt(x.skip(1), y.skip(1))
    }
}

pub proof fn lemma_lex_not_lt(x: Word)
    ensures
        !lex_lt(x, x),
    decreases x.len(),
{
    if x.len() == 0 {
    } else {
        lemma_lex_not_lt(x.skip(1));
    }
}

/// **The double advance.** On a common line the walk pushes ONE copy and moves
/// both cursors — which is why `comm -12` prints a duplicate once.
pub proof fn comm_12_one_common_line(x: Word, a_tail: Seq<Word>, b_tail: Seq<Word>)
    ensures
        merge(seq![x] + a_tail, seq![x] + b_tail, 0, 0) == seq![x] + merge(
            seq![x] + a_tail,
            seq![x] + b_tail,
            1,
            1,
        ),
{
    lemma_lex_not_lt(x);
    assert(!lex_lt(x, x));
}


// ────────────────────────────────────── §3  `diff F1 F2 [> OUT]` ──────────

/// The emitted DP table: `dp[i][j]` is the LCS length of the prefixes
/// `a[0..i]` and `b[0..j]`, exactly as the fold's nested loop fills it (the
/// `>` in `($__dp[$__i][$__j+1] > $__dp[$__i+1][$__j]) ? … : …` only picks a
/// tie-break for the same value).
pub open spec fn lcs(a: Seq<Word>, b: Seq<Word>, i: int, j: int) -> int
    decreases i + j,
{
    if i <= 0 || j <= 0 {
        0
    } else if a[i - 1] == b[j - 1] {
        lcs(a, b, i - 1, j - 1) + 1
    } else if lcs(a, b, i - 1, j) >= lcs(a, b, i, j - 1) {
        lcs(a, b, i - 1, j)
    } else {
        lcs(a, b, i, j - 1)
    }
}

/// The emit loop's control flow over the backtrack: a prefix match prints
/// nothing and moves both cursors; any other step starts a hunk, and the two
/// tail loops emit the leftovers.
pub open spec fn prints_any(a: Seq<Word>, b: Seq<Word>, i: int, j: int) -> bool
    decreases i + j,
{
    if i <= 0 || j <= 0 {
        i > 0 || j > 0
    } else if a[i - 1] == b[j - 1] {
        prints_any(a, b, i - 1, j - 1)
    } else {
        true
    }
}

/// On equal files the backtrack only ever takes the match branch: every step
/// consumes one line of each side and prints nothing, down to `(0, 0)`.
pub proof fn lemma_prints_identical(a: Seq<Word>, i: int)
    requires
        0 <= i <= a.len(),
    ensures
        prints_any(a, a, i, i) == false,
    decreases i,
{
    if i <= 0 {
    } else {
        lemma_prints_identical(a, i - 1);
    }
}

/// The DP diagonal on equal files: `dp[i][i] = i`, which is why the walk's
/// `$__a[$__i] eq $__b[$__j]` test is true at every step.
pub proof fn lemma_lcs_identical(a: Seq<Word>, i: int)
    requires
        0 <= i <= a.len(),
    ensures
        lcs(a, a, i, i) == i,
    decreases i,
{
    if i <= 0 {
    } else {
        lemma_lcs_identical(a, i - 1);
    }
}

/// **The diff law (d-F4-5, the honest version).** `diff F F` prints nothing:
/// with both sides equal, the emit loop's only reachable branch is the prefix
/// match, and the DP diagonal is full, so no hunk is ever opened.
///
/// NOT proved (d-F4-5b): the converse — that a non-empty change always prints —
/// and `diff`'s byte-level output. Both need an invariant relating the
/// backtrack's cursors to prefix equality (`a[0..i] == b[0..j]`), i.e.
/// subrange-composition facts this proof does not establish; the general
/// statement was left unproved rather than restated so it agrees with itself.
pub proof fn diff_identical_inputs_are_silent(a: Seq<Word>)
    ensures
        prints_any(a, a, a.len() as int, a.len() as int) == false,
        lcs(a, a, a.len() as int, a.len() as int) == a.len() as int,
{
    lemma_prints_identical(a, a.len() as int);
    lemma_lcs_identical(a, a.len() as int);
}

// ────────────────────────────────────────────────── §4  pins ──────────────

/// pin: the `-12` walk of two files that both hold the line `A` prints it once.
pub proof fn pin_comm_one_line_each()
    ensures
        merge(seq![seq![65int]], seq![seq![65int]], 0, 0) == seq![seq![65int]],
{
    assert(merge(seq![seq![65int]], seq![seq![65int]], 0, 0) == seq![seq![65int]]) by (compute);
}

/// pin: a duplicate in the first file still prints once — the double advance.
pub proof fn pin_comm_duplicate_prints_once()
    ensures
        merge(seq![seq![65int], seq![65int]], seq![seq![65int]], 0, 0) == seq![seq![65int]],
{
    assert(merge(seq![seq![65int], seq![65int]], seq![seq![65int]], 0, 0)
        == seq![seq![65int]]) by (compute);
}

/// pin: a differing first line opens a hunk — the law is not "always silent".
pub proof fn pin_diff_of_differing_files_prints()
    ensures
        prints_any(seq![seq![65int]], seq![seq![66int]], 1, 1),
        prints_any(seq![seq![65int]], seq![seq![65int], seq![66int]], 1, 2),
        !prints_any(seq![seq![65int]], seq![seq![65int]], 1, 1),
{
    assert(prints_any(seq![seq![65int]], seq![seq![66int]], 1, 1)) by (compute);
    assert(prints_any(seq![seq![65int]], seq![seq![65int], seq![66int]], 1, 2)) by (compute);
    assert(!prints_any(seq![seq![65int]], seq![seq![65int]], 1, 1)) by (compute);
}

/// pin: one appended xargs word is the shell's argv join (the general law's
/// one-step instance).
pub proof fn pin_xargs_appends_one_word()
    ensures
        join_at(seq![seq![65int]].push(seq![66int]), 32, 0) == seq![65int, 32int, 66int],
{
    xargs_appends_the_input_word(seq![seq![65int]], seq![66int], 0);
    assert(join_at(seq![seq![65int]], 32, 0) == seq![65int]) by (compute);
}

/// pin: the fold's own separator is what a one-word echo writes.
pub proof fn pin_one_word_echo_is_the_word_plus_newline()
    ensures
        echo_bytes(seq![seq![65int]]) == seq![65int, 10int],
{
    assert(echo_bytes(seq![seq![65int]]) == seq![65int, 10int]) by (compute);
}

// ───────────────────────────────────────────── §5  `sort` ─────────────────
//
// `slurp_op`'s `sort` branch (shir_native_stmt.rs) emits `@lines = sort @lines;`
// for the plain form, and the pass re-emits the lines with `join("\n")` plus the
// trailing newline. The Perl code sorts by string comparison, i.e. `cmp`.
//
// The SORTED-PERMUTATION law is stated over two models that are written apart
// from each other and from the emitted text:
//   * `pass_sort` is an insertion sort (`ins` into a sorted prefix), the shape
//     the law is proved about;
//   * `sorted` is the ordering predicate (adjacent pairs non-descending) and
//     `cnt` is the multiset count of a line, both defined as their own
//     recursions. Neither is derived from `pass_sort`.
// `sort_is_a_sorted_permutation` then says: the output is ordered, and every
// line's multiplicity is unchanged. Dropping the "insert keeps the count" step
// or the "insert preserves order" step turns it red (see the mutant table in
// `shir_native_pipelines_sync.txt`).
//
// The FLAGS are proved in §7 (`-n`, `-r` and the glued `-nr`): the numeric key,
// the reverse step and the flag decoder all have their own models there.
// `-u`/`-k`/`-t` stay refused by the fold itself, and `-h` plus the float
// spellings are d-F4-3c.

/// Ordered: no adjacent pair descends. This is the *semantic* ordering model.
pub open spec fn sorted(ys: Seq<Word>) -> bool
    decreases ys.len(),
{
    if ys.len() <= 1 {
        true
    } else {
        !lex_lt(ys[1], ys[0]) && sorted(ys.skip(1))
    }
}

/// Insertion of one line into a sorted list — the pass side's step.
pub open spec fn ins(x: Word, ys: Seq<Word>) -> Seq<Word>
    decreases ys.len(),
{
    if ys.len() == 0 {
        seq![x]
    } else if lex_lt(x, ys[0]) {
        seq![x] + ys
    } else {
        seq![ys[0]] + ins(x, ys.skip(1))
    }
}

/// The whole sort as the fold performs it: insert the head into the sorted tail.
pub open spec fn pass_sort(xs: Seq<Word>) -> Seq<Word>
    decreases xs.len(),
{
    if xs.len() == 0 {
        Seq::empty()
    } else {
        ins(xs[0], pass_sort(xs.skip(1)))
    }
}

/// How many copies of the line `x` the list holds, by index (so the recursion
/// does not need a `skip` rewrite). This is the multiset model.
pub open spec fn cnt_at(x: Word, ys: Seq<Word>, i: int) -> int
    decreases ys.len() - i,
{
    if i >= ys.len() as int {
        0int
    } else {
        (if ys[i] == x { 1int } else { 0int }) + cnt_at(x, ys, i + 1)
    }
}

pub open spec fn cnt(x: Word, ys: Seq<Word>) -> int {
    cnt_at(x, ys, 0)
}

proof fn lemma_cons_zero(x: Word, ys: Seq<Word>)
    ensures
        (seq![x] + ys)[0] == x,
{
}

proof fn lemma_cons_index(x: Word, ys: Seq<Word>, i: int)
    requires
        0 <= i < ys.len(),
    ensures
        (seq![x] + ys)[i + 1] == ys[i],
{
}

proof fn lemma_skip_cons(x: Word, ys: Seq<Word>)
    ensures
        (seq![x] + ys).skip(1) == ys,
{
    assert((seq![x] + ys).skip(1) =~= ys);
}

proof fn lemma_skip_len(ys: Seq<Word>)
    requires
        ys.len() >= 1,
    ensures
        ys.skip(1).len() == ys.len() - 1,
{
}

proof fn lemma_skip_index(ys: Seq<Word>, i: int)
    requires
        ys.len() >= 2,
        0 <= i < ys.len() - 1,
    ensures
        ys.skip(1)[i] == ys[i + 1],
{
}

proof fn lemma_lex_asym(x: Word, y: Word)
    requires
        lex_lt(x, y),
    ensures
        !lex_lt(y, x),
    decreases x.len(),
{
    reveal(lex_lt);
    if x.len() == 0 {
    } else if y.len() == 0 {
    } else if x[0] != y[0] {
    } else {
        lemma_lex_asym(x.skip(1), y.skip(1));
    }
}

/// Unfolding `sorted` for the assumption direction (the function is recursive,
/// so it is opaque until revealed — this is the lemma that makes it usable).
proof fn lemma_sorted_unfold(ys: Seq<Word>)
    ensures
        sorted(ys) == (ys.len() <= 1 || (!lex_lt(ys[1], ys[0]) && sorted(ys.skip(1)))),
{
    reveal(sorted);
}

/// Unfolding `sorted` for the conclusion direction.
proof fn lemma_sorted_fold(ys: Seq<Word>)
    requires
        ys.len() > 1 ==> (!lex_lt(ys[1], ys[0]) && sorted(ys.skip(1))),
    ensures
        sorted(ys),
{
    reveal(sorted);
}

/// Prepending a line that is not greater than the head keeps the list sorted.
proof fn lemma_sorted_cons(a: Word, rest: Seq<Word>)
    requires
        sorted(rest),
        rest.len() == 0 || !lex_lt(rest[0], a),
    ensures
        sorted(seq![a] + rest),
{
    if rest.len() == 0 {
        assert((seq![a] + rest).len() == 1);
        lemma_sorted_unfold(seq![a] + rest);
    } else {
        assert((seq![a] + rest).len() == rest.len() + 1);
        assert((seq![a] + rest).len() >= 2);
        lemma_cons_zero(a, rest);
        lemma_cons_index(a, rest, 0);
        lemma_skip_cons(a, rest);
        assert((seq![a] + rest)[0] == a);
        assert((seq![a] + rest)[1] == rest[0]);
        assert((seq![a] + rest).skip(1) == rest);
        assert(!lex_lt((seq![a] + rest)[1], a));
        assert(sorted((seq![a] + rest).skip(1)));
        assert(!lex_lt((seq![a] + rest)[1], (seq![a] + rest)[0]));
        lemma_sorted_unfold(seq![a] + rest);
    }
}

proof fn lemma_sorted_tail(ys: Seq<Word>)
    requires
        sorted(ys),
        ys.len() >= 2,
    ensures
        sorted(ys.skip(1)),
{
    lemma_sorted_unfold(ys);
}

proof fn lemma_sorted_two(a: Word, b: Word)
    requires
        !lex_lt(b, a),
    ensures
        sorted(seq![a, b]),
{
    assert(seq![a, b].len() == 2);
    assert(seq![a, b][0] == a);
    assert(seq![a, b][1] == b);
    assert(seq![a, b].skip(1) =~= seq![b]);
    lemma_sorted_unfold(seq![b]);
    lemma_sorted_unfold(seq![a, b]);
}

proof fn lemma_ins_head(x: Word, ys: Seq<Word>)
    requires
        ys.len() >= 1,
    ensures
        ins(x, ys)[0] == x || ins(x, ys)[0] == ys[0],
{
    reveal(ins);
    if lex_lt(x, ys[0]) {
        lemma_cons_zero(x, ys);
    } else {
        assert(ins(x, ys) == seq![ys[0]] + ins(x, ys.skip(1)));
        lemma_cons_zero(ys[0], ins(x, ys.skip(1)));
    }
}

/// The insertion step preserves the ordering.
proof fn lemma_ins_sorted(x: Word, ys: Seq<Word>)
    requires
        sorted(ys),
    ensures
        sorted(ins(x, ys)),
    decreases ys.len(),
{
    reveal(ins);
    if ys.len() == 0 {
        assert(ins(x, ys) == seq![x]);
        assert(ins(x, ys).len() == 1);
        lemma_sorted_unfold(ins(x, ys));
    } else if lex_lt(x, ys[0]) {
        lemma_lex_asym(x, ys[0]);
        assert(ins(x, ys) == seq![x] + ys);
        lemma_sorted_cons(x, ys);
    } else if ys.len() == 1 {
        assert(ys.skip(1).len() == 0);
        assert(ins(x, ys.skip(1)) == seq![x]);
        assert(seq![ys[0]] + seq![x] =~= seq![ys[0], x]);
        assert(ins(x, ys) == seq![ys[0], x]);
        lemma_sorted_two(ys[0], x);
    } else {
        lemma_sorted_tail(ys);
        lemma_ins_sorted(x, ys.skip(1));
        let rest = ins(x, ys.skip(1));
        assert(ins(x, ys) == seq![ys[0]] + rest);
        assert(!lex_lt(x, ys[0]));
        lemma_ins_head(x, ys.skip(1));
        lemma_skip_index(ys, 0);
        lemma_sorted_cons(ys[0], rest);
    }
}

proof fn lemma_cnt_at_cons_tail(x: Word, y: Word, ys: Seq<Word>, k: int)
    requires
        k >= 0,
    ensures
        cnt_at(x, seq![y] + ys, k + 1) == cnt_at(x, ys, k),
    decreases ys.len() + 2 - k,
{
    reveal(cnt_at);
    if k >= ys.len() as int {
        assert(k + 1 >= (seq![y] + ys).len());
    } else {
        lemma_cons_index(y, ys, k);
        lemma_cnt_at_cons_tail(x, y, ys, k + 1);
    }
}

proof fn lemma_cnt_cons(x: Word, y: Word, ys: Seq<Word>)
    ensures
        cnt(x, seq![y] + ys) == (if y == x { 1int } else { 0int }) + cnt(x, ys),
{
    reveal(cnt);
    lemma_cons_zero(y, ys);
    lemma_cnt_at_cons_tail(x, y, ys, 0);
}

proof fn lemma_cnt_skip_at(x: Word, ys: Seq<Word>, k: int)
    requires
        1 <= k <= ys.len(),
    ensures
        cnt_at(x, ys, k) == cnt_at(x, ys.skip(1), k - 1),
    decreases ys.len() - k,
{
    reveal(cnt_at);
    if k >= ys.len() as int {
        lemma_skip_len(ys);
        assert(cnt_at(x, ys, k) == 0);
        assert(cnt_at(x, ys.skip(1), k - 1) == 0);
    } else {
        lemma_skip_index(ys, k - 1);
        lemma_cnt_skip_at(x, ys, k + 1);
    }
}

proof fn lemma_cnt_peel(x: Word, ys: Seq<Word>)
    requires
        ys.len() >= 1,
    ensures
        cnt(x, ys) == (if ys[0] == x { 1int } else { 0int }) + cnt(x, ys.skip(1)),
{
    reveal(cnt);
    lemma_cnt_skip_at(x, ys, 1);
}

/// The insertion step preserves every line's multiplicity.
proof fn lemma_ins_cnt(x: Word, y: Word, ys: Seq<Word>)
    ensures
        cnt(x, ins(y, ys)) == cnt(x, ys) + (if x == y { 1int } else { 0int }),
    decreases ys.len(),
{
    reveal(ins);
    reveal(cnt);
    if ys.len() == 0 {
        lemma_cnt_cons(x, y, Seq::<Word>::empty());
    } else if lex_lt(y, ys[0]) {
        lemma_cnt_cons(x, y, ys);
    } else {
        lemma_cnt_cons(x, ys[0], ins(y, ys.skip(1)));
        lemma_ins_cnt(x, y, ys.skip(1));
        lemma_cnt_peel(x, ys);
    }
}

proof fn lemma_sort_sorted(xs: Seq<Word>)
    ensures
        sorted(pass_sort(xs)),
    decreases xs.len(),
{
    reveal(pass_sort);
    if xs.len() == 0 {
        assert(pass_sort(xs).len() == 0);
        lemma_sorted_unfold(pass_sort(xs));
    } else {
        lemma_sort_sorted(xs.skip(1));
        lemma_ins_sorted(xs[0], pass_sort(xs.skip(1)));
    }
}

proof fn lemma_sort_cnt(x: Word, xs: Seq<Word>)
    ensures
        cnt(x, pass_sort(xs)) == cnt(x, xs),
    decreases xs.len(),
{
    reveal(pass_sort);
    if xs.len() == 0 {
    } else {
        lemma_sort_cnt(x, xs.skip(1));
        lemma_ins_cnt(x, xs[0], pass_sort(xs.skip(1)));
        lemma_cnt_peel(x, xs);
    }
}

/// **The sort law.** The emitted `sort @lines` output is ordered and holds
/// exactly the input's lines with exactly the input's multiplicities — a sorted
/// permutation of the input, over models (`sorted`, `cnt`) defined apart from
/// `ins`/`pass_sort`.
pub proof fn sort_is_a_sorted_permutation(xs: Seq<Word>)
    ensures
        sorted(pass_sort(xs)),
        forall|x: Word| #[trigger] cnt(x, pass_sort(xs)) == cnt(x, xs),
{
    lemma_sort_sorted(xs);
    assert forall|x: Word| #[trigger] cnt(x, pass_sort(xs)) == cnt(x, xs) by {
        lemma_sort_cnt(x, xs);
    }
}

// ──────────────────────────────────────── §6  `ls` and `|| echo` ──────────
//
// `native_ls_stmt` emits, per operand: `if (!-e $p) { $__ls_rc = 2; }` inside a
// loop over the paths, then `$__rc = $__ls_rc ? 2 : 0;` and the pipeline's
// `|| echo` printing when `$__rc` is non-zero. The law below fixes the wiring:
// the fallback prints exactly when some operand was missing, and the status is 2
// in that case and 0 otherwise.
//
// Two recursions, written apart:
//   * `scan_rc` is the emitted accumulator loop (`bad` is the running flag);
//   * `any_missing` is the semantic OR-fold over the operand list, its own
//     function; `ls_rc` maps that to a status and `prints_echo` is the pipeline's
//     `if ($rc) { print ... }` guard.
// The pin pair below keeps it non-vacuous: a list with a missing operand prints,
// an all-present list stays silent.

/// The list the `ls` fold prints: `true` = the operand exists.
pub type Present = Seq<bool>;

/// Pass side: the emitted loop's status accumulator, in index form.
pub open spec fn scan_rc(present: Present, bad: bool, i: int) -> int
    decreases present.len() - i,
{
    if i >= present.len() as int {
        if bad { 2int } else { 0int }
    } else {
        scan_rc(present, bad || !present[i], i + 1)
    }
}

/// Semantic side: is some operand missing? Its own OR-fold.
pub open spec fn any_missing(present: Present, i: int) -> bool
    decreases present.len() - i,
{
    if i >= present.len() as int {
        false
    } else {
        !present[i] || any_missing(present, i + 1)
    }
}

/// The status the listing command reports to the pipeline.
pub open spec fn ls_rc(present: Present) -> int {
    if any_missing(present, 0) { 2int } else { 0int }
}

/// The emitted `if ($__rc) { print ECHO }` guard of `|| echo`.
pub open spec fn prints_echo(rc: int) -> bool {
    rc != 0
}

proof fn lemma_scan_unfold(present: Present, bad: bool, i: int)
    ensures
        scan_rc(present, bad, i) == (if i >= present.len() as int {
            if bad { 2int } else { 0int }
        } else {
            scan_rc(present, bad || !present[i], i + 1)
        }),
{
    reveal(scan_rc);
}

proof fn lemma_any_unfold(present: Present, i: int)
    ensures
        any_missing(present, i) == (if i >= present.len() as int {
            false
        } else {
            !present[i] || any_missing(present, i + 1)
        }),
{
    reveal(any_missing);
}

/// The accumulator loop reports 2 exactly when the running flag or a missing
/// operand says so.
pub proof fn lemma_scan_is_status(present: Present, bad: bool, i: int)
    requires
        0 <= i <= present.len(),
    ensures
        scan_rc(present, bad, i) == (if bad || any_missing(present, i) { 2int } else { 0int }),
    decreases present.len() - i,
{
    lemma_scan_unfold(present, bad, i);
    lemma_any_unfold(present, i);
    if i >= present.len() as int {
    } else {
        lemma_scan_is_status(present, bad || !present[i], i + 1);
    }
}

/// The emitted loop's status equals the semantic status.
pub proof fn lemma_ls_rc_matches(present: Present)
    ensures
        scan_rc(present, false, 0) == ls_rc(present),
{
    lemma_scan_is_status(present, false, 0);
    reveal(ls_rc);
}

/// **The `ls` fallback law.** `|| echo` prints exactly when an operand was
/// missing — the status wiring, stated against `any_missing`, not against the
/// loop's own accumulator.
pub proof fn ls_fallback_prints_iff_missing(present: Present)
    ensures
        prints_echo(scan_rc(present, false, 0)) <==> any_missing(present, 0),
{
    lemma_scan_is_status(present, false, 0);
    reveal(prints_echo);
}

// pins: the sort law is non-vacuous on a concrete two-line file, and the
// multiset half really counts duplicates.
pub proof fn pin_sort_sorted_permutation()
    ensures
        sorted(pass_sort(seq![seq![66int], seq![65int]])),
        pass_sort(seq![seq![66int], seq![65int]]) == seq![seq![65int], seq![66int]],
{
    sort_is_a_sorted_permutation(seq![seq![66int], seq![65int]]);
    assert(pass_sort(seq![seq![66int], seq![65int]]) == seq![seq![65int], seq![66int]])
        by (compute);
}

pub proof fn pin_sort_keeps_duplicates()
    ensures
        pass_sort(seq![seq![66int], seq![65int], seq![66int]])
            == seq![seq![65int], seq![66int], seq![66int]],
        cnt(seq![66int], pass_sort(seq![seq![66int], seq![65int], seq![66int]])) == 2,
        cnt(seq![65int], pass_sort(seq![seq![66int], seq![65int], seq![66int]])) == 1,
{
    sort_is_a_sorted_permutation(seq![seq![66int], seq![65int], seq![66int]]);
    assert(pass_sort(seq![seq![66int], seq![65int], seq![66int]])
        == seq![seq![65int], seq![66int], seq![66int]]) by (compute);
    assert(cnt(seq![66int], seq![seq![65int], seq![66int], seq![66int]]) == 2) by (compute);
    assert(cnt(seq![65int], seq![seq![65int], seq![66int], seq![66int]]) == 1) by (compute);
}

// pins: the `ls || echo` wiring prints on a missing operand and stays silent
// on an all-present list.
pub proof fn pin_ls_missing_operand_prints()
    ensures
        ls_rc(seq![true, false]) == 2int,
        prints_echo(scan_rc(seq![true, false], false, 0)),
{
    lemma_ls_rc_matches(seq![true, false]);
    assert(any_missing(seq![true, false], 0)) by (compute);
    assert(ls_rc(seq![true, false]) == 2) by (compute);
    reveal(prints_echo);
}

pub proof fn pin_ls_all_present_silent()
    ensures
        ls_rc(seq![true, true]) == 0int,
        !prints_echo(scan_rc(seq![true, true], false, 0)),
{
    lemma_ls_rc_matches(seq![true, true]);
    assert(!any_missing(seq![true, true], 0)) by (compute);
    assert(ls_rc(seq![true, true]) == 0) by (compute);
    reveal(prints_echo);
}


// ────────────────────────── §7  `sort -n` / `sort -r` (d-F4-3b) ──────────
//
// `native_sort_stmt` reads the consumer's flag words: `-n` (numeric key), `-h`
// (human key), `-r` (reverse) and the glued spellings `-nr`/`-rn`/`-hr`/`-rh`;
// any other word makes the fold REFUSE (the shell-out stays). With no flag the
// pass sorts bytewise (`cmp`), with `-n` it sorts on `parse::<f64>()` with
// `unwrap_or(0.0)` for a line that does not parse, and `-r` calls
// `lines.reverse()` on the sorted vector.
//
// The pass-side models are written in the pass's own shape:
//   * `ins_key`/`pass_sort_n` — the numeric insertion sort; a line goes in
//     before the first line whose key is at least its own, which is what keeps
//     equal keys in input order (the emitted `sort_by` is stable, and the head
//     is inserted into a sorted tail, so `>` here would reverse ties — the
//     stability pin below is what caught that);
//   * `rev` — the `lines.reverse()` step, as a `push` recursion.
// The SEMANTIC models are written apart from those and from the emitted text:
//   * `num_key` — the value of a signed decimal-integer line, and 0 for a line
//     that is not one (`unwrap_or(0.0)`), built from `is_int_lit`/`digits_val`;
//   * `asc_key`/`desc_key` — no adjacent pair of lines has keys that ascend /
//     descend, quantified over all adjacent indices;
//   * `desc_lex` — the same for byte order (`-r` alone);
//   * `cnt` — the multiset model of §5, reused for the permutation half.
// `decode_flags` models the flag loop itself, including its refusal.
//
// NOT modelled here, stated rather than implied (d-F4-3c):
//   * fractional and exponent spellings (`1.5`, `1e3`): `num_key` covers the
//     integer fragment of `parse::<f64>`; the parse is a lexical front end and
//     the ORDER reasoning above it is what is proved, but the lexical step for
//     those spellings is not modelled, so the law is about integer lines;
//   * `-h`'s `human_size` key: the fold accepts `-h`, this proof does not model
//     the key, so nothing is claimed for `-h` (it stays refused for proof
//     purposes even though the pass folds it);
//   * `-u` (uniquify) and `-k`/`-t` (key selection) are refused by the fold and
//     so are outside every law here.

/// Every byte of `w` from `i` on is an ASCII digit.
pub open spec fn all_digits(w: Word, i: int) -> bool
    decreases w.len() - i,
{
    if i >= w.len() as int {
        true
    } else {
        48 <= w[i] && w[i] <= 57 && all_digits(w, i + 1)
    }
}

/// A left fold over those digits: `acc * 10 + digit` per byte, most
/// significant byte first.
pub open spec fn digits_acc(w: Word, i: int, acc: int) -> int
    decreases w.len() - i,
{
    if i >= w.len() as int {
        acc
    } else {
        digits_acc(w, i + 1, acc * 10int + (w[i] - 48int))
    }
}

/// The value of the digits of `w` from `i` on.
pub open spec fn digits_val(w: Word, i: int) -> int {
    digits_acc(w, i, 0int)
}

/// What `parse::<f64>()` accepts on the integer fragment: an optional `-` and
/// at least one digit.
pub open spec fn is_int_lit(w: Word) -> bool {
    (w.len() >= 1 && all_digits(w, 0))
        || (w.len() >= 2 && w[0] == 45int && all_digits(w, 1))
}

pub open spec fn key_signed(w: Word) -> int {
    if w.len() >= 2 && w[0] == 45int {
        -digits_val(w, 1)
    } else {
        digits_val(w, 0)
    }
}

/// **The numeric key.** `parse::<f64>()`'s value for an integer line, and 0 for
/// a line that does not parse (`unwrap_or(0.0)`). Nothing here is derived from
/// `ins_key`/`pass_sort_n`.
pub open spec fn num_key(w: Word) -> int {
    if is_int_lit(w) { key_signed(w) } else { 0int }
}

proof fn lemma_digits_nonneg(w: Word, i: int, acc: int)
    requires
        all_digits(w, i),
        acc >= 0,
    ensures
        0int <= digits_acc(w, i, acc),
    decreases w.len() - i,
{
    reveal(digits_acc);
    reveal(all_digits);
    if i >= w.len() as int {
    } else {
        lemma_digits_nonneg(w, i + 1, acc * 10int + (w[i] - 48int));
    }
}

/// A digit-only line's key is its value, and no line has a negative one.
pub proof fn digits_line_key(w: Word)
    requires
        w.len() >= 1,
        all_digits(w, 0),
    ensures
        num_key(w) == digits_val(w, 0),
        0int <= num_key(w),
{
    reveal(num_key);
    reveal(is_int_lit);
    reveal(key_signed);
    reveal(all_digits);
    assert(w[0] != 45int);
    reveal(digits_val);
    lemma_digits_nonneg(w, 0, 0int);
}

/// A line that is not a number reads as 0 — the `unwrap_or(0.0)` fallback.
pub proof fn non_number_key_is_zero(w: Word)
    requires
        !is_int_lit(w),
    ensures
        num_key(w) == 0int,
{
    reveal(num_key);
}

/// The numeric insertion step: insert before the first line whose key is at
/// least the inserted line's own. The head is inserted into an already-sorted
/// TAIL, so `>=` (not `>`) is what keeps equal keys in input order — a stable
/// sort; the emitted `sort_by` is stable too.
pub open spec fn ins_key(x: Word, ys: Seq<Word>) -> Seq<Word>
    decreases ys.len(),
{
    if ys.len() == 0 {
        seq![x]
    } else if num_key(ys[0]) >= num_key(x) {
        seq![x] + ys
    } else {
        seq![ys[0]] + ins_key(x, ys.skip(1))
    }
}

/// The whole numeric sort as the pass performs it.
pub open spec fn pass_sort_n(xs: Seq<Word>) -> Seq<Word>
    decreases xs.len(),
{
    if xs.len() == 0 {
        Seq::<Word>::empty()
    } else {
        ins_key(xs[0], pass_sort_n(xs.skip(1)))
    }
}

/// Semantic: no adjacent pair of lines has ascending keys.
pub open spec fn asc_key(ys: Seq<Word>) -> bool {
    forall|k: int| 0 <= k < ys.len() - 1 ==> #[trigger] num_key(ys[k]) <= num_key(ys[k + 1])
}

/// Semantic: no adjacent pair of lines has descending keys.
pub open spec fn desc_key(ys: Seq<Word>) -> bool {
    forall|k: int| 0 <= k < ys.len() - 1 ==> #[trigger] num_key(ys[k]) >= num_key(ys[k + 1])
}

/// Semantic: no adjacent pair of lines ascends in byte order.
pub open spec fn desc_lex(ys: Seq<Word>) -> bool {
    forall|k: int| 0 <= k < ys.len() - 1 ==> #[trigger] lex_lt(ys[k], ys[k + 1]) == false
}

/// The `lines.reverse()` step, as a `push` recursion.
pub open spec fn rev(ys: Seq<Word>) -> Seq<Word>
    decreases ys.len(),
{
    if ys.len() == 0 {
        Seq::<Word>::empty()
    } else {
        rev(ys.skip(1)).push(ys[0])
    }
}

proof fn lemma_skip_len2(ys: Seq<Word>)
    requires
        ys.len() >= 2,
    ensures
        ys.skip(1).len() == ys.len() - 1,
{
    lemma_skip_len(ys);
}

/// The insertion step's head is the inserted line or the list's own head.
proof fn lemma_ins_key_head(x: Word, ys: Seq<Word>)
    requires
        ys.len() >= 1,
    ensures
        ins_key(x, ys)[0] == x || ins_key(x, ys)[0] == ys[0],
{
    reveal(ins_key);
    if num_key(ys[0]) >= num_key(x) {
        lemma_cons_zero(x, ys);
    } else {
        reveal(ins_key);
        assert(ins_key(x, ys) == seq![ys[0]] + ins_key(x, ys.skip(1)));
        lemma_cons_zero(ys[0], ins_key(x, ys.skip(1)));
    }
}

proof fn lemma_asc_key_tail(ys: Seq<Word>)
    requires
        asc_key(ys),
        ys.len() >= 1,
    ensures
        asc_key(ys.skip(1)),
{
    lemma_skip_len(ys);
    assert forall|k: int|
        0 <= k < ys.skip(1).len() - 1
        implies #[trigger] num_key(ys.skip(1)[k]) <= num_key(ys.skip(1)[k + 1]) by {
        if 0 <= k < ys.skip(1).len() - 1 {
            lemma_skip_index(ys, k);
            lemma_skip_index(ys, k + 1);
        }
    }
}

/// The insertion step preserves the ordering.
proof fn lemma_ins_key_sorted(x: Word, ys: Seq<Word>)
    requires
        asc_key(ys),
    ensures
        asc_key(ins_key(x, ys)),
    decreases ys.len(),
{
    reveal(ins_key);
    if ys.len() == 0 {
        assert(ins_key(x, ys) == seq![x]);
        assert(asc_key(ins_key(x, ys)));
    } else if num_key(ys[0]) >= num_key(x) {
        let r = seq![x] + ys;
        assert(ins_key(x, ys) == r);
        assert(asc_key(r)) by {
            assert forall|k: int|
                0 <= k < r.len() - 1
                implies #[trigger] num_key(r[k]) <= num_key(r[k + 1]) by {
                if 0 <= k < r.len() - 1 {
                    if k == 0 {
                        lemma_cons_zero(x, ys);
                        lemma_cons_index(x, ys, 0);
                    } else {
                        lemma_cons_index(x, ys, k - 1);
                        lemma_cons_index(x, ys, k);
                    }
                }
            }
        }
    } else {
        let rest = ins_key(x, ys.skip(1));
        assert(ins_key(x, ys) == seq![ys[0]] + rest);
        lemma_asc_key_tail(ys);
        lemma_ins_key_sorted(x, ys.skip(1));
        let r = seq![ys[0]] + rest;
        assert(asc_key(r)) by {
            assert forall|k: int|
                0 <= k < r.len() - 1
                implies #[trigger] num_key(r[k]) <= num_key(r[k + 1]) by {
                if 0 <= k < r.len() - 1 {
                    if k == 0 {
                        lemma_cons_zero(ys[0], rest);
                        lemma_cons_index(ys[0], rest, 0);
                        if ys.len() == 1 {
                            lemma_skip_len(ys);
                            assert(ys.skip(1).len() == 0);
                            reveal(ins_key);
                            assert(rest == seq![x]);
                        } else {
                            lemma_ins_key_head(x, ys.skip(1));
                            if rest[0] == x {
                            } else {
                                lemma_skip_index(ys, 0);
                                assert(rest[0] == ys[1]);
                            }
                        }
                    } else {
                        lemma_cons_index(ys[0], rest, k - 1);
                        lemma_cons_index(ys[0], rest, k);
                        assert(num_key(rest[k - 1]) <= num_key(rest[k]));
                    }
                }
            }
        }
    }
}

/// The insertion step preserves every line's multiplicity.
proof fn lemma_ins_key_cnt(x: Word, y: Word, ys: Seq<Word>)
    ensures
        cnt(x, ins_key(y, ys)) == cnt(x, ys) + (if x == y { 1int } else { 0int }),
    decreases ys.len(),
{
    reveal(ins_key);
    reveal(cnt);
    if ys.len() == 0 {
        lemma_cnt_cons(x, y, Seq::<Word>::empty());
    } else if num_key(ys[0]) >= num_key(y) {
        lemma_cnt_cons(x, y, ys);
    } else {
        lemma_cnt_cons(x, ys[0], ins_key(y, ys.skip(1)));
        lemma_ins_key_cnt(x, y, ys.skip(1));
        lemma_cnt_peel(x, ys);
    }
}

proof fn lemma_sort_n_asc(xs: Seq<Word>)
    ensures
        asc_key(pass_sort_n(xs)),
    decreases xs.len(),
{
    reveal(pass_sort_n);
    if xs.len() == 0 {
        assert(pass_sort_n(xs).len() == 0);
        assert(asc_key(pass_sort_n(xs)));
    } else {
        lemma_sort_n_asc(xs.skip(1));
        lemma_ins_key_sorted(xs[0], pass_sort_n(xs.skip(1)));
    }
}

proof fn lemma_sort_n_cnt(x: Word, xs: Seq<Word>)
    ensures
        cnt(x, pass_sort_n(xs)) == cnt(x, xs),
    decreases xs.len(),
{
    reveal(pass_sort_n);
    if xs.len() == 0 {
    } else {
        lemma_sort_n_cnt(x, xs.skip(1));
        lemma_ins_key_cnt(x, xs[0], pass_sort_n(xs.skip(1)));
        lemma_cnt_peel(x, xs);
    }
}

/// **The `sort -n` law.** The numeric fold's output is ordered by the numeric
/// key and is a permutation of its input. `num_key`/`asc_key`/`cnt` are all
/// defined apart from `ins_key`/`pass_sort_n`.
pub proof fn sort_n_is_a_sorted_key_permutation(xs: Seq<Word>)
    ensures
        asc_key(pass_sort_n(xs)),
        forall|x: Word| #[trigger] cnt(x, pass_sort_n(xs)) == cnt(x, xs),
        pass_sort_n(xs).len() == xs.len(),
{
    lemma_sort_n_asc(xs);
    lemma_sort_n_len(xs);
    assert forall|x: Word| #[trigger] cnt(x, pass_sort_n(xs)) == cnt(x, xs) by {
        lemma_sort_n_cnt(x, xs);
    }
}

proof fn lemma_rev_len(ys: Seq<Word>)
    ensures
        rev(ys).len() == ys.len(),
    decreases ys.len(),
{
    reveal(rev);
    if ys.len() == 0 {
    } else {
        lemma_rev_len(ys.skip(1));
        lemma_skip_len(ys);
    }
}

/// The mirror-index fact: reversing a list maps index `i` to `len-1-i`.
proof fn lemma_rev_index(ys: Seq<Word>, i: int)
    requires
        0 <= i < ys.len(),
    ensures
        rev(ys)[i] == ys[ys.len() - 1 - i],
    decreases ys.len(),
{
    reveal(rev);
    lemma_rev_len(ys);
    if ys.len() == 1 {
        assert(rev(ys.skip(1)).len() == 0);
    } else {
        assert(rev(ys) == rev(ys.skip(1)).push(ys[0]));
        lemma_rev_len(ys.skip(1));
        lemma_skip_len2(ys);
        if i < ys.len() - 1 {
            lemma_rev_index(ys.skip(1), i);
            lemma_skip_index(ys, ys.len() - 2 - i);
        }
    }
}

/// `push` adds one occurrence to the count, from any start index.
proof fn lemma_cnt_at_push(x: Word, ys: Seq<Word>, y: Word, k: int)
    requires
        0 <= k <= ys.len(),
    ensures
        cnt_at(x, ys.push(y), k) == cnt_at(x, ys, k) + (if y == x { 1int } else { 0int }),
    decreases ys.len() - k,
{
    reveal(cnt_at);
    if k >= ys.len() as int {
        assert(k == ys.len() as int);
        assert(ys.push(y).len() == ys.len() + 1);
        reveal(cnt_at);
        assert(ys.push(y)[k] == y);
        assert(k + 1 >= ys.push(y).len() as int);
        assert(cnt_at(x, ys.push(y), k + 1) == 0int);
        assert(cnt_at(x, ys, k) == 0int);
    } else {
        assert(ys.push(y).len() == ys.len() + 1);
        assert(ys.push(y)[k] == ys[k]);
        lemma_cnt_at_push(x, ys, y, k + 1);
        reveal(cnt_at);
        assert((ys.push(y))[k] == x <==> ys[k] == x);
    }
}

/// Reversing keeps every line's multiplicity.
proof fn lemma_rev_cnt(x: Word, ys: Seq<Word>)
    ensures
        cnt(x, rev(ys)) == cnt(x, ys),
    decreases ys.len(),
{
    reveal(rev);
    reveal(cnt);
    if ys.len() == 0 {
    } else {
        lemma_rev_cnt(x, ys.skip(1));
        lemma_cnt_at_push(x, rev(ys.skip(1)), ys[0], 0);
        lemma_cnt_peel(x, ys);
    }
}

/// The adjacent pair at an arbitrary index, unwrapped from `sorted`.
proof fn lemma_sorted_adj(ys: Seq<Word>, j: int)
    requires
        sorted(ys),
        0 <= j < ys.len() - 1,
    ensures
        !lex_lt(ys[j + 1], ys[j]),
    decreases ys.len(),
{
    lemma_sorted_unfold(ys);
    if j == 0 {
    } else {
        lemma_sorted_tail(ys);
        lemma_skip_index(ys, j - 1);
        lemma_skip_index(ys, j);
        lemma_sorted_adj(ys.skip(1), j - 1);
    }
}

/// Reversing an ascending byte-order list leaves no ascending adjacent pair.
pub proof fn lemma_rev_desc_lex(ys: Seq<Word>)
    requires
        sorted(ys),
    ensures
        desc_lex(rev(ys)),
{
    lemma_rev_len(ys);
    assert forall|k: int|
        0 <= k < rev(ys).len() - 1
        implies #[trigger] lex_lt(rev(ys)[k], rev(ys)[k + 1]) == false by {
        lemma_rev_index(ys, k);
        lemma_rev_index(ys, k + 1);
        lemma_sorted_adj(ys, ys.len() - 2 - k);
    }
}

/// Reversing a key-ascending list leaves no ascending key pair.
pub proof fn lemma_rev_desc_key(ys: Seq<Word>)
    requires
        asc_key(ys),
    ensures
        desc_key(rev(ys)),
{
    lemma_rev_len(ys);
    assert forall|k: int|
        0 <= k < rev(ys).len() - 1
        implies #[trigger] num_key(rev(ys)[k]) >= num_key(rev(ys)[k + 1]) by {
        lemma_rev_index(ys, k);
        lemma_rev_index(ys, k + 1);
        assert(num_key(ys[ys.len() - 2 - k]) <= num_key(ys[ys.len() - 1 - k]));
    }
}

/// **The `sort -r` law.** Byte-order sort then `lines.reverse()` is descending
/// in byte order and a permutation of the input.
pub proof fn sort_r_is_a_descending_permutation(xs: Seq<Word>)
    ensures
        desc_lex(rev(pass_sort(xs))),
        forall|x: Word| #[trigger] cnt(x, rev(pass_sort(xs))) == cnt(x, xs),
        rev(pass_sort(xs)).len() == xs.len(),
{
    lemma_sort_sorted(xs);
    lemma_rev_desc_lex(pass_sort(xs));
    assert forall|x: Word| #[trigger] cnt(x, rev(pass_sort(xs))) == cnt(x, xs) by {
        lemma_rev_cnt(x, pass_sort(xs));
        lemma_sort_cnt(x, xs);
    }
    lemma_rev_len(pass_sort(xs));
    lemma_pass_sort_len(xs);
}

/// **The `sort -nr` law.** Numeric sort then reverse is key-descending and a
/// permutation of the input.
pub proof fn sort_nr_is_a_descending_key_permutation(xs: Seq<Word>)
    ensures
        desc_key(rev(pass_sort_n(xs))),
        forall|x: Word| #[trigger] cnt(x, rev(pass_sort_n(xs))) == cnt(x, xs),
        rev(pass_sort_n(xs)).len() == xs.len(),
{
    lemma_sort_n_asc(xs);
    lemma_rev_desc_key(pass_sort_n(xs));
    assert forall|x: Word| #[trigger] cnt(x, rev(pass_sort_n(xs))) == cnt(x, xs) by {
        lemma_rev_cnt(x, pass_sort_n(xs));
        lemma_sort_n_cnt(x, xs);
    }
    lemma_rev_len(pass_sort_n(xs));
    lemma_sort_n_len(xs);
}

/// Decoded `sort` flags as `native_sort_stmt` folds them: `(numeric, human,
/// reverse)`. `None` is the fold's refusal — any other word keeps the shell-out.
pub open spec fn decode_flags(
    ws: Seq<Word>,
    i: int,
    numeric: bool,
    human: bool,
    reverse: bool,
) -> Option<(bool, bool, bool)>
    decreases ws.len() - i,
{
    if i >= ws.len() as int {
        Some((numeric, human, reverse))
    } else if ws[i] == seq![45int, 110int] {
        decode_flags(ws, i + 1, true, human, reverse)
    } else if ws[i] == seq![45int, 104int] {
        decode_flags(ws, i + 1, numeric, true, reverse)
    } else if ws[i] == seq![45int, 114int] {
        decode_flags(ws, i + 1, numeric, human, true)
    } else if ws[i] == seq![45int, 110int, 114int]
        || ws[i] == seq![45int, 114int, 110int]
    {
        decode_flags(ws, i + 1, true, human, true)
    } else if ws[i] == seq![45int, 104int, 114int]
        || ws[i] == seq![45int, 114int, 104int]
    {
        decode_flags(ws, i + 1, numeric, true, true)
    } else {
        None::<(bool, bool, bool)>
    }
}

proof fn lemma_ins_len(x: Word, ys: Seq<Word>)
    ensures
        ins(x, ys).len() == ys.len() + 1,
    decreases ys.len(),
{
    reveal(ins);
    if ys.len() == 0 {
    } else if lex_lt(x, ys[0]) {
    } else {
        lemma_ins_len(x, ys.skip(1));
    }
}

proof fn lemma_pass_sort_len(xs: Seq<Word>)
    ensures
        pass_sort(xs).len() == xs.len(),
    decreases xs.len(),
{
    reveal(pass_sort);
    if xs.len() == 0 {
    } else {
        lemma_pass_sort_len(xs.skip(1));
        lemma_ins_len(xs[0], pass_sort(xs.skip(1)));
    }
}

proof fn lemma_ins_key_len(x: Word, ys: Seq<Word>)
    ensures
        ins_key(x, ys).len() == ys.len() + 1,
    decreases ys.len(),
{
    reveal(ins_key);
    if ys.len() == 0 {
    } else if num_key(ys[0]) >= num_key(x) {
    } else {
        lemma_ins_key_len(x, ys.skip(1));
    }
}

proof fn lemma_sort_n_len(xs: Seq<Word>)
    ensures
        pass_sort_n(xs).len() == xs.len(),
    decreases xs.len(),
{
    reveal(pass_sort_n);
    if xs.len() == 0 {
    } else {
        lemma_sort_n_len(xs.skip(1));
        lemma_ins_key_len(xs[0], pass_sort_n(xs.skip(1)));
    }
}

// The concrete lines the flag pins sort: `"10"`, `"9"`, `"1"`, `"01"`, `"abc"`
// and `"-3"`, as their bytes.
pub open spec fn w_ten() -> Word { seq![49int, 48int] }
pub open spec fn w_nine() -> Word { seq![57int] }
pub open spec fn w_one() -> Word { seq![49int] }
pub open spec fn w_zero_one() -> Word { seq![48int, 49int] }
pub open spec fn w_abc() -> Word { seq![97int, 98int, 99int] }
pub open spec fn w_neg3() -> Word { seq![45int, 51int] }

// pins: the numeric key really reads the line's value, and it really disagrees
// with the byte order the unflagged sort uses.
pub proof fn pin_numeric_key_is_the_line_value()
    ensures
        num_key(w_ten()) == 10int,
        num_key(w_nine()) == 9int,
        num_key(w_neg3()) == -3int,
{
    assert(num_key(w_ten()) == 10int) by (compute);
    assert(num_key(w_nine()) == 9int) by (compute);
    assert(num_key(w_neg3()) == -3int) by (compute);
}

pub proof fn pin_numeric_key_disagrees_with_byte_order()
    ensures
        lex_lt(w_ten(), w_nine()),
        num_key(w_ten()) > num_key(w_nine()),
{
    assert(lex_lt(w_ten(), w_nine())) by (compute);
    assert(num_key(w_ten()) == 10int) by (compute);
    assert(num_key(w_nine()) == 9int) by (compute);
}

/// A line that does not parse reads as 0, so `-3` sorts before it and `abc`
/// ties with `0`.
pub proof fn pin_non_number_reads_as_zero()
    ensures
        num_key(w_abc()) == 0int,
        num_key(w_neg3()) < num_key(w_abc()),
{
    non_number_key_is_zero(w_abc());
    assert(num_key(w_neg3()) == -3int) by (compute);
}

// pins: `-n` reorders the pair the byte-order sort leaves alone, and equal keys
// keep their input order.
pub proof fn pin_sort_n_puts_9_before_10()
    ensures
        pass_sort(seq![w_ten(), w_nine()]) == seq![w_ten(), w_nine()],
        pass_sort_n(seq![w_ten(), w_nine()]) == seq![w_nine(), w_ten()],
        asc_key(pass_sort_n(seq![w_ten(), w_nine()])),
{
    sort_n_is_a_sorted_key_permutation(seq![w_ten(), w_nine()]);
    assert(pass_sort(seq![w_ten(), w_nine()]) == seq![w_ten(), w_nine()]) by (compute);
    assert(pass_sort_n(seq![w_ten(), w_nine()]) == seq![w_nine(), w_ten()]) by (compute);
}

pub proof fn pin_sort_n_keeps_equal_keys_in_input_order()
    ensures
        num_key(w_one()) == num_key(w_zero_one()),
        pass_sort_n(seq![w_one(), w_zero_one()]) == seq![w_one(), w_zero_one()],
{
    assert(num_key(w_one()) == 1int) by (compute);
    assert(num_key(w_zero_one()) == 1int) by (compute);
    assert(pass_sort_n(seq![w_one(), w_zero_one()]) == seq![w_one(), w_zero_one()])
        by (compute);
}

// pins: `-r` and `-nr` reverse, and the glued `-nr` spelling decodes like `-n`
// `-r`; an unknown word is refused.
pub proof fn pin_sort_r_reverses_the_two_line_file()
    ensures
        // byte order puts "01" before "1"; the reverse puts it back
        pass_sort(seq![w_one(), w_zero_one()]) == seq![w_zero_one(), w_one()],
        rev(pass_sort(seq![w_one(), w_zero_one()])) == seq![w_one(), w_zero_one()],
        desc_lex(rev(pass_sort(seq![w_one(), w_zero_one()]))),
{
    sort_r_is_a_descending_permutation(seq![w_one(), w_zero_one()]);
    assert(pass_sort(seq![w_one(), w_zero_one()]) == seq![w_zero_one(), w_one()])
        by (compute);
    assert(rev(seq![w_zero_one(), w_one()]) == seq![w_one(), w_zero_one()]) by (compute);
}

pub proof fn pin_sort_nr_descends()
    ensures
        rev(pass_sort_n(seq![w_ten(), w_nine()])) == seq![w_ten(), w_nine()],
        desc_key(rev(pass_sort_n(seq![w_ten(), w_nine()]))),
{
    sort_nr_is_a_descending_key_permutation(seq![w_ten(), w_nine()]);
    assert(pass_sort_n(seq![w_ten(), w_nine()]) == seq![w_nine(), w_ten()]) by (compute);
    assert(rev(seq![w_nine(), w_ten()]) == seq![w_ten(), w_nine()]) by (compute);
}

pub proof fn pin_flag_spelling_nr_matches_n_then_r()
    ensures
        decode_flags(seq![seq![45int, 110int, 114int]], 0, false, false, false)
            == Some((true, false, true)),
        decode_flags(seq![seq![45int, 110int], seq![45int, 114int]], 0, false, false, false)
            == Some((true, false, true)),
        decode_flags(seq![seq![45int, 114int, 110int]], 0, false, false, false)
            == Some((true, false, true)),
{
    assert(decode_flags(seq![seq![45int, 110int, 114int]], 0, false, false, false)
        == Some((true, false, true))) by (compute);
    assert(decode_flags(seq![seq![45int, 110int], seq![45int, 114int]], 0, false, false, false)
        == Some((true, false, true))) by (compute);
    assert(decode_flags(seq![seq![45int, 114int, 110int]], 0, false, false, false)
        == Some((true, false, true))) by (compute);
}

pub proof fn pin_unknown_flag_is_refused()
    ensures
        decode_flags(seq![seq![45int, 117int]], 0, false, false, false)
            == None::<(bool, bool, bool)>,
        decode_flags(seq![seq![45int, 110int], seq![45int, 117int]], 0, false, false, false)
            == None::<(bool, bool, bool)>,
{
    assert(decode_flags(seq![seq![45int, 117int]], 0, false, false, false)
        == None::<(bool, bool, bool)>) by (compute);
    assert(decode_flags(seq![seq![45int, 110int], seq![45int, 117int]], 0, false, false, false)
        == None::<(bool, bool, bool)>) by (compute);
}

} // verus!

fn main() {}
