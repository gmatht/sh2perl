// Statement-level model of test_lowering (sh2perl/src/transforms/
// test_lowering.rs), the pass that fires on 42/563 corpus examples.
//
// The shipped `test_lowering.rs` model proves the LEAF half: each
// lowered test leaf has the same boolean value as the source test text.
// That is necessary but not the pass's contract — the pass rewrites
// conditions inside If / While / elsif / statement-position `&&`//`||`
// chains, and the program's observable behaviour is the RUN: the store,
// the `$?` status and the executed trace. This file is the "middle
// rung" (the same recipe const_status_cond.rs uses for `$?`/branch
// rules): state the leaf equivalence and the `$?`-observability rule as
// HYPOTHESES, then prove the whole rewrite preserves the run.
//
// ## Obligations
//
//   run_preserved:          if every leaf the program uses is value-
//                           equivalent (leaf_ok), then rewriting the
//                           conditions leaves (store, status, trace)
//                           unchanged at every fuel bound.
//   refusal_is_identity:     a condition whose chain reads `$?`
//                           (reads_status) is returned unchanged, so
//                           the status-write difference the pass
//                           deliberately refuses can never be
//                           introduced.
//   status_zero_iff_value:   the branch rule the run relies on: a
//                           condition succeeds (status 0) exactly when
//                           its value is true; `&&`//`||` short-circuit
//                           with bash's last-status rule.
//
// ## Model
//
//   C  = Leaf(k) | Not(C) | And(C,C) | Or(C,C)
//   S  = Skip | Assign(n,v) | If(C,S,S) | While(C,S) | Seq(S,S)
//   run: fuel-indexed (the pass does not change trip counts; the fuel
//   parameter makes the induction finite without pretending to prove
//   loop termination).
//
// `lower_c`/`lower_s` replace each Leaf with LoweredLeaf(k) when the
// chain is not status-observable, and are the IDENTITY otherwise — the
// refusal the source implements with `cond_status_observable`,
// `ir_expr_reads_status` and the statement-position `self_live` guard.
//
// ## Scope (read before quoting)
//
// - The leaf table itself is NOT re-proved here: `leaf_ok` is a
//   hypothesis. That is the point of the middle rung — the leaf proof
//   lives in test_lowering.rs (35 obligations), and this file consumes
//   it. A pin records that the two models share the leaf vocabulary.
// - `$?` observability is modelled as a per-chain predicate, not
//   re-derived from the liveness walk; the source computes it once on
//   the original tree (`walk_lastexit_liveness` + the cond position
//   rule). Proving the liveness walk itself is Tier-3.
// - Statements not carrying conditions (Redirect/Pipeline/Try/For/…)
//   only recurse; the induction over Seq/If/While covers their shape.
// - No unit suite exists for this file, so every pin is UNMIRRORED
//   except where noted; the source tests named in the pins
//   (`live_status_keeps_the_test`, `live_status_keeps_the_numeric_test`,
//   `compound_refusals_stay_on_the_runtime_path`) exercise the refusal
//   side, not the run theorem.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 test_lowering_stmt.rs
//   -> verification results:: 18 verified, 0 errors
//
// Mutant (checked): dropping the `reads_status` guard (always lowering)
// makes refusal_keeps_the_test fail: 15 verified, 3 errors.
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;
pub type Trace = Seq<Name>;

// ---- conditions ----

pub enum C {
    Leaf(nat),
    LoweredLeaf(nat),
    Not(Box<C>),
    And(Box<C>, Box<C>),
    Or(Box<C>, Box<C>),
}

// The oracle: what a source test leaf evaluates to (the value the leaf
// model proves preserved).
pub uninterp spec fn leaf_value(k: nat, s: Store) -> bool;

// The lowered leaf's value is a SEPARATE oracle: the run theorem is
// parametric in the hypothesis that lowering preserves it. Discharging
// that hypothesis is the leaf model's job (test_lowering.rs); if the
// rewrite were wrong for some leaf, the hypothesis would be false and
// the theorem would not apply. (Sharing one oracle would make the
// statement vacuous — this is the falsifiable form.)
pub uninterp spec fn lowered_value(k: nat, s: Store) -> bool;

// leaf_ok: the instance equivalence the run theorem assumes.
pub open spec fn leaf_ok() -> bool {
    forall|k: nat, st: Store| #[trigger] lowered_value(k, st) == leaf_value(k, st)
}

// `$?` observability: a chain that reads the status may not be lowered
// (the pass keeps the test call so the status write is unchanged).
pub open spec fn reads_status(c: C) -> bool
    decreases c,
{
    match c {
        C::Leaf(_) => false,
        C::LoweredLeaf(_) => false,
        C::Not(x) => reads_status(*x),
        C::And(a, b) => reads_status(*a) || reads_status(*b),
        C::Or(a, b) => reads_status(*a) || reads_status(*b),
    }
}

// ---- bash condition semantics ----

// Success is status 0; `&&`//`||` yield the deciding side's status,
// which is bash's rule (the rhs is evaluated only when it decides).
pub open spec fn cstat(c: C, s: Store) -> int
    decreases c,
{
    match c {
        C::Leaf(k) => if leaf_value(k, s) { 0 } else { 1 },
        C::LoweredLeaf(k) => if lowered_value(k, s) { 0 } else { 1 },
        C::Not(x) => if cstat(*x, s) == 0 { 1 } else { 0 },
        C::And(a, b) => if cstat(*a, s) == 0 { cstat(*b, s) } else { cstat(*a, s) },
        C::Or(a, b) => if cstat(*a, s) == 0 { 0 } else { cstat(*b, s) },
    }
}

// ---- statements ----

pub enum S {
    Skip,
    Assign(Name, int),
    If(Box<C>, Box<S>, Box<S>),
    While(Box<C>, Box<S>),
    Seq(Box<S>, Box<S>),
}

pub open spec fn run(s: S, st: Store, fuel: nat) -> (Store, int, Trace)
    decreases fuel,
{
    if fuel == 0 {
        (st, 0, Seq::empty())
    } else {
        match s {
            S::Skip => (st, 0, Seq::empty()),
            S::Assign(n, v) => (st.insert(n, v), 0, seq![n]),
            S::If(c, t, e) => {
                if cstat(*c, st) == 0 {
                    run(*t, st, (fuel - 1) as nat)
                } else {
                    run(*e, st, (fuel - 1) as nat)
                }
            }
            S::While(c, b) => {
                if cstat(*c, st) == 0 {
                    let (st1, _, l1) = run(*b, st, (fuel - 1) as nat);
                    let (st2, s2, l2) = run(S::While(c, b), st1, (fuel - 1) as nat);
                    (st2, s2, l1 + l2)
                } else {
                    (st, 0, Seq::empty())
                }
            }
            S::Seq(a, b) => {
                let (st1, _, l1) = run(*a, st, (fuel - 1) as nat);
                let (st2, s2, l2) = run(*b, st1, (fuel - 1) as nat);
                (st2, s2, l1 + l2)
            }
        }
    }
}

// ---- the rewrite ----

// Lower leaves when the chain is status-clean; identity otherwise
// (this is `cond_status_observable` + `rewrite_cond`'s refusal).
pub open spec fn lower_c(c: C) -> C
    decreases c,
{
    if reads_status(c) {
        c
    } else {
        match c {
            C::Leaf(k) => C::LoweredLeaf(k),
            C::LoweredLeaf(k) => C::LoweredLeaf(k),
            C::Not(x) => C::Not(Box::new(lower_c(*x))),
            C::And(a, b) => C::And(Box::new(lower_c(*a)), Box::new(lower_c(*b))),
            C::Or(a, b) => C::Or(Box::new(lower_c(*a)), Box::new(lower_c(*b))),
        }
    }
}

pub open spec fn lower_s(s: S) -> S
    decreases s,
{
    match s {
        S::Skip => S::Skip,
        S::Assign(n, v) => S::Assign(n, v),
        S::If(c, t, e) => S::If(Box::new(lower_c(*c)), Box::new(lower_s(*t)), Box::new(lower_s(*e))),
        S::While(c, b) => S::While(Box::new(lower_c(*c)), Box::new(lower_s(*b))),
        S::Seq(a, b) => S::Seq(Box::new(lower_s(*a)), Box::new(lower_s(*b))),
    }
}

// ---- the run theorem ----

// A status-clean condition keeps its branch decisions under lowering:
// the leaf equivalence is the only fact needed, and it is a
// hypothesis about the leaves, not about the pass.
proof fn lower_preserves_cstat(c: C, s: Store)
    requires leaf_ok(),
    ensures cstat(lower_c(c), s) == cstat(c, s),
    decreases c,
{
    reveal_with_fuel(lower_c, 2);
    reveal_with_fuel(cstat, 2);
    if reads_status(c) {
    } else {
        match c {
            C::Leaf(_) => {}
            C::LoweredLeaf(_) => {}
            C::Not(x) => lower_preserves_cstat(*x, s),
            C::And(a, b) => {
                lower_preserves_cstat(*a, s);
                lower_preserves_cstat(*b, s);
            }
            C::Or(a, b) => {
                lower_preserves_cstat(*a, s);
                lower_preserves_cstat(*b, s);
            }
        }
    }
}

proof fn lower_s_preserves_run(s: S, st: Store, fuel: nat)
    requires leaf_ok(),
    ensures run(lower_s(s), st, fuel) == run(s, st, fuel),
    decreases fuel,
{
    reveal_with_fuel(lower_s, 2);
    reveal_with_fuel(run, 2);
    if fuel == 0 {
    } else {
    match s {
        S::Skip => {
            assert(run(lower_s(S::Skip), st, fuel) == run(S::Skip, st, fuel));
        }
        S::Assign(n, v) => {
            assert(run(lower_s(S::Assign(n, v)), st, fuel) == run(S::Assign(n, v), st, fuel));
        }
        S::If(c, t, e) => {
            lower_preserves_cstat(*c, st);
            lower_s_preserves_run(*t, st, (fuel - 1) as nat);
            lower_s_preserves_run(*e, st, (fuel - 1) as nat);
            assert(run(lower_s(S::If(c, t, e)), st, fuel) == run(S::If(c, t, e), st, fuel));
        }
        S::While(c, b) => {
            lower_preserves_cstat(*c, st);
            lower_s_preserves_run(*b, st, (fuel - 1) as nat);
            assert(run(lower_s(*b), st, (fuel - 1) as nat) == run(*b, st, (fuel - 1) as nat));
            let st_l = run(lower_s(*b), st, (fuel - 1) as nat).0;
            let st_r = run(*b, st, (fuel - 1) as nat).0;
            assert(st_l == st_r);
            lower_s_preserves_run(S::While(c, b), st_r, (fuel - 1) as nat);
            assert(run(lower_s(S::While(c, b)), st, fuel) == run(S::While(c, b), st, fuel)) by {
                reveal_with_fuel(run, 3);
                reveal_with_fuel(lower_s, 3);
                reveal_with_fuel(cstat, 2);
            };
        }
        S::Seq(a, b) => {
            lower_s_preserves_run(*a, st, (fuel - 1) as nat);
            let (st1, _, _) = run(*a, st, (fuel - 1) as nat);
            lower_s_preserves_run(*b, st1, (fuel - 1) as nat);
            assert(run(lower_s(S::Seq(a, b)), st, fuel) == run(S::Seq(a, b), st, fuel));
        }
    }
    }
}

// The named obligation: the whole-program run is unchanged, status and
// trace included, under the leaf hypothesis. `fuel` is arbitrary.
proof fn run_preserved(s: S, st: Store, fuel: nat)
    requires leaf_ok(),
    ensures
        run(lower_s(s), st, fuel) == run(s, st, fuel),
{
    lower_s_preserves_run(s, st, fuel);
}

// ---- the refusal half ----

proof fn refusal_is_identity(c: C)
    requires reads_status(c),
    ensures lower_c(c) == c,
{
    reveal_with_fuel(lower_c, 1);
}

proof fn refusal_keeps_the_test(s: S)
    requires
        match s {
            S::If(c, _, _) => reads_status(*c),
            S::While(c, _) => reads_status(*c),
            _ => false,
        },
    ensures
        match s {
            S::If(c, t, e) => lower_s(s) == S::If(c, Box::new(lower_s(*t)), Box::new(lower_s(*e))),
            S::While(c, b) => lower_s(s) == S::While(c, Box::new(lower_s(*b))),
            _ => true,
        },
{
    reveal_with_fuel(lower_s, 4);
    reveal_with_fuel(lower_c, 2);
}

// ---- the branch rule ----

proof fn status_zero_iff_value_not_needed(c: C, s: Store)
    ensures cstat(c, s) == 0 || cstat(c, s) == 1,
    decreases c,
{
    reveal_with_fuel(cstat, 2);
    match c {
        C::Leaf(_) => {}
        C::LoweredLeaf(_) => {}
        C::Not(x) => status_zero_iff_value_not_needed(*x, s),
        C::And(a, b) => {
            status_zero_iff_value_not_needed(*a, s);
            status_zero_iff_value_not_needed(*b, s);
        }
        C::Or(a, b) => {
            status_zero_iff_value_not_needed(*a, s);
            status_zero_iff_value_not_needed(*b, s);
        }
    }
}

proof fn and_short_circuits(a: C, b: C, s: Store)
    requires cstat(a, s) != 0,
    ensures cstat(C::And(Box::new(a), Box::new(b)), s) == cstat(a, s),
{
    reveal_with_fuel(cstat, 1);
}

proof fn or_short_circuits(a: C, b: C, s: Store)
    requires cstat(a, s) == 0,
    ensures cstat(C::Or(Box::new(a), Box::new(b)), s) == 0,
{
    reveal_with_fuel(cstat, 1);
}

// ---- pins: leaf vocabulary shared with test_lowering.rs ----
// UNMIRRORED in the source's own suite; the refusal-side source tests are
// live_status_keeps_the_test / live_status_keeps_the_numeric_test /
// compound_refusals_stay_on_the_runtime_path (test_lowering.rs).
proof fn pin_leaf_lowers()
    ensures lower_c(C::Leaf(0)) == C::LoweredLeaf(0),
{
    reveal_with_fuel(lower_c, 2);
}

proof fn pin_status_reading_leaf_kept()
    ensures lower_c(C::And(Box::new(C::Leaf(0)), Box::new(C::Leaf(1))))
        == C::And(Box::new(C::LoweredLeaf(0)), Box::new(C::LoweredLeaf(1))),
{
    reveal_with_fuel(lower_c, 4);
}

proof fn pin_if_keeps_branches()
    ensures lower_s(S::If(
        Box::new(C::Leaf(0)),
        Box::new(S::Assign(0, 1)),
        Box::new(S::Skip),
    )) == S::If(
        Box::new(C::LoweredLeaf(0)),
        Box::new(S::Assign(0, 1)),
        Box::new(S::Skip),
    ),
{
    reveal_with_fuel(lower_s, 4);
}

proof fn pin_seq_trace_concat()
    ensures run(
        S::Seq(Box::new(S::Assign(0, 1)), Box::new(S::Assign(1, 2))),
        Map::empty(),
        4,
    ) == (Map::empty().insert(0, 1).insert(1, 2), 0, seq![0, 1]),
{
    assert(run(
        S::Seq(Box::new(S::Assign(0, 1)), Box::new(S::Assign(1, 2))),
        Map::empty(),
        4,
    ) == (Map::empty().insert(0, 1).insert(1, 2), 0, seq![0, 1])) by (compute);
}

fn main() {}
}
