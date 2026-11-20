// Standalone model of `capture_node_fold`
// (sh2perl/src/transforms/capture_node_fold.rs).
//
// `Call { "capture", [Arrow(body)] }` becomes the first-class
// `Capture { expr: Arrow(body), native: false }` node. Both spellings render
// the same `$(…)` substitution, but through different backend arms: the
// `Capture` arm is the one with the native paths, so the fold is a carrier
// rename in VALUE position.
//
// ## What is proved here (bash-side, not a restatement)
//
//   fold_preserves_output: for EVERY expression, the folded program produces
//   the same command-substitution bytes — the carrier law, and the thing that
//   lets the Perl renderer take its native arm without a differential.
//
//   words_position_fold_would_glue: THE LOAD-BEARING REFUSAL. In WORD
//   position bash splits a `$(…)` result on IFS, so `basename $(pwd)` passes
//   one argument per word, while the non-splitting `Capture` node passes the
//   whole output as ONE argument. `captureWords` is therefore refused; the
//   model shows the argument counts differ (2 vs 1 for `a b`), i.e. folding
//   the word spelling would glue words together.
//
//   status_read_blocks_the_fold: a body that reads `$?` keeps the call
//   spelling. The model captures the reason the guard can be decided
//   syntactically: `reads_status` is structural, and a body containing a
//   status read is never folded (`fold` leaves that node alone), so the
//   status-save arms still see the spelling they key on.
//
//   fold_leaves_no_foldable_call: COMPLETENESS. Every value position is
//   reached (nested, interpolated, both children of a two-position node), so
//   after the pass no foldable `capture` call remains — the class of bug that
//   a missed recursion arm would introduce.
//
// ## Hypotheses that are NOT discharged (stated, as required)
//
// - The backend arms: the model assumes the `Capture` arm and the
//   `Call{"capture"}` arm render the same `$(…)` bytes in every target. The
//   source's module docs assert this per backend (Perl's strict superset,
//   ESTree's convert-back, sh/c/go/rust's two arms); the model does not
//   re-derive it, so `fold_preserves_output` is conditional on that carrier
//   contract.
// - The `$?` guard exists for the renderers' status save/restore, not for
//   bash semantics: bash's substitution status is the same for both
//   spellings. The model therefore proves the GUARD is structural and total,
//   not that the fold would change bash's `$?`.
// - `split_count` models IFS word splitting with a single space separator at
//   the default IFS; tab/newline and custom IFS are not modelled (they only
//   make the refusal MORE necessary).
// - `Cmd` is an Arrow body; statement-level recursion (If/For/Try bodies) is
//   the same walk as `Cat`, pinned but not re-derived per statement variant.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 capture_node_fold.rs
//   -> verification results:: 20 verified, 0 errors
//
//   Mutants (checked, measured on copies under the same flags):
//     - folding the word-position spelling too: 18 verified, 2 errors
//       (`words_position_fold_would_glue` + the `captureWords` pin).
//     - dropping the `$?` guard: 18 verified, 2 errors.
//     - dropping the Arrow guard: 18 verified, 2 errors.
//     - missing the second child of a two-position node: 17 verified,
//       3 errors (completeness).
//     - treating the non-splitting node as splitting is rejected at
//       simplification (`2 == 1` in a `compute_only` assert), so it has no
//       verified/error count.
//   Restored and re-checked at 20 verified, 0 errors.
use vstd::prelude::*;

verus! {

// Expression model. `Cmd` is an Arrow body (a command substitution's
// content); `Word` is a plain operand that is NOT an Arrow (the source's
// non-arrow-payload refusal).
pub enum X {
    Cmd(Seq<int>),
    Word(Seq<int>),
    // IrExpr::Capture { expr }
    Cap(Box<X>),
    // Call { "capture", [expr] }
    Call(Box<X>),
    // Call { "captureWords", [expr] } — the word-position spelling
    CallWords(Box<X>),
    // getVar("?")
    Status,
    // a two-position node (Interpolate part, BinOp, Cat, …)
    Cat(Box<X>, Box<X>),
}

// Not an Arrow, so the source never folds it.
// An Arrow body: a command, or a `Cat` of statements/positions inside it.
// `Word` is the source's non-arrow payload.
pub open spec fn payload_is_arrow(x: X) -> bool {
    match x {
        X::Cmd(_) => true,
        X::Cat(_, _) => true,
        X::Word(_) => false,
        _ => false,
    }
}

// The source's `reads_exit_status`, structurally.
pub open spec fn reads_status(x: X) -> bool
    decreases x,
{
    match x {
        X::Status => true,
        X::Cmd(_) => false,
        X::Word(_) => false,
        X::Cap(i) => reads_status(*i),
        X::Call(i) => reads_status(*i),
        X::CallWords(i) => reads_status(*i),
        X::Cat(a, b) => reads_status(*a) || reads_status(*b),
    }
}

// The pass, in value position: recurse into every position, then fold a
// `capture` call whose payload is an Arrow that does not read `$?`.
pub open spec fn fold(x: X) -> X
    decreases x,
{
    match x {
        X::Cmd(t) => X::Cmd(t),
        X::Word(t) => X::Word(t),
        X::Status => X::Status,
        // already the first-class node: recurse only (idempotent)
        X::Cap(i) => X::Cap(Box::new(fold(*i))),
        X::Call(i) => {
            if payload_is_arrow(*i) && !reads_status(*i) {
                X::Cap(Box::new(fold(*i)))
            } else {
                X::Call(Box::new(fold(*i)))
            }
        }
        // never folded: the word-position spelling word-splits
        X::CallWords(i) => X::CallWords(Box::new(fold(*i))),
        X::Cat(a, b) => X::Cat(Box::new(fold(*a)), Box::new(fold(*b))),
    }
}

// The substitution's bytes as the shell sees them.
pub open spec fn bytes(x: X) -> Seq<int>
    decreases x,
{
    match x {
        X::Cmd(t) => t,
        X::Word(t) => t,
        X::Status => seq![63],
        X::Cap(i) => bytes(*i),
        X::Call(i) => bytes(*i),
        X::CallWords(i) => bytes(*i),
        X::Cat(a, b) => bytes(*a) + bytes(*b),
    }
}

// ---- the carrier law ----

// Value position: the fold never changes the substitution's bytes.
proof fn fold_preserves_output(x: X)
    ensures bytes(fold(x)) == bytes(x),
    decreases x,
{
    match x {
        X::Cmd(_) => {}
        X::Word(_) => {}
        X::Status => {}
        X::Cap(i) => {
            fold_preserves_output(*i);
        }
        X::Call(i) => {
            fold_preserves_output(*i);
            assert(bytes(fold(*i)) == bytes(*i));
        }
        X::CallWords(i) => {
            fold_preserves_output(*i);
        }
        X::Cat(a, b) => {
            fold_preserves_output(*a);
            fold_preserves_output(*b);
        }
    }
}

// ---- completeness: every value position is reached ----

// No foldable `capture` call survives the pass.
pub open spec fn no_foldable_call(x: X) -> bool
    decreases x,
{
    match x {
        X::Cmd(_) => true,
        X::Word(_) => true,
        X::Status => true,
        X::Cap(i) => no_foldable_call(*i),
        // a surviving call must be one the guards refused
        X::Call(i) => (!payload_is_arrow(*i) || reads_status(*i)) && no_foldable_call(*i),
        X::CallWords(i) => no_foldable_call(*i),
        X::Cat(a, b) => no_foldable_call(*a) && no_foldable_call(*b),
    }
}

// The fold preserves both guards' verdicts on a node: an Arrow stays an
// Arrow (`Cmd`/`Cat` are the Arrow shapes) and a status read is neither
// created nor erased. Needed because a surviving call's payload is the
// FOLDED child.
proof fn fold_preserves_guards(x: X)
    ensures payload_is_arrow(fold(x)) == payload_is_arrow(x),
        reads_status(fold(x)) == reads_status(x),
    decreases x,
{
    reveal_with_fuel(fold, 2);
    reveal_with_fuel(reads_status, 2);
    match x {
        X::Cmd(_) => {}
        X::Word(_) => {}
        X::Status => {}
        X::Cap(i) => {
            fold_preserves_guards(*i);
        }
        X::Call(i) => {
            fold_preserves_guards(*i);
        }
        X::CallWords(i) => {
            fold_preserves_guards(*i);
        }
        X::Cat(a, b) => {
            fold_preserves_guards(*a);
            fold_preserves_guards(*b);
        }
    }
}

proof fn fold_leaves_no_foldable_call(x: X)
    ensures no_foldable_call(fold(x)),
    decreases x,
{
    match x {
        X::Cmd(_) => {}
        X::Word(_) => {}
        X::Status => {}
        X::Cap(i) => {
            fold_leaves_no_foldable_call(*i);
        }
        X::Call(i) => {
            fold_leaves_no_foldable_call(*i);
            if payload_is_arrow(*i) && !reads_status(*i) {
                assert(fold(X::Call(Box::new(*i))) == X::Cap(Box::new(fold(*i))));
                // Cap recurses further; the child was already cleaned
                assert(no_foldable_call(fold(*i)));
            } else {
                assert(fold(X::Call(Box::new(*i))) == X::Call(Box::new(fold(*i))));
                assert(!payload_is_arrow(*i) || reads_status(*i));
                fold_preserves_guards(*i);
                assert(payload_is_arrow(fold(*i)) == payload_is_arrow(*i));
                assert(reads_status(fold(*i)) == reads_status(*i));
                assert(!payload_is_arrow(fold(*i)) || reads_status(fold(*i)));
                assert(no_foldable_call(fold(*i)));
            }
        }
        X::CallWords(i) => {
            fold_leaves_no_foldable_call(*i);
        }
        X::Cat(a, b) => {
            fold_leaves_no_foldable_call(*a);
            fold_leaves_no_foldable_call(*b);
        }
    }
}

// ---- the load-bearing refusal: word position ----

// Words in a string: runs of non-space characters. `$(…)` in word position
// is split into exactly these.
pub open spec fn word_starts(t: Seq<int>, i: int) -> int
    decreases t.len() - i,
{
    if i >= t.len() {
        0
    } else {
        let prev_sep: bool = i == 0 || t[i - 1] == 32;
        let here: int = if t[i] != 32 && prev_sep { 1 } else { 0 };
        here + word_starts(t, i + 1)
    }
}

pub open spec fn words(t: Seq<int>) -> int {
    word_starts(t, 0)
}

// Argument count in WORD position.
pub open spec fn arg_count(x: X) -> int
    decreases x,
{
    match x {
        // a plain word is ONE argument, spaces and all (it is quoted)
        X::Cmd(_) => 1,
        X::Word(_) => 1,
        X::Status => 1,
        // `$(…)` in word position: bash splits the output on IFS
        X::Call(i) => words(bytes(*i)),
        X::CallWords(i) => words(bytes(*i)),
        // the non-splitting node: at most one argument
        X::Cap(i) => if bytes(*i).len() == 0 { 0 } else { 1 },
        X::Cat(a, b) => arg_count(*a) + arg_count(*b),
    }
}

// THE REFUSAL: folding the word-position spelling changes the argument count
// — `basename $(pwd)` on a directory whose path contains a space passes one
// argument per word, while the folded node passes the whole output as one.
proof fn words_position_fold_would_glue()
    ensures ({
        let t = seq![97, 32, 98]; // "a b"
        // bash word-splits the call spelling into two arguments
        arg_count(X::Call(Box::new(X::Cmd(t)))) == 2
            // and the first-class node would pass one
            && arg_count(X::Cap(Box::new(X::Cmd(t)))) == 1
            && arg_count(X::Call(Box::new(X::Cmd(t))))
                != arg_count(X::Cap(Box::new(X::Cmd(t))))
            // so the pass must not fold the word spelling — and does not
            && (match fold(X::CallWords(Box::new(X::Cmd(seq![97, 32, 98])))) {
                X::CallWords(_) => true,
                _ => false,
            })
    }),
{
    assert(words(seq![97, 32, 98]) == 2) by (compute_only);
    assert(arg_count(X::Call(Box::new(X::Cmd(seq![97, 32, 98])))) == 2) by (compute_only);
    assert(arg_count(X::Cap(Box::new(X::Cmd(seq![97, 32, 98])))) == 1) by (compute_only);
}

// A single-word output is the case where the two agree — which is exactly why
// the refusal has to be structural (the pass cannot know the runtime bytes).
proof fn single_word_output_agrees()
    ensures arg_count(X::Call(Box::new(X::Cmd(seq![97, 98])))) == 1
        && arg_count(X::Cap(Box::new(X::Cmd(seq![97, 98])))) == 1,
{
    assert(words(seq![97, 98]) == 1) by (compute_only);
}

// ---- the `$?` guard ----

// A body that reads `$?` is never folded, so the backends' status-save arms
// keep the spelling they key on. `reads_status` is structural (no runtime
// information needed), which is what makes the guard decidable.
proof fn status_read_blocks_the_fold()
    ensures ({
        let s = X::Status;
        reads_status(s)
            && (match fold(X::Call(Box::new(X::Cat(
                Box::new(X::Cmd(seq![97])),
                Box::new(X::Status),
            )))) {
                X::Call(_) => true,
                _ => false,
            })
    }),
{
    assert(reads_status(X::Status));
}

// A clean body is folded, and so is every clean call in a two-position node
// (the walk is complete, not just top-level).
proof fn clean_bodies_are_folded()
    ensures ({
        match fold(X::Cat(
            Box::new(X::Call(Box::new(X::Cmd(seq![97])))),
            Box::new(X::Call(Box::new(X::Cmd(seq![98])))),
        )) {
            X::Cat(a, b) => (match *a {
                X::Cap(_) => true,
                _ => false,
            }) && (match *b {
                X::Cap(_) => true,
                _ => false,
            }),
            _ => false,
        }
    }),
{
    reveal_with_fuel(fold, 3);
    reveal_with_fuel(reads_status, 3);
    assert(payload_is_arrow(X::Cmd(seq![97])));
    assert(payload_is_arrow(X::Cmd(seq![98])));
    assert(reads_status(X::Cmd(seq![97])) == false);
    assert(reads_status(X::Cmd(seq![98])) == false);
}

// ---- pins: the source's tests ----

// folds_the_call_form_to_the_node.
proof fn pin_folds_the_call_form()
    ensures fold(X::Call(Box::new(X::Cmd(seq![104]))))
        == X::Cap(Box::new(X::Cmd(seq![104]))),
{
    reveal_with_fuel(fold, 3);
    reveal_with_fuel(reads_status, 2);
}

// is_idempotent_on_the_node_form.
proof fn pin_idempotent()
    ensures fold(X::Cap(Box::new(X::Cmd(seq![104]))))
        == X::Cap(Box::new(X::Cmd(seq![104]))),
{
    reveal_with_fuel(fold, 3);
}

// refuses_capture_words_the_word_split_spelling.
proof fn pin_refuses_capture_words()
    ensures (match fold(X::CallWords(Box::new(X::Cmd(seq![104])))) {
        X::CallWords(_) => true,
        _ => false,
    }),
{
}

// refuses_a_body_that_reads_status: the status-carrying body keeps the call.
proof fn pin_refuses_status_body()
    ensures (match fold(X::Call(Box::new(X::Status))) {
        X::Call(_) => true,
        _ => false,
    }),
{
}

// refuses_a_non_arrow_payload: a plain word operand is not folded.
proof fn pin_refuses_non_arrow()
    ensures (match fold(X::Call(Box::new(X::Word(seq![104])))) {
        X::Call(_) => true,
        _ => false,
    }),
{
}

// folds_nested_inside_an_interpolated_word.
proof fn pin_folds_inside_interpolation()
    ensures ({
        match fold(X::Cat(
            Box::new(X::Word(seq![118, 61])),  // "v="
            Box::new(X::Call(Box::new(X::Cmd(seq![104])))),
        )) {
            X::Cat(_, b) => match *b {
                X::Cap(_) => true,
                _ => false,
            },
            _ => false,
        }
    }),
{
    reveal_with_fuel(fold, 3);
    reveal_with_fuel(reads_status, 2);
}

fn main() {}
}
