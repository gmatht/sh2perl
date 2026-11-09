// Standalone model of interp-lit-merge
// (sh2perl/src/transforms/interp_lit_merge.rs): an `Expr` part whose
// value is a frozen literal `Str` (DoubleQuoted | SingleQuoted) becomes
// a `Lit` part carrying that text, then adjacent `Lit`s merge into one.
// Proves the denotation-preservation obligation:
//
//   merge_sound: the merged part list denotes exactly the concat the
//   original denoted — renderers append parts in order, so the fold
//   only removes the SLOT BOUNDARY between adjacent literals.
//
// Strings are Seq<char> (the sibling-proof convention). The quoting
// style gate is data (`plain_quoted`: DoubleQuoted | SingleQuoted fold,
// Command | Heredoc | Raw refuse — the load-bearing half, per the
// module doc "Soundness"). The traversal (`stmt_pass`/`expr_pass`
// walkers) and the per-backend rollout in `transform_applies` are
// placement, not value (ternary_desugar precedent).
//
// What is modelled and what is NOT (fidelity, checked against the
// source's 7 unit tests):
//   - the two-phase rewrite verbatim: phase 1 folds every
//     plain-quoted frozen-literal Expr part (a fold in step 1 can mint
//     a pair that was not adjacent before, so phase 2 REBUILDS — the
//     changed and 2' arms rebuild identically); phase 2 merges adjacent
//     Lits left-associatively (`prev.push_str(cur)`).
//   - the three style refusals as pins (Command, Heredoc, Raw) plus the
//     non-literal-Expr refusal, mirroring the source tests.
//   - idempotence: a merged list has no foldable Expr part and no
//     adjacent Lit pair, so a second pass is the identity (the source's
//     own idempotence assertions).
//
// MUTANT (checked red): folding Command-style parts too (dropping the
// plain-quoting gate) makes the same command report 23 verified, 1
// error — pin_command_style_refused goes red. The file was restored
// and re-checked at 24 verified, 0 errors. Note the denotation
// theorem still holds without the gate (both sides append the same
// text) — the pin, not the soundness lemma, is load-bearing here,
// exactly the slice_identity/exec_call_fold shape: the gates preserve
// renderer-visible semantics (unrendered substitutions, Perl `$`/`@`
// interpolation), not value.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 interp_lit_merge.rs
//   -> verification results:: 24 verified, 0 errors
use vstd::prelude::*;

verus! {

// Quoting styles: only the two plain-quoting styles carry their text
// as the value (the other three carry renderer-visible semantics).
pub enum Style {
    DoubleQuoted,
    SingleQuoted,
    Command,
    Heredoc,
    Raw,
}

pub open spec fn plain_quoted(s: Style) -> bool {
    match s {
        Style::DoubleQuoted => true,
        Style::SingleQuoted => true,
        Style::Command => false,
        Style::Heredoc => false,
        Style::Raw => false,
    }
}

// Interpolate parts: frozen-literal carriers (Lit text, or an Expr
// wrapping a Str of some style) and opaque parts the fold never touches.
pub enum Part {
    Lit(Seq<char>),
    ExprStr(Seq<char>, Style),
    Opaque,
}

// The denotation: parts appended in order (what every renderer prints).
// A frozen-literal Expr part denotes its text regardless of style —
// the STYLE gate decides whether the fold may fire, not the value.
pub open spec fn denote(ps: Seq<Part>) -> Seq<char>
    decreases ps.len(),
{
    if ps.len() == 0 {
        Seq::empty()
    } else {
        part_den(ps[0]).add(denote(ps.drop_first()))
    }
}

pub open spec fn part_den(p: Part) -> Seq<char> {
    match p {
        Part::Lit(t) => t,
        Part::ExprStr(t, _) => t,
        Part::Opaque => Seq::empty(),
    }
}

// Phase 1: fold every plain-quoted frozen-literal Expr part to Lit.
pub open spec fn fold_exprs(ps: Seq<Part>) -> Seq<Part>
    decreases ps.len(),
{
    if ps.len() == 0 {
        Seq::empty()
    } else {
        seq![fold_head(ps[0])].add(fold_exprs(ps.drop_first()))
    }
}

// Phase 2: merge adjacent Lits left-associatively (prev ++ cur).
pub open spec fn merge_lits(ps: Seq<Part>) -> Seq<Part>
    decreases ps.len(),
{
    if ps.len() == 0 {
        Seq::empty()
    } else if ps.len() == 1 {
        ps
    } else {
        match (ps[0], ps[1]) {
            (Part::Lit(a), Part::Lit(b)) => merge_lits(seq![Part::Lit(a.add(b))].add(ps.skip(2))),
            _ => seq![ps[0]].add(merge_lits(ps.drop_first())),
        }
    }
}

// The whole rewrite: fold then merge (both source arms rebuild
// identically, so one composition covers them).
pub open spec fn merge(ps: Seq<Part>) -> Seq<Part> {
    merge_lits(fold_exprs(ps))
}

// Cons unfolding for the denotation: denote(ps) via head/tail.
proof fn denote_cons(ps: Seq<Part>)
    requires ps.len() >= 1,
    ensures denote(ps) == part_den(ps[0]).add(denote(ps.drop_first())),
{
    assert(ps.len() != 0);
    assert(denote(ps) == part_den(ps[0]).add(denote(ps.drop_first())));
}

// Cons unfolding for phase 1: fold_exprs(ps) via head/tail.
proof fn fold_exprs_cons(ps: Seq<Part>)
    requires ps.len() >= 1,
    ensures fold_exprs(ps) == seq![fold_head(ps[0])].add(fold_exprs(ps.drop_first())),
{
    assert(ps.len() != 0);
    assert(fold_exprs(ps) == seq![fold_head(ps[0])].add(fold_exprs(ps.drop_first())));
}

// The per-part fold: plain-quoted frozen literals become Lit.
pub open spec fn fold_head(h: Part) -> Part {
    match h {
        Part::ExprStr(t, s) => {
            if plain_quoted(s) {
                Part::Lit(t)
            } else {
                h
            }
        },
        _ => h,
    }
}

// A folded head denotes its source part.
proof fn fold_head_sound(h: Part)
    ensures part_den(fold_head(h)) == part_den(h),
{
    match h {
        Part::ExprStr(t, s) => {
            if plain_quoted(s) {
            }
        },
        _ => {},
    }
}

// Phase 1 preserves the denotation (a folded part denotes its text).
proof fn fold_exprs_sound(ps: Seq<Part>)
    ensures denote(fold_exprs(ps)) == denote(ps),
    decreases ps.len(),
{
    if ps.len() == 0 {
        assert(fold_exprs(ps) == Seq::<Part>::empty());
        assert(denote(ps) == Seq::<char>::empty());
    } else {
        fold_exprs_cons(ps);
        assert(fold_exprs(ps)[0] == fold_head(ps[0]));
        assert(fold_exprs(ps).drop_first() == fold_exprs(ps.drop_first()));
        denote_cons(fold_exprs(ps));
        denote_cons(ps);
        fold_head_sound(ps[0]);
        fold_exprs_sound(ps.drop_first());
    }
}

// Cons unfolding for phase 2 (non-merging head): merge_lits(ps).
proof fn merge_lits_cons(ps: Seq<Part>)
    requires
        ps.len() >= 1 && (ps.len() == 1 || !(match (ps[0], ps[1]) {
            (Part::Lit(_), Part::Lit(_)) => true,
            _ => false,
        })),
    ensures merge_lits(ps) == seq![ps[0]].add(merge_lits(ps.drop_first())),
{
    if ps.len() == 1 {
        assert(merge_lits(ps) == ps);
        assert(ps == seq![ps[0]].add(merge_lits(ps.drop_first())));
    } else {
        assert(ps.len() >= 2);
        assert(merge_lits(ps) == seq![ps[0]].add(merge_lits(ps.drop_first())));
    }
}

// Merging head pair: merge_lits(ps) with two leading Lits.
proof fn merge_lits_merge_head(ps: Seq<Part>)
    requires
        ps.len() >= 2 && match (ps[0], ps[1]) {
            (Part::Lit(_), Part::Lit(_)) => true,
            _ => false,
        },
    ensures merge_lits(ps) == merge_lits(
        seq![Part::Lit(lit_text(ps[0]).add(lit_text(ps[1])))].add(ps.skip(2)),
    ),
{
    assert(ps.len() >= 2);
    assert(merge_lits(ps) == merge_lits(
        seq![Part::Lit(lit_text(ps[0]).add(lit_text(ps[1])))].add(ps.skip(2)),
    ));
}

// The text of a Lit part (defined on all parts; meaningful for Lit).
pub open spec fn lit_text(p: Part) -> Seq<char> {
    match p {
        Part::Lit(t) => t,
        Part::ExprStr(t, _) => t,
        Part::Opaque => Seq::empty(),
    }
}

// Base cases: empty and singleton lists are fixed points of both phases.
proof fn fold_exprs_nil()
    ensures fold_exprs(Seq::<Part>::empty()) == Seq::<Part>::empty(),
{
}

proof fn merge_lits_nil()
    ensures merge_lits(Seq::<Part>::empty()) == Seq::<Part>::empty(),
{
}

proof fn merge_lits_singleton(x: Part)
    ensures merge_lits(seq![x]) == seq![x],
{
}

// Single-element phase-1 expansion.
proof fn fold_exprs_single(h: Part)
    ensures fold_exprs(seq![h]) == seq![fold_head(h)],
{
    fold_exprs_nil();
    assert(seq![h].drop_first() == Seq::<Part>::empty());
    assert(seq![fold_head(h)].add(Seq::<Part>::empty()) == seq![fold_head(h)]);
}

// Single-element denotation expansion.
proof fn denote_single(h: Part)
    ensures denote(seq![h]) == part_den(h),
{
    assert(seq![h].drop_first() == Seq::<Part>::empty());
    assert(denote(Seq::<Part>::empty()) == Seq::<char>::empty());
    assert(part_den(h).add(Seq::<char>::empty()) == part_den(h));
}

// Phase 2 preserves the denotation (concat associativity at the seam).
proof fn merge_lits_sound(ps: Seq<Part>)
    ensures denote(merge_lits(ps)) == denote(ps),
    decreases ps.len(),
{
    if ps.len() == 0 {
        assert(merge_lits(ps) == Seq::<Part>::empty());
        assert(denote(ps) == Seq::<char>::empty());
    } else if ps.len() == 1 {
        assert(merge_lits(ps) == ps);
    } else {
        match (ps[0], ps[1]) {
            (Part::Lit(a), Part::Lit(b)) => {
                merge_lits_merge_head(ps);
                assert(lit_text(ps[0]) == a);
                assert(lit_text(ps[1]) == b);
                assert(ps.skip(2) == ps.drop_first().drop_first());
                // Index links: the tail's head is ps[1].
                assert(ps.drop_first()[0] == ps[1]);
                assert(ps.drop_first().drop_first() == ps.skip(2));
                // Projections on the constructed merged term.
                assert(part_den(ps[0]) == a);
                assert(part_den(ps[1]) == b);
                assert(
                    seq![Part::Lit(a.add(b))].add(ps.skip(2))[0] == Part::Lit(a.add(b))
                );
                assert(seq![Part::Lit(a.add(b))].add(ps.skip(2)).drop_first() == ps.skip(2));
                assert(part_den(Part::Lit(a.add(b))) == a.add(b));
                merge_lits_sound(seq![Part::Lit(a.add(b))].add(ps.skip(2)));
                denote_cons(ps);
                denote_cons(ps.drop_first());
                denote_cons(seq![Part::Lit(a.add(b))].add(ps.skip(2)));
                // Concat associativity at the seam: a ++ (b ++ d).
                assert(
                    a.add(b.add(denote(ps.skip(2)))) == a.add(b).add(denote(ps.skip(2)))
                );
            },
            _ => {
                merge_lits_cons(ps);
                assert(merge_lits(ps).len() >= 1);
                assert(merge_lits(ps)[0] == ps[0]);
                assert(merge_lits(ps).drop_first() == merge_lits(ps.drop_first()));
                denote_cons(merge_lits(ps));
                denote_cons(ps);
                merge_lits_sound(ps.drop_first());
            },
        }
    }
}

// THE OBLIGATION: the merged list denotes the original concat.
proof fn merge_sound(ps: Seq<Part>)
    ensures denote(merge(ps)) == denote(ps),
{
    fold_exprs_sound(ps);
    merge_lits_sound(fold_exprs(ps));
}

// ---- Fidelity pins (mirror the source's 7 unit tests) ----

// The measured corpus shape folds and merges to one Lit.
proof fn pin_frozen_literal_folds_and_merges()
    ensures merge(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
            Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
        ]
    ) == seq![
        Part::Lit(
            seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ', 'H', 'e', 'l', 'l', 'o']
        )
    ]
{
    // Phase 1: the Expr part folds (heads already literal / plain-quoted).
    assert(
        merge(
            seq![
                Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
                Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
            ]
        ) == merge_lits(
            fold_exprs(
                seq![
                    Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
                    Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
                ]
            )
        )
    );
    fold_exprs_cons(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
            Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
        ]
    );
    assert(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
            Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
        ][0] == Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' '])
    );
    assert(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
            Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
        ].drop_first() == seq![
            Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
        ]
    );
    assert(
        fold_head(
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
        ) == Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' '])
    );
    fold_exprs_single(Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted));
    assert(fold_head(Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)) == Part::Lit(seq!['H', 'e', 'l', 'l', 'o']));
    assert(
        fold_exprs(
            seq![
                Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
                Part::ExprStr(seq!['H', 'e', 'l', 'l', 'o'], Style::DoubleQuoted)
            ]
        ) == seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
            Part::Lit(seq!['H', 'e', 'l', 'l', 'o'])
        ]
    );
    merge_lits_merge_head(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
            Part::Lit(seq!['H', 'e', 'l', 'l', 'o'])
        ]
    );
    assert(
        lit_text(
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
        ) == seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']
    );
    assert(lit_text(Part::Lit(seq!['H', 'e', 'l', 'l', 'o'])) == seq!['H', 'e', 'l', 'l', 'o']);
    assert(
        seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' '].add(
            seq!['H', 'e', 'l', 'l', 'o'],
        ) == seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ', 'H', 'e', 'l', 'l', 'o']
    );
    assert(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ']),
            Part::Lit(seq!['H', 'e', 'l', 'l', 'o'])
        ].skip(2) == Seq::<Part>::empty()
    );
    merge_lits_singleton(Part::Lit(seq!['E', 'c', 'h', 'o', ' ', 'r', 'e', 's', 'u', 'l', 't', ':', ' ', 'H', 'e', 'l', 'l', 'o']));
}

// Pre-existing adjacent Lits merge with no fold.
proof fn pin_adjacent_lits_merge()
    ensures merge(seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])]) == seq![
        Part::Lit(seq!['a', 'b'])
    ]
{
    assert(
        merge(seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])]) == merge_lits(
            fold_exprs(seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])])
        )
    );
    fold_exprs_cons(seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])]);
    assert(seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])][0] == Part::Lit(seq!['a']));
    assert(
        seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])].drop_first() == seq![
            Part::Lit(seq!['b'])
        ]
    );
    assert(fold_head(Part::Lit(seq!['a'])) == Part::Lit(seq!['a']));
    fold_exprs_single(Part::Lit(seq!['b']));
    assert(fold_head(Part::Lit(seq!['b'])) == Part::Lit(seq!['b']));
    assert(
        fold_exprs(seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])]) == seq![
            Part::Lit(seq!['a']),
            Part::Lit(seq!['b'])
        ]
    );
    assert(seq!['a'].add(seq!['b']) == seq!['a', 'b']);
    merge_lits_merge_head(seq![Part::Lit(seq!['a']), Part::Lit(seq!['b'])]);
    assert(lit_text(Part::Lit(seq!['a'])) == seq!['a']);
    assert(lit_text(Part::Lit(seq!['b'])) == seq!['b']);
    merge_lits_singleton(Part::Lit(seq!['a', 'b']));
}

// A middle literal merges after the surrounding folds.
proof fn pin_middle_literal_merges()
    ensures merge(
        seq![
            Part::ExprStr(seq!['a'], Style::DoubleQuoted),
            Part::Lit(seq![' ']),
            Part::ExprStr(seq!['b'], Style::DoubleQuoted)
        ]
    ) == seq![Part::Lit(seq!['a', ' ', 'b'])]
{
    // Phase 1 folds all three parts (two Expr + one Lit head chain).
    assert(
        merge(
            seq![
                Part::ExprStr(seq!['a'], Style::DoubleQuoted),
                Part::Lit(seq![' ']),
                Part::ExprStr(seq!['b'], Style::DoubleQuoted)
            ]
        ) == merge_lits(
            fold_exprs(
                seq![
                    Part::ExprStr(seq!['a'], Style::DoubleQuoted),
                    Part::Lit(seq![' ']),
                    Part::ExprStr(seq!['b'], Style::DoubleQuoted)
                ]
            )
        )
    );
    fold_exprs_cons(
        seq![
            Part::ExprStr(seq!['a'], Style::DoubleQuoted),
            Part::Lit(seq![' ']),
            Part::ExprStr(seq!['b'], Style::DoubleQuoted)
        ]
    );
    assert(
        seq![
            Part::ExprStr(seq!['a'], Style::DoubleQuoted),
            Part::Lit(seq![' ']),
            Part::ExprStr(seq!['b'], Style::DoubleQuoted)
        ][0] == Part::ExprStr(seq!['a'], Style::DoubleQuoted)
    );
    assert(
        seq![
            Part::ExprStr(seq!['a'], Style::DoubleQuoted),
            Part::Lit(seq![' ']),
            Part::ExprStr(seq!['b'], Style::DoubleQuoted)
        ].drop_first() == seq![
            Part::Lit(seq![' ']),
            Part::ExprStr(seq!['b'], Style::DoubleQuoted)
        ]
    );
    assert(
        fold_head(Part::ExprStr(seq!['a'], Style::DoubleQuoted)) == Part::Lit(seq!['a'])
    );
    fold_exprs_cons(seq![Part::Lit(seq![' ']), Part::ExprStr(seq!['b'], Style::DoubleQuoted)]);
    assert(
        seq![Part::Lit(seq![' ']), Part::ExprStr(seq!['b'], Style::DoubleQuoted)][0] == Part::Lit(
            seq![' '],
        )
    );
    assert(
        seq![Part::Lit(seq![' ']), Part::ExprStr(seq!['b'], Style::DoubleQuoted)].drop_first() == seq![
            Part::ExprStr(seq!['b'], Style::DoubleQuoted)
        ]
    );
    assert(fold_head(Part::Lit(seq![' '])) == Part::Lit(seq![' ']));
    fold_exprs_single(Part::ExprStr(seq!['b'], Style::DoubleQuoted));
    assert(fold_head(Part::ExprStr(seq!['b'], Style::DoubleQuoted)) == Part::Lit(seq!['b']));
    assert(
        fold_exprs(
            seq![
                Part::ExprStr(seq!['a'], Style::DoubleQuoted),
                Part::Lit(seq![' ']),
                Part::ExprStr(seq!['b'], Style::DoubleQuoted)
            ]
        ) == seq![Part::Lit(seq!['a']), Part::Lit(seq![' ']), Part::Lit(seq!['b'])]
    );
    // Phase 2 merges left-associatively: ((a ++ ' ') ++ b).
    assert(seq!['a'].add(seq![' ']) == seq!['a', ' ']);
    assert(
        seq![Part::Lit(seq!['a']), Part::Lit(seq![' ']), Part::Lit(seq!['b'])] == seq![
            Part::Lit(seq!['a']),
            Part::Lit(seq![' '])
        ].add(seq![Part::Lit(seq!['b'])])
    );
    merge_lits_merge_head(
        seq![Part::Lit(seq!['a']), Part::Lit(seq![' ']), Part::Lit(seq!['b'])]
    );
    assert(lit_text(Part::Lit(seq!['a'])) == seq!['a']);
    assert(lit_text(Part::Lit(seq![' '])) == seq![' ']);
    assert(seq!['a', ' '].add(seq!['b']) == seq!['a', ' ', 'b']);
    assert(
        seq![Part::Lit(seq!['a', ' ']), Part::Lit(seq!['b'])] == seq![
            Part::Lit(seq!['a', ' ']),
            Part::Lit(seq!['b'])
        ].add(Seq::<Part>::empty())
    );
    merge_lits_merge_head(seq![Part::Lit(seq!['a', ' ']), Part::Lit(seq!['b'])]);
    assert(lit_text(Part::Lit(seq!['a', ' '])) == seq!['a', ' ']);
    assert(lit_text(Part::Lit(seq!['b'])) == seq!['b']);
    assert(
        seq![Part::Lit(seq!['a', ' ', 'b'])].add(Seq::<Part>::empty()) == seq![
            Part::Lit(seq!['a', ' ', 'b'])
        ]
    );
    merge_lits_singleton(Part::Lit(seq!['a', ' ', 'b']));
}

// Command style (an unrendered substitution) is refused.
proof fn pin_command_style_refused()
    ensures merge(
        seq![
            Part::Lit(seq!['x', '=']),
            Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
        ]
    ) == seq![
        Part::Lit(seq!['x', '=']),
        Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
    ]
{
    assert(
        merge(
            seq![
                Part::Lit(seq!['x', '=']),
                Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
            ]
        ) == merge_lits(
            fold_exprs(
                seq![
                    Part::Lit(seq!['x', '=']),
                    Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
                ]
            )
        )
    );
    // Phase 1: Command style never folds (both heads stay).
    fold_exprs_cons(
        seq![
            Part::Lit(seq!['x', '=']),
            Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
        ]
    );
    assert(
        seq![
            Part::Lit(seq!['x', '=']),
            Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
        ][0] == Part::Lit(seq!['x', '='])
    );
    assert(
        seq![
            Part::Lit(seq!['x', '=']),
            Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
        ].drop_first() == seq![Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)]
    );
    assert(fold_head(Part::Lit(seq!['x', '='])) == Part::Lit(seq!['x', '=']));
    fold_exprs_single(Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command));
    assert(
        fold_exprs(
            seq![
                Part::Lit(seq!['x', '=']),
                Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
            ]
        ) == seq![
            Part::Lit(seq!['x', '=']),
            Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
        ]
    );
    assert(
        fold_head(Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)) == Part::ExprStr(
            seq!['d', 'a', 't', 'e'],
            Style::Command
        )
    );
    // Phase 2: a (Lit, non-Lit) pair never merges.
    merge_lits_cons(
        seq![
            Part::Lit(seq!['x', '=']),
            Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command)
        ]
    );
    merge_lits_singleton(Part::ExprStr(seq!['d', 'a', 't', 'e'], Style::Command));
}

// Heredoc and Raw styles (Perl still interpolates) are refused.
proof fn pin_heredoc_raw_refused()
    ensures merge(
        seq![Part::ExprStr(seq!['$', 'v'], Style::Heredoc)]
    ) == seq![Part::ExprStr(seq!['$', 'v'], Style::Heredoc)]
        && merge(seq![Part::ExprStr(seq!['$', 'v'], Style::Raw)]) == seq![
            Part::ExprStr(seq!['$', 'v'], Style::Raw)
        ]
{
    assert(
        merge(
            seq![Part::ExprStr(seq!['$', 'v'], Style::Heredoc)]
        ) == merge_lits(
            fold_exprs(seq![Part::ExprStr(seq!['$', 'v'], Style::Heredoc)])
        )
    );
    assert(
        merge(seq![Part::ExprStr(seq!['$', 'v'], Style::Raw)]) == merge_lits(
            fold_exprs(seq![Part::ExprStr(seq!['$', 'v'], Style::Raw)])
        )
    );
    fold_exprs_single(Part::ExprStr(seq!['$', 'v'], Style::Heredoc));
    fold_exprs_single(Part::ExprStr(seq!['$', 'v'], Style::Raw));
    assert(
        fold_head(Part::ExprStr(seq!['$', 'v'], Style::Heredoc)) == Part::ExprStr(
            seq!['$', 'v'],
            Style::Heredoc
        )
    );
    assert(
        fold_head(Part::ExprStr(seq!['$', 'v'], Style::Raw)) == Part::ExprStr(
            seq!['$', 'v'],
            Style::Raw
        )
    );
    merge_lits_singleton(Part::ExprStr(seq!['$', 'v'], Style::Heredoc));
    merge_lits_singleton(Part::ExprStr(seq!['$', 'v'], Style::Raw));
}

// A non-literal Expr is untouched (and the merged shape is stable).
proof fn pin_non_literal_untouched()
    ensures merge(seq![Part::Lit(seq!['a']), Part::Opaque]) == seq![
        Part::Lit(seq!['a']),
        Part::Opaque
    ]
{
    assert(
        merge(seq![Part::Lit(seq!['a']), Part::Opaque]) == merge_lits(
            fold_exprs(seq![Part::Lit(seq!['a']), Part::Opaque])
        )
    );
    fold_exprs_cons(seq![Part::Lit(seq!['a']), Part::Opaque]);
    assert(seq![Part::Lit(seq!['a']), Part::Opaque][0] == Part::Lit(seq!['a']));
    assert(
        seq![Part::Lit(seq!['a']), Part::Opaque].drop_first() == seq![Part::Opaque]
    );
    assert(fold_head(Part::Lit(seq!['a'])) == Part::Lit(seq!['a']));
    fold_exprs_single(Part::Opaque);
    assert(fold_head(Part::Opaque) == Part::Opaque);
    assert(
        fold_exprs(seq![Part::Lit(seq!['a']), Part::Opaque]) == seq![
            Part::Lit(seq!['a']),
            Part::Opaque
        ]
    );
    merge_lits_cons(seq![Part::Lit(seq!['a']), Part::Opaque]);
    assert(seq![Part::Lit(seq!['a']), Part::Opaque][0] == Part::Lit(seq!['a']));
    assert(seq![Part::Lit(seq!['a']), Part::Opaque][1] == Part::Opaque);
    merge_lits_singleton(Part::Opaque);
}

// Denotation check on live values: the fold is invisible.
proof fn pin_merge_invisible_at_runtime()
    ensures denote(
        merge(
            seq![
                Part::Lit(seq!['E', 'c', 'h', 'o', ' ']),
                Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)
            ]
        )
    ) == seq!['E', 'c', 'h', 'o', ' ', 'o', 'k']
{
    merge_sound(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ']),
            Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)
        ]
    );
    denote_cons(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ']),
            Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)
        ]
    );
    assert(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ']),
            Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)
        ].drop_first() == seq![Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)]
    );
    assert(
        seq![
            Part::Lit(seq!['E', 'c', 'h', 'o', ' ']),
            Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)
        ][0] == Part::Lit(seq!['E', 'c', 'h', 'o', ' '])
    );
    assert(part_den(Part::Lit(seq!['E', 'c', 'h', 'o', ' '])) == seq!['E', 'c', 'h', 'o', ' ']);
    denote_cons(seq![Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)]);
    assert(
        seq![Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)].drop_first() == Seq::<Part>::empty()
    );
    assert(denote(Seq::<Part>::empty()) == Seq::<char>::empty());
    assert(
        seq![Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)][0] == Part::ExprStr(
            seq!['o', 'k'],
            Style::DoubleQuoted
        )
    );
    assert(
        part_den(Part::ExprStr(seq!['o', 'k'], Style::DoubleQuoted)) == seq!['o', 'k']
    );
    assert(
        seq!['E', 'c', 'h', 'o', ' '].add(seq!['o', 'k']) == seq!['E', 'c', 'h', 'o', ' ', 'o', 'k']
    );
}

fn main() {}

}
