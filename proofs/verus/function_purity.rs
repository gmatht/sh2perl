// Standalone model of function-purity's verdict soundness
// (sh2perl/src/transforms/function_purity.rs): the structural purity
// predicates and the call-graph fixpoint over function bodies. Proves
// the soundness obligation:
//
//   purity_sound: every function the verdict calls Pure emits no
//   events at any fuel, and every function it calls Emulable emits
//   only capturable (Print) events at any fuel.
//
// Names are interned to nat (as in the sibling proofs). Purity is the
// meet over a three-point lattice (Impure=0 < Emulable=1 < Pure=2) of
// the per-statement verdicts, where a call's purity is the callee's
// verdict (unknown callee = Impure). The real fixpoint starts
// all-Impure and iterates to a least fixpoint; the model proves
// soundness for ANY consistent table (recomputation never drops below
// the verdict), which covers the real iteration's limit.
//
// What is modelled and what is NOT (fidelity, checked against
// function_purity.rs):
//   - FnDef/Assign/Decl/Expr of pure expressions (Pure), Output of a
//     pure value (Emulable — bytes are capturable, not absent),
//     If/Loop over pure conds with purity-meet bodies, calls (callee
//     verdict meet arg purities), and the Impure default for
//     Other (Exec/Pipeline/Capture/Background/...).
//   - The expression predicate (Lit/Var/Add pure; Call = callee meet
//     args; Capture Impure) — structurally identical to the real
//     expr_purity.
//   - The fixpoint iteration itself is NOT modelled (no fuel-indexed
//     table sequence); the theorem takes a consistent table as a
//     parameter. The real loop's limit satisfies the consistency
//     hypothesis by construction. Monotonicity from the all-Impure
//     start is out of scope — soundness needs only the limit property.
//   - Flat function bodies (statement sequences); nested statement
//     recursion is the same meet presented per straight-line list.
//   - The inline_pure_fns consumer is out of scope (that inlining step
//     is Tier-3-flavored: call preservation, not verdict-shaped).
use vstd::prelude::*;

verus! {

pub type Name = nat;

// Purity lattice: 0 = Impure, 1 = Emulable, 2 = Pure. The meet is min.
pub type Purity = nat;

pub open spec fn pl_min(a: Purity, b: Purity) -> Purity {
    if a <= b {
        a
    } else {
        b
    }
}

pub open spec fn pl_min3(a: Purity, b: Purity, c: Purity) -> Purity {
    pl_min(pl_min(a, b), c)
}

// ---- Expressions ----

pub enum Expr {
    Lit(int),
    Var(Name),
    Add(Box<Expr>, Box<Expr>),
    Call { f: Name, args: Seq<Expr> },
    Capture { inner: Box<Expr> },
}

pub enum Ev {
    Print(int),
    RunCall(Name),
    Spawn,
}

// A capturable event is a Print (bytes the capture form can compute
// in-process); anything else escapes the capture.
pub open spec fn ev_capturable(e: Ev) -> bool {
    match e {
        Ev::Print(_) => true,
        _ => false,
    }
}

pub open spec fn trace_capturable(t: Seq<Ev>) -> bool {
    forall|i: int| 0 <= i < t.len() ==> ev_capturable(t[i])
}

pub open spec fn expr_pure(e: Expr, v: Map<Name, Purity>) -> Purity
    decreases e,
{
    match e {
        Expr::Lit(_) => 2,
        Expr::Var(_) => 2,
        Expr::Add(l, r) => pl_min(expr_pure(*l, v), expr_pure(*r, v)),
        Expr::Call { f, args } => {
            let fv: Purity = if v.dom().contains(f) {
                v[f]
            } else {
                0
            };
            pl_min(fv, args_pure(args, v))
        },
        Expr::Capture { .. } => 0,
    }
}

pub open spec fn args_pure(a: Seq<Expr>, v: Map<Name, Purity>) -> Purity
    decreases a,
{
    if a.len() == 0 {
        2
    } else {
        pl_min(expr_pure(a[0], v), args_pure(a.drop_first(), v))
    }
}

// ---- Statements ----

pub enum Stmt {
    FnDef { name: Name, body: Seq<Stmt> },
    Assign { target: Name, rhs: Expr },
    Decl { init: Seq<Expr> },
    Output { val: Expr },
    If { cond: Expr, then: Seq<Stmt>, els: Seq<Stmt> },
    Loop { cond: Expr, body: Seq<Stmt> },
    Other,
}

pub open spec fn stmt_kind(st: Stmt) -> nat {
    match st {
        Stmt::FnDef { .. } => 6,
        Stmt::Assign { .. } => 0,
        Stmt::Decl { .. } => 1,
        Stmt::Output { .. } => 2,
        Stmt::If { .. } => 3,
        Stmt::Loop { .. } => 4,
        Stmt::Other => 5,
    }
}

pub open spec fn fndef_name(st: Stmt) -> Name {
    match st {
        Stmt::FnDef { name, .. } => name,
        _ => 0,
    }
}

pub open spec fn fndef_body(st: Stmt) -> Seq<Stmt> {
    match st {
        Stmt::FnDef { body, .. } => body,
        _ => Seq::empty(),
    }
}

pub open spec fn assign_rhs(st: Stmt) -> Expr {
    match st {
        Stmt::Assign { rhs, .. } => rhs,
        _ => Expr::Lit(0),
    }
}

pub open spec fn decl_init(st: Stmt) -> Seq<Expr> {
    match st {
        Stmt::Decl { init, .. } => init,
        _ => Seq::empty(),
    }
}

pub open spec fn output_val(st: Stmt) -> Expr {
    match st {
        Stmt::Output { val } => val,
        _ => Expr::Lit(0),
    }
}

pub open spec fn if_cond(st: Stmt) -> Expr {
    match st {
        Stmt::If { cond, .. } => cond,
        Stmt::Loop { cond, .. } => cond,
        _ => Expr::Lit(0),
    }
}

pub open spec fn if_then(st: Stmt) -> Seq<Stmt> {
    match st {
        Stmt::If { then, .. } => then,
        Stmt::Loop { body, .. } => body,
        _ => Seq::empty(),
    }
}

pub open spec fn if_els(st: Stmt) -> Seq<Stmt> {
    match st {
        Stmt::If { els, .. } => els,
        _ => Seq::empty(),
    }
}

// The meet of a statement sequence (empty = Pure).
pub open spec fn seq_purity(s: Seq<Stmt>, v: Map<Name, Purity>) -> Purity
    decreases s,
{
    if s.len() == 0 {
        2
    } else {
        pl_min(stmt_purity(s[0], v), seq_purity(s.drop_first(), v))
    }
}

pub open spec fn exprs_pure(a: Seq<Expr>, v: Map<Name, Purity>) -> Purity
    decreases a.len(),
{
    if a.len() == 0 {
        2
    } else {
        pl_min(expr_pure(a[0], v), exprs_pure(a.drop_first(), v))
    }
}

pub open spec fn stmt_purity(st: Stmt, v: Map<Name, Purity>) -> Purity
    decreases st,
{
    if stmt_kind(st) == 6 {
        // A definition contributes no purity of its own at top level
        // (it does not execute); the verdict covers the body.
        2
    } else if stmt_kind(st) == 0 {
        expr_pure(assign_rhs(st), v)
    } else if stmt_kind(st) == 1 {
        exprs_pure(decl_init(st), v)
    } else if stmt_kind(st) == 2 {
        // Output of a pure value is Emulable (capturable bytes), else
        // Impure (a side-effecting value computation).
        if expr_pure(output_val(st), v) == 0 {
            0
        } else {
            1
        }
    } else if stmt_kind(st) == 3 {
        pl_min3(expr_pure(if_cond(st), v), seq_purity(if_then(st), v), seq_purity(if_els(st), v))
    } else if stmt_kind(st) == 4 {
        pl_min(expr_pure(if_cond(st), v), seq_purity(if_then(st), v))
    } else {
        0
    }
}

// Structural measures so stmt_purity/seq_purity terminate mutually.
pub open spec fn stmt_measure(st: Stmt) -> nat
    decreases st,
{
    match st {
        Stmt::FnDef { body, .. } => 2 + seq_measure(body),
        Stmt::If { then, els, .. } => 2 + seq_measure(then) + seq_measure(els),
        Stmt::Loop { body, .. } => 2 + seq_measure(body),
        _ => 1,
    }
}

pub open spec fn seq_measure(s: Seq<Stmt>) -> nat
    decreases s,
{
    if s.len() == 0 {
        0
    } else {
        stmt_measure(s[0]) + seq_measure(s.drop_first()) + 1
    }
}

// ---- Lookup and consistency ----

// First-def-wins lookup of a function body (either rule is preserved
// by the verdict argument; both sides use the same one).
pub open spec fn find_body(p: Seq<Stmt>, f: Name) -> Seq<Stmt>
    decreases p.len(),
{
    if p.len() == 0 {
        Seq::empty()
    } else if stmt_kind(p[0]) == 6 && fndef_name(p[0]) == f {
        fndef_body(p[0])
    } else {
        find_body(p.drop_first(), f)
    }
}

pub open spec fn is_defined(p: Seq<Stmt>, f: Name) -> bool {
    exists|i: int| 0 <= i < p.len() && stmt_kind(p[i]) == 6 && fndef_name(p[i]) == f
}

// A verdict table is consistent when every defined function meets its
// recomputed body purity (the real fixpoint's limit property: one
// more iteration changes nothing observable at or below the verdict),
// and every verdict lands in the lattice (the real iteration only
// emits 0/1/2 — without this, a table claiming v[f] = 5 for an
// UNDEFINED f would make `Call(f, [])` compute Pure yet dispatch a
// RunCall: the model pins this as unsound, faithful to the real
// unknown-callee-Impure default).
pub open spec fn table_consistent(p: Seq<Stmt>, v: Map<Name, Purity>) -> bool {
    (forall|f: Name| is_defined(p, f) ==> seq_purity(find_body(p, f), v) >= v[f]) && (forall|f: Name|
        v.dom().contains(f) ==> v[f] <= 2) && (forall|f: Name|
        v.dom().contains(f) ==> is_defined(p, f))
}

// ---- Execution: event traces at fuel bounds ----

pub open spec fn eval_events(p: Seq<Stmt>, e: Expr, v: Map<Name, Purity>, fuel: int) -> Seq<Ev>
    decreases fuel,
{
    if fuel <= 0 {
        Seq::empty()
    } else {
        match e {
            Expr::Lit(_) => Seq::empty(),
            Expr::Var(_) => Seq::empty(),
            Expr::Add(l, r) => {
                eval_events(p, *l, v, fuel - 1).add(eval_events(p, *r, v, fuel - 1))
            },
            Expr::Call { f, args } => {
                eval_args_events(p, args, v, fuel - 1).add(dispatch_events(p, f, v, fuel - 1))
            },
            Expr::Capture { inner } => {
                seq![Ev::Spawn].add(eval_events(p, *inner, v, fuel - 1))
            },
        }
    }
}

pub open spec fn eval_args_events(
    p: Seq<Stmt>,
    a: Seq<Expr>,
    v: Map<Name, Purity>,
    fuel: int,
) -> Seq<Ev>
    decreases fuel,
{
    if fuel <= 0 {
        Seq::empty()
    } else if a.len() == 0 {
        Seq::empty()
    } else {
        eval_events(p, a[0], v, fuel - 1).add(eval_args_events(p, a.drop_first(), v, fuel - 1))
    }
}

// Dispatch: a defined body runs; an undefined callee emits RunCall
// and stops (the shell's external-exec fallback).
pub open spec fn dispatch_events(
    p: Seq<Stmt>,
    f: Name,
    v: Map<Name, Purity>,
    fuel: int,
) -> Seq<Ev>
    decreases fuel,
{
    if fuel <= 0 {
        Seq::empty()
    } else if is_defined(p, f) {
        run_events(p, find_body(p, f), v, fuel - 1)
    } else {
        seq![Ev::RunCall(f)]
    }
}

pub open spec fn run_events(p: Seq<Stmt>, s: Seq<Stmt>, v: Map<Name, Purity>, fuel: int) -> Seq<Ev>
    decreases fuel,
{
    if fuel <= 0 {
        Seq::empty()
    } else if s.len() == 0 {
        Seq::empty()
    } else {
        stmt_events(p, s[0], v, fuel - 1).add(run_events(p, s.drop_first(), v, fuel - 1))
    }
}

pub open spec fn exprs_events(
    p: Seq<Stmt>,
    a: Seq<Expr>,
    v: Map<Name, Purity>,
    fuel: int,
) -> Seq<Ev>
    decreases fuel,
{
    if fuel <= 0 {
        Seq::empty()
    } else if a.len() == 0 {
        Seq::empty()
    } else {
        eval_events(p, a[0], v, fuel - 1).add(exprs_events(p, a.drop_first(), v, fuel - 1))
    }
}

pub open spec fn stmt_events(p: Seq<Stmt>, st: Stmt, v: Map<Name, Purity>, fuel: int) -> Seq<Ev>
    decreases fuel,
{
    if fuel <= 0 {
        Seq::empty()
    } else if stmt_kind(st) == 6 {
        // Definitions do not execute at top level.
        Seq::empty()
    } else if stmt_kind(st) == 0 {
        eval_events(p, assign_rhs(st), v, fuel - 1)
    } else if stmt_kind(st) == 1 {
        exprs_events(p, decl_init(st), v, fuel - 1)
    } else if stmt_kind(st) == 2 {
        eval_events(p, output_val(st), v, fuel - 1).add(seq![Ev::Print(0)])
    } else if stmt_kind(st) == 3 {
        eval_events(p, if_cond(st), v, fuel - 1).add(
            run_events(p, if_then(st), v, fuel - 1).add(run_events(p, if_els(st), v, fuel - 1)),
        )
    } else if stmt_kind(st) == 4 {
        eval_events(p, if_cond(st), v, fuel - 1).add(run_events(p, if_then(st), v, fuel - 1))
    } else {
        seq![Ev::RunCall(0)]
    }
}

// ---- Soundness lemmas ----

// Every purity computation lands in the lattice (0..2).
proof fn expr_bounded(e: Expr, v: Map<Name, Purity>)
    ensures expr_pure(e, v) <= 2,
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(_) => {},
        Expr::Add(l, r) => {
            expr_bounded(*l, v);
            expr_bounded(*r, v);
        },
        Expr::Call { args, .. } => {
            args_bounded(args, v);
        },
        Expr::Capture { .. } => {},
    }
}

proof fn args_bounded(a: Seq<Expr>, v: Map<Name, Purity>)
    ensures args_pure(a, v) <= 2,
    decreases a,
{
    if a.len() == 0 {
    } else {
        expr_bounded(a[0], v);
        args_bounded(a.drop_first(), v);
    }
}

proof fn exprs_bounded(a: Seq<Expr>, v: Map<Name, Purity>)
    ensures exprs_pure(a, v) <= 2,
    decreases a.len(),
{
    if a.len() == 0 {
    } else {
        expr_bounded(a[0], v);
        exprs_bounded(a.drop_first(), v);
    }
}

proof fn stmt_bounded(st: Stmt, v: Map<Name, Purity>)
    ensures stmt_purity(st, v) <= 2,
    decreases st,
{
    if stmt_kind(st) == 0 {
        expr_bounded(assign_rhs(st), v);
    } else if stmt_kind(st) == 1 {
        exprs_bounded(decl_init(st), v);
    } else if stmt_kind(st) == 2 {
        expr_bounded(output_val(st), v);
    } else if stmt_kind(st) == 3 {
        expr_bounded(if_cond(st), v);
        seq_bounded(if_then(st), v);
        seq_bounded(if_els(st), v);
    } else if stmt_kind(st) == 4 {
        expr_bounded(if_cond(st), v);
        seq_bounded(if_then(st), v);
    } else {
    }
}

proof fn seq_bounded(s: Seq<Stmt>, v: Map<Name, Purity>)
    ensures seq_purity(s, v) <= 2,
    decreases s,
{
    if s.len() == 0 {
    } else {
        stmt_bounded(s[0], v);
        seq_bounded(s.drop_first(), v);
    }
}

// Min inversion: the meet is Pure iff both sides are Pure, and
// Emulable-or-better iff both sides are.
proof fn pl_min_eq2(a: Purity, b: Purity)
    requires pl_min(a, b) == 2, a <= 2, b <= 2,
    ensures a == 2 && b == 2,
{
    if a <= b {
        assert(pl_min(a, b) == a);
    } else {
        assert(pl_min(a, b) == b);
    }
}

proof fn pl_min_ge1(a: Purity, b: Purity)
    requires pl_min(a, b) >= 1, a <= 2, b <= 2,
    ensures a >= 1 && b >= 1,
{
    if a <= b {
        assert(pl_min(a, b) == a);
    } else {
        assert(pl_min(a, b) == b);
    }
}

proof fn pl_min3_eq2(a: Purity, b: Purity, c: Purity)
    requires pl_min3(a, b, c) == 2, a <= 2, b <= 2, c <= 2,
    ensures a == 2 && b == 2 && c == 2,
{
    assert(pl_min(a, b) <= 2);
    pl_min_eq2(pl_min(a, b), c);
    pl_min_eq2(a, b);
}

proof fn pl_min3_ge1(a: Purity, b: Purity, c: Purity)
    requires pl_min3(a, b, c) >= 1, a <= 2, b <= 2, c <= 2,
    ensures a >= 1 && b >= 1 && c >= 1,
{
    assert(pl_min(a, b) <= 2);
    pl_min_ge1(pl_min(a, b), c);
    pl_min_ge1(a, b);
}

// Trace append preserves emptiness and capturability.
proof fn trace_add_empty(a: Seq<Ev>)
    ensures a.add(Seq::<Ev>::empty()) == a,
{
    assert(a.add(Seq::<Ev>::empty()) == a);
}

proof fn trace_empty_add(a: Seq<Ev>)
    ensures Seq::<Ev>::empty().add(a) == a,
{
    assert(Seq::<Ev>::empty().add(a) == a);
}

proof fn trace_capturable_add(a: Seq<Ev>, b: Seq<Ev>)
    requires trace_capturable(a), trace_capturable(b),
    ensures trace_capturable(a.add(b)),
{
    assert forall|i: int| 0 <= i < a.add(b).len() implies ev_capturable(a.add(b)[i]) by {
        if i < a.len() {
        } else {
        }
    };
}

// Pure expressions emit nothing; Emulable-or-better expressions emit
// only capturable events. Mutual induction over (expr, args) with the
// program's verdict table fixed.
proof fn eval_sound(p: Seq<Stmt>, e: Expr, v: Map<Name, Purity>, fuel: int)
    requires table_consistent(p, v),
    ensures
        expr_pure(e, v) == 2 ==> eval_events(p, e, v, fuel) == Seq::<Ev>::empty(),
        expr_pure(e, v) >= 1 ==> trace_capturable(eval_events(p, e, v, fuel)),
    decreases fuel,
{
    if fuel <= 0 {
    } else {
        match e {
            Expr::Lit(_) => {},
            Expr::Var(_) => {},
            Expr::Add(l, r) => {
                eval_sound(p, *l, v, fuel - 1);
                eval_sound(p, *r, v, fuel - 1);
                assert(eval_events(p, e, v, fuel) == eval_events(p, *l, v, fuel - 1).add(
                    eval_events(p, *r, v, fuel - 1),
                ));
                if expr_pure(e, v) == 2 {
                    expr_bounded(*l, v);
                    expr_bounded(*r, v);
                    pl_min_eq2(expr_pure(*l, v), expr_pure(*r, v));
                    assert(eval_events(p, *l, v, fuel - 1) == Seq::<Ev>::empty());
                    assert(eval_events(p, *r, v, fuel - 1) == Seq::<Ev>::empty());
                    trace_empty_add(eval_events(p, *r, v, fuel - 1));
                }
                if expr_pure(e, v) >= 1 {
                    expr_bounded(*l, v);
                    expr_bounded(*r, v);
                    pl_min_ge1(expr_pure(*l, v), expr_pure(*r, v));
                    trace_capturable_add(
                        eval_events(p, *l, v, fuel - 1),
                        eval_events(p, *r, v, fuel - 1),
                    );
                }
            },
            Expr::Call { f, args } => {
                eval_args_sound(p, args, v, fuel - 1);
                dispatch_sound(p, f, v, fuel - 1);
                assert(eval_events(p, e, v, fuel) == eval_args_events(p, args, v, fuel - 1).add(
                    dispatch_events(p, f, v, fuel - 1),
                ));
                let fv: Purity = if v.dom().contains(f) {
                    v[f]
                } else {
                    0
                };
                if expr_pure(e, v) == 2 {
                    args_bounded(args, v);
                    assert(fv <= 2) by {
                        assert(table_consistent(p, v));
                    };
                    pl_min_eq2(fv, args_pure(args, v));
                    assert(v.dom().contains(f) && v[f] == 2);
                    assert(eval_args_events(p, args, v, fuel - 1) == Seq::<Ev>::empty());
                    assert(dispatch_events(p, f, v, fuel - 1) == Seq::<Ev>::empty());
                    trace_empty_add(dispatch_events(p, f, v, fuel - 1));
                }
                if expr_pure(e, v) >= 1 {
                    args_bounded(args, v);
                    assert(fv <= 2) by {
                        assert(table_consistent(p, v));
                    };
                    pl_min_ge1(fv, args_pure(args, v));
                    assert(v.dom().contains(f));
                    trace_capturable_add(
                        eval_args_events(p, args, v, fuel - 1),
                        dispatch_events(p, f, v, fuel - 1),
                    );
                }
            },
            Expr::Capture { .. } => {},
        }
    }
}

proof fn eval_args_sound(p: Seq<Stmt>, a: Seq<Expr>, v: Map<Name, Purity>, fuel: int)
    requires table_consistent(p, v),
    ensures
        args_pure(a, v) == 2 ==> eval_args_events(p, a, v, fuel) == Seq::<Ev>::empty(),
        args_pure(a, v) >= 1 ==> trace_capturable(eval_args_events(p, a, v, fuel)),
    decreases fuel,
{
    if fuel <= 0 {
    } else if a.len() == 0 {
    } else {
        eval_sound(p, a[0], v, fuel - 1);
        eval_args_sound(p, a.drop_first(), v, fuel - 1);
        assert(eval_args_events(p, a, v, fuel) == eval_events(p, a[0], v, fuel - 1).add(
            eval_args_events(p, a.drop_first(), v, fuel - 1),
        ));
        if args_pure(a, v) == 2 {
            expr_bounded(a[0], v);
            args_bounded(a.drop_first(), v);
            pl_min_eq2(expr_pure(a[0], v), args_pure(a.drop_first(), v));
            assert(eval_events(p, a[0], v, fuel - 1) == Seq::<Ev>::empty());
            assert(eval_args_events(p, a.drop_first(), v, fuel - 1) == Seq::<Ev>::empty());
            trace_empty_add(eval_args_events(p, a.drop_first(), v, fuel - 1));
        }
        if args_pure(a, v) >= 1 {
            expr_bounded(a[0], v);
            args_bounded(a.drop_first(), v);
            pl_min_ge1(expr_pure(a[0], v), args_pure(a.drop_first(), v));
            trace_capturable_add(
                eval_events(p, a[0], v, fuel - 1),
                eval_args_events(p, a.drop_first(), v, fuel - 1),
            );
        }
    }
}

// Dispatch on a Pure-callee verdict is silent; on an Emulable verdict
// is capturable. Consistency lifts the verdict to the recomputed body
// purity, and the induction hypothesis (on smaller fuel) closes it.
proof fn dispatch_sound(p: Seq<Stmt>, f: Name, v: Map<Name, Purity>, fuel: int)
    requires table_consistent(p, v),
    ensures
        v.dom().contains(f) && v[f] == 2 ==> dispatch_events(p, f, v, fuel) == Seq::<Ev>::empty(),
        v.dom().contains(f) && v[f] >= 1 ==> trace_capturable(dispatch_events(p, f, v, fuel)),
    decreases fuel,
{
    if fuel <= 0 {
    } else if is_defined(p, f) {
        if v.dom().contains(f) {
            // Defined-callee case: the body facts as explicit steps
            // (implication chaining through map-indexed terms is
            // fragile, so each link is its own assert).
            let body = find_body(p, f);
            assert(seq_purity(body, v) >= v[f]) by {
                assert(table_consistent(p, v));
            };
            assert(dispatch_events(p, f, v, fuel) == run_events(p, body, v, fuel - 1));
            if v[f] == 2 {
                assert(seq_purity(body, v) == 2) by {
                    seq_bounded(body, v);
                };
                run_sound(p, body, v, fuel - 1);
                assert(run_events(p, body, v, fuel - 1) == Seq::<Ev>::empty());
            }
            if v[f] >= 1 {
                assert(seq_purity(body, v) >= 1);
                run_sound(p, body, v, fuel - 1);
                assert(trace_capturable(run_events(p, body, v, fuel - 1)));
            }
        }
        // Undefined-verdict case: v[f] defaults are handled by the
        // caller (fv = 0 when absent); no obligation here.
    } else {
        // Undefined callee with a table verdict is impossible:
        // consistency keys are defined names only. Either implication's
        // antecedent (contains) contradicts !defined.
        assert(!v.dom().contains(f)) by {
            assert(table_consistent(p, v));
        };
    }
}

proof fn run_sound(p: Seq<Stmt>, s: Seq<Stmt>, v: Map<Name, Purity>, fuel: int)
    requires table_consistent(p, v),
    ensures
        seq_purity(s, v) == 2 ==> run_events(p, s, v, fuel) == Seq::<Ev>::empty(),
        seq_purity(s, v) >= 1 ==> trace_capturable(run_events(p, s, v, fuel)),
    decreases fuel,
{
    if fuel <= 0 {
    } else if s.len() == 0 {
    } else {
        stmt_sound(p, s[0], v, fuel - 1);
        run_sound(p, s.drop_first(), v, fuel - 1);
        assert(run_events(p, s, v, fuel) == stmt_events(p, s[0], v, fuel - 1).add(
            run_events(p, s.drop_first(), v, fuel - 1),
        ));
        if seq_purity(s, v) == 2 {
            stmt_bounded(s[0], v);
            seq_bounded(s.drop_first(), v);
            pl_min_eq2(stmt_purity(s[0], v), seq_purity(s.drop_first(), v));
            assert(stmt_events(p, s[0], v, fuel - 1) == Seq::<Ev>::empty());
            assert(run_events(p, s.drop_first(), v, fuel - 1) == Seq::<Ev>::empty());
            trace_empty_add(run_events(p, s.drop_first(), v, fuel - 1));
        }
        if seq_purity(s, v) >= 1 {
            stmt_bounded(s[0], v);
            seq_bounded(s.drop_first(), v);
            pl_min_ge1(stmt_purity(s[0], v), seq_purity(s.drop_first(), v));
            trace_capturable_add(
                stmt_events(p, s[0], v, fuel - 1),
                run_events(p, s.drop_first(), v, fuel - 1),
            );
        }
    }
}

proof fn exprs_sound(p: Seq<Stmt>, a: Seq<Expr>, v: Map<Name, Purity>, fuel: int)
    requires table_consistent(p, v),
    ensures
        exprs_pure(a, v) == 2 ==> exprs_events(p, a, v, fuel) == Seq::<Ev>::empty(),
        exprs_pure(a, v) >= 1 ==> trace_capturable(exprs_events(p, a, v, fuel)),
    decreases fuel,
{
    if fuel <= 0 {
    } else if a.len() == 0 {
    } else {
        eval_sound(p, a[0], v, fuel - 1);
        exprs_sound(p, a.drop_first(), v, fuel - 1);
        assert(exprs_events(p, a, v, fuel) == eval_events(p, a[0], v, fuel - 1).add(
            exprs_events(p, a.drop_first(), v, fuel - 1),
        ));
        if exprs_pure(a, v) == 2 {
            expr_bounded(a[0], v);
            exprs_bounded(a.drop_first(), v);
            pl_min_eq2(expr_pure(a[0], v), exprs_pure(a.drop_first(), v));
            assert(eval_events(p, a[0], v, fuel - 1) == Seq::<Ev>::empty());
            assert(exprs_events(p, a.drop_first(), v, fuel - 1) == Seq::<Ev>::empty());
            trace_empty_add(exprs_events(p, a.drop_first(), v, fuel - 1));
        }
        if exprs_pure(a, v) >= 1 {
            expr_bounded(a[0], v);
            exprs_bounded(a.drop_first(), v);
            pl_min_ge1(expr_pure(a[0], v), exprs_pure(a.drop_first(), v));
            trace_capturable_add(
                eval_events(p, a[0], v, fuel - 1),
                exprs_events(p, a.drop_first(), v, fuel - 1),
            );
        }
    }
}

proof fn stmt_sound(p: Seq<Stmt>, st: Stmt, v: Map<Name, Purity>, fuel: int)
    requires table_consistent(p, v),
    ensures
        stmt_purity(st, v) == 2 ==> stmt_events(p, st, v, fuel) == Seq::<Ev>::empty(),
        stmt_purity(st, v) >= 1 ==> trace_capturable(stmt_events(p, st, v, fuel)),
    decreases fuel,
{
    if fuel <= 0 {
    } else if stmt_kind(st) == 6 {
    } else if stmt_kind(st) == 0 {
        eval_sound(p, assign_rhs(st), v, fuel - 1);
        assert(stmt_events(p, st, v, fuel) == eval_events(p, assign_rhs(st), v, fuel - 1));
    } else if stmt_kind(st) == 1 {
        exprs_sound(p, decl_init(st), v, fuel - 1);
        assert(stmt_events(p, st, v, fuel) == exprs_events(p, decl_init(st), v, fuel - 1));
    } else if stmt_kind(st) == 2 {
        eval_sound(p, output_val(st), v, fuel - 1);
        assert(stmt_events(p, st, v, fuel) == eval_events(p, output_val(st), v, fuel - 1).add(
            seq![Ev::Print(0)],
        ));
        if stmt_purity(st, v) == 1 {
            assert(expr_pure(output_val(st), v) >= 1);
            trace_capturable_add(eval_events(p, output_val(st), v, fuel - 1), seq![Ev::Print(0)]);
        }
    } else if stmt_kind(st) == 3 {
        eval_sound(p, if_cond(st), v, fuel - 1);
        run_sound(p, if_then(st), v, fuel - 1);
        run_sound(p, if_els(st), v, fuel - 1);
        assert(stmt_events(p, st, v, fuel) == eval_events(p, if_cond(st), v, fuel - 1).add(
            run_events(p, if_then(st), v, fuel - 1).add(run_events(p, if_els(st), v, fuel - 1)),
        ));
        if stmt_purity(st, v) == 2 {
            expr_bounded(if_cond(st), v);
            seq_bounded(if_then(st), v);
            seq_bounded(if_els(st), v);
            pl_min3_eq2(expr_pure(if_cond(st), v), seq_purity(if_then(st), v), seq_purity(if_els(st), v));
            assert(eval_events(p, if_cond(st), v, fuel - 1) == Seq::<Ev>::empty());
            assert(run_events(p, if_then(st), v, fuel - 1) == Seq::<Ev>::empty());
            assert(run_events(p, if_els(st), v, fuel - 1) == Seq::<Ev>::empty());
            trace_empty_add(
                run_events(p, if_then(st), v, fuel - 1).add(
                    run_events(p, if_els(st), v, fuel - 1),
                ),
            );
            trace_empty_add(run_events(p, if_els(st), v, fuel - 1));
        }
        if stmt_purity(st, v) >= 1 {
            expr_bounded(if_cond(st), v);
            seq_bounded(if_then(st), v);
            seq_bounded(if_els(st), v);
            pl_min3_ge1(expr_pure(if_cond(st), v), seq_purity(if_then(st), v), seq_purity(if_els(st), v));
            trace_capturable_add(
                run_events(p, if_then(st), v, fuel - 1),
                run_events(p, if_els(st), v, fuel - 1),
            );
            trace_capturable_add(
                eval_events(p, if_cond(st), v, fuel - 1),
                run_events(p, if_then(st), v, fuel - 1).add(
                    run_events(p, if_els(st), v, fuel - 1),
                ),
            );
        }
    } else if stmt_kind(st) == 4 {
        eval_sound(p, if_cond(st), v, fuel - 1);
        run_sound(p, if_then(st), v, fuel - 1);
        assert(stmt_events(p, st, v, fuel) == eval_events(p, if_cond(st), v, fuel - 1).add(
            run_events(p, if_then(st), v, fuel - 1),
        ));
        if stmt_purity(st, v) == 2 {
            expr_bounded(if_cond(st), v);
            seq_bounded(if_then(st), v);
            pl_min_eq2(expr_pure(if_cond(st), v), seq_purity(if_then(st), v));
            assert(eval_events(p, if_cond(st), v, fuel - 1) == Seq::<Ev>::empty());
            assert(run_events(p, if_then(st), v, fuel - 1) == Seq::<Ev>::empty());
            trace_empty_add(run_events(p, if_then(st), v, fuel - 1));
        }
        if stmt_purity(st, v) >= 1 {
            expr_bounded(if_cond(st), v);
            seq_bounded(if_then(st), v);
            pl_min_ge1(expr_pure(if_cond(st), v), seq_purity(if_then(st), v));
            trace_capturable_add(
                eval_events(p, if_cond(st), v, fuel - 1),
                run_events(p, if_then(st), v, fuel - 1),
            );
        }
    } else {
    }
}

// THE OBLIGATION: under a consistent table, a Pure verdict means an
// empty trace at every fuel, and an Emulable verdict means a
// capturable-only trace at every fuel.
proof fn purity_sound(p: Seq<Stmt>, v: Map<Name, Purity>, f: Name, fuel: int)
    requires table_consistent(p, v), is_defined(p, f),
    ensures
        v.dom().contains(f) && v[f] == 2 ==> run_events(p, find_body(p, f), v, fuel) == Seq::<Ev>::empty(),
        v.dom().contains(f) && v[f] >= 1 ==> trace_capturable(
            run_events(p, find_body(p, f), v, fuel),
        ),
{
    assert(table_consistent(p, v));
    if v.dom().contains(f) {
        assert(seq_purity(find_body(p, f), v) >= v[f]);
        seq_bounded(find_body(p, f), v);
    }
    run_sound(p, find_body(p, f), v, fuel);
}

// ---- Fidelity pins (mirror function_purity.rs's verdict behavior) ----

// `f() { x=5; }` — a pure-assign body recomputes to Pure.
proof fn pin_pure_assign_body()
    ensures seq_purity(
        seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }],
        Map::<Name, Purity>::empty(),
    ) == 2,
{
    assert(seq_purity(seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }], Map::empty()) == pl_min(
        stmt_purity(Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }, Map::empty()),
        seq_purity(Seq::empty(), Map::empty()),
    )) by {
        reveal_with_fuel(seq_purity, 2);
    };
}

// `f() { echo hi; }` — an Output body recomputes to Emulable (1).
proof fn pin_output_body_emulable()
    ensures seq_purity(
        seq![Stmt::Output { val: Expr::Lit(1) }],
        Map::<Name, Purity>::empty(),
    ) == 1,
{
    assert(seq_purity(seq![Stmt::Output { val: Expr::Lit(1) }], Map::empty()) == pl_min(
        stmt_purity(Stmt::Output { val: Expr::Lit(1) }, Map::empty()),
        seq_purity(Seq::empty(), Map::empty()),
    )) by {
        reveal_with_fuel(seq_purity, 2);
    };
}

// `f() { exec foo; }` — an Other body recomputes to Impure (0).
proof fn pin_other_body_impure()
    ensures seq_purity(seq![Stmt::Other], Map::<Name, Purity>::empty()) == 0,
{
    assert(seq_purity(seq![Stmt::Other], Map::empty()) == pl_min(
        stmt_purity(Stmt::Other, Map::empty()),
        seq_purity(Seq::empty(), Map::empty()),
    )) by {
        reveal_with_fuel(seq_purity, 2);
    };
}

// A call to an UNKNOWN callee is Impure — the conservative default
// the real `callees.get(func).unwrap_or(Impure)` takes.
proof fn pin_unknown_callee_impure()
    ensures expr_pure(
        Expr::Call { f: 9nat, args: Seq::empty() },
        Map::<Name, Purity>::empty(),
    ) == 0,
{
}

// A call to a Pure-callee with pure args is Pure.
proof fn pin_pure_call_pure()
    ensures expr_pure(
        Expr::Call { f: 3nat, args: seq![Expr::Lit(1)] },
        Map::empty().insert(3nat, 2),
    ) == 2,
{
    assert(args_pure(seq![Expr::Lit(1)], Map::empty().insert(3nat, 2)) == 2) by {
        reveal_with_fuel(args_pure, 2);
    };
}

// A call to a Pure-callee with an IMPURE (Capture) arg is Impure —
// the meet takes the minimum.
proof fn pin_impure_arg_taints()
    ensures expr_pure(
        Expr::Call { f: 3nat, args: seq![Expr::Capture { inner: Box::new(Expr::Lit(1)) }] },
        Map::empty().insert(3nat, 2),
    ) == 0,
{
    assert(args_pure(seq![Expr::Capture { inner: Box::new(Expr::Lit(1)) }], Map::empty().insert(3nat, 2)) == 0) by {
        reveal_with_fuel(args_pure, 2);
    };
}

// A consistent Pure table over `f() { x=5; }` runs silent at fuel 2.
proof fn pin_pure_runs_silent()
    ensures run_events(
        seq![Stmt::FnDef { name: 3nat, body: seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }] }],
        seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }],
        Map::empty().insert(3nat, 2),
        2,
    ) == Seq::<Ev>::empty(),
{
    let p = seq![Stmt::FnDef { name: 3nat, body: seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }] }];
    let b = seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }];
    let v = Map::empty().insert(3nat, 2);
    assert(is_defined(p, 3nat)) by {
        assert(stmt_kind(p[0]) == 6 && fndef_name(p[0]) == 3nat);
    };
    assert(table_consistent(p, v)) by {
        assert forall|f: Name| is_defined(p, f) implies seq_purity(find_body(p, f), v) >= v[f] by {
            if f == 3nat {
                assert(find_body(p, f) == b) by {
                    reveal_with_fuel(find_body, 2);
                };
                // seq_purity([Assign(7, Lit 5)]) = min(2, 2) = 2 >= v[3] = 2.
                assert(seq_purity(b, v) == 2) by {
                    reveal_with_fuel(seq_purity, 3);
                    reveal_with_fuel(stmt_purity, 3);
                };
            } else {
                assert(!is_defined(p, f)) by {
                    assert forall|i: int| 0 <= i < p.len() implies !(stmt_kind(p[i]) == 6 && fndef_name(p[i]) == f) by {
                        assert(p[i] == p[0]);
                    };
                };
            }
        };
        assert forall|f: Name| v.dom().contains(f) implies v[f] <= 2 by {
            assert(v.dom().contains(f) ==> f == 3nat);
        };
        assert forall|f: Name| v.dom().contains(f) implies is_defined(p, f) by {
            assert(v.dom().contains(f) ==> f == 3nat);
            if v.dom().contains(f) {
                assert(f == 3nat);
                assert(is_defined(p, 3nat)) by {
                    assert(stmt_kind(p[0]) == 6 && fndef_name(p[0]) == 3nat);
                };
            }
        };
    };
    purity_sound(p, v, 3nat, 2);
}

fn main() {}

}
