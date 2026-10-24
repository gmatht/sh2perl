// Standalone model of dead-store-elim's core
// (sh2perl/src/transforms/dead_store_elim.rs): the read/write census, the
// purity gate, and the purge, over a store + observable-trace semantics.
// Proves the soundness obligation:
//
//   purge_sound: purging dead stores preserves the event trace and every
//   live variable's final value, from any closed start store.
//
// Names are interned to nat (as in the sibling proofs). The observable
// effect is the event trace (prints, external calls, spawns); stores to
// dead variables are unobservable by construction of the dead predicate.
//
// What is modelled and what is NOT (fidelity, checked against
// dead_store_elim.rs):
//   - Assign (plain scalar target + RHS), Declare (names + optional
//     init), SetVar (the sh2.setVar store channel), Output, Other
//     (opaque statements that are never dropped: kept with their
//     effects; they contribute reads only — as in the real census,
//     whose Other arms record reads and never writes).
//   - The purity gate is STRUCTURAL over a small expression language
//     (Lit/Var/Add/Call/Capture) and its soundness (pure ==> no events)
//     is PROVED (pure_no_events), not assumed.
//   - The Const-only guard is modelled as single-write
//     (count_writes == 1): the real analyze_var_const verdict's key
//     structural property for the case the transform exploits (one
//     store site, never rewritten). The full const analysis is out of
//     scope; the model takes "written once" as the sufficient condition.
//   - Dynamic writes (eval/source/.) are modelled as Call nodes with
//     dyn_write set; they make reads_any total, so the dead set is
//     empty and purge is the identity — the same outcome as the real
//     early-return (`if has_dynamic_write { return false; }`), proved
//     uniformly rather than as a special case.
//   - Shell-implicit vars (IFS/PATH/...) are a parameter set that is
//     never dead, as in the real SHELL_IMPLICIT_VARS guard.
//   - The escapes set is NOT modelled: every escapes.insert in the real
//     census is paired with a reads.insert on the same name, so escapes
//     is a subset of reads and the separate check is defensive
//     redundancy. (Checked arm-by-arm against census_stmt/census_expr.)
//   - Indexed targets (`arr[i]=`, `a[1]`-named stores) are NOT modelled:
//     the real purge keeps them unconditionally; the model has plain
//     scalar targets only, i.e. exactly the droppable class.
//   - Case-pattern `$name` reads, `$name`-in-Str reads, `[*]`-reads-IFS,
//     Ext-node reads: all folded into statements' reads sets (the model
//     assumes the census's structural completeness for the constructs it
//     covers — the same assumption has_dynamic_write protects).
//   - Flat statement sequences only: the real purge recurses into
//     If/Loop/Block/Function bodies with the same globally-computed
//     dead set; each nested case is the same induction presented here.
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;

// ---- Expressions ----

pub enum Expr {
    Lit(int),
    Var(Name),
    Add(Box<Expr>, Box<Expr>),
    Call { f: Name, args: Seq<Expr>, dyn_write: bool },
    Capture { inner: Box<Expr> },
}

pub enum Ev {
    Print(int),
    RunCall(Name),
    Spawn,
}

// Termination is structural (decreases e / decreases a); no size metrics needed.

pub open spec fn expr_pure(e: Expr) -> bool
    decreases e,
{
    match e {
        Expr::Lit(_) => true,
        Expr::Var(_) => true,
        Expr::Add(l, r) => expr_pure(*l) && expr_pure(*r),
        Expr::Call { .. } => false,
        Expr::Capture { .. } => false,
    }
}

pub open spec fn args_pure(a: Seq<Expr>) -> bool
    decreases a,
{
    if a.len() == 0 {
        true
    } else {
        expr_pure(a[0]) && args_pure(a.drop_first())
    }
}

pub open spec fn expr_reads(e: Expr, n: Name) -> bool
    decreases e,
{
    match e {
        Expr::Lit(_) => false,
        Expr::Var(m) => m == n,
        Expr::Add(l, r) => expr_reads(*l, n) || expr_reads(*r, n),
        Expr::Call { args, .. } => args_reads(args, n),
        Expr::Capture { inner } => expr_reads(*inner, n),
    }
}

pub open spec fn args_reads(a: Seq<Expr>, n: Name) -> bool
    decreases a,
{
    if a.len() == 0 {
        false
    } else {
        expr_reads(a[0], n) || args_reads(a.drop_first(), n)
    }
}

pub open spec fn expr_has_dyn(e: Expr) -> bool
    decreases e,
{
    match e {
        Expr::Lit(_) => false,
        Expr::Var(_) => false,
        Expr::Add(l, r) => expr_has_dyn(*l) || expr_has_dyn(*r),
        Expr::Call { args, dyn_write, .. } => dyn_write || args_have_dyn(args),
        Expr::Capture { inner } => expr_has_dyn(*inner),
    }
}

pub open spec fn args_have_dyn(a: Seq<Expr>) -> bool
    decreases a,
{
    if a.len() == 0 {
        false
    } else {
        expr_has_dyn(a[0]) || args_have_dyn(a.drop_first())
    }
}

// Total lookup with a 0 default (the closedness precondition makes the
// default unreachable for census-visible names).
pub open spec fn lookup(s: Store, n: Name) -> int {
    if s.dom().contains(n) {
        s[n]
    } else {
        0
    }
}

// Evaluation returns (value, events). Calls emit RunCall plus their
// args' events; captures emit Spawn plus the inner events.
pub open spec fn eval(e: Expr, s: Store) -> (int, Seq<Ev>)
    decreases e,
{
    match e {
        Expr::Lit(v) => (v, Seq::empty()),
        Expr::Var(n) => (lookup(s, n), Seq::empty()),
        Expr::Add(l, r) => {
            let lv = eval(*l, s);
            let rv = eval(*r, s);
            (lv.0 + rv.0, lv.1.add(rv.1))
        },
        Expr::Call { f, args, .. } => {
            let av = eval_args(args, s);
            (0, seq![Ev::RunCall(f)].add(av.1))
        },
        Expr::Capture { inner } => {
            let iv = eval(*inner, s);
            (iv.0, seq![Ev::Spawn].add(iv.1))
        },
    }
}

pub open spec fn eval_args(a: Seq<Expr>, s: Store) -> (Seq<int>, Seq<Ev>)
    decreases a,
{
    if a.len() == 0 {
        (Seq::empty(), Seq::empty())
    } else {
        let h = eval(a[0], s);
        let t = eval_args(a.drop_first(), s);
        (seq![h.0].add(t.0), h.1.add(t.1))
    }
}

// Purity is sound: a pure expression emits no events. (The real
// expr_pure/arith_pure gate; here proved from the evaluation rules.)
proof fn pure_no_events(e: Expr, s: Store)
    ensures expr_pure(e) ==> eval(e, s).1 == Seq::<Ev>::empty(),
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(_) => {},
        Expr::Add(l, r) => {
            pure_no_events(*l, s);
            pure_no_events(*r, s);
        },
        Expr::Call { .. } => {},
        Expr::Capture { .. } => {},
    }
}

proof fn args_pure_no_events(a: Seq<Expr>, s: Store)
    ensures args_pure(a) ==> eval_args(a, s).1 == Seq::<Ev>::empty(),
    decreases a,
{
    if a.len() == 0 {
    } else {
        pure_no_events(a[0], s);
        args_pure_no_events(a.drop_first(), s);
    }
}

// ---- Statements ----

pub enum Stmt {
    Assign { target: Name, rhs: Expr },
    Decl { names: Seq<Name>, init: Option<Expr> },
    SetVar { name: Name, val: Expr },
    Output { val: Expr },
    Other { reads: Seq<Name>, effect: Seq<Ev> },
}

pub open spec fn stmt_kind(st: Stmt) -> nat {
    match st {
        Stmt::Assign { .. } => 0,
        Stmt::Decl { .. } => 1,
        Stmt::SetVar { .. } => 2,
        Stmt::Output { .. } => 3,
        Stmt::Other { .. } => 4,
    }
}

pub open spec fn assign_target(st: Stmt) -> Name {
    match st {
        Stmt::Assign { target, .. } => target,
        _ => 0,
    }
}

pub open spec fn assign_rhs(st: Stmt) -> Expr {
    match st {
        Stmt::Assign { rhs, .. } => rhs,
        _ => Expr::Lit(0),
    }
}

pub open spec fn decl_names(st: Stmt) -> Seq<Name> {
    match st {
        Stmt::Decl { names, .. } => names,
        _ => Seq::empty(),
    }
}

pub open spec fn decl_init(st: Stmt) -> Option<Expr> {
    match st {
        Stmt::Decl { init, .. } => init,
        _ => None,
    }
}

pub open spec fn setvar_name(st: Stmt) -> Name {
    match st {
        Stmt::SetVar { name, .. } => name,
        _ => 0,
    }
}

pub open spec fn setvar_val(st: Stmt) -> Expr {
    match st {
        Stmt::SetVar { val, .. } => val,
        _ => Expr::Lit(0),
    }
}

pub open spec fn output_val(st: Stmt) -> Expr {
    match st {
        Stmt::Output { val } => val,
        _ => Expr::Lit(0),
    }
}

pub open spec fn other_reads(st: Stmt) -> Seq<Name> {
    match st {
        Stmt::Other { reads, .. } => reads,
        _ => Seq::empty(),
    }
}

pub open spec fn other_effect(st: Stmt) -> Seq<Ev> {
    match st {
        Stmt::Other { effect, .. } => effect,
        _ => Seq::empty(),
    }
}

pub open spec fn seq_has(s: Seq<Name>, n: Name) -> bool {
    exists|k: int| 0 <= k < s.len() && s[k] == n
}

// Reads of one statement.
pub open spec fn stmt_reads(st: Stmt, n: Name) -> bool {
    if stmt_kind(st) == 0 {
        expr_reads(assign_rhs(st), n)
    } else if stmt_kind(st) == 1 {
        match decl_init(st) {
            Some(e) => expr_reads(e, n),
            None => false,
        }
    } else if stmt_kind(st) == 2 {
        expr_reads(setvar_val(st), n)
    } else if stmt_kind(st) == 3 {
        expr_reads(output_val(st), n)
    } else {
        seq_has(other_reads(st), n)
    }
}

// Writes of one statement (Other never writes — as in the real census).
pub open spec fn stmt_writes(st: Stmt, n: Name) -> bool {
    if stmt_kind(st) == 0 {
        assign_target(st) == n
    } else if stmt_kind(st) == 1 {
        seq_has(decl_names(st), n)
    } else if stmt_kind(st) == 2 {
        setvar_name(st) == n
    } else {
        false
    }
}

pub open spec fn stmt_has_dyn(st: Stmt) -> bool {
    if stmt_kind(st) == 0 {
        expr_has_dyn(assign_rhs(st))
    } else if stmt_kind(st) == 1 {
        match decl_init(st) {
            Some(e) => expr_has_dyn(e),
            None => false,
        }
    } else if stmt_kind(st) == 2 {
        expr_has_dyn(setvar_val(st))
    } else if stmt_kind(st) == 3 {
        expr_has_dyn(output_val(st))
    } else {
        false
    }
}

// ---- Whole-program census ----

pub open spec fn has_dyn(p: Seq<Stmt>) -> bool {
    exists|i: int| 0 <= i < p.len() && stmt_has_dyn(p[i])
}

// A dynamic write can touch any variable, so under one every name
// counts as read (the dead set is then empty — the real early-return,
// proved uniformly).
pub open spec fn reads_any(p: Seq<Stmt>, n: Name) -> bool {
    has_dyn(p) || exists|i: int| 0 <= i < p.len() && stmt_reads(p[i], n)
}

// Structural reads only (no dyn blanket): the closedness precondition.
pub open spec fn reads_some(p: Seq<Stmt>, n: Name) -> bool {
    exists|i: int| 0 <= i < p.len() && stmt_reads(p[i], n)
}

pub open spec fn count_writes(p: Seq<Stmt>, n: Name) -> nat
    decreases p.len(),
{
    if p.len() == 0 {
        0
    } else {
        (if stmt_writes(p[0], n) {
            1nat
        } else {
            0nat
        }) + count_writes(p.drop_first(), n)
    }
}

// Dead: single store site, never read anywhere (dyn blanket included),
// never shell-implicit.
pub open spec fn is_dead(p: Seq<Stmt>, implicit: Set<Name>, n: Name) -> bool {
    !implicit.contains(n) && count_writes(p, n) == 1 && !reads_any(p, n)
}

pub open spec fn names_all_dead(names: Seq<Name>, p: Seq<Stmt>, implicit: Set<Name>) -> bool
    decreases names.len(),
{
    if names.len() == 0 {
        true
    } else {
        is_dead(p, implicit, names[0]) && names_all_dead(names.drop_first(), p, implicit)
    }
}

pub open spec fn init_pure(o: Option<Expr>) -> bool {
    match o {
        Some(e) => expr_pure(e),
        None => true,
    }
}

// The drop rule: a dead store with a pure RHS goes; Decl needs every
// name dead and a pure-or-absent init; nothing else drops.
pub open spec fn drop_stmt(st: Stmt, p: Seq<Stmt>, implicit: Set<Name>) -> bool {
    if stmt_kind(st) == 0 {
        is_dead(p, implicit, assign_target(st)) && expr_pure(assign_rhs(st))
    } else if stmt_kind(st) == 1 {
        names_all_dead(decl_names(st), p, implicit) && init_pure(decl_init(st))
    } else if stmt_kind(st) == 2 {
        is_dead(p, implicit, setvar_name(st)) && expr_pure(setvar_val(st))
    } else {
        false
    }
}

pub open spec fn purge_in(s: Seq<Stmt>, whole: Seq<Stmt>, implicit: Set<Name>) -> Seq<Stmt>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if drop_stmt(s[0], whole, implicit) {
        purge_in(s.drop_first(), whole, implicit)
    } else {
        seq![s[0]].add(purge_in(s.drop_first(), whole, implicit))
    }
}

pub open spec fn purge(p: Seq<Stmt>, implicit: Set<Name>) -> Seq<Stmt> {
    purge_in(p, p, implicit)
}

// ---- Execution ----

pub open spec fn bind_all(s: Store, names: Seq<Name>, v: int) -> Store
    decreases names.len(),
{
    if names.len() == 0 {
        s
    } else {
        bind_all(s.insert(names[0], v), names.drop_first(), v)
    }
}

pub open spec fn step(st: Stmt, s: Store) -> (Store, Seq<Ev>) {
    if stmt_kind(st) == 0 {
        let r = eval(assign_rhs(st), s);
        (s.insert(assign_target(st), r.0), r.1)
    } else if stmt_kind(st) == 1 {
        match decl_init(st) {
            Some(e) => {
                let r = eval(e, s);
                (bind_all(s, decl_names(st), r.0), r.1)
            },
            None => (s, Seq::empty()),
        }
    } else if stmt_kind(st) == 2 {
        let r = eval(setvar_val(st), s);
        (s.insert(setvar_name(st), r.0), r.1)
    } else if stmt_kind(st) == 3 {
        let r = eval(output_val(st), s);
        (s, r.1.add(seq![Ev::Print(r.0)]))
    } else {
        (s, other_effect(st))
    }
}

// ---- Runs and the obligation ----

pub open spec fn run(p: Seq<Stmt>, s: Store) -> (Store, Seq<Ev>)
    decreases p.len(),
{
    if p.len() == 0 {
        (s, Seq::empty())
    } else {
        let h = step(p[0], s);
        let t = run(p.drop_first(), h.0);
        (t.0, h.1.add(t.1))
    }
}

// Closed: every structurally-read name is bound (the default in lookup
// is then unreachable for census-visible names).
pub open spec fn closed_for(p: Seq<Stmt>, s: Store) -> bool {
    forall|n: Name| reads_some(p, n) ==> s.dom().contains(n)
}

// Agreement on live state: equal traces are handled separately; stores
// agree on every non-dead name (dead bindings may differ — nothing
// observes them).
pub open spec fn agree_live(s1: Store, s2: Store, p: Seq<Stmt>, implicit: Set<Name>) -> bool {
    forall|n: Name| !is_dead(p, implicit, n) ==> lookup(s1, n) == lookup(s2, n)
}

// Unfold lemmas.
proof fn run_cons(h: Stmt, t: Seq<Stmt>, s: Store)
    ensures run(seq![h].add(t), s) == {
        let hh = step(h, s);
        let tt = run(t, hh.0);
        (tt.0, hh.1.add(tt.1))
    },
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

proof fn purge_step_kept(s: Seq<Stmt>, whole: Seq<Stmt>, implicit: Set<Name>)
    requires s.len() > 0, !drop_stmt(s[0], whole, implicit),
    ensures purge_in(s, whole, implicit) == seq![s[0]].add(purge_in(s.drop_first(), whole, implicit)),
{
    reveal_with_fuel(purge_in, 2);
}

proof fn purge_step_dropped(s: Seq<Stmt>, whole: Seq<Stmt>, implicit: Set<Name>)
    requires s.len() > 0, drop_stmt(s[0], whole, implicit),
    ensures purge_in(s, whole, implicit) == purge_in(s.drop_first(), whole, implicit),
{
    reveal_with_fuel(purge_in, 2);
}

proof fn count_step(p: Seq<Stmt>, n: Name)
    requires p.len() > 0,
    ensures count_writes(p, n) == (if stmt_writes(p[0], n) {
        1nat
    } else {
        0nat
    }) + count_writes(p.drop_first(), n),
{
    reveal_with_fuel(count_writes, 2);
}

// Census monotonicity over tails.
proof fn reads_mono(s: Seq<Stmt>, n: Name)
    requires s.len() > 0, reads_some(s.drop_first(), n),
    ensures reads_some(s, n),
{
    let i = choose|i: int| 0 <= i < s.drop_first().len() && stmt_reads(s.drop_first()[i], n);
    assert(stmt_reads(s[i + 1], n));
}

// Stepping only inserts bindings, so the domain grows.
proof fn insert_grows(s: Store, k: Name, v: int)
    ensures s.dom().subset_of(s.insert(k, v).dom()),
{
    assert forall|x: Name| s.dom().contains(x) implies s.insert(k, v).dom().contains(x) by {
    }
}

proof fn bind_grows(s: Store, names: Seq<Name>, v: int)
    ensures s.dom().subset_of(bind_all(s, names, v).dom()),
    decreases names.len(),
{
    if names.len() == 0 {
    } else {
        insert_grows(s, names[0], v);
        bind_grows(s.insert(names[0], v), names.drop_first(), v);
        assert(bind_all(s, names, v) == bind_all(s.insert(names[0], v), names.drop_first(), v));
    }
}

// Stepping preserves closedness: step only inserts bindings.
proof fn step_closed(st: Stmt, whole: Seq<Stmt>, s: Store)
    requires closed_for(whole, s),
    ensures closed_for(whole, step(st, s).0),
{
    if stmt_kind(st) == 0 {
        let r = eval(assign_rhs(st), s);
        insert_grows(s, assign_target(st), r.0);
        assert(step(st, s).0 == s.insert(assign_target(st), r.0));
    } else if stmt_kind(st) == 2 {
        let r = eval(setvar_val(st), s);
        insert_grows(s, setvar_name(st), r.0);
        assert(step(st, s).0 == s.insert(setvar_name(st), r.0));
    } else if stmt_kind(st) == 1 {
        match decl_init(st) {
            Some(e) => {
                let r = eval(e, s);
                bind_grows(s, decl_names(st), r.0);
                assert(step(st, s).0 == bind_all(s, decl_names(st), r.0));
            },
            None => {},
        }
    } else {
    }
}

// Evaluations that read only agreeing state agree on value and events.
proof fn eval_same(e: Expr, s1: Store, s2: Store)
    requires forall|m: Name| expr_reads(e, m) ==> lookup(s1, m) == lookup(s2, m),
    ensures eval(e, s1).0 == eval(e, s2).0 && eval(e, s1).1 == eval(e, s2).1,
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(n) => {
            assert(lookup(s1, n) == lookup(s2, n));
        },
        Expr::Add(l, r) => {
            assert forall|m: Name| expr_reads(*l, m) implies lookup(s1, m) == lookup(s2, m) by {
                assert(expr_reads(e, m));
            };
            assert forall|m: Name| expr_reads(*r, m) implies lookup(s1, m) == lookup(s2, m) by {
                assert(expr_reads(e, m));
            };
            eval_same(*l, s1, s2);
            eval_same(*r, s1, s2);
        },
        Expr::Call { args, .. } => {
            eval_args_same(args, s1, s2);
        },
        Expr::Capture { inner } => {
            assert forall|m: Name| expr_reads(*inner, m) implies lookup(s1, m) == lookup(s2, m) by {
                assert(expr_reads(e, m));
            };
            eval_same(*inner, s1, s2);
        },
    }
}

proof fn eval_args_same(a: Seq<Expr>, s1: Store, s2: Store)
    requires forall|m: Name| args_reads(a, m) ==> lookup(s1, m) == lookup(s2, m),
    ensures eval_args(a, s1).0 == eval_args(a, s2).0 && eval_args(a, s1).1 == eval_args(a, s2).1,
    decreases a,
{
    if a.len() == 0 {
    } else {
        assert forall|m: Name| expr_reads(a[0], m) implies lookup(s1, m) == lookup(s2, m) by {
            assert(args_reads(a, m));
        };
        eval_same(a[0], s1, s2);
        assert forall|m: Name| args_reads(a.drop_first(), m) implies lookup(s1, m) == lookup(s2, m) by {
            assert(args_reads(a, m));
        };
        eval_args_same(a.drop_first(), s1, s2);
    }
}

// A dropped statement's step changes only dead state and emits nothing.
proof fn drop_step_same(st: Stmt, whole: Seq<Stmt>, implicit: Set<Name>, s: Store)
    requires
        drop_stmt(st, whole, implicit),
        closed_for(whole, s),
    ensures
        step(st, s).1 == Seq::<Ev>::empty(),
        agree_live(step(st, s).0, s, whole, implicit),
{
    if stmt_kind(st) == 0 {
        let t = assign_target(st);
        let r = assign_rhs(st);
        assert(is_dead(whole, implicit, t) && expr_pure(r));
        pure_no_events(r, s);
        assert(step(st, s).1 == Seq::<Ev>::empty());
        assert forall|n: Name| !is_dead(whole, implicit, n) implies lookup(step(st, s).0, n) == lookup(s, n) by {
            if n == t {
                // t is dead; the implication is vacuous.
                assert(is_dead(whole, implicit, t));
            } else {
                assert(step(st, s).0 == s.insert(t, eval(r, s).0));
                assert(lookup(s.insert(t, eval(r, s).0), n) == lookup(s, n));
            }
        };
    } else if stmt_kind(st) == 1 {
        assert(names_all_dead(decl_names(st), whole, implicit) && init_pure(decl_init(st)));
        match decl_init(st) {
            Some(e) => {
                pure_no_events(e, s);
                assert(step(st, s).1 == Seq::<Ev>::empty());
                assert forall|n: Name| !is_dead(whole, implicit, n) implies lookup(step(st, s).0, n) == lookup(s, n) by {
                    if seq_has(decl_names(st), n) {
                        assert(is_dead(whole, implicit, n)) by {
                            names_dead_mem(decl_names(st), whole, implicit, n);
                        };
                    } else {
                        bind_skips(step(st, s).0, s, decl_names(st), eval(e, s).0, n);
                    }
                };
            },
            None => {},
        }
    } else {
        // SetVar: same argument as Assign.
        let t = setvar_name(st);
        let r = setvar_val(st);
        assert(is_dead(whole, implicit, t) && expr_pure(r));
        pure_no_events(r, s);
        assert(step(st, s).1 == Seq::<Ev>::empty());
        assert forall|n: Name| !is_dead(whole, implicit, n) implies lookup(step(st, s).0, n) == lookup(s, n) by {
            if n == t {
                assert(is_dead(whole, implicit, t));
            } else {
                assert(step(st, s).0 == s.insert(t, eval(r, s).0));
                assert(lookup(s.insert(t, eval(r, s).0), n) == lookup(s, n));
            }
        };
    }
}

// Agreement is symmetric and transitive (pointwise equalities).
proof fn agree_symm(s1: Store, s2: Store, p: Seq<Stmt>, implicit: Set<Name>)
    requires agree_live(s1, s2, p, implicit),
    ensures agree_live(s2, s1, p, implicit),
{
}

proof fn agree_trans(s1: Store, s2: Store, s3: Store, p: Seq<Stmt>, implicit: Set<Name>)
    requires agree_live(s1, s2, p, implicit), agree_live(s2, s3, p, implicit),
    ensures agree_live(s1, s3, p, implicit),
{
}

// Membership in an all-dead name list is dead.
proof fn names_dead_mem(names: Seq<Name>, p: Seq<Stmt>, implicit: Set<Name>, n: Name)
    requires names_all_dead(names, p, implicit) && seq_has(names, n),
    ensures is_dead(p, implicit, n),
    decreases names.len(),
{
    assert(names.len() > 0);
    if names[0] == n {
    } else {
        let k = choose|k: int| 0 <= k < names.len() && names[k] == n;
        assert(k != 0) by {
            if k == 0 {
                assert(names[k] == names[0]);
            }
        };
        assert(names.drop_first()[k - 1] == names[k]);
        assert(seq_has(names.drop_first(), n));
        names_dead_mem(names.drop_first(), p, implicit, n);
    }
}

// bind_all only touches listed names.
proof fn bind_skips(bound: Store, s: Store, names: Seq<Name>, v: int, n: Name)
    requires bound == bind_all(s, names, v) && !seq_has(names, n),
    ensures lookup(bound, n) == lookup(s, n),
    decreases names.len(),
{
    if names.len() == 0 {
    } else {
        assert(!seq_has(names.drop_first(), n));
        bind_skips(bind_all(s.insert(names[0], v), names.drop_first(), v), s.insert(names[0], v), names.drop_first(), v, n);
        assert(lookup(s.insert(names[0], v), n) == lookup(s, n));
    }
}

// Map/lookup facts.
proof fn lookup_insert_self(m: Store, k: Name, v: int)
    ensures lookup(m.insert(k, v), k) == v,
{
    assert(m.insert(k, v).dom().contains(k));
    assert(m.insert(k, v)[k] == v);
}

proof fn lookup_insert_other(m: Store, k: Name, v: int, j: Name)
    requires k != j,
    ensures lookup(m.insert(k, v), j) == lookup(m, j),
{
    if m.insert(k, v).dom().contains(j) {
        assert(m.dom().contains(j));
        assert(m.insert(k, v)[j] == m[j]);
    } else {
        assert(!m.dom().contains(j));
    }
}

// A kept statement executes identically on agreeing states: same
// events, and live-agreement preserved.
proof fn keep_step_same(st: Stmt, whole: Seq<Stmt>, implicit: Set<Name>, s1: Store, s2: Store)
    requires
        closed_for(whole, s1),
        closed_for(whole, s2),
        agree_live(s1, s2, whole, implicit),
        forall|m: Name| stmt_reads(st, m) ==> !is_dead(whole, implicit, m),
    ensures
        step(st, s1).1 == step(st, s2).1,
        agree_live(step(st, s1).0, step(st, s2).0, whole, implicit),
{
    if stmt_kind(st) == 0 {
        let t = assign_target(st);
        let r = assign_rhs(st);
        assert forall|m: Name| expr_reads(r, m) implies lookup(s1, m) == lookup(s2, m) by {
            assert(stmt_reads(st, m));
        };
        eval_same(r, s1, s2);
        assert(step(st, s1).1 == step(st, s2).1);
        assert forall|n: Name| !is_dead(whole, implicit, n) implies lookup(step(st, s1).0, n) == lookup(step(st, s2).0, n) by {
            if n == t {
                lookup_insert_self(s1, t, eval(r, s1).0);
                lookup_insert_self(s2, t, eval(r, s2).0);
            } else {
                lookup_insert_other(s1, t, eval(r, s1).0, n);
                lookup_insert_other(s2, t, eval(r, s2).0, n);
            }
        };
    } else if stmt_kind(st) == 1 {
        match decl_init(st) {
            Some(e) => {
                assert forall|m: Name| expr_reads(e, m) implies lookup(s1, m) == lookup(s2, m) by {
                    assert(stmt_reads(st, m));
                };
                eval_same(e, s1, s2);
                assert(step(st, s1).1 == step(st, s2).1);
                // Both sides bind the same names to the same value.
                assert forall|n: Name| !is_dead(whole, implicit, n) implies lookup(step(st, s1).0, n) == lookup(step(st, s2).0, n) by {
                    bind_all_same(decl_names(st), s1, s2, eval(e, s1).0, eval(e, s2).0, whole, implicit, n);
                };
            },
            None => {},
        }
    } else if stmt_kind(st) == 2 {
        let t = setvar_name(st);
        let r = setvar_val(st);
        assert forall|m: Name| expr_reads(r, m) implies lookup(s1, m) == lookup(s2, m) by {
            assert(stmt_reads(st, m));
        };
        eval_same(r, s1, s2);
        assert(step(st, s1).1 == step(st, s2).1);
        assert forall|n: Name| !is_dead(whole, implicit, n) implies lookup(step(st, s1).0, n) == lookup(step(st, s2).0, n) by {
            if n == t {
                lookup_insert_self(s1, t, eval(r, s1).0);
                lookup_insert_self(s2, t, eval(r, s2).0);
            } else {
                lookup_insert_other(s1, t, eval(r, s1).0, n);
                lookup_insert_other(s2, t, eval(r, s2).0, n);
            }
        };
    } else if stmt_kind(st) == 3 {
        let r = output_val(st);
        assert forall|m: Name| expr_reads(r, m) implies lookup(s1, m) == lookup(s2, m) by {
            assert(stmt_reads(st, m));
        };
        eval_same(r, s1, s2);
    } else {
    }
}

// Unfold bind_all one step.
proof fn bind_step(s: Store, names: Seq<Name>, v: int)
    requires names.len() > 0,
    ensures bind_all(s, names, v) == bind_all(s.insert(names[0], v), names.drop_first(), v),
{
    reveal_with_fuel(bind_all, 2);
}

// bind_all with the same value on agreeing states preserves agreement
// (duplicate names rebind identically on both sides — no distinctness
// needed since the same sequence runs on both).
proof fn bind_all_agree(names: Seq<Name>, s1: Store, s2: Store, v: int, whole: Seq<Stmt>, implicit: Set<Name>)
    requires agree_live(s1, s2, whole, implicit),
    ensures agree_live(bind_all(s1, names, v), bind_all(s2, names, v), whole, implicit),
    decreases names.len(),
{
    if names.len() == 0 {
    } else {
        assert(agree_live(s1.insert(names[0], v), s2.insert(names[0], v), whole, implicit)) by {
            assert forall|m: Name| !is_dead(whole, implicit, m) implies lookup(s1.insert(names[0], v), m) == lookup(s2.insert(names[0], v), m) by {
                if m == names[0] {
                    lookup_insert_self(s1, names[0], v);
                    lookup_insert_self(s2, names[0], v);
                } else {
                    lookup_insert_other(s1, names[0], v, m);
                    lookup_insert_other(s2, names[0], v, m);
                }
            };
        };
        bind_all_agree(names.drop_first(), s1.insert(names[0], v), s2.insert(names[0], v), v, whole, implicit);
        bind_step(s1, names, v);
        bind_step(s2, names, v);
    }
}

// bind_all with equal values on agreeing states agrees at every live name.
proof fn bind_all_same(names: Seq<Name>, s1: Store, s2: Store, v: int, w: int, whole: Seq<Stmt>, implicit: Set<Name>, n: Name)
    requires
        v == w,
        agree_live(s1, s2, whole, implicit),
        !is_dead(whole, implicit, n),
    ensures lookup(bind_all(s1, names, v), n) == lookup(bind_all(s2, names, w), n),
{
    bind_all_agree(names, s1, s2, v, whole, implicit);
}

// THE OBLIGATION.
proof fn purge_sound(s: Seq<Stmt>, whole: Seq<Stmt>, implicit: Set<Name>, cur1: Store, cur2: Store)
    requires
        forall|n: Name| reads_some(s, n) ==> reads_some(whole, n),
        closed_for(whole, cur1),
        closed_for(whole, cur2),
        agree_live(cur1, cur2, whole, implicit),
    ensures
        run(purge_in(s, whole, implicit), cur1).1 == run(s, cur2).1,
        agree_live(run(purge_in(s, whole, implicit), cur1).0, run(s, cur2).0, whole, implicit),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        assert forall|n: Name| reads_some(s.drop_first(), n) implies reads_some(whole, n) by {
            reads_mono(s, n);
        };
        if drop_stmt(s[0], whole, implicit) {
            purge_step_dropped(s, whole, implicit);
            // The dropped store is pure (no events) and dead (its
            // bindings are unobservable): the step changes only dead
            // state, so agreement is preserved and the trace is
            // untouched.
            drop_step_same(s[0], whole, implicit, cur2);
            step_closed(s[0], whole, cur1);
            step_closed(s[0], whole, cur2);
            agree_symm(step(s[0], cur2).0, cur2, whole, implicit);
            agree_trans(cur1, cur2, step(s[0], cur2).0, whole, implicit);
            purge_sound(s.drop_first(), whole, implicit, cur1, step(s[0], cur2).0);
            run_cons(s[0], s.drop_first(), cur2);
            assert(run(purge_in(s, whole, implicit), cur1).1 == run(s, cur2).1);
        } else {
            purge_step_kept(s, whole, implicit);
            // The kept statement executes identically on both sides:
            // every name it reads is census-live, hence agreeing.
            assert forall|m: Name| stmt_reads(s[0], m) implies !is_dead(whole, implicit, m) by {
                assert(reads_some(s, m)) by {
                    assert(stmt_reads(s[0], m));
                };
                assert(reads_some(whole, m));
                assert(reads_any(whole, m));
            };
            keep_step_same(s[0], whole, implicit, cur1, cur2);
            step_closed(s[0], whole, cur1);
            step_closed(s[0], whole, cur2);
            purge_sound(s.drop_first(), whole, implicit, step(s[0], cur1).0, step(s[0], cur2).0);
            run_cons(s[0], purge_in(s.drop_first(), whole, implicit), cur1);
            run_cons(s[0], s.drop_first(), cur2);
            assert(run(purge_in(s, whole, implicit), cur1).1 == run(s, cur2).1);
        }
    }
}

// THE TOP-LEVEL OBLIGATION: purge preserves the trace and every live
// variable's final value, from any closed start store.
proof fn purge_sound_top(p: Seq<Stmt>, implicit: Set<Name>, init: Store)
    requires closed_for(p, init),
    ensures
        run(purge(p, implicit), init).1 == run(p, init).1,
        forall|n: Name| !is_dead(p, implicit, n) ==> lookup(run(purge(p, implicit), init).0, n) == lookup(run(p, init).0, n),
{
    assert(agree_live(init, init, p, implicit)) by {
    };
    purge_sound(p, p, implicit, init, init);
}

// ---- Fidelity pins ----

// Lone pure store with no readers is dead and drops.
proof fn pin_lone_store_drops()
    ensures purge(seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }], Set::<Name>::empty()).len() == 0,
{
    let p = seq![Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) }];
    let im = Set::<Name>::empty();
    // count == 1: the head writes, the tail is empty.
    assert(stmt_writes(p[0], 7nat));
    assert(count_writes(p.drop_first(), 7nat) == 0) by {
        reveal_with_fuel(count_writes, 2);
    };
    count_step(p, 7nat);
    assert(count_writes(p, 7nat) == 1);
    // no reads, no dyn: the head reads nothing and the tail is empty.
    assert(!stmt_reads(p[0], 7nat));
    assert(!stmt_has_dyn(p[0]));
    assert(!has_dyn(p)) by {
        assert forall|i: int| 0 <= i < p.len() implies !stmt_has_dyn(p[i]) by {
            assert(p[i] == p[0]);
        };
    };
    assert(!reads_any(p, 7nat)) by {
        assert forall|i: int| 0 <= i < p.len() implies !stmt_reads(p[i], 7nat) by {
            assert(p[i] == p[0]);
        };
    };
    assert(is_dead(p, im, 7nat));
    assert(expr_pure(assign_rhs(p[0])));
    assert(drop_stmt(p[0], p, im));
    assert(purge_in(p, p, im) == purge_in(p.drop_first(), p, im)) by {
        reveal_with_fuel(purge_in, 2);
    };
    assert(purge_in(p.drop_first(), p, im) == Seq::<Stmt>::empty()) by {
        reveal_with_fuel(purge_in, 2);
    };
}

// A store read later is kept.
proof fn pin_read_store_kept()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Var(7nat) },
    ], Set::<Name>::empty()).len() == 2,
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    let im = Set::<Name>::empty();
    assert(p[0] == Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) });
    assert(p[1] == Stmt::Output { val: Expr::Var(7nat) });
    // The Output reads 7: witness index 1.
    assert(expr_reads(output_val(p[1]), 7nat));
    assert(stmt_reads(p[1], 7nat));
    assert(reads_some(p, 7nat));
    assert(reads_any(p, 7nat));
    assert(!is_dead(p, im, 7nat));
    assert(!drop_stmt(p[0], p, im));
    assert(!drop_stmt(p[1], p, im));
    assert(purge_in(p, p, im) == p) by {
        reveal_with_fuel(purge_in, 4);
    };
}

// An impure RHS is kept even with no readers.
proof fn pin_impure_rhs_kept()
    ensures purge(seq![Stmt::Assign { target: 7nat, rhs: Expr::Call { f: 3nat, args: Seq::empty(), dyn_write: false } }], Set::<Name>::empty()).len() == 1,
{
    let p = seq![Stmt::Assign { target: 7nat, rhs: Expr::Call { f: 3nat, args: Seq::empty(), dyn_write: false } }];
    let im = Set::<Name>::empty();
    assert(p[0] == Stmt::Assign { target: 7nat, rhs: Expr::Call { f: 3nat, args: Seq::empty(), dyn_write: false } });
    assert(!expr_pure(assign_rhs(p[0])));
    assert(!drop_stmt(p[0], p, im));
    assert(purge_in(p, p, im) == p) by {
        reveal_with_fuel(purge_in, 4);
    };
}

// A twice-written variable is kept (dead-store-elim needs the Const
// single-site guard; the intermediate store is redundant-store-elim's
// domain, not this pass's).
proof fn pin_multi_write_kept()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(6) },
    ], Set::<Name>::empty()).len() == 2,
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(6) },
    ];
    let im = Set::<Name>::empty();
    assert(p[0] == Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) });
    assert(p[1] == Stmt::Assign { target: 7nat, rhs: Expr::Lit(6) });
    assert(stmt_writes(p[0], 7nat));
    assert(stmt_writes(p[1], 7nat));
    count_step(p, 7nat);
    count_step(p.drop_first(), 7nat);
    assert(count_writes(p, 7nat) == 2);
    assert(!is_dead(p, im, 7nat));
    assert(!drop_stmt(p[0], p, im));
    assert(!drop_stmt(p[1], p, im));
    assert(purge_in(p, p, im) == p) by {
        reveal_with_fuel(purge_in, 4);
    };
}

// A shell-implicit store survives with no syntactic read anywhere.
proof fn pin_implicit_kept()
    ensures purge(seq![Stmt::Assign { target: 1nat, rhs: Expr::Lit(5) }], Set::<Name>::empty().insert(1nat)).len() == 1,
{
    let p = seq![Stmt::Assign { target: 1nat, rhs: Expr::Lit(5) }];
    let im = Set::<Name>::empty().insert(1nat);
    assert(p[0] == Stmt::Assign { target: 1nat, rhs: Expr::Lit(5) });
    assert(im.contains(1nat));
    assert(!is_dead(p, im, 1nat));
    assert(!drop_stmt(p[0], p, im));
    assert(purge_in(p, p, im) == p) by {
        reveal_with_fuel(purge_in, 4);
    };
}

// A dynamic write anywhere freezes the whole program: nothing drops.
proof fn pin_dyn_freezes()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Call { f: 3nat, args: seq![Expr::Lit(1)], dyn_write: true } },
    ], Set::<Name>::empty()).len() == 2,
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Call { f: 3nat, args: seq![Expr::Lit(1)], dyn_write: true } },
    ];
    let im = Set::<Name>::empty();
    assert(p[0] == Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) });
    assert(p[1] == Stmt::Output { val: Expr::Call { f: 3nat, args: seq![Expr::Lit(1)], dyn_write: true } });
    assert(expr_has_dyn(output_val(p[1])));
    assert(stmt_has_dyn(p[1]));
    assert(has_dyn(p));
    assert(reads_any(p, 7nat));
    assert(!is_dead(p, im, 7nat));
    assert(!drop_stmt(p[0], p, im));
    assert(!drop_stmt(p[1], p, im));
    assert(purge_in(p, p, im) == p) by {
        reveal_with_fuel(purge_in, 4);
    };
}

fn main() {}

}
