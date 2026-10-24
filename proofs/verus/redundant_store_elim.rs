// Standalone model of the FIXED redundant-store-elim
// (sh2perl/src/transforms/redundant_store_elim.rs): a scalar store `x=A`
// at i drops iff a LATER scalar store to x overwrites it before anything
// can observe it. Proves the soundness obligation:
//
//   rse_sound_top: purging redundant stores preserves the final store
//   AND the event trace EXACTLY, from any closed start store.
//
// Names are interned to nat (as in the sibling proofs). The observable
// effect is the event trace (prints, calls, spawns) plus the final
// store; a dropped store's value is provably replaced before any
// possible observation.
//
// What is modelled and what is NOT (fidelity, checked line-by-line
// against the fixed redundant_store_elim.rs):
//   - Assign { target, idx, rhs }: plain-scalar-target stores. idx is a
//     sequence of index expressions; a nonempty idx disqualifies the
//     store as a drop candidate AND as an overwriting store (real
//     scalar_pure_assign/scalar_assign_to both require indices.is_empty),
//     but idx READS stop the scan (the Bug-2 fix in stmt_reads).
//   - Output { val } (transparent: reads only, never an observer),
//     CallStmt { f, args, srefs } (an indirect observer; srefs models
//     the $names inside plain-Str args — the Bug-3 fix), Other { reads }
//     (plain reads, never dropped, never an observer).
//   - Purity is STRUCTURAL over Lit/Var/Add/ArithSE/Call/Capture, with
//     ArithSE (models ArithAst::Assign/IncDec) IMPURE — the Bug-1 fix
//     (the old `Arith(_) => true` is the can-fail mutant). Its
//     soundness (pure ==> no events, no writes) is PROVED.
//   - The drop rule is DECLARATIVE (exists a later scalar store with a
//     clean middle and a clean justifier): shown equivalent to the
//     O(n^2) forward scan in the header lemma discussion (scan-drop
//     ⟺ declarative; the scan's read/observer check at the justifier
//     itself is the (d) clause).
//   - The shadow argument: after dropping i (target x, justifier j),
//     x is "shadowed" until the next store; shadowed vars are never
//     read or observed (proved from the justifier), so the divergent
//     intermediate bindings are unobservable. Drop chains always end
//     at a kept store, so at end-of-program nothing is shadowed and
//     every variable lookup agrees (observational exactness).
//   - Dynamic writes / eval: a Str-carried `$x` that REACHES an eval is
//     a read (Bug-3 fix, srefs); eval's own dynamic effects beyond its
//     args' reads are outside the model (same boundary as the real
//     pass, which has no has_dynamic_write gate — RSE relies on the
//     textual census, now completed).
//   - Capture in an Assign RHS is NOT an observer (faithful to the
//     real indirect_observer, which matches only Expr-statements); a
//     capture that textually reads x stops the scan via expr_reads.
//     Builtins that dump the whole shell store (env-dump shapes) are
//     outside the model.
//   - Flat sequences only; the real transform applies the same
//     drop_redundant per function body (same lemma per body).
//   - First-significant-wins is modelled by the existential: any clean
//     justifier suffices, and the proof uses only its cleanliness.
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;

// ---- Expressions ----

pub enum Expr {
    Lit(int),
    Var(Name),
    Add(Box<Expr>, Box<Expr>),
    // Side-effecting arithmetic (models ArithAst::Assign/IncDec):
    // reads var, writes var := old + 1. IMPURE (Bug-1 fix).
    ArithSE { var: Name },
    // Call with plain-Str-embedded $names (srefs — Bug-3 fix).
    Call { f: Name, args: Seq<Expr>, srefs: Seq<Name> },
    Capture { inner: Box<Expr> },
}

pub enum Ev {
    Print(int),
    RunCall(Name),
    Spawn,
}

pub open spec fn expr_pure(e: Expr) -> bool
    decreases e,
{
    match e {
        Expr::Lit(_) => true,
        Expr::Var(_) => true,
        Expr::Add(l, r) => expr_pure(*l) && expr_pure(*r),
        Expr::ArithSE { .. } => false,
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

pub open spec fn seq_has(s: Seq<Name>, n: Name) -> bool {
    exists|k: int| 0 <= k < s.len() && s[k] == n
}

pub open spec fn expr_reads(e: Expr, n: Name) -> bool
    decreases e,
{
    match e {
        Expr::Lit(_) => false,
        Expr::Var(m) => m == n,
        Expr::Add(l, r) => expr_reads(*l, n) || expr_reads(*r, n),
        Expr::ArithSE { var } => var == n,
        Expr::Call { args, srefs, .. } => args_reads(args, n) || seq_has(srefs, n),
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

pub open spec fn lookup(s: Store, n: Name) -> int {
    if s.dom().contains(n) {
        s[n]
    } else {
        0
    }
}

// eval threads the store (ArithSE writes); returns (value, events, store).
pub open spec fn eval(e: Expr, s: Store) -> (int, Seq<Ev>, Store)
    decreases e,
{
    match e {
        Expr::Lit(v) => (v, Seq::empty(), s),
        Expr::Var(n) => (lookup(s, n), Seq::empty(), s),
        Expr::Add(l, r) => {
            let lv = eval(*l, s);
            let rv = eval(*r, lv.2);
            (lv.0 + rv.0, lv.1.add(rv.1), rv.2)
        },
        Expr::ArithSE { var } => {
            let nv = lookup(s, var) + 1;
            (nv, Seq::empty(), s.insert(var, nv))
        },
        Expr::Call { f, args, .. } => {
            let av = eval_args(args, s);
            (0, seq![Ev::RunCall(f)].add(av.1), av.2)
        },
        Expr::Capture { inner } => {
            let iv = eval(*inner, s);
            (iv.0, seq![Ev::Spawn].add(iv.1), iv.2)
        },
    }
}

pub open spec fn eval_args(a: Seq<Expr>, s: Store) -> (Seq<int>, Seq<Ev>, Store)
    decreases a,
{
    if a.len() == 0 {
        (Seq::empty(), Seq::empty(), s)
    } else {
        let h = eval(a[0], s);
        let t = eval_args(a.drop_first(), h.2);
        (seq![h.0].add(t.0), h.1.add(t.1), t.2)
    }
}

pub open spec fn eval_idx(idx: Seq<Expr>, s: Store) -> (Store, Seq<Ev>)
    decreases idx,
{
    if idx.len() == 0 {
        (s, Seq::empty())
    } else {
        let h = eval(idx[0], s);
        let t = eval_idx(idx.drop_first(), h.2);
        (t.0, h.1.add(t.1))
    }
}

// Purity is sound: pure expressions emit nothing and write nothing.
proof fn pure_no_events(e: Expr, s: Store)
    ensures
        expr_pure(e) ==> eval(e, s).1 == Seq::<Ev>::empty(),
        expr_pure(e) ==> eval(e, s).2 == s,
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(_) => {},
        Expr::Add(l, r) => {
            pure_no_events(*l, s);
            pure_no_events(*r, eval(*l, s).2);
        },
        Expr::ArithSE { .. } => {},
        Expr::Call { .. } => {},
        Expr::Capture { .. } => {},
    }
}

proof fn args_pure_no_change(a: Seq<Expr>, s: Store)
    ensures
        args_pure(a) ==> eval_args(a, s).1 == Seq::<Ev>::empty(),
        args_pure(a) ==> eval_args(a, s).2 == s,
    decreases a,
{
    if a.len() == 0 {
    } else {
        pure_no_events(a[0], s);
        args_pure_no_change(a.drop_first(), eval(a[0], s).2);
    }
}

proof fn idx_pure_no_change(idx: Seq<Expr>, s: Store)
    ensures
        args_pure(idx) ==> eval_idx(idx, s).1 == Seq::<Ev>::empty(),
        args_pure(idx) ==> eval_idx(idx, s).0 == s,
    decreases idx,
{
    if idx.len() == 0 {
    } else {
        pure_no_events(idx[0], s);
        idx_pure_no_change(idx.drop_first(), eval(idx[0], s).2);
    }
}

// ---- Statements ----

pub enum Stmt {
    Assign { target: Name, idx: Seq<Expr>, rhs: Expr },
    Output { val: Expr },
    CallStmt { f: Name, args: Seq<Expr>, srefs: Seq<Name> },
    Other { reads: Seq<Name> },
}

pub open spec fn stmt_kind(st: Stmt) -> nat {
    match st {
        Stmt::Assign { .. } => 0,
        Stmt::Output { .. } => 1,
        Stmt::CallStmt { .. } => 2,
        Stmt::Other { .. } => 3,
    }
}

pub open spec fn assign_target(st: Stmt) -> Name {
    match st {
        Stmt::Assign { target, .. } => target,
        _ => 0,
    }
}

pub open spec fn assign_idx(st: Stmt) -> Seq<Expr> {
    match st {
        Stmt::Assign { idx, .. } => idx,
        _ => Seq::empty(),
    }
}

pub open spec fn assign_rhs(st: Stmt) -> Expr {
    match st {
        Stmt::Assign { rhs, .. } => rhs,
        _ => Expr::Lit(0),
    }
}

pub open spec fn output_val(st: Stmt) -> Expr {
    match st {
        Stmt::Output { val } => val,
        _ => Expr::Lit(0),
    }
}

pub open spec fn callstmt_args(st: Stmt) -> Seq<Expr> {
    match st {
        Stmt::CallStmt { args, .. } => args,
        _ => Seq::empty(),
    }
}

pub open spec fn callstmt_srefs(st: Stmt) -> Seq<Name> {
    match st {
        Stmt::CallStmt { srefs, .. } => srefs,
        _ => Seq::empty(),
    }
}

pub open spec fn other_reads(st: Stmt) -> Seq<Name> {
    match st {
        Stmt::Other { reads } => reads,
        _ => Seq::empty(),
    }
}

// Reads of one statement. Assign checks the RHS AND the target indices
// (Bug-2 fix); CallStmt checks args AND Str-embedded names (Bug-3 fix).
pub open spec fn stmt_reads(st: Stmt, n: Name) -> bool {
    if stmt_kind(st) == 0 {
        expr_reads(assign_rhs(st), n) || args_reads(assign_idx(st), n)
    } else if stmt_kind(st) == 1 {
        expr_reads(output_val(st), n)
    } else if stmt_kind(st) == 2 {
        args_reads(callstmt_args(st), n) || seq_has(callstmt_srefs(st), n)
    } else {
        seq_has(other_reads(st), n)
    }
}

// Indirect observers: calls (and only calls — Assign/Output/Other are
// never observers, faithful to the real indirect_observer).
pub open spec fn is_observer(st: Stmt) -> bool {
    stmt_kind(st) == 2
}

// A plain scalar store to x (drop candidates and overwrites alike).
pub open spec fn is_scalar_store(st: Stmt, x: Name) -> bool {
    stmt_kind(st) == 0 && assign_target(st) == x && assign_idx(st).len() == 0
}

// ---- The drop rule (declarative = the forward scan) ----

// Clean middle: no read of x and no observer strictly between i and j.
pub open spec fn clean_mid(p: Seq<Stmt>, i: int, j: int, x: Name) -> bool {
    forall|k: int| i < k && k < j ==> !stmt_reads(p[k], x) && !is_observer(p[k])
}

// The justifier itself is clean (the scan checks reads/observers at j
// before accepting it as the overwriting store).
pub open spec fn clean_at(p: Seq<Stmt>, j: int, x: Name) -> bool {
    !stmt_reads(p[j], x) && !is_observer(p[j])
}

// Index i (a pure scalar store to x) drops iff some later scalar store
// to x overwrites it with a clean middle and a clean justifier.
pub open spec fn should_drop(p: Seq<Stmt>, i: int) -> bool {
    0 <= i && i < p.len() && is_scalar_store(p[i], assign_target(p[i]))
        && expr_pure(assign_rhs(p[i]))
        && exists|j: int, x: Name|
            x == assign_target(p[i]) && i < j && j < p.len() && is_scalar_store(p[j], x)
                && clean_mid(p, i, j, x) && clean_at(p, j, x)
}

pub open spec fn drop_idx(p: Seq<Stmt>, pos: int) -> bool {
    should_drop(p, pos)
}

// ---- Shadowed variables ----

// x is shadowed at pos iff its last store before pos was dropped.
pub open spec fn last_store_is_dropped(p: Seq<Stmt>, pos: int, x: Name) -> bool {
    exists|i: int|
        0 <= i && i < pos && is_scalar_store(p[i], x) && should_drop(p, i)
            && forall|m: int| i < m && m < pos ==> !is_scalar_store(p[m], x)
}

pub open spec fn shadowed(p: Seq<Stmt>, pos: int, x: Name) -> bool {
    last_store_is_dropped(p, pos, x)
}

// ---- Execution ----

pub open spec fn step(st: Stmt, s: Store) -> (Store, Seq<Ev>) {
    if stmt_kind(st) == 0 {
        // Indexed stores evaluate idx + rhs for effects; only plain
        // stores bind the scalar target (array state is unmodeled).
        let ie = eval_idx(assign_idx(st), s);
        let r = eval(assign_rhs(st), ie.0);
        let s1 = if assign_idx(st).len() == 0 {
            r.2.insert(assign_target(st), r.0)
        } else {
            r.2
        };
        (s1, ie.1.add(r.1))
    } else if stmt_kind(st) == 1 {
        let r = eval(output_val(st), s);
        (r.2, r.1.add(seq![Ev::Print(r.0)]))
    } else if stmt_kind(st) == 2 {
        let av = eval_args(callstmt_args(st), s);
        (av.2, seq![Ev::RunCall(callstmt_args_f(st))].add(av.1))
    } else {
        (s, Seq::empty())
    }
}

pub open spec fn callstmt_args_f(st: Stmt) -> Name {
    match st {
        Stmt::CallStmt { f, .. } => f,
        _ => 0,
    }
}

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

pub open spec fn closed_for(p: Seq<Stmt>, s: Store) -> bool {
    forall|n: Name| reads_some(p, n) ==> s.dom().contains(n)
}

pub open spec fn reads_some(p: Seq<Stmt>, n: Name) -> bool {
    exists|i: int| 0 <= i < p.len() && stmt_reads(p[i], n)
}

// Agreement except on shadowed vars.
pub open spec fn agree_shadow(s1: Store, s2: Store, p: Seq<Stmt>, pos: int) -> bool {
    forall|n: Name| !shadowed(p, pos, n) ==> lookup(s1, n) == lookup(s2, n)
}

// ---- Suffix / position plumbing ----

// drop_first commutes with subrange (suffix of a suffix is a suffix).
proof fn drop_suffix(s: Seq<Stmt>, pos: nat)
    requires pos < s.len(),
    ensures s.subrange(pos as int, s.len() as int).drop_first() == s.subrange(pos as int + 1, s.len() as int),
{
    assert(s.subrange(pos as int, s.len() as int).len() == s.len() - pos);
    assert(s.subrange(pos as int, s.len() as int).drop_first().len() == s.len() - pos - 1);
    assert(s.subrange(pos as int + 1, s.len() as int).len() == s.len() - (pos + 1));
    assert forall|k: int| 0 <= k < s.subrange(pos as int, s.len() as int).drop_first().len() implies s.subrange(pos as int, s.len() as int).drop_first()[k] == s.subrange(pos as int + 1, s.len() as int)[k] by {
        assert(s.subrange(pos as int, s.len() as int).drop_first()[k] == s.subrange(pos as int, s.len() as int)[k + 1]);
        assert(s.subrange(pos as int, s.len() as int)[k + 1] == s[pos + k + 1]);
        assert(s.subrange(pos as int + 1, s.len() as int)[k] == s[pos + 1 + k]);
    };
}

proof fn head_at(s: Seq<Stmt>, pos: nat)
    requires pos < s.len(),
    ensures s.subrange(pos as int, s.len() as int)[0] == s[pos as int],
{
    assert(s.subrange(pos as int, s.len() as int)[0] == s[pos as int]);
}

// ---- Purge (position-indexed filter) ----

pub open spec fn purge_at(s: Seq<Stmt>, whole: Seq<Stmt>, pos: nat) -> Seq<Stmt>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if drop_idx(whole, pos as int) {
        purge_at(s.drop_first(), whole, pos + 1)
    } else {
        seq![s[0]].add(purge_at(s.drop_first(), whole, pos + 1))
    }
}

pub open spec fn purge(p: Seq<Stmt>) -> Seq<Stmt> {
    purge_at(p, p, 0)
}

proof fn purge_step_kept(s: Seq<Stmt>, whole: Seq<Stmt>, pos: nat)
    requires s.len() > 0, s == whole.subrange(pos as int, whole.len() as int), !drop_idx(whole, pos as int),
    ensures purge_at(s, whole, pos) == seq![s[0]].add(purge_at(s.drop_first(), whole, pos + 1)),
{
    reveal_with_fuel(purge_at, 2);
}

proof fn purge_step_dropped(s: Seq<Stmt>, whole: Seq<Stmt>, pos: nat)
    requires s.len() > 0, s == whole.subrange(pos as int, whole.len() as int), drop_idx(whole, pos as int),
    ensures purge_at(s, whole, pos) == purge_at(s.drop_first(), whole, pos + 1),
{
    reveal_with_fuel(purge_at, 2);
}

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

// ---- Map facts ----

// Subset membership transfers.
proof fn subset_has(a: Set<Name>, b: Set<Name>, n: Name)
    requires a.subset_of(b), a.contains(n),
    ensures b.contains(n),
{
}

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

proof fn insert_grows(s: Store, k: Name, v: int)
    ensures s.dom().subset_of(s.insert(k, v).dom()),
{
    assert forall|x: Name| s.dom().contains(x) implies s.insert(k, v).dom().contains(x) by {
    }
}

// Stepping only inserts scalar bindings, so closedness is preserved.
// (Indexed stores and Other bind nothing; ArithSE/call-arg writes
// insert too — every arm is dom-monotone.)
proof fn step_closed(st: Stmt, whole: Seq<Stmt>, s: Store)
    requires closed_for(whole, s),
    ensures closed_for(whole, step(st, s).0),
{
    step_grows(st, s);
    assert forall|n: Name| reads_some(whole, n) implies step(st, s).0.dom().contains(n) by {
        subset_has(s.dom(), step(st, s).0.dom(), n);
    };
}

// Every step is dom-monotone (only inserts scalar bindings).
proof fn step_grows(st: Stmt, s: Store)
    ensures s.dom().subset_of(step(st, s).0.dom()),
{
    if stmt_kind(st) == 0 {
        if assign_idx(st).len() == 0 {
            assert(step(st, s).0 == eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).2.insert(assign_target(st), eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).0));
        } else {
            assert(step(st, s).0 == eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).2);
        }
        idx_grows(assign_idx(st), s);
        eval_grows(assign_rhs(st), eval_idx(assign_idx(st), s).0);
        insert_grows(eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).2, assign_target(st), eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).0);
        assert forall|x: Name| s.dom().contains(x) implies step(st, s).0.dom().contains(x) by {
            subset_has(s.dom(), eval_idx(assign_idx(st), s).0.dom(), x);
            subset_has(eval_idx(assign_idx(st), s).0.dom(), eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).2.dom(), x);
            if assign_idx(st).len() == 0 {
                subset_has(eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).2.dom(), step(st, s).0.dom(), x);
            } else {
                assert(eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).2.dom() == step(st, s).0.dom());
            }
        };
    } else if stmt_kind(st) == 1 {
        eval_grows(output_val(st), s);
        assert(step(st, s).0 == eval(output_val(st), s).2);
    } else if stmt_kind(st) == 2 {
        eval_args_grows(callstmt_args(st), s);
        assert(step(st, s).0 == eval_args(callstmt_args(st), s).2);
    } else {
    }
}

// eval_grows, pointwise (for chaining inside step_grows).
proof fn eval_grows_at(e: Expr, s: Store, x: Name)
    requires s.dom().contains(x),
    ensures eval(e, s).2.dom().contains(x),
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(_) => {},
        Expr::Add(l, r) => {
            let lv = eval(*l, s);
            eval_grows_at(*l, s, x);
            eval_grows_at(*r, lv.2, x);
        },
        Expr::ArithSE { var } => {
            insert_grows(s, var, lookup(s, var) + 1);
            subset_has(s.dom(), s.insert(var, lookup(s, var) + 1).dom(), x);
        },
        Expr::Call { args, .. } => {
            eval_args_grows_at(args, s, x);
        },
        Expr::Capture { inner } => {
            eval_grows_at(*inner, s, x);
        },
    }
}

proof fn eval_args_grows_at(a: Seq<Expr>, s: Store, x: Name)
    requires s.dom().contains(x),
    ensures eval_args(a, s).2.dom().contains(x),
    decreases a,
{
    if a.len() == 0 {
    } else {
        let h = eval(a[0], s);
        eval_grows_at(a[0], s, x);
        eval_args_grows_at(a.drop_first(), h.2, x);
    }
}

proof fn idx_grows(idx: Seq<Expr>, s: Store)
    ensures s.dom().subset_of(eval_idx(idx, s).0.dom()),
    decreases idx,
{
    if idx.len() == 0 {
    } else {
        let h = eval(idx[0], s);
        eval_grows(idx[0], s);
        idx_grows(idx.drop_first(), h.2);
    }
}

proof fn eval_grows(e: Expr, s: Store)
    ensures s.dom().subset_of(eval(e, s).2.dom()),
    decreases e,
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(_) => {},
        Expr::Add(l, r) => {
            let lv = eval(*l, s);
            eval_grows(*l, s);
            eval_grows(*r, lv.2);
        },
        Expr::ArithSE { var } => {
            insert_grows(s, var, lookup(s, var) + 1);
        },
        Expr::Call { args, .. } => {
            eval_args_grows(args, s);
        },
        Expr::Capture { inner } => {
            eval_grows(*inner, s);
        },
    }
}

proof fn eval_args_grows(a: Seq<Expr>, s: Store)
    ensures s.dom().subset_of(eval_args(a, s).2.dom()),
    decreases a,
{
    if a.len() == 0 {
    } else {
        let h = eval(a[0], s);
        eval_grows(a[0], s);
        eval_args_grows(a.drop_first(), h.2);
    }
}

// ---- Evaluation agreement ----

// Evaluations that read only unshadowed state agree on value and
// events, and preserve shadow-agreement (writes are deterministic
// functions of agreeing reads).
proof fn eval_same(e: Expr, s1: Store, s2: Store, p: Seq<Stmt>, pos: int)
    requires
        forall|m: Name| expr_reads(e, m) ==> !shadowed(p, pos, m),
        agree_shadow(s1, s2, p, pos),
    ensures
        eval(e, s1).0 == eval(e, s2).0,
        eval(e, s1).1 == eval(e, s2).1,
        agree_shadow(eval(e, s1).2, eval(e, s2).2, p, pos),
    decreases e,
{
    match e {
        Expr::Lit(_) => {
            assert(eval(e, s1).2 == s1);
            assert(eval(e, s2).2 == s2);
        },
        Expr::Var(n) => {
            assert(expr_reads(e, n));
            assert(!shadowed(p, pos, n));
            assert(eval(e, s1).2 == s1);
            assert(eval(e, s2).2 == s2);
        },
        Expr::Add(l, r) => {
            assert forall|m: Name| expr_reads(*l, m) implies !shadowed(p, pos, m) by {
                assert(expr_reads(e, m));
            };
            eval_same(*l, s1, s2, p, pos);
            assert forall|m: Name| expr_reads(*r, m) implies !shadowed(p, pos, m) by {
                assert(expr_reads(e, m));
            };
            assert(agree_shadow(eval(*l, s1).2, eval(*l, s2).2, p, pos));
            eval_same(*r, eval(*l, s1).2, eval(*l, s2).2, p, pos);
            assert(eval(e, s1).2 == eval(*r, eval(*l, s1).2).2);
            assert(eval(e, s2).2 == eval(*r, eval(*l, s2).2).2);
        },
        Expr::ArithSE { var } => {
            assert(expr_reads(e, var));
            assert(!shadowed(p, pos, var));
            assert(lookup(s1, var) == lookup(s2, var));
            assert(eval(e, s1).2 == s1.insert(var, lookup(s1, var) + 1));
            assert(eval(e, s2).2 == s2.insert(var, lookup(s2, var) + 1));
            assert forall|n: Name| !shadowed(p, pos, n) implies lookup(eval(e, s1).2, n) == lookup(eval(e, s2).2, n) by {
                if n == var {
                    lookup_insert_self(s1, var, lookup(s1, var) + 1);
                    lookup_insert_self(s2, var, lookup(s2, var) + 1);
                } else {
                    lookup_insert_other(s1, var, lookup(s1, var) + 1, n);
                    lookup_insert_other(s2, var, lookup(s2, var) + 1, n);
                }
            };
        },
        Expr::Call { args, .. } => {
            assert forall|m: Name| args_reads(args, m) implies !shadowed(p, pos, m) by {
                assert(expr_reads(e, m));
            };
            eval_args_same(args, s1, s2, p, pos);
            assert(eval(e, s1).2 == eval_args(args, s1).2);
            assert(eval(e, s2).2 == eval_args(args, s2).2);
        },
        Expr::Capture { inner } => {
            assert forall|m: Name| expr_reads(*inner, m) implies !shadowed(p, pos, m) by {
                assert(expr_reads(e, m));
            };
            eval_same(*inner, s1, s2, p, pos);
            assert(eval(e, s1).2 == eval(*inner, s1).2);
            assert(eval(e, s2).2 == eval(*inner, s2).2);
        },
    }
}

proof fn eval_args_same(a: Seq<Expr>, s1: Store, s2: Store, p: Seq<Stmt>, pos: int)
    requires
        forall|m: Name| args_reads(a, m) ==> !shadowed(p, pos, m),
        agree_shadow(s1, s2, p, pos),
    ensures
        eval_args(a, s1).0 == eval_args(a, s2).0,
        eval_args(a, s1).1 == eval_args(a, s2).1,
        agree_shadow(eval_args(a, s1).2, eval_args(a, s2).2, p, pos),
    decreases a,
{
    if a.len() == 0 {
    } else {
        assert forall|m: Name| expr_reads(a[0], m) implies !shadowed(p, pos, m) by {
            assert(args_reads(a, m));
        };
        eval_same(a[0], s1, s2, p, pos);
        assert forall|m: Name| args_reads(a.drop_first(), m) implies !shadowed(p, pos, m) by {
            assert(args_reads(a, m));
        };
        eval_args_same(a.drop_first(), eval(a[0], s1).2, eval(a[0], s2).2, p, pos);
    }
}

proof fn eval_idx_same(idx: Seq<Expr>, s1: Store, s2: Store, p: Seq<Stmt>, pos: int)
    requires
        forall|m: Name| args_reads(idx, m) ==> !shadowed(p, pos, m),
        agree_shadow(s1, s2, p, pos),
    ensures
        eval_idx(idx, s1).1 == eval_idx(idx, s2).1,
        agree_shadow(eval_idx(idx, s1).0, eval_idx(idx, s2).0, p, pos),
    decreases idx,
{
    if idx.len() == 0 {
    } else {
        assert forall|m: Name| expr_reads(idx[0], m) implies !shadowed(p, pos, m) by {
            assert(args_reads(idx, m));
        };
        eval_same(idx[0], s1, s2, p, pos);
        assert forall|m: Name| args_reads(idx.drop_first(), m) implies !shadowed(p, pos, m) by {
            assert(args_reads(idx, m));
        };
        eval_idx_same(idx.drop_first(), eval(idx[0], s1).2, eval(idx[0], s2).2, p, pos);
    }
}

// ---- The shadow key lemma ----

// A justifier for a dropped store: clean middle, clean self.
proof fn justifier_of(p: Seq<Stmt>, i: int, x: Name) -> (j: int)
    requires
        0 <= i && i < p.len(),
        should_drop(p, i),
        x == assign_target(p[i]),
    ensures
        i < j && j < p.len() && is_scalar_store(p[j], x) && clean_mid(p, i, j, x)
            && clean_at(p, j, x),
{
    let j: int = choose|j: int|
        i < j && j < p.len() && is_scalar_store(p[j], x) && clean_mid(p, i, j, x)
            && clean_at(p, j, x);
    j
}

// Shadowed variables are never observed by the head statement: any
// read of x — or any observer — at pos would contradict the
// justifier's cleanliness (strictly inside (i,j), or at the clean
// justifier j itself).
proof fn shadow_stops_scan(p: Seq<Stmt>, pos: nat, x: Name)
    requires
        pos < p.len(),
        shadowed(p, pos as int, x),
    ensures
        !stmt_reads(p[pos as int], x),
        !is_observer(p[pos as int]),
{
    // Unfold the shadow witness: a dropped last-store i before pos.
    assert(exists|i: int|
        0 <= i && i < pos as int && is_scalar_store(p[i], x) && should_drop(p, i)
            && forall|m: int| i < m && m < pos as int ==> !is_scalar_store(p[m], x));
    let i: int = choose|i: int|
        0 <= i && i < pos as int && is_scalar_store(p[i], x) && should_drop(p, i)
            && forall|m: int| i < m && m < pos as int ==> !is_scalar_store(p[m], x);
    let j: int = justifier_of(p, i, x);
    // The justifier is at or after pos (it is a store to x after i).
    assert(j >= pos as int) by {
        if j < pos as int {
            assert(is_scalar_store(p[j], x));
        }
    };
    if j == pos as int {
        // The head IS the justifier: clean_at contradicts observation.
        assert(clean_at(p, j, x));
        assert(p[j] == p[pos as int]);
    } else {
        // The head is strictly inside (i, j): clean_mid contradicts.
        assert(j > pos as int);
        assert(clean_mid(p, i, j, x));
        assert(!stmt_reads(p[pos as int], x) && !is_observer(p[pos as int]));
    }
}

// ---- Step lemmas ----

// A kept head executes identically on shadow-agreeing states: every
// name it reads is unshadowed (shadow_stops_scan), so evaluations
// agree and the trace is equal.
proof fn keep_step_same(st: Stmt, p: Seq<Stmt>, pos: nat, s1: Store, s2: Store)
    requires
        pos < p.len(),
        st == p[pos as int],
        closed_for(p, s1),
        closed_for(p, s2),
        agree_shadow(s1, s2, p, pos as int),
    ensures
        step(st, s1).1 == step(st, s2).1,
        agree_shadow(step(st, s1).0, step(st, s2).0, p, pos as int + 1),
{
    // Every read of the head is unshadowed (a shadowed read would
    // contradict its justifier via the lemma).
    assert forall|m: Name| stmt_reads(st, m) implies !shadowed(p, pos as int, m) by {
        if shadowed(p, pos as int, m) {
            shadow_stops_scan(p, pos, m);
        }
    };
    if stmt_kind(st) == 0 {
        // Assign: idx + rhs evaluate identically. Final agreement: for
        // n unshadowed at pos+1, either the head stores to n (both sides
        // bind the same value) or n was already unshadowed (agreement
        // carries through the identical writes).
        assert forall|m: Name| args_reads(assign_idx(st), m) implies !shadowed(p, pos as int, m) by {
            if shadowed(p, pos as int, m) {
                assert(stmt_reads(st, m));
                shadow_stops_scan(p, pos, m);
            }
        };
        eval_idx_same(assign_idx(st), s1, s2, p, pos as int);
        assert forall|m: Name| expr_reads(assign_rhs(st), m) implies !shadowed(p, pos as int, m) by {
            if shadowed(p, pos as int, m) {
                assert(stmt_reads(st, m));
                shadow_stops_scan(p, pos, m);
            }
        };
        let ie1 = eval_idx(assign_idx(st), s1);
        let ie2 = eval_idx(assign_idx(st), s2);
        eval_same(assign_rhs(st), ie1.0, ie2.0, p, pos as int);
        let r1 = eval(assign_rhs(st), ie1.0);
        let r2 = eval(assign_rhs(st), ie2.0);
        assert(r1.0 == r2.0 && r1.1 == r2.1);
        assert(agree_shadow(r1.2, r2.2, p, pos as int));
        assert(step(st, s1).1 == step(st, s2).1);
        let t = assign_target(st);
        assert forall|n: Name| !shadowed(p, pos as int + 1, n) implies lookup(step(st, s1).0, n) == lookup(step(st, s2).0, n) by {
            if n == t && assign_idx(st).len() == 0 {
                // Fresh identical binding on both sides.
                assert(step(st, s1).0 == r1.2.insert(t, r1.0));
                assert(step(st, s2).0 == r2.2.insert(t, r2.0));
                lookup_insert_self(r1.2, t, r1.0);
                lookup_insert_self(r2.2, t, r2.0);
            } else {
                // Unshadowed before (the head stores elsewhere or not
                // at all): agreement carries through identical writes.
                // (The disjunct "head stores to n" is impossible here:
                // a scalar store to n means target==n with empty idx,
                // excluded by this branch.)
                assert(!shadowed(p, pos as int, n)) by {
                    if is_scalar_store(p[pos as int], n) {
                        assert(p[pos as int] == st);
                        assert(assign_target(st) == n);
                        assert(assign_idx(st).len() == 0);
                    } else {
                        unshadow_before(p, pos, n);
                    }
                };
                assert(lookup(r1.2, n) == lookup(r2.2, n));
                if assign_idx(st).len() == 0 {
                    assert(step(st, s1).0 == r1.2.insert(t, r1.0));
                    assert(step(st, s2).0 == r2.2.insert(t, r2.0));
                    lookup_insert_other(r1.2, t, r1.0, n);
                    lookup_insert_other(r2.2, t, r2.0, n);
                } else {
                    assert(step(st, s1).0 == r1.2);
                    assert(step(st, s2).0 == r2.2);
                }
            }
        };
    } else if stmt_kind(st) == 1 {
        assert forall|m: Name| expr_reads(output_val(st), m) implies !shadowed(p, pos as int, m) by {
            if shadowed(p, pos as int, m) {
                assert(stmt_reads(st, m));
                shadow_stops_scan(p, pos, m);
            }
        };
        eval_same(output_val(st), s1, s2, p, pos as int);
        assert(step(st, s1).1 == step(st, s2).1);
        // Output binds nothing scalarly: status preserved, agreement carries.
        assert forall|n: Name| !shadowed(p, pos as int + 1, n) implies lookup(step(st, s1).0, n) == lookup(step(st, s2).0, n) by {
            assert(!is_scalar_store(p[pos as int], n));
            shadow_preserved(p, pos, n);
            assert(lookup(eval(output_val(st), s1).2, n) == lookup(eval(output_val(st), s2).2, n));
            assert(step(st, s1).0 == eval(output_val(st), s1).2);
            assert(step(st, s2).0 == eval(output_val(st), s2).2);
        };
    } else if stmt_kind(st) == 2 {
        // A CallStmt head means nothing is shadowed at all
        // (shadow_stops_scan: observers meet no shadows).
        // A CallStmt head meets no shadows: a shadowed m would give
        // !is_observer(head) via the lemma, contradicting kind == 2.
        assert forall|m: Name| !shadowed(p, pos as int, m) by {
            if shadowed(p, pos as int, m) {
                shadow_stops_scan(p, pos, m);
                assert(stmt_kind(p[pos as int]) == 2);
                assert(is_observer(p[pos as int]));
            }
        };
        assert forall|m: Name| args_reads(callstmt_args(st), m) implies !shadowed(p, pos as int, m) by {
            if shadowed(p, pos as int, m) {
                assert(stmt_reads(st, m));
                shadow_stops_scan(p, pos, m);
            }
        };
        eval_args_same(callstmt_args(st), s1, s2, p, pos as int);
        assert(step(st, s1).1 == step(st, s2).1);
        assert forall|n: Name| !shadowed(p, pos as int + 1, n) implies lookup(step(st, s1).0, n) == lookup(step(st, s2).0, n) by {
            assert(!is_scalar_store(p[pos as int], n));
            shadow_preserved(p, pos, n);
            assert(lookup(eval_args(callstmt_args(st), s1).2, n) == lookup(eval_args(callstmt_args(st), s2).2, n));
            assert(step(st, s1).0 == eval_args(callstmt_args(st), s1).2);
            assert(step(st, s2).0 == eval_args(callstmt_args(st), s2).2);
        };
    } else {
        // Other binds nothing.
        assert(step(st, s1).1 == step(st, s2).1);
        assert forall|n: Name| !shadowed(p, pos as int + 1, n) implies lookup(step(st, s1).0, n) == lookup(step(st, s2).0, n) by {
            assert(!is_scalar_store(p[pos as int], n));
            shadow_preserved(p, pos, n);
            assert(step(st, s1).0 == s1);
            assert(step(st, s2).0 == s2);
        };
    }
}

// Unfold equation for the scalar-store predicate (non-recursive, free).
proof fn sss_unfold(st: Stmt, x: Name)
    ensures is_scalar_store(st, x) == (stmt_kind(st) == 0 && assign_target(st) == x && assign_idx(st).len() == 0),
{
}

// A head that does not scalar-store to n preserves n's shadow status.
proof fn shadow_preserved(p: Seq<Stmt>, pos: nat, n: Name)
    requires pos < p.len(), !is_scalar_store(p[pos as int], n),
    ensures shadowed(p, pos as int + 1, n) == shadowed(p, pos as int, n),
{
    if shadowed(p, pos as int, n) {
        assert(exists|i: int|
            0 <= i && i < pos as int && is_scalar_store(p[i], n) && should_drop(p, i)
                && forall|m: int| i < m && m < pos as int ==> !is_scalar_store(p[m], n));
        let i: int = choose|i: int|
            0 <= i && i < pos as int && is_scalar_store(p[i], n) && should_drop(p, i)
                && forall|m: int| i < m && m < pos as int ==> !is_scalar_store(p[m], n);
        assert(forall|m: int| i < m && m < pos as int + 1 ==> !is_scalar_store(p[m], n)) by {
            assert forall|m: int| i < m && m < pos as int + 1 implies !is_scalar_store(p[m], n) by {
                if m == pos as int {
                    assert(p[m] == p[pos as int]);
                }
            };
        };
        assert(shadowed(p, pos as int + 1, n));
    }
    if shadowed(p, pos as int + 1, n) {
        assert(exists|i: int|
            0 <= i && i < pos as int + 1 && is_scalar_store(p[i], n) && should_drop(p, i)
                && forall|m: int| i < m && m < pos as int + 1 ==> !is_scalar_store(p[m], n));
        let i: int = choose|i: int|
            0 <= i && i < pos as int + 1 && is_scalar_store(p[i], n) && should_drop(p, i)
                && forall|m: int| i < m && m < pos as int + 1 ==> !is_scalar_store(p[m], n);
        assert(i < pos as int) by {
            if i == pos as int {
                assert(p[i] == p[pos as int]);
                assert(is_scalar_store(p[pos as int], n));
            }
        };
        assert(forall|m: int| i < m && m < pos as int ==> !is_scalar_store(p[m], n)) by {
            assert forall|m: int| i < m && m < pos as int implies !is_scalar_store(p[m], n) by {
            };
        };
        assert(shadowed(p, pos as int, n));
    }
}

// A var unshadowed after the head was unshadowed before it, unless the
// head itself scalar-stores to it (fresh kept binding).
proof fn unshadow_before(p: Seq<Stmt>, pos: nat, n: Name)
    requires pos < p.len(), !shadowed(p, pos as int + 1, n),
    ensures !shadowed(p, pos as int, n) || is_scalar_store(p[pos as int], n),
{
    if shadowed(p, pos as int, n) && !is_scalar_store(p[pos as int], n) {
        assert(exists|i: int|
            0 <= i && i < pos as int && is_scalar_store(p[i], n) && should_drop(p, i)
                && forall|m: int| i < m && m < pos as int ==> !is_scalar_store(p[m], n));
        let i: int = choose|i: int|
            0 <= i && i < pos as int && is_scalar_store(p[i], n) && should_drop(p, i)
                && forall|m: int| i < m && m < pos as int ==> !is_scalar_store(p[m], n);
        assert(forall|m: int| i < m && m < pos as int + 1 ==> !is_scalar_store(p[m], n)) by {
            assert forall|m: int| i < m && m < pos as int + 1 implies !is_scalar_store(p[m], n) by {
                if m == pos as int {
                    assert(p[m] == p[pos as int]);
                }
            };
        };
        assert(shadowed(p, pos as int + 1, n));
    }
}

// ---- The main induction ----

// Purging the suffix from pos preserves the trace and shadow-agreement.
proof fn rse_sound(s: Seq<Stmt>, whole: Seq<Stmt>, pos: nat, cur1: Store, cur2: Store)
    requires
        s == whole.subrange(pos as int, whole.len() as int),
        pos as int + s.len() as int == whole.len() as int,
        closed_for(whole, cur1),
        closed_for(whole, cur2),
        agree_shadow(cur1, cur2, whole, pos as int),
    ensures
        run(purge_at(s, whole, pos), cur1).1 == run(s, cur2).1,
        agree_shadow(run(purge_at(s, whole, pos), cur1).0, run(s, cur2).0, whole, pos as int + s.len() as int),
    decreases s.len(),
{
    if s.len() == 0 {
        assert(purge_at(s, whole, pos) == Seq::<Stmt>::empty()) by {
            reveal_with_fuel(purge_at, 2);
        };
    } else {
        assert(s[0] == whole[pos as int]) by {
            head_at(whole, pos);
        };
        drop_suffix(whole, pos);
        assert(s.drop_first() == whole.subrange(pos as int + 1, whole.len() as int));
        if drop_idx(whole, pos as int) {
            purge_step_dropped(s, whole, pos);
            // The dropped head is a pure scalar store: no events, and
            // only its (shadowed-after) target's binding differs.
            drop_step_same(s[0], whole, pos, cur2);
            step_closed(s[0], whole, cur1);
            step_closed(s[0], whole, cur2);
            rse_sound(s.drop_first(), whole, pos + 1, cur1, step(s[0], cur2).0);
            run_cons(s[0], s.drop_first(), cur2);
        } else {
            purge_step_kept(s, whole, pos);
            keep_step_same(s[0], whole, pos, cur1, cur2);
            step_closed(s[0], whole, cur1);
            step_closed(s[0], whole, cur2);
            rse_sound(s.drop_first(), whole, pos + 1, step(s[0], cur1).0, step(s[0], cur2).0);
            run_cons(s[0], purge_at(s.drop_first(), whole, pos + 1), cur1);
            run_cons(s[0], s.drop_first(), cur2);
        }
    }
}

// A dropped head's step emits nothing and preserves agreement up to
// its (newly shadowed) target.
proof fn drop_step_same(st: Stmt, whole: Seq<Stmt>, pos: nat, s: Store)
    requires
        pos < whole.len(),
        st == whole[pos as int],
        drop_idx(whole, pos as int),
        closed_for(whole, s),
    ensures
        step(st, s).1 == Seq::<Ev>::empty(),
        agree_shadow(step(st, s).0, s, whole, pos as int + 1),
{
    // A drop fires only on pure scalar stores (empty idx by candidacy).
    assert(is_scalar_store(st, assign_target(st)));
    assert(assign_idx(st).len() == 0);
    assert(expr_pure(assign_rhs(st)));
    pure_no_events(assign_rhs(st), s);
    assert(step(st, s).1 == Seq::<Ev>::empty()) by {
        assert(step(st, s).1 == eval_idx(assign_idx(st), s).1.add(eval(assign_rhs(st), eval_idx(assign_idx(st), s).0).1));
        idx_pure_no_change(assign_idx(st), s);
    };
    let t = assign_target(st);
    assert forall|n: Name| !shadowed(whole, pos as int + 1, n) implies lookup(step(st, s).0, n) == lookup(s, n) by {
        // The only binding that changes is t's — and t is shadowed
        // after (its last store is this dropped head).
        assert(n != t) by {
            if n == t {
                let ppi: int = pos as int;
                assert(shadowed(whole, ppi + 1, t)) by {
                    assert(is_scalar_store(whole[ppi], t));
                    assert(should_drop(whole, ppi));
                    assert(forall|m: int| ppi < m && m < ppi + 1 ==> !is_scalar_store(whole[m], t)) by {
                        assert forall|m: int| ppi < m && m < ppi + 1 implies !is_scalar_store(whole[m], t) by {
                        };
                    };
                };
            }
        };
        assert(step(st, s).0 == eval(assign_rhs(st), s).2.insert(t, eval(assign_rhs(st), s).0));
        pure_no_events(assign_rhs(st), s);
        assert(eval(assign_rhs(st), s).2 == s);
        lookup_insert_other(s, t, eval(assign_rhs(st), s).0, n);
    };
}

// At end-of-program nothing is shadowed (every dropped store has a
// later store, so the last store is always kept): agreement is exact.
proof fn nothing_shadowed_at_end(p: Seq<Stmt>, x: Name)
    ensures !shadowed(p, p.len() as int, x),
{
    if shadowed(p, p.len() as int, x) {
        assert(exists|i: int|
            0 <= i && i < p.len() as int && is_scalar_store(p[i], x) && should_drop(p, i)
                && forall|m: int| i < m && m < p.len() as int ==> !is_scalar_store(p[m], x));
        let i: int = choose|i: int|
            0 <= i && i < p.len() as int && is_scalar_store(p[i], x) && should_drop(p, i)
                && forall|m: int| i < m && m < p.len() as int ==> !is_scalar_store(p[m], x);
        let j: int = justifier_of(p, i, x);
        assert(i < j && j < p.len());
        assert(is_scalar_store(p[j], x));
    }
}

// THE TOP-LEVEL OBLIGATION: purging is observationally exact — same
// trace, same value for every variable. (Stated via lookup: the two
// runs' domains can differ on never-observed dead bindings, but every
// lookup agrees — and at end-of-program nothing is shadowed, so this
// covers all names.)
proof fn rse_sound_top(p: Seq<Stmt>, init: Store)
    requires closed_for(p, init),
    ensures
        run(purge(p), init).1 == run(p, init).1,
        forall|n: Name| lookup(run(purge(p), init).0, n) == lookup(run(p, init).0, n),
{
    assert(purge(p) == purge_at(p, p, 0));
    assert(p == p.subrange(0, p.len() as int));
    rse_sound(p, p, 0, init, init);
    assert(agree_shadow(run(purge(p), init).0, run(p, init).0, p, p.len() as int));
    assert forall|n: Name| lookup(run(purge(p), init).0, n) == lookup(run(p, init).0, n) by {
        nothing_shadowed_at_end(p, n);
    };
}

// ---- Fidelity pins (one per bug fix + basic behaviors) ----

// A sequence is its own 0-suffix (feeds purge_at_* preconditions).
proof fn subrange_self(s: Seq<Stmt>)
    ensures s == s.subrange(0, s.len() as int),
{
    assert(s.len() == s.subrange(0, s.len() as int).len());
    assert forall|k: int| 0 <= k < s.len() implies s[k] == s.subrange(0, s.len() as int)[k] by {
    };
}

// Unfold purge_at one step, by verdict.
proof fn purge_at_kept(s: Seq<Stmt>, whole: Seq<Stmt>, pos: nat)
    requires s.len() > 0, s == whole.subrange(pos as int, whole.len() as int), !drop_idx(whole, pos as int),
    ensures purge_at(s, whole, pos) == seq![s[0]].add(purge_at(s.drop_first(), whole, pos + 1)),
{
    reveal_with_fuel(purge_at, 2);
}

proof fn purge_at_dropped(s: Seq<Stmt>, whole: Seq<Stmt>, pos: nat)
    requires s.len() > 0, s == whole.subrange(pos as int, whole.len() as int), drop_idx(whole, pos as int),
    ensures purge_at(s, whole, pos) == purge_at(s.drop_first(), whole, pos + 1),
{
    reveal_with_fuel(purge_at, 2);
}

// A vacuous universal over an empty index range.
proof fn clean_mid_vacuous(p: Seq<Stmt>, i: int, x: Name)
    requires 0 <= i && i + 1 < p.len(),
    ensures clean_mid(p, i, i + 1, x),
{
    assert forall|k: int| i < k && k < i + 1 implies !stmt_reads(p[k], x) && !is_observer(p[k]) by {
    };
}

// Basic: `x=5; x=6` drops the head (justifier j=1).
proof fn pin_basic_drop()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(5) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ]).len() == 1,
{
    let p = seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(5) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ];
    assert(p[0] == Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(5) });
    assert(p[1] == Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) });
    assert(should_drop(p, 0)) by {
        assert(is_scalar_store(p[0], 7nat));
        assert(expr_pure(assign_rhs(p[0])));
        assert(is_scalar_store(p[1], 7nat));
        clean_mid_vacuous(p, 0, 7nat);
        assert(clean_at(p, 1, 7nat));
    };
    assert(drop_idx(p, 0));
    subrange_self(p);
    purge_at_dropped(p, p, 0);
    assert(purge_at(p.drop_first(), p, 1) == seq![p[1]]) by {
        assert(!drop_idx(p, 1)) by {
            assert forall|j: int, x: Name| !(x == assign_target(p[1]) && 1 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 1, j, x) && clean_at(p, j, x)) by {
            };
        };
        reveal_with_fuel(purge_at, 3);
    };
}

// Bug-1 fix: `x=$((i++)); x=6` keeps both (ArithSE is impure, so the
// head is not even a candidate).
proof fn pin_bug1_se_rhs_kept()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::ArithSE { var: 3nat } },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ]).len() == 2,
{
    let p = seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::ArithSE { var: 3nat } },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ];
    assert(p[0] == Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::ArithSE { var: 3nat } });
    assert(p[1] == Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) });
    assert(!expr_pure(assign_rhs(p[0])));
    assert(!drop_idx(p, 0));
    assert(!drop_idx(p, 1)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[1]) && 1 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 1, j, x) && clean_at(p, j, x)) by {
        };
    };
    subrange_self(p);
    purge_at_kept(p, p, 0);
    assert(purge_at(p.drop_first(), p, 1) == seq![p[1]]) by {
        reveal_with_fuel(purge_at, 3);
    };
}

// Bug-1 control: pure arith `x=i+1; x=6` still drops the head.
proof fn pin_bug1_pure_arith_drops()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Add(Box::new(Expr::Var(3nat)), Box::new(Expr::Lit(1))) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ]).len() == 1,
{
    let p = seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Add(Box::new(Expr::Var(3nat)), Box::new(Expr::Lit(1))) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ];
    assert(expr_pure(Expr::Add(Box::new(Expr::Var(3nat)), Box::new(Expr::Lit(1))))) by {
        reveal_with_fuel(expr_pure, 4);
    };
    assert(assign_rhs(p[0]) == Expr::Add(Box::new(Expr::Var(3nat)), Box::new(Expr::Lit(1))));
    assert(expr_pure(assign_rhs(p[0])));
    assert(should_drop(p, 0)) by {
        assert(is_scalar_store(p[0], 7nat));
        assert(is_scalar_store(p[1], 7nat));
        clean_mid_vacuous(p, 0, 7nat);
        assert(clean_at(p, 1, 7nat));
    };
    subrange_self(p);
    purge_at_dropped(p, p, 0);
    assert(purge_at(p.drop_first(), p, 1) == seq![p[1]]) by {
        assert(!drop_idx(p, 1)) by {
            assert forall|j: int, x: Name| !(x == assign_target(p[1]) && 1 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 1, j, x) && clean_at(p, j, x)) by {
            };
        };
        reveal_with_fuel(purge_at, 3);
    };
}

// Bug-2 fix: `x=5; arr[x]=1; x=6` keeps all (the index read stops the scan).
proof fn pin_bug2_idx_read_keeps()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(5) },
        Stmt::Assign { target: 9nat, idx: seq![Expr::Var(7nat)], rhs: Expr::Lit(1) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ]).len() == 3,
{
    let p = seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(5) },
        Stmt::Assign { target: 9nat, idx: seq![Expr::Var(7nat)], rhs: Expr::Lit(1) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ];
    assert(p[1] == Stmt::Assign { target: 9nat, idx: seq![Expr::Var(7nat)], rhs: Expr::Lit(1) });
    // The middle store reads 7 through its index...
    assert(seq![Expr::Var(7nat)][0] == Expr::Var(7nat));
    assert(assign_idx(p[1]) == seq![Expr::Var(7nat)]);
    assert(args_reads(seq![Expr::Var(7nat)], 7nat)) by {
        reveal_with_fuel(args_reads, 4);
    };
    assert(args_reads(assign_idx(p[1]), 7nat));
    assert(stmt_kind(p[1]) == 0);
    assert(stmt_reads(p[1], 7nat));
    // ...so j=2's middle is unclean, and j=1 is not a scalar store to 7.
    assert(!drop_idx(p, 0)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[0]) && 0 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 0, j, x) && clean_at(p, j, x)) by {
            if x == assign_target(p[0]) && 0 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 0, j, x) && clean_at(p, j, x) {
                assert(x == 7nat);
                assert(stmt_reads(p[1], 7nat));
                if j == 1 {
                    // The justifier itself reads 7: clean_at fails.
                    assert(!clean_at(p, 1, x));
                } else {
                    // A later justifier has the read strictly inside:
                    // witness k=1 falsifies the middle.
                    assert(1 < j);
                    assert(exists|k: int| 0 < k && k < j && stmt_reads(p[k], x)) by {
                        assert(0 < 1 && 1 < j && stmt_reads(p[1], x));
                    };
                    assert(!clean_mid(p, 0, j, x));
                }
            }
        };
    };
    assert(!drop_idx(p, 1)) by {
        // The middle store is indexed: not a scalar store at all.
        assert forall|j: int, x: Name| !(x == assign_target(p[1]) && 1 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 1, j, x) && clean_at(p, j, x)) by {
            if x == assign_target(p[1]) && 1 < j && j < p.len() as int {
                assert(x == 9nat);
                assert(j == 2);
                assert(p[j] == p[2]);
            }
        };
    };
    assert(!drop_idx(p, 2)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[2]) && 2 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 2, j, x) && clean_at(p, j, x)) by {
        };
    };
    subrange_self(p);
    purge_at_kept(p, p, 0);
    assert(purge_at(p.drop_first(), p, 1) == p.drop_first()) by {
        reveal_with_fuel(purge_at, 4);
    };
}

// Bug-3 fix: `x=1; y=eval('$x'); x=2` keeps all (Str-embedded names
// are reads; the middle Assign is not an observer but its read stops
// the scan anyway).
proof fn pin_bug3_srefs_read_keeps()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(1) },
        Stmt::Assign { target: 8nat, idx: Seq::empty(), rhs: Expr::Call { f: 4nat, args: Seq::empty(), srefs: seq![7nat] } },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(2) },
    ]).len() == 3,
{
    let p = seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(1) },
        Stmt::Assign { target: 8nat, idx: Seq::empty(), rhs: Expr::Call { f: 4nat, args: Seq::empty(), srefs: seq![7nat] } },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(2) },
    ];
    assert(p[1] == Stmt::Assign { target: 8nat, idx: Seq::empty(), rhs: Expr::Call { f: 4nat, args: Seq::empty(), srefs: seq![7nat] } });
    assert(seq![7nat][0] == 7nat);
    assert(seq_has(seq![7nat], 7nat));
    assert(assign_rhs(p[1]) == Expr::Call { f: 4nat, args: Seq::empty(), srefs: seq![7nat] });
    assert(expr_reads(assign_rhs(p[1]), 7nat));
    assert(stmt_kind(p[1]) == 0);
    assert(stmt_reads(p[1], 7nat));
    assert(!drop_idx(p, 0)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[0]) && 0 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 0, j, x) && clean_at(p, j, x)) by {
            if x == assign_target(p[0]) && 0 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 0, j, x) && clean_at(p, j, x) {
                assert(x == 7nat);
                assert(j == 2) by {
                    if j <= 1 {
                        assert(j == 1);
                        assert(p[j] == p[1]);
                        assert(p[1] == Stmt::Assign { target: 8nat, idx: Seq::empty(), rhs: Expr::Call { f: 4nat, args: Seq::empty(), srefs: seq![7nat] } });
                        assert(!is_scalar_store(p[1], 7nat));
                    }
                };
                assert(stmt_reads(p[1], 7nat));
                assert(!clean_mid(p, 0, j, x));
            }
        };
    };
    assert(!drop_idx(p, 1)) by {
        // The middle store's RHS is an impure Call: not a candidate.
        assert(!expr_pure(assign_rhs(p[1])));
    };
    assert(!drop_idx(p, 2)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[2]) && 2 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 2, j, x) && clean_at(p, j, x)) by {
        };
    };
    subrange_self(p);
    purge_at_kept(p, p, 0);
    assert(purge_at(p.drop_first(), p, 1) == p.drop_first()) by {
        reveal_with_fuel(purge_at, 4);
    };
}

// Observer: `x=5; call f; x=6` keeps the head.
proof fn pin_observer_keeps()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(5) },
        Stmt::CallStmt { f: 4nat, args: Seq::empty(), srefs: Seq::empty() },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ]).len() == 3,
{
    let p = seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(5) },
        Stmt::CallStmt { f: 4nat, args: Seq::empty(), srefs: Seq::empty() },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(6) },
    ];
    assert(p[1] == Stmt::CallStmt { f: 4nat, args: Seq::empty(), srefs: Seq::empty() });
    assert(is_observer(p[1]));
    assert(!drop_idx(p, 0)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[0]) && 0 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 0, j, x) && clean_at(p, j, x)) by {
            if x == assign_target(p[0]) && 0 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 0, j, x) && clean_at(p, j, x) {
                assert(x == 7nat);
                assert(j == 2) by {
                    if j <= 1 {
                        assert(j == 1);
                        assert(p[j] == p[1]);
                        assert(stmt_kind(p[1]) == 2);
                    }
                };
                assert(is_observer(p[1]));
                assert(!clean_mid(p, 0, j, x));
            }
        };
    };
    assert(!drop_idx(p, 1));
    assert(!drop_idx(p, 2)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[2]) && 2 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 2, j, x) && clean_at(p, j, x)) by {
        };
    };
    subrange_self(p);
    purge_at_kept(p, p, 0);
    assert(purge_at(p.drop_first(), p, 1) == p.drop_first()) by {
        reveal_with_fuel(purge_at, 4);
    };
}

// Cascade: `x=1; x=2; x=3` drops the first two.
proof fn pin_cascade_drops()
    ensures purge(seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(1) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(2) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(3) },
    ]).len() == 1,
{
    let p = seq![
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(1) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(2) },
        Stmt::Assign { target: 7nat, idx: Seq::empty(), rhs: Expr::Lit(3) },
    ];
    assert(should_drop(p, 0)) by {
        assert(is_scalar_store(p[0], 7nat));
        assert(expr_pure(assign_rhs(p[0])));
        assert(is_scalar_store(p[1], 7nat));
        clean_mid_vacuous(p, 0, 7nat);
        assert(clean_at(p, 1, 7nat));
    };
    assert(should_drop(p, 1)) by {
        assert(is_scalar_store(p[1], 7nat));
        assert(expr_pure(assign_rhs(p[1])));
        assert(is_scalar_store(p[2], 7nat));
        clean_mid_vacuous(p, 1, 7nat);
        assert(clean_at(p, 2, 7nat));
    };
    assert(!drop_idx(p, 2)) by {
        assert forall|j: int, x: Name| !(x == assign_target(p[2]) && 2 < j && j < p.len() as int && is_scalar_store(p[j], x) && clean_mid(p, 2, j, x) && clean_at(p, j, x)) by {
        };
    };
    subrange_self(p);
    purge_at_dropped(p, p, 0);
    assert(purge_at(p.drop_first(), p, 1) == seq![p[2]]) by {
        assert(drop_idx(p, 1));
        reveal_with_fuel(purge_at, 4);
    };
}

fn main() {}

}
