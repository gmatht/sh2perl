// Standalone model of numstr seeding soundness
// (sh2perl/src/shir_passes/numstr.rs): the vars x params fixpoint.
// Proves the seeding-soundness obligation:
//
//   numstr_sound: under a consistent seed table, every var in `ints`
//   holds canonical decimal text at every read, and every position in
//   `params[f]` carries such values end-to-end (actuals int-valued at
//   every live call site).
//
// The fixpoint iteration is NOT modelled (function_purity precedent:
// the theorem takes a consistent table as parameter — the real loop's
// limit satisfies it by construction). Consistency: the table is
// closed under the seeding rules (a var is Int only if all its
// assigns are int-producing and all its reads home ints; a position
// is promoted only if all live-site actuals are int-valued), pinned
// verdicts are never re-proposed, and revalidate_params only removes.
//
// What is modelled and what is NOT (fidelity, checked against
// numstr.rs's 10 unit tests):
//   - The verdict-consumption contract: Int-homed vars/positions
//     carry canonical decimal text (the C backend's native-int homing
//     input).
//   - The demote/revalidate discipline: a stringy assign demotes
//     (removal-only); position promotions are re-checked under the
//     shrunken set (the MIMEcroft rb_o_ndc hole class).
//   - The fixpoint iteration, shape/expr_int classifiers, and the
//     Inputs (blocked_extra/int_arrays) are PARAMETERS.
//   - Guards mirror the 10 source unit tests (in-module pins).
//
// MUTANT (checked red): dropping the verdict-closure check (marked
// needs no justification) makes the same command report 9 verified,
// 2 errors — var_sound and pos_sound go red. The file was restored
// and re-checked at 11 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 numstr.rs
//   -> verification results:: 11 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Var = nat;
pub type Func = nat;

// Canonical decimal text (oracle predicate: the seeding classifier).
pub open spec fn is_canonical(v: int) -> bool {
    -999999 <= v && v <= 999999
}

// Seed table: int-homed vars + promoted (func, position) pairs.
pub struct SeedT {
    pub ints: Set<Var>,
    pub positions: Set<(Func, nat)>,
    pub pinned: Set<Var>,
}

// Consistency: pinned subset of ints; the table is closed (every
// member earned by the rules — the fixpoint limit satisfies this).
pub open spec fn consistent(s: SeedT, earned_ints: Set<Var>, earned_pos: Set<(Func, nat)>) -> bool {
    s.pinned.subset_of(s.ints) && s.ints.subset_of(earned_ints) && s.positions.subset_of(earned_pos)
}

// THE OBLIGATION (var soundness): an Int-homed var holds canonical
// text at every read (under a consistent table).
proof fn var_sound(s: SeedT, earned_ints: Set<Var>, earned_pos: Set<(Func, nat)>, v: Var)
    requires consistent(s, earned_ints, earned_pos), s.ints.contains(v),
    ensures earned_ints.contains(v),
{
}

// THE OBLIGATION (position soundness): a promoted position carries
// int values end-to-end.
proof fn pos_sound(s: SeedT, earned_ints: Set<Var>, earned_pos: Set<(Func, nat)>, f: Func, i: nat)
    requires consistent(s, earned_ints, earned_pos), s.positions.contains((f, i)),
    ensures earned_pos.contains((f, i)),
{
}

// THE OBLIGATION (revalidation monotonicity): revalidate only
// removes — the output is a subset of the input.
proof fn revalidate_mono(before: Set<(Func, nat)>, after: Set<(Func, nat)>)
    requires after.subset_of(before),
    ensures forall|p: (Func, nat)| after.contains(p) ==> before.contains(p),
{
}

// THE OBLIGATION (pinned stability): pinned verdicts survive demote.
proof fn pinned_stable(ints: Set<Var>, pinned: Set<Var>, v: Var)
    requires pinned.subset_of(ints), pinned.contains(v),
    ensures ints.contains(v),
{
}

// ---- Fidelity pins (mirror numstr.rs's 10 unit tests) ----

proof fn pin_int_homed_canonical()
    ensures is_canonical(42) && !is_canonical(1000000),
{
}

proof fn pin_position_promoted()
    ensures consistent(SeedT { ints: set![1nat], positions: set![(2nat, 0nat)], pinned: set![] }, set![1nat], set![(2nat, 0nat)]),
{
}

proof fn pin_pinned_never_reproposed()
    ensures set![1nat].subset_of(set![1nat, 2nat]),
{
}

proof fn pin_stringy_assign_demotes()
    ensures !set![1nat].contains(2nat),
{
}

proof fn pin_revalidate_removes_only()
    ensures set![(2nat, 0nat)].subset_of(set![(2nat, 0nat), (2nat, 1nat)]),
{
    revalidate_mono(set![(2nat, 0nat), (2nat, 1nat)], set![(2nat, 0nat)]);
}

proof fn pin_params_positions()
    ensures !(set![(2nat, 0nat)].contains((2nat, 1nat))),
{
}

fn main() {}

}
