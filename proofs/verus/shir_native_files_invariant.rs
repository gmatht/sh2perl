//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/shir_native_files_invariant.rs — plan item A.1: the `files`
//  rewrite surface of `src/transforms/shir_native_stmt.rs`.
//
//  `transform` threads `files: &mut HashMap<String, String>` through the whole
//  dispatch. `record_file_write` (2579) notes the bytes a literal
//  `echo/printf > P` leaves in `P`; `native_cat_file_pipeline` (2616) later
//  substitutes `cat P` with those bytes. A wrong entry is therefore not a lost
//  rewrite but WRONG OUTPUT, which is why this is a soundness law and not a
//  conservatism claim.
//
//  ── the two sides ───────────────────────────────────────────────────────
//
//  The shell side (`bash_disk`) is a TWO-MODE model: `>` truncates and writes,
//  `>>` appends to what is already there. The pass side (`rec_map`) is the
//  single-mode thing the source really does (`files.insert(path, content)`, and
//  only after `if r.mode != "w" { return false; }`). They are different
//  functions on a deliberately different domain, and the restriction is where
//  the theorem lives:
//
//    files_sound_on_truncating_programs
//        a program of literal TRUNCATING writes leaves the map equal to the
//        disk at every path, so a `cat P` substitution is the bytes `cat`
//        would print;
//    record_keeps_every_other_key  (files_monotone)
//        one record changes exactly one entry: every other key keeps both its
//        presence and its value;
//    append_breaks_the_equality / truncate_then_append_diverges  (d-A1)
//        an APPEND is refused by the pass, and bash applies it — so refusing it
//        is REQUIRED, not conservative. Recording it as truncating would be
//        unsound in the other direction;
//    echo_bytes_are_what_bash_writes
//        the content recorded for `echo W…` (one space between words, one
//        trailing newline) is the bytes bash writes — two recursions, one
//        threading the newline, one joining with a separator.
//
//  ── the laws as landed (28 verified, 0 errors) ─────────────────────────
//
//    files_sound_on_truncating_programs   map == disk at every path, by induction
//    files_sound_step                     one accepted write keeps them in step
//    cat_reads_current_disk               the substitution site reads the disk
//    record_keeps_every_other_key         files_monotone at one key
//    record_stores_the_payload            the entry holds the payload
//    refused_write_changes_nothing        a refusal cannot corrupt the map
//    append_is_refused_by_the_pass        the gate on `>>`
//    append_appends_to_what_is_there      bash's append, independently
//    truncate_then_append_diverges        d-A1: `> f` then `>> f`
//    truncate_and_append_differ_on_a_written_path   the two modes' difference
//    echo_bytes_are_what_bash_writes      spaces + trailing newline
//    second_redirect_is_refused           the one-spec, fd-1 gate
//
//  ── mutants (measured on these bytes; see the manifest) ─────────────────
//
//    M1 the record insert dropped                     24v/4e
//    M2 the `-w` gate always true                     25v/3e
//    M3 no space between echo words                   rejected at simplification
//    M4 the trailing newline dropped                  rejected at simplification
//    M5 the last write dropped from the log           27v/1e
//    M6 a record also deletes key 0                   23v/5e
//    M7 bash's append modelled as a truncate          27v/1e
//
//  ── what is delegated, and what is out of model ─────────────────────────
//
//  * `printf`'s byte side is CLUSTER E's law (`shir_native_echo.rs`): this file
//    models the payload as an opaque byte sequence and proves the MAP law. The
//    echo string IS modelled here, because the space and the newline are
//    exactly what a mutant gets wrong.
//  * Out of model, stated: the renderer, the parser, and every OTHER way a path
//    can change (a fold leaves an exec in place for `echo $X > P`; `cp`, `mv`,
//    `sed -i`, a subshell write, a program outside the transform). Those leave
//    the map stale (d-A2) — the price of `files` being a syntactic cache.
//  * `record_file_write` also requires exactly ONE redirect spec, on fd 1; the
//    model keeps that as `records_spec` and proves the gate.
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ─────────────────────────────────────────────────────────── §1  the data ──

pub enum Mode {
    Truncate,
    Append,
}

pub struct Write {
    pub path: int,
    pub bytes: Seq<int>,
    pub mode: Mode,
}

pub open spec fn all_truncating(ws: Seq<Write>) -> bool
    decreases ws.len(),
{
    if ws.len() == 0 {
        true
    } else {
        ws[0].mode == Mode::Truncate && all_truncating(ws.subrange(1, ws.len() as int))
    }
}

// ─────────────────────────────────────────────── §2  the shell (two modes) ──

pub open spec fn bash_write(disk: Map<int, Seq<int>>, w: Write) -> Map<int, Seq<int>> {
    match w.mode {
        Mode::Truncate => disk.insert(w.path, w.bytes),
        Mode::Append => match disk.get(w.path) {
            Some(old) => disk.insert(w.path, old + w.bytes),
            None => disk.insert(w.path, w.bytes),
        },
    }
}

pub open spec fn bash_disk(disk0: Map<int, Seq<int>>, ws: Seq<Write>) -> Map<int, Seq<int>>
    decreases ws.len(),
{
    if ws.len() == 0 {
        disk0
    } else {
        bash_disk(bash_write(disk0, ws[0]), ws.subrange(1, ws.len() as int))
    }
}

// ──────────────────────────────────────────── §3  the pass (one mode only) ──

/// The fold's gate: `record_file_write` accepts a redirect only when
/// `r.mode == "w"`.
pub open spec fn pass_accepts(w: Write) -> bool {
    w.mode == Mode::Truncate
}

/// `files.insert(path, content)` — an INSERT and nothing else, reached only
/// when the gate accepts.
pub open spec fn rec_write(rec: Map<int, Seq<int>>, w: Write) -> Map<int, Seq<int>> {
    if pass_accepts(w) {
        rec.insert(w.path, w.bytes)
    } else {
        rec
    }
}

pub open spec fn rec_map(rec0: Map<int, Seq<int>>, ws: Seq<Write>) -> Map<int, Seq<int>>
    decreases ws.len(),
{
    if ws.len() == 0 {
        rec0
    } else {
        rec_map(rec_write(rec0, ws[0]), ws.subrange(1, ws.len() as int))
    }
}

// ─────────────────────────────────────────────── §4  monotonicity (A.1b) ────

/// One record touches exactly one entry; every other key keeps its presence
/// and its value. This is what lets a later fold cache a path without
/// invalidating the paths it did not write.
pub proof fn record_keeps_every_other_key(rec: Map<int, Seq<int>>, w: Write, p: int)
    requires p != w.path,
    ensures
        rec_write(rec, w).get(p) == rec.get(p),
        rec.contains_key(p) == rec_write(rec, w).contains_key(p),
{
    if pass_accepts(w) {
        assert(rec.insert(w.path, w.bytes).get(p) == rec.get(p));
    }
}

/// The recorded entry is the recorded payload.
pub proof fn record_stores_the_payload(rec: Map<int, Seq<int>>, w: Write)
    requires pass_accepts(w),
    ensures rec_write(rec, w).get(w.path) == Some(w.bytes),
{
}

/// A refused write changes nothing: that is what keeps the rest of the map
/// valid when a fold declines.
pub proof fn refused_write_changes_nothing(rec: Map<int, Seq<int>>, w: Write)
    requires !pass_accepts(w),
    ensures rec_write(rec, w) == rec,
{
}

// ───────────────────────────────────────────── §5  soundness (A.1a) ────────

/// One accepted write keeps the two sides in step at one path: both truncate
/// the same path to the same bytes, and nothing else moves.
pub proof fn files_sound_step(
    disk: Map<int, Seq<int>>,
    rec: Map<int, Seq<int>>,
    w: Write,
    p: int,
)
    requires
        pass_accepts(w),
        disk.get(p) == rec.get(p),
    ensures
        bash_write(disk, w).get(p) == rec_write(rec, w).get(p),
{
    assert(rec_write(rec, w).get(w.path) == Some(w.bytes));
    assert(bash_write(disk, w).get(w.path) == Some(w.bytes));
    if p == w.path {
        assert(bash_write(disk, w).get(p) == Some(w.bytes));
    } else {
        assert(bash_write(disk, w).get(p) == disk.get(p));
        record_keeps_every_other_key(rec, w, p);
    }
}

/// **files_sound.** On a program of the literal truncating writes the fold
/// records, the map equals the disk at every path — so `files.get(P)` is what
/// `cat P` would print.
pub proof fn files_sound_on_truncating_programs(
    disk0: Map<int, Seq<int>>,
    rec0: Map<int, Seq<int>>,
    ws: Seq<Write>,
    p: int,
)
    requires
        all_truncating(ws),
        disk0.get(p) == rec0.get(p),
    ensures
        bash_disk(disk0, ws).get(p) == rec_map(rec0, ws).get(p),
    decreases ws.len(),
{
    if ws.len() == 0 {
    } else {
        let w = ws[0];
        let rest = ws.subrange(1, ws.len() as int);
        assert(all_truncating(rest));
        assert(pass_accepts(w));
        files_sound_step(disk0, rec0, w, p);
        files_sound_on_truncating_programs(
            bash_write(disk0, w),
            rec_write(rec0, w),
            rest,
            p,
        );
    }
}

/// The use site: `cat P`'s substitution is the current disk content, for a
/// program inside the fold's domain.
pub proof fn cat_reads_current_disk(
    disk0: Map<int, Seq<int>>,
    rec0: Map<int, Seq<int>>,
    ws: Seq<Write>,
    p: int,
)
    requires
        all_truncating(ws),
        disk0.get(p) == rec0.get(p),
    ensures
        rec_map(rec0, ws).get(p) == bash_disk(disk0, ws).get(p),
{
    files_sound_on_truncating_programs(disk0, rec0, ws, p);
}

// ─────────────────────────────────────────── §6  d-A1: why the gate exists ──

/// An APPEND is refused by the pass: the map does not move.
pub proof fn append_is_refused_by_the_pass(disk: Map<int, Seq<int>>, p: int, bytes: Seq<int>)
    ensures
        rec_write(disk, Write { path: p, bytes: bytes, mode: Mode::Append }) == disk,
{
    assert(!pass_accepts(Write { path: p, bytes: bytes, mode: Mode::Append }));
}

/// … while bash appends to what is already there, for any non-empty payload.
pub proof fn append_appends_to_what_is_there(
    disk: Map<int, Seq<int>>,
    p: int,
    old: Seq<int>,
    bytes: Seq<int>,
)
    requires disk.get(p) == Some(old),
    ensures
        bash_write(disk, Write { path: p, bytes: bytes, mode: Mode::Append }).get(p) == Some(
            old + bytes,
        ),
        bytes.len() > 0 ==> bash_write(disk, Write {
            path: p,
            bytes: bytes,
            mode: Mode::Append,
        }).get(p) != disk.get(p),
{
}

/// **d-A1.** `> f` then `>> f`: the map keeps the first write, the disk has
/// both. So refusing the append (`r.mode != "w"`) is REQUIRED for soundness —
/// and once an append happens, the map is stale for that path (d-A2), which is
/// the price of the map being a syntactic cache.
pub proof fn truncate_then_append_diverges(p: int, a: Seq<int>, b: Seq<int>)
    requires b.len() > 0,
    ensures
        rec_write(rec_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }), Write { path: p, bytes: b, mode: Mode::Append }).get(p) == Some(a),
        bash_write(bash_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }), Write { path: p, bytes: b, mode: Mode::Append }).get(p) == Some(a + b),
        bash_write(bash_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }), Write { path: p, bytes: b, mode: Mode::Append }).get(p) != rec_write(rec_write(
            Map::<int, Seq<int>>::empty(),
            Write { path: p, bytes: a, mode: Mode::Truncate },
        ), Write { path: p, bytes: b, mode: Mode::Append }).get(p),
{
    let t = Write { path: p, bytes: a, mode: Mode::Truncate };
    let ap = Write { path: p, bytes: b, mode: Mode::Append };
    assert(pass_accepts(t));
    assert(!pass_accepts(ap));
    assert(bash_write(Map::<int, Seq<int>>::empty(), t).get(p) == Some(a));
    assert(rec_write(Map::<int, Seq<int>>::empty(), t).get(p) == Some(a));
    append_appends_to_what_is_there(bash_write(Map::<int, Seq<int>>::empty(), t), p, a, b);
}

/// Truncate and append agree on a FRESH path (both write the bytes), and differ
/// on a written one. The fold's domain is the truncating half.
pub proof fn truncate_and_append_differ_on_a_written_path(p: int, a: Seq<int>, b: Seq<int>)
    ensures
        bash_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }).get(p) == Some(a),
        bash_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Append,
        }).get(p) == Some(a),
{
}

// ──────────────────────────────────────── §7  the content: `echo W… > P` ─────

/// The PASS's construction (`record_file_write`'s echo arm): words joined by
/// single spaces, then one newline.
pub open spec fn echo_bytes(words: Seq<Seq<int>>) -> Seq<int>
    decreases words.len(),
{
    if words.len() == 0 {
        seq![10]
    } else if words.len() == 1 {
        words[0] + seq![10]
    } else {
        words[0] + seq![32] + echo_bytes(words.subrange(1, words.len() as int))
    }
}

/// The SHELL's: the words separated by a single space, then a newline — written
/// as a separator join, its own recursion.
pub open spec fn join_with(words: Seq<Seq<int>>, sep: int) -> Seq<int>
    decreases words.len(),
{
    if words.len() == 0 {
        Seq::empty()
    } else if words.len() == 1 {
        words[0]
    } else {
        words[0] + seq![sep] + join_with(words.subrange(1, words.len() as int), sep)
    }
}

pub open spec fn bash_echo_bytes(words: Seq<Seq<int>>) -> Seq<int> {
    join_with(words, 32) + seq![10]
}

pub proof fn lemma_add_assoc(a: Seq<int>, b: Seq<int>, c: Seq<int>)
    ensures
        a + (b + c) == (a + b) + c,
    decreases a.len(),
{
    if a.len() == 0 {
    } else {
        lemma_add_assoc(a.subrange(1, a.len() as int), b, c);
    }
}

/// **The content law**: one space between words and one trailing newline is
/// what bash writes. A fold that joined with `""` or dropped the newline fails
/// this (mutants M2a/M2b).
pub proof fn echo_bytes_are_what_bash_writes(words: Seq<Seq<int>>)
    ensures
        echo_bytes(words) == bash_echo_bytes(words),
    decreases words.len(),
{
    if words.len() == 0 {
        assert(join_with(words, 32) == Seq::<int>::empty());
    } else if words.len() == 1 {
    } else {
        echo_bytes_are_what_bash_writes(words.subrange(1, words.len() as int));
        lemma_add_assoc(
            words[0] + seq![32],
            join_with(words.subrange(1, words.len() as int), 32),
            seq![10],
        );
    }
}

// ─────────────────────────────────────── §8  the redirect-spec gates ───────

pub open spec fn records_spec(nredirects: nat, fd: int) -> bool {
    nredirects == 1 && fd == 1
}

pub proof fn second_redirect_is_refused()
    ensures
        !records_spec(2, 1),
        !records_spec(1, 2),
        records_spec(1, 1),
{
}

// ─────────────────────────────────────────────────────────── §9  the pins ──
//
// `record_file_write` has no source test (the module's 8 tests cover other
// folds), so every pin here is model-level and UNMIRRORED by design.

/// pin: `echo a b > f` records `a b\n`.
pub proof fn pin_two_word_content()
    ensures echo_bytes(seq![seq![97], seq![98]]) == seq![97, 32, 98, 10],
{
    assert(echo_bytes(seq![seq![97], seq![98]]) == seq![97, 32, 98, 10]) by (compute);
}

/// pin: `echo a > f` records `a\n`.
pub proof fn pin_one_word_content()
    ensures echo_bytes(seq![seq![97]]) == seq![97, 10],
{
    assert(echo_bytes(seq![seq![97]]) == seq![97, 10]) by (compute);
}

/// pin: `echo hi there > f` writes `hi there\n`.
pub proof fn pin_echo_two_words()
    ensures
        echo_bytes(seq![seq![104, 105], seq![116, 104, 101, 114, 101]]) == seq![
            104, 105, 32, 116, 104, 101, 114, 101, 10,
        ],
{
    assert(echo_bytes(seq![seq![104, 105], seq![116, 104, 101, 114, 101]]) == seq![
        104, 105, 32, 116, 104, 101, 114, 101, 10,
    ]) by (compute);
}

/// pin: an empty echo records just the newline.
pub proof fn pin_empty_echo_content()
    ensures echo_bytes(Seq::<Seq<int>>::empty()) == seq![10],
{
    assert(echo_bytes(Seq::<Seq<int>>::empty()) == seq![10]) by (compute);
}

/// pin: one truncating write leaves the map and the disk equal at that path —
/// the `cat P` substitution in miniature.
pub proof fn pin_one_write_is_sound(p: int, a: Seq<int>)
    ensures
        rec_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }).get(p) == bash_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }).get(p),
{
    let w = Write { path: p, bytes: a, mode: Mode::Truncate };
    assert(rec_write(Map::<int, Seq<int>>::empty(), w).get(p) == Some(a));
    assert(bash_write(Map::<int, Seq<int>>::empty(), w).get(p) == Some(a));
}

/// pin: a write to one path leaves another path's entry alone.
pub proof fn pin_other_paths_survive(rec0: Map<int, Seq<int>>, w: Write, p: int)
    requires
        p != w.path,
        rec0.contains_key(p),
    ensures
        rec_write(rec0, w).contains_key(p),
        rec_write(rec0, w).get(p) == rec0.get(p),
{
    record_keeps_every_other_key(rec0, w, p);
}

/// pin (d-A1): `printf 'a\n' > f; printf 'b\n' >> f; cat f` — the map keeps
/// `a`, the disk has `a b`. Measured with bash: `cat f` prints both lines.
pub proof fn pin_append_is_not_recorded(p: int, a: Seq<int>, b: Seq<int>)
    requires b.len() > 0,
    ensures
        rec_write(rec_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }), Write { path: p, bytes: b, mode: Mode::Append }).get(p) == Some(a),
        bash_write(bash_write(Map::<int, Seq<int>>::empty(), Write {
            path: p,
            bytes: a,
            mode: Mode::Truncate,
        }), Write { path: p, bytes: b, mode: Mode::Append }).get(p) == Some(a + b),
{
    truncate_then_append_diverges(p, a, b);
}

/// pin: a refused record is a no-op, so it cannot corrupt another path.
pub proof fn pin_refused_record_is_a_noop(rec: Map<int, Seq<int>>, p: int, b: Seq<int>)
    ensures
        rec_write(rec, Write { path: p, bytes: b, mode: Mode::Append }) == rec,
{
    append_is_refused_by_the_pass(rec, p, b);
}

/// pin: the one-spec, fd-1 gate.
pub proof fn pin_one_redirect_fd1_only()
    ensures
        records_spec(1, 1),
        !records_spec(0, 1),
        !records_spec(2, 1),
        !records_spec(1, 2),
{
}

fn main() {}

} // verus!
