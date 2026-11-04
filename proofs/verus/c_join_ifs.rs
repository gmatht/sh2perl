// Standalone model of the C backend's IFS JOINS — the runtime separator
// read behind `${a[*]}` and the ELEMENT slice behind `${a[*]:off:len}`
// (sh2perl/src/c_backend.rs: `_sh_join_arr_sep`, `_sh_join_arr_rng`,
//  the `star` dispatch in `param_call`'s slice arm, `_sh_join_arr` for
//  the `[@]` face, and `emit_var_decl`'s IFS const-lift VETO — all
//  measured red before 2026-10-30's fixes: 064_07's `sort` fed one
//  space-joined line, probe c09 printed the assigned IFS before its
//  assignment, probes c10/c16 lost `${a[*]:off:len}` entirely).
//
// The `[*]` FACE reaches the renderer in the name (`param("slice",
// "a[*]", off, len)`): the parser used to drop `:off:len` from
// `${a[*]:off:len}` altogether, and once it stopped dropping it the
// star slice needed a shape the `sep` dispatch could tell apart from
// the `[@]` one — without the suffix both faces rendered the space
// join and probe c05 (`IFS=:; ${a[*]:1:2}` → `y:z`) stayed red.
// `harness/ifs-probe.sh` checks every face against real bash.
//
// THE DENOTATIONAL EXTENSION (this file): bash's scalar `${a[*]}` is
// ONE word built by joining the array's ELEMENTS with a separator
// derived from IFS, and `${a[*]:off:len}` slices that ELEMENT sequence
// before joining. Three faces and one state:
//
//   ifs_sep(ifs)  the FIRST char of IFS; `IFS=""` → NO separator;
//                 an IFS bash never had is the decl's startup SPACE
//                 (bash resets inherited IFS at startup) — which is
//                 exactly why the const-lift is vetoed for IFS: the
//                 decl initialiser must be the startup state, never
//                 the first assignment's literal (probe c09).
//   present(slots) the element words in index order, holes dropped
//                 (None slot = unset; a set-but-EMPTY word survives —
//                 `_f` tracks presence, not emptiness)
//   star_join     join_sep(ifs_sep(ifs), present)   — `${a[*]}`  (c01-c07)
//   at_join       join_sep(Some(' '), present)      — `${a[@]}`  (c08/c17:
//                 the `[@]` face re-joins with a SPACE under ANY IFS —
//                 measured; it never reads IFS)
//   rng_slice     presence-ORDER slice: bounds are computed over the
//                 element count (bash ground truth: `a=(a b c);
//                 a[5]=z; ${a[*]:1:3}` → `b c z`, MEASURED 2026-10-30
//                 — offset/length count ELEMENTS, holes skipped), then
//                 the words in [b,e) are taken from `present`
//
// Obligations (each maps to a probe in the 29-case battery, or to a
// named mutant of the retired rendering):
//   sep_*                 the IFS read: startup space, "" → none, first char only
//   c01..c08, c06/c07     the join faces (default / newline / colon /
//                         empty / multichar / holes / empty element / @)
//   at_face_ignores_ifs   the `[@]` face never depends on IFS
//   decl_ifs_veto         emit_var_decl never const-lifts IFS (the fix)
//   c09_pre/post_assign   the measured two-line scenario (line 1 space,
//                         line 2 colon)
//   rng_* / c16..c28      the slice arithmetic and every measured slice
//                         (default, offset-only, negative offset, colon,
//                         empty IFS, empty element, holes, assoc, int
//                         array, @-face empty lead, @-face embedded
//                         space, holes with offset, @-face under IFS)
//   c29..c32              the OUTCOME layer: offset past the end ->
//                         empty value; in-range negative length -> Err;
//                         the GUARD ORDER (out-of-range beats len<0);
//                         an empty array never errors
//   rng_bounds_matches_   the model's clamp == the emitted arithmetic
//   source                on the path the helper takes
//   m1..m6                mutants: hardcoded space separator, the
//                         retired join-then-space-split slice, the
//                         const-lift without the veto, the `[@]` face
//                         routed through IFS, DENSE-index slicing
//                         (holes counted as empty words), and the
//                         retired negative-length clamp (a value where
//                         bash exits 1)
//
// THE OUTCOME LAYER (added 2026-11-02, MEASURED): bash's negative
// LENGTH is not a string at all — it is an expansion ERROR. The guards
// run in the order the emitted helper evaluates them:
//
//   1. offset out of range -> EMPTY VALUE (tested FIRST, so
//      `${a[*]:7:-2}` is "" with rc 0, NOT an error)
//   2. len < 0             -> Err (`substring expression < 0`, rc 1);
//      an EMPTY array never reaches this arm (cnt=0 fails test 1)
//   3. otherwise           -> the join of present[b, min(b+len, cnt))
//
// `SliceOut` is that trichotomy; `rng_out`/`slice_out` are the
// denotations of the emitted helper's control flow, and
// `rng_bounds_matches_source` is the bridge that lets every value-face
// obligation (c16..c28) keep stating the value alone: on the path the
// helper actually takes, this model's clamp and the emitted arithmetic
// are the same pair. The retired clamp counted the negative length back
// from the end (`cnt+len`), which produced a VALUE where bash exits 1 —
// m6 kills exactly that. Key-list
// slices (`${!m[*]:o:l}`) keep the pre-fix path (the parser declines
// the shape — a keys join then slice is the next increment). Dense
// VALUE storage (long long[]/mpz_t[]) keeps `_sh_arr_slice` too
// (`dense_value_storage` guard in the dispatch). Assoc storage is
// modeled as dense Seq (unset compacts `_v`/`_n`, no holes). The
// capacity guard (exit 127) and string ownership are outside the
// denotation like every other c_*.rs model.
use vstd::prelude::*;

verus! {

// vstd's broadcast lemma groups are NOT in scope by default; the
// take_n_all round-trip and the whole-slice short-circuit need the seq
// push/skip/subrange laws.
broadcast use {vstd::seq::group_seq_axioms, vstd::seq_lib::group_seq_lib_default};

pub type Str = Seq<char>;
pub type Slot = Option<Str>;

// ---------------------------------------------------------------------
// The elements: presence protocol (holes dropped, empty words kept)
// ---------------------------------------------------------------------

pub open spec fn present(slots: Seq<Slot>) -> Seq<Str>
    decreases slots.len()
{
    if slots.len() == 0 {
        Seq::empty()
    } else {
        let rest = present(slots.skip(1));
        match slots[0] {
            Some(v) => seq![v] + rest,
            None => rest,
        }
    }
}

// ---------------------------------------------------------------------
// The IFS read
// ---------------------------------------------------------------------

// bash: the separator is the FIRST char of IFS; an empty IFS joins with
// NOTHING. "IFS unset" is not a fourth case — the C decl initialises
// the buffer to the startup space (see decl_init), so an IFS the
// program never assigned reads ' ' from the store itself.
pub open spec fn ifs_sep(ifs: Str) -> Option<char> {
    if ifs.len() == 0 {
        None
    } else {
        Some(ifs[0])
    }
}

pub open spec fn sep_chars(s: Option<char>) -> Str {
    match s {
        None => Seq::empty(),
        Some(c) => seq![c],
    }
}

// The join fold: one separator BETWEEN words (a leading empty word
// still gets its separator — `a=("" x)` → " x"; a trailing empty word
// keeps its trailing one — `a=(a "")` → "a ").
pub open spec fn join_sep(s: Option<char>, xs: Seq<Str>) -> Str
    decreases xs.len()
{
    if xs.len() == 0 {
        Seq::empty()
    } else if xs.len() == 1 {
        xs[0]
    } else {
        xs[0] + sep_chars(s) + join_sep(s, xs.skip(1))
    }
}

// ---------------------------------------------------------------------
// The three faces at the dispatch (`star` selects the IFS read)
// ---------------------------------------------------------------------

pub open spec fn star_join(slots: Seq<Slot>, ifs: Str) -> Str {
    join_sep(ifs_sep(ifs), present(slots))
}

pub open spec fn at_join(slots: Seq<Slot>) -> Str {
    join_sep(Some(' '), present(slots))
}

pub open spec fn join_face(star: bool, slots: Seq<Slot>, ifs: Str) -> Str {
    if star {
        star_join(slots, ifs)
    } else {
        at_join(slots)
    }
}

// ---------------------------------------------------------------------
// The slice arithmetic — `_sh_join_arr_rng`'s b/e clamps, verbatim
// ---------------------------------------------------------------------

pub open spec fn rng_bounds(cnt: int, off: int, len: int) -> (int, int) {
    let b0 = if off < 0 {
        cnt + off
    } else {
        off
    };
    let b = if b0 < 0 {
        0
    } else if b0 > cnt {
        cnt
    } else {
        b0
    };
    let e0 = b + (if len < 0 {
        cnt + len
    } else {
        len
    });
    let e = if e0 > cnt {
        cnt
    } else if e0 < b {
        b
    } else {
        e0
    };
    (b, e)
}

// Subrange by recursion (NOT Seq::subrange): the model evaluates these
// calls by `compute` in every concrete obligation, and an axiom-backed
// subrange would not unfold.
pub open spec fn drop_n(xs: Seq<Str>, k: int) -> Seq<Str>
    decreases xs.len()
{
    if k <= 0 {
        xs
    } else if xs.len() == 0 {
        Seq::empty()
    } else {
        drop_n(xs.skip(1), k - 1)
    }
}

pub open spec fn take_n(xs: Seq<Str>, k: int) -> Seq<Str>
    decreases xs.len()
{
    if k <= 0 || xs.len() == 0 {
        Seq::empty()
    } else {
        seq![xs[0]] + take_n(xs.skip(1), k - 1)
    }
}

// `take(xs, b, e)` with the clamps already applied by rng_bounds; the
// guard-style fallback keeps the spec total (b,e are always in range
// for a rng_bounds output, and the lemmas below say so where needed).
pub open spec fn take(xs: Seq<Str>, b: int, e: int) -> Seq<Str> {
    if b < 0 || e < b {
        Seq::empty()
    } else {
        take_n(drop_n(xs, b), e - b)
    }
}

pub open spec fn rng_slice(slots: Seq<Slot>, off: int, len: int) -> Seq<Str> {
    let xs = present(slots);
    let (b, e) = rng_bounds(xs.len() as int, off, len);
    take(xs, b, e)
}

pub open spec fn star_slice(slots: Seq<Slot>, off: int, len: int, ifs: Str) -> Str {
    join_sep(ifs_sep(ifs), rng_slice(slots, off, len))
}

// The SEP the `[@]` SLICE passes — NOT the whole-array rule. MEASURED
// 2026-11-04 on real bash (`a=(x y z)`):
//   IFS=:   ${a[@]}=`x y z`  ${a[*]}=`x:y:z`  ${a[@]:1:2}=`y:z`
//   IFS=""  ${a[@]}=`x y z`  ${a[*]}=`xyz`    ${a[@]:1:2}=`y z`
// So the whole `[@]` is space-joined unconditionally (`at_join`), while
// the SLICE joins on IFS[0] and falls back to a space when IFS is
// empty — a multi-WORD expansion still has to be joined by something,
// and a null IFS falls back to the space. The renderer used `' '` for
// both, so `IFS=:; v="${a[@]:1:2}"` rendered `y z` (the retired xfail).
pub open spec fn slice_at_sep(ifs: Str) -> Option<char> {
    if (ifs.len() as int) == 0 {
        Some(' ')
    } else {
        Some(ifs[0])
    }
}

pub open spec fn at_slice(slots: Seq<Slot>, off: int, len: int, ifs: Str) -> Str {
    join_sep(slice_at_sep(ifs), rng_slice(slots, off, len))
}

// The dispatch: `star` selects the IFS read for the SLICE face too.
pub open spec fn slice_face(star: bool, slots: Seq<Slot>, off: int, len: int, ifs: Str) -> Str {
    if star {
        star_slice(slots, off, len, ifs)
    } else {
        at_slice(slots, off, len, ifs)
    }
}

// ---------------------------------------------------------------------
// The OUTCOME layer — `_sh_join_arr_rng`'s guards, in source order
// ---------------------------------------------------------------------

// An expansion that errors is not a string: bash exits 1 with no value.
pub enum SliceOut { Val(Str), Err }

// The offset arm as emitted: `off < 0` counts back from cnt, and a
// count that lands before the start is the -1 OUT sentinel — never a
// clamp to 0 (bash returns the EMPTY value for an out-of-range offset).
pub open spec fn rng_b(cnt: int, off: int) -> int {
    if off < 0 {
        if off < -cnt {
            -1int
        } else {
            cnt + off
        }
    } else {
        off
    }
}

// The helper's first test: `if (b < 0 || b >= cnt) -> empty`.
pub open spec fn rng_in(cnt: int, off: int) -> bool {
    let b = rng_b(cnt, off);
    0 <= b && b < cnt
}

// The value path's bounds, transcribed from the emitted arithmetic
// (`b` is in range and `len >= 0` here — the guards have already run):
// `e = len > cnt - b ? cnt : b + len`, written so the addition cannot
// overflow (the pre-fix `b + len` was a UBSAN-measured signed overflow
// at len = i64::MAX).
pub open spec fn rng_now(cnt: int, off: int, len: int) -> (int, int) {
    let b = rng_b(cnt, off);
    let e = if len > cnt - b {
        cnt
    } else {
        b + len
    };
    (b, e)
}

// The denotation of the emitted control flow, guard by guard.
pub open spec fn rng_out(slots: Seq<Slot>, off: int, len: int, ifs: Str) -> SliceOut {
    let xs = present(slots);
    if !rng_in(xs.len() as int, off) {
        SliceOut::Val(Seq::empty())
    } else if len < 0 {
        SliceOut::Err
    } else {
        SliceOut::Val(star_slice(slots, off, len, ifs))
    }
}

// Both faces reach the same helper (`sep` is the only difference), so
// the outcome is shared; `star` still picks the join for the value arm.
pub open spec fn slice_out(star: bool, slots: Seq<Slot>, off: int, len: int, ifs: Str) -> SliceOut {
    if star {
        rng_out(slots, off, len, ifs)
    } else {
        let xs = present(slots);
        if !rng_in(xs.len() as int, off) {
            SliceOut::Val(Seq::empty())
        } else if len < 0 {
            SliceOut::Err
        } else {
            SliceOut::Val(at_slice(slots, off, len, ifs))
        }
    }
}

// THE BRIDGE: on the path the helper actually takes (in-range offset,
// non-negative length) this model's clamp and the emitted arithmetic
// are the same (b, e) — so every value-face obligation above may keep
// stating `star_slice` alone.
pub open spec fn min2(a: int, b: int) -> int {
    if a < b {
        a
    } else {
        b
    }
}

proof fn rng_bounds_matches_source(cnt: int, off: int, len: int)
    requires cnt >= 0, rng_in(cnt, off), len >= 0
    ensures rng_bounds(cnt, off, len) == rng_now(cnt, off, len)
{
    let bnow = rng_b(cnt, off);
    assert(bnow >= 0 && bnow < cnt);
    if off < 0 {
        // the -1 arm is excluded by rng_in, so rng_b took cnt + off
        assert(bnow == cnt + off);
        assert(off >= 0 - cnt);
    } else {
        assert(bnow == off);
    }
    let b0 = if off < 0 { cnt + off } else { off };
    assert(b0 == bnow);
    assert(0 <= b0 && b0 <= cnt);
    // rng_bounds' clamp is the identity at b0, and its length arm is
    // `len` itself (len >= 0), so e0 = b0 + len and the e-clamp keeps
    // min(b0 + len, cnt).
    assert(rng_bounds(cnt, off, len) == (b0, min2(b0 + len, cnt)));
    // rng_now: `len > cnt - b0` decides the same min (b0 <= cnt).
    assert((b0 + len > cnt) == (len > cnt - b0));
    assert(rng_now(cnt, off, len) == (bnow, if len > cnt - b0 { cnt } else { b0 + len }));
}

// The whole-slice short-circuit (`BASHC_OPT item 28`): off missing/0
// with len missing returns the JOIN directly — rng_whole_is_all below
// is exactly the statement that this returns the same value the range
// helper would have.
pub open spec fn big() -> int {
    1152921504606846976 // 1LL<<60, the emitted no-length sentinel
}

// ---------------------------------------------------------------------
// The declaration state: the IFS const-lift VETO
// ---------------------------------------------------------------------

// emit_var_decl: `let ok = ok && v != "IFS";` — the const-lift branch
// (which bakes the literal AND suppresses the assignment statement) is
// never taken for IFS, so the decl takes the buf branch whose init is
// bash's startup state. (For every other var the const-lift keeps its
// old meaning — the assignment IS the declaration initialiser.)
pub open spec fn decl_init(v: Str, first_assign: Str) -> Str {
    if v == seq!['I', 'F', 'S'] {
        seq![' ']
    } else {
        first_assign
    }
}

// The mutant: the pre-fix behaviour (no veto — the literal lands in
// the declaration at time 0).
pub open spec fn decl_init_mut(v: Str, first_assign: Str) -> Str {
    first_assign
}

// ---------------------------------------------------------------------
// The RETIRED route, transcribed: join first, then split on a SPACE
// (`_sh_arr_slice` over the joined string — what every array slice used
// to render through). Kept here only so m2 can name what it replaces.
// ---------------------------------------------------------------------

pub open spec fn skip_sp(s: Str) -> Str
    decreases s.len()
{
    if s.len() > 0 && s[0] == ' ' {
        skip_sp(s.skip(1))
    } else {
        s
    }
}

// Fuel-carrying: Verus cannot see that the tuple-returning `word_upto`'s
// remainder is a strict suffix, so the measure rides explicitly (the
// callers pass `len + 1` — a word count never exceeds the byte count).
pub open spec fn word_upto(s: Str, fuel: int) -> (Str, Str)
    decreases fuel
{
    if fuel <= 0 || s.len() == 0 || s[0] == ' ' {
        (Seq::empty(), s)
    } else {
        let (w, r) = word_upto(s.skip(1), fuel - 1);
        (seq![s[0]] + w, r)
    }
}

pub open spec fn space_words(s: Str, fuel: int) -> Seq<Str>
    decreases fuel
{
    if fuel <= 0 {
        Seq::empty()
    } else {
        let t = skip_sp(s);
        if t.len() == 0 {
            Seq::empty()
        } else {
            let (w, r) = word_upto(t, fuel);
            seq![w] + space_words(r, fuel - 1)
        }
    }
}

pub open spec fn space_slice(s: Str, off: int, len: int) -> Str {
    let ws = space_words(s, s.len() as int + 1);
    let (b, e) = rng_bounds(ws.len() as int, off, len);
    join_sep(Some(' '), take(ws, b, e))
}

// ---------------------------------------------------------------------
// The DENSE-index mutant (m5): holes counted as EMPTY words and the
// bounds taken over the slot count instead of the element count.
// ---------------------------------------------------------------------

pub open spec fn densify(slots: Seq<Slot>) -> Seq<Str>
    decreases slots.len()
{
    if slots.len() == 0 {
        Seq::empty()
    } else {
        let rest = densify(slots.skip(1));
        match slots[0] {
            Some(v) => seq![v] + rest,
            None => seq![Seq::empty()] + rest,
        }
    }
}

pub open spec fn dense_slice(slots: Seq<Slot>, off: int, len: int) -> Seq<Str> {
    let ds = densify(slots);
    let (b, e) = rng_bounds(ds.len() as int, off, len);
    take(ds, b, e)
}

// ---------------------------------------------------------------------
// The DENSE-VALUE join (`declare -ia a=(1 2 3)`)
// ---------------------------------------------------------------------
//
// Value storage (`long long[]` / the analysed narrow width / `mpz_t[]`)
// carries NO presence marker, so the bash-visible element list is the
// WHOLE EXTENT: `densify`'s list, holes rendered as the value a zeroed
// slot holds. That is the documented edge, and it is why this join is a
// DIFFERENT function from `star_join`/`at_join` — those take
// `present(slots)` (char** storage, where NULL really is "unset").
//
// The separator discipline is shared with the char** fold
// (`join_sep`): the rendered loop is that same fold, applied to the
// densified list. `dense_join` names the function, and `d7` proves the
// two agree, so the ONE separator invariant (no leading separator, no
// trailing one, `sep == None` concatenates) is stated once and holds
// for both element protocols.
//
// SCOPE, stated plainly: the defect this replaced was a C TYPE
// confusion (`_sh_join_arr_sep(char **a, ...)` handed a `long long[]`,
// reading slot 0 as a pointer — rc 139). Verus cannot see C types;
// what it can state is the value obligation that makes the confusion
// WRONG rather than merely ill-typed (`d4`: the dense list and the
// presence list of the same array differ), which is the fact a future
// refactor must not erase. The type side is the C gate's, and it is a
// RUNTIME check: on the gate's own compiler (GCC 13.3, MEASURED
// 2026-11-04) the old shape is only a `-Wincompatible-pointer-types`
// WARNING, so it compiled and then died (rc 139); GCC 14+ promotes that
// diagnostic to an error, and the probe battery is what catches it
// here.
pub open spec fn dense_join(sep: Option<char>, xs: Seq<Str>) -> Str {
    join_sep(sep, xs)
}

// The face at the dispatch, over dense elements: `[*]` on IFS[0]
// (empty IFS concatenates), `[@]` on a space whatever IFS says.
pub open spec fn dense_face(star: bool, xs: Seq<Str>, ifs: Str) -> Str {
    if star {
        dense_join(ifs_sep(ifs), xs)
    } else {
        dense_join(Some(' '), xs)
    }
}

// ---------------------------------------------------------------------
// The PRESENCE marker: dense storage made exact (`{id}_set[]`)
// ---------------------------------------------------------------------
//
// The extent protocol above is the pre-2026-11-04 approximation. Bash
// tracks which elements are SET, and a value-typed slot cannot say
// "unset" (a zeroed slot is the integer 0), so a fixed dense array now
// carries a parallel byte-per-slot marker and the element list becomes
// `present` again — the SAME list the char** faces use.
//
// That is the whole content of the fix, and it is provable: with the
// marker, all four dense faces ARE the char** faces (`e4`). The densified
// list is kept as the vacuous-measurement mutant: on a sparse array it
// provably disagrees with the marked one (`e7`), which is exactly the
// divergence the marker removed.
pub open spec fn dense_marked_join(sep: Option<char>, slots: Seq<Slot>) -> Str {
    join_sep(sep, present(slots))
}

pub open spec fn dense_marked_face(star: bool, slots: Seq<Slot>, ifs: Str) -> Str {
    if star {
        dense_marked_join(ifs_sep(ifs), slots)
    } else {
        dense_marked_join(Some(' '), slots)
    }
}

// `${#arr[@]}` — the marker's popcount IS the element count.
pub open spec fn dense_count(slots: Seq<Slot>) -> int {
    present(slots).len() as int
}

// `${arr[i]}` — a present slot yields its value; an unset one is EMPTY
// (never "0", which is what the extent protocol prints).
pub open spec fn dense_get(slots: Seq<Slot>, i: int) -> Str {
    if i >= 0 && i < (slots.len() as int) {
        match slots[i] {
            Some(v) => v,
            None => Seq::empty(),
        }
    } else {
        Seq::empty()
    }
}

// `unset 'arr[i]'` — clear the slot (the renderer zeroes the value AND
// the marker byte).
pub open spec fn dense_clear(slots: Seq<Slot>, i: int) -> Seq<Slot> {
    if i >= 0 && i < (slots.len() as int) {
        Seq::new(slots.len(), |j: int| if j == i { None } else { slots[j] })
    } else {
        slots
    }
}

// ---------------------------------------------------------------------
// `arr+=(…)` — WHERE the append lands
// ---------------------------------------------------------------------
//
// Bash appends one past the LAST SET element, not one past the stored
// extent: `_len` (max index + 1) happens to agree for a dense array but
// not after an `unset` empties the tail. MEASURED 2026-11-04:
//
//   d=(1); d[3]=9; d+=(5)      -> the append lands at 4 (bash `1 9 5`,
//                                 keys `0 3 4`)
//   i=(5); unset 'i[0]'; i+=(6)-> the array is EMPTY, so the append lands
//                                 at 0 (bash `6`, keys `0`) while `_len`
//                                 is still 1
//
// So the renderer scans back for the last present slot instead of using
// `_len`, and the extent becomes max(base + n, _len) rather than `_len
// + n`. The `last_set` scan is written as an index recursion because the
// obligations below are discharged by `compute` (an axiom-backed
// `drop_last` would not unfold).
pub open spec fn last_set(slots: Seq<Slot>, i: int) -> int
    decreases i + 1
{
    if i < 0 {
        -1
    } else if i < (slots.len() as int) && slots[i].is_Some() {
        i
    } else {
        last_set(slots, i - 1)
    }
}

pub open spec fn append_base(slots: Seq<Slot>) -> int {
    last_set(slots, (slots.len() as int) - 1) + 1
}

// the element count after appending `k` values at the base
pub open spec fn append_count(slots: Seq<Slot>, k: int) -> int {
    dense_count(slots) + k
}

// `${!arr[*]}` — the SET indices, ascending (the key list).
pub open spec fn present_idx(slots: Seq<Slot>) -> Seq<int>
    decreases slots.len()
{
    if slots.len() == 0 {
        Seq::empty()
    } else if slots[0].is_Some() {
        seq![0int] + shift_idx(present_idx(slots.skip(1)))
    } else {
        shift_idx(present_idx(slots.skip(1)))
    }
}

pub open spec fn shift_idx(ix: Seq<int>) -> Seq<int> {
    Seq::new(ix.len(), |j: int| ix[j] + 1)
}

// ---------------------------------------------------------------------
// Measured scenarios (the 29-case probe battery, /tmp/probe_ifs/run.sh)
// ---------------------------------------------------------------------

pub open spec fn startup() -> Str { seq![' '] }
pub open spec fn ifs_nl() -> Str { seq!['\n'] }
pub open spec fn ifs_colon() -> Str { seq![':'] }
pub open spec fn ifs_colon_comma() -> Str { seq![':', ','] }
pub open spec fn ifs_var() -> Str { seq!['I', 'F', 'S'] }

// The join faces take SLOTS (presence), so the word scenarios wrap:
// `Some(w)` at every index — no holes.
pub open spec fn xyz() -> Seq<Slot> {
    seq![Some(seq!['x']), Some(seq!['y']), Some(seq!['z'])]
}
pub open spec fn bac() -> Seq<Slot> {
    seq![Some(seq!['b']), Some(seq!['a']), Some(seq!['c'])]
}
pub open spec fn abc123() -> Seq<Slot> {
    seq![Some(seq!['1']), Some(seq!['2']), Some(seq!['3'])]
}

pub open spec fn hole_vec() -> Seq<Slot> {
    seq![
        Some(seq!['a']),
        Some(seq!['b']),
        Some(seq!['c']),
        None,
        None,
        Some(seq!['z']),
    ]
}

pub open spec fn empty_lead() -> Seq<Slot> {
    seq![Some(Seq::empty()), Some(seq!['x']), Some(seq!['y'])]
}

pub open spec fn space_elem() -> Seq<Slot> {
    seq![Some(seq!['x', ' ', 'y']), Some(seq!['z'])]
}

pub open spec fn empty_mid() -> Seq<Slot> {
    seq![Some(seq!['a']), Some(Seq::empty()), Some(seq!['c'])]
}

pub open spec fn assoc_vals() -> Seq<Slot> {
    seq![Some(seq!['v', '1']), Some(seq!['v', '2'])]
}

// Dense-value scenarios: the ELEMENT LIST itself (no slots).
pub open spec fn digits123() -> Seq<Str> {
    seq![seq!['1'], seq!['2'], seq!['3']]
}

// `declare -ia n; n[0]=7; n[2]=9` — dense storage holds three slots
// (`7`, a zeroed `0`, `9`); bash's presence view holds two.
pub open spec fn dense709() -> Seq<Str> {
    seq![seq!['7'], seq!['0'], seq!['9']]
}
pub open spec fn present79() -> Seq<Str> {
    seq![seq!['7'], seq!['9']]
}
pub open spec fn empty_ifs() -> Str { Seq::empty() }

// The sparse dense-array scenario, as SLOTS: `n[0]=7; n[2]=9` — the hole
// at index 1 is what the value store cannot express without a marker.
pub open spec fn sparse7h9() -> Seq<Slot> {
    seq![Some(seq!['7']), None, Some(seq!['9'])]
}

// ---------------------------------------------------------------------
// Lemmas
// ---------------------------------------------------------------------

// take_n over the full length is the identity — the whole-slice
// short-circuit (rng_whole_is_all) and every clamp-to-end obligation
// reduce to this.
proof fn take_n_all(xs: Seq<Str>)
    ensures take_n(xs, xs.len() as int) == xs,
    decreases xs.len()
{
    if xs.len() == 0 {
        // take_n(_, 0) == Seq::empty() == xs
    } else {
        take_n_all(xs.skip(1));
        assert(take_n(xs.skip(1), xs.len() as int - 1) == xs.skip(1));
        // seq![xs[0]] + xs.skip(1) == xs, via the subrange laws:
        // xs.subrange(0,1) == seq![xs[0]] and concat over [0,1)+[1,len)
        assert(xs.subrange(0, 1) == seq![xs[0]]);
        assert(xs.subrange(0, 1) + xs.subrange(1, xs.len() as int) == xs);
        assert(xs.skip(1) == xs.subrange(1, xs.len() as int));
    }
}

// The emitted short-circuit: a whole slice (off 0, len the sentinel)
// returns the JOIN — which equals what the range helper would build.
// (requires the sentinel to exceed the element count; every real store
// is far below 1LL<<60.)
proof fn rng_whole_is_all(slots: Seq<Slot>)
    requires
        present(slots).len() as int <= big(),
    ensures
        rng_slice(slots, 0, big()) == present(slots),
{
    let xs = present(slots);
    let (b, e) = rng_bounds(xs.len() as int, 0, big());
    assert(b == 0);
    assert(e == xs.len() as int);
    take_n_all(xs);
    assert(drop_n(xs, 0) == xs);
    assert(take(xs, 0, xs.len() as int) == xs);
}

// ---------------------------------------------------------------------
// The IFS read — the three cases and the join faces
// ---------------------------------------------------------------------

proof fn sep_startup()
    ensures ifs_sep(startup()) == Some(' ')
{ assert(ifs_sep(startup()) == Some(' ')); }

proof fn sep_empty_ifs()
    ensures ifs_sep(Seq::empty()) == None
{ assert(ifs_sep(Seq::empty()) == None); }

proof fn sep_first_char_only()
    ensures ifs_sep(ifs_colon_comma()) == Some(':')
{ assert(ifs_sep(ifs_colon_comma()) == Some(':')); }

// c01: `a=(x y z); echo "[${a[*]}]"` — never-assigned IFS reads the
// decl's startup space.
proof fn c01_default_space_join()
    ensures star_join(xyz(), startup()) == seq!['x', ' ', 'y', ' ', 'z']
{ assert(star_join(xyz(), startup()) == seq!['x', ' ', 'y', ' ', 'z']) by (compute); }

proof fn c02_newline_join()
    ensures star_join(xyz(), ifs_nl()) == seq!['x', '\n', 'y', '\n', 'z']
{ assert(star_join(xyz(), ifs_nl()) == seq!['x', '\n', 'y', '\n', 'z']) by (compute); }

proof fn c03_colon_join()
    ensures star_join(xyz(), ifs_colon()) == seq!['x', ':', 'y', ':', 'z']
{ assert(star_join(xyz(), ifs_colon()) == seq!['x', ':', 'y', ':', 'z']) by (compute); }

// c04: `IFS=""; a=(x y z); echo "[${a[*]}]"` → `xyz` (NO separator).
proof fn c04_empty_ifs_join()
    ensures star_join(xyz(), Seq::empty()) == seq!['x', 'y', 'z']
{ assert(star_join(xyz(), Seq::empty()) == seq!['x', 'y', 'z']) by (compute); }

// c05: `IFS=':,'` joins on ':' only — bash reads ONE char.
proof fn c05_multichar_first_char()
    ensures star_join(xyz(), ifs_colon_comma()) == seq!['x', ':', 'y', ':', 'z']
{ assert(star_join(xyz(), ifs_colon_comma()) == seq!['x', ':', 'y', ':', 'z']) by (compute); }

// c06: holes vanish under the IFS join too (presence protocol, not
// index protocol: `a=(a b c); a[5]=z` → `a\nb\nc\nz`).
proof fn c06_holes_newline_join()
    ensures star_join(hole_vec(), ifs_nl()) == seq!['a', '\n', 'b', '\n', 'c', '\n', 'z']
{ assert(star_join(hole_vec(), ifs_nl()) == seq!['a', '\n', 'b', '\n', 'c', '\n', 'z']) by (compute); }

// c07: a set-but-EMPTY element keeps its separators (`a=(a "" c)` →
// `a\n\nc`) — `_f` tracks presence, not emptiness.
proof fn c07_empty_middle_join()
    ensures star_join(empty_mid(), ifs_nl()) == seq!['a', '\n', '\n', 'c']
{ assert(star_join(empty_mid(), ifs_nl()) == seq!['a', '\n', '\n', 'c']) by (compute); }

// c08: the `[@]` face is SPACE under any IFS.
proof fn c08_at_face_space_join()
    ensures at_join(xyz()) == seq!['x', ' ', 'y', ' ', 'z']
{ assert(at_join(xyz()) == seq!['x', ' ', 'y', ' ', 'z']) by (compute); }

proof fn at_face_ignores_ifs(slots: Seq<Slot>, ifs1: Str, ifs2: Str)
    ensures join_face(false, slots, ifs1) == join_face(false, slots, ifs2)
{ // definitional: join_face(false, ..) never mentions ifs
}

// ---------------------------------------------------------------------
// The decl state: the const-lift veto (probe c09)
// ---------------------------------------------------------------------

proof fn decl_ifs_veto()
    ensures decl_init(ifs_var(), ifs_colon()) == startup()
{ assert(decl_init(ifs_var(), ifs_colon()) == startup()); }

// c09 line 1: `a=(x y z); echo "[${a[*]}"` BEFORE any assignment reads
// the startup space (bash: `x y z`).
proof fn c09_pre_assign_join()
    ensures star_join(xyz(), decl_init(ifs_var(), ifs_colon())) == seq!['x', ' ', 'y', ' ', 'z']
{ assert(star_join(xyz(), decl_init(ifs_var(), ifs_colon())) == seq!['x', ' ', 'y', ' ', 'z']) by (compute); }

// c09 line 2: after `IFS=:` the same join reads the colon.
proof fn c09_post_assign_join()
    ensures star_join(xyz(), ifs_colon()) == seq!['x', ':', 'y', ':', 'z']
{ assert(star_join(xyz(), ifs_colon()) == seq!['x', ':', 'y', ':', 'z']) by (compute); }

// ---------------------------------------------------------------------
// The slice arithmetic
// ---------------------------------------------------------------------

// The clamps, at the c16 numbers (a=(b a c), ${a[*]:1:2} → (1,3)).
proof fn rng_bounds_identity()
    ensures rng_bounds(3, 1, 2) == (1, 3)
{ assert(rng_bounds(3, 1, 2) == (1, 3)) by (compute); }

// c16: default IFS, offset+length.
proof fn c16_star_slice_default()
    ensures star_slice(bac(), 1, 2, startup()) == seq!['a', ' ', 'c']
{ assert(star_slice(bac(), 1, 2, startup()) == seq!['a', ' ', 'c']) by (compute); }

// c18: offset only (`${a[*]:2}` → the tail).
proof fn c18_star_slice_off_only()
    ensures star_slice(bac(), 2, big(), startup()) == seq!['c']
{ assert(star_slice(bac(), 2, big(), startup()) == seq!['c']) by (compute); }

// c19: negative offset counts back from the end (`${a[*]: -2}`).
proof fn c19_star_slice_neg_off()
    ensures star_slice(
            seq![Some(seq!['w']), Some(seq!['x']), Some(seq!['y']), Some(seq!['z'])],
            -2,
            big(),
            startup(),
        ) == seq!['y', ' ', 'z']
{ assert(
    star_slice(
        seq![Some(seq!['w']), Some(seq!['x']), Some(seq!['y']), Some(seq!['z'])],
        -2,
        big(),
        startup(),
    ) == seq!['y', ' ', 'z']
) by (compute); }

// c20: the slice joins on IFS like the whole face (`IFS=:; …:1:2` → `a:c`).
proof fn c20_star_slice_colon()
    ensures star_slice(bac(), 1, 2, ifs_colon()) == seq!['a', ':', 'c']
{ assert(star_slice(bac(), 1, 2, ifs_colon()) == seq!['a', ':', 'c']) by (compute); }

// c21: `IFS=""; …:1:2` → `ac` (no separator).
proof fn c21_star_slice_empty_ifs()
    ensures star_slice(bac(), 1, 2, Seq::empty()) == seq!['a', 'c']
{ assert(star_slice(bac(), 1, 2, Seq::empty()) == seq!['a', 'c']) by (compute); }

// c22: an EMPTY element inside the range keeps its separator
// (`IFS=$'\n'; a=(a "" c); ${a[*]:0:2}` → `a\n`).
proof fn c22_star_slice_empty_elem()
    ensures star_slice(empty_mid(), 0, 2, ifs_nl()) == seq!['a', '\n']
{ assert(star_slice(empty_mid(), 0, 2, ifs_nl()) == seq!['a', '\n']) by (compute); }

// c23: holes are skipped by ORDER — `a=(a b c); a[5]=z;
// ${a[*]:1:3}` → `b\nc\nz` (MEASURED: the bounds count elements).
proof fn c23_star_slice_holes()
    ensures star_slice(hole_vec(), 1, 3, ifs_nl()) == seq!['b', '\n', 'c', '\n', 'z']
{ assert(star_slice(hole_vec(), 1, 3, ifs_nl()) == seq!['b', '\n', 'c', '\n', 'z']) by (compute); }

// c24: assoc values are dense — the range helper runs over `_v/_n`
// (`${m[*]:0:1}` → `v1`).
proof fn c24_assoc_star_slice()
    ensures star_slice(assoc_vals(), 0, 1, ifs_nl()) == seq!['v', '1']
{ assert(star_slice(assoc_vals(), 0, 1, ifs_nl()) == seq!['v', '1']) by (compute); }

// c25: a numeric-literal array is char** storage too
// (`a=(1 2 3); ${a[*]:1:2}` → `2 3`).
proof fn c25_intarr_star_slice()
    ensures star_slice(abc123(), 1, 2, startup()) == seq!['2', ' ', '3']
{ assert(star_slice(abc123(), 1, 2, startup()) == seq!['2', ' ', '3']) by (compute); }

// c26: the `[@]` face keeps a set-but-EMPTY leading element
// (`a=("" x y); ${a[@]:0:2}` → ` x`) — the retired space-split
// skipped the leading space run and rendered `[x y]`.
proof fn c26_at_slice_empty_lead()
    ensures at_slice(empty_lead(), 0, 2, startup()) == seq![' ', 'x']
{ assert(at_slice(empty_lead(), 0, 2, startup()) == seq![' ', 'x']) by (compute); }

// c27: an element CONTAINING a space stays one element
// (`a=("x y" z); ${a[@]:1:1}` → `z`, not `y`).
proof fn c27_at_slice_space_elem()
    requires present(space_elem()).len() == 2,
    ensures at_slice(space_elem(), 1, 1, startup()) == seq!['z']
{ assert(at_slice(space_elem(), 1, 1, startup()) == seq!['z']) by (compute); }

// c28: `${a[*]:3}` over the hole vector → `z` (element 3 in order).
proof fn c28_star_slice_holes_off()
    ensures star_slice(hole_vec(), 3, big(), ifs_nl()) == seq!['z']
{ assert(star_slice(hole_vec(), 3, big(), ifs_nl()) == seq!['z']) by (compute); }

// c17 (REVISED 2026-11-04 — this obligation used to claim a SPACE):
// the `[@]` SLICE joins on IFS[0], so under IFS=$'\n' the join is a
// newline (`IFS=$'\n'; a=(a b c); v="${a[@]:1:2}"` → `a\nc`). The
// pre-fix renderer hard-coded `' '` here; the whole-array `[@]` does
// stay space-joined (c35).
proof fn c17_at_slice_under_newline_ifs()
    ensures slice_face(false, bac(), 1, 2, ifs_nl()) == seq!['a', '\n', 'c']
{ assert(slice_face(false, bac(), 1, 2, ifs_nl()) == seq!['a', '\n', 'c']) by (compute); }

// c33 (NEW): the reported defect itself — `a=(a b c); IFS=:;
// v="${a[@]:1:2}"` is `a:c`; the pre-fix arm rendered `a c` (this was
// ifs-probe case 21, an xfail until the sep was threaded).
proof fn c33_at_slice_colon()
    ensures slice_face(false, bac(), 1, 2, ifs_colon()) == seq!['a', ':', 'c']
{ assert(slice_face(false, bac(), 1, 2, ifs_colon()) == seq!['a', ':', 'c']) by (compute); }

// c34 (NEW): the SPACE FALLBACK — an empty IFS still joins the `[@]`
// slice with a space (`a c`), unlike the `[*]` slice, which
// concatenates (c21: `ac`).
proof fn c34_at_slice_empty_ifs_falls_back()
    ensures slice_face(false, bac(), 1, 2, Seq::empty()) == seq!['a', ' ', 'c']
{ assert(slice_face(false, bac(), 1, 2, Seq::empty()) == seq!['a', ' ', 'c']) by (compute); }

// c35 (NEW): the contrast that made the defect invisible — the WHOLE
// `[@]` expansion stays space-joined under a colon IFS, so a single
// `' '` literal served the whole face and looked right.
proof fn c35_at_whole_vs_slice_colon()
    ensures at_join(bac()) == seq!['b', ' ', 'a', ' ', 'c'],
        slice_face(false, bac(), 0, 3, ifs_colon()) == seq!['b', ':', 'a', ':', 'c'],
        slice_face(false, bac(), 0, 3, ifs_colon()) != at_join(bac())
{ assert(at_join(bac()) == seq!['b', ' ', 'a', ' ', 'c']) by (compute);
  assert(slice_face(false, bac(), 0, 3, ifs_colon())
    == seq!['b', ':', 'a', ':', 'c']) by (compute);
  assert(slice_face(false, bac(), 0, 3, ifs_colon()) != at_join(bac())) by (compute); }

// ---------------------------------------------------------------------
// The OUTCOME obligations — the guard order, measured 2026-11-01
// ---------------------------------------------------------------------

// c29 (probe 14): an offset past the end is the EMPTY VALUE —
// `a=(x y z); v="${a[*]:7:2}"` is "" in bash and in C, never a clamp.
pub open spec fn c29_out() -> SliceOut { rng_out(xyz(), 7int, 2int, startup()) }
proof fn c29_off_range_is_empty()
    ensures c29_out() == SliceOut::Val(Seq::empty())
{ assert(c29_out() == SliceOut::Val(Seq::empty())) by (compute); }

// c30 (probe 22, the promoted xfail): an in-range negative LENGTH is
// bash's expansion ERROR — `a=(a b c d); v="${a[@]:1:-2}"` → rc 1,
// `substring expression < 0`, no value. Both C arms used to accept it.
pub open spec fn c30_out() -> SliceOut { rng_out(bac(), 1int, 0int - 2int, startup()) }
proof fn c30_neg_len_is_error()
    ensures c30_out() == SliceOut::Err
{ assert(c30_out() == SliceOut::Err) by (compute); }

// c31: THE GUARD ORDER — the offset test runs first, so
// `${a[*]:7:-2}` is the empty VALUE (rc 0), NOT the error.
pub open spec fn c31_out() -> SliceOut { rng_out(xyz(), 7int, 0int - 2int, startup()) }
proof fn c31_neg_len_out_of_range_is_empty()
    ensures c31_out() == SliceOut::Val(Seq::empty())
{ assert(c31_out() == SliceOut::Val(Seq::empty())) by (compute); }

// c32: an EMPTY array never errors — cnt = 0 fails the offset test
// before the length test is ever reached (`${a[@]:0:-1}` → "").
pub open spec fn c32_out() -> SliceOut { rng_out(Seq::<Slot>::empty(), 0int, 0int - 1int, startup()) }
proof fn c32_empty_neg_len_is_empty()
    ensures c32_out() == SliceOut::Val(Seq::empty())
{ assert(c32_out() == SliceOut::Val(Seq::empty())) by (compute); }

// ---------------------------------------------------------------------
// Mutants — each kills exactly one obligation
// ---------------------------------------------------------------------

// m1: a hardcoded space separator (the pre-064_07 rendering) — the
// colon IFS would join `x y z`.
proof fn m1_hardcoded_space()
    ensures star_join(xyz(), ifs_colon()) != at_join(xyz())
{ assert(star_join(xyz(), ifs_colon()) != at_join(xyz())) by (compute); }

// m2: the retired route — join first, then element-slice by splitting
// on a SPACE (c10: IFS=$'\n' gives one word with no boundary, so the
// slice comes out EMPTY instead of `a\nc`).
proof fn m2_join_then_space_split()
    ensures space_slice(star_join(bac(), ifs_nl()), 1, 2) != star_slice(bac(), 1, 2, ifs_nl())
{ assert(
    space_slice(star_join(bac(), ifs_nl()), 1, 2) != star_slice(bac(), 1, 2, ifs_nl())
) by (compute); }

// m3: the const-lift without the veto — the assigned literal lands in
// the decl at time 0, so c09's FIRST line joins on ':' (bash: space).
proof fn m3_const_lift_baked()
    ensures star_join(xyz(), decl_init_mut(ifs_var(), ifs_colon()))
        != star_join(xyz(), decl_init(ifs_var(), ifs_colon()))
{ assert(
    star_join(xyz(), decl_init_mut(ifs_var(), ifs_colon()))
        != star_join(xyz(), decl_init(ifs_var(), ifs_colon()))
) by (compute); }

// m4: routing the `[@]` face through the IFS read — c08's words would
// come out newline-joined.
proof fn m4_at_face_on_ifs()
    ensures star_join(xyz(), ifs_nl()) != join_face(false, xyz(), ifs_nl())
{ assert(star_join(xyz(), ifs_nl()) != join_face(false, xyz(), ifs_nl())) by (compute); }

// m5: DENSE-index slicing — holes counted as empty words in the slot
// range (c28: `${a[*]:3}` would render `\n\nz` instead of `z`).
proof fn m5_dense_index()
    ensures join_sep(ifs_sep(ifs_nl()), dense_slice(hole_vec(), 3, big()))
        != star_slice(hole_vec(), 3, big(), ifs_nl())
{ assert(
    join_sep(ifs_sep(ifs_nl()), dense_slice(hole_vec(), 3, big()))
        != star_slice(hole_vec(), 3, big(), ifs_nl())
) by (compute); }

// m6: the RETIRED negative-length arithmetic (the old clamp counted
// back from the end, `cnt+len`) — it produced the VALUE "a" where bash
// exits 1. The outcome layer is what makes that difference visible.
proof fn m6_neg_len_clamped_value()
    ensures rng_out(bac(), 1int, 0int - 2int, startup())
        != SliceOut::Val(star_slice(bac(), 1int, 0int - 2int, startup()))
{ assert(
    rng_out(bac(), 1int, 0int - 2int, startup())
        != SliceOut::Val(star_slice(bac(), 1int, 0int - 2int, startup()))
) by (compute); }

// m7 (NEW 2026-11-04): the RETIRED `[@]`-slice rendering — a hard-coded
// space. It agrees with the fix on the default IFS (which is why no
// default-IFS example could catch it) and disagrees under every other
// non-empty IFS.
proof fn m7_retired_at_slice_space()
    ensures at_slice(bac(), 1, 2, ifs_colon()) != join_sep(Some(' '), rng_slice(bac(), 1, 2))
{ assert(
    at_slice(bac(), 1, 2, ifs_colon()) != join_sep(Some(' '), rng_slice(bac(), 1, 2))
) by (compute); }

// ── the dense-value join (2026-11-04) ───────────────────────────────
// d1: the DEFECT and its fix, as values. `declare -ia a=(1 2 3);
// echo "${a[*]}"` — bash `1 2 3`; the source emitted the char** fold
// on the integer store and died at the first element (measured rc 139).
// The obligation the fix must satisfy is that the dense face on the
// startup IFS is exactly the space join.
proof fn d1_dense_star_startup()
    ensures dense_face(true, digits123(), startup()) == seq!['1', ' ', '2', ' ', '3']
{ assert(
    dense_face(true, digits123(), startup()) == seq!['1', ' ', '2', ' ', '3']
) by (compute); }

// d2: the `[*]` face reads ONE char of IFS, and an EMPTY IFS joins with
// NOTHING (measured: `IFS=; echo "${a[*]}"` -> `123`). The old
// unconditional space would give `1 2 3` here.
proof fn d2_dense_star_empty_ifs()
    ensures dense_face(true, digits123(), empty_ifs()) == seq!['1', '2', '3']
{ assert(
    dense_face(true, digits123(), empty_ifs()) == seq!['1', '2', '3']
) by (compute); }

// d3: `[@]` is a SPACE whatever IFS says (measured: `IFS=:; echo
// "${a[@]}"` -> `1 2 3`), and the two faces therefore DISAGREE on the
// same array — the reason the separator is a runtime argument (IFS[0])
// and not a literal in the emitted loop.
proof fn d3_dense_at_is_space()
    ensures
        dense_face(false, digits123(), ifs_colon()) == seq!['1', ' ', '2', ' ', '3'],
        dense_face(true, digits123(), ifs_colon()) == seq!['1', ':', '2', ':', '3'],
        dense_face(false, digits123(), ifs_colon())
            != dense_face(true, digits123(), ifs_colon()),
{ assert(
    dense_face(false, digits123(), ifs_colon()) == seq!['1', ' ', '2', ' ', '3']
) by (compute);
  assert(
    dense_face(true, digits123(), ifs_colon()) == seq!['1', ':', '2', ':', '3']
) by (compute);
  assert(
    dense_face(false, digits123(), ifs_colon())
        != dense_face(true, digits123(), ifs_colon())
) by (compute); }

// d4: dense value storage and char** presence storage are NOT the same
// element list, so the one helper cannot serve both (this is the value
// half of the type confusion above; MEASURED `declare -ia n; n[0]=7;
// n[2]=9; echo "${#n[@]} ${n[*]}"` -> C `3 7 0 9`, bash `2 7 9`).
// A future refactor that "unifies" the two joins fails here.
proof fn d4_dense_is_not_presence()
    ensures
        dense_join(ifs_sep(ifs_colon()), dense709()) != join_sep(ifs_sep(ifs_colon()), present79()),
        dense_join(ifs_sep(ifs_colon()), dense709()) == seq!['7', ':', '0', ':', '9'],
        join_sep(ifs_sep(ifs_colon()), present79()) == seq!['7', ':', '9'],
{ assert(
    dense_join(ifs_sep(ifs_colon()), dense709()) == seq!['7', ':', '0', ':', '9']
) by (compute);
  assert(
    join_sep(ifs_sep(ifs_colon()), present79()) == seq!['7', ':', '9']
) by (compute);
  assert(
    dense_join(ifs_sep(ifs_colon()), dense709())
        != join_sep(ifs_sep(ifs_colon()), present79())
); }

// d5: the boundaries. Zero elements join to the EMPTY string (bash:
// `declare -ia a; echo "[${a[*]}]"` -> `[]`), one element keeps no
// separator, and an empty IFS on a single element is the element.
proof fn d5_dense_boundaries()
    ensures
        dense_face(true, Seq::<Str>::empty(), startup()) == Seq::<char>::empty(),
        dense_face(false, Seq::<Str>::empty(), ifs_colon()) == Seq::<char>::empty(),
        dense_join(ifs_sep(startup()), seq![seq!['4', '2']]) == seq!['4', '2'],
        dense_join(None, seq![seq!['4', '2']]) == seq!['4', '2'],
{ assert(
    dense_face(true, Seq::<Str>::empty(), startup()) == Seq::<char>::empty()
) by (compute);
  assert(
    dense_face(false, Seq::<Str>::empty(), ifs_colon()) == Seq::<char>::empty()
) by (compute);
  assert(dense_join(ifs_sep(startup()), seq![seq!['4', '2']]) == seq!['4', '2']) by (compute);
  assert(dense_join(None, seq![seq!['4', '2']]) == seq!['4', '2']) by (compute); }

// d6: the separator count — with `k` elements a separator appears exactly
// k-1 times, and `sep == None` inserts none. Stated on the total length,
// which is what the emitted loop's `if (_jdi)` guard buys: it writes the
// separator on every element EXCEPT the first, so no leading and no
// trailing byte can appear. (Symmetric to `d7`; together they say the
// renderer's guard is the fold and not an approximation of it.)
proof fn d6_dense_separator_count()
    ensures
        dense_join(Some(':'), digits123()).len()
            == digits123()[0].len() + digits123()[1].len() + digits123()[2].len() + 2,
        dense_join(None, digits123()).len()
            == digits123()[0].len() + digits123()[1].len() + digits123()[2].len(),
{ assert(
    dense_join(Some(':'), digits123()).len()
        == digits123()[0].len() + digits123()[1].len() + digits123()[2].len() + 2
) by (compute);
  assert(
    dense_join(None, digits123()).len()
        == digits123()[0].len() + digits123()[1].len() + digits123()[2].len()
) by (compute); }

// d7: ONE separator invariant, two element protocols — the dense join IS
// the shared fold applied to the densified list, so the loop the fix
// emits is the char** loop with a different element source (integer
// rendering instead of a pointer dereference). This is what makes the
// fix a routing change and not a second join implementation.
proof fn d7_dense_join_is_the_shared_fold()
    ensures
        dense_join(Some(':'), dense709()) == join_sep(Some(':'), dense709()),
        dense_join(ifs_sep(ifs_colon()), dense709()) == join_sep(ifs_sep(ifs_colon()), dense709()),
        forall|xs: Seq<Str>| #[trigger] dense_join(None, xs) == join_sep(None, xs),
{ assert(dense_join(Some(':'), dense709()) == join_sep(Some(':'), dense709())) by (compute);
  assert(
    dense_join(ifs_sep(ifs_colon()), dense709()) == join_sep(ifs_sep(ifs_colon()), dense709())
) by (compute); }

// ── the presence marker (2026-11-04) ────────────────────────────────
// e1: the sparse array, exactly as bash prints it. `n[0]=7; n[2]=9`
// gives `${#n[@]}`=2, `${n[*]}`=`7 9`, `${n[1]}`="" — the three readers
// the marker feeds. (Pre-fix the C said 3, `7 0 9`, `0`.)
proof fn e1_marked_sparse()
    ensures
        dense_count(sparse7h9()) == 2,
        dense_marked_face(true, sparse7h9(), startup()) == seq!['7', ' ', '9'],
        dense_get(sparse7h9(), 1) == Seq::<char>::empty(),
        dense_get(sparse7h9(), 2) == seq!['9'],
{ assert(dense_count(sparse7h9()) == 2) by (compute);
  assert(
    dense_marked_face(true, sparse7h9(), startup()) == seq!['7', ' ', '9']
) by (compute);
  assert(dense_get(sparse7h9(), 1) == Seq::<char>::empty()) by (compute);
  assert(dense_get(sparse7h9(), 2) == seq!['9']) by (compute); }

// e2: count is the POPCOUNT, not the extent — the one-line statement of
// what the marker bought (the extent protocol says 3).
proof fn e2_count_is_popcount()
    ensures
        dense_count(sparse7h9()) == 2,
        densify(sparse7h9()).len() == 3,
        dense_count(sparse7h9()) != densify(sparse7h9()).len(),
{ assert(dense_count(sparse7h9()) == 2) by (compute);
  assert(densify(sparse7h9()).len() == 3) by (compute); }

// e3: `${!n[*]}` is the SET indices, ascending: `0 2` (measured).
proof fn e3_keys_are_set_indices()
    ensures present_idx(sparse7h9()) == seq![0int, 2int]
{ assert(present_idx(sparse7h9()) == seq![0int, 2int]) by (compute); }

// e4: THE fix, as a theorem-ish obligation — with the marker the dense
// faces ARE the char** faces, for both faces and any IFS. Nothing
// dense-specific is left in the semantics: the marker restores
// `present(slots)`, so one join rule serves both storages.
proof fn e4_marked_equals_char_star()
    ensures
        dense_marked_face(true, sparse7h9(), ifs_colon())
            == join_face(true, sparse7h9(), ifs_colon()),
        dense_marked_face(false, sparse7h9(), ifs_colon())
            == join_face(false, sparse7h9(), ifs_colon()),
        dense_marked_face(true, hole_vec(), startup())
            == join_face(true, hole_vec(), startup()),
{ assert(
    dense_marked_face(true, sparse7h9(), ifs_colon())
        == join_face(true, sparse7h9(), ifs_colon())
) by (compute);
  assert(
    dense_marked_face(false, sparse7h9(), ifs_colon())
        == join_face(false, sparse7h9(), ifs_colon())
) by (compute);
  assert(
    dense_marked_face(true, hole_vec(), startup())
        == join_face(true, hole_vec(), startup())
) by (compute); }

// e5: `unset 'n[0]'` removes the ELEMENT — the count drops, the join
// loses the value, and the read is empty (measured: `1 [9] [2] []`).
proof fn e5_unset_removes_element()
    ensures
        dense_count(dense_clear(sparse7h9(), 0)) == 1,
        dense_marked_face(true, dense_clear(sparse7h9(), 0), startup()) == seq!['9'],
        dense_get(dense_clear(sparse7h9(), 0), 0) == Seq::<char>::empty(),
        present_idx(dense_clear(sparse7h9(), 0)) == seq![2int],
{ assert(dense_count(dense_clear(sparse7h9(), 0)) == 1) by (compute);
  assert(
    dense_marked_face(true, dense_clear(sparse7h9(), 0), startup()) == seq!['9']
) by (compute);
  assert(
    dense_get(dense_clear(sparse7h9(), 0), 0) == Seq::<char>::empty()
) by (compute);
  assert(present_idx(dense_clear(sparse7h9(), 0)) == seq![2int]) by (compute); }

// e6: a REAL ZERO is an element. `a=(0 0 0)` counts 3 and prints `0 0 0`
// — which is exactly why presence cannot be derived from the value.
proof fn e6_zero_is_present()
    ensures
        dense_count(seq![Some(seq!['0']), Some(seq!['0']), Some(seq!['0'])]) == 3,
        dense_get(seq![Some(seq!['0'])], 0) == seq!['0'],
{ assert(
    dense_count(seq![Some(seq!['0']), Some(seq!['0']), Some(seq!['0'])]) == 3
) by (compute);
  assert(dense_get(seq![Some(seq!['0'])], 0) == seq!['0']) by (compute); }

// e7: the MUTANT — the extent protocol (no marker) on the same array.
// Where the two disagree is precisely the divergence the marker removed,
// and it is provably not dismissible as a formatting difference.
// (`densify` maps a hole to an EMPTY word, so the fold leaves a doubled
// separator: `7  9`. The renderer's extent join instead prints the slot's
// VALUE, so the emitted C says `7 0 9` — measured. Both differ from
// bash's `7 9`, and the marker removes the cause rather than the
// symptom; e1/e4 are the shapes that must now hold.)
proof fn e7_extent_protocol_disagrees()
    ensures
        dense_face(true, densify(sparse7h9()), startup())
            != dense_marked_face(true, sparse7h9(), startup()),
        dense_face(true, densify(sparse7h9()), startup())
            == seq!['7', ' ', ' ', '9'],
{  assert(
    dense_face(true, densify(sparse7h9()), startup()) == seq!['7', ' ', ' ', '9']
) by (compute);
  // ground the RIGHT side too, or the inequality cannot be discharged
  assert(
    dense_marked_face(true, sparse7h9(), startup()) == seq!['7', ' ', '9']
) by (compute);
  assert(
    dense_face(true, densify(sparse7h9()), startup())
        != dense_marked_face(true, sparse7h9(), startup())
); }

// ── the append base (2026-11-04) ───────────────────────────────────
// f1: the three measured cases. A dense array appends at its extent; a
// SPARSE one appends past the last hole; an EMPTIED one appends at 0
// even though the extent is still 1.
pub open spec fn sparse_d() -> Seq<Slot> {
    seq![Some(seq!['1']), None, None, Some(seq!['9'])]
}
pub open spec fn emptied_i() -> Seq<Slot> {
    seq![None]
}
proof fn f1_append_base()
    ensures
        append_base(sparse7h9()) == 3,
        append_base(sparse_d()) == 4,
        append_base(emptied_i()) == 0,
        append_base(seq![Some(seq!['1']), Some(seq!['2'])]) == 2,
        append_base(Seq::<Slot>::empty()) == 0,
{ assert(append_base(sparse7h9()) == 3) by (compute);
  assert(append_base(sparse_d()) == 4) by (compute);
  assert(append_base(emptied_i()) == 0) by (compute);
  assert(append_base(seq![Some(seq!['1']), Some(seq!['2'])]) == 2) by (compute);
  assert(append_base(Seq::<Slot>::empty()) == 0) by (compute); }

// f2: `_len` is NOT the base — the emptied case is where the two split,
// and using the extent there would leave a phantom slot 0 (count 2 where
// bash says 1).
proof fn f2_extent_is_not_the_base()
    ensures
        append_base(emptied_i()) != (emptied_i().len() as int),
        append_base(sparse_d()) == (sparse_d().len() as int),
{ assert(append_base(emptied_i()) != (emptied_i().len() as int)) by (compute);
  assert(append_base(sparse_d()) == (sparse_d().len() as int)) by (compute); }

// f3: the count after an append is popcount + k — the appended values are
// ELEMENTS, which is what the missing presence bits broke (the C stored
// them but counted `2 [1 2]`).
proof fn f3_append_count()
    ensures
        append_count(sparse7h9(), 1) == 3,
        append_count(seq![Some(seq!['1']), Some(seq!['2'])], 2) == 4,
        append_count(emptied_i(), 1) == 1,
{ assert(append_count(sparse7h9(), 1) == 3) by (compute);
  assert(append_count(seq![Some(seq!['1']), Some(seq!['2'])], 2) == 4) by (compute);
  assert(append_count(emptied_i(), 1) == 1) by (compute); }

} // verus!

fn main() {}
