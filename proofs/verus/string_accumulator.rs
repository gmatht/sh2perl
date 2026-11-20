// Standalone model of string-accumulator's structural lowering
// (sh2perl/src/transforms/string_accumulator.rs): a loop whose only writes
// to `v` are self-appends `v="$v$seg"` is rewritten from per-iteration
// re-concatenation to a chunk array plus ONE `concatItems` after the loop.
// Proves the soundness obligation:
//
//   string_accumulator_preserves_output: for EVERY statement trace, the
//   chunked lowering and the eager re-concatenation emit the same bytes,
//   with the verdict computed FROM the trace (a refused program takes both
//   sides with the flag `false`, so nothing is assumed about the shape).
//
// ## What is modelled
//
// A string is a byte sequence (`Str = Seq<u8>`); a trace is the ordered
// events that touch `v`:
//   Init(s)        `v = <literal>` before the loop (the seeding write)
//   InitOther(s)   `v = <non-literal>`: no literal text to seed from
//   Append(s)      `v="$v$seg"` in the ONE shape the lowering rewrites
//                  (single scalar target, loop-body top level)
//   AppendOther(s) the same append the lowering CANNOT rewrite (multi-target,
//                  indexed target, or nested in a branch)
//   LateWrite(s)   another write of `v` (a later seed)
//   Read           a read of `v` (loop-body, loop-condition, or post-loop)
//   Join           the lowering's single materialization, right after the loop
//
// Two strategies:
//   eager    every append re-concatenates `v` (bash / the unlowered IR)
//   chunked  every append is DEFERRED: it extends the chunk array's joined
//            value (`joined`) and bumps the push count, leaving `v` STALE;
//            `Join` sets `v = joined`, once, after the loop
//
// `joined` is the chunk array's semantic content — the concatenation the
// `concatItems` call produces. Modelling it incrementally (rather than as a
// list re-joined at the end) is exactly what the lowering exploits: the
// chunks are concatenated ONCE, in order, at the join. `pushes` records that
// appends became pushes, so the lowered program is not the identity. The
// chunked `v` is stale between appends — that is the whole semantic
// difference, and the reason the verdict must refuse a read taken while an
// append is outstanding.
//
// ## The verdict (computed from the trace, never assumed)
//
//   acc_ok(evs) = has_append
//             && !has_init_other      (no literal text to seed the chunks)
//             && !has_append_other    (every append must be rewritable, or
//                                      the surviving one is overwritten by
//                                      the join)
//             && ok_reads(evs, true)  (no read while an append is
//                                      outstanding: a body/condition read
//                                      observes the growing string)
//
// ## What is NOT modelled
//
//   - The TRIGGER conditions (`self_append_vars`'s write-surface census, the
//     last-write-before-the-loop init search, `reads_after_loop`): the model
//     starts from the raw trace of writes/reads the lowering sees, so those
//     are the hypotheses that the trace is complete. They are the *trigger*,
//     not the law: the law holds for every trace.
//   - The eval-order of the segment expression (`seg` is taken as evaluated
//     at the append's own position; the lowering keeps it in place, and any
//     read of `v` in the segment is a mid-loop read, refused by `ok_reads`).
//   - `$?`/`IFS` side effects of the runtime calls (estree's setArray family):
//     renderer contract, not the rewrite.
//   - The C hook (`acc_vars_tree` → `_alen_` length mirror): a renderer
//     invariant (every store funnel updates the mirror), not this rewrite.
//
// ## Mutants (checked red, each a real edit of this file)
//
//   identity (acc_ok false)            -> an_accumulator_is_flagged
//   allow a stale read                 -> the law + a_stale_read_would_differ
//   allow an unrewritable append       -> the law + an_unrewritable_append_would_differ
//   allow a non-literal init           -> the law + a_nonliteral_init_would_differ
//   drop has_append (init-only)        -> an_init_only_trace_is_not_flagged
//   flag everything                    -> the law + the completeness pins
//   drop the init seed (joined = "")   -> the law + the_seed_is_load_bearing
//   join by the last segment, not the
//   deferred concatenation             -> the law + the_join_is_load_bearing
//
// ## Defects the model forced (both repaired in the transform, tests pin them)
//
//   1. `body_reads_var_nonleading` exempted a self-append whenever ANY target
//      named `v`, but `lower` rewrites only a single scalar target. A
//      multi-target assignment was therefore accepted and NOT rewritten: its
//      append kept re-concatenating `v` and the post-loop join overwrote the
//      result (the segment was lost). The exemption now uses the lowering's
//      exact predicate. Test: a_multi_target_self_append_refuses_the_verdict.
//   2. The same hole for an INDEXED target `v[i]="$v$x"`: it writes the array
//      `v`, not the scalar the chunk array stands in for, and the join
//      clobbered it. Test: an_indexed_self_append_refuses_the_verdict.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 string_accumulator.rs
//   -> verification results:: 22 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Str = Seq<u8>;

pub enum Ev {
    /// `v = <literal>` before the loop.
    Init(Str),
    /// `v = <non-literal>`: no literal text to seed the chunk array.
    InitOther(Str),
    /// `v = "$v$seg"` in the shape the structural lowering rewrites.
    Append(Str),
    /// the same append in a shape the lowering skips.
    AppendOther(Str),
    /// another write of `v` (a later seed).
    LateWrite(Str),
    /// a read of `v` (loop body, loop condition, or after the loop).
    Read,
    /// the lowering's single materialization, immediately after the loop.
    Join,
}

pub open spec fn empty_out() -> Seq<Str> {
    Seq::<Str>::empty()
}

// The trace is walked by INDEX (`evs[i]`, never `subrange`), so every step is
// a definitional unfolding at a known position.

/// Is there anything to lower at or after `i` at all?
pub open spec fn has_append_at(evs: Seq<Ev>, i: int) -> bool
    decreases evs.len() - i,
{
    if i >= evs.len() {
        false
    } else {
        match evs[i] {
            Ev::Append(_) => true,
            _ => has_append_at(evs, i + 1),
        }
    }
}

pub open spec fn has_init_other_at(evs: Seq<Ev>, i: int) -> bool
    decreases evs.len() - i,
{
    if i >= evs.len() {
        false
    } else {
        match evs[i] {
            Ev::InitOther(_) => true,
            _ => has_init_other_at(evs, i + 1),
        }
    }
}

pub open spec fn has_append_other_at(evs: Seq<Ev>, i: int) -> bool
    decreases evs.len() - i,
{
    if i >= evs.len() {
        false
    } else {
        match evs[i] {
            Ev::AppendOther(_) => true,
            _ => has_append_other_at(evs, i + 1),
        }
    }
}

/// May every read at or after `i` be taken? `synced` says the chunked `v` still
/// equals the eager one on entry. An append makes them differ until the next
/// write or the join, so a read while one is outstanding observes a stale
/// string.
pub open spec fn ok_reads_at(evs: Seq<Ev>, i: int, synced: bool) -> bool
    decreases evs.len() - i,
{
    if i >= evs.len() {
        true
    } else {
        match evs[i] {
            Ev::Read => synced && ok_reads_at(evs, i + 1, synced),
            Ev::Init(_) | Ev::InitOther(_) | Ev::LateWrite(_) | Ev::Join => ok_reads_at(
                evs,
                i + 1,
                true,
            ),
            Ev::Append(_) => ok_reads_at(evs, i + 1, false),
            Ev::AppendOther(_) => ok_reads_at(evs, i + 1, false),
        }
    }
}

/// The verdict, computed from the trace.
pub open spec fn acc_ok(evs: Seq<Ev>) -> bool {
    &&& has_append_at(evs, 0)
    &&& !has_init_other_at(evs, 0)
    &&& !has_append_other_at(evs, 0)
    &&& ok_reads_at(evs, 0, true)
}

/// The eager semantics: every append re-concatenates. Returns the final `v`
/// and the values every read observed, in order.
pub open spec fn eager_at(evs: Seq<Ev>, i: int, v: Str) -> (Str, Seq<Str>)
    decreases evs.len() - i,
{
    if i >= evs.len() {
        (v, empty_out())
    } else {
        match evs[i] {
            Ev::Init(s) | Ev::InitOther(s) | Ev::LateWrite(s) => eager_at(evs, i + 1, s),
            Ev::Append(s) | Ev::AppendOther(s) => eager_at(evs, i + 1, v + s),
            Ev::Read => {
                let r = eager_at(evs, i + 1, v);
                (r.0, r.1.push(v))
            },
            Ev::Join => eager_at(evs, i + 1, v),
        }
    }
}

/// The chunked semantics: appends are deferred into `joined`, `pushes` counts
/// them, `Join` materializes once. `v` is STALE between appends (the lowering
/// defers materialization), so a read there observes the pre-append value.
pub open spec fn chunked_at(
    evs: Seq<Ev>,
    i: int,
    v: Str,
    joined: Str,
    pushes: nat,
) -> (Str, Str, nat, Seq<Str>)
    decreases evs.len() - i,
{
    if i >= evs.len() {
        (v, joined, pushes, empty_out())
    } else {
        match evs[i] {
            Ev::Init(s) => chunked_at(evs, i + 1, s, s, 0),
            // no literal text: the lowering cannot seed (and `lower` skips it)
            Ev::InitOther(s) => chunked_at(evs, i + 1, s, Seq::<u8>::empty(), 0),
            Ev::Append(s) => chunked_at(evs, i + 1, v, joined + s, pushes + 1),
            // not rewritten: stays eager, and the join then drops it
            Ev::AppendOther(s) => chunked_at(evs, i + 1, v + s, joined, pushes),
            Ev::LateWrite(s) => chunked_at(evs, i + 1, s, s, 0),
            Ev::Read => {
                let r = chunked_at(evs, i + 1, v, joined, pushes);
                (r.0, r.1, r.2, r.3.push(v))
            },
            Ev::Join => chunked_at(evs, i + 1, joined, joined, pushes),
        }
    }
}

/// One equation for every trace: the flag selects the strategy, and it is
/// computed FROM the trace. The observable is what the program PRINTS — the
/// values every read observed, in order (the final `v` is only ever observed
/// through such a read, and `reads_after_loop` guarantees the final one).
pub open spec fn go(evs: Seq<Ev>, chunk: bool) -> Seq<Str> {
    if chunk {
        chunked_at(evs, 0, Seq::<u8>::empty(), Seq::<u8>::empty(), 0).3
    } else {
        eager_at(evs, 0, Seq::<u8>::empty()).1
    }
}

/// The number of segment pushes the chunked run performed.
pub open spec fn pushes_of(evs: Seq<Ev>) -> nat {
    chunked_at(evs, 0, Seq::<u8>::empty(), Seq::<u8>::empty(), 0).2
}

// ---------------------------------------------------------------------------
// The law
// ---------------------------------------------------------------------------

/// The agreement lemma, carrying both states. `joined == v_e` is the deferred-
/// concatenation invariant: a push extends `joined` by exactly the segment the
/// eager side appends. `synced` says the chunked `v` still equals the eager
/// one, and `ok_reads` guarantees no read is taken while it does not.
proof fn agreement(
    evs: Seq<Ev>,
    i: int,
    v_e: Str,
    v_c: Str,
    joined: Str,
    synced: bool,
    pushes: nat,
)
    requires
        i >= 0,
        joined == v_e,
        synced ==> v_c == v_e,
        ok_reads_at(evs, i, synced),
        !has_init_other_at(evs, i),
        !has_append_other_at(evs, i),
    ensures
        eager_at(evs, i, v_e).1 == chunked_at(evs, i, v_c, joined, pushes).3,
        chunked_at(evs, i, v_c, joined, pushes).1 == eager_at(evs, i, v_e).0,
    decreases evs.len() - i,
{
    reveal_with_fuel(ok_reads_at, 2);
    reveal_with_fuel(has_init_other_at, 2);
    reveal_with_fuel(has_append_other_at, 2);
    if i >= evs.len() {
    } else {
        reveal_with_fuel(eager_at, 2);
        reveal_with_fuel(chunked_at, 2);
        match evs[i] {
            Ev::Read => {
                // ok_reads forces `synced`: a read while an append is
                // outstanding is refused, which is the whole point.
                assert(synced);
                assert(eager_at(evs, i, v_e) == (
                    eager_at(evs, i + 1, v_e).0,
                    eager_at(evs, i + 1, v_e).1.push(v_e),
                ));
                assert(chunked_at(evs, i, v_c, joined, pushes) == (
                    chunked_at(evs, i + 1, v_c, joined, pushes).0,
                    chunked_at(evs, i + 1, v_c, joined, pushes).1,
                    chunked_at(evs, i + 1, v_c, joined, pushes).2,
                    chunked_at(evs, i + 1, v_c, joined, pushes).3.push(v_c),
                ));
                agreement(evs, i + 1, v_e, v_c, joined, synced, pushes);
            },
            Ev::Init(s) => {
                assert(eager_at(evs, i, v_e) == eager_at(evs, i + 1, s));
                assert(chunked_at(evs, i, v_c, joined, pushes) == chunked_at(
                    evs,
                    i + 1,
                    s,
                    s,
                    0,
                ));
                agreement(evs, i + 1, s, s, s, true, 0);
            },
            Ev::InitOther(_) => {
                // forbidden by the verdict; the trace cannot reach here
                assert(false);
            },
            Ev::Append(s) => {
                assert(eager_at(evs, i, v_e) == eager_at(evs, i + 1, v_e + s));
                assert(chunked_at(evs, i, v_c, joined, pushes) == chunked_at(
                    evs,
                    i + 1,
                    v_c,
                    joined + s,
                    pushes + 1,
                ));
                agreement(evs, i + 1, v_e + s, v_c, joined + s, false, pushes + 1);
            },
            Ev::AppendOther(_) => {
                assert(false);
            },
            Ev::LateWrite(s) => {
                assert(eager_at(evs, i, v_e) == eager_at(evs, i + 1, s));
                assert(chunked_at(evs, i, v_c, joined, pushes) == chunked_at(
                    evs,
                    i + 1,
                    s,
                    s,
                    0,
                ));
                agreement(evs, i + 1, s, s, s, true, 0);
            },
            Ev::Join => {
                // the materialization sets `v` to the deferred concatenation,
                // which the invariant pins to the eager value
                assert(eager_at(evs, i, v_e) == eager_at(evs, i + 1, v_e));
                assert(chunked_at(evs, i, v_c, joined, pushes) == chunked_at(
                    evs,
                    i + 1,
                    joined,
                    joined,
                    pushes,
                ));
                agreement(evs, i + 1, v_e, joined, joined, true, pushes);
            },
        }
    }
}

/// THE LAW: for every trace, if the verdict accepts it the chunked lowering
/// emits exactly the eager bytes; a refused trace takes both sides with the
/// flag `false` and is unchanged.
proof fn string_accumulator_preserves_output(evs: Seq<Ev>)
    ensures
        go(evs, acc_ok(evs)) == go(evs, false),
{
    if acc_ok(evs) {
        agreement(
            evs,
            0,
            Seq::<u8>::empty(),
            Seq::<u8>::empty(),
            Seq::<u8>::empty(),
            true,
            0,
        );
    } else {
        // the flag is `false`: both sides are the reference semantics
    }
}

// ---------------------------------------------------------------------------
// Completeness: the verdict is a decision computed from the trace, not a
// constant — an identity transformation must NOT satisfy these.
// ---------------------------------------------------------------------------

pub open spec fn w_canonical() -> Seq<Ev> {
    seq![Ev::Init(seq![1u8]), Ev::Append(seq![2u8]), Ev::Append(seq![3u8]), Ev::Join, Ev::Read]
}

pub open spec fn w_stale() -> Seq<Ev> {
    seq![Ev::Init(seq![1u8]), Ev::Append(seq![2u8]), Ev::Read, Ev::Join, Ev::Read]
}

pub open spec fn w_unrewritten() -> Seq<Ev> {
    seq![
        Ev::Init(seq![1u8]),
        Ev::Append(seq![2u8]),
        Ev::AppendOther(seq![3u8]),
        Ev::Join,
        Ev::Read,
    ]
}

pub open spec fn w_nonliteral() -> Seq<Ev> {
    seq![Ev::InitOther(seq![1u8]), Ev::Append(seq![2u8]), Ev::Join, Ev::Read]
}

/// A clean accumulator trace IS flagged (so `acc_ok = false` is red here).
proof fn an_accumulator_is_flagged()
    ensures
        acc_ok(w_canonical()),
{
    reveal_with_fuel(has_append_at, 8);
    reveal_with_fuel(has_init_other_at, 8);
    reveal_with_fuel(has_append_other_at, 8);
    reveal_with_fuel(ok_reads_at, 8);
}

/// A read while an append is outstanding observes the growing string.
proof fn a_stale_read_is_refused()
    ensures
        !acc_ok(w_stale()),
        !ok_reads_at(w_stale(), 0, true),
{
    reveal_with_fuel(ok_reads_at, 8);
}

/// An append the lowering cannot rewrite leaves a re-concatenation in the
/// body, and the single join then overwrites it.
proof fn an_unrewritable_append_is_refused()
    ensures
        !acc_ok(w_unrewritten()),
        has_append_other_at(w_unrewritten(), 0),
{
    reveal_with_fuel(has_append_other_at, 8);
}

/// A non-literal init has no text to seed the chunk array with.
proof fn a_nonliteral_init_is_refused()
    ensures
        !acc_ok(w_nonliteral()),
        has_init_other_at(w_nonliteral(), 0),
{
    reveal_with_fuel(has_init_other_at, 8);
}

/// The verdict is not "every loop": with nothing to append there is nothing
/// to lower.
proof fn an_init_only_trace_is_not_flagged()
    ensures
        !acc_ok(seq![Ev::Init(seq![1u8]), Ev::Read]),
{
    reveal_with_fuel(has_append_at, 4);
}

/// The control: a read BEFORE the loop sees the init in both strategies, so
/// it is not a stale read and must stay accepted.
proof fn a_read_before_the_loop_is_fine()
    ensures
        acc_ok(
            seq![
                Ev::Init(seq![1u8]),
                Ev::Read,
                Ev::Append(seq![2u8]),
                Ev::Join,
                Ev::Read,
            ],
        ),
{
    reveal_with_fuel(has_append_at, 8);
    reveal_with_fuel(has_init_other_at, 8);
    reveal_with_fuel(has_append_other_at, 8);
    reveal_with_fuel(ok_reads_at, 8);
}

// ---------------------------------------------------------------------------
// Pins: each rule is LOAD-BEARING — flipping the strategy on that shape
// changes the emitted bytes, so the rule is what keeps the law true. Stated
// over SYMBOLIC segments (`a`, `x`, `y`), so no concrete-sequence evaluation
// is involved: the law and the pins talk about the same abstract trace.
// ---------------------------------------------------------------------------

/// The lowering is not the identity: every accepted append became a chunk
/// push, and the post-loop read sees the whole deferred concatenation.
proof fn the_chunks_really_accumulate()
    ensures
        forall|a: Str, x: Str, y: Str|
            pushes_of(
                seq![Ev::Init(a), Ev::Append(x), Ev::Append(y), Ev::Join, Ev::Read],
            ) == 2,
        forall|a: Str, x: Str, y: Str|
            go(
                seq![Ev::Init(a), Ev::Append(x), Ev::Append(y), Ev::Join, Ev::Read],
                true,
            ) == seq![a + x + y],
{
    reveal_with_fuel(chunked_at, 8);
    reveal_with_fuel(eager_at, 8);
}

/// A read while an append is outstanding observes the PRE-append string, so
/// `ok_reads` is load-bearing: the refused trace's second read is `a` in the
/// chunked run and `a+x` in the eager one (differing whenever `x` is
/// non-empty).
proof fn a_stale_read_would_differ()
    ensures
        forall|a: Str, x: Str| go(
            seq![Ev::Init(a), Ev::Append(x), Ev::Read, Ev::Join, Ev::Read],
            false,
        )[1] == a + x,
        forall|a: Str, x: Str| go(
            seq![Ev::Init(a), Ev::Append(x), Ev::Read, Ev::Join, Ev::Read],
            true,
        )[1] == a,
{
    reveal_with_fuel(chunked_at, 12);
    reveal_with_fuel(eager_at, 12);
}

/// An append the lowering cannot rewrite keeps re-concatenating, and the join
/// then overwrites it: the post-loop read is `a+x+y` eagerly and `a+x` chunked.
/// So `has_append_other` is load-bearing.
proof fn an_unrewritable_append_would_differ()
    ensures
        forall|a: Str, x: Str, y: Str| go(
            seq![Ev::Init(a), Ev::Append(x), Ev::AppendOther(y), Ev::Join, Ev::Read],
            false,
        )[0] == a + x + y,
        forall|a: Str, x: Str, y: Str| go(
            seq![Ev::Init(a), Ev::Append(x), Ev::AppendOther(y), Ev::Join, Ev::Read],
            true,
        )[0] == a + x,
{
    reveal_with_fuel(chunked_at, 12);
    reveal_with_fuel(eager_at, 12);
}

/// A non-literal init has no text to seed the chunk array with, so the join
/// loses the prefix: `a+x` eagerly, `x` chunked. So `has_init_other` is
/// load-bearing.
proof fn a_nonliteral_init_would_differ()
    ensures
        forall|a: Str, x: Str| go(
            seq![Ev::InitOther(a), Ev::Append(x), Ev::Join, Ev::Read],
            false,
        )[0] == a + x,
        forall|a: Str, x: Str| go(
            seq![Ev::InitOther(a), Ev::Append(x), Ev::Join, Ev::Read],
            true,
        )[0] == x,
{
    reveal_with_fuel(chunked_at, 12);
    reveal_with_fuel(eager_at, 12);
}

/// The join materializes the WHOLE deferred concatenation in order — not the
/// first or the last segment — and the eager side agrees (the law at the
/// accepted witness).
proof fn the_join_is_load_bearing()
    ensures
        forall|a: Str, x: Str, y: Str| go(
            seq![Ev::Init(a), Ev::Append(x), Ev::Append(y), Ev::Join, Ev::Read],
            true,
        )[0] == a + x + y,
        forall|a: Str, x: Str, y: Str| go(
            seq![Ev::Init(a), Ev::Append(x), Ev::Append(y), Ev::Join, Ev::Read],
            false,
        )[0] == a + x + y,
{
    reveal_with_fuel(chunked_at, 12);
    reveal_with_fuel(eager_at, 12);
}

/// The law instantiated at each refusal shape with no lemma in between: a
/// widened verdict makes a conjunct false HERE, so the failure is pinned on
/// the law itself.
proof fn law_at_the_refusal_witnesses()
    ensures
        go(w_stale(), acc_ok(w_stale())) == go(w_stale(), false),
        go(w_unrewritten(), acc_ok(w_unrewritten())) == go(w_unrewritten(), false),
        go(w_nonliteral(), acc_ok(w_nonliteral())) == go(w_nonliteral(), false),
{
    reveal_with_fuel(has_append_at, 8);
    reveal_with_fuel(has_init_other_at, 8);
    reveal_with_fuel(has_append_other_at, 8);
    reveal_with_fuel(ok_reads_at, 8);
    assert(!acc_ok(w_stale()));
    assert(!acc_ok(w_unrewritten()));
    assert(!acc_ok(w_nonliteral()));
}

proof fn law_at_the_accept_witness()
    ensures
        go(w_canonical(), acc_ok(w_canonical())) == go(w_canonical(), false),
{
    string_accumulator_preserves_output(w_canonical());
}

fn main() {}

}
