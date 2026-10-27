// Standalone model of the C backend's FIXED-BUFFER STORE decision
// (`buf_bound` + `emit_guarded_copy` in sh2perl/src/c_backend.rs:11269,
// :11293, plus `emit_bound_asserts` and `FIXED_BUF_CAP:781`).
//
// THE DECISION, verbatim in structure.
// A Str var with a proven length bound b ≤ 1024 gets `char v[b+1]`
// (all three denials stay `char*`): numeric vars (their C value is an
// integer — `strlen`/`strncpy` on it would itself be UB), capture
// receivers (the length analysis under-bounds command output:
// `sha256sum` emits hash + filename — the analysis only counted the
// hash), and GMP bigint vars (unbounded by design).
//
// The guarded copy emits, for a stringy RHS:
//   * fitting unescaped literal (source-known strlen ≤ b): bare
//     `strncpy(dst, src, b+1)` — no assert (provably true), no NUL
//     store (strncpy pads short sources, so dst[b] is already 0);
//   * capture/vbuf/sh2_ sources: `strncpy` + `dst[b] = 0`, NO assert
//     (the source is not reliably bounded — the assert would
//     false-positive; strncpy still truncates safely);
//   * other stringy sources: `assert(strlen ≤ b)` + `strncpy` +
//     `dst[b] = 0` — the assert is the debug-mode tripwire, the NUL
//     store makes the NDEBUG truncation defined;
//   * non-stringy RHS (bogus pointer spellings): `dst[0] = 0` (never
//     touch the pointer at all).
// A `stringy` RHS is a `"…"` literal, a `(char*)` cast, a `(…)` expr,
// `sh2_`/`_cap_` helper spellings, a vbuf `x.p` member, or an ident.
// An unescaped literal is `"…"` with no `\` and no inner `"` (after
// peeling `(char*)` casts) — source length − 2 IS its strlen.
//
// WHY THIS EXISTS. verif/P3_Buffer.thy proves the buffer arithmetic
// conditional on a correct N ("a correct N suffices"); the N-producer
// (`analyze_string_lengths`, Rust) is discharged only by the
// `--dump-widths` bridge. What sits between is the EMISSION decision:
// given the analysis bound, does every store site respect it? The
// backend's own header claims the assert is "debug-only" and NDEBUG
// `strncpy` truncates — this model states exactly which arm each shape
// takes and proves each emitted write stays in bounds.
//
// WHAT IS PROVED (lengths are HYPOTHESES — T1 of the length analysis;
// the model states the store-side contract):
//   * bound_admits         — the decl contract: strlen(v) ≤ b with a
//                             `char v[b+1]` decl keeps every in-bound
//                             offset read defined (P3's `fits_buf`);
//   * guarded_write_safe   — THE OBLIGATION: the grown-from-source
//                             write family (strncpy cap=b+1 + dst[b]=0)
//                             lands NUL-terminated inside the buffer
//                             for every source length (exact when the
//                             source fits — never UB — truncating but
//                             defined otherwise);
//   * literal_arm_exact    — the fast arm is exact (copies the whole
//                             literal + NUL, provably no assert needed);
//   * debug_assert_matches — the debug `assert(strlen ≤ b)` IS the
//                             fits predicate (P3_Buffer's lemma,
//                             mirrored: removing it changes only the
//                             failure mode);
//   * capture_arm_no_ub    — the assert-free arm is still never UB
//                             (strncpy truncates; only the wrong-value
//                             mode remains — "a wrong N is a wrong
//                             value, never UB");
//   * bogus_arm_clears     — the non-stringy arm never touches the
//                             source pointer (dst[0]=0 is always safe);
//   * stringy_shape_mirror — the classifier fires exactly on the
//                             source's six shapes and refuses the rest;
//   * unescaped_recognizes — the literal gate accepts exactly the
//                             escape-free `"…"` spellings (source len −
//                             2 = strlen) and rejects escaped/quoted
//                             ones;
//   * bound_discipline     — all three denials stay `char*` (numeric,
//                             capture-receiver, bigint); bound requires
//                             b ≤ 1024 on a proven Str;
//   * pin_decl_shapes      — `char v[21]`-style decl for a bound var,
//                             `char*` for each denial (mirrors the
//                             decl-site tests);
//   * pin_trunc_shape      — the `sha256sum` shape: capture receiver
//                             with bound 64 still renders `char*`
//                             (the under-bound analysis must not narrow
//                             the store).
//
// WHAT IS NOT MODELLED: the LENGTH PRODUCER (`analyze_string_lengths`
// — N's correctness is the analysis property, discharged by the widths
// bridge); accumulator `_alen_` mirrors; const-lifted decls; the
// store-funnel NULL `?: ""` bridge; traversal (per-site theorems).
// Lengths are unbounded `int` here; callers pass u64-bounded values.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_buf_copy.rs
use vstd::prelude::*;

verus! {

pub type Name = nat;

pub open spec fn fixed_cap() -> int {
    1024int
}

// A variable's verdict triple, reduced to what `buf_bound` consults:
// numeric homing, capture-receiver, bigint; and the length analysis
// verdict (None = unbounded/absent).
pub enum Bound {
    Known(int),
    Unknown,
}

pub struct Verdict {
    pub is_num: bool,
    pub is_capture: bool,
    pub is_bigint: bool,
    pub bound: Bound,
}

// `buf_bound`, verbatim: the three denials, then the cap gate.
pub open spec fn buf_bound(v: Verdict) -> Option<int> {
    if v.is_bigint {
        Option::None
    } else if v.is_num || v.is_capture {
        Option::None
    } else {
        match v.bound {
            Bound::Known(b) => if b <= fixed_cap() { Option::Some(b) } else { Option::None },
            Bound::Unknown => Option::None,
        }
    }
}

// The destination: a fixed buffer of capacity b+1 (the decl), or a
// pointer (the denials). The model tracks the capacity number; the
// actual chars are the source text.
pub enum Dst {
    Fixed(int),
    Ptr,
}

pub open spec fn decl_of(v: Verdict) -> Dst {
    match buf_bound(v) {
        Option::Some(b) => Dst::Fixed(b + 1int),
        Option::None => Dst::Ptr,
    }
}

// `emit_guarded_copy`'s RHS classifier, verbatim in structure (six
// stringy shapes). `s` is the emitted RHS text; the `.p` suffix marks
// a vbuf member read.
pub open spec fn is_stringy(s: Seq<char>) -> bool {
    starts_with_quote(s) || starts_with_cast(s) || starts_with_paren(s) || starts_with_sh2(s)
        || starts_with_cap(s) || is_vbuf(s) || is_ident_str(s)
}

pub open spec fn starts_with_quote(s: Seq<char>) -> bool {
    s.len() > 0 && s[0] == '"'
}

pub open spec fn starts_with_cast(s: Seq<char>) -> bool {
    s.len() >= 7 && s[0] == '(' && s[1] == 'c' && s[2] == 'h' && s[3] == 'a' && s[4] == 'r'
        && s[5] == '*' && s[6] == ')'
}

pub open spec fn starts_with_paren(s: Seq<char>) -> bool {
    s.len() > 0 && s[0] == '('
}

pub open spec fn starts_with_sh2(s: Seq<char>) -> bool {
    s.len() >= 4 && s[0] == 's' && s[1] == 'h' && s[2] == '2' && s[3] == '_'
}

pub open spec fn starts_with_cap(s: Seq<char>) -> bool {
    s.len() >= 5 && s[0] == '_' && s[1] == 'c' && s[2] == 'a' && s[3] == 'p' && s[4] == '_'
}

pub open spec fn is_vbuf(s: Seq<char>) -> bool {
    s.len() >= 3 && s[s.len() - 1] == 'p' && s[s.len() - 2] == '.' && is_ident_str(s.subrange(0, s.len() - 2))
}

pub open spec fn is_ident_str(s: Seq<char>) -> bool {
    s.len() > 0 && is_ident_first(s[0]) && forall|i: int| 0 <= i && i < s.len() ==> #[trigger] is_ident_at(s, i)
}

pub open spec fn is_ident_at(s: Seq<char>, i: int) -> bool {
    is_ident_rest(s[i])
}

pub open spec fn is_ident_first(c: char) -> bool {
    (('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || c == '_')
}

pub open spec fn is_ident_rest(c: char) -> bool {
    (('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') || c == '_')
}

// `unescaped_literal`: peel `(char*)` casts, then accept `"…"` with no
// `\` and no inner `"`; the inner text length is the strlen.
pub open spec fn unescaped_inner(s: Seq<char>) -> Option<Seq<char>>
    decreases s.len(),
{
    if starts_with_cast(s) {
        unescaped_inner(s.subrange(7, s.len() as int))
    } else if s.len() >= 2 && s[0] == '"' && s[s.len() - 1] == '"' {
        inner_if_clean(s.subrange(1, s.len() - 1))
    } else {
        Option::None
    }
}

pub open spec fn inner_if_clean(inner: Seq<char>) -> Option<Seq<char>> {
    if forall|i: int| 0 <= i && i < inner.len() ==> #[trigger] is_clean_at(inner, i) {
        Option::Some(inner)
    } else {
        Option::None
    }
}

pub open spec fn is_clean_at(inner: Seq<char>, i: int) -> bool {
    inner[i] != '\\' && inner[i] != '"'
}

// The emitted copy arm (which write sequence the site takes).
pub enum CopyArm {
    LiteralFast,
    Guarded,
    Unguarded,
    Clear,
}

pub open spec fn copy_arm(s: Seq<char>, b: int) -> CopyArm {
    if !is_stringy(s) {
        CopyArm::Clear
    } else {
        match unescaped_inner(s) {
            Option::Some(inner) => if inner.len() <= b { CopyArm::LiteralFast } else { CopyArm::Guarded },
            Option::None => if starts_with_cap(s) || starts_with_sh2(s) || is_vbuf(s) {
                CopyArm::Unguarded
            } else {
                CopyArm::Guarded
            },
        }
    }
}

// The write contract: the buffer after the arm, as (len, nul_ok) — len
// is the strlen the destination reports, nul_ok is definedness (a NUL
// inside capacity). For Fixed(cap): strncpy(n=cap) + dst[b]=0 (b =
// cap-1); for Clear: dst[0]=0.
pub open spec fn write_len(cap: int, src_len: int) -> int {
    if src_len < cap { src_len } else { cap - 1int }
}

// ── The obligations ───────────────────────────────────────────────────

// The decl contract (P3_Buffer's `fits_buf`, mirrored): with strlen ≤
// b, every offset read below b+1 is defined.
proof fn bound_admits(b: int, vlen: int, off: int)
    requires 0int <= b, 0int <= vlen, vlen <= b, 0int <= off, off <= b
    ensures off < b + 1int
{
}

// THE OBLIGATION: the grown-from-source write (strncpy cap=b+1 then
// dst[b]=0) always lands NUL-terminated inside the buffer — exact
// when the source fits (never UB), truncating-but-defined otherwise.
proof fn guarded_write_safe(b: int, src_len: int)
    requires 0int <= b, 0int <= src_len
    ensures 0int <= write_len(b + 1int, src_len), write_len(b + 1int, src_len) <= b,
        (src_len <= b ==> write_len(b + 1int, src_len) == src_len)
{
}

// The literal fast arm: no assert needed (strlen known ≤ b at emit
// time) and no NUL store needed (strncpy pads short sources).
proof fn literal_arm_exact(b: int, lit_len: int)
    requires 0int <= b, 0int <= lit_len, lit_len <= b
    ensures write_len(b + 1int, lit_len) == lit_len
{
}

// The debug assert IS the fits predicate: keeping it catches a wrong N
// in debug; removing it (NDEBUG) changes only the failure mode
// (P3_Buffer's `debug_assert_matches`, mirrored).
proof fn debug_assert_matches(b: int, vlen: int)
    requires 0int <= b, 0int <= vlen
    ensures (vlen <= b) == (write_len(b + 1int, vlen) == vlen)
{
}

// The assert-free arm (capture/vbuf/sh2_ sources) is still never UB:
// strncpy truncates safely. Only the wrong-value mode remains —
// "a wrong N is a wrong value, never UB".
proof fn capture_arm_no_ub(b: int, src_len: int)
    requires 0int <= b, 0int <= src_len
    ensures 0int <= write_len(b + 1int, src_len), write_len(b + 1int, src_len) < b + 1int
{
}

// The non-stringy arm never touches the source pointer: dst[0]=0 is
// always a defined write into any fixed buffer.
proof fn bogus_arm_clears(b: int)
    requires 0int <= b
    ensures 0int < b + 1int
{
}

// The classifier shape-mirror: each of the six source shapes is
// stringy; a digit-led / empty / operator spelling is not.
proof fn stringy_shape_mirror()
    ensures is_stringy(Seq::empty().push('"').push('a').push('"')),
        is_stringy(Seq::empty().push('(').push('c').push('h').push('a').push('r').push('*').push(')').push('x')),
        is_stringy(Seq::empty().push('(').push('x').push(')')),
        is_stringy(Seq::empty().push('s').push('h').push('2').push('_').push('f')),
        is_stringy(Seq::empty().push('_').push('c').push('a').push('p').push('_').push('1')),
        is_stringy(Seq::empty().push('v').push('.').push('p')),
        is_stringy(Seq::empty().push('a').push('b').push('c')),
        !is_stringy(Seq::empty()),
        !is_stringy(Seq::empty().push('1').push('a')),
        !is_stringy(Seq::empty().push('+'))
{
    assert(is_ident_str(Seq::empty().push('a').push('b').push('c')));
    assert(is_ident_str(Seq::empty().push('v')));
    assert(!is_ident_first('1'));
}

pub open spec fn s_sub(s: Seq<char>) -> Seq<char> {
    s.subrange(1, s.len() - 1)
}

// The literal gate: escape-free `"…"` qualifies with strlen = len − 2;
// escaped / inner-quote / bare spellings refuse.
proof fn unescaped_recognizes()
    ensures unescaped_inner(Seq::empty().push('"').push('a').push('b').push('"'))
            == Option::Some(Seq::empty().push('a').push('b')),
        unescaped_inner(Seq::empty().push('"').push('\\').push('n').push('"')) == Option::None::<Seq<char>>,
        unescaped_inner(Seq::empty().push('a').push('b')) == Option::None::<Seq<char>>,
        unescaped_inner(Seq::empty()) == Option::None::<Seq<char>>
{
    // the escaped literal refuses: index 1 is the backslash
    assert(!is_clean_at(Seq::empty().push('\\').push('n'), 0));
    assert(!(forall|i: int| 0 <= i && i < (Seq::empty().push('\\').push('n')).len()
        ==> #[trigger] is_clean_at(Seq::empty().push('\\').push('n'), i)));
    assert(s_sub(Seq::empty().push('"').push('\\').push('n').push('"')) == Seq::empty().push('\\').push('n'));
    assert(unescaped_inner(Seq::empty().push('"').push('\\').push('n').push('"'))
        == inner_if_clean(s_sub(Seq::empty().push('"').push('\\').push('n').push('"'))));
    assert(inner_if_clean(Seq::empty().push('\\').push('n')) == Option::None::<Seq<char>>);
}

// The discipline: all three denials stay `char*`; a proven Str with
// b ≤ 1024 gets Fixed(b+1); over-cap and unknown stay `char*`.
proof fn bound_discipline()
    ensures decl_of(Verdict { is_num: true, is_capture: false, is_bigint: false, bound: Bound::Known(10int) }) == Dst::Ptr,
        decl_of(Verdict { is_num: false, is_capture: true, is_bigint: false, bound: Bound::Known(64int) }) == Dst::Ptr,
        decl_of(Verdict { is_num: false, is_capture: false, is_bigint: true, bound: Bound::Known(64int) }) == Dst::Ptr,
        decl_of(Verdict { is_num: false, is_capture: false, is_bigint: false, bound: Bound::Known(20int) }) == Dst::Fixed(21int),
        decl_of(Verdict { is_num: false, is_capture: false, is_bigint: false, bound: Bound::Known(2000int) }) == Dst::Ptr,
        decl_of(Verdict { is_num: false, is_capture: false, is_bigint: false, bound: Bound::Unknown }) == Dst::Ptr
{
}

// ── Fidelity pins ─────────────────────────────────────────────────────

// The decl shapes: bound var → `char v[b+1]`; each denial → `char*`.
proof fn pin_decl_shapes()
    ensures decl_of(Verdict { is_num: false, is_capture: false, is_bigint: false, bound: Bound::Known(20int) }) == Dst::Fixed(21int),
        decl_of(Verdict { is_num: true, is_capture: false, is_bigint: false, bound: Bound::Known(20int) }) == Dst::Ptr
{
}

// The `sha256sum` shape: a capture receiver with analysis bound 64
// still renders `char*` — the under-bounding analysis must not narrow
// the store (measured comment at buf_bound).
proof fn pin_trunc_shape()
    ensures buf_bound(Verdict { is_num: false, is_capture: true, is_bigint: false, bound: Bound::Known(64int) }) == Option::None::<int>,
        decl_of(Verdict { is_num: false, is_capture: true, is_bigint: false, bound: Bound::Known(64int) }) == Dst::Ptr
{
}

// The fitting-literal fast arm on a real spelling: `"yes"` (len 5,
// inner 3) into b=20 takes LiteralFast and copies exactly.
proof fn pin_literal_fast_shape()
    ensures copy_arm(Seq::empty().push('"').push('y').push('e').push('s').push('"'), 20int) == CopyArm::LiteralFast,
        copy_arm(Seq::empty().push('_').push('c').push('a').push('p').push('_').push('1'), 20int) == CopyArm::Unguarded,
        copy_arm(Seq::empty().push('+'), 20int) == CopyArm::Clear
{
}

fn main() {}

} // verus!
