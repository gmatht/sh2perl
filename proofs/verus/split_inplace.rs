//! split_inplace — the destructive buffer-reuse flag preserves the output.
//!
//!   verus --triggers-mode silent --rlimit 40 split_inplace.rs
//!   -> verification results:: 29 verified, 0 errors
//!
//! `shir_passes/split_inplace.rs` rewrites the legacy
//! `for w in $X; do …; done` iterator (`Call{"split", [Var X]}`) into
//! `Ext(Split{in_place: true})`, licensing the C renderer to tokenize the
//! variable's OWN buffer (delimiters stamped with NUL) instead of copying it
//! into a scratch. The flag is not a hint about the program: it is a LICENCE
//! the transform earns from a census over every statement — the source is a
//! plain variable, it is stored somewhere outside the loop (so its buffer is a
//! `_sh_sset` heap block, never `getenv`/`argv` storage), the loop statement
//! reads it exactly once and stores it not at all, nothing after the loop
//! reads it, nothing reads it from a function body, the name is not exported,
//! `set -o allexport` is off, and a store after the loop never outlives a read
//! of the loop variable.
//!
//! This file models the two homes and proves they print the same bytes:
//!
//!   go(evs, inplace_ok(evs), w0(), delim(), false, false, 0)
//!     == go(evs, false, w0(), delim(), false, false, 0)
//!
//! for EVERY event sequence. `inplace_ok` is computed from the sequence, so
//! there is no antecedent on the shape: a refused program takes both sides
//! with the flag `false`, and an accepted one earns the equality from the
//! census.
//!
//! What the flag MEANS is the C renderer's contract (`c_backend`, the
//! for-in-split arm's `Ext(Split)` block). Three facts, each with witnesses:
//!
//! * the loop prints the SAME words in both homes — in-place tokenization
//!   keeps the field boundaries (it stamps the delimiter it just consumed);
//! * the buffer's BYTES change (delimiters become NUL), so a later read of the
//!   variable, an export, or a function that mentions it sees different bytes;
//! * the word array points INTO that buffer, so a store to the variable after
//!   the loop destroys the words bash keeps as copies
//!   (`a_store_that_outlives_a_word_read_is_load_bearing` — the measured bug),
//!   and a store INSIDE the loop frees the block under the iterator.
//!
//! Buffer ownership is a fact the IR cannot express, so it is its own refusal
//! (`an_unstored_source_is_refused`) rather than a hypothesis on the law.

use vstd::prelude::*;

verus! {

// ---------- bytes and values ----------

pub open spec fn nl() -> int {
    10
}

pub open spec fn delim() -> int {
    32
}

/// The byte in-place tokenization stamps over a consumed delimiter.
pub open spec fn zero() -> int {
    0
}

/// The marker a diverged observation contributes (never a byte either home
/// prints), so any occurrence makes the two traces differ.
pub open spec fn fault() -> int {
    33
}

/// The program's source value: two words, so stamping the delimiter is
/// observable (`[1,0,2]` vs `[1,32,2]`).
pub open spec fn w0() -> Seq<int> {
    seq![1, 2]
}

/// The value a later store writes.
pub open spec fn w2() -> Seq<int> {
    seq![3, 4]
}

// ---------- events ----------

pub enum Ev {
    /// `for w in $X; do <body that never mentions X>; done`
    SplitLoop,
    /// …its body reads `$X` (bash keeps X pristine; in place it is stamped)
    LoopReads,
    /// …its body stores `X` (the block is freed under the iterator)
    LoopWrites,
    /// a read of `$X`
    ReadX,
    /// a store to `X` — a fresh, program-owned buffer
    WriteX,
    /// a read of the loop variable `w` (a pointer into the source buffer)
    ReadWord,
    /// an opaque statement: eval/source/trap/asm/computed command
    WildX,
    /// `export X`: a child sees the bytes
    ExportX,
    /// a function body mentions `X` (call time unknown)
    FnX,
    /// `set -o allexport`
    Allexport,
    /// anything else
    Other,
}

// ---------- byte strings ----------

pub open spec fn join(ws: Seq<int>, sep: int) -> Seq<int>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else if ws.len() == 1 {
        seq![ws[0]]
    } else {
        seq![ws[0], sep] + join(ws.subrange(1, ws.len() as int), sep)
    }
}

/// What the loop body prints: each word followed by a newline.
pub open spec fn print_words(ws: Seq<int>) -> Seq<int> {
    join(ws, nl())
}

/// The variable's bytes in each home.
pub open spec fn buf(ws: Seq<int>, dl: int, stamped: bool) -> Seq<int> {
    if stamped {
        join(ws, zero())
    } else {
        join(ws, dl)
    }
}

pub open spec fn last_word(ws: Seq<int>) -> int {
    if ws.len() == 0 {
        0
    } else {
        ws[ws.len() - 1]
    }
}

// ---------- the two homes ----------
//
// `go` threads
//   ip      — the flag (the transform's verdict, fixed for the run),
//   ws, dl  — the source variable's current value,
//   st      — the buffer is stamped right now,
//   dead    — a store has run since the in-place loop, so the word array may
//             point at freed/overwritten bytes,
//   last    — the word the loop variable holds.

pub open spec fn go(
    evs: Seq<Ev>,
    ip: bool,
    ws: Seq<int>,
    dl: int,
    st: bool,
    dead: bool,
    last: int,
) -> Seq<int>
    decreases evs.len(),
{
    if evs.len() == 0 {
        Seq::empty()
    } else {
        let rest = evs.subrange(1, evs.len() as int);
        match evs[0] {
            Ev::SplitLoop => print_words(ws) + go(rest, ip, ws, dl, ip, false, last_word(ws)),
            Ev::LoopReads => print_words(ws) + buf(ws, dl, ip) + (if ip {
                seq![fault()]
            } else {
                Seq::empty()
            }) + go(rest, ip, ws, dl, ip, false, last_word(ws)),
            Ev::LoopWrites => (if ip {
                seq![fault()]
            } else {
                print_words(ws) + go(rest, ip, w2(), dl, false, false, last_word(ws))
            }),
            Ev::ReadX => buf(ws, dl, st) + go(rest, ip, ws, dl, st, dead, last),
            Ev::WriteX => go(rest, ip, w2(), dl, false, dead || st, last),
            Ev::ReadWord => (if ip && dead {
                seq![fault()]
            } else {
                seq![last]
            }) + go(rest, ip, ws, dl, st, dead, last),
            Ev::WildX => buf(ws, dl, st) + go(rest, ip, ws, dl, st, dead, last),
            Ev::ExportX => buf(ws, dl, st) + go(rest, ip, ws, dl, st, dead, last),
            Ev::FnX => buf(ws, dl, st) + go(rest, ip, ws, dl, st, dead, last),
            Ev::Allexport => go(rest, ip, ws, dl, st, dead, last),
            Ev::Other => go(rest, ip, ws, dl, st, dead, last),
        }
    }
}

// ---------- the census ----------

pub open spec fn is_loop(e: Ev) -> bool {
    match e {
        Ev::SplitLoop => true,
        _ => false,
    }
}

/// A loop whose BODY touches the source.
pub open spec fn is_hazard(e: Ev) -> bool {
    match e {
        Ev::LoopReads => true,
        Ev::LoopWrites => true,
        _ => false,
    }
}

/// An event that observes the source's bytes while the buffer may be stamped.
pub open spec fn is_observer(e: Ev) -> bool {
    match e {
        Ev::ReadX => true,
        Ev::WildX => true,
        Ev::SplitLoop => true,
        Ev::LoopReads => true,
        Ev::LoopWrites => true,
        _ => false,
    }
}

pub open spec fn is_write(e: Ev) -> bool {
    match e {
        Ev::WriteX => true,
        _ => false,
    }
}

pub open spec fn is_readword(e: Ev) -> bool {
    match e {
        Ev::ReadWord => true,
        _ => false,
    }
}

pub open spec fn has_loop(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        is_loop(evs[0]) || has_loop(evs.subrange(1, evs.len() as int))
    }
}

pub open spec fn has_hazard(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        is_hazard(evs[0]) || has_hazard(evs.subrange(1, evs.len() as int))
    }
}

pub open spec fn has_write(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        is_write(evs[0]) || has_write(evs.subrange(1, evs.len() as int))
    }
}

pub open spec fn has_export(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        match evs[0] {
            Ev::ExportX => true,
            _ => has_export(evs.subrange(1, evs.len() as int)),
        }
    }
}

pub open spec fn has_fn(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        match evs[0] {
            Ev::FnX => true,
            _ => has_fn(evs.subrange(1, evs.len() as int)),
        }
    }
}

pub open spec fn has_allexport(evs: Seq<Ev>) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        false
    } else {
        match evs[0] {
            Ev::Allexport => true,
            _ => has_allexport(evs.subrange(1, evs.len() as int)),
        }
    }
}

/// The statements after the first clean split loop.
pub open spec fn post(evs: Seq<Ev>) -> Seq<Ev>
    decreases evs.len(),
{
    if evs.len() == 0 {
        Seq::empty()
    } else if is_loop(evs[0]) {
        evs.subrange(1, evs.len() as int)
    } else {
        post(evs.subrange(1, evs.len() as int))
    }
}

/// The suffix licence. `seen` = a store to the source has already run, which
/// is what kills the words in the in-place home.
pub open spec fn suffix_ok(evs: Seq<Ev>, seen: bool) -> bool
    decreases evs.len(),
{
    if evs.len() == 0 {
        true
    } else {
        let rest = evs.subrange(1, evs.len() as int);
        if is_observer(evs[0]) {
            false
        } else if is_write(evs[0]) {
            suffix_ok(rest, true)
        } else if is_readword(evs[0]) {
            !seen && suffix_ok(rest, seen)
        } else {
            suffix_ok(rest, seen)
        }
    }
}

/// The transform's verdict. Nothing here is an antecedent — a `false` verdict
/// is a program the pass leaves alone.
pub open spec fn inplace_ok(evs: Seq<Ev>) -> bool {
    &&& has_loop(evs)
    // the source must live in program storage (a store outside the loop)
    &&& has_write(evs)
    &&& !has_hazard(evs)
    &&& !has_export(evs)
    &&& !has_fn(evs)
    &&& !has_allexport(evs)
    &&& suffix_ok(post(evs), false)
}

// ---------- the suffix ----------

/// `seen = true` forbids strictly more than `seen = false`.
proof fn suffix_ok_weaker(evs: Seq<Ev>)
    requires suffix_ok(evs, true),
    ensures suffix_ok(evs, false),
    decreases evs.len(),
{
    reveal_with_fuel(suffix_ok, 2);
    if evs.len() > 0 {
        let rest = evs.subrange(1, evs.len() as int);
        if is_observer(evs[0]) {
        } else if is_write(evs[0]) {
            suffix_ok_weaker(rest);
        } else if is_readword(evs[0]) {
            suffix_ok_weaker(rest);
        } else {
            suffix_ok_weaker(rest);
        }
    }
}

/// Once the loop has run, the two homes differ only in the buffer. The
/// in-place run starts stamped; the copying run never is (no loop remains in
/// the suffix), so its words stay alive.
proof fn suffix_agrees(evs: Seq<Ev>, ws: Seq<int>, dl: int, st1: bool, dead: bool, last: int)
    requires
        suffix_ok(evs, dead),
        !has_export(evs),
        !has_fn(evs),
    ensures go(evs, true, ws, dl, st1, dead, last) == go(evs, false, ws, dl, false, false, last),
    decreases evs.len(),
{
    reveal_with_fuel(go, 2);
    reveal_with_fuel(suffix_ok, 2);
    reveal_with_fuel(has_export, 2);
    reveal_with_fuel(has_fn, 2);
    if evs.len() > 0 {
        let rest = evs.subrange(1, evs.len() as int);
        match evs[0] {
            Ev::WriteX => {
                reveal_with_fuel(go, 2);
                assert(go(evs, true, ws, dl, st1, dead, last)
                    == go(rest, true, w2(), dl, false, dead || st1, last));
                assert(go(evs, false, ws, dl, false, false, last)
                    == go(rest, false, w2(), dl, false, false, last));
                if !(dead || st1) {
                    suffix_ok_weaker(rest);
                }
                suffix_agrees(rest, w2(), dl, false, dead || st1, last);
            },
            Ev::ReadWord => {
                reveal_with_fuel(go, 2);
                assert(!dead);
                assert(go(evs, true, ws, dl, st1, dead, last)
                    == seq![last] + go(rest, true, ws, dl, st1, dead, last));
                assert(go(evs, false, ws, dl, false, false, last)
                    == seq![last] + go(rest, false, ws, dl, false, false, last));
                suffix_agrees(rest, ws, dl, st1, dead, last);
            },
            Ev::Allexport | Ev::Other => {
                reveal_with_fuel(go, 2);
                assert(go(evs, true, ws, dl, st1, dead, last)
                    == go(rest, true, ws, dl, st1, dead, last));
                assert(go(evs, false, ws, dl, false, false, last)
                    == go(rest, false, ws, dl, false, false, last));
                suffix_agrees(rest, ws, dl, st1, dead, last);
            },
            _ => {
                // every other variant is excluded by the hypotheses: a read
                // of the bytes by `suffix_ok`, an export or function mention
                // by the census.
                assert(false);
            },
        }
    }
}

// ---------- the prefix ----------

/// Peels statements until the clean split loop. Before it the flag is
/// invisible: `st` and `dead` are false, so every read sees the pristine
/// bytes and a word read is the same in both homes.
proof fn law_from(evs: Seq<Ev>, ws: Seq<int>, dl: int)
    requires
        has_loop(evs),
        !has_hazard(evs),
        !has_export(evs),
        !has_fn(evs),
        suffix_ok(post(evs), false),
    ensures go(evs, true, ws, dl, false, false, 0) == go(evs, false, ws, dl, false, false, 0),
    decreases evs.len(),
{
    reveal_with_fuel(go, 2);
    reveal_with_fuel(has_loop, 2);
    reveal_with_fuel(has_hazard, 2);
    reveal_with_fuel(has_export, 2);
    reveal_with_fuel(has_fn, 2);
    reveal_with_fuel(post, 2);
    if evs.len() == 0 {
        assert(has_loop(evs) == false);
    } else {
        let rest = evs.subrange(1, evs.len() as int);
        match evs[0] {
            Ev::SplitLoop => {
                reveal_with_fuel(go, 2);
                assert(post(evs) == rest);
                assert(go(evs, true, ws, dl, false, false, 0)
                    == print_words(ws) + go(rest, true, ws, dl, true, false, last_word(ws)));
                assert(go(evs, false, ws, dl, false, false, 0)
                    == print_words(ws) + go(rest, false, ws, dl, false, false, last_word(ws)));
                suffix_agrees(rest, ws, dl, true, false, last_word(ws));
            },
            Ev::WriteX => {
                reveal_with_fuel(go, 2);
                assert(post(rest) == post(evs));
                assert(go(evs, true, ws, dl, false, false, 0)
                    == go(rest, true, w2(), dl, false, false, 0));
                assert(go(evs, false, ws, dl, false, false, 0)
                    == go(rest, false, w2(), dl, false, false, 0));
                law_from(rest, w2(), dl);
            },
            Ev::ReadWord => {
                reveal_with_fuel(go, 2);
                assert(post(rest) == post(evs));
                assert(go(evs, true, ws, dl, false, false, 0)
                    == seq![0] + go(rest, true, ws, dl, false, false, 0));
                assert(go(evs, false, ws, dl, false, false, 0)
                    == seq![0] + go(rest, false, ws, dl, false, false, 0));
                law_from(rest, ws, dl);
            },
            Ev::ReadX | Ev::WildX | Ev::ExportX | Ev::FnX => {
                // a read before the loop sees the pristine bytes in both homes
                reveal_with_fuel(go, 2);
                assert(post(rest) == post(evs));
                assert(go(evs, true, ws, dl, false, false, 0)
                    == buf(ws, dl, false) + go(rest, true, ws, dl, false, false, 0));
                assert(go(evs, false, ws, dl, false, false, 0)
                    == buf(ws, dl, false) + go(rest, false, ws, dl, false, false, 0));
                law_from(rest, ws, dl);
            },
            Ev::Allexport | Ev::Other => {
                reveal_with_fuel(go, 2);
                assert(post(rest) == post(evs));
                assert(go(evs, true, ws, dl, false, false, 0)
                    == go(rest, true, ws, dl, false, false, 0));
                assert(go(evs, false, ws, dl, false, false, 0)
                    == go(rest, false, ws, dl, false, false, 0));
                law_from(rest, ws, dl);
            },
            _ => {
                // a loop whose body touches the source is refused outright
                assert(false);
            },
        }
    }
}

// ---------- the law ----------

/// Hypothesis-free: every event sequence, every verdict.
proof fn split_in_place_preserves_output(evs: Seq<Ev>)
    ensures go(evs, inplace_ok(evs), w0(), delim(), false, false, 0) == go(
        evs,
        false,
        w0(),
        delim(),
        false,
        false,
        0,
    ),
{
    if inplace_ok(evs) {
        law_from(evs, w0(), delim());
    } else {
        assert(inplace_ok(evs) == false);
    }
}

/// The law instantiated at each refusal shape, with NO lemma in between: a
/// census that let any of these through makes a conjunct false HERE, so the
/// failure is pinned on the law itself and not only on the agreement lemma it
/// consumes.
pub open spec fn w_rx() -> Seq<Ev> {
    seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadX]
}

pub open spec fn w_wild() -> Seq<Ev> {
    seq![Ev::WriteX, Ev::SplitLoop, Ev::WildX]
}

pub open spec fn w_ex() -> Seq<Ev> {
    seq![Ev::WriteX, Ev::SplitLoop, Ev::ExportX]
}

pub open spec fn w_fn() -> Seq<Ev> {
    seq![Ev::WriteX, Ev::SplitLoop, Ev::FnX]
}

pub open spec fn w_store() -> Seq<Ev> {
    seq![Ev::WriteX, Ev::SplitLoop, Ev::WriteX, Ev::ReadWord]
}

proof fn law_at_the_refusal_witnesses()
    ensures
        go(w_rx(), inplace_ok(w_rx()), w0(), delim(), false, false, 0) == go(
            w_rx(),
            false,
            w0(),
            delim(),
            false,
            false,
            0,
        ),
        go(w_wild(), inplace_ok(w_wild()), w0(), delim(), false, false, 0) == go(
            w_wild(),
            false,
            w0(),
            delim(),
            false,
            false,
            0,
        ),
        go(w_ex(), inplace_ok(w_ex()), w0(), delim(), false, false, 0) == go(
            w_ex(),
            false,
            w0(),
            delim(),
            false,
            false,
            0,
        ),
        go(w_fn(), inplace_ok(w_fn()), w0(), delim(), false, false, 0) == go(
            w_fn(),
            false,
            w0(),
            delim(),
            false,
            false,
            0,
        ),
        go(w_store(), inplace_ok(w_store()), w0(), delim(), false, false, 0) == go(
            w_store(),
            false,
            w0(),
            delim(),
            false,
            false,
            0,
        ),
{
    reveal_with_fuel(has_loop, 6);
    reveal_with_fuel(has_write, 6);
    reveal_with_fuel(has_hazard, 6);
    reveal_with_fuel(has_export, 6);
    reveal_with_fuel(has_fn, 6);
    reveal_with_fuel(has_allexport, 6);
    reveal_with_fuel(post, 6);
    reveal_with_fuel(suffix_ok, 6);
    assert(!inplace_ok(w_rx()));
    assert(!inplace_ok(w_wild()));
    assert(!inplace_ok(w_ex()));
    assert(!inplace_ok(w_fn()));
    assert(!inplace_ok(w_store()));
}

// ---------- completeness: the flag is a decision, not a constant ----------

/// An eligible program IS flagged, so an identity pass is red here.
proof fn an_eligible_loop_is_flagged()
    ensures inplace_ok(seq![Ev::WriteX, Ev::SplitLoop]),
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_write, 3);
    reveal_with_fuel(has_hazard, 3);
    reveal_with_fuel(has_export, 3);
    reveal_with_fuel(has_fn, 3);
    reveal_with_fuel(has_allexport, 3);
    reveal_with_fuel(post, 3);
    reveal_with_fuel(suffix_ok, 3);
}

/// …and a program the census does not license is NOT flagged.
proof fn a_hazardous_loop_is_not_flagged()
    ensures !inplace_ok(seq![Ev::WriteX, Ev::LoopReads]),
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_hazard, 3);
}

/// The buffer-ownership rule: a source the program never stores is whatever
/// `getenv`/`argv` hands back, so there is no program buffer to stamp. The
/// model has no foreign storage, so the refusal is its own obligation and the
/// traces are pinned EQUAL (the refusal is a C-contract fact, not a byte one).
proof fn an_unstored_source_is_refused()
    ensures
        !inplace_ok(seq![Ev::SplitLoop]),
        go(seq![Ev::SplitLoop], true, w0(), delim(), false, false, 0) == go(
            seq![Ev::SplitLoop],
            false,
            w0(),
            delim(),
            false,
            false,
            0,
        ),
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_write, 3);
    reveal_with_fuel(go, 3);
    reveal_with_fuel(join, 4);
}

/// `set -o allexport` is refused conservatively: a store after the loop writes
/// a fresh value, so nothing re-exports the stamped bytes in this model. Stated
/// as a refusal, NOT claimed as load-bearing.
proof fn allexport_is_refused()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::Allexport]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::Allexport], true, w0(), delim(), false, false, 0)
            == go(seq![Ev::WriteX, Ev::SplitLoop, Ev::Allexport], false, w0(), delim(), false, false, 0),
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_allexport, 3);
    reveal_with_fuel(post, 3);
    reveal_with_fuel(go, 4);
    reveal_with_fuel(join, 4);
}

// ---------- the refusals, each with its two traces ----------

/// A body read of the source: bash keeps `$X` pristine, the in-place buffer is
/// stamped while the body runs.
proof fn body_read_is_load_bearing()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::LoopReads]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::LoopReads], true, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 10, 4, 3, 0, 4, 33],
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::LoopReads], false, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 10, 4, 3, 32, 4],
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_hazard, 3);
    reveal_with_fuel(go, 8);
    reveal_with_fuel(join, 6);
}

/// A body store of the source: the block is freed under the iterator.
proof fn body_write_is_load_bearing()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::LoopWrites]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::LoopWrites], true, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 33],
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::LoopWrites], false, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 10, 4],
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_hazard, 3);
    reveal_with_fuel(go, 8);
    reveal_with_fuel(join, 6);
}

/// A later read of the source observes the stamped bytes.
proof fn later_read_is_load_bearing()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadX]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadX], true, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 0, 4],
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadX], false, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 32, 4],
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(post, 3);
    reveal_with_fuel(suffix_ok, 3);
    reveal_with_fuel(go, 8);
    reveal_with_fuel(join, 6);
}

/// An opaque statement after the loop may read the bytes.
proof fn later_wild_is_load_bearing()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::WildX]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::WildX], true, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 0, 4],
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::WildX], false, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 32, 4],
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(post, 3);
    reveal_with_fuel(suffix_ok, 3);
    reveal_with_fuel(go, 8);
    reveal_with_fuel(join, 6);
}

/// An export hands the stamped bytes to a child.
proof fn export_is_load_bearing()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::ExportX]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ExportX], true, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 0, 4],
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ExportX], false, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 32, 4],
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_export, 3);
    reveal_with_fuel(go, 8);
    reveal_with_fuel(join, 6);
}

/// A function that mentions the source may run at any point.
proof fn fn_mention_is_load_bearing()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::FnX]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::FnX], true, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 0, 4],
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::FnX], false, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 3, 32, 4],
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(has_fn, 3);
    reveal_with_fuel(go, 8);
    reveal_with_fuel(join, 6);
}

/// THE MEASURED BUG (2026-11-20): a store after the loop destroys the words,
/// while bash's `w` keeps its value. Before the rule this printed a stale byte
/// (`last=\x05`) instead of the last word.
proof fn a_store_that_outlives_a_word_read_is_load_bearing()
    ensures
        !inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::WriteX, Ev::ReadWord]),
        go(
            seq![Ev::WriteX, Ev::SplitLoop, Ev::WriteX, Ev::ReadWord],
            true,
            w0(),
            delim(),
            false,
            false,
            0,
        ) == seq![3, 10, 4, 33],
        go(
            seq![Ev::WriteX, Ev::SplitLoop, Ev::WriteX, Ev::ReadWord],
            false,
            w0(),
            delim(),
            false,
            false,
            0,
        ) == seq![3, 10, 4, 4],
{
    reveal_with_fuel(has_loop, 3);
    reveal_with_fuel(post, 4);
    reveal_with_fuel(suffix_ok, 4);
    reveal_with_fuel(go, 8);
    reveal_with_fuel(join, 6);
}

/// The control: with no store after the loop the later word read is the same
/// word in both homes — the rule refuses the PAIR, not the read.
proof fn a_word_read_without_a_later_store_is_fine()
    ensures
        inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadWord]),
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadWord], true, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 4],
        go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadWord], false, w0(), delim(), false, false, 0)
            == seq![3, 10, 4, 4],
{
    reveal_with_fuel(has_loop, 6);
    reveal_with_fuel(has_write, 6);
    reveal_with_fuel(has_hazard, 6);
    reveal_with_fuel(has_export, 6);
    reveal_with_fuel(has_fn, 6);
    reveal_with_fuel(has_allexport, 6);
    reveal_with_fuel(post, 6);
    reveal_with_fuel(suffix_ok, 6);
    reveal_with_fuel(go, 6);
    reveal_with_fuel(join, 6);
    assert(inplace_ok(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadWord]));
    assert(go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadWord], true, w0(), delim(), false, false, 0)
        == seq![3, 10, 4, 4]);
    assert(go(seq![Ev::WriteX, Ev::SplitLoop, Ev::ReadWord], false, w0(), delim(), false, false, 0)
        == seq![3, 10, 4, 4]);
}

/// The law at the simplest eligible program, with no lemma in between.
proof fn law_at_the_simplest_witness()
    ensures go(
        seq![Ev::WriteX, Ev::SplitLoop],
        inplace_ok(seq![Ev::WriteX, Ev::SplitLoop]),
        w0(),
        delim(),
        false,
        false,
        0,
    ) == seq![3, 10, 4],
{
    reveal_with_fuel(go, 4);
    reveal_with_fuel(join, 6);
    an_eligible_loop_is_flagged();
}

fn main() {}

} // verus!
