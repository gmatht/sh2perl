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
//   - The other THREE rewrite sites are modelled too, each with its own
//     bash status rule stated below:
//       * `DoWhile` (`do { body } while (test)`) — the body runs once
//         before the first test; the loop reports the last BODY status,
//         so the test's write is never the loop's;
//       * `ForInit` (`for (; test ;) { body; step }`) — the loop reports
//         the last BODY status (bash's for/while rule), not the step's
//         and not the test's; the step runs between iterations and the
//         next test sees the step's status;
//       * statement-position `[[ … ]] && cmd` / `|| cmd` — the chain's
//         value IS its 0/1 status on BOTH faces, in all three renderers,
//         each read from its emitter and each held by a gate guard in
//         `harness/verus-sync-coverage.sh` (so a change to any emitter
//         fails the gate):
//           perl: `control_chain_to_perl` seeds
//             `$main_exit_code = $CHILD_ERROR = $__sh_ok ? 0 : 1`;
//           C: the statement-position And/Or arm (`c_backend.rs`, the
//             `chain_lhs` block) emits `{ if ((l)) { _sh_rc = 0; … } else
//             { _sh_rc = 1; } }` (Or swaps which arm runs the rhs);
//           estree: every native test records
//             `(sh2._g = t, sh2.lastExit = sh2._g ? 0 : 1, sh2._g)`
//             (`shir::native_test_statused`), and the `&&`/`||` links
//             branch on `lastExit` and only re-record the same value
//             (`is_status_record_seq`), so the short-circuit keeps the
//             lead's 0/1.
//         The only difference a `$?` reader could see is INSIDE the rhs
//         (immediately after the leaf), which the transform's
//         `ir_expr_reads_status` guard excludes and `clean_list(rhs)`
//         encodes here.
//   - NOT modelled: `nocasematch` (guard 3) and self-recursion (guard 2),
//     which refuse lifts and so only shrink the firing set; `For`/`ForIn`
//     (they carry no test condition to exchange); `until` negation (an
//     orthogonal flip on both faces); other statement kinds the pass
//     only recurses into without rewriting a condition of its own; nested
//     `$?` reads inside conditions (no leaf arm produces one).
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

pub enum ChainOp {
    And,
    Or,
}

pub enum Stmt {
    While {
        cond: Cond,
        body: Seq<Stmt>,
    },
    // `do { body } while (cond)` / `until (cond)`: the body runs once
    // before the first test. The loop reports the last body status (the
    // `while` rule the IR's own loop form uses), so the test's status
    // write is never the loop's.
    DoWhile {
        cond: Cond,
        body: Seq<Stmt>,
    },
    // `for (; cond ;) { body; step }` with the init list before it. The
    // loop reports the last BODY status (bash's `for` rule), not the
    // step's and not the test's.
    ForInit {
        init: Seq<Stmt>,
        cond: Cond,
        step: Seq<Stmt>,
        body: Seq<Stmt>,
    },
    // A statement-position `[[ … ]] && cmd` / `|| cmd` chain: the
    // chain's value IS its 0/1 status (the renderers seed it from the
    // value), and a short-circuit reports 1 (And) / 0 (Or).
    Chain {
        op: ChainOp,
        cond: Cond,
        rhs: Seq<Stmt>,
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
        Stmt::DoWhile { body, .. } => 1 + list_size(body),
        Stmt::ForInit { init, step, body, .. } => 1 + list_size(init) + list_size(step) + list_size(body),
        Stmt::If { then_b, els_b, .. } => 1 + list_size(then_b) + list_size(els_b),
        Stmt::Chain { rhs, .. } => 1 + list_size(rhs),
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
        Stmt::DoWhile { body, .. } => body.len() > 0 && wf_list(body),
        Stmt::ForInit { init, step, body, .. } => body.len() > 0 && wf_list(init) && wf_list(step) && wf_list(body),
        Stmt::If { then_b, els_b, .. } => then_b.len() > 0 && wf_list(then_b) && wf_list(els_b),
        Stmt::Chain { rhs, .. } => rhs.len() > 0 && wf_list(rhs),
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
        Stmt::DoWhile { body, .. } => clean_list(body),
        Stmt::ForInit { init, step, body, .. } => clean_list(init) && clean_list(step) && clean_list(body),
        Stmt::If { then_b, els_b, .. } => clean_list(then_b) && clean_list(els_b),
        // The chain guard: nothing in the chain may READ `$?` (the rhs
        // runs after the leaf, so a read there would observe the test
        // call's status write directly). The short-circuit status is the
        // chain's value on both faces, which is why no further premise is
        // needed here.
        Stmt::Chain { rhs, .. } => clean_list(rhs),
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
        Stmt::DoWhile { cond, body } => {
            if clean_list(body) {
                Stmt::DoWhile { cond: rewrite_cond(cond), body: rewrite_list(body) }
            } else {
                Stmt::DoWhile { cond, body: rewrite_list(body) }
            }
        },
        Stmt::ForInit { init, cond, step, body } => {
            if clean_list(init) && clean_list(step) && clean_list(body) {
                Stmt::ForInit {
                    init: rewrite_list(init),
                    cond: rewrite_cond(cond),
                    step: rewrite_list(step),
                    body: rewrite_list(body),
                }
            } else {
                Stmt::ForInit {
                    init: rewrite_list(init),
                    cond,
                    step: rewrite_list(step),
                    body: rewrite_list(body),
                }
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
        Stmt::Chain { op, cond, rhs } => {
            if clean_list(rhs) {
                Stmt::Chain { op, cond: rewrite_cond(cond), rhs: rewrite_list(rhs) }
            } else {
                Stmt::Chain { op, cond, rhs: rewrite_list(rhs) }
            }
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

// `do { … } while (cond)`: the body runs once before the first test;
// the loop reports the last body status (never the test's).
pub open spec fn exec_dowhile(
    c: Cond,
    b: Seq<Stmt>,
    s: int,
    f: nat,
) -> (int, Seq<Ev>)
    decreases f, b,
{
    if f == 0 {
        (0, Seq::empty())
    } else {
        let be = exec_list(b, s, (f - 1) as nat);
        let ce = exec_cond(c, be.0);
        if decide(c) {
            let re = exec_dowhile(c, b, ce.0, (f - 1) as nat);
            (re.0, be.1.add(ce.1).add(re.1))
        } else {
            (be.0, be.1.add(ce.1))
        }
    }
}

// `for (; cond ;) { body; step }`: `blast` is the last BODY status (the
// reported value), `s` is what the next test sees (after the step).
pub open spec fn exec_fori(
    c: Cond,
    b: Seq<Stmt>,
    stp: Seq<Stmt>,
    s: int,
    blast: int,
    ran: bool,
    f: nat,
) -> (int, Seq<Ev>)
    decreases f, b, stp,
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
            let se = exec_list(stp, be.0, (f - 1) as nat);
            let re = exec_fori(c, b, stp, se.0, be.0, true, (f - 1) as nat);
            (re.0, ce.1.add(be.1).add(se.1).add(re.1))
        } else {
            ((if ran {
                blast
            } else {
                0
            }), ce.1)
        }
    }
}

pub open spec fn exec_forinit(
    init: Seq<Stmt>,
    c: Cond,
    stp: Seq<Stmt>,
    b: Seq<Stmt>,
    s: int,
    f: nat,
) -> (int, Seq<Ev>)
    decreases f, init, b, stp,
{
    if f == 0 {
        (0, Seq::empty())
    } else {
        let ie = exec_list(init, s, (f - 1) as nat);
        // blast is the last BODY status; before any body ran it is unused
        // (`ran` is false), so 0 keeps the loop blind to the init status.
        let re = exec_fori(c, b, stp, ie.0, 0, false, (f - 1) as nat);
        (re.0, ie.1.add(re.1))
    }
}

// A statement-position `&&`/`||` chain. The renderers seed the status
// from the chain VALUE (`$main_exit_code = $CHILD_ERROR = ok ? 0 : 1`),
// so a short-circuit reports 1 (And) / 0 (Or) on BOTH faces — the
// lowered leaf keeps that status write. The only observable difference
// would be a `$?` read in the rhs (immediately after the leaf), which
// `clean_list(rhs)` excludes.
pub open spec fn exec_chain(
    op: ChainOp,
    c: Cond,
    rhs: Seq<Stmt>,
    s: int,
    f: nat,
) -> (int, Seq<Ev>)
    decreases f, rhs,
{
    if f == 0 {
        (0, Seq::empty())
    } else {
        let ce = exec_cond(c, s);
        match op {
            ChainOp::And => {
                if decide(c) {
                    let re = exec_list(rhs, ce.0, (f - 1) as nat);
                    (re.0, ce.1.add(re.1))
                } else {
                    (1, ce.1)
                }
            },
            ChainOp::Or => {
                if decide(c) {
                    (0, ce.1)
                } else {
                    let re = exec_list(rhs, ce.0, (f - 1) as nat);
                    (re.0, ce.1.add(re.1))
                }
            },
        }
    }
}

pub open spec fn exec_stmt(st: Stmt, s: int, f: nat) -> (int, Seq<Ev>)
    decreases f, st,
{
    match st {
        Stmt::While { cond, body } => exec_while(cond, body, s, s, false, f),
        Stmt::DoWhile { cond, body } => exec_dowhile(cond, body, s, f),
        Stmt::ForInit { init, cond, step, body } => exec_forinit(init, cond, step, body, s, f),
        Stmt::Chain { op, cond, rhs } => exec_chain(op, cond, rhs, s, f),
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

proof fn unfold_dowhile_full(c: Cond, b: Seq<Stmt>, s: int, f: nat)
    ensures exec_dowhile(c, b, s, f) == (if f == 0 {
        (0, Seq::empty())
    } else {
        let be = exec_list(b, s, (f - 1) as nat);
        let ce = exec_cond(c, be.0);
        if decide(c) {
            let re = exec_dowhile(c, b, ce.0, (f - 1) as nat);
            (re.0, be.1.add(ce.1).add(re.1))
        } else {
            (be.0, be.1.add(ce.1))
        }
    }),
{
}

proof fn unfold_fori_full(
    c: Cond,
    b: Seq<Stmt>,
    stp: Seq<Stmt>,
    s: int,
    bl: int,
    rn: bool,
    f: nat,
)
    ensures exec_fori(c, b, stp, s, bl, rn, f) == (if f == 0 {
        ((if rn {
            bl
        } else {
            0
        }), Seq::empty())
    } else {
        let ce = exec_cond(c, s);
        if decide(c) {
            let be = exec_list(b, ce.0, (f - 1) as nat);
            let se = exec_list(stp, be.0, (f - 1) as nat);
            let re = exec_fori(c, b, stp, se.0, be.0, true, (f - 1) as nat);
            (re.0, ce.1.add(be.1).add(se.1).add(re.1))
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

proof fn unfold_forinit(
    init: Seq<Stmt>,
    c: Cond,
    stp: Seq<Stmt>,
    b: Seq<Stmt>,
    s: int,
    f: nat,
)
    ensures exec_forinit(init, c, stp, b, s, f) == (if f == 0 {
        (0, Seq::empty())
    } else {
        let ie = exec_list(init, s, (f - 1) as nat);
        let re = exec_fori(c, b, stp, ie.0, 0, false, (f - 1) as nat);
        (re.0, ie.1.add(re.1))
    }),
{
}

proof fn unfold_chain(
    op: ChainOp,
    c: Cond,
    rhs: Seq<Stmt>,
    s: int,
    f: nat,
)
    ensures exec_chain(op, c, rhs, s, f) == (if f == 0 {
        (0, Seq::empty())
    } else {
        let ce = exec_cond(c, s);
        match op {
            ChainOp::And => {
                if decide(c) {
                    let re = exec_list(rhs, ce.0, (f - 1) as nat);
                    (re.0, ce.1.add(re.1))
                } else {
                    (1, ce.1)
                }
            },
            ChainOp::Or => {
                if decide(c) {
                    (0, ce.1)
                } else {
                    let re = exec_list(rhs, ce.0, (f - 1) as nat);
                    (re.0, ce.1.add(re.1))
                }
            },
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
        Stmt::DoWhile { cond, body } => {
            reveal_with_fuel(wf_stmt, 2);
            assert(body.len() > 0);
            dowhile_blind(cond, body, s1, s2, f);
        },
        Stmt::ForInit { init, cond, step, body } => {
            reveal_with_fuel(wf_stmt, 2);
            assert(body.len() > 0);
            forinit_blind(init, cond, step, body, s1, s2, f);
        },
        Stmt::Chain { op, cond, rhs } => {
            reveal_with_fuel(wf_stmt, 2);
            assert(rhs.len() > 0);
            chain_blind(op, cond, rhs, s1, s2, f);
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

proof fn dowhile_blind(c: Cond, b: Seq<Stmt>, s1: int, s2: int, f: nat)
    requires clean_list(b), wf_list(b), b.len() > 0,
    ensures exec_dowhile(c, b, s1, f) == exec_dowhile(c, b, s2, f),
    decreases f, list_size(b),
{
    unfold_dowhile_full(c, b, s1, f);
    unfold_dowhile_full(c, b, s2, f);
    if f > 0 {
        clean_list_blind(b, s1, s2, (f - 1) as nat);
        let be1 = exec_list(b, s1, (f - 1) as nat);
        let be2 = exec_list(b, s2, (f - 1) as nat);
        assert(be1 == be2);
        assert(exec_cond(c, be1.0) == exec_cond(c, be2.0));
    }
}

proof fn chain_blind(op: ChainOp, c: Cond, rhs: Seq<Stmt>, s1: int, s2: int, f: nat)
    requires clean_list(rhs), wf_list(rhs), rhs.len() > 0,
    ensures exec_chain(op, c, rhs, s1, f) == exec_chain(op, c, rhs, s2, f),
    decreases f, list_size(rhs),
{
    unfold_chain(op, c, rhs, s1, f);
    unfold_chain(op, c, rhs, s2, f);
    let ce1 = exec_cond(c, s1);
    let ce2 = exec_cond(c, s2);
    assert(ce1.1 == ce2.1);
    if f > 0 {
        clean_list_blind(rhs, ce1.0, ce2.0, (f - 1) as nat);
    }
}

proof fn fori_blind(
    c: Cond,
    b: Seq<Stmt>,
    stp: Seq<Stmt>,
    s1: int,
    s2: int,
    bl: int,
    rn: bool,
    f: nat,
)
    requires
        clean_list(b),
        wf_list(b),
        b.len() > 0,
        clean_list(stp),
        wf_list(stp),
    ensures exec_fori(c, b, stp, s1, bl, rn, f) == exec_fori(c, b, stp, s2, bl, rn, f),
    decreases f, list_size(b), list_size(stp),
{
    unfold_fori_full(c, b, stp, s1, bl, rn, f);
    unfold_fori_full(c, b, stp, s2, bl, rn, f);
    if f > 0 {
        let ce1 = exec_cond(c, s1);
        let ce2 = exec_cond(c, s2);
        assert(ce1.1 == ce2.1);
        if decide(c) {
            clean_list_blind(b, ce1.0, ce2.0, (f - 1) as nat);
            let be1 = exec_list(b, ce1.0, (f - 1) as nat);
            let be2 = exec_list(b, ce2.0, (f - 1) as nat);
            assert(be1 == be2);
            if stp.len() > 0 {
                clean_list_blind(stp, be1.0, be2.0, (f - 1) as nat);
            }
            let se1 = exec_list(stp, be1.0, (f - 1) as nat);
            let se2 = exec_list(stp, be2.0, (f - 1) as nat);
            assert(se1 == se2);
            fori_blind(c, b, stp, se1.0, se2.0, be1.0, true, (f - 1) as nat);
        }
    }
}

proof fn forinit_blind(
    init: Seq<Stmt>,
    c: Cond,
    stp: Seq<Stmt>,
    b: Seq<Stmt>,
    s1: int,
    s2: int,
    f: nat,
)
    requires
        clean_list(init),
        wf_list(init),
        clean_list(stp),
        wf_list(stp),
        clean_list(b),
        wf_list(b),
        b.len() > 0,
    ensures exec_forinit(init, c, stp, b, s1, f) == exec_forinit(init, c, stp, b, s2, f),
    decreases f, list_size(init), list_size(b), list_size(stp),
{
    unfold_forinit(init, c, stp, b, s1, f);
    unfold_forinit(init, c, stp, b, s2, f);
    if f > 0 {
        if init.len() > 0 {
            clean_list_blind(init, s1, s2, (f - 1) as nat);
        }
        let ie1 = exec_list(init, s1, (f - 1) as nat);
        let ie2 = exec_list(init, s2, (f - 1) as nat);
        assert(ie1.1 == ie2.1);
        fori_blind(c, b, stp, ie1.0, ie2.0, 0, false, (f - 1) as nat);
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

// `do { body } while (test)` → the same loop on the typed boolean. The
// entries may differ (the test writes 0/1 on the source face and
// nothing on the lowered one), which is why the statement is generalised
// over both entry statuses: the body is clean, so it is blind to them.
proof fn dowhile_sound_any(c: Cond, b: Seq<Stmt>, s1: int, s2: int, f: nat)
    requires clean_list(b), wf_list(b), b.len() > 0,
    ensures exec_dowhile(rewrite_cond(c), rewrite_list(b), s1, f) == exec_dowhile(c, b, s2, f),
    decreases f, list_size(b),
{
    unfold_dowhile_full(rewrite_cond(c), rewrite_list(b), s1, f);
    unfold_dowhile_full(c, b, s2, f);
    if f > 0 {
        exec_list_sound(b, s1, (f - 1) as nat);
        clean_list_blind(b, s1, s2, (f - 1) as nat);
        let be1 = exec_list(rewrite_list(b), s1, (f - 1) as nat);
        let be2 = exec_list(b, s2, (f - 1) as nat);
        assert(be1 == be2);
        assert(decide(rewrite_cond(c)) == decide(c));
        if decide(c) {
            let ce1 = exec_cond(rewrite_cond(c), be1.0);
            let ce2 = exec_cond(c, be2.0);
            dowhile_sound_any(c, b, ce1.0, ce2.0, (f - 1) as nat);
        }
    }
}

// `for (; test ;) { body; step }` → typed test: the loop reports the
// last BODY status (bash's `for`/`while` rule), never the test's and
// never the step's, so the differing test-status writes are invisible
// once the body and the step are clean (blind).
proof fn fori_sound_any(
    c: Cond,
    b: Seq<Stmt>,
    stp: Seq<Stmt>,
    s1: int,
    s2: int,
    bl: int,
    rn: bool,
    f: nat,
)
    requires
        clean_list(b),
        wf_list(b),
        b.len() > 0,
        clean_list(stp),
        wf_list(stp),
    ensures exec_fori(rewrite_cond(c), rewrite_list(b), rewrite_list(stp), s1, bl, rn, f) == exec_fori(
        c,
        b,
        stp,
        s2,
        bl,
        rn,
        f,
    ),
    decreases f, list_size(b), list_size(stp),
{
    unfold_fori_full(rewrite_cond(c), rewrite_list(b), rewrite_list(stp), s1, bl, rn, f);
    unfold_fori_full(c, b, stp, s2, bl, rn, f);
    if f > 0 {
        let ce1 = exec_cond(rewrite_cond(c), s1);
        let ce2 = exec_cond(c, s2);
        assert(decide(rewrite_cond(c)) == decide(c));
        if decide(c) {
            exec_list_sound(b, ce1.0, (f - 1) as nat);
            clean_list_blind(b, ce1.0, ce2.0, (f - 1) as nat);
            let be1 = exec_list(rewrite_list(b), ce1.0, (f - 1) as nat);
            let be2 = exec_list(b, ce2.0, (f - 1) as nat);
            assert(be1 == be2);
            exec_list_sound(stp, be1.0, (f - 1) as nat);
            if stp.len() > 0 {
                clean_list_blind(stp, be1.0, be2.0, (f - 1) as nat);
            }
            let se1 = exec_list(rewrite_list(stp), be1.0, (f - 1) as nat);
            let se2 = exec_list(stp, be2.0, (f - 1) as nat);
            assert(se1 == se2);
            fori_sound_any(c, b, stp, se1.0, se2.0, be1.0, true, (f - 1) as nat);
        }
    }
}

proof fn forinit_sound_any(
    init: Seq<Stmt>,
    c: Cond,
    stp: Seq<Stmt>,
    b: Seq<Stmt>,
    s1: int,
    s2: int,
    f: nat,
)
    requires
        clean_list(init),
        wf_list(init),
        clean_list(stp),
        wf_list(stp),
        clean_list(b),
        wf_list(b),
        b.len() > 0,
    ensures exec_forinit(rewrite_list(init), rewrite_cond(c), rewrite_list(stp), rewrite_list(b), s1, f)
        == exec_forinit(init, c, stp, b, s2, f),
    decreases f, list_size(init), list_size(b), list_size(stp),
{
    unfold_forinit(rewrite_list(init), rewrite_cond(c), rewrite_list(stp), rewrite_list(b), s1, f);
    unfold_forinit(init, c, stp, b, s2, f);
    if f > 0 {
        exec_list_sound(init, s1, (f - 1) as nat);
        if init.len() > 0 {
            clean_list_blind(init, s1, s2, (f - 1) as nat);
        }
        let ie1 = exec_list(rewrite_list(init), s1, (f - 1) as nat);
        let ie2 = exec_list(init, s2, (f - 1) as nat);
        assert(ie1.1 == ie2.1);
        fori_sound_any(c, b, stp, ie1.0, ie2.0, 0, false, (f - 1) as nat);
    }
}

// Statement-position `&&`/`||`: both faces report the short-circuit
// status (1 for a failed And lead, 0 for a succeeded Or lead) and the
// rhs runs exactly when the value says. The only status visible to a
// `$?` reader would be the lead's write inside the rhs, excluded by
// `clean_list(rhs)` — the transform's `ir_expr_reads_status` guard.
proof fn chain_sound(op: ChainOp, c: Cond, rhs: Seq<Stmt>, s: int, f: nat)
    requires clean_list(rhs), wf_list(rhs), rhs.len() > 0,
    ensures exec_chain(op, rewrite_cond(c), rewrite_list(rhs), s, f) == exec_chain(
        op,
        c,
        rhs,
        s,
        f,
    ),
    decreases f, list_size(rhs),
{
    unfold_chain(op, rewrite_cond(c), rewrite_list(rhs), s, f);
    unfold_chain(op, c, rhs, s, f);
    if f > 0 {
        let ce1 = exec_cond(rewrite_cond(c), s);
        let ce2 = exec_cond(c, s);
        assert(decide(rewrite_cond(c)) == decide(c));
        assert(ce1.1 == ce2.1);
        if (op == ChainOp::And && decide(c)) || (op == ChainOp::Or && !decide(c)) {
            exec_list_sound(rhs, ce1.0, (f - 1) as nat);
            clean_list_blind(rhs, ce1.0, ce2.0, (f - 1) as nat);
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
        Stmt::DoWhile { cond, body } => {
            assert(clean_list(body));
            reveal_with_fuel(rewrite_stmt, 2);
            assert(rewrite_stmt(st) == Stmt::DoWhile {
                cond: rewrite_cond(cond),
                body: rewrite_list(body),
            });
            dowhile_sound_any(cond, body, s, s, f);
        },
        Stmt::ForInit { init, cond, step, body } => {
            assert(clean_list(init) && clean_list(step) && clean_list(body));
            reveal_with_fuel(rewrite_stmt, 2);
            assert(rewrite_stmt(st) == Stmt::ForInit {
                init: rewrite_list(init),
                cond: rewrite_cond(cond),
                step: rewrite_list(step),
                body: rewrite_list(body),
            });
            forinit_sound_any(init, cond, step, body, s, s, f);
        },
        Stmt::Chain { op, cond, rhs } => {
            assert(clean_list(rhs));
            reveal_with_fuel(rewrite_stmt, 2);
            assert(rewrite_stmt(st) == Stmt::Chain {
                op,
                cond: rewrite_cond(cond),
                rhs: rewrite_list(rhs),
            });
            chain_sound(op, cond, rhs, s, f);
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

// `do { body } while (test)`: the body runs once, then the (false)
// test ends the loop, and the loop reports the BODY status.
proof fn pin_dowhile_runs_the_body_before_the_test()
    ensures exec_stmt(Stmt::DoWhile {
        cond: Cond::Test(false),
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 5, 4) == (0, seq![Ev::Print(1)]),
{
    reveal_with_fuel(exec_stmt, 8);
    reveal_with_fuel(exec_dowhile, 8);
    reveal_with_fuel(exec_list, 8);
}

// The do-while rewrite agrees on the same program.
proof fn pin_dowhile_lowered_agrees()
    ensures exec_stmt(Stmt::DoWhile {
        cond: Cond::Test(false),
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 5, 4) == exec_stmt(Stmt::DoWhile {
        cond: Cond::Bool(false),
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 5, 4),
{
    assert(clean_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(wf_list, 4);
    };
    dowhile_sound_any(Cond::Test(false), seq![Stmt::Run(Cmd::Run(1))], 5, 5, 4);
}

// A `for (; test ;)` loop that never iterates reports 0 (bash's rule).
proof fn pin_forinit_no_iteration_reports_zero()
    ensures exec_stmt(Stmt::ForInit {
        init: Seq::empty(),
        cond: Cond::Test(false),
        step: Seq::empty(),
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 9, 4) == (0, Seq::empty()),
{
    reveal_with_fuel(exec_stmt, 8);
    reveal_with_fuel(exec_forinit, 8);
    reveal_with_fuel(exec_fori, 8);
}

// `for (; test ;) { body; step }` agrees after the rewrite; the loop
// reports the body status even though the step runs last.
proof fn pin_forinit_lowered_agrees()
    ensures exec_stmt(Stmt::ForInit {
        init: seq![Stmt::Run(Cmd::Run(3))],
        cond: Cond::Test(false),
        step: seq![Stmt::Run(Cmd::Fail(4))],
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 5, 3) == exec_stmt(Stmt::ForInit {
        init: seq![Stmt::Run(Cmd::Run(3))],
        cond: Cond::Bool(false),
        step: seq![Stmt::Run(Cmd::Fail(4))],
        body: seq![Stmt::Run(Cmd::Run(1))],
    }, 5, 3),
{
    assert(clean_list(seq![Stmt::Run(Cmd::Run(3))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Run(3))])) by {
        reveal_with_fuel(wf_list, 4);
    };
    assert(clean_list(seq![Stmt::Run(Cmd::Fail(4))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Fail(4))])) by {
        reveal_with_fuel(wf_list, 4);
    };
    assert(clean_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Run(1))])) by {
        reveal_with_fuel(wf_list, 4);
    };
    forinit_sound_any(
        seq![Stmt::Run(Cmd::Run(3))],
        Cond::Test(false),
        seq![Stmt::Run(Cmd::Fail(4))],
        seq![Stmt::Run(Cmd::Run(1))],
        5,
        5,
        3,
    );
    reveal_with_fuel(rewrite_stmt, 4);
    reveal_with_fuel(exec_stmt, 4);
}

// `[[ false ]] && cmd` short-circuits and reports 1 — the value status,
// NOT the incoming status.
proof fn pin_chain_short_circuit_reports_the_value_status()
    ensures exec_stmt(Stmt::Chain {
        op: ChainOp::And,
        cond: Cond::Test(false),
        rhs: seq![Stmt::Run(Cmd::Run(1))],
    }, 0, 3) == (1, Seq::empty()),
{
    reveal_with_fuel(exec_stmt, 6);
    reveal_with_fuel(exec_chain, 6);
}

// A taken `&&` runs its rhs and reports the rhs status; a false `||`
// runs its rhs too. Both faces of the rewrite agree on the trace.
proof fn pin_chain_lowered_agrees()
    ensures
        exec_stmt(Stmt::Chain {
            op: ChainOp::And,
            cond: Cond::Test(true),
            rhs: seq![Stmt::Run(Cmd::Fail(2))],
        }, 5, 3) == exec_stmt(Stmt::Chain {
            op: ChainOp::And,
            cond: Cond::Bool(true),
            rhs: seq![Stmt::Run(Cmd::Fail(2))],
        }, 5, 3),
        exec_stmt(Stmt::Chain {
            op: ChainOp::Or,
            cond: Cond::Test(false),
            rhs: seq![Stmt::Run(Cmd::Run(2))],
        }, 5, 3) == exec_stmt(Stmt::Chain {
            op: ChainOp::Or,
            cond: Cond::Bool(false),
            rhs: seq![Stmt::Run(Cmd::Run(2))],
        }, 5, 3),
{
    assert(clean_list(seq![Stmt::Run(Cmd::Fail(2))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Fail(2))])) by {
        reveal_with_fuel(wf_list, 4);
    };
    chain_sound(ChainOp::And, Cond::Test(true), seq![Stmt::Run(Cmd::Fail(2))], 5, 3);
    reveal_with_fuel(rewrite_stmt, 4);
    reveal_with_fuel(exec_stmt, 4);
    assert(clean_list(seq![Stmt::Run(Cmd::Run(2))])) by {
        reveal_with_fuel(clean_list, 4);
    };
    assert(wf_list(seq![Stmt::Run(Cmd::Run(2))])) by {
        reveal_with_fuel(wf_list, 4);
    };
    chain_sound(ChainOp::Or, Cond::Test(false), seq![Stmt::Run(Cmd::Run(2))], 5, 3);
    reveal_with_fuel(rewrite_stmt, 4);
    reveal_with_fuel(exec_stmt, 4);
}

// Both faces of the seed: a satisfied `||` lead reports 0 without
// running the rhs, and a satisfied `&&` lead runs the rhs and reports
// the rhs status (the 0 seed is overwritten by the rhs). These are the
// values the three emitters' status writes produce (see the header).
proof fn pin_chain_seed_status_both_faces()
    ensures
        exec_stmt(Stmt::Chain {
            op: ChainOp::Or,
            cond: Cond::Test(true),
            rhs: seq![Stmt::Run(Cmd::Fail(3))],
        }, 7, 3) == (0, Seq::empty()),
        exec_stmt(Stmt::Chain {
            op: ChainOp::And,
            cond: Cond::Test(true),
            rhs: seq![Stmt::Run(Cmd::Fail(3))],
        }, 7, 3) == (1, seq![Ev::Print(3)]),
{
    reveal_with_fuel(exec_stmt, 8);
    reveal_with_fuel(exec_chain, 8);
}

fn main() {}

}
