// fbdev_gpu.rs — the device-write LIFT: `echo PAYLOAD > /dev/fb0/VERB` →
// `GpuOp{device, slot, verb, payload}`.
//
// WHAT THE DEVICE SEES. A shell draws by writing text to a device FILE:
//
//     echo "$blk_p" > /dev/fb0/blocks          # one line per draw
//     echo "1"      > /dev/fb0/depthmask
//     echo "$yaw"   > /dev/fb0/uniform/1f/uCamYaw
//
// The bytes on the device are the echo line: `PAYLOAD ++ "\n"`, written to the
// path `DEVICE/VERB` (or `DEVICE/SLOT/VERB` when the device has sub-segments).
// bash opens the path `O_WRONLY|O_CREAT|O_TRUNC` and writes those bytes; the
// device's own parser then reads the line. So the observable of a write is the
// pair (path, payload ++ "\n"), and the lift is correct exactly when it keeps
// BOTH: a renderer with a GPU leg dispatches the op, a renderer without one
// reproduces the same file write.
//
// THE LAW. One obligation per composition of the rewrite:
//
//   L1 the_path_survives_the_lift — device ++ "/" ++ verb, and
//      device ++ "/" ++ slot ++ "/" ++ verb when the slot is non-empty, which
//      is the path the C fallback opens (a `uniform/1f` op must stay on
//      `uniform/1f`, not collapse to the verb).
//   L2 the_line_keeps_its_terminator — the op's bytes are `payload ++ "\n"`,
//      never the payload alone. This is the defect the batch rung measured:
//      the pre-fix fallback emitted `fputs(payload)`.
//   L3 the_lift_preserves_the_device_stream — over EVERY statement list, the
//      lifted list's (path, bytes) sequence is the original's: same count,
//      same order, same entries. `go`/`go_lifted` are the two folds.
//   L4 the_lift_is_a_pointwise_image — the i-th lifted statement is the i-th
//      statement's op (no statement is dropped, added or reordered).
//
// PINNED pre-fix / naive facts (each is a measured witness, see the anchor in
// harness/anchor-fbdev-gpu.sh):
//
//   P1 the_naive_fallback_loses_the_line — `wire_naive(o) = payload` differs
//      from `wire(o)` for EVERY op (the pre-fix `fputs(payload)`; measured:
//      `echo "1" > /dev/fb0/depthmask` emitted `fputs("1")` where bash wrote
//      `1\n`, under the shipped default transform set).
//   P2 an_empty_write_is_still_a_line — an empty payload is the line "\n";
//      the pre-fix refusal (P1's naive fallback) dropped it entirely.
//   P3 a_naive_slot_join_moves_the_write — always spelling
//      `device/slot/verb` yields `device//verb` for a top-level verb.
//   P4 a_printf_write_is_not_an_echo_write — `printf 'x' > dev` writes `x`
//      with no terminator (measured), so a printf payload is not the op model
//      and the lift must refuse it.
//   P5 an_echo_option_is_not_a_payload — `echo -n > dev` writes NOTHING
//      (measured), so a lone `-n` word is not a payload.
//
// NOT MODELLED, stated rather than assumed: the device's own parser (what a
// line means to `fb_blocks`), whether `fopen`/`write` succeed (an open failure
// is the same failure on both sides), and the lift's *recursion* into nested
// bodies — the driver models a flat list, which is what the transform's own
// statement walk produces for a top-level program (a nested body is a list of
// the same shape, transported by the same `go`). `printf`/`echo -n` are
// modelled only as REFUSED shapes (P4/P5), never as lifted ones.

use vstd::prelude::*;

verus! {

pub open spec fn nl() -> int { 10 }
pub open spec fn slash() -> int { 47 }

/// A device write as both the shell statement and the lifted node spell it:
/// the device root, the sub-segment slot, the last path segment (the verb)
/// and the payload expression's text.
pub struct Op {
    pub device: Seq<int>,
    pub slot: Seq<int>,
    pub verb: Seq<int>,
    pub payload: Seq<int>,
}

/// The path the write lands on. The C fallback spells
/// `device/verb` when the slot is empty and `device/slot/verb` otherwise;
/// the shell's own path is the same string.
pub open spec fn path_of(o: Op) -> Seq<int> {
    if o.slot.len() == 0 {
        o.device + seq![slash()] + o.verb
    } else {
        o.device + seq![slash()] + o.slot + seq![slash()] + o.verb
    }
}

/// The bytes of the device line: the echo text plus its terminator.
pub open spec fn wire(o: Op) -> Seq<int> {
    o.payload.push(nl())
}

/// The PRE-FIX fallback's bytes: `fputs(payload, _f)` with no terminator.
pub open spec fn wire_naive(o: Op) -> Seq<int> {
    o.payload
}

/// A naive slot join that always writes `device/slot/verb`.
pub open spec fn path_naive(o: Op) -> Seq<int> {
    o.device + seq![slash()] + o.slot + seq![slash()] + o.verb
}

/// `printf PAYLOAD > dev` has no implicit terminator.
pub open spec fn wire_printf(o: Op) -> Seq<int> {
    o.payload
}

/// `echo -n > dev` writes nothing at all.
pub open spec fn wire_echo_option(o: Op) -> Seq<int> {
    seq![]
}

pub enum Stmt {
    Write(Op),
    Lifted(Op),
    Other(int),
}

/// What the device sees for one statement, in order.
pub open spec fn obs(st: Stmt) -> Seq<(Seq<int>, Seq<int>)> {
    match st {
        Stmt::Write(o) => seq![(path_of(o), wire(o))],
        Stmt::Lifted(o) => seq![(path_of(o), wire(o))],
        Stmt::Other(_) => seq![],
    }
}

/// The transform: a device write becomes the op carrying the SAME record.
pub open spec fn lift(st: Stmt) -> Stmt {
    match st {
        Stmt::Write(o) => Stmt::Lifted(o),
        s => s,
    }
}

pub open spec fn go(xs: Seq<Stmt>) -> Seq<(Seq<int>, Seq<int>)>
    decreases xs.len()
{
    if xs.len() == 0 {
        seq![]
    } else {
        obs(xs[0]) + go(xs.skip(1))
    }
}

pub open spec fn go_lifted(xs: Seq<Stmt>) -> Seq<(Seq<int>, Seq<int>)>
    decreases xs.len()
{
    if xs.len() == 0 {
        seq![]
    } else {
        obs(lift(xs[0])) + go_lifted(xs.skip(1))
    }
}

// ── L1: the path composition ─────────────────────────────────────────────

proof fn a_top_level_verb_is_device_slash_verb(o: Op)
    requires o.slot.len() == 0
    ensures path_of(o) == o.device + seq![slash()] + o.verb
{
}

proof fn a_slot_keeps_its_path(o: Op)
    requires o.slot.len() > 0
    ensures path_of(o) == o.device + seq![slash()] + o.slot + seq![slash()] + o.verb
{
}

// ── L2: the line ─────────────────────────────────────────────────────────

proof fn the_line_keeps_its_terminator(o: Op)
    ensures wire(o) == o.payload + seq![nl()]
{
}

proof fn the_line_is_not_empty(o: Op)
    ensures wire(o).len() == o.payload.len() + 1
{
}

proof fn an_empty_write_is_still_a_line(o: Op)
    requires o.payload.len() == 0
    ensures wire(o) == seq![nl()], wire(o).len() == 1
{
}

// ── L3: the stream law ───────────────────────────────────────────────────

proof fn go_empty()
    ensures go(seq![]) == seq![], go_lifted(seq![]) == seq![]
{
}

proof fn the_lift_preserves_the_device_stream(xs: Seq<Stmt>)
    ensures go_lifted(xs) == go(xs)
    decreases xs.len()
{
    if xs.len() == 0 {
        go_empty();
    } else {
        assert(obs(lift(xs[0])) == obs(xs[0])) by {
            let st = xs[0];
            match st {
                Stmt::Write(o) => { assert(lift(st) == Stmt::Lifted(o)); }
                Stmt::Lifted(_) => {}
                Stmt::Other(_) => {}
            }
        };
        the_lift_preserves_the_device_stream(xs.skip(1));
    }
}

// ── L4: pointwise image ──────────────────────────────────────────────────

proof fn the_lift_is_a_pointwise_image(xs: Seq<Stmt>, i: int)
    requires 0 <= i && i < xs.len()
    ensures obs(lift(xs[i])) == obs(xs[i])
{
    let st = xs[i];
    match st {
        Stmt::Write(o) => { assert(lift(st) == Stmt::Lifted(o)); }
        Stmt::Lifted(_) => {}
        Stmt::Other(_) => {}
    }
}

// ── pins: the pre-fix / naive facts ──────────────────────────────────────

proof fn the_naive_fallback_loses_the_line(o: Op)
    ensures wire_naive(o) != wire(o), wire_naive(o).len() + 1 == wire(o).len()
{
}

proof fn an_empty_write_is_not_the_naive_fallback(o: Op)
    requires o.payload.len() == 0
    ensures wire_naive(o) == seq![], wire(o) == seq![nl()]
{
}

proof fn a_naive_slot_join_moves_the_write(o: Op)
    requires o.slot.len() == 0
    ensures path_naive(o) != path_of(o)
{
    assert(path_of(o) == o.device + seq![slash()] + o.verb);
    assert(path_naive(o) == o.device + seq![slash()] + seq![] + seq![slash()] + o.verb);
    assert(path_naive(o).len() == path_of(o).len() + 1);
}

proof fn a_printf_write_is_not_an_echo_write(o: Op)
    ensures wire_printf(o) != wire(o)
{
    assert(wire_printf(o).len() == o.payload.len());
    assert(wire(o).len() == o.payload.len() + 1);
}

proof fn an_echo_option_is_not_a_payload(o: Op)
    requires o.payload == seq![45int, 110int]  // "-n"
    ensures wire_echo_option(o) != wire(o)
{
    assert(wire(o).len() == 3);
}

// ── the measured witnesses, on the model ─────────────────────────────────

proof fn law_at_the_witnesses()
{
    let xs: Seq<Stmt> = seq![
        Stmt::Write(Op { device: dev(), slot: seq![], verb: verb_blocks(), payload: seq![98, 103] }),
        Stmt::Write(Op { device: dev(), slot: seq![], verb: verb_blocks(), payload: seq![98, 108, 107] }),
    ];
    the_lift_preserves_the_device_stream(xs);
    assert(go(xs) == go_lifted(xs));
    // the empty payload is the one-byte line
    assert(wire(Op { device: dev(), slot: seq![], verb: verb_blocks(), payload: seq![] })
        == seq![nl()]);
    // a slotted write stays on its sub-segment path
    assert(path_of(Op { device: dev(), slot: seq![117, 110, 105, 102, 111, 114, 109, 47, 49, 102],
                         verb: seq![117, 67, 97, 109, 89, 97, 119], payload: seq![49] })
        == seq![47, 100, 101, 118, 47, 102, 98, 48, 47,
                117, 110, 105, 102, 111, 114, 109, 47, 49, 102, 47,
                117, 67, 97, 109, 89, 97, 119]);
}

pub open spec fn dev() -> Seq<int> { seq![47, 100, 101, 118, 47, 102, 98, 48] }
pub open spec fn verb_blocks() -> Seq<int> { seq![98, 108, 111, 99, 107, 115] }

fn main() {}

} // verus!
