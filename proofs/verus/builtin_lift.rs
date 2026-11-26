// Standalone model of `builtin-lift`
// (sh2perl/src/transforms/builtin-lift.rs).
//
// The pass rewrites `exec("cmd", args)` to `builtin("cmd", args)` for every
// command in the shared namespace contract (`<workspace>/harness/builtins.json`), so the
// native-vs-spawn decision moves from each renderer into the A1. Two things
// can go wrong, and they are different obligations:
//
//   1. SOUNDNESS: lifting a name whose native op does not behave like the
//      external binary changes the program. The pass's guard is the table
//      membership, so `lift_preserves` is stated under the runtime
//      hypothesis `∀ cmd ∈ table: ext(cmd) == native(cmd)` — the namespace
//      contract, which the model does NOT prove (it is a runtime-equivalence
//      claim, not a rewrite property). `off_table_lift_is_wrong` keeps the
//      disagreement for an off-table name, so the table check is
//      load-bearing rather than decorative.
//
//   2. COMPLETENESS: the rewrite must reach EVERY expression position
//      (statement expressions, conditions, redirect targets, nested
//      functions/loops). A missed `descend` arm silently leaves a spawn in
//      place — a performance bug, not a semantic one, but exactly the class
//      the pass's big match invites. `lift_reaches_every_position` proves it
//      by structural induction; a mutant that forgets one arm is red.
//
// ## Scope (read before quoting it)
//
// - Model, not code: commands are `nat` ids, the contract table is a
//   `Set<nat>` built from a concrete list (pins: `echo`/`printf`/`rm`/`cd`
//   in, `gcc` out), and a program is a tree of `Exec`/`Builtin`/`Seq`/`If`/
//   `Nested` nodes with an abstract runtime value per command.
// - `args` are not modelled: the pass leaves them untouched (it only
//   recurses into them), and `lift_reaches_every_position` covers that
//   recursion structurally.
// - The runtime counterpart is an ASSUMPTION, stated: the native op for a
//   table name must match the external binary's observable behaviour. The
//   corpus gates and the contract check (`node <workspace>/harness/estree-contract.mjs`)
//   are what discharge it outside Verus.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 builtin_lift.rs
//   -> verification results:: 12 verified, 0 errors
//
// Mutants (checked, measured on copies):
//   - lifting OFF-TABLE names too: 9 verified, 3 errors (soundness fails —
//     the table check is what makes the rewrite true).
//   - forgetting the `Nested` recursion arm: 11 verified, 1 error
//     (completeness — the missed position keeps an `Exec`).
// Restored and re-checked at 12 verified, 0 errors.
use vstd::prelude::*;

verus! {

// The shared contract: the source's BUILTINS table, reduced to the four
// names the pins use plus a name that is deliberately absent.
pub open spec fn table() -> Set<nat> {
    Set::empty().insert(1).insert(2).insert(3).insert(4) // echo, printf, rm, cd
}

pub open spec fn in_table(cmd: nat) -> bool {
    table().contains(cmd)
}

// `none` = a command name that is not a literal (a variable: `"$cmd" args`).
pub enum X {
    Exec(Option<nat>),
    Builtin(nat),
    Seq(Box<X>, Box<X>),
    If(Box<X>, Box<X>, Box<X>),
    Nested(Box<X>),
}

// The runtime value of a command node. `ext` is the external binary and
// `native` the backend's own op; the contract hypothesis equates them on
// the table.
pub open spec fn run(x: X) -> int
    decreases x,
{
    match x {
        X::Exec(Option::Some(c)) => ext(c),
        // a variable-sourced command: the curtailed model has no value for
        // it, and the pass never rewrites it, so both sides agree by
        // construction (pinned by `dynamic_command_untouched`)
        X::Exec(Option::None) => -1,
        X::Builtin(c) => native(c),
        X::Seq(a, b) => run(*a) + run(*b),
        X::If(c, t, e) => if run(*c) > 0 { run(*t) } else { run(*e) },
        X::Nested(a) => run(*a),
    }
}

pub open spec fn ext(c: nat) -> int {
    (c * 10) as int
}

// The namespace contract, in model form: a table name's native op matches
// the external binary; an off-table name's does NOT (that is the whole
// point of the table — those commands are genuinely external).
pub open spec fn native(c: nat) -> int {
    if in_table(c) { (c * 10) as int } else { (c * 10 + 7) as int }
}

// The pass: a literal table name becomes `builtin`; everything else is
// rebuilt with the recursion applied (the source's `lift_stmt`/`lift_expr`).
pub open spec fn lift(x: X) -> X
    decreases x,
{
    match x {
        X::Exec(Option::Some(c)) => if in_table(c) { X::Builtin(c) } else { X::Exec(Option::Some(c)) },
        X::Exec(Option::None) => X::Exec(Option::None),
        X::Builtin(c) => X::Builtin(c),
        X::Seq(a, b) => X::Seq(Box::new(lift(*a)), Box::new(lift(*b))),
        X::If(c, t, e) => X::If(Box::new(lift(*c)), Box::new(lift(*t)), Box::new(lift(*e))),
        X::Nested(a) => X::Nested(Box::new(lift(*a))),
    }
}

// ---- soundness ----

// THE OBLIGATION: under the namespace contract, lifting does not change the
// program's observable value.
proof fn lift_preserves(x: X)
    ensures run(lift(x)) == run(x),
    decreases x,
{
    reveal_with_fuel(lift, 1);
    reveal_with_fuel(run, 1);
    match x {
        X::Exec(Option::Some(c)) => {
            if in_table(c) {
                // the hypothesis is the whole content of "this op is native"
            }
        }
        X::Exec(Option::None) => {}
        X::Builtin(_) => {}
        X::Seq(a, b) => {
            lift_preserves(*a);
            lift_preserves(*b);
        }
        X::If(c, t, e) => {
            lift_preserves(*c);
            lift_preserves(*t);
            lift_preserves(*e);
        }
        X::Nested(a) => lift_preserves(*a),
    }
}

// Lifting an OFF-TABLE command is not free: the contract promises nothing
// about its native op, and here it observably differs. Kept as an
// obligation so the table check cannot be dropped silently.
proof fn lifting_an_off_table_name_is_wrong()
    ensures ({
        let x = X::Exec(Option::Some(9));
        // the pass refuses to lift it...
        !in_table(9) && lift(x) == x
            // ...and a mutant that lifted anyway would change the value
            && run(X::Builtin(9)) != run(x)
    }),
{
    reveal_with_fuel(run, 1);
}

// ---- completeness ----

// Every literal table command in the tree is a `Builtin` after the lift.
pub open spec fn all_table_cmds_lifted(x: X) -> bool
    decreases x,
{
    match x {
        X::Exec(Option::Some(c)) => !in_table(c),
        X::Exec(Option::None) => true,
        X::Builtin(_) => true,
        X::Seq(a, b) => all_table_cmds_lifted(*a) && all_table_cmds_lifted(*b),
        X::If(c, t, e) => {
            all_table_cmds_lifted(*c) && all_table_cmds_lifted(*t) && all_table_cmds_lifted(*e)
        }
        X::Nested(a) => all_table_cmds_lifted(*a),
    }
}

// THE COMPLETENESS OBLIGATION: every expression position is reached — a
// missed recursion arm (a condition, a redirect target, a nested body)
// leaves an `Exec` behind and fails here.
proof fn lift_reaches_every_position(x: X)
    ensures all_table_cmds_lifted(lift(x)),
    decreases x,
{
    reveal_with_fuel(lift, 1);
    reveal_with_fuel(all_table_cmds_lifted, 1);
    match x {
        X::Exec(Option::Some(c)) => {}
        X::Exec(Option::None) => {}
        X::Builtin(_) => {}
        X::Seq(a, b) => {
            lift_reaches_every_position(*a);
            lift_reaches_every_position(*b);
        }
        X::If(c, t, e) => {
            lift_reaches_every_position(*c);
            lift_reaches_every_position(*t);
            lift_reaches_every_position(*e);
        }
        X::Nested(a) => lift_reaches_every_position(*a),
    }
}

// ---- pins: the contract table and the refusal ----

// The table the pass checks is the shared contract, not a local list: the
// four names the source's corpus measurement leads with are in it.
proof fn pin_table_names()
    ensures in_table(1) && in_table(2) && in_table(3) && in_table(4) && !in_table(9),
{
}

// A literal name in the table lifts; the arguments are not modelled, and
// the node keeps its identity.
proof fn pin_lift_is_a_carrier_rename()
    ensures lift(X::Exec(Option::Some(2))) == X::Builtin(2)
        && lift(X::Builtin(2)) == X::Builtin(2),
{
    reveal_with_fuel(lift, 2);
}

// A variable-sourced command name is never lifted (the source's
// "only a STATIC first-arg command name" rule).
proof fn pin_dynamic_command_untouched()
    ensures lift(X::Exec(Option::None)) == X::Exec(Option::None),
{
}

// An off-table literal keeps its exec (a genuinely-external command).
proof fn pin_off_table_keeps_exec()
    ensures lift(X::Exec(Option::Some(9))) == X::Exec(Option::Some(9)),
{
    reveal_with_fuel(lift, 2);
}

// The lift is idempotent: a second run changes nothing (the source's
// `builtin` arm only recurses).
proof fn pin_idempotent()
    ensures lift(lift(X::Seq(
        Box::new(X::Exec(Option::Some(1))),
        Box::new(X::Exec(Option::Some(9))),
    ))) == lift(X::Seq(
        Box::new(X::Exec(Option::Some(1))),
        Box::new(X::Exec(Option::Some(9))),
    )),
{
    reveal_with_fuel(lift, 4);
}

fn main() {}
}
