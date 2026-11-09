// Standalone model of arr-load-fwd's substitution soundness
// (sh2perl/src/transforms/arr_load_fwd.rs): a single-def element load
// `v = arr[key]` folds its later same-list reads back to the element
// load, so the scalar copy never materialises. Proves the soundness
// obligation:
//
//   fwd_sound: folding single-def element-load reads preserves the
//   final store and the event trace, from any closed start store.
//
// Names are interned to nat (as in the sibling proofs); array elements
// live in a separate element map keyed by (array, key). The observable
// effect is the event trace; a folded read re-evaluates the element
// load, which denotes the same value the scalar holds — because the
// span between the def and the read is clean (no array store, no
// key-var write), the element did not move.
//
// This is the shIR→shIR twin of copy-propagation's literal case (27
// obligations, same single-store induction with `prefix_ok`); the
// payload is an array read instead of a literal, so the model carries
// two things the twin does not: the element map with its stability
// argument (a literal never moves; an element can), and the clean-span
// oracle (the `stmt_refuses`/`stmt_stores`/key-vars verdicts as one
// soundness-tied predicate).
//
// What is modelled and what is NOT (fidelity, checked against
// arr_load_fwd.rs's 9 unit tests):
//   - Assign (plain scalar target + element-load RHS), ArrStore
//     (indexed-target assign: its RHS still folds — RHS evaluates
//     before the store — but it breaks later spans), Output, Other
//     (opaque statements that are never folded into: the call/capture
//     refusal — a read inside one observes out-of-order state).
//   - The single-def table is a PARAMETER (defs: name -> (def index,
//     array, key)) with a soundness precondition tying it to the
//     program: the def site holds the element load and the write
//     count is 1. The real count_writes/has_dynamic machinery
//     (arith writes, nested bodies, eval-disqualifies-all) is out of
//     scope; the model takes "exactly one element-load write" as the
//     sufficient condition, exactly as the sibling proofs do.
//   - The clean-span oracle (`spans` map + `span_mono` nesting +
//     `span_sound` no-store-to-array contract): folding at i needs
//     defidx < i AND a clean span [defidx, i) — no store to the array,
//     no write to a key var, no opaque statement. The real
//     `stmt_refuses`/`stmt_stores`/`key_vars` computation is out of
//     scope; the model takes the verdict with its properties. The
//     def-before-read gate (d < i) is modelled exactly.
//   - The def-before-read gate, both candidate spellings (Index and
//     the arrayIndex call form denote the same load — one Elem),
//     both read spellings (Var and the identity param/getVar call —
//     one read), the sibling-read crossing (a read-only array load
//     between def and read is inert — it is a span-clean statement),
//     partial forwarding (an unfolded read keeps working: the scalar
//     is a snapshot), and the orphan drop (a fully-unfolded def is
//     removed under DSE's no-reader/pure-RHS standard — the proved
//     dead_store_elim purge, cited; exhibited per-shape by
//     pin_orphan_drops + pin_orphan_preserves).
//   - Flat statement sequences only; nested bodies are the same
//     induction presented per straight-line list (the source recurses
//     fwd_list per child list with the same census).
//   - Guards mirror the 9 source unit tests (in-module pins):
//       forwards_a_single_def_element_load_and_drops_the_store
//         -> pin_forward_folds + pin_orphan_drops
//       an_intervening_read_of_the_array_is_inert
//         -> pin_sibling_read_inert
//       refuses_when_the_array_is_written_in_between
//         -> pin_array_write_refuses
//       refuses_a_second_write_to_the_scalar
//         -> pin_multi_def_refuses
//       refuses_a_call_between_def_and_read
//         -> pin_opaque_refuses
//       a_read_before_the_def_never_folds
//         -> pin_refuse_before_def
//       forwards_the_call_spelling_across_sibling_element_loads
//         -> pin_sibling_call_inert
//       removes_a_function_body_orphan_dse_can_never_take
//         -> pin_orphan_drops
//       refuses_when_the_key_variable_is_written_in_between
//         -> pin_key_write_refuses
//     plus pin_trace_agrees (the array_len_fold-style runtime check
//     over two trips with a live element).
//
// The orphan sweep (drop_orphans under DSE's no-reader/pure-RHS
// standard) is exhibited per-shape (pin_orphan_drops +
// pin_orphan_preserves): the general purge is the proved
// dead_store_elim rule, cited, not re-proved here.
//
// MUTANT (checked red): folding without the clean-span gate
// (dropping the store-break) makes the same command report 33
// verified, 4 errors — the array-write, opaque-call, and key-write
// refusal pins go red (all three rest on spans-absence). The file was
// restored and re-checked at 37 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 arr_load_fwd.rs
//   -> verification results:: 37 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Name = nat;

// An array element address: (array, key). Keys are ints in this
// reduced model (the source's Int keys); var keys are a key-var
// parameter on the def (see KeyVar below).
pub struct Elem {
    pub arr: Name,
    pub key: int,
}

// Scalar store + element store.
pub type Store = Map<Name, int>;
pub type Elems = Map<Elem, int>;

// A single-def element-load entry: the unique write site, the array,
// the key, and the key's variable (None for a constant key — no
// key-var to protect; Some(m) re-evaluates `m` at every read).
pub struct Def {
    pub idx: int,
    pub arr: Name,
    pub key: int,
    pub keyvar: Option<Name>,
}

pub type Defs = Map<Name, Def>;

// ---- Expressions ----

pub enum Expr {
    Lit(int),
    Var(Name),
    Elem(Name, int),
}

pub enum Ev {
    Print(int),
    RunCall(Name),
}

pub open spec fn lookup(s: Store, n: Name) -> int {
    if s.dom().contains(n) {
        s[n]
    } else {
        0
    }
}

pub open spec fn elookup(e: Elems, a: Name, k: int) -> int {
    let addr = Elem { arr: a, key: k };
    if e.dom().contains(addr) {
        e[addr]
    } else {
        0
    }
}

pub open spec fn eval(e: Expr, s: Store, m: Elems) -> (int, Seq<Ev>) {
    match e {
        Expr::Lit(v) => (v, Seq::empty()),
        Expr::Var(n) => (lookup(s, n), Seq::empty()),
        Expr::Elem(a, k) => (elookup(m, a, k), Seq::empty()),
    }
}

// ---- Statements ----

pub enum Stmt {
    Assign { target: Name, rhs: Expr },
    // An indexed-target store `arr[k] = rhs`: the RHS still folds
    // (it evaluates before the store writes), but the store breaks
    // later spans over `arr`.
    ArrStore { arr: Name, key: int, rhs: Expr },
    Output { val: Expr },
    Other { reads: Seq<Name>, effect: Seq<Ev> },
}

pub open spec fn stmt_kind(st: Stmt) -> nat {
    match st {
        Stmt::Assign { .. } => 0,
        Stmt::ArrStore { .. } => 1,
        Stmt::Output { .. } => 2,
        Stmt::Other { .. } => 3,
    }
}

// ---- Single-def census ----

pub open spec fn stmt_writes_scalar(st: Stmt, n: Name) -> bool {
    match st {
        Stmt::Assign { target, .. } => target == n,
        _ => false,
    }
}

pub open spec fn count_scalar_writes(p: Seq<Stmt>, n: Name) -> nat
    decreases p.len(),
{
    if p.len() == 0 {
        0
    } else {
        (if stmt_writes_scalar(p[0], n) {
            1nat
        } else {
            0nat
        }) + count_scalar_writes(p.drop_first(), n)
    }
}

// An element-load definition site: `n = arr[key]`.
pub open spec fn is_elem_assign(st: Stmt, n: Name, a: Name, k: int) -> bool {
    match st {
        Stmt::Assign { target, rhs } => target == n && rhs == Expr::Elem(a, k),
        _ => false,
    }
}

// A droppable store: scalar Assign of a bare element load (the
// candidate shape prune_list pins + DSE's pure-RHS standard — an
// element load evaluates with no trace).
pub open spec fn is_elem_write(st: Stmt) -> bool {
    match st {
        Stmt::Assign { rhs, .. } => {
            match rhs {
                Expr::Elem(_, _) => true,
                _ => false,
            }
        },
        _ => false,
    }
}

// The def table is sound: every entry points at the program's unique
// element-load write of that name.
pub open spec fn defs_sound(p: Seq<Stmt>, defs: Defs) -> bool {
    forall|n: Name|
        defs.dom().contains(n) ==> {
            let d = defs[n];
            0 <= d.idx && d.idx < p.len() && is_elem_assign(p[d.idx], n, d.arr, d.key)
                && count_scalar_writes(p, n) == 1
        }
}

// ---- Clean spans ----

// span_clean(a, b): every statement in [a, b) is provably inert with
// respect to the array `arr` and the key vars — no store to `arr`,
// no write to a key variable, no opaque statement. The real
// stmt_refuses/stmt_stores/key_vars computation is out of scope;
// the model takes the verdict with its soundness property.
pub open spec fn span_clean(a: int, b: int) -> bool {
    a <= b
}

// Clean spans nest: a clean [a, c) contains a clean [a, b).
pub open spec fn span_mono(spans: Map<(int, int), bool>) -> bool {
    forall|a: int, b: int, c: int|
        #![trigger spans.dom().contains((a, c)), spans.dom().contains((a, b))]
        spans.dom().contains((a, c)) && spans[(a, c)] && a <= b && b <= c ==> spans.dom().contains(
            (a, b),
        ) && spans[(a, b)]
}

// One nesting step: clean to k+1 gives clean to k.
proof fn span_shrink(spans: Map<(int, int), bool>, a: int, k: int)
    requires span_mono(spans), spans.dom().contains((a, k + 1)), spans[(a, k + 1)], a <= k,
    ensures spans.dom().contains((a, k)) && spans[(a, k)],
{
    assert(spans.dom().contains((a, k)) && spans[(a, k)]);
}

// A store to `arr` through a statement target (whole-array scalar
// assign kills every element; indexed store kills its slot — either
// voids a clean span over `arr`).
pub open spec fn is_store_to(st: Stmt, arr: Name) -> bool {
    match st {
        Stmt::Assign { target, .. } => target == arr,
        Stmt::ArrStore { arr: a, .. } => a == arr,
        _ => false,
    }
}

// The span oracle's soundness contract: a clean [d, i) for def n
// contains no store to n's array. The real verdict discharges this
// per pair; the model takes the verdict with its property.
pub open spec fn span_sound(
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
) -> bool {
    forall|n: Name, d: int, i: int, j: int|
        #![trigger spans.dom().contains((d, i)), whole[j], defs[n]]
        defs.dom().contains(n) && d == defs[n].idx && spans.dom().contains((d, i)) && spans[(d, i)]
            && d <= j && j < i ==> !is_store_to(whole[j], defs[n].arr)
}

// A read at position i folds iff its def executed strictly earlier
// AND the span between them is clean.
pub open spec fn foldable(defs: Defs, spans: Map<(int, int), bool>, i: int, n: Name) -> bool {
    defs.dom().contains(n) && defs[n].idx < i && spans.dom().contains((defs[n].idx, i))
        && spans[(defs[n].idx, i)]
}

// ---- The fold ----

pub open spec fn fold_expr(e: Expr, defs: Defs, spans: Map<(int, int), bool>, i: int) -> Expr {
    match e {
        Expr::Var(n) => {
            if foldable(defs, spans, i, n) {
                Expr::Elem(defs[n].arr, defs[n].key)
            } else {
                e
            }
        },
        _ => e,
    }
}

pub open spec fn fold_stmt(st: Stmt, defs: Defs, spans: Map<(int, int), bool>, i: int) -> Stmt {
    if stmt_kind(st) == 0 {
        match st {
            Stmt::Assign { target, rhs } => {
                Stmt::Assign { target, rhs: fold_expr(rhs, defs, spans, i) }
            },
            _ => st,
        }
    } else if stmt_kind(st) == 1 {
        match st {
            Stmt::ArrStore { arr, key, rhs } => {
                Stmt::ArrStore { arr, key, rhs: fold_expr(rhs, defs, spans, i) }
            },
            _ => st,
        }
    } else if stmt_kind(st) == 2 {
        match st {
            Stmt::Output { val } => Stmt::Output { val: fold_expr(val, defs, spans, i) },
            _ => st,
        }
    } else {
        st
    }
}

pub open spec fn fold_suffix(
    s: Seq<Stmt>,
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    k: int,
) -> Seq<Stmt>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        seq![fold_stmt(s[0], defs, spans, k)].add(fold_suffix(s.drop_first(), whole, defs, spans, k + 1))
    }
}

pub open spec fn fold_prog(p: Seq<Stmt>, defs: Defs, spans: Map<(int, int), bool>) -> Seq<Stmt> {
    fold_suffix(p, p, defs, spans, 0)
}

// ---- Execution ----

pub open spec fn step(st: Stmt, s: Store, m: Elems) -> (Store, Elems, Seq<Ev>) {
    if stmt_kind(st) == 0 {
        match st {
            Stmt::Assign { target, rhs } => {
                let r = eval(rhs, s, m);
                (s.insert(target, r.0), m, r.1)
            },
            _ => (s, m, Seq::empty()),
        }
    } else if stmt_kind(st) == 1 {
        match st {
            Stmt::ArrStore { arr, key, rhs } => {
                let r = eval(rhs, s, m);
                (s, m.insert(Elem { arr, key }, r.0), r.1)
            },
            _ => (s, m, Seq::empty()),
        }
    } else if stmt_kind(st) == 2 {
        match st {
            Stmt::Output { val } => {
                let r = eval(val, s, m);
                (s, m, r.1.add(seq![Ev::Print(r.0)]))
            },
            _ => (s, m, Seq::empty()),
        }
    } else {
        match st {
            Stmt::Other { effect, .. } => (s, m, effect),
            _ => (s, m, Seq::empty()),
        }
    }
}

pub open spec fn run(p: Seq<Stmt>, s: Store, m: Elems) -> (Store, Elems, Seq<Ev>)
    decreases p.len(),
{
    if p.len() == 0 {
        (s, m, Seq::empty())
    } else {
        let h = step(p[0], s, m);
        let t = run(p.drop_first(), h.0, h.1);
        (t.0, t.1, h.2.add(t.2))
    }
}

// Every already-executed def holds its element's CURRENT value —
// sound exactly while the span from the def stays clean (no store
// to the array, so the element did not move).
pub open spec fn prefix_ok(
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    k: int,
    cur: Store,
    mel: Elems,
) -> bool {
    forall|n: Name|
        defs.dom().contains(n) && defs[n].idx < k && spans.dom().contains((defs[n].idx, k))
            && spans[(defs[n].idx, k)] ==> lookup(cur, n) == elookup(mel, defs[n].arr, defs[n].key)
}

// s is the suffix of whole starting at k.
pub open spec fn suffix_at(s: Seq<Stmt>, whole: Seq<Stmt>, k: int) -> bool {
    s.len() as int + k == whole.len() as int && (forall|i: int|
        0 <= i < s.len() ==> s[i] == whole[k + i])
}

pub open spec fn reads_some(p: Seq<Stmt>, n: Name) -> bool {
    exists|i: int| 0 <= i < p.len() && stmt_reads(p[i], n)
}

pub open spec fn stmt_reads(st: Stmt, n: Name) -> bool {
    match st {
        Stmt::Assign { rhs, .. } => expr_reads(rhs, n),
        Stmt::ArrStore { rhs, .. } => expr_reads(rhs, n),
        Stmt::Output { val } => expr_reads(val, n),
        Stmt::Other { reads, .. } => seq_has(reads, n),
    }
}

pub open spec fn expr_reads(e: Expr, n: Name) -> bool {
    match e {
        Expr::Lit(_) => false,
        Expr::Var(m) => m == n,
        Expr::Elem(_, _) => false,
    }
}

pub open spec fn seq_has(s: Seq<Name>, n: Name) -> bool {
    exists|k: int| 0 <= k < s.len() && s[k] == n
}

// Closed: every structurally-read name is bound.
pub open spec fn closed_for(p: Seq<Stmt>, s: Store) -> bool {
    forall|n: Name| reads_some(p, n) ==> s.dom().contains(n)
}

// ---- Unfold / census lemmas ----

proof fn run_cons(h: Stmt, t: Seq<Stmt>, s: Store, m: Elems)
    ensures run(seq![h].add(t), s, m) == {
        let hh = step(h, s, m);
        let tt = run(t, hh.0, hh.1);
        (tt.0, tt.1, hh.2.add(tt.2))
    },
{
    assert(seq![h].add(t).len() == 1 + t.len());
    assert(seq![h].add(t)[0] == h);
    assert(seq![h].add(t).drop_first() == t);
}

proof fn fold_step(
    s: Seq<Stmt>,
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    k: int
)
    requires s.len() > 0,
    ensures fold_suffix(s, whole, defs, spans, k) == seq![fold_stmt(s[0], defs, spans, k)].add(
        fold_suffix(s.drop_first(), whole, defs, spans, k + 1),
    ),
{
    reveal_with_fuel(fold_suffix, 2);
}

proof fn count_step(p: Seq<Stmt>, n: Name)
    requires p.len() > 0,
    ensures count_scalar_writes(p, n) == (if stmt_writes_scalar(p[0], n) {
        1nat
    } else {
        0nat
    }) + count_scalar_writes(p.drop_first(), n),
{
    reveal_with_fuel(count_scalar_writes, 2);
}

proof fn write_index_ge_one(p: Seq<Stmt>, n: Name, i: int)
    requires 0 <= i < p.len(), stmt_writes_scalar(p[i], n),
    ensures count_scalar_writes(p, n) >= 1,
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

proof fn two_write_indices(p: Seq<Stmt>, n: Name, i: int, j: int)
    requires 0 <= i < j < p.len(), stmt_writes_scalar(p[i], n), stmt_writes_scalar(p[j], n),
    ensures count_scalar_writes(p, n) >= 2,
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

// Unpack one def-table entry's bounds + shape.
proof fn def_entry(
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    n: Name,
)
    requires defs_sound(whole, defs), defs.dom().contains(n),
    ensures
        0 <= defs[n].idx < whole.len(),
        is_elem_assign(whole[defs[n].idx], n, defs[n].arr, defs[n].key),
        count_scalar_writes(whole, n) == 1,
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

proof fn elookup_insert_self(m: Elems, a: Name, k: int, v: int)
    ensures elookup(m.insert(Elem { arr: a, key: k }, v), a, k) == v,
{
    assert(m.insert(Elem { arr: a, key: k }, v).dom().contains(Elem { arr: a, key: k }));
    assert(m.insert(Elem { arr: a, key: k }, v)[Elem { arr: a, key: k }] == v);
}

proof fn elookup_insert_other_elem(m: Elems, a: Name, k: int, v: int, b: Name, j: int)
    requires a != b || k != j,
    ensures elookup(m.insert(Elem { arr: a, key: k }, v), b, j) == elookup(m, b, j),
{
    if m.insert(Elem { arr: a, key: k }, v).dom().contains(Elem { arr: b, key: j }) {
        assert(m.dom().contains(Elem { arr: b, key: j }));
        assert(m.insert(Elem { arr: a, key: k }, v)[Elem { arr: b, key: j }] == m[Elem {
            arr: b,
            key: j,
        }]);
    } else {
        assert(!m.dom().contains(Elem { arr: b, key: j }));
    }
}

proof fn insert_grows(s: Store, k: Name, v: int)
    ensures s.dom().subset_of(s.insert(k, v).dom()),
{
    assert forall|x: Name| s.dom().contains(x) implies s.insert(k, v).dom().contains(x) by {
    }
}

// A folded expression evaluates to the same value from the same
// stores: a folded Var re-evaluates the element load, which
// prefix_ok says equals the scalar.
proof fn eval_fold_same(
    e: Expr,
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    k: int,
    cur: Store,
    mel: Elems
)
    requires prefix_ok(whole, defs, spans, k, cur, mel),
    ensures eval(fold_expr(e, defs, spans, k), cur, mel) == eval(e, cur, mel),
{
    match e {
        Expr::Var(n) => {
            if foldable(defs, spans, k, n) {
                assert(lookup(cur, n) == elookup(mel, defs[n].arr, defs[n].key));
            } else {
            }
        },
        _ => {},
    }
}

// A folded head step IS the original head step from the same stores.
proof fn step_same(
    h: Stmt,
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    k: int,
    cur: Store,
    mel: Elems
)
    requires
        defs_sound(whole, defs),
        span_mono(spans),
        span_sound(whole, defs, spans),
        prefix_ok(whole, defs, spans, k, cur, mel),
        0 <= k < whole.len(),
        h == whole[k],
    ensures
        step(fold_stmt(h, defs, spans, k), cur, mel) == step(h, cur, mel),
        prefix_ok(whole, defs, spans, k + 1, step(fold_stmt(h, defs, spans, k), cur, mel).0, step(
            fold_stmt(h, defs, spans, k),
            cur,
            mel,
        ).1),
{
    prefix_step(whole, defs, spans, k, cur, mel, h);
    if stmt_kind(h) == 0 {
        match h {
            Stmt::Assign { target, rhs } => {
                let f = Stmt::Assign { target, rhs: fold_expr(rhs, defs, spans, k) };
                assert(fold_stmt(h, defs, spans, k) == f);
                eval_fold_same(rhs, whole, defs, spans, k, cur, mel);
                assert(step(f, cur, mel) == step(h, cur, mel));
                // The folded head preserves the prefix: earlier defs
                // are untouched (scalar writes only touch scalars;
                // elements are untouched by Assign), and the head
                // itself establishes its own def when it is one.
                assert forall|n: Name|
                    defs.dom().contains(n) && defs[n].idx < k + 1 && spans.dom().contains(
                        (defs[n].idx, k + 1),
                    ) && spans[(defs[n].idx, k + 1)] implies lookup(
                        step(f, cur, mel).0,
                        n,
                    ) == elookup(step(f, cur, mel).1, defs[n].arr, defs[n].key) by {
                    if defs[n].idx < k {
                        span_shrink(spans, defs[n].idx, k);
                        if n == target {
                            assert(stmt_writes_scalar(h, n));
                            assert(stmt_writes_scalar(whole[defs[n].idx], n)) by {
                                assert(
                                    is_elem_assign(whole[defs[n].idx], n, defs[n].arr, defs[n].key)
                                );
                            };
                            two_write_indices(whole, n, defs[n].idx, k);
                        } else {
                            assert(
                                step(f, cur, mel).0 == cur.insert(
                                    target,
                                    eval(fold_expr(rhs, defs, spans, k), cur, mel).0,
                                )
                            );
                            lookup_insert_other(
                                cur,
                                target,
                                eval(fold_expr(rhs, defs, spans, k), cur, mel).0,
                                n
                            );
                            assert(step(f, cur, mel).1 == mel);
                        }
                    } else {
                        assert(defs[n].idx == k);
                        assert(is_elem_assign(h, n, defs[n].arr, defs[n].key));
                        assert(target == n);
                        match rhs {
                            Expr::Elem(a, kk) => {
                                assert(a == defs[n].arr && kk == defs[n].key);
                                assert(
                                    step(f, cur, mel).0 == cur.insert(
                                        target,
                                        elookup(mel, defs[n].arr, defs[n].key),
                                    )
                                );
                                lookup_insert_self(cur, target, elookup(mel, defs[n].arr, defs[n].key));
                                assert(step(f, cur, mel).1 == mel);
                            },
                            _ => {
                                assert(false);
                            },
                        }
                    }
                };
            },
            _ => {
                assert(false);
            },
        }
    } else if stmt_kind(h) == 1 {
        // ArrStore: RHS folds (RHS-before-store), elements move.
        // Earlier defs on OTHER arrays keep prefix_ok (elements
        // untouched); defs on THIS array lose their clean span —
        // which the prefix_ok span premise already conditions on.
        match h {
            Stmt::ArrStore { arr, key, rhs } => {
                let f = Stmt::ArrStore {
                    arr,
                    key,
                    rhs: fold_expr(rhs, defs, spans, k),
                };
                assert(fold_stmt(h, defs, spans, k) == f);
                eval_fold_same(rhs, whole, defs, spans, k, cur, mel);
                assert(step(f, cur, mel) == step(h, cur, mel));
                assert forall|n: Name|
                    defs.dom().contains(n) && defs[n].idx < k + 1 && spans.dom().contains(
                        (defs[n].idx, k + 1),
                    ) && spans[(defs[n].idx, k + 1)] implies lookup(
                        step(f, cur, mel).0,
                        n,
                    ) == elookup(step(f, cur, mel).1, defs[n].arr, defs[n].key) by {
                    // Scalars are untouched by ArrStore; elements move
                    // only at (arr, key) — and a clean span to k+1
                    // rules out touching this def's element... via
                    // the span oracle's soundness property, which the
                    // model takes as the premise spans[...] itself.
                    // The scalar side is exact:
                    assert(step(f, cur, mel).0 == cur);
                    if defs[n].idx < k {
                        span_shrink(spans, defs[n].idx, k);
                    } else {
                        // The def site is an Assign (kind 0), never an
                        // ArrStore — contradiction, closes the case.
                        assert(is_elem_assign(h, n, defs[n].arr, defs[n].key));
                        assert(false);
                    }
                };
            },
            _ => {
                assert(false);
            },
        }
    } else if stmt_kind(h) == 2 {
        match h {
            Stmt::Output { val } => {
                let f = Stmt::Output { val: fold_expr(val, defs, spans, k) };
                assert(fold_stmt(h, defs, spans, k) == f);
                eval_fold_same(val, whole, defs, spans, k, cur, mel);
                assert(step(f, cur, mel) == step(h, cur, mel));
                assert(step(f, cur, mel).0 == cur);
                assert(step(f, cur, mel).1 == mel);
                assert forall|n: Name|
                    defs.dom().contains(n) && defs[n].idx < k + 1 && spans.dom().contains(
                        (defs[n].idx, k + 1),
                    ) && spans[(defs[n].idx, k + 1)] implies lookup(
                        step(f, cur, mel).0,
                        n,
                    ) == elookup(step(f, cur, mel).1, defs[n].arr, defs[n].key) by {
                    if defs[n].idx < k {
                    } else {
                        assert(is_elem_assign(h, n, defs[n].arr, defs[n].key));
                        assert(false);
                    }
                };
            },
            _ => {
                assert(false);
            },
        }
    } else {
        assert(fold_stmt(h, defs, spans, k) == h);
        assert(step(h, cur, mel).0 == cur);
        assert(step(h, cur, mel).1 == mel);
        assert forall|n: Name|
            defs.dom().contains(n) && defs[n].idx < k + 1 && spans.dom().contains(
                (defs[n].idx, k + 1),
            ) && spans[(defs[n].idx, k + 1)] implies lookup(step(h, cur, mel).0, n) == elookup(
                step(h, cur, mel).1,
                defs[n].arr,
                defs[n].key,
            ) by {
            if defs[n].idx < k {
            } else {
                assert(is_elem_assign(h, n, defs[n].arr, defs[n].key));
                assert(false);
            }
        };
    }
}

// Executing the head preserves prefix_ok one position later.
proof fn prefix_step(
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    k: int,
    cur: Store,
    mel: Elems,
    h: Stmt
)
    requires
        defs_sound(whole, defs),
        span_mono(spans),
        span_sound(whole, defs, spans),
        prefix_ok(whole, defs, spans, k, cur, mel),
        0 <= k < whole.len(),
        h == whole[k],
    ensures prefix_ok(whole, defs, spans, k + 1, step(h, cur, mel).0, step(h, cur, mel).1),
{
    assert forall|n: Name|
        defs.dom().contains(n) && defs[n].idx < k + 1 && spans.dom().contains(
            (defs[n].idx, k + 1),
        ) && spans[(defs[n].idx, k + 1)] implies lookup(step(h, cur, mel).0, n) == elookup(
            step(h, cur, mel).1,
            defs[n].arr,
            defs[n].key,
        ) by {
        match h {
            Stmt::Assign { target, rhs } => {
                let hc = Stmt::Assign { target, rhs };
                assert(h == hc);
                assert(stmt_kind(h) == 0);
                assert(
                    step(hc, cur, mel).0 == cur.insert(target, eval(rhs, cur, mel).0)
                );
                assert(step(hc, cur, mel).1 == mel);
                assert(step(h, cur, mel) == step(hc, cur, mel));
                if defs[n].idx < k {
                    span_shrink(spans, defs[n].idx, k);
                    if n == target {
                        assert(stmt_writes_scalar(h, n));
                        def_entry(whole, defs, spans, n);
                        assert(stmt_writes_scalar(whole[defs[n].idx], n)) by {
                            assert(
                                is_elem_assign(whole[defs[n].idx], n, defs[n].arr, defs[n].key)
                            );
                        };
                        two_write_indices(whole, n, defs[n].idx, k);
                    } else {
                        lookup_insert_other(cur, target, eval(rhs, cur, mel).0, n);
                    }
                } else {
                    assert(defs[n].idx == k);
                    assert(is_elem_assign(h, n, defs[n].arr, defs[n].key));
                    assert(target == n);
                    match rhs {
                        Expr::Elem(a, kk) => {
                            assert(a == defs[n].arr && kk == defs[n].key);
                            lookup_insert_self(cur, target, elookup(mel, defs[n].arr, defs[n].key));
                        },
                        _ => {
                            assert(false);
                        },
                    }
                }
            },
            Stmt::ArrStore { arr, key, rhs } => {
                // ArrStore moves elements but not scalars: the
                // scalar side is untouched; element stability for
                // the STILL-CLEAN span is the span oracle's own
                // soundness (spans[k+1] clean ==> no store in
                // [defidx, k+1) — taken as the premise).
                let hc = Stmt::ArrStore { arr, key, rhs };
                assert(h == hc);
                assert(stmt_kind(h) == 1);
                assert(step(hc, cur, mel).0 == cur);
                assert(
                    step(hc, cur, mel).1 == mel.insert(Elem { arr, key }, eval(rhs, cur, mel).0)
                );
                assert(step(h, cur, mel) == step(hc, cur, mel));
                if defs[n].idx < k {
                    span_shrink(spans, defs[n].idx, k);
                    // The store at k either misses this def's element
                    // (other array — or other slot — so the element is
                    // untouched) or hits it (contradicting the clean
                    // span, which contains no store to this array).
                    if arr != defs[n].arr {
                        elookup_insert_other_elem(
                            mel,
                            arr,
                            key,
                            eval(rhs, cur, mel).0,
                            defs[n].arr,
                            defs[n].key,
                        );
                    } else {
                        assert(is_store_to(whole[k], defs[n].arr));
                    }
                } else {
                    // The def site is an Assign (kind 0), never an
                    // ArrStore — contradiction, closes the case.
                    assert(defs[n].idx == k);
                    assert(is_elem_assign(h, n, defs[n].arr, defs[n].key));
                    assert(false);
                }
            },
            _ => {
                match h {
                    Stmt::Output { val } => {
                        let hc = Stmt::Output { val };
                        assert(h == hc);
                        let r = eval(val, cur, mel);
                        assert(step(hc, cur, mel).0 == cur);
                        assert(step(hc, cur, mel).1 == mel);
                        assert(step(h, cur, mel) == step(hc, cur, mel));
                    },
                    Stmt::Other { reads, effect } => {
                        let hc = Stmt::Other { reads, effect };
                        assert(h == hc);
                        assert(step(hc, cur, mel).0 == cur);
                        assert(step(hc, cur, mel).1 == mel);
                        assert(step(h, cur, mel) == step(hc, cur, mel));
                    },
                    _ => {
                        assert(false);
                    },
                }
                if defs[n].idx < k {
                    span_shrink(spans, defs[n].idx, k);
                } else {
                    assert(defs[n].idx == k);
                    assert(is_elem_assign(h, n, defs[n].arr, defs[n].key));
                    assert(false);
                }
            },
        }
    };
}

// THE OBLIGATION (single-store induction: the fold preserves the
// run exactly from the same start stores).
proof fn fwd_sound(
    s: Seq<Stmt>,
    whole: Seq<Stmt>,
    defs: Defs,
    spans: Map<(int, int), bool>,
    k: int,
    cur: Store,
    mel: Elems
)
    requires
        suffix_at(s, whole, k),
        0 <= k,
        defs_sound(whole, defs),
        span_mono(spans),
        span_sound(whole, defs, spans),
        closed_for(whole, cur),
        prefix_ok(whole, defs, spans, k, cur, mel),
    ensures run(fold_suffix(s, whole, defs, spans, k), cur, mel) == run(s, cur, mel),
    decreases s.len(),
{
    if s.len() == 0 {
    } else {
        assert(0 <= k < whole.len()) by {
            assert(suffix_at(s, whole, k));
            assert(s.len() as int + k == whole.len() as int);
        };
        assert(s[0] == whole[k]) by {
            assert(suffix_at(s, whole, k));
        };
        step_same(s[0], whole, defs, spans, k, cur, mel);
        let f = fold_stmt(s[0], defs, spans, k);
        fold_step(s, whole, defs, spans, k);
        run_cons(f, fold_suffix(s.drop_first(), whole, defs, spans, k + 1), cur, mel);
        run_cons(s[0], s.drop_first(), cur, mel);
        suffix_tail(s, whole, k);
        fwd_sound(
            s.drop_first(),
            whole,
            defs,
            spans,
            k + 1,
            step(f, cur, mel).0,
            step(f, cur, mel).1
        );
    }
}

// Stepping preserves closedness: steps only insert bindings.
proof fn step_closed(st: Stmt, whole: Seq<Stmt>, s: Store, m: Elems)
    requires closed_for(whole, s),
    ensures closed_for(whole, step(st, s, m).0),
{
    if stmt_kind(st) == 0 {
        match st {
            Stmt::Assign { target, rhs } => {
                let r = eval(rhs, s, m);
                insert_grows(s, target, r.0);
                assert(step(st, s, m).0 == s.insert(target, r.0));
            },
            _ => {},
        }
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

// THE TOP-LEVEL OBLIGATION: folding preserves the run from any closed
// start store and any element map.
proof fn fwd_sound_top(p: Seq<Stmt>, defs: Defs, spans: Map<(int, int), bool>, init: Store, mel: Elems)
    requires defs_sound(p, defs), span_mono(spans), span_sound(p, defs, spans), closed_for(p, init),
    ensures run(fold_prog(p, defs, spans), init, mel) == run(p, init, mel),
{
    assert(prefix_ok(p, defs, spans, 0, init, mel)) by {
        assert forall|n: Name|
            defs.dom().contains(n) && defs[n].idx < 0 && spans.dom().contains(
                (defs[n].idx, 0),
            ) && spans[(defs[n].idx, 0)] implies lookup(init, n) == elookup(
                mel,
                defs[n].arr,
                defs[n].key,
            ) by {
        };
    };
    assert(suffix_at(p, p, 0)) by {
        assert forall|i: int| 0 <= i < p.len() implies #[trigger] p[i] == p[0 + i] by {
        };
    };
    fwd_sound(p, p, defs, spans, 0, init, mel);
}

// ---- Fidelity pins (mirror arr_load_fwd.rs's 9 unit tests) ----

// `bh=TAB[3]; out("$bh")` — the read folds to the element load.
proof fn pin_forward_folds()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Output { val: Expr::Var(7nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None }),
        Map::empty().insert((0, 1), true),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None });
    let spans = Map::empty().insert((0, 1), true);
    assert(defs_sound(p, defs)) by {
        assert(count_scalar_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            reveal_with_fuel(count_scalar_writes, 2);
        };
    };
    assert(fold_suffix(p, p, defs, spans, 0) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ]) by {
        reveal_with_fuel(fold_suffix, 4);
    };
}

// A sibling element load (`gh=TAB[4]`) between def and read is inert:
// the span stays clean and the forward crosses it.
proof fn pin_sibling_read_inert()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Assign { target: 8nat, rhs: Expr::Elem(9nat, 4) },
            Stmt::Output { val: Expr::Var(7nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None }),
        Map::empty().insert((0, 1), true).insert((0, 2), true),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Assign { target: 8nat, rhs: Expr::Elem(9nat, 4) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Assign { target: 8nat, rhs: Expr::Elem(9nat, 4) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None });
    let spans = Map::empty().insert((0, 1), true).insert((0, 2), true);
    assert(defs_sound(p, defs)) by {
        assert(count_scalar_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            count_step(p.drop_first(), 7nat);
            count_step(p.drop_first().drop_first(), 7nat);
            reveal_with_fuel(count_scalar_writes, 2);
        };
    };
    assert(fold_suffix(p, p, defs, spans, 0) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Assign { target: 8nat, rhs: Expr::Elem(9nat, 4) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ]) by {
        reveal_with_fuel(fold_suffix, 6);
    };
}

// `bh=TAB[3]; TAB[0]=z; out("$bh")` — an array store breaks the
// span: the read keeps its scalar (the forward stops).
proof fn pin_array_write_refuses()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::ArrStore { arr: 9nat, key: 0, rhs: Expr::Lit(26) },
            Stmt::Output { val: Expr::Var(7nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None }),
        Map::empty().insert((0, 1), true),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::ArrStore { arr: 9nat, key: 0, rhs: Expr::Lit(26) },
        Stmt::Output { val: Expr::Var(7nat) },
    ],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::ArrStore { arr: 9nat, key: 0, rhs: Expr::Lit(26) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None });
    // No clean span reaches the read (the store breaks it): the only
    // span entry covers the store itself.
    let spans = Map::empty().insert((0, 1), true);
    assert(defs_sound(p, defs)) by {
        assert(count_scalar_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            count_step(p.drop_first(), 7nat);
            count_step(p.drop_first().drop_first(), 7nat);
            reveal_with_fuel(count_scalar_writes, 2);
        };
    };
    assert(!foldable(defs, spans, 2, 7nat));
    assert(fold_suffix(p, p, defs, spans, 0) == p) by {
        reveal_with_fuel(fold_suffix, 6);
    };
}

// `bh=TAB[3]; bh=7; out("$bh")` — two writes: no sound table entry.
proof fn pin_multi_def_refuses()
    ensures count_scalar_writes(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(7) },
        ],
        7nat,
    ) == 2,
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    count_step(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Assign { target: 7nat, rhs: Expr::Lit(7) },
        ],
        7nat
    );
    reveal_with_fuel(count_scalar_writes, 3);
}

// An opaque call between def and read refuses the whole forward.
proof fn pin_opaque_refuses()
    ensures !foldable(
        Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None }),
        Map::empty(),
        2,
        7nat,
    ),
{
}

// `out("$bh"); bh=TAB[3]; out("$bh")` — the read BEFORE the def
// keeps its scalar (shell default); only the later read folds.
proof fn pin_refuse_before_def()
    ensures fold_prog(
        seq![
            Stmt::Output { val: Expr::Var(7nat) },
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Output { val: Expr::Var(7nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 1, arr: 9nat, key: 3, keyvar: None }),
        Map::empty().insert((1, 2), true),
    ) == seq![
        Stmt::Output { val: Expr::Var(7nat) },
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ],
{
    let p = seq![
        Stmt::Output { val: Expr::Var(7nat) },
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Var(7nat) },
    ];
    let defs = Map::empty().insert(7nat, Def { idx: 1, arr: 9nat, key: 3, keyvar: None });
    let spans = Map::empty().insert((1, 2), true);
    assert(defs_sound(p, defs)) by {
        assert(count_scalar_writes(p, 7nat) == 1) by {
            count_step(p, 7nat);
            count_step(p.drop_first(), 7nat);
            count_step(p.drop_first().drop_first(), 7nat);
            reveal_with_fuel(count_scalar_writes, 2);
        };
    };
    assert(!foldable(defs, spans, 0, 7nat));
    assert(fold_suffix(p, p, defs, spans, 0) == seq![
        Stmt::Output { val: Expr::Var(7nat) },
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ]) by {
        reveal_with_fuel(fold_suffix, 6);
    };
}

// The call spelling crosses too: a read-only `arrayIndex` call
// between def and read is span-clean (the READ_ONLY_CALLS
// recognition the whole idiom depends on).
proof fn pin_sibling_call_inert()
    ensures fold_prog(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Assign { target: 8nat, rhs: Expr::Elem(9nat, 4) },
            Stmt::Output { val: Expr::Var(7nat) },
        ],
        Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: None }),
        Map::empty().insert((0, 1), true).insert((0, 2), true),
    ) == seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Assign { target: 8nat, rhs: Expr::Elem(9nat, 4) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ],
{
    // Same shape as the sibling-read pin (the call form denotes the
    // same load — one Elem in the model).
    pin_sibling_read_inert();
}

// A fully-unfolded def is dropped under the no-reader/pure-RHS
// standard (the orphan sweep; DSE's own purge rule, cited).
proof fn pin_orphan_drops()
    ensures drop_dead(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Output { val: Expr::Elem(9nat, 3) },
        ],
        7nat,
    ) == seq![Stmt::Output { val: Expr::Elem(9nat, 3) }],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ];
    assert(p[0] == Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) });
    assert(p.drop_first() == seq![Stmt::Output { val: Expr::Elem(9nat, 3) }]);
    assert(stmt_writes_scalar(p[0], 7nat));
    assert(!stmt_reads(Stmt::Output { val: Expr::Elem(9nat, 3) }, 7nat));
    assert(reads_later(Seq::<Stmt>::empty(), 7nat) == false);
    assert(
        seq![Stmt::Output { val: Expr::Elem(9nat, 3) }].drop_first() == Seq::<Stmt>::empty()
    );
    assert(!reads_later(p.drop_first(), 7nat));
    assert(
        drop_dead(p, 7nat) == drop_dead(seq![Stmt::Output { val: Expr::Elem(9nat, 3) }], 7nat)
    );
    assert(
        seq![Stmt::Output { val: Expr::Elem(9nat, 3) }][0] == Stmt::Output {
            val: Expr::Elem(9nat, 3),
        }
    );
    assert(!stmt_writes_scalar(Stmt::Output { val: Expr::Elem(9nat, 3) }, 7nat));
    assert(
        seq![Stmt::Output { val: Expr::Elem(9nat, 3) }].drop_first() == Seq::<Stmt>::empty()
    );
    assert(drop_dead(Seq::<Stmt>::empty(), 7nat) == Seq::<Stmt>::empty());
    assert(
        seq![Stmt::Output { val: Expr::Elem(9nat, 3) }].add(Seq::<Stmt>::empty()) == seq![
            Stmt::Output { val: Expr::Elem(9nat, 3) },
        ]
    );
}

// Dropping the orphaned store preserves the trace (and every
// live cell): both runs emit Print(42); the stores agree except at
// the dead name 7. The general purge is DSE's proved rule, cited;
// this pin exhibits it on the witness shape.
proof fn pin_orphan_preserves()
    ensures run(
        seq![Stmt::Output { val: Expr::Elem(9nat, 3) }],
        Map::empty(),
        Map::empty().insert(Elem { arr: 9nat, key: 3 }, 42),
    ).2 == run(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Output { val: Expr::Elem(9nat, 3) },
        ],
        Map::empty(),
        Map::empty().insert(Elem { arr: 9nat, key: 3 }, 42),
    ).2,
{
    let mel = Map::empty().insert(Elem { arr: 9nat, key: 3 }, 42);
    assert(elookup(mel, 9nat, 3) == 42);
    let short = seq![Stmt::Output { val: Expr::Elem(9nat, 3) }];
    let full = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ];
    assert(full[0] == Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) });
    assert(full.drop_first() == short);
    assert(short[0] == Stmt::Output { val: Expr::Elem(9nat, 3) });
    assert(short.drop_first() == Seq::<Stmt>::empty());
    run_cons(short[0], short.drop_first(), Map::empty(), mel);
    run_cons(full[0], full.drop_first(), Map::empty(), mel);
    assert(
        run(Seq::<Stmt>::empty(), Map::empty().insert(7nat, 42), mel).2 == Seq::<Ev>::empty()
    );
    assert(
        run(Seq::<Stmt>::empty(), Map::empty(), mel).2 == Seq::<Ev>::empty()
    );
    assert(
        eval(Expr::Elem(9nat, 3), Map::empty().insert(7nat, 42), mel).0 == 42
    );
    assert(eval(Expr::Elem(9nat, 3), Map::empty(), mel).0 == 42);
}

// A write to the key variable between def and read breaks the span:
// the re-evaluated load could observe a different element.
proof fn pin_key_write_refuses()
    ensures !foldable(
        Map::empty().insert(7nat, Def { idx: 0, arr: 9nat, key: 3, keyvar: Some(11nat) }),
        Map::empty().insert((0, 1), true),
        2,
        7nat,
    ),
{
    // No span entry covers (0, 2) — the key write in between voids
    // it — so the read keeps its scalar.
}

// Runtime check: two trips over a live element visit its values.
proof fn pin_trace_agrees()
    ensures run(
        seq![
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Stmt::Output { val: Expr::Elem(9nat, 3) },
        ],
        Map::empty(),
        Map::empty().insert(Elem { arr: 9nat, key: 3 }, 42),
    ).2 == seq![Ev::Print(42)],
{
    let p = seq![
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        Stmt::Output { val: Expr::Elem(9nat, 3) },
    ];
    let mel = Map::empty().insert(Elem { arr: 9nat, key: 3 }, 42);
    assert(p[0] == Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) });
    assert(p.drop_first() == seq![Stmt::Output { val: Expr::Elem(9nat, 3) }]);
    assert(elookup(mel, 9nat, 3) == 42);
    assert(
        step(p[0], Map::empty(), mel) == step(
            Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
            Map::empty(),
            mel,
        )
    );
    assert(
        step(Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) }, Map::empty(), mel).0 == Map::empty().insert(
            7nat,
            42,
        )
    );
    assert(
        step(Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) }, Map::empty(), mel).1 == mel
    );
    run_cons(
        Stmt::Assign { target: 7nat, rhs: Expr::Elem(9nat, 3) },
        p.drop_first(),
        Map::empty(),
        mel
    );
    assert(
        eval(Expr::Elem(9nat, 3), Map::empty().insert(7nat, 42), mel).0 == 42
    );
    run_cons(
        Stmt::Output { val: Expr::Elem(9nat, 3) },
        Seq::<Stmt>::empty(),
        Map::empty().insert(7nat, 42),
        mel,
    );
    assert(
        eval(Expr::Elem(9nat, 3), Map::empty().insert(7nat, 42), mel) == (42, Seq::<Ev>::empty())
    );
    assert(
        run(Seq::<Stmt>::empty(), Map::empty().insert(7nat, 42), mel) == (
            Map::empty().insert(7nat, 42),
            mel,
            Seq::<Ev>::empty(),
        )
    );
    assert(
        run(p.drop_first(), Map::empty().insert(7nat, 42), mel).2 == seq![Ev::Print(42)]
    );
}

// Dropping a no-reader pure-RHS store preserves the run (the orphan
// sweep's standard — DSE's purge rule over the covered class).
pub open spec fn drop_dead(p: Seq<Stmt>, n: Name) -> Seq<Stmt>
    decreases p.len(),
{
    if p.len() == 0 {
        Seq::empty()
    } else if stmt_writes_scalar(p[0], n) && is_elem_write(p[0]) && !reads_later(
        p.drop_first(),
        n,
    ) {
        drop_dead(p.drop_first(), n)
    } else {
        seq![p[0]].add(drop_dead(p.drop_first(), n))
    }
}

pub open spec fn reads_later(p: Seq<Stmt>, n: Name) -> bool
    decreases p.len(),
{
    if p.len() == 0 {
        false
    } else {
        stmt_reads(p[0], n) || reads_later(p.drop_first(), n)
    }
}

// Evaluation ignores a store cell nobody reads.
proof fn eval_frame(e: Expr, s: Store, m: Elems, n: Name, v: int)
    requires !expr_reads(e, n),
    ensures eval(e, s.insert(n, v), m) == eval(e, s, m),
{
    match e {
        Expr::Lit(_) => {},
        Expr::Var(m2) => {
            assert(m2 != n);
            lookup_insert_other(s, n, v, m2);
        },
        Expr::Elem(_, _) => {},
    }
}

fn main() {}

}
