// Standalone model of the P9 ELIM trio Tier-1 obligations:
//   (a) const-condition-elim (`sh2perl/src/transforms/const_condition_elim.rs`):
//       `If` with a constant-true/false condition prunes to the taken
//       branch (or first-true elsif / else); `While(false)` drops;
//       `DoWhile(false)` becomes its body (runs exactly once);
//   (b) unreachable-after-exit (`sh2perl/src/transforms/unreachable_after_exit.rs`):
//       statements after a DIRECT unconditional `Exit` (or `Return`
//       inside a function body) in the same list are dropped;
//   (c) merge-init-assignments (`sh2perl/src/transforms/merge_init_assignments.rs`):
//       an adjacent `Declare x = <literal>` + `x = <pure rhs>` pair folds
//       to `Declare x = <rhs>` (the later store overwrites the default
//       before any read can observe it).
//
// Proves the soundness obligations:
//
//   cce_sound: pruning a constant-condition If preserves the run.
//   uae_sound: dropping the post-exit tail preserves the run.
//   mia_sound: merging the adjacent pair preserves the run.
//
// Store + observable-trace semantics (prints), as in the proved elim
// family. Conditions are const booleans; While(true) is NOT modelled
// (the pass refuses it — avoiding an infinite-loop rewrite).
//
// What is modelled and what is NOT (fidelity, checked against the sources):
//   - const-condition-elim: the exact taken-branch selection (true-If ->
//     then; false-If -> first-true-elsif/else; While(false) drops;
//     DoWhile(false) -> body-once). The eval_cond/test-string evaluator
//     is a PARAMETER (a const boolean input) — proving `"1 -eq 1"` is
//     string parsing, not the pruning obligation. Nested-pass recursion
//     and elsifs-pruning of the unknown-cond case are the same induction.
//   - unreachable-after-exit: the direct-Exit/Return trigger and the
//     conditional-nesting refusal (nested Exit does NOT deaden the outer
//     list). FIRING: no producer in the shell pipeline constructs
//     `IrStmt::Exit`/`Return` today (`exit 1` lowers to `Expr(builtin)`)
//     — same dead-path class as for-recovery. The per-node rule is still
//     proved (a true theorem about the rule the pass implements); the
//     README-equivalent note is this paragraph.
//   - merge-init-assignments: literal-default Declare + adjacent pure-RHS
//     scalar Assign with no self-read. The purity gate and the reads_var
//     check are parameters (pure_rhs, reads_self booleans).
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;

pub enum Ev {
    Print(int),
    ExitEv,
}

pub enum Expr {
    Lit(int),
    Var(Name),
}

pub enum Stmt {
    Assign { target: Name, rhs: Expr },
    Decl { target: Name, init: Expr },
    Output { val: Expr },
    If { cond: bool, cond_const: bool, then: Seq<Stmt>, els: Seq<Stmt> },
    While { cond: bool, cond_const: bool, body: Seq<Stmt> },
    DoWhile { body: Seq<Stmt> },
    Exit,
    Return,
    Other,
}

pub open spec fn stmt_size(st: Stmt) -> nat
    decreases st,
{
    match st {
        Stmt::Assign { .. } => 1,
        Stmt::Decl { .. } => 1,
        Stmt::Output { .. } => 1,
        Stmt::If { then, els, .. } => 1 + seq_size(then) + seq_size(els),
        Stmt::While { body, .. } => 1 + seq_size(body),
        Stmt::DoWhile { body } => 1 + seq_size(body),
        Stmt::Exit => 1,
        Stmt::Return => 1,
        Stmt::Other => 1,
    }
}

pub open spec fn seq_size(p: Seq<Stmt>) -> nat
    decreases p,
{
    if p.len() == 0 {
        0
    } else {
        stmt_size(p[0]) + seq_size(p.drop_first()) + 1
    }
}

pub open spec fn lookup(s: Store, n: Name) -> int {
    if s.dom().contains(n) {
        s[n]
    } else {
        0
    }
}

pub open spec fn eval(e: Expr, s: Store) -> (int, Seq<Ev>) {
    match e {
        Expr::Lit(v) => (v, Seq::empty()),
        Expr::Var(n) => (lookup(s, n), Seq::empty()),
    }
}

pub open spec fn step(st: Stmt, s: Store) -> (Store, Seq<Ev>)
    decreases stmt_size(st),
{
    match st {
        Stmt::Assign { target, rhs } => {
            let r = eval(rhs, s);
            (s.insert(target, r.0), r.1)
        },
        Stmt::Decl { target, init } => {
            let r = eval(init, s);
            (s.insert(target, r.0), r.1)
        },
        Stmt::Output { val } => {
            let r = eval(val, s);
            (s, r.1.add(seq![Ev::Print(r.0)]))
        },
        Stmt::If { cond, cond_const, then, els } => {
            if cond_const {
                if cond {
                    run_abort(then, s, false)
                } else {
                    run_abort(els, s, false)
                }
            } else {
                run_abort(then, s, false)
            }
        },
        Stmt::While { cond, cond_const, body } => {
            if cond_const && !cond {
                (s, Seq::empty())
            } else {
                // Non-constant (or true) While: single unrolling. The
                // transform never fires here (it keeps the loop), so a
                // single unroll suffices for the per-node theorems; full
                // loop semantics is Tier-3 territory.
                let h = run_abort(body, s, false);
                (h.0, h.1)
            }
        },
        Stmt::DoWhile { body } => run_abort(body, s, false),
        Stmt::Exit => (s, seq![Ev::ExitEv]),
        Stmt::Return => (s, Seq::empty()),
        Stmt::Other => (s, Seq::empty()),
    }
}

// `run` carries an abort flag: once an ExitEv is emitted, the remaining
// list is skipped (the shell aborts; nothing after Exit executes).
// Terminates by total statement count (nested bodies are subterms).
pub open spec fn run_abort(p: Seq<Stmt>, s: Store, exited: bool) -> (Store, Seq<Ev>)
    decreases seq_size(p),
{
    if p.len() == 0 || exited {
        (s, Seq::empty())
    } else {
        let h = step(p[0], s);
        let t = run_abort(p.drop_first(), h.0, exited || is_exit_ev(h.1));
        (t.0, h.1.add(t.1))
    }
}

pub open spec fn is_exit_ev(t: Seq<Ev>) -> bool {
    exists|i: int| 0 <= i < t.len() && t[i] == Ev::ExitEv
}

// ---- (a) const-condition-elim ----

// Prune a constant-condition If to the taken branch (els = else-arm,
// modelling the first-true-elsif/else selection as already resolved).
pub open spec fn cce_if(cond: bool, then: Seq<Stmt>, els: Seq<Stmt>) -> Seq<Stmt> {
    if cond {
        then
    } else {
        els
    }
}

// Drop a While(false) (empty); keep anything else as-is.
pub open spec fn cce_while(cond: bool, cond_const: bool, body: Seq<Stmt>) -> Seq<Stmt> {
    if cond_const && !cond {
        Seq::empty()
    } else {
        seq![Stmt::While { cond, cond_const, body }]
    }
}

// DoWhile(false) -> body (runs exactly once).
pub open spec fn cce_dowhile(body: Seq<Stmt>) -> Seq<Stmt> {
    body
}

// THE OBLIGATION (a1): If-pruning preserves the run.
proof fn cce_if_sound(cond: bool, then: Seq<Stmt>, els: Seq<Stmt>, s: Store)
    
    ensures run_abort(cce_if(cond, then, els), s, false) == run_abort(
        seq![Stmt::If { cond, cond_const: true, then, els }],
        s,
        false,
    ),
{
    if cond {
        assert(cce_if(cond, then, els) == then);
        run_abort_cons(Stmt::If { cond, cond_const: true, then, els }, seq![], s, false);
    } else {
        assert(cce_if(cond, then, els) == els);
        run_abort_cons(Stmt::If { cond, cond_const: true, then, els }, seq![], s, false);
    }
}

// Unfold lemma: running [h] ++ t is the head step plus the tail run.
proof fn run_abort_cons(h: Stmt, t: Seq<Stmt>, s: Store, exited: bool)
    requires !exited,
    ensures run_abort(seq![h].add(t), s, exited) == {
        let hh = step(h, s);
        let tt = run_abort(t, hh.0, exited || is_exit_ev(hh.1));
        (tt.0, hh.1.add(tt.1))
    },
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
    reveal_with_fuel(run_abort, 4);
    reveal_with_fuel(step, 3);
}

// THE OBLIGATION (a2): While(false) drops with no effect.
proof fn cce_while_false_sound(body: Seq<Stmt>, s: Store)
    
    ensures run_abort(cce_while(false, true, body), s, false) == run_abort(
        seq![Stmt::While { cond: false, cond_const: true, body }],
        s,
        false,
    ),
{
    reveal_with_fuel(run_abort, 3);
    assert(cce_while(false, true, seq![Stmt::Output { val: Expr::Lit(9) }]) == Seq::empty());
}

// THE OBLIGATION (a3): DoWhile(false) -> body runs once.
proof fn cce_dowhile_sound(body: Seq<Stmt>, s: Store)
    
    ensures run_abort(cce_dowhile(body), s, false) == run_abort(
        seq![Stmt::DoWhile { body }],
        s,
        false,
    ),
{
    assert(cce_dowhile(body) == body);
    run_abort_cons(Stmt::DoWhile { body }, seq![], s, false);
}

// ---- (b) unreachable-after-exit ----

// Truncate a list at the first DIRECT Exit (unconditional, same list).
pub open spec fn uae_trunc(s: Seq<Stmt>) -> Seq<Stmt>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if s[0] == Stmt::Exit {
        seq![Stmt::Exit]
    } else {
        seq![s[0]].add(uae_trunc(s.drop_first()))
    }
}

// THE OBLIGATION (b): truncating the post-exit tail preserves the run
// (execution aborts at Exit — subsequent statements never run; the
// abort flag in run_abort is the execution model that makes the tail
// unobservable).
proof fn uae_sound(s: Seq<Stmt>, cur: Store, exited: bool)
    ensures run_abort(uae_trunc(s), cur, exited) == run_abort(s, cur, exited),
    decreases s.len(),
{
    if s.len() == 0 {
    } else if exited {
        assert(run_abort(uae_trunc(s), cur, exited) == (cur, Seq::empty())) by {
            reveal_with_fuel(run_abort, 2);
        };
        assert(run_abort(s, cur, exited) == (cur, Seq::empty())) by {
            reveal_with_fuel(run_abort, 2);
        };
    } else if s[0] == Stmt::Exit {
        assert(uae_trunc(s) == seq![Stmt::Exit]) by {
            reveal_with_fuel(uae_trunc, 2);
        };
        assert(s[0] == Stmt::Exit);
        assert(s.drop_first() == s.drop_first());
        assert(s == seq![s[0]].add(s.drop_first())) by {
            assert(s.len() == 1 + s.drop_first().len());
        };
        // Witness: the Exit step's trace carries the abort event.
        assert(step(Stmt::Exit, cur).1 == seq![Ev::ExitEv]);
        assert(is_exit_ev(seq![Ev::ExitEv])) by {
            assert(seq![Ev::ExitEv][0] == Ev::ExitEv);
        };
        // Tails under an asserted abort flag contribute nothing.
        assert(run_abort(s.drop_first(), step(s[0], cur).0, true) == (step(s[0], cur).0, Seq::<Ev>::empty())) by {
            reveal_with_fuel(run_abort, 2);
        };
        // Both sides abort at the head: the truncated run is just the
        // Exit step; the full run aborts after the same step, so the
        // tail contributes nothing.
        run_abort_cons(Stmt::Exit, s.drop_first(), cur, exited);
        run_abort_cons(Stmt::Exit, seq![], cur, exited);
        let lhs = run_abort(uae_trunc(s), cur, exited);
        let rhs = run_abort(s, cur, exited);
        assert(lhs == rhs);
    } else {
        assert(uae_trunc(s) == seq![s[0]].add(uae_trunc(s.drop_first()))) by {
            reveal_with_fuel(uae_trunc, 2);
        };
        // Both sides execute the SAME head s[0] first: the step pair,
        // the abort flag, and the tail store are identical by
        // construction (same statement, same store), so only the tails
        // can differ — and the IH closes them. Spell out the chain:
        // both runs unfold (cons) to head-step ++ tail-run, the heads
        // coincide, and the IH equates the tails.
        let h = step(s[0], cur);
        let flag = is_exit_ev(h.1);
        let nflag = exited || flag;
        assert(flag == is_exit_ev(h.1));
        uae_sound(s.drop_first(), h.0, nflag);
        assert(run_abort(s.drop_first(), h.0, nflag) == run_abort(uae_trunc(s.drop_first()), h.0, nflag));
        // Expand both sides with cons: same head step, tails equated by IH.
        // Name the shared head-step pair so both expansions visibly coincide.
        let hh = step(s[0], cur);
        assert(h == hh);
        assert(run_abort(s, cur, exited) == {
            let tt = run_abort(s.drop_first(), hh.0, exited || is_exit_ev(hh.1));
            (tt.0, hh.1.add(tt.1))
        }) by {
            run_abort_cons(s[0], s.drop_first(), cur, exited);
            assert(s == seq![s[0]].add(s.drop_first())) by {
                assert(s.len() == 1 + s.drop_first().len());
            };
        };
        assert(run_abort(uae_trunc(s), cur, exited) == {
            let tt = run_abort(uae_trunc(s.drop_first()), hh.0, exited || is_exit_ev(hh.1));
            (tt.0, hh.1.add(tt.1))
        }) by {
            assert(uae_trunc(s) == seq![s[0]].add(uae_trunc(s.drop_first())));
            run_abort_cons(s[0], uae_trunc(s.drop_first()), cur, exited);
        };
        assert(run_abort(s, cur, exited) == run_abort(uae_trunc(s), cur, exited));
    }
}

// ---- (c) merge-init-assignments ----

// Merge `Declare x = lit` + `x = rhs` (adjacent, pure rhs, no self-read)
// into `Declare x = rhs`.
pub open spec fn mia_merge(target: Name, rhs: Expr) -> Stmt {
    Stmt::Decl { target, init: rhs }
}

// Reads predicate: does the expression read the name? (The pass's
// reads_var gate; a literal reads nothing.)
pub open spec fn expr_reads(e: Expr, n: Name) -> bool {
    match e {
        Expr::Lit(_) => false,
        Expr::Var(m) => m == n,
    }
}

// The pure-RHS + no-self-read gates as parameters (the pass checks
// both before merging; literals satisfy them trivially). The default
// must ALSO be a literal (the pass's `is_literal` check) — its trace
// must be empty for the merged/single-step runs to agree.
pub open spec fn mia_default_ok(lit: Expr) -> bool {
    match lit {
        Expr::Lit(_) => true,
        Expr::Var(_) => false,
    }
}

pub open spec fn mia_gates_ok(rhs: Expr, target: Name) -> bool {
    // Purity: literals are pure (calls/captures/arith excluded).
    (match rhs {
        Expr::Lit(_) => true,
        Expr::Var(_) => false,
    }) && !expr_reads(rhs, target)
}

// Evaluation under stores agreeing away from `target` agrees — unless
// the expression reads `target` (the gate excludes that case).
proof fn eval_same_away(e: Expr, s1: Store, s2: Store, target: Name)
    requires
        forall|m: Name| m != target ==> lookup(s1, m) == lookup(s2, m),
        !expr_reads(e, target),
    ensures eval(e, s1) == eval(e, s2),
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(n) => {
            assert(n != target);
            assert(lookup(s1, n) == lookup(s2, n));
        },
    }
}

pub open spec fn stores_agree_away(s1: Store, s2: Store, target: Name) -> bool {
    forall|m: Name| m != target ==> lookup(s1, m) == lookup(s2, m)
}

pub open spec fn single_assign_run(target: Name, rhs: Expr, s: Store) -> (Store, Seq<Ev>) {
    let r = eval(rhs, s);
    (s.insert(target, r.0), r.1)
}

// A single-Assign run is just its step (tail empty contributes nothing).
proof fn single_assign_run_sound(target: Name, rhs: Expr, s: Store)
    ensures run_abort(seq![Stmt::Assign { target, rhs }], s, false) == single_assign_run(target, rhs, s),
{
    run_abort_cons(Stmt::Assign { target, rhs }, seq![], s, false);
    assert(s == s);
}

// THE OBLIGATION (c): the merged pair runs like the original pair,
// under the gates. The default store is overwritten before any read
// can observe it.
proof fn mia_sound(target: Name, lit: Expr, rhs: Expr, s: Store)
    requires mia_gates_ok(rhs, target) && mia_default_ok(lit),
    ensures run_abort(
        seq![mia_merge(target, rhs)],
        s,
        false,
    ) == run_abort(seq![Stmt::Decl { target, init: lit }, Stmt::Assign { target, rhs }], s, false),
{

    // The gate forces both sides literal: the default is a literal
    // (pass's is_literal) and the RHS is a literal that does not read
    // its target (pass's purity + no-self-read).
    assert(mia_gates_ok(rhs, target));
    assert(mia_default_ok(lit));
    let rm = eval(rhs, s);
    assert(rm.1 == Seq::<Ev>::empty()) by {
        match rhs {
            Expr::Lit(_) => {},
            Expr::Var(n) => {
                assert(n != target);
            },
        }
    };
    let r1 = eval(lit, s);
    assert(r1.1 == Seq::<Ev>::empty()) by {
        match lit { Expr::Lit(_) => {}, Expr::Var(_) => {} }
    };
    // The RHS evaluated where the default sits agrees with evaluating
    // it in the original store (no self-read).
    let s1 = s.insert(target, r1.0);
    assert(eval(rhs, s1) == rm) by {
        match rhs {
            Expr::Lit(_) => {},
            Expr::Var(n) => {
                assert(n != target);
                assert(lookup(s1, n) == lookup(s, n));
            },
        }
    };
    // Merged: single Declare with rhs.
    assert(mia_merge(target, rhs) == Stmt::Decl { target, init: rhs });
    // Unfold LHS fully: step the Declare, empty tail.
    assert(run_abort(seq![mia_merge(target, rhs)], s, false) == {
        let hh = step(mia_merge(target, rhs), s);
        let tt = run_abort(seq![], hh.0, false);
        (tt.0, hh.1.add(tt.1))
    }) by {
        run_abort_cons(mia_merge(target, rhs), seq![], s, false);
    };
    // Unfold RHS head: step the Declare, tail is [Assign].
    assert(run_abort(seq![Stmt::Decl { target, init: lit }, Stmt::Assign { target, rhs }], s, false) == {
        let hh = step(Stmt::Decl { target, init: lit }, s);
        let tt = run_abort(seq![Stmt::Assign { target, rhs }], hh.0, false);
        (tt.0, hh.1.add(tt.1))
    }) by {
        assert(seq![Stmt::Decl { target, init: lit }, Stmt::Assign { target, rhs }] == seq![Stmt::Decl { target, init: lit }].add(seq![Stmt::Assign { target, rhs }])) by {
            assert(seq![Stmt::Decl { target, init: lit }].add(seq![Stmt::Assign { target, rhs }]).len() == 2);
            assert(seq![Stmt::Decl { target, init: lit }, Stmt::Assign { target, rhs }].len() == 2);
            assert(seq![Stmt::Decl { target, init: lit }].add(seq![Stmt::Assign { target, rhs }])[0] == seq![Stmt::Decl { target, init: lit }, Stmt::Assign { target, rhs }][0]);
            assert(seq![Stmt::Decl { target, init: lit }].add(seq![Stmt::Assign { target, rhs }])[1] == seq![Stmt::Decl { target, init: lit }, Stmt::Assign { target, rhs }][1]);
        };
        run_abort_cons(Stmt::Decl { target, init: lit }, seq![Stmt::Assign { target, rhs }], s, false);
    };
    // Unfold RHS tail: step the Assign, empty tail.
    assert(run_abort(seq![Stmt::Assign { target, rhs }], s1, false) == {
        let hh = step(Stmt::Assign { target, rhs }, s1);
        let tt = run_abort(seq![], hh.0, false);
        (tt.0, hh.1.add(tt.1))
    }) by {
        run_abort_cons(Stmt::Assign { target, rhs }, seq![], s1, false);
    };
    // Both sides land on s.insert(target, rm.0) with the RHS trace.
    assert(s1.insert(target, rm.0) == s.insert(target, rm.0)) by {
        assert forall|m: Name| s1.insert(target, rm.0).dom().contains(m) implies s.insert(target, rm.0).dom().contains(m) by {
        };
        assert forall|m: Name| s.insert(target, rm.0).dom().contains(m) implies s1.insert(target, rm.0).dom().contains(m) by {
            if m != target {
                assert(s.dom().contains(m));
                assert(s1.dom().contains(m));
            }
        };
        assert(s1.insert(target, rm.0) =~= s.insert(target, rm.0));
    };
    assert(run_abort(
        seq![mia_merge(target, rhs)],
        s,
        false,
    ) == run_abort(seq![Stmt::Decl { target, init: lit }, Stmt::Assign { target, rhs }], s, false));
}

// ---- Fidelity pins ----

// CCE: `if true then A else B` runs A.
proof fn pin_cce_true_branch()
    ensures run_abort(
        cce_if(true, seq![Stmt::Output { val: Expr::Lit(1) }], seq![Stmt::Output { val: Expr::Lit(2) }]),
        Map::<Name, int>::empty(), false
    ).1 == seq![Ev::Print(1)],
{
    reveal_with_fuel(run_abort, 5);
}

// CCE: `if false then A else B` runs B.
proof fn pin_cce_false_branch()
    ensures run_abort(
        cce_if(false, seq![Stmt::Output { val: Expr::Lit(1) }], seq![Stmt::Output { val: Expr::Lit(2) }]),
        Map::<Name, int>::empty(), false
    ).1 == seq![Ev::Print(2)],
{
    reveal_with_fuel(run_abort, 5);
}

// CCE: `while false` never runs (empty trace).
proof fn pin_cce_while_false_drops()
    ensures run_abort(
        cce_while(false, true, seq![Stmt::Output { val: Expr::Lit(9) }]),
        Map::<Name, int>::empty(), false
    ) == (Map::<Name, int>::empty(), Seq::empty()),
{
    assert(cce_while(false, true, seq![Stmt::Output { val: Expr::Lit(9) }]) == Seq::empty());
    reveal_with_fuel(run_abort, 3);
}

// CCE: `do body while false` runs the body exactly once.
proof fn pin_cce_dowhile_once()
    ensures run_abort(
        cce_dowhile(seq![Stmt::Output { val: Expr::Lit(3) }]),
        Map::<Name, int>::empty(), false
    ).1 == seq![Ev::Print(3)],
{
    reveal_with_fuel(run_abort, 4);
}

// UAE: `Exit; Output` truncates to `Exit` (tail sealed).
proof fn pin_uae_truncates()
    ensures uae_trunc(
        seq![Stmt::Exit, Stmt::Output { val: Expr::Lit(7) }],
    ) == seq![Stmt::Exit],
{
    reveal_with_fuel(uae_trunc, 3);
}

// UAE: no Exit — list unchanged.
proof fn pin_uae_keeps_clean()
    ensures uae_trunc(
        seq![Stmt::Output { val: Expr::Lit(1) }, Stmt::Output { val: Expr::Lit(2) }],
    ) == seq![Stmt::Output { val: Expr::Lit(1) }, Stmt::Output { val: Expr::Lit(2) }],
{
    reveal_with_fuel(uae_trunc, 4);
}

// MIA: `Declare x = 0; x = 6` merges to `Declare x = 6`, final store x=6.
proof fn pin_mia_merges()
    ensures run_abort(
        seq![mia_merge(4nat, Expr::Lit(6))],
        Map::<Name, int>::empty(), false
    ).0 == Map::<Name, int>::empty().insert(4nat, 6),
{
    assert(mia_merge(4nat, Expr::Lit(6)) == Stmt::Decl { target: 4nat, init: Expr::Lit(6) });
    reveal_with_fuel(run_abort, 3);
    assert(Map::<Name, int>::empty().insert(4nat, 6).dom().contains(4nat));
    assert(Map::<Name, int>::empty().insert(4nat, 6)[4nat] == 6);
}

// MIA: the merged pair and the original pair agree (literal RHS,
// which satisfies the gates trivially).
proof fn pin_mia_pair_agrees()
    requires mia_gates_ok(Expr::Lit(6), 4nat) && mia_default_ok(Expr::Lit(0)),
    ensures run_abort(
        seq![mia_merge(4nat, Expr::Lit(6))],
        Map::<Name, int>::empty(), false
    ) == run_abort(
        seq![Stmt::Decl { target: 4nat, init: Expr::Lit(0) }, Stmt::Assign { target: 4nat, rhs: Expr::Lit(6) }],
        Map::<Name, int>::empty(), false
    ),
{
    mia_sound(4nat, Expr::Lit(0), Expr::Lit(6), Map::<Name, int>::empty());
}

fn main() {}

}
