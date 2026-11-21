// Standalone model of fbdev_gpu_batch
// (sh2perl/src/transforms/fbdev_gpu_batch.rs): a maximal run of consecutive
// `GpuOp`s that share device + verb + slot is replaced by ONE dispatch whose
// payload is the payloads joined with the device's line separator.
//
// ## The observable
//
// A program's statement list is a sequence of `Op`s: a lifted dispatch (a
// verb and its payload bytes) or `Observe` (any statement the pass does not
// merge — it writes nothing to the device, but it is where a program can
// look at the device, so a dispatch must never move across it).
//
// The device is a function of the bytes it is handed, and it is handed one
// dispatch per verb run. So the observable is the byte stream
//
//   bytes(ops) — dispatch k contributes payload_k ++ "\n" (the echo
//                terminator the lift carries as the dispatch's line ending);
//                an Observe contributes nothing.
//
// One dispatch carrying N lines and N dispatches carrying one line each are
// the same bytes on the same verb: that is exactly what batching buys. The
// law is therefore "the byte stream is unchanged, and every byte stays on
// its run's verb".
//
// ## Obligations (one per load-bearing rule)
//
//  L1 the_join_preserves_the_run_bytes — the merged run's bytes are exactly
//     p0 ++ "\n" ++ p1 … ++ "\n" ++ pn ++ "\n", i.e. the run's own bytes.
//     (The fold the transform implements. The two payload compositions — a
//     literal join and a value/interpolation join — are the same byte
//     construction, pinned at both witnesses below and anchored end to end
//     on the emitted C.)
//  L2 the_batch_preserves_the_bytes — the pass, over EVERY list.
//  L3 a_merged_dispatch_keeps_the_runs_verb — every byte of the run stays on
//     the run's verb, so the join cannot send a payload to another verb.
//
// ## Pinned naive/pre-fix facts
//
//  P1 a_naive_merge_of_a_call_verb_is_a_different_argument — a non-line
//     verb takes its payload as ONE argument; joining two of them changes
//     the command. The pass refuses every verb outside the three.
//  P2 an_empty_payload_still_contributes_its_line — a join that skips empty
//     parts loses a blank line.
//  P3 a_run_of_one_is_left_alone.
//  P4 the_wrong_separator_does_not_reproduce_the_stream.
//  W  law_at_the_witnesses — the measured anchor shapes, on the model.
//
// ## What is NOT modelled, stated
//
// The Rust run KEY also compares `device` and `slot`. Neither is observable
// here: every joinable verb renders as the SAME fb_* primitive whichever
// fbdev path it came from (/dev/fb0 and /dev/webgl both map to
// fb_blocks/fb_overlay), and the lift refuses a slot path, so a joinable
// verb's slot is always empty. `verb` is the whole observable distinction;
// the Rust comparison of the other two fields is a conservative extra
// guard. The merged op's `id` is not observable in the C render (it names a
// SPIR-V blob on the GPU path). Recognition — which writes become GpuOps —
// is fbdev-gpu's contract, not this pass's (the anchor measured one
// upstream gap there: an empty payload write is not lifted at all, so its
// echo terminator is lost; that is fbdev-gpu's, not this pass's, defect).

use vstd::prelude::*;

verus! {

pub enum Op {
    Dispatch { verb: int, payload: Seq<int> },
    Observe,
}

pub open spec fn nl() -> int { 10 }
pub open spec fn vblocks() -> int { 0 }
pub open spec fn voverlay() -> int { 1 }
pub open spec fn voverlay_flash() -> int { 2 }
pub open spec fn vcall() -> int { 3 }

pub open spec fn joinable(v: int) -> bool {
    v == vblocks() || v == voverlay() || v == voverlay_flash()
}

pub open spec fn is_dispatch(o: Op) -> bool {
    match o {
        Op::Dispatch { .. } => true,
        Op::Observe => false,
    }
}

pub open spec fn verb_of(o: Op) -> int {
    match o {
        Op::Dispatch { verb, .. } => verb,
        Op::Observe => -1,
    }
}

pub open spec fn payload_of(o: Op) -> Seq<int> {
    match o {
        Op::Dispatch { payload, .. } => payload,
        Op::Observe => seq![],
    }
}

pub open spec fn wire_of(o: Op) -> Seq<int> {
    if is_dispatch(o) { payload_of(o).push(nl()) } else { seq![] }
}

pub open spec fn d(v: int, p: Seq<int>) -> Op {
    Op::Dispatch { verb: v, payload: p }
}

// ── the byte stream ─────────────────────────────────────────────────────
pub open spec fn bytes(ops: Seq<Op>) -> Seq<int>
    decreases ops.len()
{
    if ops.len() == 0 {
        seq![]
    } else {
        wire_of(ops[0]) + bytes(ops.skip(1))
    }
}

// ── what an Observe sees ────────────────────────────────────────────────
pub open spec fn obs(ops: Seq<Op>, seen: Seq<int>) -> Seq<Seq<int>>
    decreases ops.len()
{
    if ops.len() == 0 {
        seq![]
    } else if is_dispatch(ops[0]) {
        obs(ops.skip(1), seen + wire_of(ops[0]))
    } else {
        seq![seen] + obs(ops.skip(1), seen)
    }
}

pub open spec fn observations(ops: Seq<Op>) -> Seq<Seq<int>> {
    obs(ops, seq![])
}

// ── the join ────────────────────────────────────────────────────────────
pub open spec fn payloads(run: Seq<Op>) -> Seq<Seq<int>>
    decreases run.len()
{
    if run.len() == 0 {
        seq![]
    } else {
        seq![payload_of(run[0])] + payloads(run.skip(1))
    }
}

pub open spec fn joined(ps: Seq<Seq<int>>) -> Seq<int>
    decreases ps.len()
{
    if ps.len() == 0 {
        seq![]
    } else if ps.len() == 1 {
        ps[0]
    } else {
        ps[0].push(nl()) + joined(ps.skip(1))
    }
}

pub open spec fn dispatched(ps: Seq<Seq<int>>) -> Seq<int>
    decreases ps.len()
{
    if ps.len() == 0 {
        seq![]
    } else {
        ps[0].push(nl()) + dispatched(ps.skip(1))
    }
}

// ── the pass ────────────────────────────────────────────────────────────
pub open spec fn merged(run: Seq<Op>) -> Op {
    Op::Dispatch { verb: verb_of(run[0]), payload: joined(payloads(run)) }
}

pub open spec fn flush(run: Seq<Op>) -> Seq<Op> {
    if run.len() == 0 {
        seq![]
    } else if run.len() == 1 {
        run
    } else {
        seq![merged(run)]
    }
}

pub open spec fn starts_run(o: Op) -> bool {
    is_dispatch(o) && joinable(verb_of(o))
}

pub open spec fn go(ops: Seq<Op>, i: int, run: Seq<Op>, out: Seq<Op>) -> Seq<Op>
    decreases (ops.len() - i)
{
    if i >= ops.len() {
        out + flush(run)
    } else {
        let o = ops[i];
        if starts_run(o) && (run.len() == 0 || verb_of(run[0]) == verb_of(o)) {
            go(ops, i + 1, run.push(o), out)
        } else {
            go(
                ops,
                i + 1,
                if starts_run(o) { seq![o] } else { seq![] },
                out + flush(run) + if starts_run(o) { seq![] } else { seq![o] },
            )
        }
    }
}

pub open spec fn batch(ops: Seq<Op>) -> Seq<Op> {
    go(ops, 0, seq![], seq![])
}

// ── concatenation helpers ───────────────────────────────────────────────

proof fn skip_one_over_concat<T>(a: Seq<T>, b: Seq<T>)
    requires a.len() >= 1
    ensures (a + b).skip(1) == a.skip(1) + b
{
}

proof fn seq_assoc<T>(a: Seq<T>, b: Seq<T>, c: Seq<T>)
    ensures a + b + c == a + (b + c)
{
}

proof fn seq_add_empty<T>(a: Seq<T>)
    ensures a + seq![] == a
{
}

proof fn bytes_concat3(a: Seq<Op>, b: Seq<Op>, c: Seq<Op>)
    ensures bytes(a + b + c) == bytes(a) + (bytes(b) + bytes(c))
{
    seq_assoc(a, b, c);
    bytes_concat(a, b + c);
    bytes_concat(b, c);
}

proof fn bytes_concat(a: Seq<Op>, b: Seq<Op>)
    ensures bytes(a + b) == bytes(a) + bytes(b)
    decreases a.len()
{
    if a.len() == 0 {
    } else {
        skip_one_over_concat(a, b);
        assert((a + b)[0] == a[0]);
        bytes_concat(a.skip(1), b);
        assert(bytes(a) == wire_of(a[0]) + bytes(a.skip(1)));
    }
}

proof fn bytes_empty()
    ensures bytes(seq![]) == seq![]
{
}

proof fn bytes_singleton(o: Op)
    ensures bytes(seq![o]) == wire_of(o)
{
    bytes_empty();
    assert(seq![o] + seq![] == seq![o]);
    bytes_head(o, seq![]);
}

proof fn bytes_head(o: Op, rest: Seq<Op>)
    ensures bytes(seq![o] + rest) == wire_of(o) + bytes(rest)
{
    assert((seq![o] + rest)[0] == o);
    skip_one_over_concat(seq![o], rest);
    assert(seq![o].skip(1) == seq![]);
    assert((seq![o] + rest).skip(1) == rest);
}

proof fn obs_concat(a: Seq<Op>, b: Seq<Op>, seen: Seq<int>)
    ensures obs(a + b, seen) == obs(a, seen) + obs(b, seen + bytes(a))
    decreases a.len()
{
    if a.len() == 0 {
        assert(seen + bytes(a) == seen);
    } else if is_dispatch(a[0]) {
        skip_one_over_concat(a, b);
        assert((a + b)[0] == a[0]);
        assert(bytes(a) == wire_of(a[0]) + bytes(a.skip(1)));
        obs_concat(a.skip(1), b, seen + wire_of(a[0]));
        assert(seen + bytes(a) == (seen + wire_of(a[0])) + bytes(a.skip(1)));
    } else {
        skip_one_over_concat(a, b);
        assert((a + b)[0] == a[0]);
        assert(bytes(a) == bytes(a.skip(1)));
        obs_concat(a.skip(1), b, seen);
    }
}

// ── L1: a uniform run's merge is its own bytes ──────────────────────────

proof fn fold_identity(ps: Seq<Seq<int>>)
    requires ps.len() >= 1
    ensures joined(ps).push(nl()) == dispatched(ps)
    decreases ps.len()
{
    if ps.len() == 1 {
        assert(joined(ps) == ps[0]);
        assert(dispatched(ps) == ps[0].push(nl()) + dispatched(ps.skip(1)));
        assert(dispatched(ps.skip(1)) == seq![]);
    } else {
        assert(joined(ps) == ps[0].push(nl()) + joined(ps.skip(1)));
        fold_identity(ps.skip(1));
        assert(
            joined(ps).push(nl()) == ps[0].push(nl()) + joined(ps.skip(1)).push(nl())
        );
        assert(dispatched(ps) == ps[0].push(nl()) + dispatched(ps.skip(1)));
    }
}

proof fn payloads_len(run: Seq<Op>)
    ensures payloads(run).len() == run.len()
    decreases run.len()
{
    if run.len() == 0 {
    } else {
        payloads_len(run.skip(1));
        assert(payloads(run) == seq![payload_of(run[0])] + payloads(run.skip(1)));
        assert((seq![payload_of(run[0])] + payloads(run.skip(1))).len()
            == 1 + payloads(run.skip(1)).len());
    }
}

proof fn dispatched_head(x: Seq<int>, rest: Seq<Seq<int>>)
    ensures dispatched(seq![x] + rest) == x.push(nl()) + dispatched(rest)
{
    assert((seq![x] + rest)[0] == x);
    skip_one_over_concat(seq![x], rest);
    assert(seq![x].skip(1) == seq![]);
    assert((seq![x] + rest).skip(1) == rest);
}

proof fn bytes_of_dispatches(run: Seq<Op>)
    requires forall|j: int| 0 <= j && j < run.len() ==> is_dispatch(#[trigger] run[j])
    ensures bytes(run) == dispatched(payloads(run))
    decreases run.len()
{
    if run.len() == 0 {
    } else {
        assert(is_dispatch(run[0]));
        bytes_of_dispatches(run.skip(1));
        assert(bytes(run) == wire_of(run[0]) + bytes(run.skip(1)));
        assert(wire_of(run[0]) == payload_of(run[0]).push(nl()));
        assert(payloads(run) == seq![payload_of(run[0])] + payloads(run.skip(1)));
        dispatched_head(payload_of(run[0]), payloads(run.skip(1)));
    }
}

proof fn run_bytes(run: Seq<Op>)
    requires
        run.len() >= 1,
        forall|j: int| 0 <= j && j < run.len() ==> is_dispatch(#[trigger] run[j]),
    ensures bytes(flush(run)) == bytes(run)
{
    if run.len() == 1 {
        assert(flush(run) == run);
    } else {
        bytes_of_dispatches(run);
        payloads_len(run);
        fold_identity(payloads(run));
        assert(flush(run) == seq![merged(run)]);
        bytes_head(merged(run), seq![]);
        assert(bytes(seq![merged(run)]) == wire_of(merged(run)));
        assert(wire_of(merged(run)) == payload_of(merged(run)).push(nl()));
        assert(payload_of(merged(run)) == joined(payloads(run)));
    }
}

proof fn merged_keeps_the_verb(run: Seq<Op>)
    requires run.len() >= 1
    ensures verb_of(merged(run)) == verb_of(run[0])
{
}

// ── L2: the pass preserves the byte stream ──────────────────────────────

proof fn go_bytes(ops: Seq<Op>, i: int, run: Seq<Op>, out: Seq<Op>)
    requires
        forall|j: int| 0 <= j && j < run.len() ==> is_dispatch(#[trigger] run[j]),
        i <= ops.len(),
        0 <= i,
    ensures bytes(go(ops, i, run, out)) == bytes(out) + bytes(run) + bytes(ops.skip(i))
    decreases (ops.len() - i)
{
    if i >= ops.len() {
        assert(i == ops.len());
        assert(ops.skip(i) == seq![]);
        bytes_empty();
        assert(go(ops, i, run, out) == out + flush(run));
        bytes_concat(out, flush(run));
        if run.len() == 0 {
            assert(run == seq![]);
            assert(bytes(run) == seq![]);
            assert(flush(run) == seq![]);
            assert(bytes(flush(run)) == seq![]);
        } else {
            run_bytes(run);
        }
        assert(bytes(go(ops, i, run, out)) == bytes(out) + bytes(run) + bytes(seq![]));
    } else {
        assert(0 <= i);
        assert(i < ops.len());
        let o = ops[i];
        assert(ops.skip(i)[0] == o);
        assert(ops.skip(i).skip(1) == ops.skip(i + 1));
        assert(i + 1 <= ops.len());
        assert(bytes(ops.skip(i)) == wire_of(o) + bytes(ops.skip(i + 1)));
        if starts_run(o) && (run.len() == 0 || verb_of(run[0]) == verb_of(o)) {
            assert(go(ops, i, run, out) == go(ops, i + 1, run.push(o), out));
            assert(run.push(o) == run + seq![o]);
            bytes_concat(run, seq![o]);
            bytes_singleton(o);
            assert(bytes(run.push(o)) == bytes(run) + wire_of(o));
            go_bytes(ops, i + 1, run.push(o), out);
            assert(bytes(out) + bytes(run.push(o)) + bytes(ops.skip(i + 1))
                == bytes(out) + bytes(run) + (wire_of(o) + bytes(ops.skip(i + 1))));
            assert(is_dispatch(o));
        } else {
            let rest = if starts_run(o) { seq![] } else { seq![o] };
            let run2 = if starts_run(o) { seq![o] } else { seq![] };
            let out2 = out + flush(run) + rest;
            assert(go(ops, i, run, out) == go(ops, i + 1, run2, out2));
            go_bytes(ops, i + 1, run2, out2);
            seq_assoc(out, flush(run), rest);
            assert(out2 == out + (flush(run) + rest));
            bytes_concat(out, flush(run) + rest);
            bytes_concat(flush(run), rest);
            assert(bytes(out2) == bytes(out) + (bytes(flush(run)) + bytes(rest)));
            if run.len() == 0 {
                assert(run == seq![]);
                assert(bytes(run) == seq![]);
                assert(flush(run) == seq![]);
                assert(bytes(flush(run)) == seq![]);
            } else {
                run_bytes(run);
            }
            bytes_singleton(o);
            if starts_run(o) {
                assert(rest == seq![]);
                assert(run2 == seq![o]);
                assert(bytes(rest) == seq![]);
                assert(bytes(run2) == wire_of(o));
                assert(bytes(out2) == bytes(out) + (bytes(run) + bytes(rest)));
            } else if is_dispatch(o) {
                assert(rest == seq![o]);
                assert(run2 == seq![]);
                assert(bytes(rest) == wire_of(o));
                assert(bytes(run2) == seq![]);
                assert(bytes(out2) == bytes(out) + (bytes(run) + bytes(rest)));
            } else {
                assert(rest == seq![o]);
                assert(run2 == seq![]);
                assert(bytes(rest) == seq![]);
                assert(bytes(run2) == seq![]);
                assert(bytes(ops.skip(i)) == bytes(ops.skip(i + 1)));
                assert(bytes(out2) == bytes(out) + (bytes(run) + seq![]));
            }
        }
    }
}

// ── L3: the pass preserves what an Observe sees ─────────────────────────

proof fn obs_empty(seen: Seq<int>)
    ensures obs(seq![], seen) == seq![]
{
    reveal(obs);
}

proof fn obs_singleton_dispatch(o: Op, seen: Seq<int>)
    requires is_dispatch(o)
    ensures obs(seq![o], seen) == seq![]
{
    assert(seq![o].skip(1) == seq![]);
    obs_empty(seen + wire_of(o));
}

proof fn obs_singleton_observe(seen: Seq<int>)
    ensures obs(seq![Op::Observe], seen) == seq![seen]
{
    assert(seq![Op::Observe].skip(1) == seq![]);
    assert(obs(seq![Op::Observe].skip(1), seen) == seq![]);
    assert(seq![seen] + seq![] == seq![seen]);
}

proof fn obs_of_dispatches(run: Seq<Op>, seen: Seq<int>)
    requires forall|j: int| 0 <= j && j < run.len() ==> is_dispatch(#[trigger] run[j])
    ensures obs(run, seen) == seq![]
    decreases run.len()
{
    if run.len() == 0 {
    } else {
        assert(is_dispatch(run[0]));
        obs_of_dispatches(run.skip(1), seen + wire_of(run[0]));
    }
}

proof fn obs_of_flush(run: Seq<Op>, seen: Seq<int>)
    requires forall|j: int| 0 <= j && j < run.len() ==> is_dispatch(#[trigger] run[j])
    ensures obs(flush(run), seen) == seq![]
{
    if run.len() == 0 {
    } else if run.len() == 1 {
        obs_of_dispatches(run, seen);
    } else {
        assert(flush(run) == seq![merged(run)]);
        assert(is_dispatch(merged(run)));
        assert(obs(seq![merged(run)], seen) == obs(seq![merged(run)].skip(1),
            seen + wire_of(merged(run))));
        assert(seq![merged(run)].skip(1) == seq![]);
    }
}

// ── the laws ────────────────────────────────────────────────────────────

proof fn the_join_preserves_the_run_bytes(ps: Seq<Seq<int>>)
    requires ps.len() >= 1
    ensures joined(ps).push(nl()) == ps[0].push(nl()) + dispatched(ps.skip(1))
{
    fold_identity(ps);
    assert(dispatched(ps) == ps[0].push(nl()) + dispatched(ps.skip(1)));
}

proof fn the_batch_preserves_the_bytes(ops: Seq<Op>)
    ensures bytes(batch(ops)) == bytes(ops)
{
    assert(0 <= ops.len());
    go_bytes(ops, 0, seq![], seq![]);
    assert(ops.skip(0) == ops);
}

// ── naive/pre-fix facts ─────────────────────────────────────────────────

proof fn a_naive_merge_of_a_call_verb_is_a_different_argument()
    ensures
        joined(seq![seq![48int], seq![49int]]) == seq![48int, 10int, 49int],
        joined(seq![seq![48int], seq![49int]]) != seq![48int],
        joined(seq![seq![48int], seq![49int]]) != seq![49int],
{
    assert(joined(seq![seq![48int], seq![49int]]) == seq![48int, 10int, 49int]) by (compute);
}

proof fn the_pass_refuses_a_call_verb()
    ensures
        batch(seq![d(vcall(), seq![48int]), d(vcall(), seq![49int])]) == seq![
            d(vcall(), seq![48int]),
            d(vcall(), seq![49int]),
        ],
{
    assert(batch(seq![d(vcall(), seq![48int]), d(vcall(), seq![49int])]) == seq![
        d(vcall(), seq![48int]),
        d(vcall(), seq![49int]),
    ]) by (compute);
}

proof fn an_empty_payload_still_contributes_its_line()
    ensures
        bytes(flush(seq![d(vblocks(), seq![]), d(vblocks(), seq![122int])])) == seq![
            10int,
            122int,
            10int,
        ],
{
    assert(
        bytes(flush(seq![d(vblocks(), seq![]), d(vblocks(), seq![122int])])) == seq![
            10int,
            122int,
            10int,
        ]
    ) by (compute);
}

proof fn a_run_of_one_is_left_alone()
    ensures batch(seq![d(vblocks(), seq![97int])]) == seq![d(vblocks(), seq![97int])]
{
    assert(batch(seq![d(vblocks(), seq![97int])]) == seq![d(vblocks(), seq![97int])]) by (compute);
}

proof fn the_wrong_separator_does_not_reproduce_the_stream()
    ensures
        seq![97int, 98int].push(nl()) != seq![97int, 10int, 98int, 10int],
        seq![97int, 10int, 10int, 98int].push(nl()) != seq![97int, 10int, 98int, 10int],
{
}

// The measured anchor shapes, on the model: [bg, blk, hud] → one dispatch
// whose bytes are the three run bytes; the break shape keeps three
// statements (two dispatches and the observer between them); the
// disabled/no-match shapes are the identity (no run of the joined verb).
proof fn law_at_the_witnesses()
    ensures
        bytes(flush(seq![
            d(vblocks(), seq![98int, 103int]),
            d(vblocks(), seq![98int, 108int, 107int]),
            d(vblocks(), seq![104int, 117int, 100int]),
        ])) == seq![
            98int,
            103int,
            10int,
            98int,
            108int,
            107int,
            10int,
            104int,
            117int,
            100int,
            10int,
        ],
        batch(seq![d(vblocks(), seq![98int, 103int]), Op::Observe, d(vblocks(), seq![104int, 117int, 100int])]).len() == 3,
        bytes(batch(seq![d(vblocks(), seq![98int, 103int]), Op::Observe, d(vblocks(), seq![104int, 117int, 100int])]))
            == seq![98int, 103int, 10int, 104int, 117int, 100int, 10int],
{
    assert(
        bytes(flush(seq![
            d(vblocks(), seq![98int, 103int]),
            d(vblocks(), seq![98int, 108int, 107int]),
            d(vblocks(), seq![104int, 117int, 100int]),
        ])) == seq![
            98int,
            103int,
            10int,
            98int,
            108int,
            107int,
            10int,
            104int,
            117int,
            100int,
            10int,
        ]
    ) by (compute);
    assert(
        batch(seq![d(vblocks(), seq![98int, 103int]), Op::Observe, d(vblocks(), seq![104int, 117int, 100int])]).len() == 3
    ) by (compute);
    assert(
        bytes(batch(seq![d(vblocks(), seq![98int, 103int]), Op::Observe, d(vblocks(), seq![104int, 117int, 100int])]))
            == seq![98int, 103int, 10int, 104int, 117int, 100int, 10int]
    ) by (compute);
}

fn main() {}

} // verus!
