// Standalone model of copy-propagation's substitution soundness
// (sh2perl/src/transforms/copy_propagation.rs): the single-def scan, the
// def-before-read gate, and the read-site fold, over a store + observable-
// trace semantics. Proves the soundness obligation:
//
//   copy_sound: folding single-def literal reads preserves the final
//   store and the event trace, from any closed start store.
//
// Names are interned to nat (as in the sibling proofs). The observable
// effect is the event trace (prints, external calls); a folded read
// evaluates to the same value the variable holds, so the trace and the
// final store are unchanged.
//
// What is modelled and what is NOT (fidelity, checked against
// copy_propagation.rs):
//   - Assign (plain scalar target + Lit/Var RHS), Output, Other (opaque
//     statements that are never folded into and never define: the real
//     Subshell/Background/Capture refusal — a read inside one observes
//     out-of-order state, so the pass never folds there).
//   - The single-def table is a PARAMETER (defs: name -> (def index,
//     literal value)) with a soundness precondition tying it to the
//     program: the def site holds the literal and the write count is 1.
//     The real collect_defs/count_assigns machinery (builtin writes,
//     arith writes, loop headers, eval-disqualifies-all) is out of scope;
//     the model takes "exactly one literal write" as the sufficient
//     condition, exactly as the sibling dead-store-elim proof does.
//   - The def-before-read gate (d < i) is modelled exactly: a read at
//     position i folds only when its def executed strictly earlier. A
//     read before the def observes the unset default, never the literal.
//   - Copy defs (x=y), string-digit arith gates, param-const-fold,
//     AffixStrip folds, quoted-Interpolate defs: all out of scope (the
//     same single-def/literal core; the extensions reuse this gate).
//   - Flat statement sequences only; nested bodies are the same
//     induction presented per straight-line list.
use vstd::prelude::*;

verus! {

pub type Name = nat;
pub type Store = Map<Name, int>;

// A single-def literal entry: the unique write site and its value.
pub struct Def {
    pub idx: int,
    pub val: int,
}

pub type Defs = Map<Name, Def>;

// ---- Expressions ----

pub enum Expr {
    Lit(int),
    Var(Name),
}

pub enum Ev {
    Print(int),
    RunCall(Name),
}

pub open spec fn expr_reads(e: Expr, n: Name) -> bool {
    match e {
        Expr::Lit(_) => false,
        Expr::Var(m) => m == n,
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

pub open spec fn eval(e: Expr, s: Store) -> (int, Seq<Ev>) {
    match e {
        Expr::Lit(v) => (v, Seq::empty()),
        Expr::Var(n) => (lookup(s, n), Seq::empty()),
    }
}

// ---- Statements ----

pub enum Stmt {
    Assign { target: Name, rhs: Expr },
    Output { val: Expr },
    Other { reads: Seq<Name>, effect: Seq<Ev> },
}

pub open spec fn stmt_kind(st: Stmt) -> nat {
    match st {
        Stmt::Assign { .. } => 0,
        Stmt::Output { .. } => 1,
        Stmt::Other { .. } => 2,
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

pub open spec fn stmt_reads(st: Stmt, n: Name) -> bool {
    if stmt_kind(st) == 0 {
        expr_reads(assign_rhs(st), n)
    } else if stmt_kind(st) == 1 {
        expr_reads(output_val(st), n)
    } else {
        seq_has(other_reads(st), n)
    }
}

pub open spec fn stmt_writes(st: Stmt, n: Name) -> bool {
    if stmt_kind(st) == 0 {
        assign_target(st) == n
    } else {
        false
    }
}

// A literal definition site: `n = v` with a literal RHS.
pub open spec fn is_lit_assign(st: Stmt, n: Name, v: int) -> bool {
    match st {
        Stmt::Assign { target, rhs } => target == n && rhs == Expr::Lit(v),
        _ => false,
    }
}

// ---- Single-def census ----

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

// The def table is sound: every entry points at the program's unique
// literal write of that name.
pub open spec fn defs_sound(p: Seq<Stmt>, defs: Defs) -> bool {
    forall|n: Name|
        defs.dom().contains(n) ==> {
            let d = defs[n];
            0 <= d.idx && d.idx < p.len() && is_lit_assign(p[d.idx], n, d.val)
                && count_writes(p, n) == 1
        }
}

// A read at position i folds iff its def executed strictly earlier.
pub open spec fn foldable(defs: Defs, i: int, n: Name) -> bool {
    defs.dom().contains(n) && defs[n].idx < i
}

// ---- The fold ----

pub open spec fn fold_expr(e: Expr, defs: Defs, i: int) -> Expr {
    match e {
        Expr::Lit(_) => e,
        Expr::Var(n) => {
            if foldable(defs, i, n) {
                Expr::Lit(defs[n].val)
            } else {
                e
            }
        },
    }
}

pub open spec fn fold_stmt(st: Stmt, defs: Defs, i: int) -> Stmt {
    if stmt_kind(st) == 0 {
        Stmt::Assign { target: assign_target(st), rhs: fold_expr(assign_rhs(st), defs, i) }
    } else if stmt_kind(st) == 1 {
        Stmt::Output { val: fold_expr(output_val(st), defs, i) }
    } else {
        st
    }
}

pub open spec fn fold_suffix(s: Seq<Stmt>, whole: Seq<Stmt>, defs: Defs, k: int) -> Seq<Stmt>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        seq![fold_stmt(s[0], defs, k)].add(fold_suffix(s.drop_first(), whole, defs, k + 1))
    }
}

pub open spec fn fold_prog(p: Seq<Stmt>, defs: Defs) -> Seq<Stmt> {
    fold_suffix(p, p, defs, 0)
}

// ---- Execution ----

pub open spec fn step(st: Stmt, s: Store) -> (Store, Seq<Ev>) {
    if stmt_kind(st) == 0 {
        let r = eval(assign_rhs(st), s);
        (s.insert(assign_target(st), r.0), r.1)
    } else if stmt_kind(st) == 1 {
        let r = eval(output_val(st), s);
        (s, r.1.add(seq![Ev::Print(r.0)]))
    } else {
        (s, other_effect(st))
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

pub open spec fn reads_some(p: Seq<Stmt>, n: Name) -> bool {
    exists|i: int| 0 <= i < p.len() && stmt_reads(p[i], n)
}

// Closed: every structurally-read name is bound.
pub open spec fn closed_for(p: Seq<Stmt>, s: Store) -> bool {
    forall|n: Name| reads_some(p, n) ==> s.dom().contains(n)
}

// Full store agreement is not needed: the induction runs both sides
// from the SAME store (the fold preserves the run exactly), so literal
// equality threads through. (Pointwise `agree` cannot prove Map `==`
// anyway: `{1: 0}` and `{}` agree everywhere but differ as Maps.)

// Every already-executed def holds its literal value.
pub open spec fn prefix_ok(whole: Seq<Stmt>, defs: Defs, k: int, cur: Store) -> bool {
    forall|n: Name|
        defs.dom().contains(n) && defs[n].idx < k ==> lookup(cur, n) == defs[n].val
}

// s is the suffix of whole starting at k.
pub open spec fn suffix_at(s: Seq<Stmt>, whole: Seq<Stmt>, k: int) -> bool {
    s.len() as int + k == whole.len() as int && (forall|i: int|
        0 <= i < s.len() ==> s[i] == whole[k + i])
}

// ---- Unfold / census lemmas ----

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

proof fn fold_step(s: Seq<Stmt>, whole: Seq<Stmt>, defs: Defs, k: int)
    requires s.len() > 0,
    ensures fold_suffix(s, whole, defs, k) == seq![fold_stmt(s[0], defs, k)].add(
        fold_suffix(s.drop_first(), whole, defs, k + 1),
    ),
{
    reveal_with_fuel(fold_suffix, 2);
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

// One write site: count >= 1.
proof fn write_index_ge_one(p: Seq<Stmt>, n: Name, i: int)
    requires 0 <= i < p.len(), stmt_writes(p[i], n),
    ensures count_writes(p, n) >= 1,
    decreases p.len(),
{
    if i == 0 {
        count_step(p, n);
    } else {
        count_step(p, n);
        assert(p.drop_first()[i - 1] == p[i]);
        write_index_ge_one(p.drop_first(), n, i - 1);
    }
}

// Two distinct write sites: count >= 2 (so a def entry's count == 1
// rules out any second writer).
proof fn two_write_indices(p: Seq<Stmt>, n: Name, i: int, j: int)
    requires 0 <= i < j < p.len(), stmt_writes(p[i], n), stmt_writes(p[j], n),
    ensures count_writes(p, n) >= 2,
    decreases p.len(),
{
    count_step(p, n);
    if i == 0 {
        assert(p.drop_first()[j - 1] == p[j]);
        write_index_ge_one(p.drop_first(), n, j - 1);
    } else {
        assert(p.drop_first()[i - 1] == p[i]);
        assert(p.drop_first()[j - 1] == p[j]);
        two_write_indices(p.drop_first(), n, i - 1, j - 1);
    }
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

// Stepping preserves closedness: steps only insert bindings.
proof fn step_closed(st: Stmt, whole: Seq<Stmt>, s: Store)
    requires closed_for(whole, s),
    ensures closed_for(whole, step(st, s).0),
{
    if stmt_kind(st) == 0 {
        let r = eval(assign_rhs(st), s);
        insert_grows(s, assign_target(st), r.0);
        assert(step(st, s).0 == s.insert(assign_target(st), r.0));
    } else {
    }
}

// A non-writing step preserves every binding.
proof fn step_frame(h: Stmt, cur: Store, n: Name)
    requires !stmt_writes(h, n),
    ensures lookup(step(h, cur).0, n) == lookup(cur, n),
{
    if stmt_kind(h) == 0 {
        assert(assign_target(h) != n);
        lookup_insert_other(cur, assign_target(h), eval(assign_rhs(h), cur).0, n);
        assert(step(h, cur).0 == cur.insert(assign_target(h), eval(assign_rhs(h), cur).0));
    } else {
    }
}

// The tail of a suffix is a suffix one position later.
proof fn suffix_tail(s: Seq<Stmt>, whole: Seq<Stmt>, k: int)
    requires suffix_at(s, whole, k), s.len() > 0,
    ensures suffix_at(s.drop_first(), whole, k + 1),
{
    assert forall|i: int| 0 <= i < s.drop_first().len() implies s.drop_first()[i] == whole[k + 1 + i] by {
        assert(s[i + 1] == whole[k + (i + 1)]);
    };
}

// A folded expression evaluates to the same value from the same
// store: a folded Var reads the def's literal, which prefix_ok says
// the store holds.
proof fn eval_fold_same(e: Expr, whole: Seq<Stmt>, defs: Defs, k: int, cur: Store)
    requires prefix_ok(whole, defs, k, cur),
    ensures eval(fold_expr(e, defs, k), cur) == eval(e, cur),
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(n) => {
            if foldable(defs, k, n) {
                assert(lookup(cur, n) == defs[n].val);
            } else {
            }
        },
    }
}

// A literal-assign witness pins the statement kind and parts. (Lets
// later proofs invert `is_lit_assign` without re-matching.)
proof fn lit_assign_parts(st: Stmt, n: Name, v: int)
    requires is_lit_assign(st, n, v),
    ensures stmt_kind(st) == 0 && assign_target(st) == n && assign_rhs(st) == Expr::Lit(v),
{
}

// A folded head step IS the original head step from the same store
// with executed defs in place — and the result preserves the prefix
// property one position later.
proof fn step_same(
    h: Stmt,
    whole: Seq<Stmt>,
    defs: Defs,
    k: int,
    cur: Store,
)
    requires
        defs_sound(whole, defs),
        prefix_ok(whole, defs, k, cur),
        0 <= k < whole.len(),
        h == whole[k],
    ensures
        step(fold_stmt(h, defs, k), cur) == step(h, cur),
        prefix_ok(whole, defs, k + 1, step(fold_stmt(h, defs, k), cur).0),
{
    // The original head always preserves the prefix.
    prefix_step(whole, defs, k, cur, h);
    let f = fold_stmt(h, defs, k);
    if stmt_kind(h) == 0 {
        let t = assign_target(h);
        let r = assign_rhs(h);
        assert(f == Stmt::Assign { target: t, rhs: fold_expr(r, defs, k) });
        assert(stmt_kind(f) == 0);
        assert(assign_target(f) == t && assign_rhs(f) == fold_expr(r, defs, k));
        eval_fold_same(r, whole, defs, k, cur);
        assert(step(f, cur) == step(h, cur));
        // The folded head preserves the prefix: any earlier def is
        // untouched (a second writer would contradict count == 1),
        // and the head itself establishes its own def.
        assert forall|n: Name|
            defs.dom().contains(n) && defs[n].idx < k + 1
            implies lookup(step(f, cur).0, n) == defs[n].val by {
            if defs[n].idx < k {
                if n == t {
                    assert(stmt_writes(h, n));
                    assert(stmt_writes(whole[defs[n].idx], n)) by {
                        assert(is_lit_assign(whole[defs[n].idx], n, defs[n].val));
                    };
                    two_write_indices(whole, n, defs[n].idx, k);
                } else {
                    assert(step(f, cur).0 == cur.insert(t, eval(fold_expr(r, defs, k), cur).0));
                    lookup_insert_other(cur, t, eval(fold_expr(r, defs, k), cur).0, n);
                }
            } else {
                assert(defs[n].idx == k);
                assert(is_lit_assign(h, n, defs[n].val));
                lit_assign_parts(h, n, defs[n].val);
                assert(t == n && r == Expr::Lit(defs[n].val));
                assert(step(f, cur).0 == cur.insert(t, defs[n].val));
                lookup_insert_self(cur, t, defs[n].val);
            }
        };
    } else if stmt_kind(h) == 1 {
        let v = output_val(h);
        assert(f == Stmt::Output { val: fold_expr(v, defs, k) });
        assert(stmt_kind(f) == 1);
        assert(output_val(f) == fold_expr(v, defs, k));
        eval_fold_same(v, whole, defs, k, cur);
        assert(step(f, cur) == step(h, cur));
        assert(step(f, cur).0 == cur);
        // Outputs write nothing; no def can sit at k (the def site is
        // a literal assign, i.e. kind 0).
        assert forall|n: Name|
            defs.dom().contains(n) && defs[n].idx < k + 1
            implies lookup(step(f, cur).0, n) == defs[n].val by {
            if defs[n].idx < k {
            } else {
                assert(is_lit_assign(h, n, defs[n].val));
                lit_assign_parts(h, n, defs[n].val);
                assert(false);
            }
        };
    } else {
        assert(f == h);
        assert(step(f, cur) == step(h, cur));
        assert(step(f, cur).0 == cur);
        // Opaque statements write nothing; same no-def-at-k argument.
        assert forall|n: Name|
            defs.dom().contains(n) && defs[n].idx < k + 1
            implies lookup(step(f, cur).0, n) == defs[n].val by {
            if defs[n].idx < k {
            } else {
                assert(is_lit_assign(h, n, defs[n].val));
                lit_assign_parts(h, n, defs[n].val);
                assert(false);
            }
        };
    }
}

// Executing the head preserves prefix_ok one position later: other
// defs are untouched (a second writer would contradict count == 1),
// and the head itself establishes its own def.
proof fn prefix_step(
    whole: Seq<Stmt>,
    defs: Defs,
    k: int,
    cur: Store,
    h: Stmt,
)
    requires
        defs_sound(whole, defs),
        prefix_ok(whole, defs, k, cur),
        0 <= k < whole.len(),
        h == whole[k],
    ensures prefix_ok(whole, defs, k + 1, step(h, cur).0),
{
    assert forall|n: Name| defs.dom().contains(n) && defs[n].idx < k + 1 implies lookup(step(h, cur).0, n) == defs[n].val by {
        if defs[n].idx < k {
            if stmt_writes(h, n) {
                // h writes n and whole[defs[n].idx] writes n at an
                // earlier index: two writers, contradicting count == 1.
                assert(stmt_writes(whole[defs[n].idx], n)) by {
                    assert(is_lit_assign(whole[defs[n].idx], n, defs[n].val));
                };
                two_write_indices(whole, n, defs[n].idx, k);
            } else {
                step_frame(h, cur, n);
            }
        } else {
            assert(defs[n].idx == k);
            assert(is_lit_assign(h, n, defs[n].val));
            assert(h == Stmt::Assign { target: n, rhs: Expr::Lit(defs[n].val) });
            assert(step(h, cur).0 == cur.insert(n, defs[n].val));
            lookup_insert_self(cur, n, defs[n].val);
        }
    };
}

// THE OBLIGATION (single-store induction: the fold preserves the
// run exactly from the same start store).
proof fn copy_sound(
    s: Seq<Stmt>,
    whole: Seq<Stmt>,
    defs: Defs,
    k: int,
    cur: Store,
)
    requires
        suffix_at(s, whole, k),
        0 <= k,
        defs_sound(whole, defs),
        closed_for(whole, cur),
        prefix_ok(whole, defs, k, cur),
    ensures run(fold_suffix(s, whole, defs, k), cur) == run(s, cur),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        // Index facts: the head is a whole-program position in bounds.
        assert(0 <= k < whole.len()) by {
            assert(suffix_at(s, whole, k));
            assert(s.len() as int + k == whole.len() as int);
        };
        assert(s[0] == whole[k]) by {
            assert(suffix_at(s, whole, k));
        };
        // The folded head IS the original head as a step; both sides
        // then run the same tail from the same store.
        step_same(s[0], whole, defs, k, cur);
        let f = fold_stmt(s[0], defs, k);
        fold_step(s, whole, defs, k);
        run_cons(f, fold_suffix(s.drop_first(), whole, defs, k + 1), cur);
        run_cons(s[0], s.drop_first(), cur);
        step_closed(f, whole, cur);
        step_closed(s[0], whole, cur);
        suffix_tail(s, whole, k);
        copy_sound(
            s.drop_first(),
            whole,
            defs,
            k + 1,
            step(f, cur).0,
        );
    }
}

// THE TOP-LEVEL OBLIGATION: folding preserves the run from any closed
// start store.
proof fn copy_sound_top(p: Seq<Stmt>, defs: Defs, init: Store)
    requires defs_sound(p, defs), closed_for(p, init),
    ensures run(fold_prog(p, defs), init) == run(p, init),
{
    assert(prefix_ok(p, defs, 0, init)) by {
        assert forall|n: Name| defs.dom().contains(n) && defs[n].idx < 0 implies lookup(init, n) == defs[n].val by {
        };
    };
    assert(suffix_at(p, p, 0)) by {
        assert forall|i: int| 0 <= i < p.len() implies #[trigger] p[i] == p[0 + i] by {
        };
    };
    copy_sound(p, p, defs, 0, init);
}

// ---- Fidelity pins (mirror copy_propagation.rs's own tests) ----

// `x=5; echo $x` — the read folds to the literal.
proof fn pin_fold_after_def()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
            Stmt::Output { val: Expr::Var(7nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 0, val: 5 }),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Lit(5) },
    ],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 0, val: 5 });
    assert(defs_sound(p, defs)) by {
        assert(count_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            reveal_with_fuel(count_writes, 2);
        };
    };
    assert(fold_suffix(p, p, defs, 0) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Lit(5) },
    ]) by {
        reveal_with_fuel(fold_suffix, 4);
    };
}

// `echo $x; x=5` — the read BEFORE the def must NOT fold.
proof fn pin_refuse_before_def()
    ensures fold_prog(
        seq![
            Stmt::Output { val: Expr::Var(7nat) },
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        ],
        Map::empty().insert(7nat, Def { idx: 1, val: 5 }),
    ) == seq![
        Stmt::Output { val: Expr::Var(7nat) },
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
    ],
{
    let p = seq![
        Stmt::Output { val: Expr::Var(7nat) },
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 1, val: 5 });
    assert(defs_sound(p, defs)) by {
        assert(count_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            count_step(p.drop_first(), 7nat);
            reveal_with_fuel(count_writes, 2);
        };
    };
    assert(!foldable(defs, 0, 7nat));
    assert(fold_suffix(p, p, defs, 0) == p) by {
        reveal_with_fuel(fold_suffix, 4);
    };
}

// `x=5; x=6; echo $x` — two defs: no sound table entry exists.
proof fn pin_refuse_multi_def()
    ensures count_writes(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(6) },
        ],
        7nat,
    ) == 2,
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(6) },
    ];
    count_step(p, 7nat);
    count_step(p.drop_first(), 7nat);
    reveal_with_fuel(count_writes, 2);
}

// `x=5; y=$x; echo $y` — the copy `y=$x` is not itself a literal def,
// so reads of y stay unfolded (y's RHS read-of-x still folds in place).
proof fn pin_copy_def_not_folded()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
            Stmt::Assign { target: 8nat, rhs: Expr::Var(7nat) },
            Stmt::Output { val: Expr::Var(8nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 0, val: 5 }),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Assign { target: 8nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Var(8nat) },
    ],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Assign { target: 8nat, rhs: Expr::Var(7nat) },
        Stmt::Output { val: Expr::Var(8nat) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 0, val: 5 });
    assert(defs_sound(p, defs)) by {
        assert(count_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            count_step(p.drop_first(), 7nat);
            count_step(p.drop_first().drop_first(), 7nat);
            reveal_with_fuel(count_writes, 2);
        };
    };
    assert(!defs.dom().contains(8nat));
    assert(fold_suffix(p, p, defs, 0) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Assign { target: 8nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Var(8nat) },
    ]) by {
        reveal_with_fuel(fold_suffix, 6);
    };
}

// An opaque statement is never folded into (the Background/Capture
// refusal): even its reads of foldable vars stay as-is.
proof fn pin_other_opaque()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
            Stmt::Other { reads: seq![7nat], effect: seq![Ev::Print(1)] },
        ],
        Map::empty().insert(7nat, Def { idx: 0, val: 5 }),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Other { reads: seq![7nat], effect: seq![Ev::Print(1)] },
    ],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Other { reads: seq![7nat], effect: seq![Ev::Print(1)] },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 0, val: 5 });
    assert(defs_sound(p, defs)) by {
        assert(count_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            reveal_with_fuel(count_writes, 2);
        };
    };
    assert(fold_suffix(p, p, defs, 0) == p) by {
        reveal_with_fuel(fold_suffix, 4);
    };
}

// `x=5; echo $x $x` — both reads fold (the fold set is not one-shot).
proof fn pin_fold_both_reads()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
            Stmt::Output { val: Expr::Var(7nat) },
            Stmt::Output { val: Expr::Var(7nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 0, val: 5 }),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Lit(5) },
        Stmt::Output { val: Expr::Lit(5) },
    ],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Var(7nat) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 0, val: 5 });
    assert(defs_sound(p, defs)) by {
        assert(count_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            count_step(p.drop_first(), 7nat);
            count_step(p.drop_first().drop_first(), 7nat);
            reveal_with_fuel(count_writes, 2);
        };
    };
    assert(fold_suffix(p, p, defs, 0) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Lit(5) },
        Stmt::Output { val: Expr::Lit(5) },
        Stmt::Output { val: Expr::Lit(5) },
    ]) by {
        reveal_with_fuel(fold_suffix, 6);
    };
}

fn main() {}

}
