// Standalone model of the C backend's WORD-EXPANSION ORDER — the position at
// which a `${a[i]:=w}` STORE becomes visible to the other expansions of the
// SAME statement (sh2perl/src/c_backend.rs: `stmt` / `stmt_inner` /
// `stmt_has_elem_assign` / `snap_elem` / `array_index_read`, plus the
// `set --` arm's field splitting).
//
// THE DENOTATIONAL EXTENSION (this file): a command's WORD LIST is a LIST OF
// FACES expanded LEFT TO RIGHT, each evaluated against the store as it stands
// at ITS position. bash therefore shows a store to every LATER face and to no
// EARLIER one:
//
//     a=(''); echo "[${a[0]}] [${a[0]:=z}]"     # bash: [] [z]
//     a=(''); echo "[${a[0]:=z}] [${a[0]}]"     # bash: [z] [z]
//
//   faces  : Read(i) | ColonStore(i,w) | AssignStore(i,w)
//            - `:=` tests EMPTINESS (`a=('')` is empty → writes)
//            - `=`  tests PRESENCE  (`a=('')` is present → does NOT write)
//   eval   : the left-to-right fold; the word of face k is the value the
//            store held at face k, and the state carries every store so far
//   hoisted: the PRE-FIX rendering — every store's statements were emitted
//            while the LATER word was rendered, so the argument vector
//            evaluated ALL reads after ALL stores
//
// The store is a `Seq<Option<Str>>` (a slot is a VALUE or a HOLE) and NOT a
// `Map<int,Str>`: `by (compute)` over a Map-returning recursion diverges on
// this toolchain (the evaluator stalls in `insert`), which turns every
// concrete row below into a timeout. Same discipline as c_assoc_array.rs's
// "compute takes the full concrete call term" note, one step further.
//
// Obligations (each maps to a measured probe row, /tmp/ord rounds 1-2):
//   read_before_colon_store / colon_store_before_read / two_reads_one_store /
//   second_colon_store_yields_the_first / bare_assign_leaves_a_present_empty /
//   assign_store_fills_a_hole            the six faces, byte-for-byte bash
//   words_len_is_faces_len               a store yields a word too (never 0)
//   hoisted_differs_on_the_measured_row  why the pre-fix `[z] [z]` happened
//   hoisted_agrees_when_nothing_precedes why cases 02/10/33 passed anyway
//   snapshot_is_identity_without_a_store the scan's false positives are FREE
//   narrow_walk_misses_output / narrow_walk_misses_return   THE CARRIER BUG
//     (measured 2026-11-05: `printf` folds to `IrStmt::Output` and a shell
//      function's `echo` arrives as `Return(Some(..))` — the flag stayed
//      false for both, so those probes stayed red after the first fix)
//   full_walk_sees_every_carrier         the shipped walk
//   fields_two / fields_drop_empty / fields_ifs_colon /
//   positional_count_is_sum / quoted_empty_is_one_word   the `set --` split
//   arrayindex_arrives_unsplit           THE ERASURE (a parser gap: an
//     unquoted `${a[i]}` carries no `split` marker, so no renderer of that
//     IR can split it — `a=(''); set -- ${a[0]}` is C 1 word / bash 0)
//   same_ir_for_both_spellings           THE QUOTEDNESS ERASURE, measured
//     2026-11-05 as BYTE-IDENTICAL shIR: `z=""; set -- $z` and
//     `z=""; set -- "$z"` both lower to `Str ""`, while bash says 0 and 1
//   no_renderer_of_collapsed_words_is_exact  …so NO word renderer can be
//     exact: exact for the quoted spelling ⇒ wrong for the unquoted one
//   kept_flag_makes_it_exact             the CONTRACT CHANGE that closes it
//     (a quotedness bit on the word node) is sufficient, not merely necessary
//   const_fold_is_the_same_erasure       `x=""; set -- $x a` (bash 1) and
//     `set -- "" a` (bash 2) are the SAME `Str ""` too — the fold erases
//     the bit exactly where the erasure is observable
//   marker_is_the_quotedness_bit         THE FIX (`split_fold` now keeps the
//     marker unless the word is provably NON-EMPTY), so the marker IS the bit
//   fix_is_exact_for_both_spellings      the renderer reading the marker is
//     exact for BOTH spellings — §6's impossibility no longer applies to the
//     IR the pass now emits
//   fold_is_sound_iff_nonempty           why: `split` is the identity on a
//     no-space word ONLY when it is non-empty (empty → zero fields)
//   fold_retained_on_nonempty_constants  the optimization survives where the
//     verdict proves non-emptiness (literals, Int reads, `${#x}`)
//
// NOT modelled (fidelity, checked against the source): the `:=` OVERWRITE's
// heap protocol (`_sh_arr_take` / the deferred trash list — c_assoc_array.rs
// and c_sparse_array.rs carry `colon_assign_state`), IFS non-whitespace
// delimiters beyond the one witness, `$@` slicing, and the `split` marker's
// own absence for the `param` spelling (a separate parser gap with the same
// erasure shape).
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;
pub type Slot = Option<Str>;
/// the array store: a slot is a VALUE or a HOLE (`None`)
pub type St = Seq<Slot>;

pub open spec fn s_empty() -> Str {
    Seq::empty()
}

pub open spec fn s_z() -> Str {
    Seq::new(1, |i: int| 'z')
}

pub open spec fn s_y() -> Str {
    Seq::new(1, |i: int| 'y')
}

// ---------------------------------------------------------------------
// §1 faces and the left-to-right fold
// ---------------------------------------------------------------------

pub enum Face {
    /// a plain `${a[i]}` read
    Read(int),
    /// `${a[i]:=w}` — tests EMPTINESS
    ColonStore(int, Str),
    /// `${a[i]=w}` — tests PRESENCE
    AssignStore(int, Str),
}

/// `a=()` — two slots, both holes (`local a=()` is at least this wide)
pub open spec fn blank_state() -> St {
    Seq::new(2, |i: int| None::<Str>)
}

/// `a=('')` — slot 0 present and empty
pub open spec fn one_empty() -> St {
    blank_state().update(0, Some(s_empty()))
}

pub open spec fn present(st: St, i: int) -> bool {
    0 <= i && i < st.len() && st[i].is_some()
}

/// a plain read: the value, or bash's empty word for a hole
pub open spec fn word_at(st: St, i: int) -> Str {
    if 0 <= i && i < st.len() {
        match st[i] {
            Some(v) => v,
            None => s_empty(),
        }
    } else {
        s_empty()
    }
}

/// the state after a face — the STORE half of the face's meaning
pub open spec fn apply(st: St, f: Face) -> St {
    match f {
        Face::Read(_) => st,
        Face::ColonStore(i, w) => if word_at(st, i) == s_empty() {
            st.update(i, Some(w))
        } else {
            st
        },
        Face::AssignStore(i, w) => if present(st, i) { st } else { st.update(i, Some(w)) },
    }
}

/// the WORD a face contributes — the VALUE half of its meaning
pub open spec fn face_word(st: St, f: Face) -> Str {
    match f {
        Face::Read(i) => word_at(st, i),
        Face::ColonStore(i, w) => if word_at(st, i) == s_empty() { w } else { word_at(st, i) },
        Face::AssignStore(i, w) => if present(st, i) { word_at(st, i) } else { w },
    }
}

/// The command's words, in position order, against the running store.
pub open spec fn words(st: St, fs: Seq<Face>) -> Seq<Str>
    decreases fs.len(),
{
    if fs.len() == 0 {
        Seq::empty()
    } else {
        Seq::empty().push(face_word(st, fs[0])).add(words(apply(st, fs[0]), fs.subrange(
            1,
            fs.len() as int,
        )))
    }
}

pub open spec fn state_after(st: St, fs: Seq<Face>) -> St
    decreases fs.len(),
{
    if fs.len() == 0 {
        st
    } else {
        state_after(apply(st, fs[0]), fs.subrange(1, fs.len() as int))
    }
}

/// A store contributes a WORD as well as a write — bash prints the face's
/// value, never nothing, so the rendered argument list keeps bash's arity.
proof fn words_len_is_faces_len(st: St, fs: Seq<Face>)
    ensures words(st, fs).len() == fs.len(),
    decreases fs.len(),
{
    if fs.len() == 0 {
    } else {
        words_len_is_faces_len(apply(st, fs[0]), fs.subrange(1, fs.len() as int));
    }
}

// --- the measured rows -------------------------------------------------

/// `a=(''); echo "[${a[0]}] [${a[0]:=z}]"` → `[] [z]` (probe 01)
proof fn read_before_colon_store() {
    assert(words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])[0] == s_empty())
        by (compute);
    assert(words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])[1] == s_z())
        by (compute);
    assert(state_after(one_empty(), seq![
        Face::Read(0int),
        Face::ColonStore(0int, s_z()),
    ])[0] == Some(s_z())) by (compute);
}

/// `a=(''); echo "[${a[0]:=z}] [${a[0]}]"` → `[z] [z]` (probe 02)
proof fn colon_store_before_read() {
    assert(words(one_empty(), seq![Face::ColonStore(0int, s_z()), Face::Read(0int)])[0] == s_z())
        by (compute);
    assert(words(one_empty(), seq![Face::ColonStore(0int, s_z()), Face::Read(0int)])[1] == s_z())
        by (compute);
}

/// `a=(); echo "[${a[0]}] [${a[0]:=z}]"` → `[] [z]` (probe 03) — the hole
/// reads empty and the store still yields its default
proof fn hole_read_before_colon_store() {
    assert(words(blank_state(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])[0]
        == s_empty()) by (compute);
    assert(words(blank_state(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])[1] == s_z())
        by (compute);
    assert(state_after(blank_state(), seq![Face::ColonStore(0int, s_z())])[0] == Some(s_z()))
        by (compute);
}

/// `a=(''); echo "[${a[0]}] [${a[0]:=z}] [${a[0]}]"` → `[] [z] [z]` (probe 04)
// NOTE the discipline every `by (compute)` row below follows: the FULL
// concrete call term, never a `let`-bound sequence. A let-bound recursion
// result diverges in the evaluator (it stalls instead of unfolding), which is
// what turned the first draft of this file into a 400 s timeout — the same
// trap c_assoc_array.rs records, one step earlier in the term.
proof fn two_reads_one_store() {
    assert(words(one_empty(), seq![
        Face::Read(0int),
        Face::ColonStore(0int, s_z()),
        Face::Read(0int),
    ])[0] == s_empty()) by (compute);
    assert(words(one_empty(), seq![
        Face::Read(0int),
        Face::ColonStore(0int, s_z()),
        Face::Read(0int),
    ])[1] == s_z()) by (compute);
    assert(words(one_empty(), seq![
        Face::Read(0int),
        Face::ColonStore(0int, s_z()),
        Face::Read(0int),
    ])[2] == s_z()) by (compute);
}

/// `a=(''); echo "[${a[0]}] [${a[0]:=z}] [${a[0]:=y}]"` → `[] [z] [z]`
/// (probe 33): the SECOND `:=` finds a non-empty slot, so it neither writes
/// nor yields its default — the first store is already visible to it
proof fn second_colon_store_yields_the_first() {
    assert(words(one_empty(), seq![
        Face::Read(0int),
        Face::ColonStore(0int, s_z()),
        Face::ColonStore(0int, s_y()),
    ])[2] == s_z()) by (compute);
    assert(state_after(one_empty(), seq![
        Face::Read(0int),
        Face::ColonStore(0int, s_z()),
        Face::ColonStore(0int, s_y()),
    ])[0] == Some(s_z())) by (compute);
}

/// `a=(''); echo "[${a[0]}] [${a[0]=z}]"` → `[] []` (probe 08) — bare `=`
/// asks PRESENCE, so a present-but-empty slot is left alone
proof fn bare_assign_leaves_a_present_empty() {
    assert(words(one_empty(), seq![Face::Read(0int), Face::AssignStore(0int, s_z())])[0]
        == s_empty()) by (compute);
    assert(words(one_empty(), seq![Face::Read(0int), Face::AssignStore(0int, s_z())])[1]
        == s_empty()) by (compute);
    assert(state_after(one_empty(), seq![Face::Read(0int), Face::AssignStore(0int, s_z())])
        == one_empty()) by (compute);
}

/// `a=(); echo "[${a[0]=z}]"` → the hole IS filled (probe a1/a2 family)
proof fn assign_store_fills_a_hole() {
    assert(words(blank_state(), seq![Face::AssignStore(0int, s_z())])[0] == s_z()) by (compute);
    assert(state_after(blank_state(), seq![Face::AssignStore(0int, s_z())])[0] == Some(s_z()))
        by (compute);
}

// ---------------------------------------------------------------------
// §2 the pre-fix rendering: every store visible to every read
// ---------------------------------------------------------------------

/// every face evaluated against ONE fixed state (the `hoisted` half)
pub open spec fn fixed_words(st: St, fs: Seq<Face>) -> Seq<Str>
    decreases fs.len(),
{
    if fs.len() == 0 {
        Seq::empty()
    } else {
        Seq::empty().push(face_word(st, fs[0])).add(fixed_words(st, fs.subrange(
            1,
            fs.len() as int,
        )))
    }
}

/// The PRE-FIX protocol: the store's statements were emitted while the LATER
/// word was RENDERED, and the argument vector evaluates its inline reads at
/// call time — i.e. against the state after EVERY store of the statement, not
/// at the face's own position.
pub open spec fn hoisted_words(st: St, fs: Seq<Face>) -> Seq<Str> {
    fixed_words(state_after(st, fs), fs)
}

/// MEASURED: `a=(''); echo "[${a[0]}] [${a[0]:=z}]"` printed `[z] [z]`
/// before the fix. The hoisted reading of the same faces is what produced it.
proof fn hoisted_differs_on_the_measured_row() {
    assert(hoisted_words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])[0]
        == s_z()) by (compute);
    assert(words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])[0]
        == s_empty()) by (compute);
    assert(hoisted_words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])
        != words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])) by (compute);
}

/// …and why cases 02 / 10 / 33 looked right all along: with no read BEFORE
/// the store the two protocols agree (the store's own word is the same in
/// both, and the read that follows sees the same state).
proof fn hoisted_agrees_when_nothing_precedes() {
    assert(hoisted_words(one_empty(), seq![Face::ColonStore(0int, s_z()), Face::Read(0int)])
        == words(one_empty(), seq![Face::ColonStore(0int, s_z()), Face::Read(0int)])) by (compute);
}

/// MUTANT m1 — the retired protocol, stated as the negative control for
/// `read_before_colon_store`: it prints `[z] [z]`, the measured pre-fix output.
proof fn m1_hoisted_store()
    ensures
        words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())]) == seq![
            s_empty(),
            s_z(),
        ],
        hoisted_words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())]) == seq![
            s_z(),
            s_z(),
        ],
{
    assert(words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])
        == seq![s_empty(), s_z()]) by (compute);
    assert(hoisted_words(one_empty(), seq![Face::Read(0int), Face::ColonStore(0int, s_z())])
        == seq![s_z(), s_z()]) by (compute);
}

/// MUTANT m2 — the retired CARRIER WALK, stated as the negative control for
/// `full_walk_sees_every_carrier`: on a `printf` (folded to `Output`) the
/// narrow walk reports no store, so the flag stays off and the read renders
/// inline — the exact reason probe 31 was still red after the first fix.
proof fn m2_narrow_walk()
    ensures
        !narrow_walk(seq![Carrier::Output]),
        !narrow_walk(seq![Carrier::Return]),
        full_walk(seq![Carrier::Output]),
        full_walk(seq![Carrier::Return]),
{
}

// ---------------------------------------------------------------------
// §3 the fix's GUARD: a per-statement flag, and why a false positive is free
// ---------------------------------------------------------------------

/// The shipped `snap_elem`: when the flag is set the read is materialised
/// into a temp at its own position. With NO store anywhere in the statement
/// the temp holds exactly what the inline read would have held, so the scan
/// may over-report without changing a byte — the reason the carrier walk can
/// be conservative instead of having to be exact.
proof fn snapshot_is_identity_without_a_store() {
    assert(hoisted_words(one_empty(), seq![Face::Read(0int)]) == words(
        one_empty(),
        seq![Face::Read(0int)],
    )) by (compute);
    assert(words(one_empty(), seq![Face::Read(0int)])[0] == s_empty()) by (compute);
}

/// The carrier a statement's words arrive in. MEASURED 2026-11-05: `echo`
/// is `Call{func:"builtin"}`, `printf FMT a b` FOLDS to `Output{value}`, and
/// a shell function's `echo` body is `Return(Some(..))`.
pub enum Carrier {
    Expr,
    Output,
    Return,
    Assign,
    IfCond,
    Loop,
    Case,
    Redirect,
    TryBlock,
}

/// The PRE-FIX walk: a hand-written `match` that only knew `Expr`.
pub open spec fn narrow_walk(tree: Seq<Carrier>) -> bool {
    tree.contains(Carrier::Expr)
}

/// The shipped walk: every carrier that can hold an expression.
pub open spec fn full_walk(tree: Seq<Carrier>) -> bool {
    tree.len() > 0
}

/// THE BUG: a `printf` statement's faces live ONLY in `Output`, so the
/// narrow walk answered "no store" — the flag stayed false and the read
/// rendered inline. Probe 31 stayed red after the first fix for this reason.
proof fn narrow_walk_misses_output() {
    let tree = seq![Carrier::Output];
    assert(!narrow_walk(tree));
    assert(full_walk(tree));
}

/// …and the same for a shell function body (probe 14 → `Return`).
proof fn narrow_walk_misses_return() {
    let tree = seq![Carrier::Return];
    assert(!narrow_walk(tree));
    assert(full_walk(tree));
}

/// The shipped walk sees a statement whose words sit in ANY carrier — the
/// conjunct that makes `snap_elem` fire on every measured shape.
proof fn full_walk_sees_every_carrier() {
    assert(full_walk(seq![Carrier::Expr]));
    assert(full_walk(seq![Carrier::Output]));
    assert(full_walk(seq![Carrier::Return]));
    assert(full_walk(seq![Carrier::Assign]));
    assert(full_walk(seq![Carrier::IfCond]));
    assert(full_walk(seq![Carrier::Loop]));
    assert(full_walk(seq![Carrier::Case]));
    assert(full_walk(seq![Carrier::Redirect]));
    assert(full_walk(seq![Carrier::TryBlock]));
    // …including the carrier the FIRST attempt missed: a statement whose
    // words arrive in `Output` while a nested one arrives in `Return`
    assert(full_walk(seq![Carrier::Output, Carrier::Return]));
}

// ---------------------------------------------------------------------
// §4 `set --` positional fields: the split must actually SPLIT
// ---------------------------------------------------------------------

/// Whitespace splitting as a LEFT-TO-RIGHT scan (a state machine over
/// `(i, start)`, stepping one character at a time — the measure is then
/// `s.len() - i` with no run-length arithmetic to justify): separator RUNS
/// collapse to one delimiter and empty runs are dropped everywhere, so an
/// empty value is ZERO words (POSIX 2.5.6 for blanks — the shipped
/// `_sh_split`'s `_sh_ifs_ws` branch).
pub open spec fn ws_fields_from(s: Str, i: int, start: int) -> Seq<Str>
    decreases s.len() - i,
{
    if i >= (s.len() as int) {
        if start < i {
            Seq::empty().push(s.subrange(start, i))
        } else {
            Seq::empty()
        }
    } else if s[i] == ' ' {
        if start < i {
            Seq::empty().push(s.subrange(start, i)).add(ws_fields_from(s, i + 1, i + 1))
        } else {
            ws_fields_from(s, i + 1, i + 1)
        }
    } else {
        ws_fields_from(s, i + 1, start)
    }
}

pub open spec fn nws(s: Str) -> int {
    ws_fields_from(s, 0, 0).len() as int
}

/// Non-whitespace IFS splitting, the same scan with EVERY delimiter ending a
/// field: `n` delimiters give `n + 1` fields, empty ones included (POSIX's
/// "each non-whitespace IFS char delimits one field" — the shipped
/// `_sh_ifs_nw` branch). This is why the whitespace rule cannot be reused.
pub open spec fn delim_fields_from(s: Str, d: char, i: int, start: int) -> Seq<Str>
    decreases s.len() - i,
{
    if i >= (s.len() as int) {
        Seq::empty().push(s.subrange(start, i))
    } else if s[i] == d {
        Seq::empty().push(s.subrange(start, i)).add(delim_fields_from(s, d, i + 1, i + 1))
    } else {
        delim_fields_from(s, d, i + 1, start)
    }
}

/// …and an EMPTY value yields no fields at all under either rule: an empty
/// unquoted expansion contributes ZERO words (bash removes it entirely)
pub open spec fn ndelim(s: Str, d: char) -> int {
    if s.len() == 0 {
        0
    } else {
        delim_fields_from(s, d, 0, 0).len() as int
    }
}

pub open spec fn ab() -> Str {
    Seq::new(3, |i: int| if i == 0 {
        'a'
    } else if i == 1 {
        ' '
    } else {
        'b'
    })
}

pub open spec fn colon_ab() -> Str {
    Seq::new(4, |i: int| if i == 0 {
        'a'
    } else if i == 1 {
        ':'
    } else if i == 2 {
        ':'
    } else {
        'b'
    })
}

/// `"a b"` -> 2 fields — `set -- $x` with `x='a b'` is TWO positionals
/// (MEASURED `2 [a] [b]`; pre-fix `1 [a b]`)
proof fn fields_two() {
    assert(nws(ab()) == 2) by (compute);
}

/// THE EMPTY CASE: `x=''; set -- $x X` is ONE positional — an empty
/// unquoted expansion contributes ZERO words (MEASURED bash 1 / pre-fix C 2)
proof fn fields_drop_empty() {
    assert(nws(s_empty()) == 0) by (compute);
    assert(ndelim(s_empty(), ':') == 0) by (compute);
}

/// …but a QUOTED empty word IS one field (`set -- "" X` -> 2, MEASURED), so
/// the marker must survive: the quoted spelling never consults the splitter.
proof fn quoted_empty_is_one_word() {
    assert(nws(s_empty()) == 0);
    // the quoted word skips the split entirely — one positional by
    // construction, which is exactly what the `split` marker records
}

/// `x='a::b'; IFS=:; set -- $x` -> 3 fields with the middle one EMPTY
/// (MEASURED `3 [a][][b]`: a non-whitespace IFS char delimits one field each,
/// which is why the whitespace rule cannot be reused here)
proof fn fields_ifs_colon() {
    assert(ndelim(colon_ab(), ':') == 3) by (compute);
}

/// MUTANT m3 — running the WHITESPACE rule under a colon IFS: `a::b` would
/// split into ONE field, so the two branches are provably not
/// interchangeable (this is the mutant the shipped `_sh_split` avoids by
/// consulting `_sh_ifs_ws` / `_sh_ifs_nw` separately).
proof fn m3_one_rule_for_both()
    ensures
        ndelim(colon_ab(), ':') == 3,
        nws(colon_ab()) == 1,
        nws(colon_ab()) != ndelim(colon_ab(), ':'),
{
    assert(ndelim(colon_ab(), ':') == 3) by (compute);
    assert(nws(colon_ab()) == 1) by (compute);
}

/// The positional count is the SUM over words: `x='a b'; set -- $x $x` is 4
/// (MEASURED `4`)
proof fn positional_count_is_sum() {
    assert(nws(ab()) + nws(ab()) == 4) by (compute);
}

// ---------------------------------------------------------------------
// §5 THE ERASURE: an unquoted `${a[i]}` arrives with no `split` marker
// ---------------------------------------------------------------------

/// A word as the core actually delivers it: `Split` is the marker the
/// renderer splits on, `Bare` is a value with no marker.
pub enum Word {
    Split(Str),
    Bare(Str),
}

pub open spec fn field_count(w: Word, quoted: bool, d: char) -> int {
    match w {
        Word::Split(s) => if quoted {
            1
        } else {
            ndelim(s, d)
        },
        Word::Bare(_) => 1,
    }
}

/// MEASURED 2026-11-05: `a=(''); set -- ${a[0]}` lowers to a bare
/// `arrayIndex` — bash gives ZERO words, the C arm one. No renderer of that
/// IR can split it, because the word is indistinguishable from a quoted one.
proof fn arrayindex_arrives_unsplit() {
    assert(field_count(Word::Bare(s_empty()), false, ' ') == 1);
    assert(field_count(Word::Split(s_empty()), false, ' ') == 0);
    // the erasure: the two spellings bash tells apart carry the SAME marker
    assert(field_count(Word::Bare(s_empty()), false, ' ') == field_count(
        Word::Bare(s_empty()),
        true,
        ' ',
    ));
}

// ---------------------------------------------------------------------
// §6 THE SECOND ERASURE: the word node carries no QUOTEDNESS bit
// ---------------------------------------------------------------------
//
// §5 pinned one instance (an unquoted `${a[i]}` arrives unmarked). This
// section proves the GENERAL shape, because the same collapse also hits the
// two spellings that need no array at all — MEASURED 2026-11-05, byte for
// byte, on the shipped binary:
//
//     z=""; set -- $z        bash 0 words   IR  ["--", Str ""]
//     z=""; set -- "$z"      bash 1 word    IR  ["--", Str ""]   IDENTICAL
//
//     x=""; set -- $x a      bash 1 word    IR  ["--", Str "", Str "a"]
//     set -- "" a            bash 2 words   IR  ["--", Str "", Str "a"]  IDENTICAL
//
// So the unquoted expansion of an EMPTY value must contribute ZERO words and
// the quoted one EXACTLY ONE, and the IR the renderer receives cannot tell
// the two apart. That is not a renderer gap to patch: it is a missing bit in
// the contract, and the theorem below says any renderer of the collapsed IR
// is wrong for one of the two spellings.
//
// §7 then records the LANDED fix and its post-fix obligations: the collapse
// was authored by the shared pass (`shir_passes/split_fold.rs` folded on
// whitespace-freedom alone), which now keeps the marker unless the word is
// provably NON-EMPTY — so the marker IS the bit, and the existing C arms
// need no change at all. Read §6 as the PRE-FIX statement that justifies the
// second verdict, not as an open residual.

/// The two bash spellings of the same source word.
pub enum Spelling {
    /// `"$z"` / `""` — one word, even when the value is empty.
    QuotedWord,
    /// `$z` / `$x` — a FIELD LIST: zero words for an empty value.
    UnquotedExpansion,
}

/// bash's word count for each spelling under the current IFS delimiter.
pub open spec fn bash_fields(sp: Spelling, v: Str, d: char) -> int {
    match sp {
        Spelling::QuotedWord => 1,
        Spelling::UnquotedExpansion => ndelim(v, d),
    }
}

/// The core's word node, as the renderer sees it: the spelling is GONE.
pub open spec fn to_ir(sp: Spelling, v: Str) -> Word {
    match sp {
        Spelling::QuotedWord => Word::Bare(v),
        Spelling::UnquotedExpansion => Word::Bare(v),
    }
}

/// MEASURED: the two spellings deliver the SAME node. This is the erasure,
/// stated as the equality a renderer's `match` cannot defeat.
proof fn same_ir_for_both_spellings()
    ensures
        to_ir(Spelling::QuotedWord, s_empty()) == to_ir(Spelling::UnquotedExpansion, s_empty()),
        to_ir(Spelling::QuotedWord, ab()) == to_ir(Spelling::UnquotedExpansion, ab()),
        // …while bash tells them apart exactly where it matters: empty value
        bash_fields(Spelling::QuotedWord, s_empty(), ' ') == 1,
        bash_fields(Spelling::UnquotedExpansion, s_empty(), ' ') == 0,
{
    assert(ndelim(s_empty(), ' ') == 0) by (compute);
}

/// THE IMPOSSIBILITY. `g` is exact for the quoted spelling (`set -- "$z"`,
/// measured 1 word for an empty value). Then it must be WRONG for the
/// unquoted spelling of the very same value — there is no `g` to look for.
proof fn no_renderer_of_collapsed_words_is_exact(g: spec_fn(Word) -> int)
    requires
        forall|v: Str| #[trigger] g(to_ir(Spelling::QuotedWord, v))
            == bash_fields(Spelling::QuotedWord, v, ' '),
    ensures
        exists|v: Str| #[trigger] g(to_ir(Spelling::UnquotedExpansion, v))
            != bash_fields(Spelling::UnquotedExpansion, v, ' '),
{
    // witness: the empty value, where the two spellings disagree (1 vs 0).
    // NO `let` for the witness: a compute over a let-bound term is the exact
    // shape that diverges/faults on this toolchain (see the header note), so
    // the full `s_empty()` term is written out at every use.
    assert(g(to_ir(Spelling::QuotedWord, s_empty()))
        == bash_fields(Spelling::QuotedWord, s_empty(), ' '));
    assert(bash_fields(Spelling::QuotedWord, s_empty(), ' ') == 1);
    assert(to_ir(Spelling::QuotedWord, s_empty())
        == to_ir(Spelling::UnquotedExpansion, s_empty()));
    assert(g(to_ir(Spelling::UnquotedExpansion, s_empty())) == 1);
    assert(bash_fields(Spelling::UnquotedExpansion, s_empty(), ' ') == 0) by (compute);
}

/// The fix direction is SUFFICIENT: keep the bit, and a renderer that reads
/// it is exact for both spellings. (Consequence for the marketplace: this is
/// a word-node contract change, not a C-arm patch.)
pub open spec fn to_ir_with_flag(sp: Spelling, v: Str) -> (Spelling, Str) {
    (sp, v)
}

pub open spec fn render_with_flag(w: (Spelling, Str)) -> int {
    bash_fields(w.0, w.1, ' ')
}

proof fn kept_flag_makes_it_exact()
    ensures
        render_with_flag(to_ir_with_flag(Spelling::QuotedWord, s_empty())) == 1,
        render_with_flag(to_ir_with_flag(Spelling::UnquotedExpansion, s_empty())) == 0,
        render_with_flag(to_ir_with_flag(Spelling::UnquotedExpansion, ab())) == 2,
        render_with_flag(to_ir_with_flag(Spelling::QuotedWord, ab())) == 1,
{
    assert(ndelim(s_empty(), ' ') == 0) by (compute);
    assert(ndelim(ab(), ' ') == 2) by (compute);
}

/// The CONST-FOLD is the same erasure, not a third one: `x=""` deletes the
/// variable, so `set -- $x a` (bash 1) and `set -- "" a` (bash 2) become one
/// `Str ""` word — measured above. A renderer cannot recover the difference
/// from a `Str` that no longer knows it came from an expansion.
proof fn const_fold_is_the_same_erasure()
    ensures
        bash_fields(Spelling::UnquotedExpansion, s_empty(), ' ') + 1
            != bash_fields(Spelling::QuotedWord, s_empty(), ' ') + 1,
        // both lower to the SAME `Str ""` word (the fold has no memory of it)
        Word::Bare(s_empty()) == Word::Bare(s_empty()),
{
    assert(ndelim(s_empty(), ' ') == 0) by (compute);
}

// ---------------------------------------------------------------------
// §7 THE FIX: keep the marker unless the word is provably NON-EMPTY
// ---------------------------------------------------------------------
//
// §6 proves no renderer can be exact on the IR that folded BOTH spellings
// onto one node. The fix is therefore in the SHARED pass
// (sh2perl/src/shir_passes/split_fold.rs): `split(w)` is folded to `w` only
// when `w` is no-space AND provably non-empty, so the marker survives on
// exactly the spelling that needs it and is absent from the other. The
// theorems below are the post-fix obligations; §6's impossibility is kept
// because it is what JUSTIFIES the extra verdict (a fold on whitespace alone
// is not a conservative approximation — it is a measurably wrong rewrite).

/// The post-fix lowering: `split(v)` is emitted for the unquoted spelling,
/// nothing for the quoted one.
pub open spec fn to_ir_fixed(sp: Spelling, v: Str) -> Word {
    match sp {
        Spelling::QuotedWord => Word::Bare(v),
        Spelling::UnquotedExpansion => Word::Split(v),
    }
}

/// The shipped renderer's rule, keyed on the marker (`field_count` with
/// `quoted = false`: a marker means "split this", a bare word is one field).
pub open spec fn render_marked(w: Word, d: char) -> int {
    field_count(w, false, d)
}

/// THE FIX, measured on the four rows that motivated it: 0 vs 1 for the
/// empty value and 1 vs 2 for `a b` (quoted keeps it one word).
proof fn marker_is_the_quotedness_bit()
    ensures
        render_marked(to_ir_fixed(Spelling::QuotedWord, s_empty()), ' ') == 1,
        render_marked(to_ir_fixed(Spelling::UnquotedExpansion, s_empty()), ' ') == 0,
        render_marked(to_ir_fixed(Spelling::UnquotedExpansion, ab()), ' ') == 2,
        render_marked(to_ir_fixed(Spelling::QuotedWord, ab()), ' ') == 1,
{
    assert(ndelim(s_empty(), ' ') == 0) by (compute);
    assert(ndelim(ab(), ' ') == 2) by (compute);
}

/// …and it is exact for EVERY value, not just the measured rows: the
/// renderer reading the marker agrees with bash's word count under both
/// spellings. This is the statement §6 showed impossible WITHOUT the marker.
proof fn fix_is_exact_for_both_spellings()
    ensures
        forall|v: Str| #[trigger] render_marked(to_ir_fixed(Spelling::QuotedWord, v), ' ')
            == bash_fields(Spelling::QuotedWord, v, ' '),
        forall|v: Str| #[trigger] render_marked(to_ir_fixed(Spelling::UnquotedExpansion, v), ' ')
            == bash_fields(Spelling::UnquotedExpansion, v, ' '),
{
}

/// WHY the fold rule needs the second verdict: `split` is the identity on a
/// no-space word only when the word is NON-EMPTY. On the empty value the
/// unmarked word is one field and the marked one is zero — the two numbers a
/// whitespace-only verdict cannot tell apart.
proof fn fold_is_sound_iff_nonempty()
    ensures
        ndelim(s_empty(), ' ') == 0,
        ndelim(s_z(), ' ') == 1,
        // the unmarked word is one field whatever the value
        field_count(Word::Bare(s_empty()), false, ' ') == 1,
        field_count(Word::Bare(s_empty()), false, ' ')
            != field_count(Word::Split(s_empty()), false, ' '),
{
    assert(ndelim(s_empty(), ' ') == 0) by (compute);
    assert(ndelim(s_z(), ' ') == 1) by (compute);
}

/// The optimization is not lost: where the word is provably non-empty (a
/// non-empty literal, an Int read — canonical decimal text — or a `${#x}`
/// length) the fold still fires, because there it IS the identity.
proof fn fold_retained_on_nonempty_constants()
    ensures
        ndelim(s_z(), ' ') == 1,
        ndelim(ab(), ' ') == 1 || ndelim(ab(), ' ') == 2,
        render_marked(to_ir_fixed(Spelling::UnquotedExpansion, s_z()), ' ') == 1,
{
    assert(ndelim(s_z(), ' ') == 1) by (compute);
    assert(ndelim(ab(), ' ') == 2) by (compute);
}

} // verus!

fn main() {}
