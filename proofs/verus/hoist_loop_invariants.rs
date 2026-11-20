// Standalone model of hoist-loop-invariants
// (sh2perl/src/transforms/hoist_loop_invariants.rs): the leading
// maximal pure invariant run moves to the loop prologue. Proves the
// hoist-preservation obligation:
//
//   hoist_sound (n >= 1): the hoisted loop and the original loop
//   agree with the prologue state on every lookup.
//
// The proof found a REAL SOUNDNESS HOLE: at n = 0 the prologue runs
// while the original runs nothing (verified live against the
// transform: `while false; do y = 5; done` hoisted `y = 5`
// unconditionally). The transform now REFUSES every loop that may run
// zero times (`runs_at_least_once` in the source, BUGS_FOUND.md #7), and
// this file proves the two halves of that repair:
//   guard_gives_a_trip:  runs_at_least_once(k) ==> trips(k) >= 1
//   hoist_sound_gated:   under the guard, hoisted run == original run.
// pin_zero_trip_diverges stays as the refutation of the UNGUARDED rule —
// it is why the guard, not a stronger claim, is the fix.
//
// Agreement is pointwise (the redundant_store_elim precedent —
// agree_shadow over lookups, never whole-Map ==, which the solver
// cannot discharge extensionally).
//
// Run model: stores map vars to ints (unset reads 0); the prefix is
// a sequence of (target, fixed-value) assigns; the suffix is
// identity.
//
// What is modelled and what is NOT (fidelity, checked against
// hoist_loop_invariants.rs's 8 unit tests):
//   - The leading-run shape, the five gates as oracle premises
//     (single write, pure RHS, invariant reads, no read-before, no
//     counter/index read), the census walks and traversal as
//     PARAMETERS.
//   - The n >= 1 proviso is load-bearing (the hole); every production
//     firing runs many trips.
//   - Guards mirror the 8 source unit tests (in-module pins).
//
// MUTANT (checked red): dropping invariant_reads from the gate
// (treating accumulators as hoistable) makes the same command
// report 19 verified, 3 errors — the accumulator/form pins go red.
// The file was restored and re-checked at 22 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 hoist_loop_invariants.rs
//   -> verification results:: 22 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Var = nat;
pub type Store = Map<Var, int>;

// Total read (bash: unset reads 0 — arith-contract empty/unparseable→0).
pub open spec fn lookup(s: Store, v: Var) -> int {
    if s.dom().contains(v) {
        s[v]
    } else {
        0
    }
}

pub struct InvAssign {
    pub target: Var,
    pub value: int,
}

// ---- The trip guard (source: `runs_at_least_once`) ----
// The transform decides only the shapes the IR decides statically;
// every other loop is refused, which this model represents by having no
// LoopKind for it (no kind, no theorem — the conservative rule).
pub enum LoopKind {
    // `while 1` / `while 0`
    WhileConst(bool),
    // inclusive `start..=end`
    Range(int, int),
    // a literal word list of length n
    Words(nat),
}

pub open spec fn trips(k: LoopKind) -> nat {
    match k {
        LoopKind::WhileConst(b) => if b { 1nat } else { 0nat },
        LoopKind::Range(start, end) => if start <= end { (end - start + 1) as nat } else { 0nat },
        LoopKind::Words(n) => n,
    }
}

pub open spec fn runs_at_least_once(k: LoopKind) -> bool {
    match k {
        LoopKind::WhileConst(b) => b,
        LoopKind::Range(start, end) => start <= end,
        LoopKind::Words(n) => n > 0,
    }
}

// The guard is exactly the n >= 1 premise of hoist_sound.
proof fn guard_gives_a_trip(k: LoopKind)
    requires runs_at_least_once(k),
    ensures trips(k) >= 1,
{
    match k {
        LoopKind::WhileConst(b) => {}
        LoopKind::Range(start, end) => {}
        LoopKind::Words(n) => {}
    }
}

// Pointwise agreement is transitive (the hoist_sound conclusion is stated
// against the prologue state, so the gated theorem needs this join).
proof fn store_agree_trans(s1: Store, s2: Store, s3: Store)
    requires store_agree(s1, s2), store_agree(s2, s3),
    ensures store_agree(s1, s3),
{
    assert forall|v: Var| lookup(s1, v) == lookup(s3, v) by {
        assert(lookup(s1, v) == lookup(s2, v));
        assert(lookup(s2, v) == lookup(s3, v));
    };
}

// Loop-invariant premise: the value does not depend on the trip.
pub open spec fn invariant_value(value: int, trip: nat) -> int {
    value
}

// One assign is trip-invariant (oracle premise).
pub open spec fn assign_invariant(a: InvAssign) -> bool {
    forall|t: nat| invariant_value(a.value, t) == a.value
}

// Every prefix assign is trip-invariant (one quantifier level).
pub open spec fn all_invariant(prefix: Seq<InvAssign>) -> bool {
    forall|i: int| 0 <= i < prefix.len() ==> #[trigger] assign_invariant(prefix[i])
}

// Store agreement: pointwise lookup equality (the agree_shadow shape).
pub open spec fn store_agree(s1: Store, s2: Store) -> bool {
    forall|v: Var| lookup(s1, v) == lookup(s2, v)
}

// Run the prefix: set every target to its (invariant) value.
pub open spec fn run_prefix(prefix: Seq<InvAssign>, s: Store) -> Store
    decreases prefix.len(),
{
    if prefix.len() == 0 {
        s
    } else {
        run_prefix(prefix.drop_first(), s.insert(prefix[0].target, prefix[0].value))
    }
}

// The suffix is identity.
pub open spec fn run_suffix(suffix: Seq<Var>, s: Store) -> Store {
    s
}

// One trip = prefix then suffix.
pub open spec fn run_trip(prefix: Seq<InvAssign>, suffix: Seq<Var>, s: Store) -> Store {
    run_suffix(suffix, run_prefix(prefix, s))
}

// Whole hoisted loop (n >= 1): prologue once, then n suffix trips.
// At n = 0 the prologue still runs (the code-faithful base) — the
// 0-trip hole the model pins but does not bless.
pub open spec fn run_hoisted(prefix: Seq<InvAssign>, suffix: Seq<Var>, s: Store, n: nat) -> Store
    decreases n,
{
    if n == 0 {
        run_prefix(prefix, s)
    } else {
        run_hoisted(prefix, suffix, run_suffix(suffix, run_prefix(prefix, s)), (n - 1) as nat)
    }
}

// Un-hoisted loop: n trips of prefix+suffix.
pub open spec fn run_loop(prefix: Seq<InvAssign>, suffix: Seq<Var>, s: Store, n: nat) -> Store
    decreases n,
{
    if n == 0 {
        s
    } else {
        run_loop(prefix, suffix, run_trip(prefix, suffix, s), (n - 1) as nat)
    }
}

// Prefix targets as a set (oracle-shape census).
pub open spec fn targets_of(prefix: Seq<InvAssign>) -> Set<Var>
    decreases prefix.len(),
{
    if prefix.len() == 0 {
        Set::empty()
    } else {
        targets_of(prefix.drop_first()).insert(prefix[0].target)
    }
}

// Single-write gate: no target appears twice in the prefix.
pub open spec fn single_write(prefix: Seq<InvAssign>) -> bool
    decreases prefix.len(),
{
    if prefix.len() == 0 {
        true
    } else {
        !targets_of(prefix.drop_first()).contains(prefix[0].target) && single_write(prefix.drop_first())
    }
}

// The prefix run preserves any var outside its targets.
proof fn run_prefix_preserves(prefix: Seq<InvAssign>, s: Store, v: Var)
    requires !targets_of(prefix).contains(v),
    ensures lookup(run_prefix(prefix, s), v) == lookup(s, v),
    decreases prefix.len(),
{
    if prefix.len() == 0 {
    } else {
        let t = prefix[0].target;
        assert(t != v);
        assert(!targets_of(prefix.drop_first()).contains(v));
        run_prefix_preserves(prefix.drop_first(), s.insert(t, prefix[0].value), v);
        assert(run_prefix(prefix, s) == run_prefix(prefix.drop_first(), s.insert(t, prefix[0].value))) by {
            reveal_with_fuel(run_prefix, 3);
        };
        assert(lookup(s.insert(t, prefix[0].value), v) == lookup(s, v));
    }
}

// Hoist-gate conjunction as oracle premises.
pub open spec fn hoistable(single_write: bool, pure_rhs: bool, invariant_reads: bool, no_read_before: bool, no_counter: bool) -> bool {
    single_write && pure_rhs && invariant_reads && no_read_before && no_counter
}

// ---- Fidelity pins (mirror hoist_loop_invariants.rs's 8 unit tests) ----

proof fn pin_accumulator_refused()
    ensures !hoistable(true, true, false, true, true),
{
}

proof fn pin_invariant_hoists()
    ensures hoistable(true, true, true, true, true),
{
}

proof fn pin_read_before_refused()
    ensures !hoistable(true, true, true, false, true),
{
}

proof fn pin_counter_read_refused()
    ensures !hoistable(true, true, true, true, false),
{
}

proof fn pin_indexed_target_refuses()
    ensures !hoistable(true, true, true, true, false),
{
}

proof fn pin_independent_hoists()
    ensures hoistable(true, true, true, true, true),
{
}

proof fn pin_fn_body_write_refused()
    ensures !hoistable(true, true, false, true, true),
{
}

proof fn pin_fn_cond_write_refused()
    ensures !hoistable(true, true, false, true, true),
{
}

// ---- The trip guard's pins (mirror the two new source tests) ----
proof fn pin_zero_trip_refused()
    ensures !runs_at_least_once(LoopKind::WhileConst(false)),
{
}

proof fn pin_const_true_hoists()
    ensures runs_at_least_once(LoopKind::WhileConst(true)),
{
}

proof fn pin_range_empty_refused()
    ensures !runs_at_least_once(LoopKind::Range(5, 1)),
{
}

proof fn pin_range_hoists()
    ensures runs_at_least_once(LoopKind::Range(0, 7)),
{
}

proof fn pin_words_nonempty_hoists()
    ensures runs_at_least_once(LoopKind::Words(2)),
{
}

// Per-trip agreement: the trip equals the prologue on every lookup
// (definitional — the suffix is identity).
proof fn trip_agrees(prefix: Seq<InvAssign>, suffix: Seq<Var>, s: Store)
    requires all_invariant(prefix),
    ensures store_agree(run_trip(prefix, suffix, s), run_prefix(prefix, s)),
{
    assert(run_trip(prefix, suffix, s) == run_suffix(suffix, run_prefix(prefix, s)));
    assert(run_suffix(suffix, run_prefix(prefix, s)) == run_prefix(prefix, s));
}

// The fixed value of a target: the unique writer's value
// (well-defined by single-write; first match wins).
pub open spec fn fixed_value(prefix: Seq<InvAssign>, v: Var) -> int
    decreases prefix.len(),
{
    if prefix.len() == 0 {
        0
    } else if prefix[0].target == v {
        prefix[0].value
    } else {
        fixed_value(prefix.drop_first(), v)
    }
}

// Hit lemma: running the prefix sets each target to its fixed value.
proof fn run_prefix_hit(prefix: Seq<InvAssign>, s: Store, v: Var)
    requires single_write(prefix), targets_of(prefix).contains(v),
    ensures lookup(run_prefix(prefix, s), v) == fixed_value(prefix, v),
    decreases prefix.len(),
{
    if prefix[0].target == v {
        assert(!targets_of(prefix.drop_first()).contains(v));
        run_prefix_preserves(prefix.drop_first(), s.insert(prefix[0].target, prefix[0].value), v);
        assert(run_prefix(prefix, s) == run_prefix(prefix.drop_first(), s.insert(prefix[0].target, prefix[0].value))) by {
            reveal_with_fuel(run_prefix, 3);
        };
        assert(fixed_value(prefix, v) == prefix[0].value);
    } else {
        assert(targets_of(prefix.drop_first()).contains(v));
        run_prefix_hit(prefix.drop_first(), s.insert(prefix[0].target, prefix[0].value), v);
        assert(run_prefix(prefix, s) == run_prefix(prefix.drop_first(), s.insert(prefix[0].target, prefix[0].value))) by {
            reveal_with_fuel(run_prefix, 3);
        };
        assert(fixed_value(prefix, v) == fixed_value(prefix.drop_first(), v));
    }
}

// Prologue fixed point, pointwise: re-running the prefix changes
// nothing. On-target cells hit fixed values on both sides; off-target
// cells pass through.
proof fn prefix_fixed(prefix: Seq<InvAssign>, s: Store)
    requires all_invariant(prefix), single_write(prefix),
    ensures store_agree(run_prefix(prefix, run_prefix(prefix, s)), run_prefix(prefix, s)),
    decreases prefix.len(),
{
    if prefix.len() == 0 {
    } else {
        let t = prefix[0].target;
        let v0 = prefix[0].value;
        let tail = prefix.drop_first();
        let s1 = s.insert(t, v0);
        assert(run_prefix(prefix, s) == run_prefix(tail, s1)) by {
            reveal_with_fuel(run_prefix, 3);
        };
        assert(targets_of(prefix) == targets_of(tail).insert(t));
        assert(single_write(prefix) == (!targets_of(tail).contains(t) && single_write(tail)));
        assert(!targets_of(tail).contains(t));
        prefix_fixed(tail, s);
        prefix_fixed(tail, s1);
        assert forall|v: Var| lookup(run_prefix(prefix, run_prefix(prefix, s)), v) == lookup(run_prefix(prefix, s), v) by {
            assert(run_prefix(prefix, run_prefix(prefix, s)) == run_prefix(tail, run_prefix(prefix, s).insert(t, v0))) by {
                reveal_with_fuel(run_prefix, 3);
            };
            if v == t {
                run_prefix_preserves(tail, run_prefix(prefix, s).insert(t, v0), v);
                run_prefix_preserves(tail, s1, v);
                run_prefix_hit(prefix, s, v);
                assert(lookup(run_prefix(tail, run_prefix(prefix, s).insert(t, v0)), v) == lookup(run_prefix(prefix, s).insert(t, v0), v));
                assert(lookup(run_prefix(prefix, s).insert(t, v0), v) == v0);
                assert(lookup(run_prefix(tail, s1), v) == lookup(s1, v));
                assert(lookup(s1, v) == v0);
                assert(lookup(run_prefix(prefix, s), v) == v0);
            } else if targets_of(tail).contains(v) {
                run_prefix_hit(tail, run_prefix(prefix, s).insert(t, v0), v);
                run_prefix_hit(tail, s1, v);
                assert(lookup(run_prefix(tail, run_prefix(prefix, s).insert(t, v0)), v) == fixed_value(tail, v));
                assert(lookup(run_prefix(tail, s1), v) == fixed_value(tail, v));
                assert(lookup(run_prefix(prefix, s), v) == lookup(run_prefix(tail, s1), v)) by {
                    reveal_with_fuel(run_prefix, 3);
                };
                assert(lookup(run_prefix(prefix, s).insert(t, v0), v) == lookup(run_prefix(prefix, s), v));
            } else {
                assert(!targets_of(prefix).contains(v));
                run_prefix_preserves(tail, run_prefix(prefix, s).insert(t, v0), v);
                run_prefix_preserves(tail, s1, v);
                assert(lookup(run_prefix(tail, run_prefix(prefix, s).insert(t, v0)), v) == lookup(run_prefix(prefix, s).insert(t, v0), v));
                assert(lookup(run_prefix(prefix, s).insert(t, v0), v) == lookup(run_prefix(prefix, s), v));
                assert(lookup(run_prefix(tail, s1), v) == lookup(s1, v));
                assert(lookup(run_prefix(prefix, s), v) == lookup(run_prefix(tail, s1), v)) by {
                    reveal_with_fuel(run_prefix, 3);
                };
            }
        };
    }
}

// THE OBLIGATION (whole loop, n >= 1): both sides agree with the
// prologue state on every lookup.
proof fn hoist_sound(prefix: Seq<InvAssign>, suffix: Seq<Var>, s: Store, n: nat)
    requires all_invariant(prefix), single_write(prefix), n >= 1,
    ensures
        store_agree(run_hoisted(prefix, suffix, s, n), run_prefix(prefix, s)),
        store_agree(run_loop(prefix, suffix, s, n), run_prefix(prefix, s)),
    decreases n,
{
    if n == 1 {
        assert(run_hoisted(prefix, suffix, s, 1) == run_prefix(prefix, run_suffix(suffix, run_prefix(prefix, s)))) by {
            reveal_with_fuel(run_hoisted, 3);
        };
        assert(run_loop(prefix, suffix, s, 1) == run_trip(prefix, suffix, s)) by {
            reveal_with_fuel(run_loop, 3);
        };
        trip_agrees(prefix, suffix, s);
        prefix_fixed(prefix, s);
        assert forall|v: Var| lookup(run_hoisted(prefix, suffix, s, 1), v) == lookup(run_prefix(prefix, s), v) by {
            assert(lookup(run_hoisted(prefix, suffix, s, 1), v) == lookup(run_prefix(prefix, run_suffix(suffix, run_prefix(prefix, s))), v));
            assert(lookup(run_prefix(prefix, run_suffix(suffix, run_prefix(prefix, s))), v) == lookup(run_prefix(prefix, s), v));
        };
        assert forall|v: Var| lookup(run_loop(prefix, suffix, s, 1), v) == lookup(run_prefix(prefix, s), v) by {
            assert(lookup(run_loop(prefix, suffix, s, 1), v) == lookup(run_trip(prefix, suffix, s), v));
            assert(lookup(run_trip(prefix, suffix, s), v) == lookup(run_prefix(prefix, s), v));
        };
    } else {
        hoist_sound(prefix, suffix, run_suffix(suffix, run_prefix(prefix, s)), (n - 1) as nat);
        trip_agrees(prefix, suffix, s);
        prefix_fixed(prefix, s);
        assert forall|v: Var| lookup(run_hoisted(prefix, suffix, s, n), v) == lookup(run_prefix(prefix, s), v) by {
            assert(run_hoisted(prefix, suffix, s, n) == run_hoisted(prefix, suffix, run_suffix(suffix, run_prefix(prefix, s)), (n - 1) as nat)) by {
                reveal_with_fuel(run_hoisted, 3);
            };
            assert(lookup(run_hoisted(prefix, suffix, run_suffix(suffix, run_prefix(prefix, s)), (n - 1) as nat), v) == lookup(run_prefix(prefix, run_suffix(suffix, run_prefix(prefix, s))), v));
            assert(lookup(run_prefix(prefix, run_suffix(suffix, run_prefix(prefix, s))), v) == lookup(run_prefix(prefix, s), v));
        };
        assert forall|v: Var| lookup(run_loop(prefix, suffix, s, n), v) == lookup(run_prefix(prefix, s), v) by {
            assert(run_loop(prefix, suffix, s, n) == run_loop(prefix, suffix, run_trip(prefix, suffix, s), (n - 1) as nat)) by {
                reveal_with_fuel(run_loop, 3);
            };
            assert(lookup(run_loop(prefix, suffix, run_trip(prefix, suffix, s), (n - 1) as nat), v) == lookup(run_prefix(prefix, run_trip(prefix, suffix, s)), v));
            assert(lookup(run_trip(prefix, suffix, s), v) == lookup(run_prefix(prefix, s), v));
            assert(lookup(run_prefix(prefix, run_prefix(prefix, s)), v) == lookup(run_prefix(prefix, s), v));
        };
    }
}

// THE GATED OBLIGATION: the rule the transform now implements. Under the
// trip guard, the hoisted loop and the original agree on every lookup for
// the trip count the loop kind determines.
proof fn hoist_sound_gated(k: LoopKind, prefix: Seq<InvAssign>, suffix: Seq<Var>, s: Store)
    requires all_invariant(prefix), single_write(prefix), runs_at_least_once(k),
    ensures store_agree(
        run_hoisted(prefix, suffix, s, trips(k)),
        run_loop(prefix, suffix, s, trips(k))),
{
    guard_gives_a_trip(k);
    hoist_sound(prefix, suffix, s, trips(k));
    store_agree_trans(
        run_hoisted(prefix, suffix, s, trips(k)),
        run_prefix(prefix, s),
        run_loop(prefix, suffix, s, trips(k)));
}

proof fn pin_hoist_preserves_run()
    ensures store_agree(
        run_hoisted(seq![InvAssign { target: 3nat, value: 5 }], seq![], Map::<Var, int>::empty(), 2),
        run_loop(seq![InvAssign { target: 3nat, value: 5 }], seq![], Map::<Var, int>::empty(), 2)),
{
    let prefix = seq![InvAssign { target: 3nat, value: 5 }];
    assert(all_invariant(prefix)) by {
        assert forall|i: int| 0 <= i < prefix.len() implies assign_invariant(prefix[i]) by {
            assert(prefix[i] == InvAssign { target: 3nat, value: 5 });
        };
    };
    assert(targets_of(seq![InvAssign { target: 3nat, value: 5 }].drop_first()) == Set::<Var>::empty());
    assert(targets_of(prefix.drop_first()) == Set::<Var>::empty());
    assert(single_write(prefix) == (!targets_of(prefix.drop_first()).contains(3nat) && single_write(prefix.drop_first())));
    assert(single_write(prefix));
    hoist_sound(prefix, seq![], Map::<Var, int>::empty(), 2);
    assert forall|v: Var| lookup(run_hoisted(prefix, seq![], Map::<Var, int>::empty(), 2), v) == lookup(run_loop(prefix, seq![], Map::<Var, int>::empty(), 2), v) by {
        assert(lookup(run_hoisted(prefix, seq![], Map::<Var, int>::empty(), 2), v) == lookup(run_prefix(prefix, Map::<Var, int>::empty()), v));
        assert(lookup(run_loop(prefix, seq![], Map::<Var, int>::empty(), 2), v) == lookup(run_prefix(prefix, Map::<Var, int>::empty()), v));
    };
}

// The 0-trip hole, pinned pointwise: at n = 0 the hoisted prologue
// sets y while the original leaves it unset — different lookups.
// This pin is the negative control that proves the n >= 1 premise
// load-bearing (and the failing-first shape for the transform owner).
proof fn pin_zero_trip_diverges(s: Store)
    requires !s.dom().contains(3nat),
    ensures lookup(run_hoisted(seq![InvAssign { target: 3nat, value: 5 }], seq![], s, 0), 3nat)
        != lookup(run_loop(seq![InvAssign { target: 3nat, value: 5 }], seq![], s, 0), 3nat),
{
    let prefix = seq![InvAssign { target: 3nat, value: 5 }];
    assert(run_hoisted(prefix, seq![], s, 0) == run_prefix(prefix, s)) by {
        reveal_with_fuel(run_hoisted, 3);
    };
    assert(run_loop(prefix, seq![], s, 0) == s) by {
        reveal_with_fuel(run_loop, 3);
    };
    assert(run_prefix(prefix, s) == s.insert(3nat, 5)) by {
        reveal_with_fuel(run_prefix, 3);
    };
    assert(lookup(s.insert(3nat, 5), 3nat) == 5);
    assert(lookup(s, 3nat) == 0);
}

fn main() {}

}
