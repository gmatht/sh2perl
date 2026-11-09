// Standalone model of the C backend's STATIC-`eval` INLINING
// (sh2perl/src/c_backend.rs: `static_eval_stmts` + `expand_static_evals`,
// called from `shir_to_c` before every analysis).
//
// THE DENOTATIONAL EXTENSION (this file): a literal `eval '<text>'` denotes
// the statements of `<text>` and nothing else. That identity licenses the
// rewrite, and it is also what makes the rewrite a correctness fix rather
// than a preference: this backend has ONE census (the decl inventory, the
// type/lifetime analyses and the read routing all fold over `prog.stmts`),
// and anything the census cannot see is a program that either fails to
// compile or reads the wrong storage.
//
// MEASURED 2026-11-04 (`sh2perl/examples/063_12_complex_eval.sh`):
//     eval "result=\$(( ${var:-0} + ${array[${index:-0}]:-0} ))"
//     echo "Eval result: $result"
// rendered to
//     _sh_sset(&result, …);        // ← no declaration of `result`
//     … _sh_env("result") …        // ← the ENVIRONMENT, not the variable
//     cc: 'result' undeclared
// `try_static_eval` lowered the eval'd text at RENDER time — after the
// census, the decl inventory and read routing had run. Both symptoms are one
// cause: the eval's statements were not part of the program the analyses saw.
//
// THE VIEW. A program is FLATTENED to the in-order leaf sequence the analyses
// actually fold over, each leaf tagged with how many literal-eval texts
// enclose it:
//
//   (0, Store(result))   `result=…`     a statement at the top level
//   (1, Store(result))   the same text inside `eval '<literal>'`
//   (-1, …)              inside a text the compiler cannot see
//                        (`eval "$x"`) — NEVER expandable
//
// Splicing an eval is then exactly "retag its leaves to depth 0": the leaves
// keep their in-order position, so the flat view is faithful and `expand` is
// a pure per-leaf map. The folds — each a recursion on an int CURSOR into the
// one sequence, which is why every obligation here reduces to a definitional
// unfold:
//
//   eff_at        the denotation (W/R effects; the tag is irrelevant)
//   assigned_at   every name a store targets, at any depth
//   census_at     the PRE-FIX census: Store leaves at depth 0 only — an eval
//                 is an opaque external call, so nothing inside it declares
//   expand        retag every leaf with `flat` (the fix)
//   census        census_at(expand(p), 0) — the fixed pipeline
//   route(n, p)   Var when the census knows `n`, else Env (the demotion that
//                 turned `$result` into `_sh_env("result")`)
//
// Obligations:
//   expansion_preserves_meaning   the rewrite is a no-op on the denotation
//   assigned_invariant            …and on which names get stored
//   census_is_exact               THE FIX: with no opaque text, the census of
//                                 the expanded program IS the set of stores
//   fixed_census_is_complete      …so every store target is declared
//   opaque_never_over_declares    the pre-fix census only ever MISSES — its
//                                 failure is one-sided, never a false
//                                 "declared" that could hide a bug
//   witness_063_12                the measured program, computed
//   route_var / route_env         the read demotion, same cause
//   dyn_eval_not_covered          the honest scope: an opaque text is not
//                                 expanded, and is still missed
//   expand_idempotent             the pre-pass reaches a fixpoint, so the
//                                 depth bound is a valve, not a semantics
use vstd::prelude::*;

verus! {

/// A variable name, abstracted to an int: no fold here looks inside one.
pub type Name = int;

/// `-1` = inside an opaque (dynamic) text; `k >= 0` = inside `k` literal
/// eval texts.
pub type Depth = int;

pub enum Leaf {
    Store(Name),
    Read(Name),
}

/// One atomic effect: a write of a name, or a read of one.
pub enum Eff {
    W(Name),
    R(Name),
}

/// How a read is ROUTED: to the C variable, or to the environment.
pub enum Route {
    Var,
    Env,
}

pub type Prog = Seq<(Depth, Leaf)>;

/// `eval "$x"` — a text the compiler cannot see, so it is never spliced.
pub open spec fn opaque() -> Depth {
    -1
}

/// The retag that IS the splice: everything expandable becomes top level.
pub open spec fn flat(d: Depth) -> Depth {
    if d >= 0 {
        0
    } else {
        -1
    }
}

/// The expansion: a per-leaf retag, so the statement order is untouched.
pub open spec fn expand(p: Prog) -> Prog {
    Seq::new(p.len(), |i: int| (flat(p[i].0), p[i].1))
}

// ---------------------------------------------------------------------
// The denotation and the folds (all cursor recursions)
// ---------------------------------------------------------------------

pub open spec fn leaf_eff(l: Leaf) -> Seq<Eff> {
    match l {
        Leaf::Store(n) => seq![Eff::W(n)],
        Leaf::Read(n) => seq![Eff::R(n)],
    }
}

/// The effect sequence. A statement inside an eval text runs exactly like the
/// same statement anywhere else — the tag is irrelevant to the meaning, which
/// is the whole licence for the rewrite.
pub open spec fn eff_at(p: Prog, i: int) -> Seq<Eff>
    decreases p.len() - i,
{
    if i >= p.len() {
        Seq::empty()
    } else {
        leaf_eff(p[i].1) + eff_at(p, i + 1)
    }
}

pub open spec fn eff(p: Prog) -> Seq<Eff> {
    eff_at(p, 0)
}

/// The name a leaf stores, if it is a store (depth-independent).
pub open spec fn stores_all(l: Leaf) -> Set<Name> {
    match l {
        Leaf::Store(n) => set![n],
        Leaf::Read(_) => Set::empty(),
    }
}

/// The PRE-FIX census, per leaf: a store at TOP LEVEL declares; one inside an
/// eval does not, because `eval` is opaque to the census. One condition, and
/// it is the entire defect.
pub open spec fn stores_top(d: Depth, l: Leaf) -> Set<Name> {
    match l {
        Leaf::Store(n) => if d == 0 { set![n] } else { Set::empty() },
        Leaf::Read(_) => Set::empty(),
    }
}

pub open spec fn assigned_at(p: Prog, i: int) -> Set<Name>
    decreases p.len() - i,
{
    if i >= p.len() {
        Set::empty()
    } else {
        stores_all(p[i].1).union(assigned_at(p, i + 1))
    }
}

pub open spec fn assigned(p: Prog) -> Set<Name> {
    assigned_at(p, 0)
}

pub open spec fn census_at(p: Prog, i: int) -> Set<Name>
    decreases p.len() - i,
{
    if i >= p.len() {
        Set::empty()
    } else {
        stores_top(p[i].0, p[i].1).union(census_at(p, i + 1))
    }
}

pub open spec fn census_opaque(p: Prog) -> Set<Name> {
    census_at(p, 0)
}

/// The FIXED pipeline: one pre-pass, then the very same census.
pub open spec fn census(p: Prog) -> Set<Name> {
    census_at(expand(p), 0)
}

/// No opaque text at or after `i`: the condition under which expansion is
/// total.
pub open spec fn dyn_free_at(p: Prog, i: int) -> bool
    decreases p.len() - i,
{
    if i >= p.len() {
        true
    } else {
        p[i].0 >= 0 && dyn_free_at(p, i + 1)
    }
}

pub open spec fn dyn_free(p: Prog) -> bool {
    dyn_free_at(p, 0)
}

/// Read routing: a name the census knows is a C variable; a name it does not
/// is demoted to an environment lookup (measured: `_sh_env("result")`).
/// `route` is the FIXED pipeline (it consults the post-expansion census) and
/// `route_opaque` is the pre-fix one — the pair is what makes the demotion a
/// before/after statement rather than a description.
pub open spec fn route(n: Name, p: Prog) -> Route {
    if census(p).contains(n) {
        Route::Var
    } else {
        Route::Env
    }
}

pub open spec fn route_opaque(n: Name, p: Prog) -> Route {
    if census_opaque(p).contains(n) {
        Route::Var
    } else {
        Route::Env
    }
}

// ---------------------------------------------------------------------
// Two shims that turn a per-leaf fact into a cursor step
// ---------------------------------------------------------------------

/// A top-level store declares exactly what it stores; a read declares nothing.
proof fn stores_top_zero(l: Leaf)
    ensures
        stores_top(0, l) == stores_all(l),
{
}

/// `expand` at a cursor: the retag is visible per index.
proof fn expand_index(p: Prog, i: int)
    requires
        0 <= i < p.len(),
    ensures
        expand(p)[i] == (flat(p[i].0), p[i].1),
        expand(p)[i].1 == p[i].1,
        expand(p).len() == p.len(),
{
}

// ---------------------------------------------------------------------
// 1. The rewrite preserves the meaning — and changes no store target
// ---------------------------------------------------------------------

proof fn eff_expand_at(p: Prog, i: int)
    requires
        0 <= i <= p.len(),
    ensures
        eff_at(expand(p), i) == eff_at(p, i),
    decreases p.len() - i,
{
    if i >= p.len() {
    } else {
        expand_index(p, i);
        assert(eff_at(expand(p), i)
            == leaf_eff(expand(p)[i].1) + eff_at(expand(p), i + 1));
        assert(leaf_eff(expand(p)[i].1) == leaf_eff(p[i].1));
        eff_expand_at(p, i + 1);
    }
}

proof fn expansion_preserves_meaning(p: Prog)
    ensures
        eff(expand(p)) == eff(p),
{
    eff_expand_at(p, 0);
}

proof fn assigned_expand_at(p: Prog, i: int)
    requires
        0 <= i <= p.len(),
    ensures
        assigned_at(expand(p), i) == assigned_at(p, i),
    decreases p.len() - i,
{
    if i >= p.len() {
    } else {
        expand_index(p, i);
        assert(assigned_at(expand(p), i)
            == stores_all(expand(p)[i].1).union(assigned_at(expand(p), i + 1)));
        assert(stores_all(expand(p)[i].1) == stores_all(p[i].1));
        assigned_expand_at(p, i + 1);
    }
}

proof fn assigned_invariant(p: Prog)
    ensures
        assigned(expand(p)) == assigned(p),
{
    assigned_expand_at(p, 0);
}

// ---------------------------------------------------------------------
// 2. The pre-fix census is one-sided: it never over-declares
// ---------------------------------------------------------------------

proof fn opaque_never_over_declares_at(p: Prog, i: int)
    requires
        0 <= i <= p.len(),
    ensures
        census_at(p, i).subset_of(assigned_at(p, i)),
    decreases p.len() - i,
{
    if i >= p.len() {
    } else {
        assert(census_at(p, i)
            == stores_top(p[i].0, p[i].1).union(census_at(p, i + 1)));
        assert(assigned_at(p, i)
            == stores_all(p[i].1).union(assigned_at(p, i + 1)));
        opaque_never_over_declares_at(p, i + 1);
        // the per-leaf half: a store declares only at depth 0, and then it
        // declares exactly itself; a read never declares anything
        if p[i].0 == 0 {
            stores_top_zero(p[i].1);
        }
    }
}

proof fn opaque_never_over_declares(p: Prog)
    ensures
        census_opaque(p).subset_of(assigned(p)),
{
    opaque_never_over_declares_at(p, 0);
}

// ---------------------------------------------------------------------
// 3. THE FIX: after expansion the census declares every store target
// ---------------------------------------------------------------------

proof fn census_is_exact_at(p: Prog, i: int)
    requires
        0 <= i <= p.len(),
        dyn_free_at(p, i),
    ensures
        census_at(expand(p), i) == assigned_at(p, i),
    decreases p.len() - i,
{
    if i >= p.len() {
    } else {
        assert(dyn_free_at(p, i) ==> (p[i].0 >= 0 && dyn_free_at(p, i + 1)));
        expand_index(p, i);
        // expandable ⟹ spliced ⟹ the census sees it as top level
        assert(flat(p[i].0) == 0);
        assert(expand(p)[i].0 == 0);
        stores_top_zero(expand(p)[i].1);
        assert(stores_top(expand(p)[i].0, expand(p)[i].1)
            == stores_all(p[i].1));
        assert(census_at(expand(p), i)
            == stores_all(p[i].1).union(census_at(expand(p), i + 1)));
        assert(assigned_at(p, i)
            == stores_all(p[i].1).union(assigned_at(p, i + 1)));
        census_is_exact_at(p, i + 1);
    }
}

proof fn census_is_exact(p: Prog)
    requires
        dyn_free(p),
    ensures
        census_opaque(expand(p)) == assigned(p),
{
    census_is_exact_at(p, 0);
}

proof fn fixed_census_is_complete(p: Prog)
    requires
        dyn_free(p),
    ensures
        assigned(p).subset_of(census(p)),
{
    census_is_exact(p);
}

// ---------------------------------------------------------------------
// 4. The measured witness — 063_12, computed
// ---------------------------------------------------------------------
//
// `eval "result=$(( … ))"` is a Store at depth 1; `echo "…$result"` is a Read
// at depth 0. Pre-fix the census is EMPTY, so the emitted `_sh_sset(&result,
// …)` names a variable no declaration exists for, and the read is demoted to
// an environment lookup.

pub open spec fn p_063_12() -> Prog {
    seq![(1int, Leaf::Store(0)), (0int, Leaf::Read(0))]
}

pub open spec fn denotes_063_12() -> Seq<Eff> {
    seq![Eff::W(0), Eff::R(0)]
}

proof fn witness_063_12()
    ensures
        // the program IS `result=…; $result`
        eff(p_063_12()) == denotes_063_12(),
        // the store targets `result` …
        assigned(p_063_12()).contains(0),
        // … the PRE-FIX census declares NOTHING → cc: 'result' undeclared
        !census_opaque(p_063_12()).contains(0),
        // … and the FIXED one declares it
        census(p_063_12()).contains(0),
        // the expansion is a no-op on the meaning here too
        eff(expand(p_063_12())) == eff(p_063_12()),
{
    assert(eff(p_063_12()) == denotes_063_12()) by (compute);
    assert(assigned(p_063_12()).contains(0)) by (compute);
    // membership, not set equality: `by (compute)` normalizes the fold to
    // `empty().union(empty().union(empty()))` and the evaluator has no set
    // algebra to finish it — but `contains` on a concrete set does compute,
    // and it is the statement that matters (`result` is undeclared).
    assert(!census_opaque(p_063_12()).contains(0)) by (compute);
    assert(census(p_063_12()).contains(0)) by (compute);
    assert(eff(expand(p_063_12())) == eff(p_063_12())) by (compute);
}

// ---------------------------------------------------------------------
// 5. The read demotion — same cause, second symptom. The READ was at the
//    top level all along; it is the census that could not see the store.
// ---------------------------------------------------------------------

proof fn route_var()
    ensures
        // pre-fix `result` is not in the census → `_sh_env("result")`
        route_opaque(0, p_063_12()) == Route::Env,
        // post-fix it is a C variable → `(result ? result : "")`
        route(0, p_063_12()) == Route::Var,
{
    assert(!census_opaque(p_063_12()).contains(0)) by (compute);
    assert(census(p_063_12()).contains(0)) by (compute);
}

proof fn route_env(n: Name, p: Prog)
    requires
        !census(p).contains(n),
    ensures
        route(n, p) == Route::Env,
{
}

// ---------------------------------------------------------------------
// 6. The honest scope: an OPAQUE text is not expanded, and is still missed
// ---------------------------------------------------------------------

pub open spec fn p_dyn() -> Prog {
    seq![(opaque(), Leaf::Store(0)), (0int, Leaf::Read(0))]
}

proof fn dyn_eval_not_covered()
    ensures
        // a dynamic `eval "$x"` is left exactly as it was found
        expand(p_dyn()) == p_dyn(),
        !dyn_free(p_dyn()),
        // so its store stays invisible: `eval "$x"` is a documented gap,
        // never a silent one
        !census(p_dyn()).contains(0),
        assigned(p_dyn()).contains(0),
{
    assert(expand(p_dyn()) == p_dyn()) by (compute);
    assert(!census(p_dyn()).contains(0)) by (compute);
    assert(assigned(p_dyn()).contains(0)) by (compute);
}

// ---------------------------------------------------------------------
// 7. The pre-pass reaches a fixpoint (the depth bound is a valve, not a
//    semantic parameter)
// ---------------------------------------------------------------------

proof fn expand_idempotent(p: Prog)
    ensures
        expand(expand(p)) == expand(p),
{
    assert forall|i: int| 0 <= i < p.len() implies
        #[trigger] expand(expand(p))[i] == expand(p)[i]
    by {
        expand_index(p, i);
        assert(flat(flat(p[i].0)) == flat(p[i].0));
        assert(expand(expand(p))[i] == (flat(expand(p)[i].0), expand(p)[i].1));
    }
    assert(expand(expand(p)).len() == expand(p).len());
}

} // verus!

fn main() {}
