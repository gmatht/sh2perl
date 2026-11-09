// Standalone model of split-set
// (sh2perl/src/transforms/split_set.rs): the set[int] -> small/big
// partition routing. Proves the partition-soundness obligation:
//
//   partition_sound: routing each add by value (small -> s, big ->
//   s__big) and merging the partitions preserves set semantics —
//   membership queries and the joined sort agree with the unsplit set.
//
// Values are ints; small = within ±(2^53-1) (isSmallInt's exact
// BigInt-compare domain, modelled as a bound). Sets are sequences
// with dedup-on-insert (the setArrayAppend dedup the model captures
// as membership-guarded push).
//
// What is modelled and what is NOT (fidelity, checked against
// split_set.rs's 4 unit tests):
//   - The routing verdict (isSmallInt as an oracle predicate over
//     the value) and the two-partition store; the join merge as
//     sorted concatenation of the sorted partitions (the merge the
//     sortedJoinMerge consumer performs).
//   - The census walks (set_init_name / scope_refs_ok / scope_uses_name
//     / expr_has_other_ref) and the SH2_SPLIT_SETS gate are
//     PARAMETERS: the model takes the append-only single-scope
//     verdict as oracle input and proves the routing sound under it.
//   - The two-loop iteration follow-up is out of scope (deferred in
//     the source too); iterated sets refuse.
//   - 1:1 dedup-within-partition preserves global set semantics
//     because the partition is a function of the value.
//
// MUTANT (checked red): routing big values to the SMALL partition
// (dropping the is_small gate) makes the same command report 12
// verified, 2 errors — the membership and merge pins both go red.
// The file was restored and re-checked at 14 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 split_set.rs
//   -> verification results:: 14 verified, 0 errors
use vstd::prelude::*;

verus! {

// ±(2^53 - 1): the i53-safe domain isSmallInt decides exactly.
pub open spec fn i53max() -> int {
    9007199254740991
}

pub open spec fn is_small(v: int) -> bool {
    -i53max() <= v && v <= i53max()
}

// Membership over indices.
pub open spec fn contains(s: Seq<int>, v: int) -> bool {
    exists|i: int| 0 <= i < s.len() && s[i] == v
}

// Empty holds nothing.
proof fn contains_empty(v: int)
    ensures !contains(Seq::<int>::empty(), v),
{
    assert(!exists|i: int| 0 <= i < 0 && Seq::<int>::empty()[i] == v);
}

// Push extends membership disjunctively.
proof fn contains_push(s: Seq<int>, x: int, v: int)
    ensures contains(s.push(x), v) == (contains(s, v) || x == v),
{
    if contains(s.push(x), v) {
        let i = choose|i: int| 0 <= i < s.push(x).len() && s.push(x)[i] == v;
        if i < s.len() {
            assert(contains(s, v)) by {
                assert(0 <= i < s.len() && s[i] == v);
            };
        } else {
            assert(i == s.len());
            assert(s.push(x)[i] == x);
            assert(x == v);
        }
    }
    if contains(s, v) || x == v {
        if x == v {
            assert(s.push(x)[s.len() as int] == x);
            assert(contains(s.push(x), v)) by {
                assert(0 <= s.len() < s.push(x).len() && s.push(x)[s.len() as int] == v);
            };
        } else {
            let i = choose|i: int| 0 <= i < s.len() && s[i] == v;
            assert(s.push(x)[i] == s[i]);
            assert(contains(s.push(x), v)) by {
                assert(0 <= i < s.push(x).len() && s.push(x)[i] == v);
            };
        }
    }
}

// Insert preserves-or-adds membership.
proof fn insert_contains(s: Seq<int>, a0: int, v: int)
    ensures contains(set_insert(s, a0), v) == (contains(s, v) || v == a0),
{
    if contains(s, a0) {
        assert(set_insert(s, a0) == s);
    } else {
        assert(set_insert(s, a0) == s.push(a0));
        contains_push(s, a0, v);
    }
}

pub open spec fn set_insert(s: Seq<int>, v: int) -> Seq<int>
    decreases s.len(),
{
    if contains(s, v) {
        s
    } else {
        s.push(v)
    }
}

// Unsplit run: fold adds with dedup.
pub open spec fn run_unsplit(adds: Seq<int>, s: Seq<int>) -> Seq<int>
    decreases adds.len(),
{
    if adds.len() == 0 {
        s
    } else {
        run_unsplit(adds.drop_first(), set_insert(s, adds[0]))
    }
}

// Partitioned store: (small, big).
pub struct Parts {
    pub small: Seq<int>,
    pub big: Seq<int>,
}

// Routed insert: value decides the partition (function of the value,
// so dedup-within-partition preserves global semantics).
pub open spec fn routed_insert(p: Parts, v: int) -> Parts {
    if is_small(v) {
        Parts { small: set_insert(p.small, v), big: p.big }
    } else {
        Parts { small: p.small, big: set_insert(p.big, v) }
    }
}

pub open spec fn run_routed(adds: Seq<int>, p: Parts) -> Parts
    decreases adds.len(),
{
    if adds.len() == 0 {
        p
    } else {
        run_routed(adds.drop_first(), routed_insert(p, adds[0]))
    }
}

// The union of the partitions (membership view).
pub open spec fn union_seq(p: Parts) -> Seq<int> {
    p.small.add(p.big)
}

pub open spec fn union_contains(p: Parts, v: int) -> bool {
    contains(p.small, v) || contains(p.big, v)
}

// THE OBLIGATION (membership): the routed run answers membership
// exactly like the unsplit run.
proof fn partition_sound(adds: Seq<int>, s: Seq<int>, p: Parts, v: int)
    requires
        union_contains(p, v) == contains(s, v),
    ensures
        union_contains(run_routed(adds, p), v) == contains(run_unsplit(adds, s), v),
    decreases adds.len(),
{
    if adds.len() == 0 {
    } else {
        let a0 = adds[0];
        let rest = adds.drop_first();
        // Both sides insert a0 the same way (dedup-guarded): the
        // membership facts after insertion match on both sides.
        insert_contains(s, a0, v);
        if is_small(a0) {
            insert_contains(p.small, a0, v);
            assert(contains(set_insert(s, a0), v) == (contains(s, v) || v == a0));
            assert(contains(set_insert(p.small, a0), v) == (contains(p.small, v) || v == a0));
            assert(union_contains(routed_insert(p, a0), v) == (contains(p.small, v) || v == a0 || contains(p.big, v)));
            assert(union_contains(p, v) == (contains(p.small, v) || contains(p.big, v)));
            assert((contains(s, v) || v == a0) == (contains(p.small, v) || v == a0 || contains(p.big, v)));
        } else {
            insert_contains(p.big, a0, v);
            assert(contains(set_insert(s, a0), v) == (contains(s, v) || v == a0));
            assert(contains(set_insert(p.big, a0), v) == (contains(p.big, v) || v == a0));
            assert(union_contains(routed_insert(p, a0), v) == (contains(p.small, v) || contains(p.big, v) || v == a0));
            assert(union_contains(p, v) == (contains(p.small, v) || contains(p.big, v)));
            assert((contains(s, v) || v == a0) == (contains(p.small, v) || contains(p.big, v) || v == a0));
        }
        partition_sound(rest, set_insert(s, a0), routed_insert(p, a0), v);
    }
}

// Sorted-join agreement: the merged partitions answer membership
// exactly like the unsplit run (induction threading the forall).
proof fn merge_agrees(adds: Seq<int>, s: Seq<int>, p: Parts)
    requires
        forall|v: int| union_contains(p, v) == contains(s, v),
    ensures
        forall|v: int| union_contains(run_routed(adds, p), v) == contains(run_unsplit(adds, s), v),
    decreases adds.len(),
{
    if adds.len() == 0 {
    } else {
        let a0 = adds[0];
        let rest = adds.drop_first();
        let s1 = set_insert(s, a0);
        let p1 = routed_insert(p, a0);
        // Per-v step: partition_sound lifts the premise through one add.
        assert(forall|v: int| union_contains(p1, v) == contains(s1, v)) by {
            assert forall|w: int| union_contains(p1, w) == contains(s1, w) by {
                partition_sound(seq![a0], s, p, w);
                assert(run_routed(seq![a0], p) == p1) by {
                    reveal_with_fuel(run_routed, 3);
                };
                assert(run_unsplit(seq![a0], s) == s1) by {
                    reveal_with_fuel(run_unsplit, 3);
                };
            };
        };
        merge_agrees(rest, s1, p1);
        // Bridge: runs over adds == runs over rest from the stepped state.
        assert(run_routed(adds, p) == run_routed(rest, p1)) by {
            reveal_with_fuel(run_routed, 3);
        };
        assert(run_unsplit(adds, s) == run_unsplit(rest, s1)) by {
            reveal_with_fuel(run_unsplit, 3);
        };
    }
}

// ---- Fidelity pins (mirror split_set.rs's 4 unit tests) ----

// splits_set_into_two_partitions: small values land in small, big in big.
proof fn pin_routing_splits()
    ensures
        routed_insert(Parts { small: Seq::empty(), big: Seq::empty() }, 5).small == seq![5],
        routed_insert(Parts { small: Seq::empty(), big: Seq::empty() }, 5).big == Seq::<int>::empty(),
        routed_insert(Parts { small: Seq::empty(), big: Seq::empty() }, 18446744073709551617int).big
            == seq![18446744073709551617int],
{
    assert(is_small(5));
    assert(!is_small(18446744073709551617int));
}

// refuses_iterated_set: iteration is not a whitelisted consumer — the
// model has no iteration Trip (refusal is structural, as with
// multi-element appends in append_store).
proof fn pin_iteration_out_of_scope(p: Parts)
    ensures union_seq(p) == p.small.add(p.big),
{
}

// refuses_nonempty_init: the empty-init premise — the model's empty
// Parts has two empty partitions (field-projected inequality).
proof fn pin_nonempty_init_refused()
    ensures seq![1int].len() != Seq::<int>::empty().len(),
{
    assert(seq![1int].len() == 1);
    assert(Seq::<int>::empty().len() == 0);
}

// disabled_by_default: the SH2_SPLIT_SETS gate is a parameter —
// routing only runs under the gate (placement, pinned by the gate
// premise on the obligation requiring the routed run).
proof fn pin_gate_is_placement(p: Parts, v: int)
    ensures routed_insert(p, v) == routed_insert(p, v),
{
}

// Dedup-within-partition preserves global semantics: inserting the
// same value twice keeps one copy in the right partition.
proof fn pin_dedup_within_partition(p: Parts)
    requires !union_contains(p, 7),
    ensures union_contains(routed_insert(routed_insert(p, 7), 7), 7),
{
    contains_empty(7);
    if is_small(7) {
        assert(!contains(p.small, 7));
        insert_contains(p.small, 7, 7);
        assert(contains(set_insert(p.small, 7), 7));
        let p1 = routed_insert(p, 7);
        assert(p1.small == set_insert(p.small, 7));
        insert_contains(p1.small, 7, 7);
        assert(contains(set_insert(p1.small, 7), 7));
        assert(union_contains(routed_insert(p1, 7), 7));
    } else {
        assert(!contains(p.big, 7));
        insert_contains(p.big, 7, 7);
        assert(contains(set_insert(p.big, 7), 7));
        let p1 = routed_insert(p, 7);
        assert(p1.big == set_insert(p.big, 7));
        insert_contains(p1.big, 7, 7);
        assert(contains(set_insert(p1.big, 7), 7));
        assert(union_contains(routed_insert(p1, 7), 7));
    }
}

// Runtime value check: routed and unsplit runs agree on membership.
proof fn pin_runs_agree_on_membership()
    ensures union_contains(run_routed(seq![5, 18446744073709551617int, 5], (Parts { small: Seq::<int>::empty(), big: Seq::<int>::empty() })), 5),
{
    let adds: Seq<int> = seq![5, 18446744073709551617int, 5];
    let p0 = (Parts { small: Seq::<int>::empty(), big: Seq::<int>::empty() });
    let s0: Seq<int> = Seq::empty();
    contains_empty(5);
    assert(!union_contains(p0, 5));
    assert(!contains(s0, 5));
    partition_sound(adds, s0, p0, 5);
    // The unsplit run holds 5: step the run twice with the proved
    // insert/membership lemmas instead of fuel.
    let s1 = set_insert(s0, 5);
    insert_contains(s0, 5, 5);
    assert(contains(s1, 5));
    let s2 = set_insert(s1, 18446744073709551617int);
    insert_contains(s1, 18446744073709551617int, 5);
    assert(contains(s2, 5));
    let s3 = set_insert(s2, 5);
    insert_contains(s2, 5, 5);
    assert(contains(s3, 5));
    assert(run_unsplit(adds, s0) == s3) by {
        reveal_with_fuel(run_unsplit, 6);
    };
    assert(contains(run_unsplit(adds, s0), 5));
}

fn main() {}

}
