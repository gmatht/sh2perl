// Statement-level model of test-lowering
// (sh2perl/src/transforms/test_lowering.rs): the "middle rung" between
// the per-leaf value theorem in `test_lowering.rs` and the whole
// rewrite. `test_lowering.rs` proves that each lowered leaf evaluates
// to the same boolean as the source test text; what was missing is that
// exchanging the runtime `test` call for that typed boolean, inside a
// branch or loop, preserves the RUN — the observable (final status,
// print trace).
//
// Proves the soundness obligation:
//
//   rewrite_sound: for a program whose test conditions sit in a dead-
//   status position, rewriting every such condition preserves the run
//   from any start status, at every fuel bound.
//
// The recipe is const_status_cond's: the bash $?/branch rules are
// stated as the execution semantics below, and the one thing the real
// guard supplies (the condition's status write is unobservable) is
// checked by the rewrite before it fires.
//
// What is modelled and what is NOT (fidelity, checked against
// test_lowering.rs's table + its 16 tests):
//   - The rewrite: a `test` leaf inside an If cond / While cond (and an
//     elsif cond, which the model carries as a nested If in the else
//     position — bash's `elif` semantics exactly) becomes a typed
//     boolean. The value theorem supplies `truth_low == truth_src`; the
//     condition carries that truth as a parameter here, so the
//     statement proof is independent of how the boolean was computed.
//   - The status/effect difference the whole rung is about: the runtime
//     test call is SILENT but WRITES status (0 iff true — bash's `test`
//     exit status); the lowered typed expression is silent and writes NO
//     status (the renderers branch on its value; the estree `contains`
//     arm in bare position records none). Everything else is identical:
//     neither prints, and a store is not modelled because no leaf arm
//     writes one (operands are plain reads or literals; the operand gate
//     refuses `$()` / `${…}` / `$?`).
//   - The guard: `cond_status_observable` refuses when a `$?` read can
//     observe the condition's status. The model's `clean_*` predicates
//     are the structural sufficient condition "no `$?` read anywhere in
//     the arms" (strictly stronger than the real scan, which admits a
//     read after an arm's own status writer). The theorem is therefore
//     about a SUBSET of the shipped firings — conservative in the safe
//     direction — and the guard is proved load-bearing by the
//     divergence pin below.
//   - Bash's branch/status rules as encoded by `exec_stmt`/`exec_while`:
//     an if runs the taken branch and reports its last status, and
//     reports 0 when nothing is taken; a loop reports its body's last
//     status if the body ever ran and 0 otherwise; a `$?` read observes
//     the incoming status and then writes 0. Fuel bounds loop
//     iterations exactly as const_status_cond does (the theorem holds at
//     every fuel, hence for the unbounded runs the fuel approximates).
//   - NOT modelled: statement-position `[[ … ]] && cmd` chains (a second
//     rewrite site: the chain's value IS its status, so it needs the
//     `is_pure_test_chain` shape rather than branch rules);
//     `nocasematch` (guard 3) and self-recursion (guard 2), which refuse
//     lifts and so only shrink the firing set; `DoWhile`/`ForInit` (the
//     same condition exchange with a test-at-the-tail / init-and-step
//     wrapper); nested `$?` reads inside conditions (no leaf arm
//     produces one).
use vstd::prelude::*;

verus! {

pub type Name = nat;

// ---- Language ----

pub enum Cmd {
    // Prints (interned) text and writes status 0.
    Run(Name),
    // Prints (interned) text and writes status 1 — a failing command.
    Fail(Name),
    // `$?`: emits the incoming status, then writes 0 (`echo "$?"`).
    ReadQ,
}

pub enum Cond {
    // The runtime `sh2.test("…")` call: silent, and writes status
    // (0 iff the test is true) — the status write the rewrite removes.
    Test(bool),
    // The lowered typed boolean (Ne / Not(Ne) / typed BinOp / a
    // primitive call): silent, writes NO status.
    Bool(bool),
    // Any condition the pass does not touch: silent, no status write.
    Other,
}

pub enum Stmt {
    While {
        cond: Cond,
        body: Seq<Stmt>,
    },
    // `if cond; then … else … fi`. An `elif` is a `Stmt::If` in the else
    // position: when the outer condition is false, bash evaluates the
    // elif condition and takes its branch, and reports 0 when no arm is
    // taken — exactly the nested `If`'s run.
    If {
        cond: Cond,
        then_b: Seq<Stmt>,
        els_b: Seq<Stmt>,
    },
    Run(Cmd),
}

pub enum Ev {
    Print(Name),
    Opq,
    Status(int),
}

// ---- Sizes (structural termination, the const_status_cond pattern) ----

pub open spec fn list_size(l: Seq<Stmt>) -> nat
    decreases l,
{
    if l.len() == 0 {
        0
    } else {
        1 + stmt_size(l[0]) + list_size(l.drop_first())
    }
}

pub open spec fn stmt_size(st: Stmt) -> nat
    decreases st,
{
    match st {
        Stmt::While { body, .. } => 1 + list_size(body),
        Stmt::If { then_b, els_b, .. } => 1 + list_size(then_b) + list_size(els_b),
        Stmt::Run(_) => 1,
    }
}

// ---- The body-shape premise ----

// Bash cannot spell an empty loop body or an empty `then` (`while c; do
// done` and `if c; then fi` are syntax errors), and an empty body is
// what would let a condition's status leak into the statement's status
// with nothing to overwrite it. The premise is what the real IR
// preserves, so the theorem takes it rather than assuming it away.
pub open spec fn wf_list(l: Seq<Stmt>) -> bool
    decreases l,
{
    if l.len() == 0 {
        true
    } else {
        wf_stmt(l[0]) && wf_list(l.drop_first())
    }
}

pub open spec fn wf_stmt(st: Stmt) -> bool
    decreases st,
{
    match st {
        Stmt::While { body, .. } => body.len() > 0 && wf_list(body),
        Stmt::If { then_b, els_b, .. } => then_b.len() > 0 && wf_list(then_b) && wf_list(els_b),
        Stmt::Run(_) => true,
    }
}

// ---- The guard (a structural over-approximation of `lastexit_scan`) ----

// No `$?` read anywhere in the arms: the condition's status write can
// never be observed. (The real scan additionally admits a read after an
// arm's own writer; modelling the stronger rule can only shrink the
// firing set.)
pub open spec fn clean_cmd(c: Cmd) -> bool {
    match c {
        Cmd::ReadQ => false,
        _ => true,
    }
}

pub open spec fn clean_list(l: Seq<Stmt>) -> bool
    decreases l,
{
    if l.len() == 0 {
        true
    } else {
        clean_stmt(l[0]) && clean_list(l.drop_first())
    }
}

pub open spec fn clean_stmt(st: Stmt) -> bool
    decreases st,
{
    match st {
        Stmt::While { body, .. } => clean_list(body),
        Stmt::If { then_b, els_b, .. } => clean_list(then_b) && clean_list(els_b),
        Stmt::Run(c) => clean_cmd(c),
    }
}

// ---- The rewrite (mirrors rewrite_cond / rewrite_test_chain) ----

pub open spec fn rewrite_cond(c: Cond) -> Cond {
    match c {
        Cond::Test(t) => Cond::Bool(t),
        _ => c,
    }
}

pub open spec fn rewrite_list(l: Seq<Stmt>) -> Seq<Stmt>
    decreases l,
{
    if l.len() == 0 {
        Seq::empty()
    } else {
        seq![rewrite_stmt(l[0])].add(rewrite_list(l.drop_first()))
    }
}

pub open spec fn rewrite_stmt(st: Stmt) -> Stmt
    decreases st,
{
    match st {
        Stmt::While { cond, body } => {
            if clean_list(body) {
                Stmt::While { cond: rewrite_cond(cond), body: rewrite_list(body) }
            } else {
                // the guard refuses this condition; the body is still
                // recursed (the pass does the same)
                Stmt::While { cond, body: rewrite_list(body) }
            }
        },
        Stmt::If { cond, then_b, els_b } => {
            let c2 = if clean_list(then_b) && clean_list(els_b) {
                rewrite_cond(cond)
            } else {
                cond
            };
            Stmt::If { cond: c2, then_b: rewrite_list(then_b), els_b: rewrite_list(els_b) }
        },
        Stmt::Run(c) => Stmt::Run(c),
    }
}

// ---- Execution (fuel-bounded, bash exit-status rules) ----

pub open spec fn exec_cmd(c: Cmd, s: int) -> (int, Seq<Ev>) {
    match c {
        Cmd::Run(t) => (0, seq![Ev::Print(t)]),
        Cmd::Fail(t) => (1, seq![Ev::Print(t)]),
        Cmd::ReadQ => (0, seq![Ev::Status(s)]),
    }
}

// A condition is silent. The runtime test call writes status; the
// lowered boolean (and an untouched condition) leaves it alone.
pub open spec fn exec_cond(c: Cond, s: int) -> (int, Seq<Ev>) {
    match c {
        Cond::Test(t) => (if t {
            0
        } else {
            1
        }, Seq::empty()),
        Cond::Bool(_) => (s, Seq::empty()),
        Cond::Other => (s, Seq::empty()),
    }
}

// The branch/loop decision. `Test(t)` and its lower `Bool(t)` decide
// alike — the value theorem's conclusion, carried as the parameter.
pub open spec fn decide(c: Cond) -> bool {
    match c {
        Cond::Test(t) => t,
        Cond::Bool(t) => t,
        Cond::Other => false,
    }
}

pub open spec fn exec_list(l: Seq<Stmt>, s: int, f: nat) -> (int, Seq<Ev>)
    decreases f, l,
{
    if l.len() == 0 {
        (s, Seq::empty())
    } else {
        let h = exec_stmt(l[0], s, f);
        let t = exec_list(l.drop_first(), h.0, f);
        (t.0, h.1.add(t.1))
    }
}

// `blast`/`ran` thread the body's last status: a loop that never ran
// reports 0, and fuel exhaustion reports the same convention on both
// sides (equality needs agreement, not a particular value).
pub open spec fn exec_while(
    c: Cond,
    b: Seq<Stmt>,
    s: int,
    blast: int,
    ran: bool,
    f: nat,
) -> (int, Seq<Ev>)
    decreases f, b,
{
    if f == 0 {
        ((if ran {
            blast
        } else {
            0
        }), Seq::empty())
    } else {
        let ce = exec_cond(c, s);
        if decide(c) {
            let be = exec_list(b, ce.0, (f - 1) as nat);
            let re = exec_while(c, b, be.0, be.0, true, (f - 1) as nat);
            (re.0, ce.1.add(be.1).add(re.1))
        } else {
            ((if ran {
                blast
            } else {
                0
            }), ce.1)
        }
    }
}

pub open spec fn exec_stmt(st: Stmt, s: int, f: nat) -> (int, Seq<Ev>)
    decreases f, st,
{
    match st {
        Stmt::While { cond, body } => exec_while(cond, body, s, s, false, f),
        // bash: the taken branch's last status is the statement's, and
        // no branch taken reports 0 — not the condition's status.
        Stmt::If { cond, then_b, els_b } => {
            let ce = exec_cond(cond, s);
            if decide(cond) {
                let be = exec_list(then_b, ce.0, f);
                (be.0, ce.1.add(be.1))
            } else if els_b.len() == 0 {
                (0, ce.1)
            } else {
                let ee = exec_list(els_b, ce.0, f);
                (ee.0, ce.1.add(ee.1))
            }
        },
        Stmt::Run(c) => exec_cmd(c, s),
    }
}

// ---- Unfold helpers ----

proof fn list_cons(h: Stmt, t: Seq<Stmt>, s: int, f: nat)
    ensures exec_list(seq![h].add(t), s, f) == {
        let hh = exec_stmt(h, s, f);
        let tt = exec_list(t, hh.0, f);
        (tt.0, hh.1.add(tt.1))
    },
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

proof fn rewrite_list_cons(h: Stmt, t: Seq<Stmt>)
    ensures rewrite_list(seq![h].add(t)) == seq![rewrite_stmt(h)].add(rewrite_list(t)),
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

proof fn unfold_if(c: Cond, then_b: Seq<Stmt>, els_b: Seq<Stmt>, s: int, f: nat)
    ensures exec_stmt(Stmt::If { cond: c, then_b, els_b }, s, f) == {
        let ce = exec_cond(c, s);
        if decide(c) {
            let be = exec_list(then_b, ce.0, f);
            (be.0, ce.1.add(be.1))
        } else if els_b.len() == 0 {
            (0, ce.1)
        } else {
            let ee = exec_list(els_b, ce.0, f);
            (ee.0, ce.1.add(ee.1))
        }
    },
{
}

proof fn unfold_while_full(c: Cond, b: Seq<Stmt>, s: int, bl: int, rn: bool, f: nat)
    ensures exec_while(c, b, s, bl, rn, f) == (if f == 0 {
        ((if rn {
            bl
        } else {
            0
        }), Seq::empty())
    } else {
        let ce = exec_cond(c, s);
        if decide(c) {
            let be = exec_list(b, ce.0, (f - 1) as nat);
            let re = exec_while(c, b, be.0, be.0, true, (f - 1) as nat);
            (re.0, ce.1.add(be.1).add(re.1))
        } else {
            ((if rn {
                bl
            } else {
                0
            }), ce.1)
        }
    }),
{
}

// ---- The guard's meaning: a clean, non-empty list is status-blind ----

// Every command writes status except the excluded `$?` read, so a
// non-empty clean list's run does not depend on the status it starts
// from. This is exactly what the guard buys, and it is the only premise
// the statement proof needs beyond the condition values.
proof fn clean_list_blind(l: Seq<Stmt>, s1: int, s2: int, f: nat)
    requires
        clean_list(l),
        wf_list(l),
        l.len() > 0,
    ensures exec_list(l, s1, f) == exec_list(l, s2, f),
    decreases f, list_size(l),
{
    let h = l[0];
    let t = l.drop_first();
    list_cons(h, t, s1, f);
    list_cons(h, t, s2, f);
    clean_stmt_blind(h, s1, s2, f);
    assert(exec_stmt(h, s1, f) == exec_stmt(h, s2, f));
    if t.len() > 0 {
        clean_list_blind(t, exec_stmt(h, s1, f).0, exec_stmt(h, s2, f).0, f);
    }
}

proof fn clean_stmt_blind(st: Stmt, s1: int, s2: int, f: nat)
    requires clean_stmt(st), wf_stmt(st),
    ensures exec_stmt(st, s1, f) == exec_stmt(st, s2, f),
    decreases f, stmt_size(st),
{
    match st {
        Stmt::Run(c) => {
            match c {
                // a `$?` read is exactly what clean_cmd excludes
                Cmd::ReadQ => {
                    assert(clean_cmd(c));
                    assert(false);
                },
                _ => {},
            }
        },
        Stmt::If { cond, then_b, els_b } => {
            unfold_if(cond, then_b, els_b, s1, f);
            unfold_if(cond, then_b, els_b, s2, f);
            // both conditions are silent; the decision is the same
            reveal_with_fuel(wf_stmt, 2);
            assert(then_b.len() > 0);
            assert(exec_cond(cond, s1).1 == exec_cond(cond, s2).1);
            if decide(cond) {
                clean_list_blind(then_b, exec_cond(cond, s1).0, exec_cond(cond, s2).0, f);
            } else if els_b.len() == 0 {
                // nothing runs on either side, and both report 0
            } else {
                clean_list_blind(els_b, exec_cond(cond, s1).0, exec_cond(cond, s2).0, f);
            }
        },
        Stmt::While { cond, body } => {
            reveal_with_fuel(wf_stmt, 2);
            assert(body.len() > 0);
            clean_while_blind(cond, body, s1, s2, 0, 0, false, f);
        },
    }
}

proof fn clean_while_blind(
    c: Cond,
    b: Seq<Stmt>,
    s1: int,
    s2: int,
    bl1: int,
    bl2: int,
    rn: bool,
    f: nat,
)
    requires
        clean_list(b),
        wf_list(b),
        b.len() > 0,
        bl1 == bl2,
    ensures exec_while(c, b, s1, bl1, rn, f) == exec_while(c, b, s2, bl2, rn, f),
    decreases f, list_size(b),
{
    unfold_while_full(c, b, s1, bl1, rn, f);
    unfold_while_full(c, b, s2, bl2, rn, f);
    if f == 0 {
    } else {
        assert(exec_cond(c, s1).1 == exec_cond(c, s2).1);
        if decide(c) {
            clean_list_blind(b, exec_cond(c, s1).0, exec_cond(c, s2).0, (f - 1) as nat);
            let be1 = exec_list(b, exec_cond(c, s1).0, (f - 1) as nat);
            let be2 = exec_list(b, exec_cond(c, s2).0, (f - 1) as nat);
            assert(be1 == be2);
            clean_while_blind(c, b, be1.0, be1.0, be1.0, be1.0, true, (f - 1) as nat);
        }
    }
}

// ---- Soundness: the condition exchange preserves the run ----

// Loop soundness: `while [ test ]` → `while <typed bool>` agrees, at
// every entry, including the recursion back through the body.
proof fn while_sound_gen(
    c: Cond,
    b: Seq<Stmt>,
    s: int,
    bl: int,
    rn: bool,
    f: nat,
)
    requires clean_list(b), wf_list(b), b.len() > 0,
    ensures exec_while(rewrite_cond(c), rewrite_list(b), s, bl, rn, f) == exec_while(
        c,
        b,
        s,
        bl,
        rn,
        f,
    ),
    decreases f, list_size(b),
{
    unfold_while_full(rewrite_cond(c), rewrite_list(b), s, bl, rn, f);
    unfold_while_full(c, b, s, bl, rn, f);
    if f == 0 {
    } else {
        assert(exec_cond(rewrite_cond(c), s).1 == exec_cond(c, s).1);
        assert(decide(rewrite_cond(c)) == decide(c));
        if decide(c) {
            let ce = exec_cond(c, s);
            exec_list_sound(b, s, (f - 1) as nat);
            clean_list_blind(b, s, ce.0, (f - 1) as nat);
            let be = exec_list(b, ce.0, (f - 1) as nat);
            assert(exec_list(rewrite_list(b), s, (f - 1) as nat) == be);
            while_sound_gen(c, b, be.0, be.0, true, (f - 1) as nat);
        }
    }
}

proof fn exec_stmt_sound(st: Stmt, s: int, f: nat)
    requires clean_stmt(st), wf_stmt(st),
    ensures exec_stmt(rewrite_stmt(st), s, f) == exec_stmt(st, s, f),
    decreases f, stmt_size(st),
{
    match st {
        Stmt::While { cond, body } => {
            assert(clean_list(body));
            reveal_with_fuel(rewrite_stmt, 2);
            assert(rewrite_stmt(st) == Stmt::While {
                cond: rewrite_cond(cond),
                body: rewrite_list(body),
            });
            while_sound_gen(cond, body, s, s, false, f);
        },
        // Branch soundness: Test(t) becomes Bool(t) (the value theorem),
        // and the arms are clean (the guard), so the taken branch
        // produces the same status and trace from either entry status.
        Stmt::If { cond, then_b, els_b } => {
            assert(clean_list(then_b) && clean_list(els_b));
            reveal_with_fuel(rewrite_stmt, 2);
            assert(rewrite_stmt(st) == Stmt::If {
                cond: rewrite_cond(cond),
                then_b: rewrite_list(then_b),
                els_b: rewrite_list(els_b),
            });
            unfold_if(rewrite_cond(cond), rewrite_list(then_b), rewrite_list(els_b), s, f);
            unfold_if(cond, then_b, els_b, s, f);
            // the lowered condition is silent and decides alike
            assert(exec_cond(rewrite_cond(cond), s).1 == exec_cond(cond, s).1);
            assert(decide(rewrite_cond(cond)) == decide(cond));
            let ce = exec_cond(cond, s);
            if decide(cond) {
                exec_list_sound(then_b, s, f);
                // lowered body from s == original body from the test status
                clean_list_blind(then_b, s, ce.0, f);
                assert(exec_list(rewrite_list(then_b), s, f) == exec_list(then_b, ce.0, f));
            } else if els_b.len() == 0 {
                // both sides report 0 and print nothing
            } else {
                exec_list_sound(els_b, s, f);
                clean_list_blind(els_b, s, ce.0, f);
                assert(exec_list(rewrite_list(els_b), s, f) == exec_list(els_b, ce.0, f));
            }
        },
        Stmt::Run(c) => {
            assert(rewrite_stmt(st) == st);
        },
    }
}

proof fn exec_list_sound(l: Seq<Stmt>, s: int, f: nat)
    requires clean_list(l), wf_list(l),
    ensures exec_list(rewrite_list(l), s, f) == exec_list(l, s, f),
    decreases f, list_size(l),
{
    if l.len() == 0 {
    } else {
        let h = l[0];
        let t = l.drop_first();
        rewrite_list_cons(h, t);
        exec_stmt_sound(h, s, f);
        let h1 = exec_stmt(rewrite_stmt(h), s, f);
        let h2 = exec_stmt(h, s, f);
        assert(h1 == h2);
        exec_list_sound(t, h1.0, f);
        list_cons(rewrite_stmt(h), rewrite_list(t), s, f);
        list_cons(h, t, s, f);
    }
}

// THE TOP-LEVEL OBLIGATION: a program whose test conditions are all in
// a dead-status position runs identically after the rewrite.
proof fn rewrite_sound(p: Seq<Stmt>, s: int, f: nat)
    requires clean_list(p), wf_list(p),
    ensures exec_list(rewrite_list(p), s, f) == exec_list(p, s, f),
{
    exec_list_sound(p, s, f);
}

// ---- Fidelity pins (mirror test_lowering.rs's table + tests) ----

pub open spec fn true_if() -> Stmt {
    Stmt::If { cond: Cond::Test(true), then_b: seq![Stmt::Run(Cmd::Run(1))], els_b: Seq::empty() }
}

pub open spec fn false_if() -> Stmt {
    Stmt::If { cond: Cond::Test(false), then_b: seq![Stmt::Run(Cmd::Run(1))], els_b: Seq::empty() }
}

pub open spec fn readq_if() -> Stmt {
    Stmt::If {
        cond: Cond::Test(true),
        then_b: seq![Stmt::Run(Cmd::ReadQ)],
        els_b: Seq::empty(),
    }
}

// A dead-status test condition is lowered (the rule).
proof fn pin_dead_status_lowers_the_test()
    ensures rewrite_stmt(true_if()) == (Stmt::If {
        cond: Cond::Bool(true),
        then_b: seq![Stmt::Run(Cmd::Run(1))],
        els_b: Seq::empty(),
    }),
{
    assert(clean_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// A body that reads `$?` keeps the runtime test (guard 1), so the
// divergence below is unreachable through the pass.
proof fn pin_live_status_keeps_the_test()
    ensures rewrite_stmt(readq_if()) == (Stmt::If {
        cond: Cond::Test(true),
        then_b: seq![Stmt::Run(Cmd::ReadQ)],
        els_b: Seq::empty(),
    }),
{
    assert(!clean_list(seq![Stmt::Run(Cmd::ReadQ)])) by {
        reveal_with_fuel(clean_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// The condition swap is value-preserving by construction: Test(t) and
// its lower decide alike, and both are silent.
proof fn pin_test_cond_rewrites_to_the_same_value()
    ensures
        rewrite_cond(Cond::Test(true)) == Cond::Bool(true),
        rewrite_cond(Cond::Test(false)) == Cond::Bool(false),
        decide(rewrite_cond(Cond::Test(true))) == decide(Cond::Test(true)),
        exec_cond(rewrite_cond(Cond::Test(true)), 7).1 == exec_cond(Cond::Test(true), 7).1,
{
}

// A condition the pass does not touch is untouched (refuse > guess).
proof fn pin_other_cond_untouched()
    ensures
        rewrite_cond(Cond::Other) == Cond::Other,
        rewrite_stmt(Stmt::While {
            cond: Cond::Other,
            body: seq![Stmt::Run(Cmd::Run(1))],
        }) == (Stmt::While { cond: Cond::Other, body: seq![Stmt::Run(Cmd::Run(1))] }),
{
    assert(clean_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// bash: a taken branch reports its body's last status, not the
// condition's.
proof fn pin_branch_status_is_the_body_status()
    ensures exec_stmt(Stmt::If {
        cond: Cond::Test(true),
        then_b: seq![Stmt::Run(Cmd::Run(1)), Stmt::Run(Cmd::Fail(2))],
        els_b: Seq::empty(),
    }, 9, 3) == (1, seq![Ev::Print(1), Ev::Print(2)]),
{
    reveal_with_fuel(exec_stmt, 8);
    reveal_with_fuel(exec_list, 8);
}

// bash: an if with no arm taken reports 0, and prints nothing.
proof fn pin_no_branch_taken_reports_zero()
    ensures exec_stmt(false_if(), 9, 3) == (0, Seq::empty()),
{
    reveal_with_fuel(exec_stmt, 8);
}

// bash: a loop that never ran reports 0.
proof fn pin_while_never_runs_reports_zero()
    ensures exec_stmt(Stmt::While {
        cond: Cond::Test(false),
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 9, 4) == (0, Seq::empty()),
{
    reveal_with_fuel(exec_stmt, 8);
    reveal_with_fuel(exec_while, 8);
}

// The loop rewrite agrees on a concrete program: `while false` runs zero
// times on both faces and reports 0.
proof fn pin_while_lowered_agrees()
    ensures exec_stmt(Stmt::While {
        cond: Cond::Test(false),
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 0, 4) == exec_stmt(Stmt::While {
        cond: Cond::Bool(false),
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 0, 4),
{
    assert(clean_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(wf_list, 4);
    };
    while_sound_gen(Cond::Test(false), seq![Stmt::Run(Cmd::Run(1))], 0, 0, false, 4);
}

// THE GUARD IS LOAD-BEARING. Without it the rewrite is unsound: a body
// that prints `$?` sees the test call's status (0) on the source face
// and the entry status (1) on the lowered face. This is the negative
// control for `pin_live_status_keeps_the_test`.
proof fn pin_guard_is_load_bearing()
    ensures exec_stmt(readq_if(), 1, 3) != exec_stmt(Stmt::If {
        cond: Cond::Bool(true),
        then_b: seq![Stmt::Run(Cmd::ReadQ)],
        els_b: Seq::empty(),
    }, 1, 3),
{
    let src = exec_stmt(readq_if(), 1, 3);
    let low = exec_stmt(Stmt::If {
        cond: Cond::Bool(true),
        then_b: seq![Stmt::Run(Cmd::ReadQ)],
        els_b: Seq::empty(),
    }, 1, 3);
    assert(src == (0, seq![Ev::Status(0)])) by {
        reveal_with_fuel(exec_stmt, 16);
        reveal_with_fuel(exec_list, 16);
    };
    assert(low == (0, seq![Ev::Status(1)])) by {
        reveal_with_fuel(exec_stmt, 16);
        reveal_with_fuel(exec_list, 16);
    };
    assert(src.1[0] == Ev::Status(0));
    assert(low.1[0] == Ev::Status(1));
    assert(src.1[0] != low.1[0]);
    // Seq inequality needs the witness index, not just distinct lengths.
    if src == low {
        assert(src.1 == low.1);
        assert(src.1[0] == low.1[0]);
        assert(false);
    }
}

// Non-vacuity: the blindness premise the proof consumes has instances —
// a clean, non-empty command list runs the same from any status.
proof fn pin_clean_run_list_is_blind()
    ensures exec_list(seq![Stmt::Run(Cmd::Run(1)), Stmt::Run(Cmd::Fail(2))], 0, 3) == exec_list(
        seq![Stmt::Run(Cmd::Run(1)), Stmt::Run(Cmd::Fail(2))],
        7,
        3,
    ),
{
    assert(clean_list(seq![Stmt::Run(Cmd::Run(1)), Stmt::Run(Cmd::Fail(2))])) by {
        reveal_with_fuel(clean_list, 6);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Run(1)), Stmt::Run(Cmd::Fail(2))])) by {
        reveal_with_fuel(wf_list, 6);
    };
    clean_list_blind(seq![Stmt::Run(Cmd::Run(1)), Stmt::Run(Cmd::Fail(2))], 0, 7, 3);
}

// The else arm has no condition to rewrite and always runs.
proof fn pin_else_arm_is_unconditional()
    ensures exec_stmt(Stmt::If {
        cond: Cond::Test(false),
        then_b: seq![Stmt::Run(Cmd::Run(1))],
        els_b: seq![Stmt::Run(Cmd::Fail(2))],
    }, 0, 3) == (1, seq![Ev::Print(2)]),
{
    reveal_with_fuel(exec_stmt, 8);
    reveal_with_fuel(exec_list, 8);
}

// An `elif` is the nested If in the else position: false outer, true
// inner runs the second arm on both faces and agrees.
proof fn pin_elif_chain_agrees()
    ensures exec_stmt(Stmt::If {
        cond: Cond::Test(false),
        then_b: seq![Stmt::Run(Cmd::Run(1))],
        els_b: seq![Stmt::If {
            cond: Cond::Test(true),
            then_b: seq![Stmt::Run(Cmd::Fail(2))],
            els_b: Seq::empty(),
        }],
    }, 5, 3) == (1, seq![Ev::Print(2)]),
{
    reveal_with_fuel(exec_stmt, 10);
    reveal_with_fuel(exec_list, 10);
}

fn main() {}

}
