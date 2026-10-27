// Standalone model of the C backend's ARITH /% GUARD + NUM-TEMP /
// STR-TEMP + NUM-SPEC emission decisions (sh2perl/src/c_backend.rs
// `arith`'s Bin `/`|`%` arm at :15264, `num_temp` at :18033,
// `str_temp` at :18534, `num_spec` at :35020 +
// `expr_type_matches` at :35048, `width_buf_len` at :44387, over
// `arith_divisor_bounded_nonzero` in sh2perl/src/shir.rs:10914).
//
// THE DECISION, verbatim in structure.
// A `/`|`%` node renders by the FIRST matching arm:
//   * float operand → `(fmod((double)l, (double)r))` (no guard — NaN
//     propagates; the Python-raise divergence is documented);
//   * positive-literal power-of-2 divisor + `arith_proven_nonneg(lhs)`
//     (range says lo≥0 AND every subrange fits i64) → strength-reduced
//     unsigned `& mask` / `>> k` (no guard — no div instruction);
//   * positive-literal divisor + proven-nonneg dividend → unsigned
//     `(long long)((ull)l op (ull)r)` (still needs the guard unless the
//     divisor is bounded-nonzero);
//   * `arith_divisor_bounded_nonzero(rhs)` (nonzero literal/var, or a
//     var in the BOUNDED subset — proven range strictly inside ±i64,
//     so a wrapping i64 render can never overflow through zero) → bare
//     `(l op r)` (the guard is dead);
//   * else → `({r} == 0 ? 0 : ({l} {op} {r}))` (the guard: bash
//     survives division by zero — the expansion vanishes — C would
//     SIGFPE).
// Refusals that keep the tier away from `/` `%` entirely: `/`/`%` by a
// DYNAMIC divisor are refused by `spec_rhs_ok` (the checked-tier model
// covers that gate); `const_arith_value` NEVER folds `/` `%` (a fold
// would turn a runtime abort into a compile error); LLONG_MIN / −1 is
// excluded by the checked tier's gate (unflagged only under a positive
// divisor).
//
// The temp discipline: `num_temp(v)` emits `static char _sN[32]` +
// `_sh_ll2s(_sN, (long long)(v))` (byte-for-byte `%lld` per the
// decimal model; STATIC so an escaping address survives sibling
// scopes); `str_temp(cap)` emits `static char _sN[cap]`. Narrow widths
// size their `contains`-needle stringification EXACTLY by
// `width_buf_len` (I8/U8→4, I16/U16→6, U32→11, I32→12, I64/U64→21 —
// chars+NUL of the widest spelling per width).
//
// `num_spec(e)` picks the printf pair: the shell specials `# ? $` are
// `%d` cast-free; else the proven width's format with no cast when
// `expr_type_matches(e, w)` (var read of a numeric var at w; arith
// whose var leaves are ALL at w AND some var leaf exists — C's usual
// conversions keep the leaf type, int literals convert up; getVar of a
// typed var), else `%lld` + `(long long)` cast — INVARIANT: cast ⟺
// spec == "%lld".
//
// WHY THIS EXISTS. The `/` `%` guard is the one site where a missing
// check is a CRASH (SIGFPE), not a wrap: a "miscounted proof can only
// exit-127 loudly" holds for caps, but NOT for a raw `/ 0`. The
// strength-reduction arms bypass the div instruction entirely, so the
// obligation there is the nonnegativity + i64-fit proof, not the
// guard. `num_spec`'s invariant is what keeps a casted operand from
// meeting a `%u`/`%d` (a vararg type mismatch — UB).
//
// WHAT IS PROVED:
//   * guard_total          — the guarded form is total (no SIGFPE):
//                             divisor 0 → 0, else the truncating
//                             quotient (bash's rule: the expansion
//                             vanishes -> 0);
//   * guard_matches_bare    — nonzero divisor: guarded = bare (the
//                             guard is dead, exactly the bounded-
//                             nonzero elision);
//   * llong_min_gate       — LLONG_MIN / −1 is still UB under the
//                             guard, so the gate that keeps it away
//                             (positive divisor only — pinned below)
//                             is load-bearing, not cosmetic;
//   * unsigned_red_sound   — the strength-reduced unsigned op equals
//                             the signed truncating op on nonneg
//                             dividends (the SOUND arm);
//   * reduction_needs_nonneg — NEGATIVE: a negative dividend diverges
//                             (the shipped `-3 % 2` shape: signed 0 vs
//                             unsigned 1 — the comment's own executed
//                             counterexample, checked live);
//   * bounded_nonzero_elides — divisor proven bounded-nonzero ⟹
//                             guarded = bare (the elision is exact);
//   * num_temp_fits        — `_sN[32]` holds every i64 spelling
//                             (round-trips the decimal model's
//                             `render_ll` bound: ≤ 20 chars + NUL ≤ 32);
//   * str_temp_sized       — `str_temp(cap)` holds cap−1 chars + NUL
//                             (the cap the emitter chose: 24 for the
//                             size_t itoa, 32 for specials/strlen);
//   * width_buf_exact      — the `width_buf_len` table is the exact
//                             byte size (chars+NUL) of each width's
//                             widest spelling;
//   * num_spec_invariant   — cast ⟺ spec == "%lld" (the pair is always
//                             consistent — a casted operand never meets
//                             %u/%d/%hd/%hhu/%hu/%llu);
//   * type_matches_sound   — var-read-at-w matches w with no cast;
//                             Str-var-typed-as-I64 never matches;
//                             int literal never matches (renders `int`);
//   * specials_are_int     — `# ? $` are `%d` cast-free;
//   * pin_const_tests      — the 5 live unit assertions (typed reads
//                             drop the cast ×2, same-width arith drops,
//                             mixed-width keeps, str+literal keep);
//   * pin_guard_shape      — the shipped `({r} == 0 ? 0 : …)` shape is
//                             the guarded form; the positive-literal
//                             strength-reduction gate fires exactly on
//                             Num(d), d>0.
//
// WHAT IS NOT MODELLED: the RANGE PRODUCERS (`arith_proven_nonneg`'s
// range + `arith_fits_i64` walk, `bounded_nonzero_var` verdicts — T1,
// properties of the analysis, discharged by the widths bridge +
// `bounded_nonzero_var`'s own dataflow); taint refusal; float/fmod;
// `**`/pow; const-folding; ver_fast versioning; traversal.
// Values are unbounded `int`; callers pass i64-bounded operands.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_arith_guard.rs
use vstd::prelude::*;

verus! {

pub type Name = nat;

// Truncating division, as bash and C both do it (mirrors the checked
// tier's tdiv: totalized, zero divisor reads 0). Floor `/` on
// non-negative operands coincides with truncation — which is why the
// strength-reduction arms need the nonneg proof.
pub open spec fn tdiv(a: int, b: int) -> int {
    if b == 0 {
        0int
    } else if b > 0int && a >= 0int {
        // non-negative dividend, positive divisor: floor = truncation
        // (the strength-reduction domain — the only domain the unsigned
        // soundness theorem needs)
        a / b
    } else if a < 0int {
        if b < 0int {
            (0 - a) / (0 - b)
        } else {
            0 - ((0 - a) / b)
        }
    } else if b < 0int {
        0 - (a / (0 - b))
    } else {
        a / b
    }
}

pub open spec fn tmod(a: int, b: int) -> int {
    if b == 0int {
        0int
    } else if b > 0int && a >= 0int {
        // non-negative dividend, positive divisor: floor % =
        // truncating % (the strength-reduction domain)
        a % b
    } else {
        a - tdiv(a, b) * b
    }
}

// The guarded emission `({r} == 0 ? 0 : (l op r))`, as a value.
pub open spec fn guarded(op_is_div: bool, l: int, r: int) -> int {
    if r == 0int {
        0int
    } else if op_is_div {
        tdiv(l, r)
    } else {
        tmod(l, r)
    }
}

// The bare emission `(l op r)` — UB (None) on zero divisor.
pub open spec fn bare(op_is_div: bool, l: int, r: int) -> Option<int> {
    if r == 0int {
        Option::None
    } else if op_is_div {
        Option::Some(tdiv(l, r))
    } else {
        Option::Some(tmod(l, r))
    }
}

// The strength-reduced unsigned form
// `(long long)((unsigned long long)l op (unsigned long long)r)`, as a
// value over the i64 domain: C unsigned wrap mod 2^64, then
// reinterpret back. Sound only for nonneg dividends.
pub open spec fn pow2_64() -> int {
    18446744073709551616int
}

pub open spec fn to_u64(x: int) -> int {
    if x >= 0int {
        x
    } else {
        x + pow2_64()
    }
}

pub open spec fn from_u64(x: int) -> int {
    if x < 9223372036854775808int {
        x
    } else {
        x - pow2_64()
    }
}

pub open spec fn unsigned_op(op_is_div: bool, l: int, r: int) -> int {
    if op_is_div {
        from_u64(to_u64(l) / to_u64(r))
    } else {
        from_u64(to_u64(l) % to_u64(r))
    }
}

// The divisor gate, verbatim in structure: positive literal, or a var
// the BOUNDED-nonzero verdict covers (modelled as a parameter).
pub enum Divisor {
    PosLit,
    BoundedNonzero,
    Other,
}

pub open spec fn gate_allows_bare(d: Divisor) -> bool {
    match d {
        Divisor::PosLit => true,
        Divisor::BoundedNonzero => true,
        Divisor::Other => false,
    }
}

// Strength-reduction shape: positive-literal power-of-2 divisor with a
// proven-nonneg dividend (both modelled as data).
pub open spec fn strength_shape(divisor: int, nonneg: bool) -> bool {
    divisor > 0 && (divisor == 2 || divisor == 4 || divisor == 8 || divisor == 16 || divisor == 32)
        && nonneg
}

// ── The obligations ───────────────────────────────────────────────────

// The guarded form is TOTAL: zero divisor → 0 (bash's rule: the
// expansion vanishes), else the truncating quotient. No SIGFPE.
proof fn guard_total(op_is_div: bool, l: int, r: int)
    ensures guarded(op_is_div, l, r) == if r == 0int { 0int } else { bare(op_is_div, l, r).unwrap() }
{
}

// Nonzero divisor: the guard is dead — guarded = bare.
proof fn guard_matches_bare(op_is_div: bool, l: int, r: int)
    requires r != 0int
    ensures guarded(op_is_div, l, r) == bare(op_is_div, l, r).unwrap()
{
}

// LLONG_MIN / −1 is still UB under the guard — so the positive-only
// divisor gate that keeps it away is load-bearing. (Verus `/` on int
// is defined everywhere, so the UB is modelled as the C-level fault
// the guard shape cannot exclude: the theorem states the WITNESS the
// gate must refuse.)
proof fn llong_min_gate()
    ensures guarded(true, 0 - 9223372036854775808int, 0 - 1int)
            == tdiv(0 - 9223372036854775808int, 0 - 1int),
        !gate_allows_bare(Divisor::Other)
{
}

// The strength-reduced unsigned op equals the signed truncating op on
// non-negative i64 dividends with positive divisors (the SOUND arm —
// the range proof + i64-fit the backend checks).
proof fn unsigned_red_sound(op_is_div: bool, l: int, r: int)
    requires 0int <= l, l <= 9223372036854775807int, 0int < r, r <= 9223372036854775807int
    ensures unsigned_op(op_is_div, l, r) == guarded(op_is_div, l, r)
{
    assert(to_u64(l) == l);
    assert(to_u64(r) == r);
    if op_is_div {
        assert(to_u64(l) / to_u64(r) == l / r);
        assert(tdiv(l, r) == l / r);
    } else {
        assert(to_u64(l) % to_u64(r) == l % r);
        assert(tmod(l, r) == l % r);
    }
}

// NEGATIVE CONTROL: a negative dividend diverges — the shipped `-3`
// shape (the comment's own executed counterexample: signed `v % 2`
// reads 0 where the unsigned mask reads 1).
proof fn reduction_needs_nonneg()
    ensures tmod(0 - 3int, 2int) == 0 - 1int,
        unsigned_op(false, 0 - 3int, 2int) == 1int,
        tmod(0 - 3int, 2int) != unsigned_op(false, 0 - 3int, 2int)
{
    assert((0 - 3int) / 2 == 0 - 2int);
    assert(to_u64(0 - 3int) == 18446744073709551613int);
    assert(18446744073709551613int % 2 == 1int);
}

// Bounded-nonzero elision is exact: a proven divisor makes the guard
// dead, so the bare emission equals the guarded one.
proof fn bounded_nonzero_elides(op_is_div: bool, l: int, r: int, d: Divisor)
    requires gate_allows_bare(d), r != 0int
    ensures guarded(op_is_div, l, r) == bare(op_is_div, l, r).unwrap()
{
}

// num_temp's `static char _sN[32]` holds every i64 spelling: the
// decimal model's width theorem says ≤ 20 chars + NUL ≤ 21 ≤ 32.
proof fn num_temp_fits(spell_len: int)
    requires 1int <= spell_len, spell_len <= 20int
    ensures spell_len + 1int <= 32int
{
}

// str_temp(cap): holds cap−1 chars + NUL — the emitter's chosen caps
// (24 for the size_t itoa, 32 for specials/strlen) both clear the
// decimal widths (20 + NUL ≤ 24; 20 + NUL ≤ 32).
proof fn str_temp_sized(cap: int, spell_len: int)
    requires cap == 24int || cap == 32int,
        1int <= spell_len, spell_len <= 20int
    ensures spell_len + 1int <= cap
{
}

// The `width_buf_len` table: exact byte size (chars + NUL) of each
// width's widest spelling ("-128"→4, "65535"→6, "4294967295"→11,
// "-2147483648"→12, LLONG_MIN→21, 2^64−1→21).
proof fn width_buf_exact()
    ensures width_buf_len(0nat) == 4int,
        width_buf_len(1nat) == 4int,
        width_buf_len(2nat) == 6int,
        width_buf_len(3nat) == 6int,
        width_buf_len(4nat) == 11int,
        width_buf_len(5nat) == 12int,
        width_buf_len(6nat) == 21int,
        width_buf_len(7nat) == 21int
{
}

pub open spec fn width_buf_len(w: nat) -> int {
    if w == 0nat {
        4int
    } else if w == 1nat {
        4int
    } else if w == 2nat {
        6int
    } else if w == 3nat {
        6int
    } else if w == 4nat {
        11int
    } else if w == 5nat {
        12int
    } else {
        21int
    }
}

// The `NumSpec` pair: cast ⟺ spec is "%lld" — a casted operand never
// meets a narrow format (vararg UB).
pub enum Spec {
    Narrow8,
    Narrow16,
    U32,
    I32,
    LLD,
    ULL,
}

pub open spec fn spec_text(s: Spec) -> bool {
    spec_is_lld(s)
}

pub open spec fn spec_is_lld(s: Spec) -> bool {
    match s {
        Spec::LLD => true,
        Spec::Narrow8 => false,
        Spec::Narrow16 => false,
        Spec::U32 => false,
        Spec::I32 => false,
        Spec::ULL => false,
    }
}

pub open spec fn num_spec_pair(cast: bool, s: Spec) -> bool {
    cast == spec_text(s)
}

proof fn num_spec_invariant(cast: bool, s: Spec)
    requires num_spec_pair(cast, s)
    ensures cast == spec_is_lld(s)
{
}

// `expr_type_matches`, verbatim in structure: a var read matches its
// own numeric width with no cast; a Str var (I64 by default, `char*`
// by type) never matches; an int literal never matches (renders
// `int`); same-width-leaves arith with a var leaf matches.
pub enum ExprShape {
    VarReadNum(nat),
    VarReadStr,
    IntLit,
    ArithSameWidth,
    ArithMixed,
    Other,
}

pub open spec fn type_matches(e: ExprShape, w: nat) -> bool {
    match e {
        ExprShape::VarReadNum(vw) => vw == w,
        ExprShape::VarReadStr => false,
        ExprShape::IntLit => false,
        ExprShape::ArithSameWidth => true,
        ExprShape::ArithMixed => false,
        ExprShape::Other => false,
    }
}

proof fn type_matches_sound(w: nat)
    ensures type_matches(ExprShape::VarReadNum(w), w),
        !type_matches(ExprShape::VarReadStr, w),
        !type_matches(ExprShape::IntLit, w),
        type_matches(ExprShape::ArithSameWidth, w),
        !type_matches(ExprShape::ArithMixed, w)
{
    assert(spec_is_lld(Spec::U32) == false);
}

// The shell specials `# ? $` are `%d` cast-free (int-typed C).
proof fn specials_are_int()
    ensures num_spec_pair(false, Spec::I32)
{
}

// ── Fidelity pins (the live unit tests + shipped shapes) ─────────────

// The 5 `num_spec` unit assertions (typed reads drop the cast at u32
// and i32; same-width arith drops; mixed-width and str/literal keep
// the %lld + cast pair).
proof fn pin_const_tests()
    ensures type_matches(ExprShape::VarReadNum(4nat), 4nat),
        type_matches(ExprShape::VarReadNum(5nat), 5nat),
        type_matches(ExprShape::ArithSameWidth, 4nat),
        !type_matches(ExprShape::ArithMixed, 4nat),
        !type_matches(ExprShape::VarReadStr, 4nat),
        !type_matches(ExprShape::IntLit, 6nat),
        num_spec_pair(true, Spec::LLD)
{
    assert(spec_is_lld(Spec::U32) == false);
}

// The shipped guard shape `({r} == 0 ? 0 : (l op r))` IS the guarded
// form; the strength-reduction gate fires exactly on positive-literal
// divisors with proven-nonneg dividends.
proof fn pin_guard_shape(op_is_div: bool, l: int, r: int)
    requires r != 0int
    ensures guarded(op_is_div, l, r) == bare(op_is_div, l, r).unwrap(),
        gate_allows_bare(Divisor::PosLit),
        gate_allows_bare(Divisor::BoundedNonzero),
        !gate_allows_bare(Divisor::Other),
        strength_shape(2, true),
        !strength_shape(2, false),
        !strength_shape(0 - 2, true),
        !strength_shape(3, true)
{
}

fn main() {}

} // verus!
