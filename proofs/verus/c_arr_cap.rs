// Model of the C backend's FIXED-ARRAY CAPACITY decision
// (`sh2perl/src/c_backend.rs`: `array_fixed_caps` → `array_caps` →
// `array_cap()` → the decl sites at the `array_decls` loop → the reset
// emitter `emit_set_array` and its `"$@"` spread guard).
//
// THE CLAIM (C_UB_PROOF.md §2b, "array_caps is the unproven bound"):
// every array the renderer emits as a FIXED table is declared large enough
// for the elements the renderer itself writes into it, and an array whose
// literal bulk reaches `ARR_CAP` (1024) is never emitted as a fixed table at
// all — it is routed to the growable vec instead. The two halves of the
// "or the guard engages" clause are (a) the vec routing (`select_vec_arrays`
// / the `routed` set, selected on the same `append_hi`/`need` bound) and
// (b) the runtime `capacity exceeded` exit-127 guard at every dynamic store.
//
// THE SIZING RULE, verbatim (`array_fixed_caps`, c_backend.rs:9424):
//   base_need = max(max_reset, lit_index_max, counter_index_max)
//   need      = base_need + appends          (saturating in Rust)
//   need     += 1 if assoc && scalar_write
//   fixed cap = max(need, 1)   IF  counter_index_max > 0 || need < ARR_CAP
//   otherwise the array is ROUTED to vec storage (grow on demand)
// `max_reset` is the literal reset bulk — the number of elements the reset
// emitter writes — which is what makes `fixed_initializer_fits` the
// obligation the renderer's `debug_assert!` (and the corpus scan) test.
//
// WHAT IS PROVED (all under `well_formed`: every count is non-negative):
//   * fixed_initializer_fits — a fixed table is at least as large as the
//     literal it is initialized with (max_reset ≤ cap). This is the claim
//     the `emit_set_array` debug_assert checks on every render.
//   * fixed_fits_needs — cap ≥ need, i.e. the declaration covers the
//     analytic bound the census computed (bulk, literal index, counter
//     index and appends), not just the bulk.
//   * fixed_cap_below_arr_cap — a fixed table with no exact counter term
//     has cap < ARR_CAP: the 1024 table is not the sizing, it is the
//     ceiling that routes everything unproven to a vec.
//   * big_bulk_is_routed — a literal at or above ARR_CAP elements can never
//     be emitted as a fixed table (its `need` alone exceeds the gate). This
//     is the measured behaviour: a 1500-element literal renders as
//     `static char **a = NULL` + `_sh_grow_str`, not `a[1500]`.
//   * counter_term_bypasses_the_gate — the one way past the ARR_CAP gate
//     (`counter_index_max > 0`) still keeps cap ≥ need, so the bypass is a
//     larger PROVEN table, not an unproven one.
//   * declared_guard_sound — the write guard `requested > cap ⇒ exit(127)`
//     admits exactly the writes the declaration can hold.
//   * arrcap_guard_unsound — NEGATIVE CONTROL, and the bug fixed on
//     2026-11-09: the spread guard compared the write count against
//     `ARR_CAP` instead of the declared cap, so for a proof-sized table
//     (`a[3]`) it admitted `requested = 4` and the stores ran past the
//     array. The renderer now passes `acap` (the same number the
//     declaration uses), which is what `declared_guard_sound` requires.
//
// WHAT IS NOT MODELLED: the census that produces `max_reset`/`appends`
// (it is a syntactic count over the IR; a miscount is exactly what the
// debug_assert and the corpus scan exist to catch), the runtime growth
// policy of a vec (`_sh_grow_*` doubling), and the C semantics of the
// stores beyond "index < cap is in bounds".
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 c_arr_cap.rs
use vstd::prelude::*;

verus! {

/// `const ARR_CAP: usize = 1024` — the ceiling, not the sizing.
pub open spec fn arr_cap() -> int { 1024 }

/// What `array_fixed_caps` knows about one array.
pub struct Info {
    pub max_reset: int,          // literal reset bulk (elements written)
    pub lit_index_max: int,      // max literal index written
    pub counter_index_max: int,  // exact counter-derived max index
    pub appends: int,            // statically bounded appends
    pub assoc_scalar: bool,      // an assoc's scalar write costs one key
    pub defined: bool,
    pub unbounded: bool,         // split/mapfile/read: unknown extent
    pub indexed_write: bool,     // dynamic index write
    pub escaped: bool,
    pub excluded: bool,          // fn-return / bigint / assoc exclusions
}

pub open spec fn well_formed(i: Info) -> bool {
    i.max_reset >= 0 && i.lit_index_max >= 0 && i.counter_index_max >= 0 && i.appends >= 0
}

pub open spec fn max3(a: int, b: int, c: int) -> int {
    if a >= b && a >= c {
        a
    } else if b >= c {
        b
    } else {
        c
    }
}

pub open spec fn base_need(i: Info) -> int {
    max3(i.max_reset, i.lit_index_max, i.counter_index_max)
}

pub open spec fn assoc_extra(i: Info) -> int {
    if i.assoc_scalar {
        1
    } else {
        0
    }
}

/// The census's bound (Rust uses `saturating_add`; over non-negative counts
/// the plain sum is the same value).
pub open spec fn need(i: Info) -> int {
    base_need(i) + i.appends + assoc_extra(i)
}

/// The array grows on demand instead of getting a fixed table.
pub open spec fn routed(i: Info) -> bool {
    i.unbounded || i.indexed_write || !i.defined || i.escaped || i.excluded
        || !(i.counter_index_max > 0 || need(i) < arr_cap())
}

pub open spec fn fixed(i: Info) -> bool {
    !routed(i)
}

/// The declared capacity: `need.max(1)`, exactly as `array_fixed_caps`
/// inserts it (`out.insert(name, need.max(1) as usize)`).
pub open spec fn cap(i: Info) -> int {
    if need(i) > 1 {
        need(i)
    } else {
        1
    }
}

// ── the declaration covers what the renderer writes ──────────────────────

proof fn need_nonneg(i: Info)
    requires
        well_formed(i),
    ensures
        need(i) >= 0,
{
}

proof fn base_le_need(i: Info)
    requires
        well_formed(i),
    ensures
        base_need(i) <= need(i),
{
}

proof fn cap_ge_need(i: Info)
    requires
        well_formed(i),
    ensures
        cap(i) >= need(i),
{
    need_nonneg(i);
}

proof fn max_reset_le_base(i: Info)
    ensures
        i.max_reset <= base_need(i),
{
}

/// THE OBLIGATION THE RENDERER CHECKS: a fixed table can hold its literal.
proof fn fixed_initializer_fits(i: Info)
    requires
        well_formed(i),
        fixed(i),
    ensures
        i.max_reset <= cap(i),
{
    max_reset_le_base(i);
    base_le_need(i);
    cap_ge_need(i);
}

/// ... and it covers the whole analytic bound, not just the bulk.
proof fn fixed_fits_needs(i: Info)
    requires
        well_formed(i),
        fixed(i),
    ensures
        need(i) <= cap(i),
{
    cap_ge_need(i);
}

/// The 1024 gate: what stays fixed is smaller than the ceiling (unless the
/// exact counter term — a proven, not heuristic, bound — bypasses the gate).
proof fn fixed_cap_below_arr_cap(i: Info)
    requires
        well_formed(i),
        fixed(i),
        i.counter_index_max == 0,
    ensures
        cap(i) < arr_cap(),
{
    need_nonneg(i);
}

/// An array whose literal alone reaches the ceiling is never a fixed table:
/// `need ≥ max_reset ≥ ARR_CAP` fails the `< ARR_CAP` gate, so it is routed
/// to the growable vec. This is the measured 1500-element case.
proof fn big_bulk_is_routed(i: Info)
    requires
        well_formed(i),
        i.max_reset >= arr_cap(),
        i.counter_index_max == 0,
    ensures
        routed(i),
{
    max_reset_le_base(i);
    base_le_need(i);
}

/// The counter bypass is still a PROVEN cap: it only ever enlarges the table
/// to the census's `need`, so `fixed_initializer_fits` keeps holding.
proof fn counter_term_bypasses_the_gate(i: Info)
    requires
        well_formed(i),
        fixed(i),
        i.counter_index_max > 0,
    ensures
        i.max_reset <= cap(i),
{
    fixed_initializer_fits(i);
}

// ── the write guard ─────────────────────────────────────────────────────

pub enum Guard {
    /// `requested > cap ⇒ exit(127)` — the guard `array_cap`'s doc demands
    /// ("Decl and every fixed guard go through here so they always agree").
    Declared,
    /// `requested > ARR_CAP ⇒ exit(127)` — the shipped spread guard until
    /// 2026-11-09.
    ArrCap,
}

pub open spec fn admits(g: Guard, cap: int, requested: int) -> bool {
    match g {
        Guard::Declared => requested <= cap,
        Guard::ArrCap => requested <= arr_cap(),
    }
}

/// A write the declaration cannot hold.
pub open spec fn overflows(cap: int, requested: int) -> bool {
    requested > cap
}

proof fn declared_guard_sound(cap: int, requested: int)
    ensures
        admits(Guard::Declared, cap, requested) == !overflows(cap, requested),
{
}

/// NEGATIVE CONTROL (the shipped bug): for a proof-sized table the ARR_CAP
/// guard admits writes the table cannot hold. Witness: a 3-slot table and a
/// 4-element spread.
proof fn arrcap_guard_unsound()
    ensures
        admits(Guard::ArrCap, 3, 4),
        overflows(3, 4),
        !admits(Guard::Declared, 3, 4),
{
}

/// With the fixed guard the admitted writes are exactly the in-bounds ones,
/// so the reset loop and the spread loop cannot run past the declaration.
proof fn guard_bounds_the_stores(i: Info, requested: int)
    requires
        well_formed(i),
        fixed(i),
    ensures
        admits(Guard::Declared, cap(i), requested) ==> !overflows(cap(i), requested),
{
    declared_guard_sound(cap(i), requested);
}

fn main() {}

} // verus!
