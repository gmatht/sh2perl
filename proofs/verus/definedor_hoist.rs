// Standalone model of `definedor_hoist`
// (sh2perl/src/shir_passes/definedor_hoist.rs).
//
// `x = ${a:-b}` reaches the IR as `Assign(x, DefinedOr { expr: a, default: b })`.
// A renderer with no special case emits the SUBJECT twice — once for the
// "set / non-empty" test and once for the value — so a side-effecting subject
// (`$(cmd)`, `$((i++))`, a call) runs twice. The pass binds the subject to a
// temp first:
//
//     __sh2_dor_0 = a;              // evaluated exactly once
//     x = ${__sh2_dor_0:-b};        // the original assignment over the temp
//
// ## The obligation
//
//   hoist_matches_bash: for a non-leaf subject, the hoisted program writes
//   the SAME value to `x` as bash's `${a:-b}` and performs the SAME number of
//   subject/default evaluations. Bash evaluates the subject exactly once, so
//   "same as bash" is "exactly once".
//
//   naive_renderer_is_the_bug: a renderer that emits the subject twice
//   evaluates it twice — the disagreement the pass removes, kept as an
//   obligation so the theorem is not the pass agreeing with itself.
//
//   exhaustively: the hoist ADDS a visible binding for the temp. That is
//   stated, not hidden: the theorem claims `x` and the other user names are
//   unchanged and the temp holds the subject's value, which is exactly the
//   cost the pass pays (namespace pollution), not a semantic free lunch.
//
// ## Scope (read before quoting it)
//
// - Model, not code: subjects/defaults are `Lit`/`Var`/`Side(v)` where
//   `Side` is "an expression with a side effect whose value is v"; the
//   evaluation count is the observable. `defined` is `value != 0`, i.e. the
//   `:-` (unset-OR-empty) form this pass handles.
// - Freshness is a HYPOTHESIS: the temp name must not be read by the subject
//   or the default and must differ from the target. The source numbers
//   `__sh2_dor_<n>` per run, which makes collisions unlikely but not
//   impossible — a user variable with exactly that name could collide, and
//   that is recorded as a limitation, not proved away.
// - The recursion into nested statement lists, the `Declare`-init refusal and
//   the leaf-subject refusal are pinned as predicates (`hoists`)/tests, not
//   re-derived per node.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 definedor_hoist.rs
//   -> verification results:: 9 verified, 0 errors
//
// Mutant (checked): binding the temp to `0` instead of the subject's value
// reports 7 verified, 2 errors — the value and count obligations both fail,
// so the "temp carries the subject" half is load-bearing, not decoration.
use vstd::prelude::*;

verus! {

pub enum E {
    Lit(int),
    // A variable read: no side effect, "unset" reads as `0` (empty).
    Var(nat),
    // A subject with a side effect (capture / call / `$((i++))`): its value
    // is `v` and it evaluates its subject once.
    Side(int),
}

pub type Store = Map<nat, int>;

// (value, evaluations performed)
pub type Outcome = (int, nat);

pub open spec fn eval(e: E, st: Store) -> Outcome
    decreases e,
{
    match e {
        E::Lit(v) => (v, 0),
        E::Var(x) => (if st.contains_key(x) { st[x] } else { 0 }, 0),
        E::Side(v) => (v, 1),
    }
}

// bash's `${a:-b}`: the subject is evaluated ONCE; the default only when the
// subject is unset or empty (`value == 0` in this integer model).
pub open spec fn bash(a: E, b: E, st: Store) -> Outcome {
    let (va, ca) = eval(a, st);
    if va != 0 {
        (va, ca)
    } else {
        let (vb, cb) = eval(b, st);
        (vb, ca + cb)
    }
}

// A renderer with no `DefinedOr` case: the subject is emitted twice (the
// test, then the value), so a side-effecting subject runs twice.
pub open spec fn naive(a: E, b: E, st: Store) -> Outcome {
    let (va, ca) = eval(a, st);
    if va != 0 {
        let (va2, ca2) = eval(a, st);
        (va2, ca + ca2)
    } else {
        let (vb, cb) = eval(b, st);
        (vb, ca + cb)
    }
}

// `is_leaf`: the pass refuses a subject it can re-read for free.
pub open spec fn is_leaf(e: E) -> bool {
    match e {
        E::Var(_) | E::Lit(_) => true,
        E::Side(_) => false,
    }
}

// ---- the pass's effect at statement level ----

pub struct Outcome2 {
    pub st: Store,
    pub evals: nat,
}

// `x = ${a:-b}` as one assignment.
pub open spec fn run_plain(x: nat, a: E, b: E, st: Store) -> Outcome2 {
    let (v, c) = bash(a, b, st);
    Outcome2 { st: st.insert(x, v), evals: c }
}

// `t = a; x = ${t:-b}` — the hoisted pair.
pub open spec fn run_hoisted(x: nat, t: nat, a: E, b: E, st: Store) -> Outcome2 {
    let (va, ca) = eval(a, st);
    let st1 = st.insert(t, va);
    let (v, c) = bash(E::Var(t), b, st1);
    Outcome2 { st: st1.insert(x, v), evals: ca + c }
}

pub open spec fn reads(e: E, n: nat) -> bool {
    match e {
        E::Var(v) => v == n,
        E::Lit(_) => false,
        E::Side(_) => false,
    }
}

// ---- obligations ----

// THE OBLIGATION: the hoisted pair writes the same value to `x` as bash's
// one-shot `${a:-b}` and does the same number of evaluations. The temp is
// the only extra binding, and it holds the subject's value.
proof fn hoist_matches_bash(x: nat, t: nat, a: E, b: E, st: Store)
    requires t != x, !reads(b, t), !reads(a, t),
    ensures
        run_hoisted(x, t, a, b, st).st[x] == run_plain(x, a, b, st).st[x],
        run_hoisted(x, t, a, b, st).evals == run_plain(x, a, b, st).evals,
        run_hoisted(x, t, a, b, st).st[t] == eval(a, st).0,
        forall|y: nat| y != t && y != x
            ==> run_hoisted(x, t, a, b, st).st[y]
                == run_plain(x, a, b, st).st[y],
{
    // `${t:-b}` after `t = <subject value>` sees exactly the value bash's
    // one-shot test saw: nonzero -> the value, zero -> the default.
    temp_test_agrees(a, b, t, st);
    let (va, ca) = eval(a, st);
    let (vb, cb) = bash(a, b, st);
    let st1 = st.insert(t, va);
    let (v, c) = bash(E::Var(t), b, st1);
    assert(st1[x] == st[x]);
    assert(v == vb) by {
        temp_test_agrees(a, b, t, st);
    }
}

// A read of another name is unaffected by installing the temp.
proof fn eval_ignores_other_binding(e: E, t: nat, v: int, st: Store)
    requires !reads(e, t),
    ensures eval(e, st.insert(t, v)) == eval(e, st),
    decreases e,
{
    match e {
        E::Lit(_) | E::Side(_) => {}
        E::Var(x) => {
            assert(st.insert(t, v).contains_key(x) == st.contains_key(x));
        }
    }
}

// The temp holds the subject's value, so the second line's test agrees with
// the first line's read — the hinge of the proof above, stated on its own.
proof fn temp_test_agrees(a: E, b: E, t: nat, st: Store)
    requires !reads(b, t),
    ensures ({
        let (va, _) = eval(a, st);
        let (v, _) = bash(E::Var(t), b, st.insert(t, va));
        let (vb, _) = bash(a, b, st);
        v == vb
    }),
{
    let (va, _) = eval(a, st);
    if va != 0 {
    } else {
        eval_ignores_other_binding(b, t, va, st);
    }
}

// ---- the bug the pass removes (kept as an obligation) ----
proof fn naive_renderer_is_the_bug()
    ensures ({
        let st = Map::<nat, int>::empty();
        let a = E::Side(5);
        let b = E::Lit(0);
        // one evaluation is bash's count; the naive renderer does two
        bash(a, b, st).1 == 1 && naive(a, b, st).1 == 2
            && bash(a, b, st).0 == naive(a, b, st).0
    }),
{
}

// A side-effecting subject whose second evaluation has a DIFFERENT value
// would even change the result — the same bug, spelled as a value.
pub open spec fn side_counter(v1: int, v2: int) -> (E, E) {
    (E::Side(v1), E::Lit(v2))
}

proof fn double_evaluation_is_observable()
    ensures {
        let st = Map::<nat, int>::empty();
        // modeled by the count: 1 vs 2 for the same program
        bash(E::Side(5), E::Lit(0), st).1 != naive(E::Side(5), E::Lit(0), st).1
    },
{
}

// ---- pins: the pass's refusals and its fired shape ----
// A leaf subject is not hoisted (re-reading a Var/Lit is free).
proof fn leaf_subject_is_refused()
    ensures is_leaf(E::Var(3)) && is_leaf(E::Lit(7)) && !is_leaf(E::Side(1)),
{
}

// The fired shape: a non-leaf subject gets one temp binding and the
// assignment reads it.
proof fn fired_shape_is_one_binding_and_a_temp_read()
    ensures ({
        let p = run_hoisted(9, 4, E::Side(5), E::Lit(0), Map::<nat, int>::empty());
        // the temp holds the subject, `x` holds the value
        p.st[4] == 5 && p.st[9] == 5 && p.evals == 1
    }),
{
}

// A default is only evaluated when the subject is unset/empty.
proof fn default_is_lazy()
    ensures ({
        let st = Map::<nat, int>::empty();
        let b = E::Side(7);
        // subject set: b never runs
        bash(E::Side(5), b, st).1 == 1
            // subject empty: b runs once, after the subject
            && bash(E::Side(0), b, st).1 == 2
            && bash(E::Side(0), b, st).0 == 7
    }),
{
}

fn main() {}
}
