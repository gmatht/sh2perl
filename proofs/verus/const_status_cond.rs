// Standalone model of const-status-cond
// (sh2perl/src/transforms/const_status_cond.rs): a bare `true` / `:` /
// `false` command in condition position is a CONSTANT — the loop/branch
// decision is known at translate time, but the command still runs (its
// status is observable inside the body), so it MOVES to the front of
// the taken body/branch and the condition becomes the literal.
// Proves the soundness obligation:
//
//   rewrite_sound: rewriting preserves the (status, print-trace) run
//   from any start status, at every fuel bound.
//
// Names/texts are interned to nat. The observable effect is the print
// trace plus the final status; `true`/`false`/`:` print nothing (their
// only observable is the status they set). Fuel bounds loop iterations
// (Verus needs structural termination); the theorem holds for all
// fuel, hence for the unbounded runs the fuel approximates.
//
// What is modelled and what is NOT (fidelity, checked against
// const_status_cond.rs's 7-row table + refusal rules):
//   - The full table: while-enters / until-enters (cond -> Bool(true),
//     command prepended to body), while-never / until-never (whole loop
//     -> StatusZero), if-takes-then (cond -> Bool(true), command
//     prepended to then), if-takes-else (cond -> Bool(false), command
//     prepended to else), if-takes-none (whole if -> StatusZero).
//   - Refusals (all identity, recurse-only): command with words
//     (`true $(rm f)` still runs the substitution), if with elsif
//     arms (later conditions decide truth), non-const conditions.
//     `do…while` is refused in the real code; the model omits the node
//     (nothing to refuse).
//   - Negation (`until` / `! cmd`) is a flag on the condition; the
//     MOVED command is the inner unnegated one (status per its truth),
//     the decision uses enters = negated ? !truth : truth — exactly
//     the real status_const/rewrite split.
//   - Bash exit-status rules from the header: a loop whose body never
//     ran reports 0 (never the condition's status); a loop that ran
//     reports the body's last status; an if with no taken branch
//     reports 0. The model's exec_while/exec_if encode exactly these.
//   - StatusZero is observably ([], status:=0): the real node is an
//     `exec true` whose only observable is status 0 — same denotation.
//   - Non-const conditions and opaque body commands execute
//     identically on both sides (same fuel, same store-free
//     semantics); the proof never inspects their meaning.
//   - Flat statement lists + While/If nesting (the real rewrite
//     recurses into every container; each nested list is the same
//     lemma). Statement lists inside conditions: none (conditions are
//     single command nodes, as the guard requires).
use vstd::prelude::*;

verus! {

pub type Name = nat;

// ---- Language ----

pub enum Cmd {
    // Silent constant commands (print nothing; set status).
    ConstTrue,
    ConstFalse,
    ConstColon,
    // Prints text (interned), sets status 0.
    PrintT(Name),
    // Opaque body command: deterministic, identical on both sides.
    // Emits Opq, leaves status unchanged.
    Opaque,
}

pub enum Cond {
    // Bare true/:/false command with negation flag and words flag.
    // truth: true for true/colon, false for false.
    CConst { truth: bool, negated: bool, has_words: bool },
    CBool(bool),
    COther,
}

pub enum Stmt {
    While { cond: Cond, body: Seq<Stmt> },
    If { cond: Cond, then_b: Seq<Stmt>, has_elsifs: bool, els_b: Seq<Stmt> },
    Run(Cmd),
    StatusZero,
    Other,
}

pub enum Ev {
    Print(Name),
    Opq,
}

// ---- Sizes (lexicographic fuel/size termination) ----

pub open spec fn stmt_size(st: Stmt) -> nat
    decreases st,
{
    match st {
        Stmt::While { body, .. } => 1 + list_size(body),
        Stmt::If { then_b, els_b, .. } => 1 + list_size(then_b) + list_size(els_b),
        _ => 1,
    }
}

// +1 per cons cell: every proper sublist and every head is strictly
// smaller than the list (no empty-tail equality trap).
pub open spec fn list_size(l: Seq<Stmt>) -> nat
    decreases l,
{
    if l.len() == 0 {
        0
    } else {
        1 + stmt_size(l[0]) + list_size(l.drop_first())
    }
}

// ---- The rewrite (mirrors rewrite() row by row) ----

// Effective decision: enters the body/then iff truth != negated.
pub open spec fn enters(truth: bool, negated: bool) -> bool {
    negated != truth
}

// The moved command: the inner unnegated constant (status per truth).
pub open spec fn moved_cmd(truth: bool) -> Cmd {
    if truth {
        Cmd::ConstTrue
    } else {
        Cmd::ConstFalse
    }
}

pub open spec fn const_status(c: Cond) -> Option<(bool, bool)> {
    match c {
        Cond::CConst { truth, negated, has_words } => {
            if has_words {
                None
            } else {
                Some((truth, negated))
            }
        },
        _ => None,
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
            match const_status(cond) {
                Some(tn) => {
                    let (truth, negated) = tn;
                    if enters(truth, negated) {
                        Stmt::While {
                            cond: Cond::CBool(true),
                            body: seq![Stmt::Run(if truth {
                                Cmd::ConstTrue
                            } else {
                                Cmd::ConstFalse
                            })].add(rewrite_list(body)),
                        }
                    } else {
                        Stmt::StatusZero
                    }
                },
                None => Stmt::While { cond, body: rewrite_list(body) },
            }
        },
        Stmt::If { cond, then_b, has_elsifs, els_b } => {
            match const_status(cond) {
                Some(tn) => {
                    let (truth, negated) = tn;
                    if has_elsifs {
                        Stmt::If { cond, then_b: rewrite_list(then_b), has_elsifs, els_b: rewrite_list(els_b) }
                    } else if enters(truth, negated) {
                        Stmt::If {
                            cond: Cond::CBool(true),
                            then_b: seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(then_b)),
                            has_elsifs,
                            els_b: rewrite_list(els_b),
                        }
                    } else if els_b.len() == 0 {
                        Stmt::StatusZero
                    } else {
                        Stmt::If {
                            cond: Cond::CBool(false),
                            then_b: rewrite_list(then_b),
                            has_elsifs,
                            els_b: seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(els_b)),
                        }
                    }
                },
                None => Stmt::If { cond, then_b: rewrite_list(then_b), has_elsifs, els_b: rewrite_list(els_b) },
            }
        },
        _ => st,
    }
}

// ---- Execution (fuel-bounded, bash exit-status rules) ----

pub open spec fn cmd_status(c: Cmd) -> int {
    match c {
        Cmd::ConstTrue => 0,
        Cmd::ConstFalse => 1,
        Cmd::ConstColon => 0,
        Cmd::PrintT(_) => 0,
        Cmd::Opaque => -1,
    }
}

pub open spec fn exec_cmd(c: Cmd, s: int) -> (int, Seq<Ev>) {
    match c {
        Cmd::ConstTrue => (0, Seq::empty()),
        Cmd::ConstFalse => (1, Seq::empty()),
        Cmd::ConstColon => (0, Seq::empty()),
        Cmd::PrintT(t) => (0, seq![Ev::Print(t)]),
        Cmd::Opaque => (s, seq![Ev::Opq]),
    }
}

// Condition execution: const commands run silently (status set);
// Bool/other conditions touch nothing.
pub open spec fn exec_cond(c: Cond, s: int) -> (int, Seq<Ev>) {
    match c {
        Cond::CConst { truth, .. } => {
            if truth {
                (0, Seq::empty())
            } else {
                (1, Seq::empty())
            }
        },
        _ => (s, Seq::empty()),
    }
}

// The decision the loop/branch takes (status-independent: known at
// translate time for const conditions).
pub open spec fn decide(c: Cond) -> bool {
    match c {
        Cond::CConst { truth, negated, .. } => enters(truth, negated),
        Cond::CBool(b) => b,
        Cond::COther => false,
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

pub open spec fn exec_stmt(st: Stmt, s: int, f: nat) -> (int, Seq<Ev>)
    decreases f, st,
{
    match st {
        Stmt::While { cond, body } => exec_while(cond, body, s, s, false, f),
        Stmt::If { cond, then_b, els_b, .. } => {
            let ce = exec_cond(cond, s);
            if decide(cond) {
                let te = exec_list(then_b, ce.0, f);
                (te.0, ce.1.add(te.1))
            } else if els_b.len() == 0 {
                (0, ce.1)
            } else {
                let ee = exec_list(els_b, ce.0, f);
                (ee.0, ce.1.add(ee.1))
            }
        },
        Stmt::Run(c) => exec_cmd(c, s),
        Stmt::StatusZero => (0, Seq::empty()),
        Stmt::Other => (s, seq![Ev::Opq]),
    }
}

// A loop whose body never ran reports 0 (blast/ran thread the body's
// last status); fuel exhaustion follows the same convention on both
// sides (equality only needs agreement, not a particular value).
pub open spec fn exec_while(c: Cond, b: Seq<Stmt>, s: int, blast: int, ran: bool, f: nat) -> (int, Seq<Ev>)
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

// ---- Soundness ----

// A moved status command as a statement runs like the command.
proof fn run_c_exec(truth: bool, s: int, f: nat)
    ensures exec_stmt(Stmt::Run(moved_cmd(truth)), s, f) == exec_cmd(moved_cmd(truth), s),
{
    if truth {
        assert(moved_cmd(truth) == Cmd::ConstTrue);
    } else {
        assert(moved_cmd(truth) == Cmd::ConstFalse);
    }
}

// rewrite_list preserves length (elementwise map).
proof fn rewrite_list_len(l: Seq<Stmt>)
    ensures rewrite_list(l).len() == l.len(),
    decreases l,
{
    if l.len() == 0 {
    } else {
        rewrite_cons(l[0], l.drop_first());
        rewrite_list_len(l.drop_first());
    }
}

// Same-condition If congruence: equal branches (from the same cond
// state) give equal runs.
proof fn if_same_cond(cond: Cond, t1: Seq<Stmt>, t2: Seq<Stmt>, hs: bool, e1: Seq<Stmt>, e2: Seq<Stmt>, s: int, f: nat)
    requires
        exec_list(t1, exec_cond(cond, s).0, f) == exec_list(t2, exec_cond(cond, s).0, f),
        exec_list(e1, exec_cond(cond, s).0, f) == exec_list(e2, exec_cond(cond, s).0, f),
        e1.len() == 0 == e2.len() || e1.len() > 0 && e2.len() > 0,
    ensures exec_stmt(Stmt::If { cond, then_b: t1, has_elsifs: hs, els_b: e1 }, s, f)
        == exec_stmt(Stmt::If { cond, then_b: t2, has_elsifs: hs, els_b: e2 }, s, f),
{
    let ce = exec_cond(cond, s);
    unfold_if(cond, t1, hs, e1, s, f);
    unfold_if(cond, t2, hs, e2, s, f);
    if decide(cond) {
    } else if e1.len() == 0 {
        assert(e2.len() == 0);
    } else {
        assert(e1.len() > 0 && e2.len() > 0);
    }
}

// Unfold helpers.
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

proof fn rewrite_cons(h: Stmt, t: Seq<Stmt>)
    ensures rewrite_list(seq![h].add(t)) == seq![rewrite_stmt(h)].add(rewrite_list(t)),
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

// A const command's condition execution is its body execution:
// same status, same (empty) trace.
proof fn const_cmd_exec(truth: bool, s: int)
    ensures
        exec_cond(Cond::CConst { truth, negated: false, has_words: false }, s)
            == exec_cmd(moved_cmd(truth), s),
{
    if truth {
    } else {
    }
}

// List soundness: rewriting preserves the run (mutual with stmt).
proof fn exec_list_sound(l: Seq<Stmt>, s: int, f: nat)
    ensures exec_list(rewrite_list(l), s, f) == exec_list(l, s, f),
    decreases f, list_size(l),
{
    if l.len() == 0 {
    } else {
        rewrite_cons(l[0], l.drop_first());
        exec_stmt_sound(l[0], s, f);
        let h1 = exec_stmt(rewrite_stmt(l[0]), s, f);
        let h2 = exec_stmt(l[0], s, f);
        assert(h1 == h2);
        exec_list_sound(l.drop_first(), h1.0, f);
        list_cons(rewrite_stmt(l[0]), rewrite_list(l.drop_first()), s, f);
        list_cons(l[0], l.drop_first(), s, f);
    }
}

// A Some verdict pins the condition shape: bare wordless constant.
proof fn const_status_shape(cond: Cond, truth: bool, negated: bool)
    requires const_status(cond) == Some((truth, negated)),
    ensures cond == (Cond::CConst { truth, negated, has_words: false }),
{
    match cond {
        Cond::CConst { truth: t, negated: n, has_words: w } => {
            if w {
            } else {
                assert(t == truth && n == negated);
            }
        },
        _ => {},
    }
}

// Unfold exec_stmt on a While/If head (non-recursive, definitional).
proof fn unfold_while(cond: Cond, body: Seq<Stmt>, s: int, f: nat)
    ensures exec_stmt(Stmt::While { cond, body }, s, f) == exec_while(cond, body, s, s, false, f),
{
}

proof fn unfold_if(cond: Cond, then_b: Seq<Stmt>, has_elsifs: bool, els_b: Seq<Stmt>, s: int, f: nat)
    ensures exec_stmt(Stmt::If { cond, then_b, has_elsifs, els_b }, s, f) == {
        let ce = exec_cond(cond, s);
        if decide(cond) {
            let te = exec_list(then_b, ce.0, f);
            (te.0, ce.1.add(te.1))
        } else if els_b.len() == 0 {
            (0, ce.1)
        } else {
            let ee = exec_list(els_b, ce.0, f);
            (ee.0, ce.1.add(ee.1))
        }
    },
{
}

// Unfold a loop (both fuel cases in one equation, so callers at any
// fuel — including (f-1) which may be zero — can use it directly).
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

// Unfold a stopped loop (fuel out).
proof fn unfold_while_stop(c: Cond, b: Seq<Stmt>, s: int, bl: int, rn: bool)
    ensures exec_while(c, b, s, bl, rn, 0) == ((if rn {
        bl
    } else {
        0
    }), Seq::empty()),
{
}

// Statement soundness, by cases over the 7-row table + refusals.
// General entering-loop lemma, arbitrary entry (blast, ran): the
// rewritten loop (Bool cond, [C]+body') runs identically to the
// original at every fuel. Covers both loop entry (ran=false) and
// iteration tails (ran=true) — the tail case is why exec_stmt_sound
// alone (entry-only) cannot close the induction.
proof fn while_enters_gen(cond: Cond, body: Seq<Stmt>, truth: bool, negated: bool, s: int, bl: int, rn: bool, f: nat)
    requires
        const_status(cond) == Some((truth, negated)),
        enters(truth, negated),
    ensures exec_while(Cond::CBool(true), seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(body)), s, bl, rn, f)
        == exec_while(cond, body, s, bl, rn, f),
    decreases f, list_size(body),
{
    const_status_shape(cond, truth, negated);
    unfold_while_full(Cond::CBool(true), seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(body)), s, bl, rn, f);
    unfold_while_full(cond, body, s, bl, rn, f);
    if f == 0 {
    } else {
        assert(exec_cond(cond, s) == exec_cmd(moved_cmd(truth), s)) by {
            const_cmd_exec(truth, s);
        };
        assert(decide(cond));
        assert(exec_cond(Cond::CBool(true), s) == (s, Seq::empty()));
        assert(decide(Cond::CBool(true)));
        let ce = exec_cond(cond, s);
        let ce2 = exec_cmd(moved_cmd(truth), s);
        assert(ce2 == ce);
        assert(ce2.0 == ce.0);
        assert(ce2.1 == ce.1);
        exec_list_sound(body, ce.0, (f - 1) as nat);
        assert(exec_list(rewrite_list(body), ce.0, (f - 1) as nat) == exec_list(body, ce.0, (f - 1) as nat));
        list_cons(Stmt::Run(moved_cmd(truth)), rewrite_list(body), ce.0, (f - 1) as nat);
        let be = exec_list(body, ce.0, (f - 1) as nat);
        let beN = exec_list(seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(body)), s, (f - 1) as nat);
        assert(exec_stmt(Stmt::Run(moved_cmd(truth)), s, (f - 1) as nat) == ce2);
        assert(exec_list(rewrite_list(body), ce2.0, (f - 1) as nat) == exec_list(rewrite_list(body), ce.0, (f - 1) as nat));
        assert(exec_list(rewrite_list(body), ce2.0, (f - 1) as nat) == be);
        assert(beN.0 == be.0);
        assert(beN.1 == ce2.1.add(be.1));
        assert(beN.1 == be.1);
        while_enters_gen(cond, body, truth, negated, be.0, be.0, true, (f - 1) as nat);
    }
}

// General same-condition lemma: with identical conditions, a loop over
// the rewritten body runs like the loop over the original body
// (refused/non-const conditions).
proof fn while_same_cond(cond: Cond, body: Seq<Stmt>, s: int, bl: int, rn: bool, f: nat)
    ensures exec_while(cond, rewrite_list(body), s, bl, rn, f) == exec_while(cond, body, s, bl, rn, f),
    decreases f, list_size(body),
{
    unfold_while_full(cond, rewrite_list(body), s, bl, rn, f);
    unfold_while_full(cond, body, s, bl, rn, f);
    if f == 0 {
    } else {
        let ce = exec_cond(cond, s);
        exec_list_sound(body, ce.0, (f - 1) as nat);
        assert(exec_list(rewrite_list(body), ce.0, (f - 1) as nat) == exec_list(body, ce.0, (f - 1) as nat));
        if decide(cond) {
            let be = exec_list(body, ce.0, (f - 1) as nat);
            while_same_cond(cond, body, be.0, be.0, true, (f - 1) as nat);
        }
    }
}

// Row logic is inline (separate row lemmas cannot decrease: the
// branch calls sit at the same level as the If node itself).
proof fn exec_stmt_sound(st: Stmt, s: int, f: nat)
    ensures exec_stmt(rewrite_stmt(st), s, f) == exec_stmt(st, s, f),
    decreases f, stmt_size(st),
{
    match st {
        Stmt::While { cond, body } => {
            assert(st == Stmt::While { cond, body });
            match const_status(cond) {
                Some(tn) => {
                    let (truth, negated) = tn;
                    const_status_shape(cond, truth, negated);
                    if enters(truth, negated) {
                        // Rows 1-2 (while/until-enters): the general
                        // lemma covers entry and tails uniformly.
                        assert(rewrite_stmt(st) == Stmt::While {
                            cond: Cond::CBool(true),
                            body: seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(body)),
                        });
                        unfold_while(cond, body, s, f);
                        unfold_while(Cond::CBool(true), seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(body)), s, f);
                        while_enters_gen(cond, body, truth, negated, s, s, false, f);
                        // Explicit chain (diagnostic granularity).
                        assert(exec_stmt(rewrite_stmt(st), s, f)
                            == exec_stmt(Stmt::While {
                                cond: Cond::CBool(true),
                                body: seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(body)),
                            }, s, f));
                        assert(exec_stmt(Stmt::While {
                            cond: Cond::CBool(true),
                            body: seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(body)),
                        }, s, f)
                            == exec_stmt(Stmt::While { cond, body }, s, f));
                        assert(exec_stmt(Stmt::While { cond, body }, s, f) == exec_stmt(st, s, f));
                    } else {
                        // Rows 3-4 (never runs): orig runs C once and
                        // reports 0; StatusZero reports 0. Both silent.
                        assert(rewrite_stmt(st) == Stmt::StatusZero);
                        assert(exec_stmt(rewrite_stmt(st), s, f) == (0, Seq::empty()));
                        assert(!decide(cond));
                        assert(exec_cond(cond, s).1 == Seq::empty()) by {
                            if truth {
                            } else {
                            }
                        };
                        unfold_while(cond, body, s, f);
                        if f == 0 {
                            unfold_while_stop(cond, body, s, s, false);
                        } else {
                            unfold_while_full(cond, body, s, s, false, f);
                        }
                        assert(exec_stmt(rewrite_stmt(st), s, f) == exec_stmt(st, s, f));
                    }
                },
                None => {
                    assert(rewrite_stmt(st) == Stmt::While { cond, body: rewrite_list(body) });
                    unfold_while(cond, body, s, f);
                    unfold_while(cond, rewrite_list(body), s, f);
                    while_same_cond(cond, body, s, s, false, f);
                    assert(exec_stmt(rewrite_stmt(st), s, f) == exec_stmt(st, s, f));

                },
            }
        },
        Stmt::If { cond, then_b, has_elsifs, els_b } => {
            assert(st == Stmt::If { cond, then_b, has_elsifs, els_b });
            match const_status(cond) {
                Some(tn) => {
                    let (truth, negated) = tn;
                    const_status_shape(cond, truth, negated);
                    if has_elsifs {
                        assert(rewrite_stmt(st) == Stmt::If { cond, then_b: rewrite_list(then_b), has_elsifs, els_b: rewrite_list(els_b) });
                        let ce = exec_cond(cond, s);
                        exec_list_sound(then_b, ce.0, f);
                        assert(exec_list(rewrite_list(then_b), ce.0, f) == exec_list(then_b, ce.0, f));
                        exec_list_sound(els_b, ce.0, f);
                        assert(exec_list(rewrite_list(els_b), ce.0, f) == exec_list(els_b, ce.0, f));
                        rewrite_list_len(els_b);
                        if_same_cond(cond, rewrite_list(then_b), then_b, has_elsifs, rewrite_list(els_b), els_b, s, f);
                        assert(exec_stmt(rewrite_stmt(st), s, f) == exec_stmt(st, s, f));
                    } else if enters(truth, negated) {
                        // Row 5: [C-exec then then-branch] both sides.
                        assert(rewrite_stmt(st) == Stmt::If {
                            cond: Cond::CBool(true),
                            then_b: seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(then_b)),
                            has_elsifs,
                            els_b: rewrite_list(els_b),
                        });
                        unfold_if(cond, then_b, has_elsifs, els_b, s, f);
                        unfold_if(Cond::CBool(true), seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(then_b)), has_elsifs, rewrite_list(els_b), s, f);
                        assert(exec_cond(cond, s) == exec_cmd(moved_cmd(truth), s)) by {
                            const_cmd_exec(truth, s);
                        };
                        assert(decide(cond));
                        assert(decide(Cond::CBool(true)));
                        let ce = exec_cond(cond, s);
                        // New cond is silent nothing; orig cond is the silent C.
                        assert(exec_cond(Cond::CBool(true), s) == (s, Seq::empty()));
                        assert(ce.1 == Seq::empty()) by {
                            assert(ce == exec_cmd(moved_cmd(truth), s));
                            if truth {
                                assert(exec_cmd(moved_cmd(truth), s) == (0, Seq::empty()));
                            } else {
                                assert(exec_cmd(moved_cmd(truth), s) == (1, Seq::empty()));
                            }
                        };
                        assert(ce.0 == exec_cmd(moved_cmd(truth), s).0);
                        // [C] then rewritten-then == C then original-then.
                        run_c_exec(truth, s, f);
                        assert(exec_stmt(Stmt::Run(moved_cmd(truth)), s, f) == ce);
                        list_cons(Stmt::Run(moved_cmd(truth)), rewrite_list(then_b), ce.0, f);
                        exec_list_sound(then_b, ce.0, f);
                        assert(exec_list(rewrite_list(then_b), ce.0, f) == exec_list(then_b, ce.0, f));
                        let teN = exec_list(seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(then_b)), s, f);
                        let te = exec_list(then_b, ce.0, f);
                        assert(exec_stmt(Stmt::Run(moved_cmd(truth)), s, f) == exec_cmd(moved_cmd(truth), s));
                        let hh = exec_stmt(Stmt::Run(moved_cmd(truth)), s, f);
                        let tt = exec_list(rewrite_list(then_b), hh.0, f);
                        assert(hh.0 == ce.0);
                        assert(hh.1 == Seq::empty());
                        assert(tt == te);
                        assert(teN.1 == te.1);
                        assert(teN.0 == te.0);
                        // Both sides expand to the same (status, trace).
                        assert(exec_stmt(rewrite_stmt(st), s, f) == (te.0, ce.1.add(te.1)));
                        assert(exec_stmt(st, s, f) == (te.0, ce.1.add(te.1)));
                    } else if els_b.len() == 0 {
                        // Row 7: orig runs C once, takes nothing, reports
                        // 0; StatusZero reports 0. Both silent.
                        assert(rewrite_stmt(st) == Stmt::StatusZero);
                        assert(exec_stmt(rewrite_stmt(st), s, f) == (0, Seq::empty()));
                        assert(!decide(cond));
                        assert(exec_cond(cond, s).1 == Seq::empty()) by {
                            if truth {
                            } else {
                            }
                        };
                        unfold_if(cond, then_b, has_elsifs, els_b, s, f);
                        assert(exec_stmt(rewrite_stmt(st), s, f) == exec_stmt(st, s, f));
                    } else {
                        // Row 6: [C-exec then else-branch] both sides.
                        assert(rewrite_stmt(st) == Stmt::If {
                            cond: Cond::CBool(false),
                            then_b: rewrite_list(then_b),
                            has_elsifs,
                            els_b: seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(els_b)),
                        });
                        unfold_if(cond, then_b, has_elsifs, els_b, s, f);
                        unfold_if(Cond::CBool(false), rewrite_list(then_b), has_elsifs, seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(els_b)), s, f);
                        assert(exec_cond(cond, s) == exec_cmd(moved_cmd(truth), s)) by {
                            const_cmd_exec(truth, s);
                        };
                        assert(!decide(cond));
                        assert(!decide(Cond::CBool(false)));
                        let ce = exec_cond(cond, s);
                        assert(exec_cond(Cond::CBool(false), s) == (s, Seq::empty()));
                        assert(ce.1 == Seq::empty()) by {
                            assert(ce == exec_cmd(moved_cmd(truth), s));
                            if truth {
                                assert(exec_cmd(moved_cmd(truth), s) == (0, Seq::empty()));
                            } else {
                                assert(exec_cmd(moved_cmd(truth), s) == (1, Seq::empty()));
                            }
                        };
                        assert(ce.0 == exec_cmd(moved_cmd(truth), s).0);
                        run_c_exec(truth, s, f);
                        assert(exec_stmt(Stmt::Run(moved_cmd(truth)), s, f) == ce);
                        list_cons(Stmt::Run(moved_cmd(truth)), rewrite_list(els_b), ce.0, f);
                        exec_list_sound(els_b, ce.0, f);
                        assert(exec_list(rewrite_list(els_b), ce.0, f) == exec_list(els_b, ce.0, f));
                        let eeN = exec_list(seq![Stmt::Run(moved_cmd(truth))].add(rewrite_list(els_b)), s, f);
                        let ee = exec_list(els_b, ce.0, f);
                        assert(exec_stmt(Stmt::Run(moved_cmd(truth)), s, f) == exec_cmd(moved_cmd(truth), s));
                        let hh = exec_stmt(Stmt::Run(moved_cmd(truth)), s, f);
                        let tt = exec_list(rewrite_list(els_b), hh.0, f);
                        assert(hh.0 == ce.0);
                        assert(hh.1 == Seq::empty());
                        assert(tt == ee);
                        assert(eeN.1 == ee.1);
                        assert(eeN.0 == ee.0);
                        // Both sides expand to the same (status, trace).
                        assert(exec_stmt(rewrite_stmt(st), s, f) == (ee.0, ce.1.add(ee.1)));
                        assert(exec_stmt(st, s, f) == (ee.0, ce.1.add(ee.1)));
                    }
                },
                None => {
                    assert(rewrite_stmt(st) == Stmt::If { cond, then_b: rewrite_list(then_b), has_elsifs, els_b: rewrite_list(els_b) });
                    let ce = exec_cond(cond, s);
                    exec_list_sound(then_b, ce.0, f);
                    assert(exec_list(rewrite_list(then_b), ce.0, f) == exec_list(then_b, ce.0, f));
                    exec_list_sound(els_b, ce.0, f);
                    assert(exec_list(rewrite_list(els_b), ce.0, f) == exec_list(els_b, ce.0, f));
                    rewrite_list_len(els_b);
                    if_same_cond(cond, rewrite_list(then_b), then_b, has_elsifs, rewrite_list(els_b), els_b, s, f);
                    assert(exec_stmt(rewrite_stmt(st), s, f) == exec_stmt(st, s, f));
                },
            }
        },
        _ => {
            assert(rewrite_stmt(st) == st);
        },
    }
}
// THE TOP-LEVEL OBLIGATION.
proof fn rewrite_sound_top(p: Seq<Stmt>, s: int, f: nat)
    requires f >= 0,
    ensures exec_list(rewrite_list(p), s, f) == exec_list(p, s, f),
{
    exec_list_sound(p, s, f);
}

// ---- Fidelity pins (mirror const_status_cond.rs's tests) ----

// Unfold rewrite_list one step, by shape.
proof fn rw_step_kept(l: Seq<Stmt>)
    requires l.len() > 0,
    ensures rewrite_list(l) == seq![rewrite_stmt(l[0])].add(rewrite_list(l.drop_first())),
{
    reveal_with_fuel(rewrite_list, 2);
}

pub open spec fn const_true() -> Cond {
    Cond::CConst { truth: true, negated: false, has_words: false }
}

pub open spec fn const_false() -> Cond {
    Cond::CConst { truth: false, negated: false, has_words: false }
}

pub open spec fn print_hi() -> Stmt {
    Stmt::Run(Cmd::PrintT(1))
}

// Row: `until false; do echo hi` -> infinite loop, cond true, status
// write (the const command) first in the body. (`until C` is the
// negated spelling: truth=false, negated=true, enters.)
proof fn pin_until_false_infinite_with_status_inside()
    ensures rewrite_stmt(Stmt::While {
        cond: Cond::CConst { truth: false, negated: true, has_words: false },
        body: seq![Stmt::Run(Cmd::PrintT(1))],
    }) == (Stmt::While {
        cond: Cond::CBool(true),
        body: seq![Stmt::Run(Cmd::ConstFalse)].add(seq![Stmt::Run(Cmd::PrintT(1))]),
    }),
{
    assert(rewrite_list(seq![Stmt::Run(Cmd::PrintT(1))]) == seq![Stmt::Run(Cmd::PrintT(1))]) by {
        reveal_with_fuel(rewrite_list, 4);
    };
    assert(moved_cmd(false) == Cmd::ConstFalse);
    reveal_with_fuel(rewrite_stmt, 4);
}

// Row: `while true` -> cond literal, write kept, call dropped.
proof fn pin_while_true_shape()
    ensures rewrite_stmt(Stmt::While {
        cond: Cond::CConst { truth: true, negated: false, has_words: false },
        body: seq![print_hi()],
    }) == (Stmt::While {
        cond: Cond::CBool(true),
        body: seq![Stmt::Run(Cmd::ConstTrue)].add(seq![print_hi()]),
    }),
{
    assert(moved_cmd(true) == Cmd::ConstTrue);
    assert(rewrite_list(seq![print_hi()]) == seq![print_hi()]) by {
        reveal_with_fuel(rewrite_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// Row: `while false` -> StatusZero (body provably never runs).
proof fn pin_while_false_zero()
    ensures rewrite_stmt(Stmt::While { cond: const_false(), body: seq![print_hi()] }) == Stmt::StatusZero,
{
    reveal_with_fuel(rewrite_stmt, 4);
}

// Row: `if ! true` (negated) with no else -> StatusZero.
proof fn pin_if_negated_true_zero()
    ensures rewrite_stmt(Stmt::If {
        cond: Cond::CConst { truth: true, negated: true, has_words: false },
        then_b: seq![print_hi()],
        has_elsifs: false,
        els_b: Seq::empty(),
    }) == Stmt::StatusZero,
{
    reveal_with_fuel(rewrite_stmt, 4);
}

// Row: `if true; then A; else B` -> then runs with the write prefixed.
proof fn pin_if_true_then()
    ensures rewrite_stmt(Stmt::If {
        cond: const_true(),
        then_b: seq![print_hi()],
        has_elsifs: false,
        els_b: seq![print_hi()],
    }) == (Stmt::If {
        cond: Cond::CBool(true),
        then_b: seq![Stmt::Run(Cmd::ConstTrue)].add(seq![print_hi()]),
        has_elsifs: false,
        els_b: seq![print_hi()],
    }),
{
    assert(moved_cmd(true) == Cmd::ConstTrue);
    assert(rewrite_list(seq![print_hi()]) == seq![print_hi()]) by {
        reveal_with_fuel(rewrite_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// Row: `if false; then A; else B` -> else runs with the write prefixed.
proof fn pin_if_false_else()
    ensures rewrite_stmt(Stmt::If {
        cond: const_false(),
        then_b: seq![print_hi()],
        has_elsifs: false,
        els_b: seq![print_hi()],
    }) == (Stmt::If {
        cond: Cond::CBool(false),
        then_b: seq![print_hi()],
        has_elsifs: false,
        els_b: seq![Stmt::Run(Cmd::ConstFalse)].add(seq![print_hi()]),
    }),
{
    assert(moved_cmd(false) == Cmd::ConstFalse);
    assert(rewrite_list(seq![print_hi()]) == seq![print_hi()]) by {
        reveal_with_fuel(rewrite_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// Refusal: command with words is untouched.
proof fn pin_words_refused()
    ensures rewrite_stmt(Stmt::While {
        cond: Cond::CConst { truth: true, negated: false, has_words: true },
        body: seq![print_hi()],
    }) == (Stmt::While {
        cond: Cond::CConst { truth: true, negated: false, has_words: true },
        body: seq![print_hi()],
    }),
{
    reveal_with_fuel(rewrite_stmt, 4);
}

// Refusal: non-const condition is untouched.
proof fn pin_other_cond_untouched()
    ensures rewrite_stmt(Stmt::While { cond: Cond::COther, body: seq![print_hi()] }) == (Stmt::While {
        cond: Cond::COther,
        body: seq![print_hi()],
    }),
{
    assert(rewrite_list(seq![print_hi()]) == seq![print_hi()]) by {
        reveal_with_fuel(rewrite_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// Refusal: if with elsif arms keeps the command condition (later
// conditions decide truth, so the move would misattribute status).
proof fn pin_elsifs_refused()
    ensures rewrite_stmt(Stmt::If {
        cond: const_true(),
        then_b: seq![print_hi()],
        has_elsifs: true,
        els_b: seq![print_hi()],
    }) == (Stmt::If {
        cond: const_true(),
        then_b: seq![print_hi()],
        has_elsifs: true,
        els_b: seq![print_hi()],
    }),
{
    assert(rewrite_list(seq![print_hi()]) == seq![print_hi()]) by {
        reveal_with_fuel(rewrite_list, 4);
    };
    reveal_with_fuel(rewrite_stmt, 4);
}

// Behavior: `until false; do print` at fuel 2 prints twice with the
// status writes silent — original and rewritten agree.
proof fn pin_until_false_runs_twice(s: int)
    requires s == 0,
    ensures exec_list(
        rewrite_list(seq![Stmt::While { cond: const_false(), body: seq![print_hi()] }]),
        s,
        2,
    ) == exec_list(seq![Stmt::While { cond: const_false(), body: seq![print_hi()] }], s, 2),
{
    exec_list_sound(seq![Stmt::While { cond: const_false(), body: seq![print_hi()] }], s, 2);
}

fn main() {}

}
