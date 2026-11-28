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
// Proved here (19 obligations, 0 errors):
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
//
// NOT proved, stated plainly rather than papered over:
//   * the general `comm -12` intersection law over two distinct sorted files —
//     needs ordered-membership invariants for the two-pointer walk (d-F4-4).
//   * diff's output beyond silence — d-F4-5b: the converse ("a change always
//     prints") and the byte-level hunks. Both need an invariant relating the
//     backtrack's cursors to prefix equality (`a[0..i] == b[0..j]`), i.e.
//     subrange-composition facts this proof does not establish; the general
//     statement was left unproved rather than restated so it agrees with itself.
//   * `sort`'s sorted-permutation law and `ls`'s `|| echo` status wiring
//     (d-F4-3).
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

} // verus!

fn main() {}
