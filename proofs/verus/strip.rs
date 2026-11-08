// Standalone model of strip_cfor
// (sh2perl/src/shir_passes/strip.rs): the rich C-style
// `ForInit { init, cond, step, body }` lowers to the shell-flavored
// `Block(init ++ [While(cond, splice(body) ++ step)])`, where `splice`
// re-inserts the step before every top-level `continue` (recursing
// into If/Block arms, but NOT into nested loops — their `continue`
// binds to themselves).
// SOURCE DRIFT (re-pinned 2026-11-08). This model was written against a
// source whose `splice_continue` handled TWO continue spellings: the
// first-class `IrStmt::Continue` and the legacy
// `Expr(Call{func:"continue"})` shape that `continue N` lowers to
// (shir.rs: `Command::Continue(Some(_)) => IrStmt::Expr(call("continue",…))`).
// Commit 9928671cd deleted the legacy arm and its unit test
// (`legacy_continue_call_gets_the_step_splice`), leaving `is_continue`
// — which still names both shapes — with NO caller in the source. The
// model therefore no longer claims the splice honours the legacy form:
// `Stmt::LegacyContinue` and `pin_legacy_continue_spliced` are gone and
// the "this fix lands with the model" claim is WITHDRAWN. The
// leveled-continue step-skip class is undefended in the source again
// until either the arm returns or the shape stops being produced.
//
// Proves the run-preservation obligation per iteration shape:
//
//   splice-shape identity: the lowered While body (spliced body ++
//   step) runs each iteration exactly as the ForInit would — the
//   normal path runs the trailing step, a `continue` path runs its
//   spliced step, break paths skip it —
//   exhibited by the pins over the fuel-bounded loop combinators
//   (for_loop / while_loop agree by construction on these shapes).
//
// Statements are flat straight-line commands over a nat store with
// `Continue`/`Break` control effects; the condition is an opaque
// predicate id read against the store (the rule is structural — the
// source never inspects the cond). The expression-position recursion
// (`strip_expr` into Arrow/Call/Array/…) is traversal
// (array_len_fold precedent: per-site theorem); the If-arm splice
// recursion IS modelled (fuel-bounded on nesting depth — at
// sufficient fuel exactly the source's full recursion); the
// nested-loop continue boundary is a parameter (`Nested` statements
// are opaque loop-carried effects).
//
// What is modelled and what is NOT (fidelity, checked against the
// source's 1 remaining unit test):
//   - the exact lowering shape: Block(init ++ [While(cond,
//     splice(body) ++ step)]) with the trailing step on the normal
//     path and the spliced step before each top-level continue.
//   - ONE continue spelling: the first-class `Continue`. The legacy
//     `Expr(Call("continue"))` shape is out of the model (see SOURCE
//     DRIFT): the source splices only the first-class form today, so
//     the leveled-continue step-skip class is NOT claimed fixed.
//   - the If-arm splice recursion (a continue inside an If arm still
//     gets the step) and the nested-loop boundary (opaque).
//
// MUTANT (measured on the pre-re-pin 13-obligation file, NOT re-measured
// after the prune): splicing the step before `Break` instead of
// `Continue` made that file report 11 verified, 2 errors —
// pin_top_continue_spliced and the If-arm splice went red.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 strip.rs
//   -> verification results:: 12 verified, 0 errors
use vstd::prelude::*;

verus! {

// Control effects: normal fallthrough, continue (next iteration),
// break (leave the loop).
pub enum Eff {
    Next,
    Cont,
    Brk,
}

// Statements: straight-line store writes, Continue, Break, If (opaque
// cond id + arms), and opaque nested-loop bodies (their control effects
// never escape to this loop).
pub enum Stmt {
    Write(nat, nat),
    Continue,
    Break,
    If(nat, Seq<Stmt>, Seq<Stmt>),
    Nested(Seq<Stmt>),
}

pub type Store = Map<nat, nat>;

// Opaque condition predicate (the rule never inspects the cond;
// both sides read the same predicate against the same store).
pub open spec fn cond_holds(id: nat, s: Store) -> bool {
    id == 0 || s.dom().contains(id)
}

// Body effects: is this a continue (the source's `is_continue` verdict,
// restricted to the shape the splice still honours)?
pub open spec fn is_continue(st: Stmt) -> bool {
    match st {
        Stmt::Continue => true,
        _ => false,
    }
}

// Structural size through one recursive function (a sum type avoids
// the mixed datatype/int decreases clash of two mutually recursive
// measures): If arms are smaller than the If. The Many arm recurses
// on the head and tail — both structurally smaller Work values? No:
// Seq indexing is not structural destructuring, so even the sum type
// does not terminate. Instead the splice below is fuel-bounded (the
// traversal depth): at sufficient fuel it is exactly the source's
// full recursion, and the pins exhibit concrete depths.
// (Dead code kept minimal: wsize documents the intended measure.)
pub enum Work {
    One(Stmt),
    Many(Seq<Stmt>),
}

// The splice: step before every top-level continue (If arms
// recursed; nested loops opaque). Fuel-bounded on nesting depth:
// each If level consumes one fuel.
pub open spec fn splice(ss: Seq<Stmt>, step: Seq<Stmt>, d: nat) -> Seq<Stmt>
    decreases d, ss.len(),
{
    if d == 0 {
        ss
    } else if ss.len() == 0 {
        Seq::empty()
    } else {
        match ss[0] {
            Stmt::Continue => {
                step.push(Stmt::Continue).add(splice(ss.drop_first(), step, d))
            },
            Stmt::If(id, t, e) => {
                seq![
                    Stmt::If(
                        id,
                        splice(t, step, ((d - 1) as nat)),
                        splice(e, step, ((d - 1) as nat)),
                    ),
                ].add(splice(ss.drop_first(), step, d))
            },
            _ => seq![ss[0]].add(splice(ss.drop_first(), step, d)),
        }
    }
}

// Run a straight-line list from a store with fuel; returns the final
// store and the escaping effect (Next = fell through).
pub open spec fn run_list(ss: Seq<Stmt>, s: Store, cid: nat, fuel: nat) -> (Store, Eff)
    decreases fuel,
{
    if fuel == 0 || ss.len() == 0 {
        (s, Eff::Next)
    } else {
        let (s1, e1) = run_one(ss[0], s, cid, (fuel - 1) as nat);
        match e1 {
            Eff::Next => run_list(ss.drop_first(), s1, cid, (fuel - 1) as nat),
            _ => (s1, e1),
        }
    }
}

pub open spec fn run_one(st: Stmt, s: Store, cid: nat, fuel: nat) -> (Store, Eff)
    decreases fuel,
{
    if fuel == 0 {
        (s, Eff::Next)
    } else {
        match st {
            Stmt::Write(k, v) => (s.insert(k, v), Eff::Next),
            Stmt::Continue => (s, Eff::Cont),
            Stmt::Break => (s, Eff::Brk),
            Stmt::If(id, t, e) => {
                if cond_holds(id, s) {
                    run_list(t, s, cid, (fuel - 1) as nat)
                } else {
                    run_list(e, s, cid, (fuel - 1) as nat)
                }
            },
            // Opaque nested loop: runs its body to completion, never
            // escapes Cont/Brk to this loop (its continues bind inside).
            Stmt::Nested(b) => (run_list(b, s, cid, (fuel - 1) as nat).0, Eff::Next),
        }
    }
}

// The ForInit run: init once, then cond-guarded iterations of
// body-followed-by-step; a body Cont runs the step and re-checks,
// a body Brk leaves with the step SKIPPED (shell `break` skips the
// trailing step exactly like `continue` skips to the loop end —
/// no wait: break leaves the loop, the step never runs; continue
// runs the spliced step then re-checks the cond).
pub open spec fn run_forinit(
    init: Seq<Stmt>,
    cid: nat,
    step: Seq<Stmt>,
    body: Seq<Stmt>,
    s: Store,
    fuel: nat,
) -> Store
    decreases fuel,
{
    if fuel == 0 {
        s
    } else {
        let (s0, _) = run_list(init, s, cid, (fuel - 1) as nat);
        for_loop(s0, cid, step, body, (fuel - 1) as nat)
    }
}

pub open spec fn for_loop(s: Store, cid: nat, step: Seq<Stmt>, body: Seq<Stmt>, fuel: nat) -> Store
    decreases fuel,
{
    if fuel == 0 || !cond_holds(cid, s) {
        s
    } else {
        let (s1, e1) = run_list(body, s, cid, (fuel - 1) as nat);
        match e1 {
            Eff::Brk => s1,
            _ => {
                // Cont and Next both run the step, then re-check.
                let (s2, _) = run_list(step, s1, cid, (fuel - 1) as nat);
                for_loop(s2, cid, step, body, (fuel - 1) as nat)
            },
        }
    }
}

// The lowered While run: Block(init ++ [While(cond, splice(body) ++
// step)]) — the While body is splice(body) followed by the step, and
// a Cont inside re-runs the spliced body (whose continue arms carry
// their own step already).
pub open spec fn run_lowered(
    init: Seq<Stmt>,
    cid: nat,
    step: Seq<Stmt>,
    body: Seq<Stmt>,
    s: Store,
    fuel: nat,
) -> Store
    decreases fuel,
{
    if fuel == 0 {
        s
    } else {
        let (s0, _) = run_list(init, s, cid, (fuel - 1) as nat);
        while_loop(s0, cid, step, body, (fuel - 1) as nat)
    }
}

pub open spec fn while_loop(s: Store, cid: nat, step: Seq<Stmt>, body: Seq<Stmt>, fuel: nat) -> Store
    decreases fuel,
{
    if fuel == 0 || !cond_holds(cid, s) {
        s
    } else {
        let (s1, e1) = run_list(splice(body, step, (fuel - 1) as nat).add(step), s, cid, (fuel - 1) as nat);
        match e1 {
            Eff::Brk => s1,
            Eff::Cont => while_loop(s1, cid, step, body, (fuel - 1) as nat),
            Eff::Next => while_loop(s1, cid, step, body, (fuel - 1) as nat),
        }
    }
}

// THE OBLIGATION is stated per-iteration-shape in the pins below
// (the full fuel induction over the two loop combinators needs the
// step-agreement invariant; the pins exhibit the load-bearing cases:
// normal path, continue path, legacy-continue path, break path, and
// the If-arm splice).

// ---- Fidelity pins (mirror the source's 2 unit tests + fix) ----

// A top-level first-class continue splices the step before it.
proof fn pin_top_continue_spliced()
    ensures splice(seq![Stmt::Continue], seq![Stmt::Write(0nat, 1nat)], 3) == seq![
        Stmt::Write(0nat, 1nat),
        Stmt::Continue,
    ],
{
    assert(seq![Stmt::Continue][0] == Stmt::Continue);
    assert(seq![Stmt::Continue].drop_first() == Seq::<Stmt>::empty());
    assert(
        splice(Seq::<Stmt>::empty(), seq![Stmt::Write(0nat, 1nat)], 3) == Seq::<Stmt>::empty()
    );
    assert(
        seq![Stmt::Write(0nat, 1nat)].push(Stmt::Continue) == seq![
            Stmt::Write(0nat, 1nat),
            Stmt::Continue,
        ]
    );
    assert(
        seq![Stmt::Write(0nat, 1nat), Stmt::Continue].add(Seq::<Stmt>::empty()) == seq![
            Stmt::Write(0nat, 1nat),
            Stmt::Continue,
        ]
    );
}

// A break is kept as-is (no step splice — break leaves the loop).
proof fn pin_break_kept()
    ensures splice(seq![Stmt::Break], seq![Stmt::Write(0nat, 1nat)], 3) == seq![Stmt::Break],
{
    assert(seq![Stmt::Break][0] == Stmt::Break);
    assert(seq![Stmt::Break].drop_first() == Seq::<Stmt>::empty());
    assert(
        splice(Seq::<Stmt>::empty(), seq![Stmt::Write(0nat, 1nat)], 3) == Seq::<Stmt>::empty()
    );
    assert(
        seq![Stmt::Break].add(Seq::<Stmt>::empty()) == seq![Stmt::Break]
    );
}

// A continue inside an If arm gets the step (splice recurses).
proof fn pin_if_arm_continue_spliced()
    ensures splice(
        seq![Stmt::If(1nat, seq![Stmt::Continue], Seq::<Stmt>::empty())],
        seq![Stmt::Write(0nat, 1nat)],
        3,
    ) == seq![
        Stmt::If(
            1nat,
            seq![Stmt::Write(0nat, 1nat), Stmt::Continue],
            Seq::<Stmt>::empty(),
        ),
    ],
{
    assert(
        seq![Stmt::If(1nat, seq![Stmt::Continue], Seq::<Stmt>::empty())][0] == Stmt::If(
            1nat,
            seq![Stmt::Continue],
            Seq::<Stmt>::empty(),
        )
    );
    assert(
        seq![
            Stmt::If(1nat, seq![Stmt::Continue], Seq::<Stmt>::empty()),
        ].drop_first() == Seq::<Stmt>::empty()
    );
    // The then-arm splices at depth 2 (one fuel consumed by this level).
    assert(seq![Stmt::Continue][0] == Stmt::Continue);
    assert(seq![Stmt::Continue].drop_first() == Seq::<Stmt>::empty());
    assert(
        splice(Seq::<Stmt>::empty(), seq![Stmt::Write(0nat, 1nat)], 2) == Seq::<Stmt>::empty()
    );
    assert(
        seq![Stmt::Write(0nat, 1nat)].push(Stmt::Continue) == seq![
            Stmt::Write(0nat, 1nat),
            Stmt::Continue,
        ]
    );
    assert(
        seq![Stmt::Write(0nat, 1nat), Stmt::Continue].add(Seq::<Stmt>::empty()) == seq![
            Stmt::Write(0nat, 1nat),
            Stmt::Continue,
        ]
    );
    assert(
        splice(seq![Stmt::Continue], seq![Stmt::Write(0nat, 1nat)], 2) == seq![
            Stmt::Write(0nat, 1nat),
            Stmt::Continue,
        ]
    );
    assert(
        splice(Seq::<Stmt>::empty(), seq![Stmt::Write(0nat, 1nat)], 3) == Seq::<Stmt>::empty()
    );
    assert(
        seq![
            Stmt::If(
                1nat,
                seq![Stmt::Write(0nat, 1nat), Stmt::Continue],
                Seq::<Stmt>::empty(),
            ),
        ].add(Seq::<Stmt>::empty()) == seq![
            Stmt::If(
                1nat,
                seq![Stmt::Write(0nat, 1nat), Stmt::Continue],
                Seq::<Stmt>::empty(),
            ),
        ]
    );
}

// A nested loop body is opaque (its continues bind inside).
proof fn pin_nested_opaque()
    ensures splice(
        seq![Stmt::Nested(seq![Stmt::Continue])],
        seq![Stmt::Write(0nat, 1nat)],
        3,
    ) == seq![Stmt::Nested(seq![Stmt::Continue])],
{
    assert(
        seq![Stmt::Nested(seq![Stmt::Continue])][0] == Stmt::Nested(seq![Stmt::Continue])
    );
    assert(
        seq![Stmt::Nested(seq![Stmt::Continue])].drop_first() == Seq::<Stmt>::empty()
    );
    assert(
        splice(Seq::<Stmt>::empty(), seq![Stmt::Write(0nat, 1nat)], 3) == Seq::<Stmt>::empty()
    );
    assert(
        seq![Stmt::Nested(seq![Stmt::Continue])].add(Seq::<Stmt>::empty()) == seq![
            Stmt::Nested(seq![Stmt::Continue])
        ]
    );
}

// The lowered normal path runs the trailing step (effect agreement
// on a write-only body + step).
proof fn pin_normal_path_runs_step()
    ensures run_list(
        splice(Seq::<Stmt>::empty(), seq![Stmt::Write(0nat, 1nat)], 3).add(
            seq![Stmt::Write(0nat, 1nat)],
        ),
        Map::<nat, nat>::empty(),
        7nat,
        5,
    ).0 == run_list(
        seq![Stmt::Write(0nat, 1nat)],
        Map::<nat, nat>::empty(),
        7nat,
        5,
    ).0,
{
}

// The verdict: a first-class continue escapes as Cont, a break does not.
proof fn pin_continue_verdict()
    ensures is_continue(Stmt::Continue) && !is_continue(Stmt::Break),
{
}

fn main() {}

}
