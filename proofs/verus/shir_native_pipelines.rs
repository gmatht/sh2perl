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
// Proved here:
//   xargs_splits_on_whitespace   d-F4-1: content with a double space prints one
//                                space through the shell and two through the fold
//   xargs_empty_content_diverges d-F4-2: `echo "" | xargs echo A` — the fold
//                                emits `A<space>`, the shell emits `A`
//   comm_12_one_common_line      the emitted `-12` walk prints an equal line once
//                                and advances BOTH cursors (the double advance,
//                                which is what makes duplicates print once)
//
// NOT proved in this stage, stated plainly rather than papered over:
//   * xargs_appends_the_input_word — the general "one word appended" law. The
//     statement and its induction are in this file's history; the separator-join
//     induction needs `(ws + [c]).skip(1) == ws.skip(1) + [c]` and seq-concat
//     associativity at the same time, which does not discharge here yet (it is
//     the one open obligation, d-F4-1's complement).
//   * the general `comm -12` intersection law over two distinct sorted files —
//     needs ordered-membership invariants for the two-pointer walk (d-F4-4).
//   * `diff`'s output — the plan already budgets only "byte-identical inputs are
//     silent", and even that goes through the LCS backtrack (d-F4-5).
//   * `sort`'s sorted-permutation law and `ls`'s `|| echo` status wiring
//     (d-F4-3).

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


// ────────────────────────────────────────────────── §3  pins ──────────────

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

/// pin: the fold's own separator is what a one-word echo writes.
pub proof fn pin_one_word_echo_is_the_word_plus_newline()
    ensures
        echo_bytes(seq![seq![65int]]) == seq![65int, 10int],
{
    assert(echo_bytes(seq![seq![65int]]) == seq![65int, 10int]) by (compute);
}

} // verus!

fn main() {}
