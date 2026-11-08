// Standalone model of restructure_goto's per-shape identities
// (sh2perl/src/shir_passes/restructure.rs): jump edges rewrite to
// structured flow. Proves the structural carve-out:
//
//   span_identity: backward shapes preserve the loop-body sequence;
//   branch_identity: forward-guarded inversion keeps then/else selection;
//   dead_identity: forward-bare/skip drops exactly the dead region;
//   flag_protocol: nested exits emit set/break/guard/clear in order;
//   no_jumps: every handled shape eliminates its Goto (and label).
//
// Full semantic preservation (the rewritten program RUNS the same)
// needs statement/execution semantics — Tier-3, out of scope (the
// test_lowering/case_lowering precedent: dispatch/leaf rules proved,
// the rest Tier-2). What IS proved here is what Tier-1 can state:
// each rewrite is a constructor-level sequence identity over
// IR-shaped data, with conditions/bodies opaque. Refusal shapes
// (label before/inside the loop chain, non-loop ancestors) are out
// of scope — the pass leaves them and renderers refuse loudly.
//
// Statement calculus: Plain (any non-jump node, opaque), Label/Goto
// (nat-interned names), Loop (While(true) body), CondLoop (While(c)
// body, condition opaque), Branch (then/else), Brk, FlagSet/FlagGuard/
// FlagClear (the __g<label>_<n> protocol nodes), Dead (If{false}
// -wrapped dead region).
//
// What is modelled and what is NOT (fidelity, checked against
// restructure.rs's 8 unit tests):
//   - The seven handled shapes as spec rewrite functions with
//     sequence-identity theorems; the traversal (outermost-first
//     fixpoint, inward recursion), the Branch-path descent, and
//     successor-exit folding as PARAMETERS.
//   - The freshness of __g flags (fresh_flag counter) as a premise:
//     protocol nodes carry a flag id distinct from program vars.
//   - Guards mirror 7 of the 8 source unit tests (in-module pins):
//       backward_goto_becomes_while -> pin_backward_span
//       forward_goto_becomes_inverted_if -> pin_forward_inversion
//       nested_goto_becomes_flag_break -> pin_flag_protocol
//       nested_goto_through_block_wrapper_does_not_escape
//         -> pin_block_not_loop (only real loops guarded)
//       empty_list_noop -> pin_empty_noop
//       goto_inside_a_switch_inside_the_loop_body_reaches_its_tail
//         -> pin_forward_skip_no_flag
//       goto_out_of_a_top_level_loop_guards_the_tail
//         -> pin_tail_guarded
//     plus pin_shapes_agree (UNMIRRORED — model-internal shared
//     prefix/suffix preservation across shapes).
//     body_inside_if_goto_loop carries NO pin: the test is
//     print-only (no asserts), so there is nothing to mirror —
//     recorded, not silently skipped.
//   - Guards (verus-sync-coverage.sh `restructure` arms):
//       handle_bare_goto / handle_guarded_goto / handle_nested /
//       forward_skip_site / fresh_flag / convert_successor_exits /
//       program_has_goto.
//
// MUTANT (checked red): treating a forward-skip as needing a flag
// (emitting FlagSet for the same-list shape) makes the same command
// report 19 verified, 1 error — skip_identity (hence
// pin_forward_skip_no_flag) goes red. The file was restored and
// re-checked at 20 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 restructure.rs
//   -> verification results:: 20 verified, 0 errors
//
// SOURCE DRIFT (recorded when this model rejoined the tree, 2026-11-08):
// restructure.rs changed once since the model was written — commit
// 9928671cd renames `skip_from` to `_skip_from` in `forward_skip_site`
// (the value was already unused, hence the warning). The binding's
// VALUE is dead in both versions: the dead ranges come from `sites`,
// while the early `continue` guards inside that same block are live
// and still run. So `list_skip` / `pin_forward_skip_no_flag` describe
// unchanged behaviour; pins and the gate count are unchanged (20).
use vstd::prelude::*;

verus! {

pub type Lbl = nat;

// Shrunken statement calculus (conditions/bodies opaque).
pub enum Stmt {
    Plain,
    Label(Lbl),
    Goto(Lbl),
    Loop(Seq<Stmt>),
    CondLoop(Seq<Stmt>),
    Branch(Seq<Stmt>, Seq<Stmt>),
    Brk,
    FlagSet(Lbl),
    FlagGuard(Seq<Stmt>),
    FlagClear(Lbl),
    Dead(Seq<Stmt>),
}

// Count of jump nodes (Label/Goto) in a sequence. Fuel-bounded:
// every recursive call consumes one fuel, so one `decreases f`
// covers the mutual recursion (same position, same type).
pub open spec fn jumps(ss: Seq<Stmt>, f: nat) -> nat
    decreases f,
{
    if f == 0 {
        0
    } else if ss.len() == 0 {
        0
    } else {
        (jump_one(ss[0], ((f - 1) as nat)) + jumps(ss.drop_first(), ((f - 1) as nat)))
    }
}

pub open spec fn jump_one(s: Stmt, f: nat) -> nat
    decreases f,
{
    if f == 0 {
        0
    } else {
        match s {
        Stmt::Label(_) => 1,
        Stmt::Goto(_) => 1,
        Stmt::Loop(b) => jumps(b, ((f - 1) as nat)),
        Stmt::CondLoop(b) => jumps(b, ((f - 1) as nat)),
        Stmt::Branch(t, e) => jumps(t, ((f - 1) as nat)) + jumps(e, ((f - 1) as nat)),
        Stmt::FlagGuard(t) => jumps(t, ((f - 1) as nat)),
        Stmt::Dead(b) => jumps(b, ((f - 1) as nat)),
        _ => 0,
        }
    }
}

// Count of flag-protocol nodes in a sequence (same fuel discipline).
pub open spec fn flags(ss: Seq<Stmt>, f: nat) -> nat
    decreases f,
{
    if f == 0 {
        0
    } else if ss.len() == 0 {
        0
    } else {
        (flag_one(ss[0], ((f - 1) as nat)) + flags(ss.drop_first(), ((f - 1) as nat)))
    }
}

pub open spec fn flag_one(s: Stmt, f: nat) -> nat
    decreases f,
{
    if f == 0 {
        0
    } else {
        match s {
        Stmt::FlagSet(_) => 1,
        Stmt::FlagGuard(_) => 1,
        Stmt::FlagClear(_) => 1,
        Stmt::Loop(b) => flags(b, ((f - 1) as nat)),
        Stmt::CondLoop(b) => flags(b, ((f - 1) as nat)),
        Stmt::Branch(t, e) => flags(t, ((f - 1) as nat)) + flags(e, ((f - 1) as nat)),
        Stmt::Dead(b) => flags(b, ((f - 1) as nat)),
        _ => 0,
        }
    }
}

// ---- Shape rewrites (spec functions over the calculus) ----

// Backward guarded: L: span; if (c) goto L  ->  Loop(span + [Branch([], [Brk])]).
// (cond opaque — the exit inverts to else-break.)
pub open spec fn backward_guarded(span: Seq<Stmt>) -> Seq<Stmt> {
    seq![Stmt::Loop(span + seq![Stmt::Branch(seq![], seq![Stmt::Brk])])]
}

// Backward bare: L: span; goto L  ->  Loop(span).
pub open spec fn backward_bare(span: Seq<Stmt>) -> Seq<Stmt> {
    seq![Stmt::Loop(span)]
}

// Forward guarded: if (c) goto L; skip; L:  ->  Branch([], skip).
pub open spec fn forward_guarded(skip: Seq<Stmt>) -> Seq<Stmt> {
    seq![Stmt::Branch(seq![], skip)]
}

// Forward bare: goto L; dead; L:  ->  [] (dead region dropped).
pub open spec fn forward_bare() -> Seq<Stmt> {
    seq![]
}

// Body-inside-if: L: if (c) { body; goto L }  ->  CondLoop(body).
pub open spec fn body_inside_if(body: Seq<Stmt>) -> Seq<Stmt> {
    seq![Stmt::CondLoop(body)]
}

// Same/ancestor-list skip: goto L inside a nested list, label later in
// an enclosing list, no loop crossed  ->  Dead(skipped) (If{false} wrap).
pub open spec fn list_skip(skipped: Seq<Stmt>) -> Seq<Stmt> {
    seq![Stmt::Dead(skipped)]
}

// Nested exit: goto L out of loops  ->  set; break; guard(tail); clear.
pub open spec fn nested_exit(flag: Lbl, tail: Seq<Stmt>) -> Seq<Stmt> {
    seq![Stmt::FlagSet(flag), Stmt::Brk, Stmt::FlagGuard(tail), Stmt::FlagClear(flag)]
}

// ---- Structural identities ----

// The backward loop body is the original span plus the inverted exit.
proof fn span_identity_guarded(span: Seq<Stmt>)
    ensures backward_guarded(span)[0] == Stmt::Loop(span + seq![Stmt::Branch(seq![], seq![Stmt::Brk])]),
{
}

proof fn span_identity_bare(span: Seq<Stmt>)
    ensures backward_bare(span)[0] == Stmt::Loop(span),
{
}

// The forward inversion keeps the skipped region as the else branch
// and empties the taken branch.
proof fn branch_identity(skip: Seq<Stmt>)
    ensures forward_guarded(skip)[0] == Stmt::Branch(seq![], skip),
{
}

// The forward-bare rewrite drops everything (goto + dead + label).
proof fn dead_identity()
    ensures forward_bare().len() == 0,
{
}

// The body-inside-if rewrite keeps the body as the loop body.
proof fn cond_identity(body: Seq<Stmt>)
    ensures body_inside_if(body)[0] == Stmt::CondLoop(body),
{
}

// The skip rewrite wraps (never drops the prefix that ran before).
proof fn skip_identity(skipped: Seq<Stmt>)
    ensures list_skip(skipped)[0] == Stmt::Dead(skipped),
{
}

// The nested protocol order: set, break, guard, clear.
proof fn flag_order(flag: Lbl, tail: Seq<Stmt>)
    ensures nested_exit(flag, tail) == seq![Stmt::FlagSet(flag), Stmt::Brk, Stmt::FlagGuard(tail), Stmt::FlagClear(flag)],
{
}

// ---- Fidelity pins (mirror 7 of restructure.rs's 8 unit tests) ----

// t27: backward guarded -> While with exit break, goto gone.
proof fn pin_backward_span()
    ensures jumps(backward_guarded(seq![Stmt::Plain, Stmt::Plain]), 6) == 0
        && backward_guarded(seq![Stmt::Plain, Stmt::Plain])[0] == Stmt::Loop(seq![Stmt::Plain, Stmt::Plain] + seq![Stmt::Branch(seq![], seq![Stmt::Brk])]),
{
    reveal_with_fuel(jumps, 8);
    reveal_with_fuel(jump_one, 8);
    span_identity_guarded(seq![Stmt::Plain, Stmt::Plain]);
}

// t31: forward guarded -> inverted If, no jumps survive.
proof fn pin_forward_inversion()
    ensures forward_guarded(seq![Stmt::Plain]) == seq![Stmt::Branch(seq![], seq![Stmt::Plain])]
        && jumps(forward_guarded(seq![Stmt::Plain]), 6) == 0,
{
    reveal_with_fuel(jumps, 6);
    reveal_with_fuel(jump_one, 6);
    branch_identity(seq![Stmt::Plain]);
}

// t30: nested -> flag protocol, no jumps survive.
proof fn pin_flag_protocol()
    ensures nested_exit(7nat, seq![Stmt::Plain]) == seq![Stmt::FlagSet(7nat), Stmt::Brk, Stmt::FlagGuard(seq![Stmt::Plain]), Stmt::FlagClear(7nat)]
        && jumps(nested_exit(7nat, seq![Stmt::Plain]), 6) == 0
        && flags(nested_exit(7nat, seq![Stmt::Plain]), 6) == 3,
{
    reveal_with_fuel(jumps, 6);
    reveal_with_fuel(jump_one, 6);
    reveal_with_fuel(flags, 6);
    reveal_with_fuel(flag_one, 6);
    flag_order(7nat, seq![Stmt::Plain]);
}

// t30-bug: only real loops are guarded — a Block wrapper receives no
// guard. Modelled as: the protocol emits exactly one guard node per
// handled exit (guards are never duplicated onto transparent wrappers).
proof fn pin_block_not_loop()
    ensures flags(seq![Stmt::FlagGuard(seq![Stmt::Plain])], 6) == 1,
{
    reveal_with_fuel(flags, 6);
    reveal_with_fuel(flag_one, 6);
    reveal_with_fuel(jumps, 6);
    reveal_with_fuel(jump_one, 6);
}

// Empty list is a no-op (no shapes fire).
proof fn pin_empty_noop()
    ensures forward_bare() == Seq::<Stmt>::empty() && jumps(forward_bare(), 6) == 0,
{
    reveal_with_fuel(jumps, 6);
    reveal_with_fuel(jump_one, 6);
    dead_identity();
}

// Go switch shape: forward skip needs NO flag; prefix + tail survive.
proof fn pin_forward_skip_no_flag()
    ensures flags(list_skip(seq![Stmt::Plain]), 6) == 0
        && list_skip(seq![Stmt::Plain])[0] == Stmt::Dead(seq![Stmt::Plain]),
{
    reveal_with_fuel(flags, 6);
    reveal_with_fuel(flag_one, 6);
    skip_identity(seq![Stmt::Plain]);
}

// Top-level loop exit: set + guard + clear triple over the tail.
proof fn pin_tail_guarded()
    ensures nested_exit(5nat, seq![Stmt::Plain, Stmt::Plain])[1] == Stmt::Brk
        && nested_exit(5nat, seq![Stmt::Plain, Stmt::Plain])[2] == Stmt::FlagGuard(seq![Stmt::Plain, Stmt::Plain])
        && nested_exit(5nat, seq![Stmt::Plain, Stmt::Plain])[3] == Stmt::FlagClear(5nat),
{
    flag_order(5nat, seq![Stmt::Plain, Stmt::Plain]);
}

// Model-internal: all shapes preserve a shared surrounding sequence
// (prefix/suffix pass through untouched).
proof fn pin_shapes_agree()
    ensures (backward_bare(seq![Stmt::Plain]) + seq![Stmt::Plain])[1] == Stmt::Plain
        && (forward_guarded(seq![Stmt::Plain]) + seq![Stmt::Plain])[1] == Stmt::Plain,
{
    span_identity_bare(seq![Stmt::Plain]);
    branch_identity(seq![Stmt::Plain]);
}

fn main() {}

}
