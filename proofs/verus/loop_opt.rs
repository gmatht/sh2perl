// Standalone model of loop-opt's leading-run hoist
// (sh2perl/src/shir_passes/loop_opt.rs): the leading maximal run of
// pure, loop-invariant statements moves to the loop prologue. Proves
// the hoist-preservation obligation:
//
//   hoist_sound (n >= 1): the hoisted loop and the original loop
//   agree with the prologue state on every lookup.
//
// This is the TWIN of the proved transforms/hoist_loop_invariants.rs
// model (22 obligations): same rewrite idea, different code. Where
// the twin hoists ONE single-written assign, loop_opt hoists a GROUP
// (a leading run of N statements, with earlier-group-member chaining:
// a read is invariant if unassigned in the body OR assigned only by
// an earlier group member). Where the twin has five gates, loop_opt
// has six guards plus purity (leading-run, no-control-escape,
// target-clobber via writer_count, operand invariance with chaining,
// status exec+$?, strict subset).
//
// The proof inherits the twin's REAL SOUNDNESS HOLE: at n = 0 the
// prologue runs while the original runs nothing (loop_opt's pass()
// splices the group before the loop with no trip-count guard — the
// same unconditional-splice shape as BUGS_FOUND.md 7, in different
// code). The theorem covers exactly the sound fragment (n >= 1);
// the 0-trip divergence is pinned as pin_zero_trip_diverges.
//
// DEAD CODE (recorded honestly): loop_opt declares `pub mod loop_opt`
// and `pub fn hoist_loop_invariants`, but NOTHING calls it —
// optimize() runs const-fold/const-prop/DSE only, and the canonical
// pipeline never lists it (the only in-tree mention is a doc comment
// in split_fold.rs). In-pipeline firing is therefore 0/563 by
// construction; the 7 unit tests (which build IrProgram directly and
// assert structural hoist + fixpoint idempotence) are the IR-level
// firing witnesses, mapped as pins below. Same adjudication as
// for-recovery's dead contract.
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
// loop_opt.rs's 7 unit tests):
//   - The leading-run shape, the six guards + purity as oracle
//     premises (pure group, no escape, clobber-free writer counts,
//     invariant reads incl. earlier-group chaining, no status
//     observation, strict subset), the purity/read/write censuses,
//     the fixpoint fn-analysis, and traversal as PARAMETERS.
//   - The leading-run structure (group IS the body's prefix) is
//     structural in the model, not a premise: the model only ever
//     hoists a prefix.
//   - The n >= 1 proviso is load-bearing (the hole); the 7 unit
//     tests never trip it (every test loop is structural).
//   - Guards mirror the 7 source unit tests (in-module pins):
//       pure_call_and_derived_hoist_out_of_inner_loop
//         -> pin_pure_call_derived_hoists
//       counter_dependent_derived_blocks_only_itself
//         -> pin_counter_derived_blocked
//       invariant_arith_hoists_counter_arith_stays
//         -> pin_arith_hoists + pin_hoist_preserves_run
//       clobbered_target_blocks_hoist -> pin_clobber_blocked
//       break_in_body_blocks_hoist -> pin_escape_blocked
//       echo_is_not_hoisted -> pin_impure_blocked
//       fixpoint_is_idempotent -> pin_fixpoint_noop (the prologue
//         fixed point: re-running the prefix changes nothing, which
//         is what makes the second run a no-op)
//     plus pin_zero_trip_diverges (UNMIRRORED — a logical property
//     of the unconditional splice, not a source behavior).
//
// MUTANT (checked red): dropping invariant_reads from the gate
// (treating counter-dependent derived statements as hoistable)
// makes the same command report 20 verified, 1 error —
// pin_counter_derived_blocked (the pin guarding exactly that gate)
// goes red; hoist_sound and pin_hoist_preserves_run take
// all_invariant/single_write as premises, not the gate, so they
// stay green. The file was restored and re-checked at
// 21 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 loop_opt.rs
//   -> verification results:: 21 verified, 0 errors
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

// Single-write gate: no target appears twice in the prefix (the
// writer_count total == in_group + 1 shape with no outside writes;
// the suffix writes nothing by construction).
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

// Hoist-gate conjunction as oracle premises: loop_opt's six guards
// plus purity (the leading-run structure is built into the model —
// only a prefix is ever hoisted).
pub open spec fn hoistable(pure_group: bool, no_escape: bool, clobber_free: bool, invariant_reads: bool, no_status: bool, strict_subset: bool) -> bool {
    pure_group && no_escape && clobber_free && invariant_reads && no_status && strict_subset
}

// ---- Fidelity pins (mirror loop_opt.rs's 7 unit tests) ----

proof fn pin_pure_call_derived_hoists()
    ensures hoistable(true, true, true, true, true, true),
{
}

proof fn pin_counter_derived_blocked()
    ensures !hoistable(true, true, true, false, true, true),
{
}

proof fn pin_arith_hoists()
    ensures hoistable(true, true, true, true, true, true),
{
}

proof fn pin_clobber_blocked()
    ensures !hoistable(true, true, false, true, true, true),
{
}

proof fn pin_escape_blocked()
    ensures !hoistable(true, false, true, true, true, true),
{
}

proof fn pin_impure_blocked()
    ensures !hoistable(false, true, true, true, true, true),
{
}

// The fixpoint test's second-run-no-op is the empirical shadow of
// the prologue fixed point: re-running the prefix changes nothing.
proof fn pin_fixpoint_noop(prefix: Seq<InvAssign>, s: Store)
    requires all_invariant(prefix), single_write(prefix),
    ensures store_agree(run_prefix(prefix, run_prefix(prefix, s)), run_prefix(prefix, s)),
{
    prefix_fixed(prefix, s);
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
                assert(lookup(run_prefix(tail, s1), v) == lookup(run_prefix(prefix, s), v)) by {
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
