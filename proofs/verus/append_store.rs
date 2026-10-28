// Standalone model of append-to-store
// (sh2perl/src/transforms/append_to_store.rs): the counted-loop rewrite
// `a = setArrayAppend(a, [v])` -> indexed store `a[i] = v`, where `i` is
// the loop counter. Proves the value-preservation obligation:
//
//   loop_sound: the rewritten loop builds the same array (contents and
//   length) as the append loop, from any empty-init start store.
//
// Names are interned to nat (as in the sibling proofs). Array VALUES are
// sequences of ints; the store maps names to array values. The loop is a
// fold over the trip sequence (its length N is the counted bound — the
// fuel the structural recursion consumes); the counter value at trip k
// equals k (init 0, +1 step, no escapes), threaded as the rewrite index.
//
// What is modelled and what is NOT (fidelity, checked against
// append_to_store.rs):
//   - The per-trip rewrite (Append(a,v) at counter k -> Store(a,k,v))
//     and the whole-loop induction (trip_sound, loop_sound_shift,
//     loop_sound): from an empty init every trip's counter equals the
//     array length so far, so each rewritten store is a push of the
//     same value the append would push.
//   - The guard conjunction as a Shape verdict (guards_ok): exactly one
//     append site, empty init, no other write, counted 0..N loop with
//     +1 step, a literal-or-const-int bound, a 1:1 single-element
//     append, no escapes. The census walks (collect, const_int_vars,
//     literal_bound, zero_init_of, counted_forinit, escapes) and the
//     traversal (transform/recurse/rewrite_list) are PARAMETERS: the
//     model takes their verdicts as oracle inputs — exactly as
//     array_len_fold takes is_plain_ident as oracle — and proves the
//     rewrite sound under them. The theorem is per counted loop, as
//     array_len_fold is per-node.
//   - Reads of the array elsewhere are allowed (lookup is never
//     restricted); writes elsewhere are refused by the no-other-write
//     premise (other_write veto).
//   - 1:1 single-element appends only (appended_value's [v] shape): a
//     multi-element append does not advance the index by one, so the
//     model has no Trip for it — refusal is structural.
//   - Guards mirror the 7 source unit tests (in-module pins, no
//     pin_mirrors.rs addition needed for per-node folds):
//       append_in_counted_loop_becomes_store -> pin_counted_loop_rewrites
//         (+ the passing verdict pin_all_guards_pass)
//       append_with_unknown_bound_keeps_growable -> pin_unknown_bound_refuses
//       two_appends_veto -> pin_two_appends_refused
//       nonempty_init_vetoes -> pin_nonempty_init_refused
//       other_write_vetoes -> pin_other_write_refused
//       break_vetoes -> pin_break_refused
//       step2_vetoes -> pin_step2_refused (guard verdict + the
//         off-counter store/push inequality: counter 2 at length 0 is
//         observably not a push)
//     plus pin_multi_element_refused (the 1:1 gate in appended_value,
//     which has no direct unit test) and pin_rewritten_loop_agrees
//     (the array_len_fold-style runtime value check over two trips).
//
// MUTANT (checked red): deleting the `!` in `!g.other_write` (the
// no-other-write premise) makes the same command report 18 verified,
// 2 errors — pin_all_guards_pass and pin_other_write_refused both go
// red, so the obligation is not vacuous. The file was restored and
// re-checked at 20 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 append_store.rs
//   -> verification results:: 20 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Name = nat;

// Array values: the store maps names to element sequences.
pub type Store = Map<Name, Seq<int>>;

pub open spec fn lookup(s: Store, n: Name) -> Seq<int> {
    if s.dom().contains(n) {
        s[n]
    } else {
        Seq::empty()
    }
}

// One loop-trip statement: the append or its indexed-store rewrite.
pub enum Trip {
    // setArrayAppend(a, [v]): element value v (the 1:1 single-element
    // shape appended_value accepts; multi-element appends have no Trip
    // — refusal is structural).
    Append { arr: Name, val: int },
    // a[k] = v: indexed store at trip index k.
    StoreAt { arr: Name, idx: nat, val: int },
    // Anything else (opaque to the rewrite).
    Other,
}

// Census verdicts as oracle inputs (collect / const_int_vars /
// literal_bound / zero_init_of / counted_forinit / escapes walks).
pub struct Shape {
    pub appends: nat,
    pub empty_init: bool,
    pub other_write: bool,
    pub counted: bool,
    pub bound_known: bool,
    pub one_to_one: bool,
    pub no_escapes: bool,
}

// The guard conjunction, mirroring transform()'s filter order: one
// append site, empty init, no other write anywhere, counted 0..N +1
// loop, literal-or-const bound, 1:1 append, no escapes.
pub open spec fn guards_ok(g: Shape) -> bool {
    g.appends == 1 && g.empty_init && !g.other_write && g.counted && g.bound_known
        && g.one_to_one && g.no_escapes
}

// The passing shape: every guard holds.
pub open spec fn full_shape() -> Shape {
    Shape {
        appends: 1,
        empty_init: true,
        other_write: false,
        counted: true,
        bound_known: true,
        one_to_one: true,
        no_escapes: true,
    }
}

// Run one trip: append pushes val; indexed store at k == current length
// pushes val (the counter-equals-trips premise, proved per-trip below);
// an off-counter store leaves the value unchanged (the rewrite never
// produces one — the counted premise rules it out). Other is identity.
pub open spec fn run_trip(t: Trip, s: Store) -> Store {
    match t {
        Trip::Append { arr, val } => s.insert(arr, lookup(s, arr).push(val)),
        Trip::StoreAt { arr, idx, val } => {
            let cur = lookup(s, arr);
            if idx == cur.len() {
                s.insert(arr, cur.push(val))
            } else {
                s.insert(arr, cur)
            }
        },
        Trip::Other => s,
    }
}

// The rewrite of one trip at trip index k (counter == k).
pub open spec fn rewrite_trip(t: Trip, k: nat) -> Trip {
    match t {
        Trip::Append { arr, val } => Trip::StoreAt { arr, idx: k, val },
        _ => t,
    }
}

// Is this trip an append to array a?
pub open spec fn is_append_to(t: Trip, a: Name) -> bool {
    match t {
        Trip::Append { arr, .. } => arr == a,
        _ => false,
    }
}

pub open spec fn append_val(t: Trip) -> int {
    match t {
        Trip::Append { val, .. } => val,
        _ => 0,
    }
}

// THE OBLIGATION, per trip: rewriting the append at trip k (counter
// reads k, array so far holds exactly the k earlier values) preserves
// the resulting store.
proof fn trip_sound(t: Trip, s: Store, k: nat, a: Name)
    requires
        is_append_to(t, a),
        lookup(s, a).len() == k,
    ensures
        run_trip(rewrite_trip(t, k), s) == run_trip(t, s),
{
    let v = append_val(t);
    match t {
        Trip::Append { arr, val } => {
            assert(arr == a);
            assert(v == val);
            assert(t == (Trip::Append { arr: a, val: v }));
            assert(rewrite_trip(t, k) == (Trip::StoreAt { arr: a, idx: k, val: v }));
            assert(run_trip(t, s) == s.insert(a, lookup(s, a).push(v)));
            assert(run_trip(rewrite_trip(t, k), s) == s.insert(a, lookup(s, a).push(v)));
        },
        _ => {
            assert(!is_append_to(t, a));
        },
    }
}

// Whole-loop run: fold the trip list (length N is the counted bound).
pub open spec fn run_loop(trips: Seq<Trip>, s: Store) -> Store
    decreases trips.len(),
{
    if trips.len() == 0 {
        s
    } else {
        run_loop(trips.drop_first(), run_trip(trips[0], s))
    }
}

// The rewritten loop: trip j rewritten with counter j. Built with
// snoc (`push` of the LAST trip onto the rewritten prefix) so the
// run_snoc lemma applies directly. `drop_last`/`last` keep index
// order: out[j] = rewrite(trips[j], k+j).
pub open spec fn rewrite_loop(trips: Seq<Trip>, k: nat) -> Seq<Trip>
    decreases trips.len(),
{
    if trips.len() == 0 {
        Seq::empty()
    } else {
        rewrite_loop(trips.drop_last(), k).push(rewrite_trip(trips.last(), (k + trips.drop_last().len()) as nat))
    }
}

// Snoc lemma: running a pushed sequence = run the prefix, then the last.
proof fn run_loop_snoc(ts: Seq<Trip>, x: Trip, s: Store)
    ensures run_loop(ts.push(x), s) == run_trip(x, run_loop(ts, s)),
    decreases ts.len(),
{
    assert(ts.push(x).len() == ts.len() + 1);
    if ts.len() == 0 {
        assert(ts.push(x)[0] == x);
        assert(ts.push(x).drop_first() == Seq::<Trip>::empty());
        assert(run_loop(ts.push(x), s) == run_loop(Seq::<Trip>::empty(), run_trip(x, s))) by {
            reveal_with_fuel(run_loop, 3);
        };
    } else {
        assert(ts.push(x).drop_first() == ts.drop_first().push(x));
        assert(ts.push(x)[0] == ts[0]);
        run_loop_snoc(ts.drop_first(), x, run_trip(ts[0], s));
    }
}

// Snoc split (the rewrite_loop build shape): dropping the last trip
// commutes with the rewrite at the matching counter.
proof fn rewrite_loop_snoc(pre: Seq<Trip>, last: Trip, k: nat)
    ensures rewrite_loop(pre.push(last), k) == rewrite_loop(pre, k).push(rewrite_trip(last, (k + pre.len()) as nat)),
{
    assert(pre.push(last).len() == pre.len() + 1);
    assert(pre.push(last).drop_last() == pre);
    assert(pre.push(last).last() == last);
}

proof fn run_loop_cons(h: Trip, t: Seq<Trip>, s: Store)
    ensures run_loop(seq![h].add(t), s) == run_loop(t, run_trip(h, s)),
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

// Length invariant: running append-trips from length k yields length
// k + n (each append pushes exactly one).
proof fn run_loop_len(trips: Seq<Trip>, s: Store, a: Name, k: nat)
    requires
        forall|j: int| 0 <= j < trips.len() ==> is_append_to(trips[j], a),
        lookup(s, a).len() == k,
    ensures
        lookup(run_loop(trips, s), a).len() == (k + trips.len()) as nat,
    decreases trips.len(),
{
    if trips.len() == 0 {
    } else {
        run_loop_cons(trips[0], trips.drop_first(), s);
        assert(is_append_to(trips[0], a));
        assert(trips[0] == (Trip::Append { arr: a, val: append_val(trips[0]) }));
        assert(run_trip(trips[0], s) == s.insert(a, lookup(s, a).push(append_val(trips[0]))));
        assert(lookup(run_trip(trips[0], s), a).len() == (k + 1) as nat);
        assert(forall|j: int| 0 <= j < trips.drop_first().len() ==> is_append_to(trips.drop_first()[j], a));
        run_loop_len(trips.drop_first(), run_trip(trips[0], s), a, (k + 1) as nat);
    }
}

// Shifted induction: trips rewritten from counter k over an array of
// length k run exactly like the append loop.
proof fn loop_sound_shift(trips: Seq<Trip>, s: Store, a: Name, k: nat)
    requires
        forall|j: int| 0 <= j < trips.len() ==> is_append_to(trips[j], a),
        lookup(s, a).len() == k,
    ensures
        run_loop(rewrite_loop(trips, k), s) == run_loop(trips, s),
    decreases trips.len(),
{
    if trips.len() == 0 {
    } else {
        let pre = trips.drop_last();
        let last = trips.last();
        assert(trips == pre.push(last));
        assert(pre.len() + 1 == trips.len());
        rewrite_loop_snoc(pre, last, k);
        run_loop_snoc(pre, last, s);
        assert(forall|j: int| 0 <= j < pre.len() ==> is_append_to(pre[j], a));
        loop_sound_shift(pre, s, a, k);
        run_loop_len(pre, s, a, k);
        let s_pre = run_loop(pre, s);
        assert(run_loop(rewrite_loop(pre, k), s) == s_pre);
        assert(lookup(s_pre, a).len() == (k + pre.len()) as nat);
        let kk = (k + pre.len()) as nat;
        assert(lookup(s_pre, a).len() == kk);
        assert(is_append_to(last, a));
        trip_sound(last, s_pre, kk, a);
        assert(trips[trips.len() - 1] == last);
        run_loop_snoc(rewrite_loop(pre, k), rewrite_trip(last, kk), s);
        assert(run_loop(rewrite_loop(trips, k), s) == run_trip(rewrite_trip(last, kk), s_pre));
        assert(run_trip(rewrite_trip(last, kk), s_pre) == run_trip(last, s_pre));
    }
}

// Empty-init corollary: the rewritten loop agrees with the append loop.
proof fn loop_sound(trips: Seq<Trip>, s: Store, a: Name)
    requires
        forall|j: int| 0 <= j < trips.len() ==> is_append_to(trips[j], a),
        lookup(s, a).len() == 0,
    ensures
        run_loop(rewrite_loop(trips, 0), s) == run_loop(trips, s),
{
    loop_sound_shift(trips, s, a, 0);
}

// ---- Fidelity pins (mirror append_to_store.rs's 7 unit tests) ----

// append_in_counted_loop_becomes_store: the trip rewrite fires.
proof fn pin_counted_loop_rewrites()
    ensures rewrite_trip((Trip::Append { arr: 3nat, val: 5 }), 2) == (Trip::StoreAt { arr: 3nat, idx: 2nat, val: 5 }),
{
}

// The passing verdict: every guard holds, so the loop folds.
proof fn pin_all_guards_pass()
    ensures guards_ok(full_shape()),
{
}

// append_with_unknown_bound_keeps_growable: an unknown bound refuses.
proof fn pin_unknown_bound_refuses()
    ensures !guards_ok(Shape {
        appends: 1,
        empty_init: true,
        other_write: false,
        counted: true,
        bound_known: false,
        one_to_one: true,
        no_escapes: true,
    }),
{
}

// two_appends_veto: two sites refuse.
proof fn pin_two_appends_refused()
    ensures !guards_ok(Shape {
        appends: 2,
        empty_init: true,
        other_write: false,
        counted: true,
        bound_known: true,
        one_to_one: true,
        no_escapes: true,
    }),
{
}

// nonempty_init_vetoes.
proof fn pin_nonempty_init_refused()
    ensures !guards_ok(Shape {
        appends: 1,
        empty_init: false,
        other_write: false,
        counted: true,
        bound_known: true,
        one_to_one: true,
        no_escapes: true,
    }),
{
}

// other_write_vetoes.
proof fn pin_other_write_refused()
    ensures !guards_ok(Shape {
        appends: 1,
        empty_init: true,
        other_write: true,
        counted: true,
        bound_known: true,
        one_to_one: true,
        no_escapes: true,
    }),
{
}

// break_vetoes: an escape desynchronises the counter.
proof fn pin_break_refused()
    ensures !guards_ok(Shape {
        appends: 1,
        empty_init: true,
        other_write: false,
        counted: true,
        bound_known: true,
        one_to_one: true,
        no_escapes: false,
    }),
{
}

// step2_vetoes: a +2 step is not the counted +1 shape, and the
// off-counter store it would emit is observably not a push.
proof fn pin_step2_refused(s: Store)
    requires lookup(s, 3nat).len() == 0,
    ensures
        !guards_ok(Shape {
            appends: 1,
            empty_init: true,
            other_write: false,
            counted: false,
            bound_known: true,
            one_to_one: true,
            no_escapes: true,
        }),
        lookup(run_trip(Trip::StoreAt { arr: 3nat, idx: 2nat, val: 7 }, s), 3nat).len()
            != lookup(run_trip(Trip::Append { arr: 3nat, val: 7 }, s), 3nat).len(),
{
    assert(run_trip(Trip::StoreAt { arr: 3nat, idx: 2nat, val: 7 }, s) == s.insert(3nat, lookup(s, 3nat)));
    assert(run_trip(Trip::Append { arr: 3nat, val: 7 }, s) == s.insert(3nat, lookup(s, 3nat).push(7)));
    assert(lookup(s.insert(3nat, lookup(s, 3nat)), 3nat) == lookup(s, 3nat));
    assert(lookup(s.insert(3nat, lookup(s, 3nat).push(7)), 3nat) == lookup(s, 3nat).push(7));
    assert(lookup(s, 3nat).len() == 0);
    assert(lookup(s, 3nat).push(7).len() == 1);
}

// No direct unit test: appended_value only accepts the single-element
// [v] shape — a multi-element append does not advance the index by one.
proof fn pin_multi_element_refused()
    ensures !guards_ok(Shape {
        appends: 1,
        empty_init: true,
        other_write: false,
        counted: true,
        bound_known: true,
        one_to_one: false,
        no_escapes: true,
    }),
{
}

// Runtime value check (the array_len_fold pin_folded_counts_elements
// shape): two trips build the same array on both sides.
proof fn pin_rewritten_loop_agrees(s: Store)
    requires lookup(s, 3nat).len() == 0,
    ensures
        run_loop(
            rewrite_loop(
                seq![Trip::Append { arr: 3nat, val: 1 }, Trip::Append { arr: 3nat, val: 4 }],
                0,
            ),
            s,
        ) == run_loop(seq![Trip::Append { arr: 3nat, val: 1 }, Trip::Append { arr: 3nat, val: 4 }], s),
{
    let trips: Seq<Trip> = seq![Trip::Append { arr: 3nat, val: 1 }, Trip::Append { arr: 3nat, val: 4 }];
    assert(is_append_to(trips[0], 3nat));
    assert(is_append_to(trips[1], 3nat));
    assert(trips.len() == 2);
    assert(forall|j: int| 0 <= j < trips.len() ==> is_append_to(trips[j], 3nat));
    loop_sound(trips, s, 3nat);
}

fn main() {}

}
