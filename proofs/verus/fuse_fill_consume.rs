// Standalone model of fuse-fill-consume
// (sh2perl/src/transforms/fuse_fill_consume.rs): a loop that stores
//
//   for ((i = 0; i < n; i++)); do a[i] = F; done          [fill]
//
// followed (possibly across statements) by a loop that reads
//
//   for ((i = 0; i < n; i++)); do s = $(($s + a[i])); done  [consumer]
//
// is rewritten so the consumer reads `F` directly (evaluated at the consumer's
// own iteration) and the array (plus the fill loop) disappears.
//
// Proves the soundness obligation:
//
//   the_fusion_preserves_the_observable: for EVERY pair of loops the transform
//   can see — any iteration count, any store/read keys, any conditionally
//   executed stores, any scalar environment, any gap writes, any other array
//   use — the fused program emits exactly the reference bytes, with the
//   verdict `census` computed FROM those inputs, so a refused program takes
//   both sides with the flag `false`.
//
// ## What is modelled
//
// A `Trace` is the pair of loops as the transform sees them: for each of the
// `n` iterations, the fill's counter value, the consumer's counter value, the
// index the fill stores at, the index the consumer reads at, and whether the
// fill store executes at all; plus the fill RHS `rhs` (an expression over
// scalar slots and the counter), the scalar environment `env` at fill time,
// the writes `gap` between the fill's last iteration and the consumer's
// reads, and `other`, the values of any OTHER array reads in the program.
//
// The reference array state is `arr_at(t, j, n)`: the LAST store at index `j`,
// or 0 when nothing stored there. The emitted word sequence is
//   reference:  [ arr_at(t, read_key k, n) for k < n ] ++ other
//   fused:      [ ev(rhs, env after gap, cons_ctr k) for k < n ] ++ zeros
// The fused side is the observed transform output: the array is dropped, so
// its other reads would read the initial zero.
//
// ## The load-bearing rules, one obligation each
//
//  L1 the_fusion_preserves_the_observable — the census is sound: every
//     accepted trace emits the same bytes as the reference.
//  L2 the_read_value_is_preserved — at every iteration, the array element the
//     consumer read equals the expression the fusion substitutes for it.
//  L3 a_stored_value_is_the_expression_at_its_iteration — the value stored at
//     `a[k]` is `F` evaluated at THAT iteration's counter (the affine store
//     key and the shared header), so the forward needs no other element.
//  L4 the_gap_does_not_change_the_expression — the writes between the loops,
//     which the forward sees but the fill did not, leave `F`'s free names
//     alone.
//
// ## The census (computed from the trace, not a shape)
//
//   all_stored    every fill iteration stores (a hole would leave a[read]
//                 at its initial 0, which the forward replaces with F)
//   affine_fill   store_key k == fill_ctr k   (the index IS the counter)
//   affine_read   read_key k == cons_ctr k
//   same_header   fill_ctr k == cons_ctr k    (same trip count and phase)
//   stable        no gap write touches a free name of `rhs`
//   alone         no other array use observes the array's pre-fill contents
//
// ## What is NOT modelled
//
//   - The RECOGNITION (which shapes are loops, how `a[i]` is parsed to
//     (array, index) pairs, the statement-level adjacency rules). Those
//     decide what a trace the transform sees is; the law holds for every
//     trace, and the transform's own tests pin the recognition.
//   - Integer semantics: `int` is unbounded here. 64-bit wrap is a renderer
//     contract shared with bash's `$(( ))`.
//   - `MALLOC_PERTURB_` / uninitialised reads: modelled as the initial 0 that
//     the emitted C's `calloc`-like array has (see the byte anchor).
//
// ## Defects the model forced (all repaired in the transform; tests pin them)
//
//   1. A consumer RHS (or gap statement) with an arithmetic EFFECT
//      (`tmp=$((x=7))`) was not counted as a write to `x`, so a forward that
//      read `x` used 7 while the fill stored the old value (bash `2 7`,
//      emitted C `7 7`). Repaired by collecting evaluation-time effects and
//      vetoing those that hit `rhs`'s free names, the counter, the bound or
//      the condition names. Tests:
//      consumer_arith_effect_on_a_forwarded_var_vetoes,
//      gap_arith_effect_on_a_forwarded_var_vetoes,
//      consumer_arith_effect_on_the_counter_vetoes,
//      consumer_arith_effect_on_the_bound_vetoes,
//      consumer_text_arith_effect_vetoes.
//
// ## Mutants (checked red, each a real edit of this file)
//
//   identity / no-fuse (census false)        -> a_clean_fill_consumer_is_flagged
//   census widening: drop `all_stored`       -> L1 at the hole witness
//   census widening: drop `affine_fill`      -> L1 at the shifted-store witness
//   census widening: drop `affine_read`      -> L1 at the shifted-read witness
//   census widening: drop `same_header`      -> L1 at the header witness
//   census widening: drop `stable`           -> L1 at the gap witness
//   census widening: drop `alone`            -> L1 at the extra-read witness
//   census widening: accept everything       -> L1 at every refused witness
//   arr_at ignores stores (always 0)         -> L2 / L1 at the clean witness
//   fused side ignores the gap (`apply` no-op) -> L4 / L1 at the gap witness
//   forward uses the fill-time counter       -> L3 / L1 at the header witness

use vstd::prelude::*;

verus! {

pub type Val = int;

// ---------------------------------------------------------------------------
// The fill RHS, as an expression over scalar slots and the counter
// ---------------------------------------------------------------------------

pub enum E {
    Const(int),
    Ctr,
    Slot(int),
    Add(Box<E>, Box<E>),
    Sub(Box<E>, Box<E>),
    Mul(Box<E>, Box<E>),
}

pub open spec fn bx(e: E) -> Box<E> {
    Box::new(e)
}

/// Does `e` read scalar slot `s`? Free names, not surface syntax.
pub open spec fn mentions(e: &E, s: int) -> bool
    decreases *e,
{
    match e {
        E::Const(_) | E::Ctr => false,
        E::Slot(t) => *t == s,
        E::Add(a, b) | E::Sub(a, b) | E::Mul(a, b) => mentions(a, s) || mentions(b, s),
    }
}

// ---------------------------------------------------------------------------
// Scalar environment: slot id -> value, with a total guarded read and a
// length-preserving write (a slot outside the environment is 0 and a write
// to it is a no-op, on both sides).
// ---------------------------------------------------------------------------

pub type Env = Seq<Val>;

pub open spec fn get(env: Env, s: int) -> Val {
    if 0 <= s && s < env.len() as int {
        env[s]
    } else {
        0
    }
}

pub open spec fn put(env: Env, s: int, v: Val) -> Env {
    if 0 <= s && s < env.len() as int {
        env.update(s, v)
    } else {
        env
    }
}

proof fn put_hit(env: Env, s: int, v: Val)
    requires
        0 <= s < env.len(),
    ensures
        get(put(env, s, v), s) == v,
{
}

proof fn put_other(env: Env, s: int, t: int, v: Val)
    requires
        0 <= s < env.len(),
        t != s,
    ensures
        get(put(env, s, v), t) == get(env, t),
{
}

pub open spec fn ev(e: &E, env: Env, c: Val) -> Val
    decreases *e,
{
    match e {
        E::Const(k) => *k,
        E::Ctr => c,
        E::Slot(s) => get(env, *s),
        E::Add(a, b) => ev(a, env, c) + ev(b, env, c),
        E::Sub(a, b) => ev(a, env, c) - ev(b, env, c),
        E::Mul(a, b) => ev(a, env, c) * ev(b, env, c),
    }
}

/// L4's core: a write that does not touch a free name leaves the expression's
/// value alone.
proof fn ev_put(e: &E, env: Env, s: int, v: Val, c: Val)
    requires
        !mentions(e, s),
    ensures
        ev(e, put(env, s, v), c) == ev(e, env, c),
    decreases *e,
{
    match e {
        E::Const(_) | E::Ctr => {}
        E::Slot(t) => {
            assert(*t != s);
            if 0 <= s && s < env.len() as int {
                put_other(env, s, *t, v);
            } else {
                assert(put(env, s, v) == env);
            }
        },
        E::Add(a, b) | E::Sub(a, b) | E::Mul(a, b) => {
            ev_put(a, env, s, v, c);
            ev_put(b, env, s, v, c);
        },
    }
}

// ---------------------------------------------------------------------------
// The trace: the two loops as the transform sees them
// ---------------------------------------------------------------------------

pub struct Trace {
    pub n: nat,
    /// counter value at each fill iteration
    pub fill_ctr: Seq<Val>,
    /// counter value at each consumer iteration
    pub cons_ctr: Seq<Val>,
    /// index the fill stores at, per fill iteration
    pub store_key: Seq<Val>,
    /// index the consumer reads at, per consumer iteration
    pub read_key: Seq<Val>,
    /// did the fill store at all at that iteration?
    pub stored: Seq<bool>,
    /// the fill RHS (F), forwarded to the consumer
    pub rhs: E,
    /// scalar environment at fill time
    pub env: Env,
    /// writes between the fill's last iteration and the consumer's reads
    pub gap: Seq<(int, Val)>,
    /// values of any OTHER array reads in the program (other[] in order)
    pub other: Seq<Val>,
}

pub open spec fn well_formed(t: Trace) -> bool {
    &&& t.n as int == t.fill_ctr.len() as int
    &&& t.n as int == t.cons_ctr.len() as int
    &&& t.n as int == t.store_key.len() as int
    &&& t.n as int == t.read_key.len() as int
    &&& t.n as int == t.stored.len() as int
}

pub open spec fn stored_at(t: Trace, k: int) -> bool {
    if 0 <= k < t.stored.len() as int {
        t.stored[k]
    } else {
        false
    }
}

pub open spec fn store_key_at(t: Trace, k: int) -> Val {
    if 0 <= k < t.store_key.len() as int {
        t.store_key[k]
    } else {
        0
    }
}

pub open spec fn read_key_at(t: Trace, k: int) -> Val {
    if 0 <= k < t.read_key.len() as int {
        t.read_key[k]
    } else {
        0
    }
}

pub open spec fn fill_ctr_at(t: Trace, k: int) -> Val {
    if 0 <= k < t.fill_ctr.len() as int {
        t.fill_ctr[k]
    } else {
        0
    }
}

pub open spec fn cons_ctr_at(t: Trace, k: int) -> Val {
    if 0 <= k < t.cons_ctr.len() as int {
        t.cons_ctr[k]
    } else {
        0
    }
}

/// The array after the fill has run `i` iterations, read at index `j`: the
/// last executed store at `j` wins, and an index never stored reads 0.
pub open spec fn arr_at(t: Trace, j: Val, i: int) -> Val
    decreases i,
{
    if i <= 0 {
        0
    } else if stored_at(t, i - 1) && store_key_at(t, i - 1) == j {
        ev(&t.rhs, t.env, fill_ctr_at(t, i - 1))
    } else {
        arr_at(t, j, i - 1)
    }
}

/// Reference word k: the element the consumer's read saw.
pub open spec fn ref_at(t: Trace, k: int) -> Val {
    arr_at(t, read_key_at(t, k), t.n as int)
}

/// Fused word k: the forwarded expression, evaluated with the environment the
/// consumer sees (the fill environment plus the gap writes) at the consumer's
/// own counter.
pub open spec fn fused_at(t: Trace, k: int) -> Val {
    ev(&t.rhs, apply(t.env, t.gap, 0), cons_ctr_at(t, k))
}

pub open spec fn apply(env: Env, ws: Seq<(int, Val)>, i: int) -> Env
    decreases ws.len() - i,
{
    if i >= ws.len() {
        env
    } else {
        apply(put(env, ws[i].0, ws[i].1), ws, i + 1)
    }
}

pub open spec fn touches(ws: Seq<(int, Val)>, i: int, e: &E) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        mentions(e, ws[i].0) || touches(ws, i + 1, e)
    }
}

proof fn ev_apply(e: &E, env: Env, ws: Seq<(int, Val)>, i: int, c: Val)
    requires
        !touches(ws, i, e),
    ensures
        ev(e, apply(env, ws, i), c) == ev(e, env, c),
    decreases ws.len() - i,
{
    if i >= ws.len() {
    } else {
        assert(!mentions(e, ws[i].0));
        assert(!touches(ws, i + 1, e));
        ev_apply(e, put(env, ws[i].0, ws[i].1), ws, i + 1, c);
        ev_put(e, env, ws[i].0, ws[i].1, c);
    }
}

// ---------------------------------------------------------------------------
// The emitted bytes
// ---------------------------------------------------------------------------

pub open spec fn own_seq(t: Trace, fused: bool, i: int) -> Seq<Val>
    decreases t.n - i,
{
    if i >= t.n as int {
        Seq::empty()
    } else if fused {
        seq![fused_at(t, i)] + own_seq(t, fused, i + 1)
    } else {
        seq![ref_at(t, i)] + own_seq(t, fused, i + 1)
    }
}

pub open spec fn zeros(k: int) -> Seq<Val>
    decreases k,
{
    if k <= 0 {
        Seq::empty()
    } else {
        seq![0] + zeros(k - 1)
    }
}

pub open spec fn cat(a: Seq<Val>, b: Seq<Val>) -> Seq<Val> {
    if b.len() == 0 {
        a
    } else {
        a + b
    }
}

pub open spec fn ref_prog(t: Trace) -> Seq<Val> {
    cat(own_seq(t, false, 0), t.other)
}

pub open spec fn fused_prog(t: Trace) -> Seq<Val> {
    cat(own_seq(t, true, 0), zeros(t.other.len() as int))
}

pub open spec fn go(t: Trace, lift: bool) -> Seq<Val> {
    if lift {
        fused_prog(t)
    } else {
        ref_prog(t)
    }
}

// ---------------------------------------------------------------------------
// The census, computed from the trace
// ---------------------------------------------------------------------------

/// The four per-iteration rules are terminating scans (not quantifiers), so a
/// concrete trace's verdict can be computed and a mutant widening is testable.
pub open spec fn all_stored_from(t: Trace, i: int) -> bool
    decreases i,
{
    if i <= 0 {
        true
    } else {
        stored_at(t, i - 1) && all_stored_from(t, i - 1)
    }
}

pub open spec fn affine_fill_from(t: Trace, i: int) -> bool
    decreases i,
{
    if i <= 0 {
        true
    } else {
        store_key_at(t, i - 1) == fill_ctr_at(t, i - 1) && affine_fill_from(t, i - 1)
    }
}

pub open spec fn affine_read_from(t: Trace, i: int) -> bool
    decreases i,
{
    if i <= 0 {
        true
    } else {
        read_key_at(t, i - 1) == cons_ctr_at(t, i - 1) && affine_read_from(t, i - 1)
    }
}

pub open spec fn same_header_from(t: Trace, i: int) -> bool
    decreases i,
{
    if i <= 0 {
        true
    } else {
        fill_ctr_at(t, i - 1) == cons_ctr_at(t, i - 1) && same_header_from(t, i - 1)
    }
}

/// every fill iteration stores (a hole would leave a[read] at its initial 0)
pub open spec fn all_stored(t: Trace) -> bool {
    all_stored_from(t, t.n as int)
}

/// The store index is the counter value (the affine (1,0) key): so the
/// element read at iteration k is the one stored at iteration k.
pub open spec fn affine_fill(t: Trace) -> bool {
    affine_fill_from(t, t.n as int)
}

pub open spec fn affine_read(t: Trace) -> bool {
    affine_read_from(t, t.n as int)
}

/// Same bound, same phase, same trip count.
pub open spec fn same_header(t: Trace) -> bool {
    same_header_from(t, t.n as int)
}

proof fn all_stored_index(t: Trace, i: int, k: int)
    requires
        0 <= k < i,
        all_stored_from(t, i),
    ensures
        stored_at(t, k),
    decreases i,
{
    if i <= 0 {
    } else if k == i - 1 {
    } else {
        all_stored_index(t, i - 1, k);
    }
}

proof fn affine_fill_index(t: Trace, i: int, k: int)
    requires
        0 <= k < i,
        affine_fill_from(t, i),
    ensures
        store_key_at(t, k) == fill_ctr_at(t, k),
    decreases i,
{
    if i <= 0 {
    } else if k == i - 1 {
    } else {
        affine_fill_index(t, i - 1, k);
    }
}

proof fn affine_read_index(t: Trace, i: int, k: int)
    requires
        0 <= k < i,
        affine_read_from(t, i),
    ensures
        read_key_at(t, k) == cons_ctr_at(t, k),
    decreases i,
{
    if i <= 0 {
    } else if k == i - 1 {
    } else {
        affine_read_index(t, i - 1, k);
    }
}

proof fn same_header_index(t: Trace, i: int, k: int)
    requires
        0 <= k < i,
        same_header_from(t, i),
    ensures
        fill_ctr_at(t, k) == cons_ctr_at(t, k),
    decreases i,
{
    if i <= 0 {
    } else if k == i - 1 {
    } else {
        same_header_index(t, i - 1, k);
    }
}

/// No write between the loops touches a free name of the forwarded
/// expression (and, by its construction, none moves the counter or bound).
pub open spec fn stable(t: Trace) -> bool {
    !touches(t.gap, 0, &t.rhs)
}

/// No other array use observes the array: the fused program has no array.
pub open spec fn alone(t: Trace) -> bool {
    t.other.len() == 0
}

pub open spec fn census(t: Trace) -> bool {
    &&& all_stored(t)
    &&& affine_fill(t)
    &&& affine_read(t)
    &&& same_header(t)
    &&& stable(t)
    &&& alone(t)
}

// ---------------------------------------------------------------------------
// Lemmas
// ---------------------------------------------------------------------------

/// L3's core: under the census, the element read at iteration w is `rhs`
/// evaluated at iteration w's counter.
proof fn arr_at_finds_the_store(t: Trace, j: Val, w: int, i: int)
    requires
        census(t),
        0 <= w < i <= t.n as int,
        stored_at(t, w),
        store_key_at(t, w) == j,
    ensures
        arr_at(t, j, i) == ev(&t.rhs, t.env, j),
    decreases i,
{
    if i <= 0 {
        assert(false);
    } else if stored_at(t, i - 1) && store_key_at(t, i - 1) == j {
        // the affine store key makes the store's counter equal to the index
        affine_fill_index(t, t.n as int, i - 1);
        assert(fill_ctr_at(t, i - 1) == j);
    } else {
        // the store at `w` is not this one, so it is strictly earlier
        assert(w < i - 1);
        arr_at_finds_the_store(t, j, w, i - 1);
    }
}

/// L2: at every iteration, the array word equals the forwarded expression.
proof fn ref_is_fused_at(t: Trace, k: int)
    requires
        well_formed(t),
        census(t),
        0 <= k < t.n as int,
    ensures
        ref_at(t, k) == fused_at(t, k),
{
    all_stored_index(t, t.n as int, k);
    assert(stored_at(t, k));
    affine_read_index(t, t.n as int, k);
    same_header_index(t, t.n as int, k);
    affine_fill_index(t, t.n as int, k);
    assert(read_key_at(t, k) == store_key_at(t, k));
    arr_at_finds_the_store(t, read_key_at(t, k), k, t.n as int);
    assert(ref_at(t, k) == ev(&t.rhs, t.env, read_key_at(t, k)));
    ev_apply(&t.rhs, t.env, t.gap, 0, cons_ctr_at(t, k));
    assert(fused_at(t, k) == ev(&t.rhs, apply(t.env, t.gap, 0), cons_ctr_at(t, k)));
    assert(cons_ctr_at(t, k) == read_key_at(t, k));
}

proof fn own_seq_matches(t: Trace, i: int)
    requires
        well_formed(t),
        census(t),
        0 <= i <= t.n as int,
    ensures
        own_seq(t, false, i) == own_seq(t, true, i),
    decreases t.n as int - i,
{
    if i >= t.n as int {
    } else {
        ref_is_fused_at(t, i);
        own_seq_matches(t, i + 1);
        assert(own_seq(t, false, i) == seq![ref_at(t, i)] + own_seq(t, false, i + 1));
    }
}

// ---------------------------------------------------------------------------
// The laws
// ---------------------------------------------------------------------------

/// L1: THE LAW. For every pair of loops the transform can see, an ACCEPTED
/// trace emits exactly the reference bytes. A refused trace is
/// `go(..., false)` on both sides.
proof fn the_fusion_preserves_the_observable(t: Trace)
    requires
        well_formed(t),
    ensures
        go(t, census(t)) == go(t, false),
{
    if census(t) {
        own_seq_matches(t, 0);
        assert(t.other.len() == 0);
        assert(t.other == Seq::<Val>::empty());
        assert(zeros(t.other.len() as int) == Seq::<Val>::empty());
        assert(ref_prog(t) == own_seq(t, false, 0));
        assert(fused_prog(t) == own_seq(t, true, 0));
        assert(go(t, true) == go(t, false));
    }
}

/// L2 as a standalone obligation: at each iteration, the value the fused
/// program forwards is the value the reference read from the array.
proof fn the_read_value_is_preserved(t: Trace, k: int)
    requires
        well_formed(t),
        census(t),
        0 <= k < t.n as int,
    ensures
        ref_at(t, k) == fused_at(t, k),
{
    ref_is_fused_at(t, k);
}

/// L3 as a standalone obligation: the value stored at the element the
/// consumer reads is `rhs` at the store's own iteration, and under the census
/// that iteration is the consumer's iteration.
proof fn a_stored_value_is_the_expression_at_its_iteration(t: Trace, k: int)
    requires
        well_formed(t),
        census(t),
        0 <= k < t.n as int,
    ensures
        arr_at(t, read_key_at(t, k), t.n as int) == ev(
            &t.rhs,
            t.env,
            fill_ctr_at(t, k),
        ),
        fill_ctr_at(t, k) == cons_ctr_at(t, k),
{
    all_stored_index(t, t.n as int, k);
    assert(stored_at(t, k));
    affine_read_index(t, t.n as int, k);
    same_header_index(t, t.n as int, k);
    affine_fill_index(t, t.n as int, k);
    assert(store_key_at(t, k) == read_key_at(t, k));
    assert(store_key_at(t, k) == fill_ctr_at(t, k));
    arr_at_finds_the_store(t, read_key_at(t, k), k, t.n as int);
    assert(read_key_at(t, k) == fill_ctr_at(t, k));
    assert(fill_ctr_at(t, k) == cons_ctr_at(t, k));
}

/// L4 as a standalone obligation: the gap writes leave the forwarded
/// expression's value alone.
proof fn the_gap_does_not_change_the_expression(t: Trace)
    requires
        census(t),
    ensures
        forall|c: Val| ev(&t.rhs, apply(t.env, t.gap, 0), c) == ev(&t.rhs, t.env, c),
{
    assert forall|c: Val| ev(&t.rhs, apply(t.env, t.gap, 0), c) == ev(&t.rhs, t.env, c) by {
        ev_apply(&t.rhs, t.env, t.gap, 0, c);
    }
}

// ---------------------------------------------------------------------------
// Witnesses
// ---------------------------------------------------------------------------

/// The clean shape: F = 3*i, both loops 0,1 with a[i], every store executed,
/// no gap, no other use. Reference and fused are both [0, 3].
pub open spec fn w_ok() -> Trace {
    Trace {
        n: 2,
        fill_ctr: seq![0, 1],
        cons_ctr: seq![0, 1],
        store_key: seq![0, 1],
        read_key: seq![0, 1],
        stored: seq![true, true],
        rhs: E::Mul(bx(E::Ctr), bx(E::Const(3))),
        env: seq![],
        gap: seq![],
        other: seq![],
    }
}

/// A conditional fill: iteration 1 does not store, so a[1] is the initial 0
/// while F at 1 is 3.
pub open spec fn w_hole() -> Trace {
    Trace { stored: seq![true, false], ..w_ok() }
}

/// The store is shifted: a[i+1] = 3*i while the consumer reads a[i].
pub open spec fn w_shift_store() -> Trace {
    Trace {
        rhs: E::Add(bx(E::Ctr), bx(E::Const(1))),
        store_key: seq![1, 2],
        ..w_ok()
    }
}

/// The consumer reads a[i+1] while the fill stored a[i].
pub open spec fn w_shift_read() -> Trace {
    Trace { rhs: E::Add(bx(E::Ctr), bx(E::Const(1))), read_key: seq![1, 2], ..w_ok() }
}

/// Same shape, different header: the consumer runs at 2,3.
pub open spec fn w_header() -> Trace {
    Trace {
        rhs: E::Add(bx(E::Ctr), bx(E::Const(1))),
        cons_ctr: seq![2, 3],
        read_key: seq![2, 3],
        ..w_ok()
    }
}

/// F = x (slot 0), and a gap writes x.
pub open spec fn w_gap() -> Trace {
    Trace { rhs: E::Slot(0), env: seq![2], gap: seq![(0, 10)], ..w_ok() }
}

/// Another array read observes the element the fill would have written.
pub open spec fn w_other() -> Trace {
    Trace { other: seq![7], ..w_ok() }
}

// ---------------------------------------------------------------------------
// Completeness / non-vacuity: the verdict is a decision computed from the
// trace, not a constant.
// ---------------------------------------------------------------------------

/// A clean fill/consumer IS accepted (an identity policy is red here).
proof fn a_clean_fill_consumer_is_flagged()
    ensures
        census(w_ok()),
        !census(w_hole()),
        !census(w_shift_store()),
        !census(w_shift_read()),
        !census(w_header()),
        !census(w_gap()),
        !census(w_other()),
{
    assert(census(w_ok())) by (compute);
    assert(!all_stored(w_hole())) by (compute);
    assert(!census(w_hole()));
    assert(!affine_fill(w_shift_store())) by (compute);
    assert(!census(w_shift_store()));
    assert(!affine_read(w_shift_read())) by (compute);
    assert(!census(w_shift_read()));
    assert(!same_header(w_header())) by (compute);
    assert(!census(w_header()));
    assert(!stable(w_gap())) by (compute);
    assert(!census(w_gap()));
    assert(!alone(w_other())) by (compute);
    assert(!census(w_other()));
}

/// The accepted witness really emits the same bytes, and the forward is the
/// stored value (0 then 3).
proof fn a_clean_trace_fuses_to_the_same_bytes()
    ensures
        go(w_ok(), census(w_ok())) == seq![0 as Val, 3],
        go(w_ok(), false) == seq![0 as Val, 3],
        ref_at(w_ok(), 1) == 3,
        fused_at(w_ok(), 1) == 3,
{
    assert(census(w_ok())) by (compute);
    assert(well_formed(w_ok())) by (compute);
    the_fusion_preserves_the_observable(w_ok());
    assert(go(w_ok(), census(w_ok())) == go(w_ok(), true));
    assert(go(w_ok(), true) == seq![0 as Val, 3]) by (compute);
    assert(go(w_ok(), false) == seq![0 as Val, 3]) by (compute);
    assert(ref_at(w_ok(), 1) == 3) by (compute);
    assert(fused_at(w_ok(), 1) == 3) by (compute);
}

/// The law instantiates at every witness (no vacuous law).
proof fn law_at_the_witnesses()
    ensures
        go(w_ok(), census(w_ok())) == go(w_ok(), false),
        go(w_hole(), census(w_hole())) == go(w_hole(), false),
        go(w_shift_store(), census(w_shift_store())) == go(w_shift_store(), false),
        go(w_shift_read(), census(w_shift_read())) == go(w_shift_read(), false),
        go(w_header(), census(w_header())) == go(w_header(), false),
        go(w_gap(), census(w_gap())) == go(w_gap(), false),
        go(w_other(), census(w_other())) == go(w_other(), false),
{
    assert(well_formed(w_ok())) by (compute);
    assert(well_formed(w_hole())) by (compute);
    assert(well_formed(w_shift_store())) by (compute);
    assert(well_formed(w_shift_read())) by (compute);
    assert(well_formed(w_header())) by (compute);
    assert(well_formed(w_gap())) by (compute);
    assert(well_formed(w_other())) by (compute);
    the_fusion_preserves_the_observable(w_ok());
    the_fusion_preserves_the_observable(w_hole());
    the_fusion_preserves_the_observable(w_shift_store());
    the_fusion_preserves_the_observable(w_shift_read());
    the_fusion_preserves_the_observable(w_header());
    the_fusion_preserves_the_observable(w_gap());
    the_fusion_preserves_the_observable(w_other());
}

// ---------------------------------------------------------------------------
// Pins: each load-bearing rule is independently observable
// ---------------------------------------------------------------------------

/// `all_stored`: a hole reads the initial 0, the forward would supply 3.
proof fn a_fill_hole_would_read_the_initial_zero()
    ensures
        !census(w_hole()),
        ref_at(w_hole(), 1) == 0,
        fused_at(w_hole(), 1) == 3,
{
    assert(!all_stored(w_hole())) by (compute);
    assert(!census(w_hole()));
    assert(ref_at(w_hole(), 1) == 0) by (compute);
    assert(fused_at(w_hole(), 1) == 3) by (compute);
}

/// `affine_fill`: with the store shifted, the consumer's index was never
/// stored (0) while the forward supplies 1.
proof fn a_shifted_store_would_forward_the_wrong_value()
    ensures
        !census(w_shift_store()),
        ref_at(w_shift_store(), 0) == 0,
        fused_at(w_shift_store(), 0) == 1,
{
    assert(!affine_fill(w_shift_store())) by (compute);
    assert(!census(w_shift_store()));
    assert(ref_at(w_shift_store(), 0) == 0) by (compute);
    assert(fused_at(w_shift_store(), 0) == 1) by (compute);
}

/// `affine_read`: reading a[i+1] sees the next iteration's word, the forward
/// would supply this iteration's.
proof fn a_shifted_read_would_read_another_element()
    ensures
        !census(w_shift_read()),
        ref_at(w_shift_read(), 0) == 2,
        fused_at(w_shift_read(), 0) == 1,
{
    assert(!affine_read(w_shift_read())) by (compute);
    assert(!census(w_shift_read()));
    assert(ref_at(w_shift_read(), 0) == 2) by (compute);
    assert(fused_at(w_shift_read(), 0) == 1) by (compute);
}

/// `same_header`: the consumer at 2,3 reads an element the fill never wrote;
/// forwarding the fill-time value at the consumer's counter is a different
/// number.
proof fn a_header_mismatch_would_read_past_the_fill()
    ensures
        !census(w_header()),
        ref_at(w_header(), 0) == 0,
        fused_at(w_header(), 0) == 3,
{
    assert(!same_header(w_header())) by (compute);
    assert(!census(w_header()));
    assert(ref_at(w_header(), 0) == 0) by (compute);
    assert(fused_at(w_header(), 0) == 3) by (compute);
}

/// `stable`: the gap writes x, so the forward sees 10 where the fill stored 2.
proof fn a_gap_write_would_change_the_forward()
    ensures
        !census(w_gap()),
        ref_at(w_gap(), 0) == 2,
        fused_at(w_gap(), 0) == 10,
{
    assert(!stable(w_gap())) by (compute);
    assert(!census(w_gap()));
    assert(ref_at(w_gap(), 0) == 2) by (compute);
    assert(fused_at(w_gap(), 0) == 10) by (compute);
}

/// `alone`: the other read is 7 in the reference, 0 in the fused program.
proof fn an_extra_array_read_would_be_lost()
    ensures
        !census(w_other()),
        ref_prog(w_other()) == seq![0 as Val, 3, 7],
        fused_prog(w_other()) == seq![0 as Val, 3, 0],
        ref_prog(w_other()) != fused_prog(w_other()),
{
    assert(!alone(w_other())) by (compute);
    assert(!census(w_other()));
    assert(ref_prog(w_other()) == seq![0 as Val, 3, 7]) by (compute);
    assert(fused_prog(w_other()) == seq![0 as Val, 3, 0]) by (compute);
}

/// The forward is load-bearing: without the fusion the words are the array
/// elements; the fused words are the expression, and at k=1 they agree only
/// because the census says the store and the read are the same iteration.
proof fn the_forward_is_load_bearing()
    ensures
        ref_at(w_ok(), 0) == fused_at(w_ok(), 0),
        ref_at(w_ok(), 1) == fused_at(w_ok(), 1),
{
    assert(well_formed(w_ok())) by (compute);
    assert(census(w_ok())) by (compute);
    the_read_value_is_preserved(w_ok(), 0);
    the_read_value_is_preserved(w_ok(), 1);
}

fn main() {}

} // verus!
