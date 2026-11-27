//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_herestring.rs — cluster C of the
//  `shir_native_stmt` decomposition: an EMPTY-INPUT herestring collapses to
//  the status bash really returns, and to nothing else.
//
//  `transforms/shir_native_stmt.rs::empty_herestring_to_status` rewrites a
//  `cmd … <<< ''` redirect (empty target, fd 0, one redirect, a single exec)
//  into `status_exec(ok)`. Two sides, and this file proves the refinement:
//
//    collapse           the pass's analysis, transcribed: its word loops as
//                       counters (`plain`, `has_dyn`, `has_c`, `dash_ok`,
//                       `one_lit_pattern`) and its three command arms.
//    bash               the semantic side, written from what the commands do
//                       with EMPTY stdin: `cat` copies nothing (status 0),
//                       `tr` with at least one argument translates nothing
//                       (status 0; bare tr errors), `grep PAT` finds no match
//                       (status 1, no output), `grep -c` would PRINT `0`
//                       (so it cannot collapse), a file operand reads a file
//                       (unknown), an interpolation may expand to anything
//                       (unknown). `s_*` are bash's own recursions.
//
//  The law: `collapse` only ever returns a status `bash` really returns.
//
//    empty_herestring_emits_the_true_status
//        collapse(cmd, empty, ws) = Some(ok)  ⇒
//        bash(cmd, ws) = Some(Out{ bytes: "", status: 0 if ok else 1 })
//
//  A forgotten guard on the pass's side therefore disagrees with `bash` on a
//  refinement law: accepting an arbitrary dynamic word, for example, makes
//  `collapse` return a status for a word list whose runtime text is unknown
//  (mutant m6 below, which no pin computes — it fails the LAW).
//
//  ── what is modelled, and what is not ──────────────────────────────────
//
//  * The statement SHAPE gates (exactly one redirect, `mode == herestring`,
//    `fd.unwrap_or(0) == 0`, the target is `IrExpr::Str("")`, the body is
//    one `IrStmt::Expr`, `exec_parts` yields `exec|builtin` with a literal
//    command and an `Array`) are structural: the model starts from
//    "`collapse` was handed a well-shaped herestring", with the target's
//    emptiness kept as `empty_target` because one pin refuses the non-empty
//    case.
//  * `cat <<< ''` with the operand `-` is modelled (`dash_ok`); `cat`
//    reading an unreadable stdin cannot happen here.
//  * `tr`'s ARGUMENT VALIDITY is not modelled: the source accepts any
//    non-empty argument list, and the semantic side does too (`tr` over
//    empty input produces no bytes either way). An interpolated `tr`
//    argument is accepted by both sides; whether it expands to zero words is
//    a word-splitting question outside this cluster.
//  * `grep`'s pattern LANGUAGE (BRE) is not modelled: no input line can
//    match, so only the pattern's WORD SHAPE (one non-empty literal, no
//    `-c`, no file operand) matters. A pattern flag other than `-c` (e.g.
//    `-i`, `-q`) cannot change a no-match status.
//  * Out of model, stated: the IR traversal that reaches the redirect (the
//    edit is bottom-up), `lower_printf`, and every other fold in the file —
//    this is cluster C only, not a law about `shir_native_stmt` as a whole.
//  ─────────────────────────────────────────────────────────────────────────

//  ── measured mutants (6; each restored byte-identically, sha256) ────────
//
//  baseline 20 verified, 0 errors
//
//  counted (no pin computes this shape, so the proof itself goes red):
//    m6  the analysis accepts a dynamic word        19 verified, 1 error
//        lemma_dyn (has_dyn == s_dyn), the refinement bridge the law rests
//        on: a Dyn word has unknown runtime text, so bash returns None while
//        the pass emits a status.
//
//  rejected at simplification by a concrete `by (compute)` pin:
//    m1  bare `tr` collapses to `true`              (pin_refusals)
//    m2  the `-c` refusal dropped                   (pin_refusals)
//    m3  a grep file operand collapses              (pin_refusals)
//    m4  `cat` accepts a file operand               (pin_refusals)
//    m5  an interpolated pattern accepted           (pin_refusals)
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

pub enum W {
    Lit(Seq<char>),
    /// an interpolated word: expands at runtime, so its text is unknown
    Interp,
    /// an arbitrary dynamic word (command substitution, arithmetic, …)
    Dyn,
}

pub enum Cmd {
    Cat,
    Tr,
    Grep,
    Other,
}

pub struct Out {
    pub bytes: Seq<char>,
    pub status: int,
}

pub open spec fn empty_bytes() -> Seq<char> {
    Seq::empty()
}

// ══════════════════════════════════════ the pass's analysis (`collapse`) ──

/// a word the source classifies as a grep FLAG: a literal starting with `-`
pub open spec fn starts_dash(w: W) -> bool {
    match w {
        W::Lit(s) => s.len() > 0 && s[0] == '-',
        _ => false,
    }
}

/// the source's `-c` test: a literal flag exactly equal to `-c`
pub open spec fn is_c(w: W) -> bool {
    match w {
        W::Lit(s) => s.len() == 2 && s[0] == '-' && s[1] == 'c',
        _ => false,
    }
}

/// the source's `plain` list length: non-flag literals and interpolations
pub open spec fn plain(ws: Seq<W>, i: int) -> int
    decreases ws.len() - i,
{
    if i >= ws.len() {
        0
    } else {
        (match ws[i] {
            W::Lit(_) => if starts_dash(ws[i]) { 0int } else { 1int },
            W::Interp => 1,
            W::Dyn => 0,
        }) + plain(ws, i + 1)
    }
}

/// the source's `_ => return false` arm: a word that is neither a literal
/// nor an interpolation
pub open spec fn has_dyn(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        (match ws[i] {
            W::Dyn => true,
            _ => false,
        }) || has_dyn(ws, i + 1)
    }
}

pub open spec fn has_c(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        is_c(ws[i]) || has_c(ws, i + 1)
    }
}

/// `cat`: no operands, or the single operand `-` (both read stdin)
pub open spec fn dash_ok(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        true
    } else {
        (match ws[i] {
            W::Lit(s) => s.len() == 1 && s[0] == '-',
            _ => false,
        }) && dash_ok(ws, i + 1)
    }
}

/// grep's pattern: exactly one NON-FLAG word (the `plain.len() == 1` check is
/// separate) and that word is a literal with non-empty text. Flags are
/// skipped; an interpolation or a dynamic word is refused.
pub open spec fn one_lit_pattern(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else if starts_dash(ws[i]) {
        one_lit_pattern(ws, i + 1)
    } else {
        match ws[i] {
            W::Lit(s) => s.len() > 0,
            _ => false,
        }
    }
}

pub open spec fn collapse(cmd: Cmd, empty_target: bool, ws: Seq<W>) -> Option<bool> {
    if !empty_target {
        Option::None
    } else {
        match cmd {
            Cmd::Cat => if dash_ok(ws, 0) {
                Option::Some(true)
            } else {
                Option::None
            },
            Cmd::Tr => if ws.len() > 0 {
                Option::Some(true)
            } else {
                Option::None
            },
            Cmd::Grep => if plain(ws, 0) == 1 && !has_dyn(ws, 0) && !has_c(ws, 0)
                && one_lit_pattern(ws, 0)
            {
                Option::Some(false)
            } else {
                Option::None
            },
            Cmd::Other => Option::None,
        }
    }
}

// ══════════════════════════════════════ the semantic side (`bash`) ──────
//
// Written from the commands' behaviour over EMPTY stdin, as their own
// recursions, so a guard forgotten in `collapse` above disagrees here.

/// `cat` copies stdin when it has no operands or the operand `-`
pub open spec fn s_dash(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        true
    } else {
        (match ws[i] {
            W::Lit(s) => s.len() == 1 && s[0] == '-',
            _ => false,
        }) && s_dash(ws, i + 1)
    }
}

/// grep's non-flag word count, from the runtime word list
pub open spec fn s_plain(ws: Seq<W>, i: int) -> int
    decreases ws.len() - i,
{
    if i >= ws.len() {
        0
    } else {
        (match ws[i] {
            W::Lit(_) => if starts_dash(ws[i]) { 0int } else { 1int },
            W::Interp => 1,
            W::Dyn => 0,
        }) + s_plain(ws, i + 1)
    }
}

/// a dynamic word has unknown runtime text: `grep`'s words would be unknown
pub open spec fn s_dyn(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        (match ws[i] {
            W::Dyn => true,
            _ => false,
        }) || s_dyn(ws, i + 1)
    }
}

/// `grep -c` prints a count (`0`) even with no match, so it cannot collapse
pub open spec fn s_c(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        (match ws[i] {
            W::Lit(s) => s.len() == 2 && s[0] == '-' && s[1] == 'c',
            _ => false,
        }) || s_c(ws, i + 1)
    }
}

/// exactly one non-flag word, and it is a literal with non-empty text
pub open spec fn s_pattern(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else if starts_dash(ws[i]) {
        s_pattern(ws, i + 1)
    } else {
        match ws[i] {
            W::Lit(s) => s.len() > 0,
            _ => false,
        }
    }
}

pub open spec fn bash(cmd: Cmd, ws: Seq<W>) -> Option<Out> {
    match cmd {
        Cmd::Cat => if s_dash(ws, 0) {
            Option::Some(Out { bytes: empty_bytes(), status: 0 })
        } else {
            Option::None
        },
        Cmd::Tr => if ws.len() > 0 {
            Option::Some(Out { bytes: empty_bytes(), status: 0 })
        } else {
            Option::None
        },
        Cmd::Grep => if s_plain(ws, 0) == 1 && !s_dyn(ws, 0) && !s_c(ws, 0)
            && s_pattern(ws, 0)
        {
            Option::Some(Out { bytes: empty_bytes(), status: 1 })
        } else {
            Option::None
        },
        Cmd::Other => Option::None,
    }
}

// ═══════════════════════════════════════════════════════ the refinement ──

pub proof fn lemma_dash(ws: Seq<W>, i: int)
    ensures
        dash_ok(ws, i) == s_dash(ws, i),
    decreases ws.len() - i,
{
    if i < ws.len() {
        lemma_dash(ws, i + 1);
    }
}

pub proof fn lemma_plain(ws: Seq<W>, i: int)
    ensures
        plain(ws, i) == s_plain(ws, i),
    decreases ws.len() - i,
{
    if i < ws.len() {
        lemma_plain(ws, i + 1);
    }
}

pub proof fn lemma_dyn(ws: Seq<W>, i: int)
    ensures
        has_dyn(ws, i) == s_dyn(ws, i),
    decreases ws.len() - i,
{
    if i < ws.len() {
        lemma_dyn(ws, i + 1);
    }
}

pub proof fn lemma_c(ws: Seq<W>, i: int)
    ensures
        has_c(ws, i) == s_c(ws, i),
    decreases ws.len() - i,
{
    if i < ws.len() {
        lemma_c(ws, i + 1);
    }
}

pub proof fn lemma_pattern(ws: Seq<W>, i: int)
    ensures
        one_lit_pattern(ws, i) == s_pattern(ws, i),
    decreases ws.len() - i,
{
    if i < ws.len() {
        lemma_pattern(ws, i + 1);
    }
}

/// THE LAW: whenever the analysis collapses the herestring, the status it
/// emits is the one bash returns and both sides print nothing.
pub proof fn empty_herestring_emits_the_true_status(
    cmd: Cmd,
    empty_target: bool,
    ws: Seq<W>,
    ok: bool,
)
    requires
        collapse(cmd, empty_target, ws) == Option::Some(ok),
    ensures
        bash(cmd, ws) == Option::Some(
            Out { bytes: empty_bytes(), status: if ok { 0int } else { 1int } },
        ),
{
    lemma_dash(ws, 0);
    lemma_plain(ws, 0);
    lemma_dyn(ws, 0);
    lemma_c(ws, 0);
    lemma_pattern(ws, 0);
    match cmd {
        Cmd::Cat => {},
        Cmd::Tr => {},
        Cmd::Grep => {},
        Cmd::Other => {},
    }
}

// ═══════════════════════════════════════════════════════════════ pins ──

pub open spec fn lit(s: Seq<char>) -> W {
    W::Lit(s)
}

/// the collapses the transform must make (and the status it must emit)
pub proof fn pin_shapes()
    ensures
        collapse(Cmd::Cat, true, Seq::<W>::empty()) == Option::Some(true),
        collapse(Cmd::Cat, true, seq![lit(seq!['-'])]) == Option::Some(true),
        collapse(Cmd::Tr, true, seq![lit(seq!['a', 'b'])]) == Option::Some(true),
        collapse(Cmd::Tr, true, seq![lit(seq!['a']), lit(seq!['b'])]) == Option::Some(true),
        collapse(Cmd::Grep, true, seq![lit(seq!['p'])]) == Option::Some(false),
        collapse(Cmd::Grep, true, seq![lit(seq!['-', 'i']), lit(seq!['p'])])
            == Option::Some(false),
{
    assert(collapse(Cmd::Cat, true, Seq::<W>::empty()) == Option::Some(true)) by (compute);
    assert(collapse(Cmd::Cat, true, seq![lit(seq!['-'])]) == Option::Some(true)) by (compute);
    assert(collapse(Cmd::Tr, true, seq![lit(seq!['a', 'b'])]) == Option::Some(true)) by (compute);
    assert(collapse(Cmd::Tr, true, seq![lit(seq!['a']), lit(seq!['b'])]) == Option::Some(true))
        by (compute);
    assert(collapse(Cmd::Grep, true, seq![lit(seq!['p'])]) == Option::Some(false)) by (compute);
    assert(collapse(Cmd::Grep, true, seq![lit(seq!['-', 'i']), lit(seq!['p'])])
        == Option::Some(false)) by (compute);
}

/// the load-bearing refusals: each of these shapes keeps the shell-out
pub proof fn pin_refusals()
    ensures
        // bare `tr` exits 1 — it must not collapse to `true`
        collapse(Cmd::Tr, true, Seq::<W>::empty()) == Option::None,
        // `grep -c` would print `0`
        collapse(Cmd::Grep, true, seq![lit(seq!['-', 'c']), lit(seq!['p'])]) == Option::None,
        // `grep` with a file operand reads a file
        collapse(Cmd::Grep, true, seq![lit(seq!['p']), lit(seq!['f'])]) == Option::None,
        // an empty pattern
        collapse(Cmd::Grep, true, seq![lit(Seq::<char>::empty())]) == Option::None,
        // an interpolated pattern may expand to empty/many words
        collapse(Cmd::Grep, true, seq![W::Interp]) == Option::None,
        // `cat FILE` changes both streams
        collapse(Cmd::Cat, true, seq![lit(seq!['f'])]) == Option::None,
        // a NON-EMPTY herestring
        collapse(Cmd::Cat, false, Seq::<W>::empty()) == Option::None,
        // any other command
        collapse(Cmd::Other, true, Seq::<W>::empty()) == Option::None,
{
    assert(collapse(Cmd::Tr, true, Seq::<W>::empty()) == Option::None) by (compute);
    assert(collapse(Cmd::Grep, true, seq![lit(seq!['-', 'c']), lit(seq!['p'])]) == Option::None)
        by (compute);
    assert(collapse(Cmd::Grep, true, seq![lit(seq!['p']), lit(seq!['f'])]) == Option::None)
        by (compute);
    assert(collapse(Cmd::Grep, true, seq![lit(Seq::<char>::empty())]) == Option::None) by (compute);
    assert(collapse(Cmd::Grep, true, seq![W::Interp]) == Option::None) by (compute);
    assert(collapse(Cmd::Cat, true, seq![lit(seq!['f'])]) == Option::None) by (compute);
    assert(collapse(Cmd::Cat, false, Seq::<W>::empty()) == Option::None) by (compute);
    assert(collapse(Cmd::Other, true, Seq::<W>::empty()) == Option::None) by (compute);
}

/// The semantic side is not a copy of the "always true" answer: a shape the
/// analysis wrongly accepts has a semantic value of its own.
pub proof fn pin_bash_shapes()
    ensures
        bash(Cmd::Cat, Seq::<W>::empty())
            == Option::Some(Out { bytes: empty_bytes(), status: 0 }),
        bash(Cmd::Grep, seq![lit(seq!['p'])])
            == Option::Some(Out { bytes: empty_bytes(), status: 1 }),
        bash(Cmd::Tr, Seq::<W>::empty()) == Option::None,
        bash(Cmd::Cat, seq![lit(seq!['f'])]) == Option::None,
{
    assert(bash(Cmd::Cat, Seq::<W>::empty())
        == Option::Some(Out { bytes: empty_bytes(), status: 0 })) by (compute);
    assert(bash(Cmd::Grep, seq![lit(seq!['p'])])
        == Option::Some(Out { bytes: empty_bytes(), status: 1 })) by (compute);
    assert(bash(Cmd::Tr, Seq::<W>::empty()) == Option::None) by (compute);
    assert(bash(Cmd::Cat, seq![lit(seq!['f'])]) == Option::None) by (compute);
}


fn main() {}

} // verus!
