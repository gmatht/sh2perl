// Standalone model of text_ops_planner
// (sh2perl/src/transforms/text_ops_planner.rs + the `wc -l` candidates in
// sh2perl/src/transforms/planner.rs): the pipeline
//
//   echo TEXT | wc -l
//
// is rewritten to a direct count, using the composition the TARGET
// backend's node manifest selects:
//
//   regex backend (perl/js):  RegCount(TEXT, /\n/) + 1
//   no-regex backend (c):     ArrayLen(Split(TEXT, "\n"))
//
// Proves the observable law:
//
//   the_rewrite_preserves_the_count: for EVERY word the recognizer accepts
//   and every backend, the rewritten program prints the number bash prints
//   for `echo TEXT | wc -l`. The recognizer's verdict is an input to `go`,
//   so a refused shape takes the unchanged-pipeline branch — there is no
//   shape antecedent on the law.
//
// ## What is modelled
//
// A text is a byte sequence. `wc -l` counts NEWLINE BYTES in the echoed
// stream, and `echo TEXT` always appends one terminator:
//
//   wc_l(t)          = newlines(t) + 1
//   reg_count(t)     = newlines(t)              (RegCount: occurrences)
//   split_fields(t)  = newlines(t) + 1          (ArrayLen(Split): fields,
//                                                the empty word is ONE field)
//   lower(t, regex)  = regex ? reg_count(t) + 1 : split_fields(t)
//
// ## One obligation per load-bearing rule
//
//  L1 regex_candidate_counts_the_line — the regex backend's count, plus the
//     terminator, is `wc -l`.
//  L2 split_candidate_counts_the_line — the split composition needs no
//     terminator fudge: the echoed line IS the final field.
//
// ## What is NOT modelled
//
//   - The recognizer (the exact `echo TEXT` / `wc -l` two-stage shape, the
//     multi-word and flag refusals). It produces the `recognised` flag and
//     the text; the transform's own tests pin it.
//   - Integer overflow / formatting of the count (the corpus's decimal
//     printer is anchored elsewhere).
//   - The ESTree backend: no JS engine is installed on this host, so the
//     JS composition (`RegCount`) is proved here but only the Perl and C
//     renderings are executed (see the anchor).
//
// ## Defects the model forced (all repaired; tests/anchor pin them)
//
//   1. `RegCount(TEXT, /\n/)` counted the ARGUMENT, so `echo a | wc -l`
//      printed 0 where bash prints 1 — one short for EVERY word. Repaired by
//      adding the terminator (`+ 1`) in the planner's candidate.
//   2. The Perl rendering of RegCount was a list assignment
//      (`() = (TEXT =~ /\n/g)`), so an enclosing operator evaluated it in
//      list context and got the element count of a scalar (0): the `+ 1`
//      printed nothing. Repaired by forcing scalar context at the node.
//   3. `ArrayLen(Split(TEXT, "\n"))` said 0 for the empty word, so
//      `echo "" | wc -l` (bash 1) printed 0 on the C backend: the empty
//      argument is one empty field. Repaired in the C split primitive.
//   4. The C emitted for the rewritten program did not compile at all
//      (`unknown type name '_sh_vbuf'`): `_sh_splice_v` is emitted with the
//      value primitives, but the vbuf type was emitted only inside the
//      shell-runtime block, which the planner removes as the last user.
//      Repaired by emitting the type for either block.
//
// ## Mutants (checked red, each a real edit of this file)
//
//   drop the terminator from L1              -> regex_candidate_counts_the_line
//   split with the old empty case (0)        -> L2 at the empty witness
//   split counts `newlines + 2`              -> L2
//   count the wrong byte (97 for 10)         -> L1 + L2
//   `lower` uses the regex composition for both -> L2
//   spec reverted to the pre-fix meaning
//   (`wc_l = newlines`)                      -> the concrete pins, not L1/L2
//   always lower, even when not recognised   -> the law at `recognised=false`

use vstd::prelude::*;

verus! {

pub type Text = Seq<int>;

pub open spec fn nl_byte() -> int {
    10
}

pub open spec fn nl_count_from(t: Text, i: int) -> int
    decreases i,
{
    if i <= 0 || i > t.len() as int {
        0
    } else {
        nl_count_from(t, i - 1int) + (if t[i - 1int] == nl_byte() { 1int } else { 0int })
    }
}

/// Newline bytes in the word.
pub open spec fn nl_count(t: Text) -> int {
    nl_count_from(t, t.len() as int)
}

/// bash: `echo TEXT` writes TEXT, then the newline it appends. `wc -l`
/// counts newline bytes, so it prints the word's newlines PLUS the
/// terminator.
pub open spec fn wc_l(t: Text) -> int {
    nl_count(t) + 1
}

/// The RegCount primitive: occurrences of `/\n/` in its argument.
pub open spec fn reg_count(t: Text) -> int {
    nl_count(t)
}

/// `ArrayLen(Split(t, "\n"))` after the repair: splitting yields
/// `newlines + 1` fields, and the empty word is ONE empty field (which is
/// exactly the echoed terminator's line).
pub open spec fn split_fields(t: Text) -> int {
    nl_count(t) + 1
}

/// What the C primitive returned before the repair: 0 for the empty word.
pub open spec fn old_split_fields(t: Text) -> int {
    if t.len() == 0int {
        0
    } else {
        nl_count(t) + 1
    }
}

/// The count the rewritten program prints, for the manifest-selected
/// composition (`regex` = the backend has RegCount).
pub open spec fn lower(t: Text, regex: bool) -> int {
    if regex {
        reg_count(t) + 1
    } else {
        split_fields(t)
    }
}

/// The program's output. A non-recognised pipeline is left alone, so it
/// still prints bash's count — both branches are the same observable.
pub open spec fn go(t: Text, recognised: bool, regex: bool) -> Seq<int> {
    if recognised {
        seq![lower(t, regex)]
    } else {
        seq![wc_l(t)]
    }
}

pub open spec fn reference(t: Text) -> Seq<int> {
    seq![wc_l(t)]
}

// ---------------------------------------------------------------------------
// The laws
// ---------------------------------------------------------------------------

/// L1: the regex backend's composition is exactly `wc -l`.
proof fn regex_candidate_counts_the_line(t: Text)
    ensures
        lower(t, true) == wc_l(t),
{
}

/// L2: the split composition is exactly `wc -l` — the terminator's line is
/// the final field, so no `+ 1` is needed (and adding one would be wrong).
proof fn split_candidate_counts_the_line(t: Text)
    ensures
        lower(t, false) == wc_l(t),
{
}

/// THE LAW: for every text, every recognizer verdict and every manifest,
/// the rewritten program prints bash's count.
proof fn the_rewrite_preserves_the_count(t: Text, recognised: bool, regex: bool)
    ensures
        go(t, recognised, regex) == reference(t),
{
    regex_candidate_counts_the_line(t);
    split_candidate_counts_the_line(t);
}

/// The two compositions agree on every word (so the manifest choice is a
/// routing decision, not a correctness one).
proof fn every_candidate_agrees(t: Text)
    ensures
        lower(t, true) == lower(t, false),
{
    regex_candidate_counts_the_line(t);
    split_candidate_counts_the_line(t);
}

// ---------------------------------------------------------------------------
// Pins: each repaired rule is independently observable, and the pre-fix
// expressions fail on measured inputs
// ---------------------------------------------------------------------------

/// Before the repair, the regex composition was one short for EVERY word.
proof fn the_pre_fix_regex_count_is_one_short(t: Text)
    ensures
        reg_count(t) == wc_l(t) - 1int,
{
}

/// Before the repair, the split composition was one short for the EMPTY word
/// only — the measured `echo "" | wc -l` (bash 1, C 0).
proof fn the_pre_fix_split_drops_the_empty_line()
    ensures
        old_split_fields(Seq::empty()) == 0int,
        wc_l(Seq::empty()) == 1int,
        split_fields(Seq::empty()) == 1int,
        lower(Seq::empty(), false) == 1int,
        lower(Seq::empty(), true) == 1int,
{
}

/// Concrete measured shapes: `echo a` (1 line), `echo ""` (1), `echo "a\nb"`
/// (2), `echo "a\n"` (2). All four print the same under both compositions.
proof fn a_static_word_counts_its_lines()
    ensures
        wc_l(seq![97int]) == 1int,
        wc_l(Seq::empty()) == 1int,
        wc_l(seq![97int, nl_byte(), 98int]) == 2int,
        wc_l(seq![97int, nl_byte()]) == 2int,
        lower(seq![97int], true) == 1int,
        lower(seq![97int], false) == 1int,
        lower(seq![97int, nl_byte(), 98int], true) == 2int,
        lower(seq![97int, nl_byte(), 98int], false) == 2int,
        lower(Seq::empty(), true) == 1int,
        lower(Seq::empty(), false) == 1int,
{
    assert(nl_count(seq![97int]) == 0int) by (compute);
    assert(nl_count(Seq::empty()) == 0int) by (compute);
    assert(nl_count(seq![97int, nl_byte(), 98int]) == 1int) by (compute);
    assert(nl_count(seq![97int, nl_byte()]) == 1int) by (compute);
}

/// A refused shape is the unchanged pipeline, which prints the same count.
proof fn a_refused_shape_is_unchanged()
    ensures
        go(seq![97int], false, true) == reference(seq![97int]),
        go(seq![97int], false, false) == reference(seq![97int]),
{
}

/// The law instantiates on both backends and both recognizer verdicts.
proof fn law_at_the_witnesses()
    ensures
        go(seq![97int], true, true) == reference(seq![97int]),
        go(seq![97int], true, false) == reference(seq![97int]),
        go(Seq::empty(), true, true) == reference(Seq::empty()),
        go(Seq::empty(), true, false) == reference(Seq::empty()),
        go(seq![97int, nl_byte(), 98int], true, true) == reference(seq![97int, nl_byte(), 98int]),
        go(seq![97int, nl_byte(), 98int], true, false) == reference(seq![97int, nl_byte(), 98int]),
        go(seq![97int], false, false) == reference(seq![97int]),
{
    the_rewrite_preserves_the_count(seq![97int], true, true);
    the_rewrite_preserves_the_count(seq![97int], true, false);
    the_rewrite_preserves_the_count(Seq::empty(), true, true);
    the_rewrite_preserves_the_count(Seq::empty(), true, false);
    the_rewrite_preserves_the_count(seq![97int, nl_byte(), 98int], true, true);
    the_rewrite_preserves_the_count(seq![97int, nl_byte(), 98int], true, false);
    the_rewrite_preserves_the_count(seq![97int], false, false);
}

fn main() {}

} // verus!
