// fbdev_gpu_calls.rs — the CALL-EMISSION rewrite: a lifted `GpuOp` becomes
// the `fb_*` primitive the shell's device write names.
//
// A shell draws by writing a line to a device file: `echo P > /dev/fb0/blocks`
// puts `P ++ "\n"` on `blocks` and sets the redirect's status (0). The
// `fbdev-gpu` lift keeps that write as a `GpuOp`; this transform replaces the
// op with the in-process call the same program would have made had it named
// the primitive directly — `fb_blocks(P)`. The call must therefore be the
// SAME device line on the SAME path, and it must leave the same statement
// status.
//
// THE TABLE (closed list — a new primitive is an explicit edit):
//
//     verb "blocks"        slot ""  →  fb_blocks(P)
//     verb "overlay"       slot ""  →  fb_overlay(P)
//     verb "overlay_flash" slot ""  →  fb_overlay_flash(P)
//     verb "call"   payload "swap"  →  fb_present()
//
// `fb_present` takes NO argument: its device line is the fixed present
// command "swap", so the rewrite is sound for the one literal payload the
// device names as a present. Any other `call` payload is a different device
// command whose bytes the argumentless call would drop.
//
// THE STATUS. `fb_blocks`/`fb_overlay`/`fb_overlay_flash` are `void`; the
// shell's device write is a redirect, whose status is 0 on success. So an
// emitted void call must SET 0 (the pre-fix emitter left `_sh_rc` stale:
// `false; echo x > /dev/fb0/blocks; echo $?` printed 1 where bash prints 0,
// measured). `fb_present` returns int with no failure signal (runtime/fbdev.c
// returns 0), so the pre-fix polarity `ret - 1` produced -1 — a NONZERO
// status for a successful present. The emitted status is the redirect's: 0.
//
// OBLIGATIONS (one per composition, plus the pinned naive rules):
//
//   L1 the_emitted_call_lands_on_the_same_path — the primitive addresses the
//      write's own path (a slotted write must not be sent to the bare verb).
//   L2 the_emitted_call_carries_the_same_line — the bytes are payload ++ "\n"
//      for the single-payload primitives and "swap" ++ "\n" for the present.
//   L3 the_emitted_call_is_the_redirect_status — every emitted call leaves
//      status 0, for every previous status.
//   L4 the_stream_of_calls_keeps_the_device_stream — over EVERY op list, the
//      emitted stream's lines equal the device-write stream's lines (the
//      fold, `go` vs `go_calls`).
//
//   P1 the_fallback_keeps_the_old_status — the fallback leaves `_sh_rc`
//      unchanged, so it is NOT the redirect status (the pre-fix defect).
//   P2 the_pre_fix_present_polarity_is_not_the_redirect_status — `ret - 1`
//      at the device's actual return (0) is -1, not 0.
//   P3 a_call_payload_is_not_droppable — `call` with any payload but "swap"
//      would emit the fixed "swap" line, not the write's line: refused.
//   P4 a_slotted_write_lands_on_another_path — a uniform write sent to the
//      bare verb names `device/verb`, not `device/slot/verb`: refused.
//   P5 an_unmapped_verb_is_not_a_device_call — a `depthmask` write has no
//      primitive; mapping it to `fb_blocks` would name `device/blocks`.
//
// NOT MODELLED, stated rather than assumed: the device's own parser, the
// in-process call's inability to fail (the status law is about the success
// path; bash's open-failure status 2 is the fallback's, and the C backend's
// redirect status is its own pre-existing gap), and the payload-expression
// census (`payload_direct_ok`): the model quantifies over payload BYTES, not
// over IR shapes, and the transform's own tests pin the shape refusals.

use vstd::prelude::*;

verus! {

pub open spec fn nl() -> int { 10 }
pub open spec fn slash() -> int { 47 }

pub open spec fn fb0() -> Seq<int> { seq![47, 100, 101, 118, 47, 102, 98, 48] }      // "/dev/fb0"
pub open spec fn webgl() -> Seq<int> { seq![47, 100, 101, 118, 47, 119, 101, 98, 103, 108] }
pub open spec fn v_blocks() -> Seq<int> { seq![98, 108, 111, 99, 107, 115] }          // "blocks"
pub open spec fn v_overlay() -> Seq<int> { seq![111, 118, 101, 114, 108, 97, 121] }   // "overlay"
pub open spec fn v_flash() -> Seq<int> { seq![111, 118, 101, 114, 108, 97, 121, 95, 102, 108, 97, 115, 104] }
pub open spec fn v_call() -> Seq<int> { seq![99, 97, 108, 108] }                     // "call"
pub open spec fn swap_line() -> Seq<int> { seq![115, 119, 97, 112] }                 // "swap"

pub struct Op {
    pub device: Seq<int>,
    pub slot: Seq<int>,
    pub verb: Seq<int>,
    pub payload: Seq<int>,
}

pub open spec fn device_ok(o: Op) -> bool {
    o.device == fb0() || o.device == webgl()
}

pub open spec fn path_of(o: Op) -> Seq<int> {
    if o.slot.len() == 0 {
        o.device + seq![slash()] + o.verb
    } else {
        o.device + seq![slash()] + o.slot + seq![slash()] + o.verb
    }
}

/// What the transform emits for one op.
pub enum Emit {
    /// A one-payload primitive: the payload rides in the call.
    Wire(Seq<int>),
    /// `fb_present()` — no argument, the fixed present line.
    Present,
    /// Refused: the GpuOp stays and the renderer writes the original file.
    Fallback,
}

pub open spec fn emit(o: Op) -> Emit {
    if !device_ok(o) {
        Emit::Fallback
    } else if o.slot.len() != 0 {
        Emit::Fallback
    } else if o.verb == v_blocks() || o.verb == v_overlay() || o.verb == v_flash() {
        Emit::Wire(o.payload)
    } else if o.verb == v_call() && o.payload == swap_line() {
        Emit::Present
    } else {
        Emit::Fallback
    }
}

/// The device path the emitted call addresses.
pub open spec fn call_path(o: Op, e: Emit) -> Seq<int> {
    match e {
        Emit::Wire(_) => o.device + seq![slash()] + o.verb,
        Emit::Present => o.device + seq![slash()] + v_call(),
        Emit::Fallback => path_of(o),
    }
}

/// The device line the emitted call produces.
pub open spec fn call_bytes(o: Op, e: Emit) -> Seq<int> {
    match e {
        Emit::Wire(p) => p.push(nl()),
        Emit::Present => swap_line().push(nl()),
        Emit::Fallback => o.payload.push(nl()),
    }
}

/// The shell write's line (the fallback's bytes, and what the call must match).
pub open spec fn wire(o: Op) -> Seq<int> {
    o.payload.push(nl())
}

/// The status after the statement. An emitted call is a successful device
/// write: 0. The fallback leaves the previous status untouched (the stale
/// pre-fix behaviour).
pub open spec fn status_after(e: Emit, prev: int) -> int {
    match e {
        Emit::Wire(_) => 0,
        Emit::Present => 0,
        Emit::Fallback => prev,
    }
}

/// The pre-fix present polarity: `_sh_rc = (fb_present() - 1)`.
pub open spec fn pre_fix_present_status(ret: int) -> int {
    ret - 1
}

// ── L1: the path ─────────────────────────────────────────────────────────

proof fn a_single_payload_call_keeps_the_path(o: Op)
    requires emit(o) is Wire, o.slot.len() == 0
    ensures call_path(o, emit(o)) == path_of(o)
{
}

proof fn a_present_keeps_the_path(o: Op)
    requires emit(o) is Present
    ensures call_path(o, emit(o)) == path_of(o), o.verb == v_call(), o.payload == swap_line()
{
    assert(path_of(o) == o.device + seq![slash()] + v_call());
}

// ── L2: the line ─────────────────────────────────────────────────────────

proof fn a_single_payload_call_keeps_the_line(o: Op)
    requires emit(o) is Wire
    ensures call_bytes(o, emit(o)) == wire(o)
{
    let e = emit(o);
    match e {
        Emit::Wire(_) => {
            assert(e == Emit::Wire(o.payload));
        }
        _ => {}
    }
}

proof fn a_present_keeps_the_line(o: Op)
    requires emit(o) is Present
    ensures call_bytes(o, emit(o)) == wire(o)
{
    assert(o.payload == swap_line());
    assert(call_bytes(o, emit(o)) == swap_line().push(nl()));
    assert(wire(o) == swap_line().push(nl()));
}

// ── L3: the status ───────────────────────────────────────────────────────

proof fn the_emitted_call_is_the_redirect_status(o: Op, prev: int)
    requires emit(o) is Wire || emit(o) is Present
    ensures status_after(emit(o), prev) == 0
{
}

proof fn an_emitted_call_sets_zero_from_any_previous(o: Op)
    requires emit(o) is Wire || emit(o) is Present
    ensures status_after(emit(o), 1) == 0, status_after(emit(o), 127) == 0
{
}

// ── L4: the stream ───────────────────────────────────────────────────────

pub open spec fn go(ops: Seq<Op>) -> Seq<Seq<int>>
    decreases ops.len()
{
    if ops.len() == 0 {
        seq![]
    } else {
        seq![wire(ops[0])] + go(ops.skip(1))
    }
}

pub open spec fn go_calls(ops: Seq<Op>) -> Seq<Seq<int>>
    decreases ops.len()
{
    if ops.len() == 0 {
        seq![]
    } else {
        seq![call_bytes(ops[0], emit(ops[0]))] + go_calls(ops.skip(1))
    }
}

proof fn the_stream_of_calls_keeps_the_device_stream(ops: Seq<Op>)
    ensures go_calls(ops) == go(ops)
    decreases ops.len()
{
    if ops.len() == 0 {
    } else {
        let o = ops[0];
        let e = emit(o);
        if e is Wire {
            a_single_payload_call_keeps_the_line(o);
        } else if e is Present {
            a_present_keeps_the_line(o);
        } else {
            assert(call_bytes(o, e) == wire(o));
        }
        assert(call_bytes(o, emit(o)) == wire(o));
        the_stream_of_calls_keeps_the_device_stream(ops.skip(1));
    }
}

// ── the refusals are part of the table ───────────────────────────────────

proof fn a_slotted_op_is_refused(o: Op)
    requires o.slot.len() > 0
    ensures emit(o) == Emit::Fallback
{
}

proof fn a_foreign_device_is_refused(o: Op)
    requires !device_ok(o)
    ensures emit(o) == Emit::Fallback
{
}

proof fn an_unmapped_verb_is_refused(o: Op)
    requires
        device_ok(o), o.slot.len() == 0,
        o.verb != v_blocks(), o.verb != v_overlay(), o.verb != v_flash(),
        !(o.verb == v_call() && o.payload == swap_line()),
    ensures emit(o) == Emit::Fallback
{
}

// ── pins: the naive rules ────────────────────────────────────────────────

proof fn the_fallback_keeps_the_old_status(prev: int)
    requires prev != 0
    ensures status_after(Emit::Fallback, prev) == prev
{
}

proof fn a_fallback_is_not_a_redirect_status()
    ensures status_after(Emit::Fallback, 1) != 0
{
}

proof fn the_pre_fix_present_polarity_is_not_the_redirect_status()
    ensures pre_fix_present_status(0) == -1, pre_fix_present_status(0) != 0
{
}

proof fn a_call_payload_is_not_droppable()
    ensures
        swap_line().push(nl()) != seq![99, 108, 101, 97, 114].push(nl()),  // "clear\n"
{
    assert(swap_line().push(nl()).len() == 5);
    assert(seq![99, 108, 101, 97, 114].push(nl()).len() == 6);
}

proof fn a_slotted_write_lands_on_another_path()
    ensures
        fb0() + seq![slash()] + v_blocks() != fb0() + seq![slash()] + seq![117, 110, 105, 102]
            + seq![slash()] + v_blocks(),
{
    assert((fb0() + seq![slash()] + v_blocks()).len() == 15);
    assert((fb0() + seq![slash()] + seq![117, 110, 105, 102] + seq![slash()] + v_blocks()).len() == 20);
}

proof fn an_unmapped_verb_is_not_a_device_call()
    ensures
        fb0() + seq![slash()] + v_blocks() != fb0() + seq![slash()] + seq![100, 101, 112, 116, 104]
            + seq![109, 97, 115, 107],  // "depthmask"
{
    assert((fb0() + seq![slash()] + v_blocks()).len() == 15);
    assert((fb0() + seq![slash()] + seq![100, 101, 112, 116, 104] + seq![109, 97, 115, 107]).len() == 18);
}

// ── the measured witnesses ───────────────────────────────────────────────

proof fn law_at_the_witnesses()
{
    // three blocks writes and a present: five lines in, five lines out
    let ops: Seq<Op> = seq![
        Op { device: fb0(), slot: seq![], verb: v_blocks(), payload: seq![98, 103] },       // bg
        Op { device: fb0(), slot: seq![], verb: v_blocks(), payload: seq![98, 108, 107] }, // blk
        Op { device: fb0(), slot: seq![], verb: v_call(), payload: swap_line() },
    ];
    assert(emit(ops[0]) == Emit::Wire(seq![98, 103]));
    assert(emit(ops[2]) == Emit::Present);
    the_stream_of_calls_keeps_the_device_stream(ops);
    // the present's emitted line is the shell's `swap\n`
    assert(call_bytes(ops[2], emit(ops[2])) == seq![115, 119, 97, 112, 10]);
    // the status after any emitted call is the redirect's
    assert(status_after(emit(ops[2]), 1) == 0);
}

fn main() {}

} // verus!
