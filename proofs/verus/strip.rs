// Standalone model of strip_cfor
// (sh2perl/src/shir_passes/strip.rs): the rich C-style
// `ForInit { init, cond, step, body }` lowers to the shell-flavored
// `Block(init ++ [While(cond, splice(body) ++ step)])`, where `splice`
// re-inserts the step before every top-level `continue` (recursing
// into If/Block arms, but NOT into nested loops — their `continue`
// binds to themselves).
//
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
// THEOREM OF RECORD (2026-11-24). The model now proves the pass's
// run-preservation law outright instead of exhibiting it per shape:
//
//   strip_preserves_run: for every store, cond id, step, body and
//   iteration budget, if the step is straight-line (`straight`, i.e.
//   every statement is a `Write` — the source's step is `i = i + 1`)
//   then the ForInit combinator and the lowered While combinator
//   compute the SAME final store:
//     for_loop(s, cid, step, body, iters) == while_loop(s, cid, step, body, iters)
//   and `strip_preserves_lowering` lifts that to the source-shaped
//   wrappers `run_forinit` / `run_lowered` (run init once, then the
//   loop), which is exactly the block `strip_cfor` emits.
//
//   The induction is over the iteration budget, and each iteration is
//   carried by the step-agreement invariant
//
//   splice_run_agrees(body, step, s, cid): running the SPLICED body
//   agrees with running the body, statement-effect by statement-effect —
//      Next: same store, still falls through (the splice is transparent
//            on paths that never reach a continue);
//      Cont: the spliced run ends in Cont with the store = the original
//            store with the step run exactly once on top of it (this is
//            the clause that makes `continue` skip the trailing step
//            without skipping the increment);
//      Brk:  same store, still breaks (the step is NOT run on a break).
//   With `cat_runs` (running `a ++ b` is running `a` and then, if it
//   falls through, running `b`) that is exactly what the while body
//   `splice(body) ++ step` needs: on fallthrough the trailing step runs,
//   on a continue the spliced step has already run, on a break neither
//   runs.
//
//   LIMITS OF THE LAW, stated here rather than hidden in a pin:
//   `straight(step)` is load-bearing, not decoration. A step carrying
//   control flow escapes the increment: with step = [Continue] the
//   ForInit runs it and keeps iterating while the lowered body's
//   trailing Continue ends the iteration, and with step = [Break] the
//   two combinators disagree on whether the loop ends. The mutant that
//   DELETES the premise from `strip_preserves_run` makes the proof of
//   the law itself fail (MUTANT (c) below), so the premise belongs to
//   the law, not to a pin. `cond_holds` stays opaque: the rule never
//   inspects the condition, both sides read the same predicate id
//   against the same store, and the induction is store-driven.
//
// Statements are straight-line commands over a nat store with
// `Continue`/`Break` control effects; the condition is an opaque
// predicate id read against the store (the rule is structural — the
// source never inspects the cond). Blocks are cons lists (`Block`)
// rather than `Seq<Stmt>` ONLY so that the statement runs can be
// structurally recursive in the model: the source recurses to any
// depth (`splice_continue` walks If arms, the interpreter walks the
// statement tree), so the model must not truncate at a fuel bound. The
// earlier version ran statements under a shared fuel and stated only
// per-shape pins; with the fuel shared with the loop, equality is
// measurably FALSE at small fuel (body [Write(0,1), Continue], step
// [Write(1,2)], budget 4: ForInit {0:1, 1:2} vs lowered {0:1}), because
// a fixed budget truncates the longer spliced list at a different
// statement. Fuel therefore appears exactly once, as the loop's
// iteration budget (`iters`) — where the source has an unbounded loop
// and where an induction is honest. The `Box` indirection is Rust's
// requirement for a recursive datatype, not a modelling choice.
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
//     gets the step) and the nested-loop boundary (opaque: a nested
//     loop's own continue/break never escape it).
//   - the expression-position recursion (`strip_expr` into
//     Arrow/Call/Array/…) is traversal, not control flow
//     (array_len_fold precedent: per-site theorem); it cannot move a
//     `Continue` — that is a statement — so it is out of this model.
//
// MUTANT (all three measured 2026-11-24 against this 38-obligation
// file; the primary run was made on the real file and it was restored
// byte-identically — sha256 identical before and after — and re-checked
// clean):
//   (a) PRIMARY — the lowering drops the splice (`while_loop` runs
//       `cat(body, step)` instead of `cat(splice(body, step), step)`):
//       37 verified, 1 error — three assertion failures INSIDE
//       `strip_preserves_run`, on all three effect paths (Next at the
//       "spliced body falls through" step, Cont at "the spliced
//       continue ran the step itself", Brk at "neither side runs the
//       step"). The law goes red; no pin moves. This is the
//       transform's raison d'être.
//   (b) the splice inserts the step AFTER the `Continue`
//       (`cat(one(Continue), cat(step, …))`): 36 verified, 2 errors —
//       `unfold_splice_continue` (its definitional identity) and the
//       Cont clause of `splice_run_agrees`: the invariant is what
//       catches a misplaced splice.
//   (c) delete `requires straight(step)` from the law: 37 verified,
//       1 error — two precondition failures inside `strip_preserves_run`
//       (its calls to `splice_run_agrees` and `straight_runs_next`),
//       so the premise is tied to the law, not to a pin.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 strip.rs
//   -> verification results:: 38 verified, 0 errors
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
    If(nat, Box<Block>, Box<Block>),
    Nested(Box<Block>),
}

// Statement sequences as cons lists, so the interpreter and the splice
// are structurally recursive to any depth (no fuel, matching the
// source's unbounded tree walk).
pub enum Block {
    Nil,
    Cons(Stmt, Box<Block>),
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

// Straight-line blocks: every statement is a `Write`. This is the class
// the source's step lives in (`i = i + 1` is one assignment), and it is
// exactly what the lowering needs: a step whose statements all fall
// through cannot end an iteration early, so the trailing copy the While
// body carries and the copy the ForInit runs after a `continue` agree.
pub open spec fn straight(b: Block) -> bool
    decreases b,
{
    match b {
        Block::Nil => true,
        Block::Cons(Stmt::Write(_, _), r) => straight(*r),
        _ => false,
    }
}

// ---- block helpers -------------------------------------------------

pub open spec fn nil() -> Block {
    Block::Nil
}

pub open spec fn one(st: Stmt) -> Block {
    Block::Cons(st, Box::new(Block::Nil))
}

// Append `b` after `a` (the source's `++`).
pub open spec fn cat(a: Block, b: Block) -> Block
    decreases a,
{
    match a {
        Block::Nil => b,
        Block::Cons(st, r) => Block::Cons(st, Box::new(cat(*r, b))),
    }
}

// The source's `Vec<IrStmt>` entry point: a sequence becomes a block.
pub open spec fn of_seq(ss: Seq<Stmt>) -> Block
    decreases ss.len(),
{
    if ss.len() == 0 {
        Block::Nil
    } else {
        Block::Cons(ss[0], Box::new(of_seq(ss.drop_first())))
    }
}

// ---- the transform -------------------------------------------------

// The splice: step before every top-level continue (If arms recursed;
// nested loops opaque). Structurally recursive, like the source.
pub open spec fn splice(b: Block, step: Block) -> Block
    decreases b,
{
    match b {
        Block::Nil => Block::Nil,
        Block::Cons(Stmt::Continue, r) => cat(
            cat(step, one(Stmt::Continue)),
            splice(*r, step),
        ),
        Block::Cons(Stmt::If(id, t, e), r) => Block::Cons(
            Stmt::If(id, Box::new(splice(*t, step)), Box::new(splice(*e, step))),
            Box::new(splice(*r, step)),
        ),
        Block::Cons(st, r) => Block::Cons(st, Box::new(splice(*r, step))),
    }
}

// ---- statement semantics -------------------------------------------

// Which If arm runs given the store (the cond is opaque).
pub open spec fn arm_of(id: nat, t: Block, e: Block, s: Store) -> Block {
    if cond_holds(id, s) {
        t
    } else {
        e
    }
}

// Run a block from a store; returns the final store and the escaping
// effect (Next = fell through).
pub open spec fn run_block(b: Block, s: Store, cid: nat) -> (Store, Eff)
    decreases b,
{
    match b {
        Block::Nil => (s, Eff::Next),
        Block::Cons(Stmt::Write(k, v), r) => run_block(*r, s.insert(k, v), cid),
        Block::Cons(Stmt::Continue, _) => (s, Eff::Cont),
        Block::Cons(Stmt::Break, _) => (s, Eff::Brk),
        Block::Cons(Stmt::If(id, t, e), r) => {
            let (s1, e1) = run_block(arm_of(id, *t, *e, s), s, cid);
            if e1 == Eff::Next {
                run_block(*r, s1, cid)
            } else {
                (s1, e1)
            }
        },
        // Opaque nested loop: runs its body to completion, never escapes
        // Cont/Brk to this loop (its continues bind inside).
        Block::Cons(Stmt::Nested(nb), r) => run_block(*r, run_block(*nb, s, cid).0, cid),
    }
}

// ---- one-step unfolding lemmas (keep the two inductions readable) ---

pub proof fn unfold_nil(s: Store, cid: nat)
    ensures
        run_block(Block::Nil, s, cid) == (s, Eff::Next),
{
}

pub proof fn unfold_write(k: nat, v: nat, r: Block, s: Store, cid: nat)
    ensures
        run_block(Block::Cons(Stmt::Write(k, v), Box::new(r)), s, cid) == run_block(
            r,
            s.insert(k, v),
            cid,
        ),
{
}

pub proof fn unfold_continue(r: Block, s: Store, cid: nat)
    ensures
        run_block(Block::Cons(Stmt::Continue, Box::new(r)), s, cid) == (s, Eff::Cont),
{
}

pub proof fn unfold_break(r: Block, s: Store, cid: nat)
    ensures
        run_block(Block::Cons(Stmt::Break, Box::new(r)), s, cid) == (s, Eff::Brk),
{
}

pub proof fn unfold_if(id: nat, t: Block, e: Block, r: Block, s: Store, cid: nat)
    ensures
        run_block(Block::Cons(Stmt::If(id, Box::new(t), Box::new(e)), Box::new(r)), s, cid) == (if
            run_block(arm_of(id, t, e, s), s, cid).1 == Eff::Next
        {
            run_block(r, run_block(arm_of(id, t, e, s), s, cid).0, cid)
        } else {
            run_block(arm_of(id, t, e, s), s, cid)
        }),
{
}

pub proof fn unfold_nested(nb: Block, r: Block, s: Store, cid: nat)
    ensures
        run_block(Block::Cons(Stmt::Nested(Box::new(nb)), Box::new(r)), s, cid) == run_block(
            r,
            run_block(nb, s, cid).0,
            cid,
        ),
{
}

pub proof fn unfold_arm_true(id: nat, t: Block, e: Block, s: Store)
    requires
        cond_holds(id, s),
    ensures
        arm_of(id, t, e, s) == t,
{
}

pub proof fn unfold_arm_false(id: nat, t: Block, e: Block, s: Store)
    requires
        !cond_holds(id, s),
    ensures
        arm_of(id, t, e, s) == e,
{
}

pub proof fn unfold_cat_nil(b: Block)
    ensures
        cat(Block::Nil, b) == b,
{
}

pub proof fn unfold_cat_cons(st: Stmt, r: Block, b: Block)
    ensures
        cat(Block::Cons(st, Box::new(r)), b) == Block::Cons(st, Box::new(cat(r, b))),
{
}

// Appending nothing on the right is the identity (so the splice's
// `cat(step ++ [Continue], splice(rest))` really is the source's
// `step ++ [Continue] ++ rest`).
pub proof fn cat_nil_right(a: Block)
    ensures
        cat(a, Block::Nil) == a,
    decreases a,
{
    match a {
        Block::Nil => {
            unfold_cat_nil(Block::Nil);
        },
        Block::Cons(st, r) => {
            unfold_cat_cons(st, *r, Block::Nil);
            cat_nil_right(*r);
        },
    }
}

pub proof fn unfold_splice_nil(step: Block)
    ensures
        splice(Block::Nil, step) == Block::Nil,
{
}

pub proof fn unfold_splice_continue(r: Block, step: Block)
    ensures
        splice(Block::Cons(Stmt::Continue, Box::new(r)), step) == cat(
            cat(step, one(Stmt::Continue)),
            splice(r, step),
        ),
{
}

pub proof fn unfold_splice_break(r: Block, step: Block)
    ensures
        splice(Block::Cons(Stmt::Break, Box::new(r)), step) == Block::Cons(
            Stmt::Break,
            Box::new(splice(r, step)),
        ),
{
}

pub proof fn unfold_splice_if(id: nat, t: Block, e: Block, r: Block, step: Block)
    ensures
        splice(
            Block::Cons(Stmt::If(id, Box::new(t), Box::new(e)), Box::new(r)),
            step,
        ) == Block::Cons(
            Stmt::If(id, Box::new(splice(t, step)), Box::new(splice(e, step))),
            Box::new(splice(r, step)),
        ),
{
}

pub proof fn unfold_splice_nested(nb: Block, r: Block, step: Block)
    ensures
        splice(Block::Cons(Stmt::Nested(Box::new(nb)), Box::new(r)), step) == Block::Cons(
            Stmt::Nested(Box::new(nb)),
            Box::new(splice(r, step)),
        ),
{
}

pub proof fn unfold_one(st: Stmt)
    ensures
        one(st) == Block::Cons(st, Box::new(Block::Nil)),
{
}

// ---- the two loop combinators --------------------------------------

// The two loop combinators. Their guards are the same (budget left and
// the opaque condition); they differ only in what one iteration runs.
pub open spec fn for_loop(s: Store, cid: nat, step: Block, body: Block, iters: nat) -> Store
    decreases iters,
{
    if iters == 0 || !cond_holds(cid, s) {
        s
    } else {
        let (s1, e1) = run_block(body, s, cid);
        if e1 == Eff::Brk {
            s1
        } else {
            // Cont and Next both run the step, then re-check.
            let (s2, _e2) = run_block(step, s1, cid);
            for_loop(s2, cid, step, body, (iters - 1) as nat)
        }
    }
}

// The lowered While body is splice(body) followed by the step. A Cont
// inside a spliced arm has already run its own copy of the step, so it
// re-checks the cond without running anything else; a Brk leaves with
// the trailing step skipped.
pub open spec fn while_loop(s: Store, cid: nat, step: Block, body: Block, iters: nat) -> Store
    decreases iters,
{
    if iters == 0 || !cond_holds(cid, s) {
        s
    } else {
        let (s1, e1) = run_block(cat(splice(body, step), step), s, cid);
        if e1 == Eff::Brk {
            s1
        } else {
            while_loop(s1, cid, step, body, (iters - 1) as nat)
        }
    }
}

pub proof fn unfold_for_loop(s: Store, cid: nat, step: Block, body: Block, iters: nat)
    ensures
        for_loop(s, cid, step, body, iters) == (if iters == 0 || !cond_holds(cid, s) {
            s
        } else if run_block(body, s, cid).1 == Eff::Brk {
            run_block(body, s, cid).0
        } else {
            for_loop(
                run_block(step, run_block(body, s, cid).0, cid).0,
                cid,
                step,
                body,
                (iters - 1) as nat,
            )
        }),
{
}

pub proof fn unfold_while_loop(s: Store, cid: nat, step: Block, body: Block, iters: nat)
    ensures
        while_loop(s, cid, step, body, iters) == (if iters == 0 || !cond_holds(cid, s) {
            s
        } else if run_block(cat(splice(body, step), step), s, cid).1 == Eff::Brk {
            run_block(cat(splice(body, step), step), s, cid).0
        } else {
            while_loop(
                run_block(cat(splice(body, step), step), s, cid).0,
                cid,
                step,
                body,
                (iters - 1) as nat,
            )
        }),
{
}

// The source-shaped wrappers: Block(init ++ [While(cond, body')]) means
// run init once, then iterate. (They differ only in their loop.)
pub open spec fn run_forinit(
    init: Seq<Stmt>,
    cid: nat,
    step: Block,
    body: Block,
    s: Store,
    iters: nat,
) -> Store
    decreases iters,
{
    for_loop(run_block(of_seq(init), s, cid).0, cid, step, body, iters)
}

pub open spec fn run_lowered(
    init: Seq<Stmt>,
    cid: nat,
    step: Block,
    body: Block,
    s: Store,
    iters: nat,
) -> Store
    decreases iters,
{
    while_loop(run_block(of_seq(init), s, cid).0, cid, step, body, iters)
}

// ---- step-agreement invariant --------------------------------------

// Running `a` then `b` is running `a` and, if it falls through, running
// `b` on the store it produced.
pub proof fn cat_runs(a: Block, b: Block, s: Store, cid: nat)
    ensures
        run_block(a, s, cid).1 == Eff::Next ==> run_block(cat(a, b), s, cid) == run_block(
            b,
            run_block(a, s, cid).0,
            cid,
        ),
        run_block(a, s, cid).1 != Eff::Next ==> run_block(cat(a, b), s, cid) == run_block(
            a,
            s,
            cid,
        ),
    decreases a,
{
    match a {
        Block::Nil => {
            unfold_nil(s, cid);
            unfold_cat_nil(b);
        },
        Block::Cons(st, r) => {
            match st {
                Stmt::Continue => {
                    unfold_continue(*r, s, cid);
                    unfold_cat_cons(Stmt::Continue, *r, b);
                    unfold_continue(cat(*r, b), s, cid);
                },
                Stmt::Break => {
                    unfold_break(*r, s, cid);
                    unfold_cat_cons(Stmt::Break, *r, b);
                    unfold_break(cat(*r, b), s, cid);
                },
                Stmt::Write(k, v) => {
                    unfold_write(k, v, *r, s, cid);
                    unfold_cat_cons(Stmt::Write(k, v), *r, b);
                    unfold_write(k, v, cat(*r, b), s, cid);
                    cat_runs(*r, b, s.insert(k, v), cid);
                },
                Stmt::If(id, t, e) => {
                    unfold_if(id, *t, *e, *r, s, cid);
                    unfold_cat_cons(Stmt::If(id, t, e), *r, b);
                    unfold_if(id, *t, *e, cat(*r, b), s, cid);
                    let (s1, e1) = run_block(arm_of(id, *t, *e, s), s, cid);
                    cat_runs(*r, b, s1, cid);
                },
                Stmt::Nested(nb) => {
                    unfold_nested(*nb, *r, s, cid);
                    unfold_cat_cons(Stmt::Nested(nb), *r, b);
                    unfold_nested(*nb, cat(*r, b), s, cid);
                    cat_runs(*r, b, run_block(*nb, s, cid).0, cid);
                },
            }
        },
    }
}

// A straight block always falls through.
pub proof fn straight_runs_next(b: Block, s: Store, cid: nat)
    requires
        straight(b),
    ensures
        run_block(b, s, cid).1 == Eff::Next,
    decreases b,
{
    match b {
        Block::Nil => {
            unfold_nil(s, cid);
        },
        Block::Cons(st, r) => {
            match st {
                Stmt::Write(k, v) => {
                    unfold_write(k, v, *r, s, cid);
                    straight_runs_next(*r, s.insert(k, v), cid);
                },
                _ => {},
            }
        },
    }
}

// THE INVARIANT: the spliced body agrees with the unspliced body, effect
// by effect. This is the lemma the loop induction consumes once per
// iteration.
pub proof fn splice_run_agrees(b: Block, step: Block, s: Store, cid: nat)
    requires
        straight(step),
    ensures
        run_block(b, s, cid).1 == Eff::Next ==> run_block(splice(b, step), s, cid) == run_block(
            b,
            s,
            cid,
        ),
        run_block(b, s, cid).1 == Eff::Cont ==> {
            &&& run_block(splice(b, step), s, cid).1 == Eff::Cont
            &&& run_block(splice(b, step), s, cid).0 == run_block(
                step,
                run_block(b, s, cid).0,
                cid,
            ).0
        },
        run_block(b, s, cid).1 == Eff::Brk ==> run_block(splice(b, step), s, cid) == run_block(
            b,
            s,
            cid,
        ),
    decreases b,
{
    match b {
        Block::Nil => {
            unfold_nil(s, cid);
        },
        Block::Cons(st, r) => {
            match st {
                Stmt::Continue => {
                    unfold_continue(*r, s, cid);
                    unfold_one(Stmt::Continue);
                    cat_runs(step, one(Stmt::Continue), s, cid);
                    straight_runs_next(step, s, cid);
                    let (s1, _) = run_block(step, s, cid);
                    cat_runs(cat(step, one(Stmt::Continue)), splice(*r, step), s, cid);
                    unfold_continue(splice(*r, step), s1, cid);
                },
                Stmt::Break => {
                    unfold_break(*r, s, cid);
                },
                Stmt::Write(k, v) => {
                    unfold_write(k, v, *r, s, cid);
                    splice_run_agrees(*r, step, s.insert(k, v), cid);
                },
                Stmt::If(id, t, e) => {
                    unfold_if(id, *t, *e, *r, s, cid);
                    if cond_holds(id, s) {
                        unfold_arm_true(id, *t, *e, s);
                        splice_run_agrees(*t, step, s, cid);
                    } else {
                        unfold_arm_false(id, *t, *e, s);
                        splice_run_agrees(*e, step, s, cid);
                    }
                    let (s1, e1) = run_block(arm_of(id, *t, *e, s), s, cid);
                    if e1 == Eff::Next {
                        splice_run_agrees(*r, step, s1, cid);
                    }
                },
                Stmt::Nested(nb) => {
                    unfold_nested(*nb, *r, s, cid);
                    splice_run_agrees(*r, step, run_block(*nb, s, cid).0, cid);
                },
            }
        },
    }
}

// ---- THEOREM OF RECORD ---------------------------------------------

// The lowering preserves the run: for any iteration budget and any
// store, the ForInit combinator and the lowered While combinator agree.
// Universal in every parameter; the only premise is that the step is
// straight-line, which is the class the source's step belongs to and
// which the proof needs (see the header's LIMITS paragraph).
pub proof fn strip_preserves_run(s: Store, cid: nat, step: Block, body: Block, iters: nat)
    requires
        straight(step),
    ensures
        for_loop(s, cid, step, body, iters) == while_loop(s, cid, step, body, iters),
    decreases iters,
{
    unfold_for_loop(s, cid, step, body, iters);
    unfold_while_loop(s, cid, step, body, iters);
    if iters == 0 || !cond_holds(cid, s) {
        assert(for_loop(s, cid, step, body, iters) == s);
        assert(while_loop(s, cid, step, body, iters) == s);
    } else {
        let (sb, eb) = run_block(body, s, cid);
        let (ss1, es) = run_block(step, sb, cid);
        splice_run_agrees(body, step, s, cid);
        straight_runs_next(step, sb, cid);
        if eb == Eff::Next {
            // Spliced body falls through to the same store, then the
            // trailing step runs: both sides continue from ss1.
            cat_runs(splice(body, step), step, s, cid);
            assert(run_block(splice(body, step), s, cid) == (sb, Eff::Next));
            assert(run_block(cat(splice(body, step), step), s, cid) == (ss1, Eff::Next));
            assert(for_loop(s, cid, step, body, iters) == for_loop(
                ss1,
                cid,
                step,
                body,
                (iters - 1) as nat,
            ));
            assert(while_loop(s, cid, step, body, iters) == while_loop(
                ss1,
                cid,
                step,
                body,
                (iters - 1) as nat,
            ));
            strip_preserves_run(ss1, cid, step, body, (iters - 1) as nat);
        } else if eb == Eff::Cont {
            // The spliced continue ran the step itself: the lowered
            // body ends in Cont at exactly ss1, so the trailing step is
            // skipped on both sides.
            assert(run_block(splice(body, step), s, cid).0 == ss1);
            assert(run_block(splice(body, step), s, cid).1 == Eff::Cont);
            cat_runs(splice(body, step), step, s, cid);
            assert(run_block(cat(splice(body, step), step), s, cid) == (ss1, Eff::Cont));
            assert(for_loop(s, cid, step, body, iters) == for_loop(
                ss1,
                cid,
                step,
                body,
                (iters - 1) as nat,
            ));
            assert(while_loop(s, cid, step, body, iters) == while_loop(
                ss1,
                cid,
                step,
                body,
                (iters - 1) as nat,
            ));
            strip_preserves_run(ss1, cid, step, body, (iters - 1) as nat);
        } else {
            // Break: neither side runs the step.
            assert(eb == Eff::Brk);
            assert(run_block(splice(body, step), s, cid) == (sb, Eff::Brk));
            cat_runs(splice(body, step), step, s, cid);
            assert(run_block(cat(splice(body, step), step), s, cid) == (sb, Eff::Brk));
            assert(for_loop(s, cid, step, body, iters) == sb);
            assert(while_loop(s, cid, step, body, iters) == sb);
        }
    }
}

// The theorem at the source's shape: `strip_cfor` emits
// Block(init ++ [While(cond, splice(body) ++ step)]), and that whole
// lowering runs the same as the ForInit it replaced. The init run is
// literally the same block on both sides; the loop is the theorem
// above.
pub proof fn strip_preserves_lowering(
    init: Seq<Stmt>,
    cid: nat,
    step: Block,
    body: Block,
    s: Store,
    iters: nat,
)
    requires
        straight(step),
    ensures
        run_lowered(init, cid, step, body, s, iters) == run_forinit(init, cid, step, body, s, iters),
    decreases iters,
{
    strip_preserves_run(run_block(of_seq(init), s, cid).0, cid, step, body, iters);
}

// ---- Fidelity pins (mirror the source's 1 remaining unit test) -----

// A top-level first-class continue splices the step before it.
proof fn pin_top_continue_spliced()
    ensures
        splice(
            one(Stmt::Continue),
            one(Stmt::Write(0nat, 1nat)),
        ) == Block::Cons(Stmt::Write(0nat, 1nat), Box::new(one(Stmt::Continue))),
{
    unfold_one(Stmt::Continue);
    unfold_one(Stmt::Write(0nat, 1nat));
    unfold_splice_continue(Block::Nil, one(Stmt::Write(0nat, 1nat)));
    unfold_splice_nil(one(Stmt::Write(0nat, 1nat)));
    cat_nil_right(cat(one(Stmt::Write(0nat, 1nat)), one(Stmt::Continue)));
    unfold_cat_cons(Stmt::Write(0nat, 1nat), Block::Nil, one(Stmt::Continue));
    unfold_cat_nil(one(Stmt::Continue));
}

// A break is kept as-is (no step splice — break leaves the loop).
proof fn pin_break_kept()
    ensures
        splice(one(Stmt::Break), one(Stmt::Write(0nat, 1nat))) == one(Stmt::Break),
{
    unfold_one(Stmt::Break);
    unfold_splice_break(Block::Nil, one(Stmt::Write(0nat, 1nat)));
    unfold_splice_nil(one(Stmt::Write(0nat, 1nat)));
}

// A continue inside an If arm gets the step (splice recurses).
proof fn pin_if_arm_continue_spliced()
    ensures
        splice(
            one(Stmt::If(1nat, Box::new(one(Stmt::Continue)), Box::new(Block::Nil))),
            one(Stmt::Write(0nat, 1nat)),
        ) == one(
            Stmt::If(
                1nat,
                Box::new(Block::Cons(Stmt::Write(0nat, 1nat), Box::new(one(Stmt::Continue)))),
                Box::new(Block::Nil),
            ),
        ),
{
    pin_top_continue_spliced();
    unfold_one(Stmt::If(1nat, Box::new(one(Stmt::Continue)), Box::new(Block::Nil)));
    unfold_splice_if(
        1nat,
        one(Stmt::Continue),
        Block::Nil,
        Block::Nil,
        one(Stmt::Write(0nat, 1nat)),
    );
    unfold_splice_nil(one(Stmt::Write(0nat, 1nat)));
    unfold_one(
        Stmt::If(
            1nat,
            Box::new(Block::Cons(Stmt::Write(0nat, 1nat), Box::new(one(Stmt::Continue)))),
            Box::new(Block::Nil),
        ),
    );
}

// A nested loop body is opaque (its continues bind inside).
proof fn pin_nested_opaque()
    ensures
        splice(
            one(Stmt::Nested(Box::new(one(Stmt::Continue)))),
            one(Stmt::Write(0nat, 1nat)),
        ) == one(Stmt::Nested(Box::new(one(Stmt::Continue)))),
{
    unfold_one(Stmt::Nested(Box::new(one(Stmt::Continue))));
    unfold_splice_nested(
        one(Stmt::Continue),
        Block::Nil,
        one(Stmt::Write(0nat, 1nat)),
    );
    unfold_splice_nil(one(Stmt::Write(0nat, 1nat)));
}

// The lowered normal path runs the trailing step: with an empty body the
// While body is exactly `splice(nil) ++ step`, so it runs the step.
proof fn pin_normal_path_runs_step()
    ensures
        run_block(
            cat(splice(nil(), one(Stmt::Write(0nat, 1nat))), one(Stmt::Write(0nat, 1nat))),
            Map::<nat, nat>::empty(),
            7nat,
        ).0 == run_block(one(Stmt::Write(0nat, 1nat)), Map::<nat, nat>::empty(), 7nat).0,
{
    unfold_one(Stmt::Write(0nat, 1nat));
    unfold_splice_nil(one(Stmt::Write(0nat, 1nat)));
    unfold_cat_nil(one(Stmt::Write(0nat, 1nat)));
    unfold_write(0nat, 1nat, Block::Nil, Map::<nat, nat>::empty(), 7nat);
    unfold_nil(Map::<nat, nat>::empty().insert(0nat, 1nat), 7nat);
}

// The verdict: a first-class continue escapes as Cont, a break does not.
proof fn pin_continue_verdict()
    ensures
        is_continue(Stmt::Continue) && !is_continue(Stmt::Break),
{
}

fn main() {}

}
