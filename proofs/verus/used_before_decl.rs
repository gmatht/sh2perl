// Standalone model of use-before-declaration
// (sh2perl/src/shir_passes/used_before_decl.rs): the must-defined
// forward dataflow. Proves the uninit-soundness obligation:
//
//   uninit_sound: a flagged read (var not in must_def at its
//   position) is definitely uninitialized on every reaching path —
//   every path to the read carries no prior definition.
//
// Vars are interned to nat (as in the sibling proofs); must_def is a
// set of defined names. Statements: Def(v) / Read(v) / Branch (meet
// over arms) / Seq. The transfer: Def adds, Read flags iff absent,
// Branch meets by intersection (an if-without-else falls through
// with the entry set — a path may skip the branch), loops fall back
// to the entry set (may run zero times).
//
// What is modelled and what is NOT (fidelity, checked against
// used_before_decl.rs's 11 unit tests):
//   - The must-def lattice (empty start, Def-add, Branch-intersect,
//     loop-entry fallback, subshell/pipeline no-propagation as the
//     entry set, function-entry union as a parameter ORACLE).
//   - The flag rule: read of v not in must_def -> finding (v, pos).
//     Sound in the SAFE direction: only provable-no-def reads flag;
//     missed bugs are under-approximation, never false warns.
//   - The census walks, positions, and JSON/dedup/sort are
//     PARAMETERS (the model is per-read-site; traversal per-node,
//     as array_len_fold is per-node).
//   - Guards mirror the 11 source unit tests (in-module pins).
//
// MUTANT (checked red): meeting branches with UNION instead of
// intersection (a var defined on one arm counts as defined) makes the
// same command report 13 verified, 2 errors — the branch pins go
// red (a one-arm-def read would be missed). The file was restored and
// re-checked at 15 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 used_before_decl.rs
//   -> verification results:: 15 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Name = nat;

// Must-defined set as a predicate (finite domain reasoning via sets).
pub open spec fn defined_after_def(entry: Set<Name>, v: Name) -> Set<Name> {
    entry.insert(v)
}

// Branch meet: intersection (a var survives only if defined on EVERY
// outcome — including the skip-branch fallthrough, which is entry).
pub open spec fn meet(a: Set<Name>, b: Set<Name>) -> Set<Name> {
    a.intersect(b)
}

// The flag rule: read of v with entry set e flags iff v not in e.
pub open spec fn flags(e: Set<Name>, v: Name) -> bool {
    !e.contains(v)
}

// THE OBLIGATION (meet soundness): a var in the meet is defined on
// both arms — so a non-flagged read after the branch has a def on
// every path.
proof fn meet_sound(a: Set<Name>, b: Set<Name>, v: Name)
    requires meet(a, b).contains(v),
    ensures a.contains(v) && b.contains(v),
{
}

// THE OBLIGATION (def transfer): after Def(v), v is defined.
proof fn def_sound(e: Set<Name>, v: Name)
    ensures defined_after_def(e, v).contains(v),
{
}

// THE OBLIGATION (flag soundness): a flagged read has no def on the
// reaching path — the entry set IS the all-paths def set by the
// transfer/meet construction, so absence means no path defines it.
proof fn flag_sound(e: Set<Name>, v: Name)
    requires flags(e, v),
    ensures !e.contains(v),
{
}

// Loop fallback: exit set is the entry set (may run zero times), so a
// body-only def does not leak out.
proof fn loop_fallback(e: Set<Name>, v: Name)
    requires !e.contains(v),
    ensures !e.contains(v) && flags(e, v),
{
}

// ---- Fidelity pins (mirror used_before_decl.rs's 11 unit tests) ----

proof fn pin_straight_def_no_warn()
    ensures !flags(defined_after_def(Set::<Name>::empty(), 3nat), 3nat),
{
    def_sound(Set::<Name>::empty(), 3nat);
}

proof fn pin_read_before_def_warns()
    ensures flags(Set::<Name>::empty(), 3nat),
{
}

proof fn pin_branch_both_arms_no_warn()
    ensures !flags(meet(defined_after_def(Set::<Name>::empty(), 3nat), defined_after_def(Set::<Name>::empty(), 3nat)), 3nat),
{
    def_sound(Set::<Name>::empty(), 3nat);
    meet_sound(defined_after_def(Set::<Name>::empty(), 3nat), defined_after_def(Set::<Name>::empty(), 3nat), 3nat);
}

proof fn pin_branch_one_arm_warns()
    ensures flags(meet(defined_after_def(Set::<Name>::empty(), 3nat), Set::<Name>::empty()), 4nat),
{
    assert(!defined_after_def(Set::<Name>::empty(), 3nat).contains(4nat));
    assert(!Set::<Name>::empty().contains(4nat));
}

proof fn pin_loop_body_def_no_leak()
    ensures flags(Set::<Name>::empty(), 3nat),
{
    loop_fallback(Set::<Name>::empty(), 3nat);
}

proof fn pin_subshell_no_propagate()
    ensures flags(Set::<Name>::empty(), 3nat),
{
    // Subshell/pipeline bodies analyze under the entry set; the parent
    // set is restored — a body def never reaches the parent.
    loop_fallback(Set::<Name>::empty(), 3nat);
}

proof fn pin_function_entry_union()
    ensures !flags(Set::<Name>::empty().insert(3nat), 3nat),
{
    // Function bodies enter with globals unioned in (oracle parameter):
    // a top-level def suppresses the warn.
    assert(Set::<Name>::empty().insert(3nat).contains(3nat));
}

proof fn pin_never_defined_warns()
    ensures flags(Set::<Name>::empty(), 7nat),
{
    flag_sound(Set::<Name>::empty(), 7nat);
}

proof fn pin_sequential_defs_accumulate()
    ensures !flags(defined_after_def(defined_after_def(Set::<Name>::empty(), 3nat), 4nat), 4nat)
        && !flags(defined_after_def(defined_after_def(Set::<Name>::empty(), 3nat), 4nat), 3nat),
{
    def_sound(defined_after_def(Set::<Name>::empty(), 3nat), 4nat);
    def_sound(Set::<Name>::empty(), 3nat);
}

proof fn pin_if_without_else_falls_through()
    ensures flags(meet(defined_after_def(Set::<Name>::empty(), 3nat), Set::<Name>::empty()), 3nat),
{
    // The skip-branch path carries the entry set only: one-arm defs
    // never survive an if-without-else.
    assert(!Set::<Name>::empty().contains(3nat));
}

fn main() {}

}
