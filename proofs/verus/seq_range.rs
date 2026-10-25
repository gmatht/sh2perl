// Standalone model of the seq-range Tier-1 obligation
// (`sh2perl/src/transforms/seq_range_for.rs`): rewriting
// `for v in $(seq …)` (the captureWords/exec/seq-arg shape) to the bare
// `Range{start, end}` iterable preserves the iterated integer sequence —
// under the pass's refuse-on-doubt gates.
//
// Proves the soundness obligation:
//
//   seq_range_sound: the Range bounds denote exactly the word list the
//   `seq` capture would have produced (ascending 1..=n / a..=b, empty
//   when descending), from any store, when the gates hold.
//
// Integers are Verus `int` (the i64 range is enforced by the literal
// gate: |v| <= 2^53, double-precision exactness — the model takes parsed
// literal ints as input with a `big_ok` flag for the bound).
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - the exact arg shapes: 1-arg `[last]` -> 1..=last (GNU seq starts
//     at 1); 2-arg `[first, last]` -> first..=last; 3-arg/step forms,
//     flags, >3 args refuse (a step needs a stride the Range node
//     lacks). Descending is NOT a refusal: `bounds()` returns the pair
//     and the empty Range denotes the empty iteration (`seq 5 1`
//     prints nothing with default step +1).
//   - the literal gate as parameters: each arg is Some(int) (plain
//     integer literal, no floats/leading-zeros/octal forms/flags —
//     string parsing, out of scope) or None (refuse); `big_ok` is the
//     conjunction of the 2^53 exactness checks; `span_ok` is the
//     10^6 materialization bound.
//   - the body-write scan (`stmts_write_var`: assigns, declares,
//     setVar-style calls, nested loop-var rebinding) as a `body_writes`
//     parameter — a writing body derails the counter loop (the word
//     list would re-iterate regardless), so it refuses.
//   - the capture/exec/seq-name shape match (captureWords + Arrow +
//     exec + "seq" name, bare-exec env/redirects disqualify) is
//     structural recognition — the model takes already-recognised arg
//     lists as input (same per-node shape as array_len_fold: the
//     theorem is per For-site, traversal out of scope).
//   - backend consumption of bare Range (native counter loops, bounded
//     materialization fallback) is out of scope: the obligation is the
//     word-list identity at the shIR level.
use vstd::prelude::*;

verus! {

// Parsed seq arguments: Some(int) = plain integer literal, None =
// float / leading-zero / flag / non-literal (refuse).
pub enum Arg {
    Num(int),
    Other,
}

// The iterable before/after: the seq arg list, or the Range bounds.
pub enum Iter {
    SeqArgs(Seq<Arg>),
    Range(int, int),
    Other,
}

// The gate conjunction, mirroring the source check order: recognised
// 1-/2-arg integer shape, double-exactness, span bound, clean body.
pub open spec fn gates_ok(args: Seq<Arg>, big_ok: bool, span_ok: bool, body_writes: bool) -> bool {
    (if args.len() == 1 {
        match args[0] {
            Arg::Num(_) => true,
            Arg::Other => false,
        }
    } else if args.len() == 2 {
        match (args[0], args[1]) {
            (Arg::Num(_), Arg::Num(_)) => true,
            _ => false,
        }
    } else {
        false
    }) && big_ok && span_ok && !body_writes
}

// Denotation: the iterated integer list. 1-arg starts at 1; descending
// (or empty-at-zero) ranges are empty — `seq` with default step +1
// prints nothing then, and the empty Range iterates nothing.
pub open spec fn words(it: Iter) -> Seq<int>
    decreases it,
{
    match it {
        Iter::Range(lo, hi) => range_list(lo, hi),
        Iter::SeqArgs(args) => {
            if args.len() == 1 {
                match args[0] {
                    Arg::Num(last) => range_list(1, last),
                    Arg::Other => Seq::empty(),
                }
            } else if args.len() == 2 {
                match (args[0], args[1]) {
                    (Arg::Num(first), Arg::Num(last)) => range_list(first, last),
                    _ => Seq::empty(),
                }
            } else {
                Seq::empty()
            }
        },
        Iter::Other => Seq::empty(),
    }
}

pub open spec fn range_list(lo: int, hi: int) -> Seq<int>
    decreases hi - lo,
{
    if lo > hi {
        Seq::empty()
    } else if lo == hi {
        seq![lo]
    } else {
        seq![lo].add(range_list(lo + 1, hi))
    }
}

// The fold: recognised, gate-satisfying seq args -> Range bounds.
pub open spec fn lower(
    args: Seq<Arg>,
    big_ok: bool,
    span_ok: bool,
    body_writes: bool,
) -> Option<(int, int)> {
    if args.len() == 1 {
        match args[0] {
            Arg::Num(last) => {
                if gates_ok(args, big_ok, span_ok, body_writes) {
                    Some((1, last))
                } else {
                    None
                }
            },
            Arg::Other => None,
        }
    } else if args.len() == 2 {
        match (args[0], args[1]) {
            (Arg::Num(first), Arg::Num(last)) => {
                if gates_ok(args, big_ok, span_ok, body_writes) {
                    Some((first, last))
                } else {
                    None
                }
            },
            _ => None,
        }
    } else {
        None
    }
}

// THE OBLIGATION: the Range denotes the seq word list, when lowering
// succeeds.
proof fn seq_range_sound(args: Seq<Arg>, big_ok: bool, span_ok: bool, body_writes: bool)
    requires lower(args, big_ok, span_ok, body_writes).is_Some(),
    ensures {
        let (lo, hi) = lower(args, big_ok, span_ok, body_writes).get_Some_0();
        words(Iter::Range(lo, hi)) == words(Iter::SeqArgs(args))
    },
{
    if args.len() == 1nat {
        match args[0] {
            Arg::Num(last) => {
                assert(lower(args, big_ok, span_ok, body_writes) == Some((1, last))) by {
                };
                assert(words(Iter::SeqArgs(args)) == range_list(1, last)) by {
                    reveal_with_fuel(words, 1);
                };
            },
            Arg::Other => {
                assert(lower(args, big_ok, span_ok, body_writes).is_None()) by {
                };
                assert(false);
            },
        }
    } else {
        assert(args.len() == 2nat) by {
        };
        match (args[0], args[1]) {
            (Arg::Num(first), Arg::Num(last)) => {
                assert(lower(args, big_ok, span_ok, body_writes) == Some((first, last))) by {
                };
                assert(words(Iter::SeqArgs(args)) == range_list(first, last)) by {
                    reveal_with_fuel(words, 1);
                };
            },
            _ => {
                assert(lower(args, big_ok, span_ok, body_writes).is_None()) by {
                };
                assert(false);
            },
        }
    }
}

// ---- Fidelity pins (mirror the source's own tests) ----

// `seq LAST` lifts to 1..=LAST.
proof fn pin_two_arg_lifts()
    ensures lower(seq![Arg::Num(3), Arg::Num(5)], true, true, false) == Some((3, 5)),
{
}

// `seq 5` starts at one.
proof fn pin_one_arg_starts_at_one()
    ensures lower(seq![Arg::Num(5)], true, true, false) == Some((1, 5)) && words(
        Iter::Range(1, 5),
    ) == seq![1, 2, 3, 4, 5],
{
    reveal_with_fuel(words, 1);
    reveal_with_fuel(range_list, 8);
}

// Descending is an empty range (not a refusal).
proof fn pin_descending_is_empty()
    ensures lower(seq![Arg::Num(5), Arg::Num(1)], true, true, false) == Some((5, 1)) && words(
        Iter::Range(5, 1),
    ) == Seq::<int>::empty(),
{
    reveal_with_fuel(words, 1);
    reveal_with_fuel(range_list, 2);
}

// Step / 3-arg forms keep the runtime path.
proof fn pin_step_keeps_runtime()
    ensures lower(seq![Arg::Num(1), Arg::Num(2), Arg::Num(9)], true, true, false) == None,
{
}

// Float / leading-zero / non-literal args keep the runtime path.
proof fn pin_nonliteral_keeps_runtime()
    ensures lower(seq![Arg::Other, Arg::Num(9)], true, true, false) == None && lower(
        seq![Arg::Other],
        true,
        true,
        false,
    ) == None,
{
}

// A body that writes the loop var disqualifies.
proof fn pin_body_write_disqualifies()
    ensures lower(seq![Arg::Num(1), Arg::Num(9)], true, true, true) == None,
{
}

// A setVar-style store write disqualifies (same flag, same outcome).
proof fn pin_setvar_write_disqualifies()
    ensures lower(seq![Arg::Num(1), Arg::Num(9)], true, true, true) == None,
{
}

// A huge span disqualifies (materialization bound).
proof fn pin_huge_span_disqualifies()
    ensures lower(seq![Arg::Num(1), Arg::Num(9)], true, false, false) == None,
{
}

fn main() {}

}
