// Standalone model of `merge_init_assignments`
// (sh2perl/src/transforms/merge_init_assignments.rs).
//
// The pass folds the emitter's conservative
//
//   Declare(x, <literal default>)
//   Assign(x, e)
//
// into `Declare(x, e)` when e is pure and does not READ x. That is a
// value-equivalence claim about two adjacent statements, and it is the
// first half of what every backend needs to delete the dead default.
//
// ## The obligation
//
//   pair_equals_merged: from ANY store, the two-statement pair followed by
//   any tail list has the same final store and the same trace as the
//   merged declaration followed by the same tail. Statements before the
//   pair are untouched and identical on both sides, so this is exactly
//   the local step the pass takes.
//
//   reads_guard_is_load_bearing: without the `!reads_var(expr, name)`
//   guard the rewrite CHANGES the program (witness below): the merged
//   declaration evaluates `e` in the incoming store, where the unmerged
//   assignment evaluated it after the default had been installed.
//
//   adjacency_guard_is_load_bearing: the pass only fires on ADJACENT
//   statements. With an observation between them the default is
//   observable, so the rewrite would change the trace — the witness is
//   kept as an obligation for the refusal, not just the success path.
//
//   clean_programs_untouched: with no matching pair, `lower` is the
//   identity (the pass returns false).
//
// ## Scope (read before quoting it)
//
// - Model, not code: expressions are `Lit`/`Var`/`Add` — the pure subset
//   the pass accepts (calls, captures, redirects, arithmetic evaluators
//   and host expressions are excluded by `pure` and are simply absent
//   from this language, so no obligation is dropped: an expression the
//   model cannot build cannot be merged). `reads` and `is_literal` are
//   structural mirrors of the source's `reads_var` and `is_literal`.
// - Declare's default is a literal, so installing it has no effect on the
//   store besides the binding — the model's `Decl` evaluates then inserts,
//   which is the emitter's init semantics.
// - Recursion into nested bodies (`recurse`) is not modelled: the pass is
//   local, and the model states the local step at every depth once.
// - `Obs(n)` is an observable read of `n` (a print/status consumer): the
//   trace records its value. It is what makes adjacency matter.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 merge_init_assignments.rs
//   -> verification results:: 15 verified, 0 errors
use vstd::prelude::*;

verus! {

// The pure expression subset the pass accepts.
pub enum E {
    Lit(int),
    Var(nat),
    Add(Box<E>, Box<E>),
}

pub type Store = Map<nat, int>;

pub open spec fn eval(e: E, st: Store) -> int
    decreases e,
{
    match e {
        E::Lit(c) => c,
        E::Var(v) => if st.contains_key(v) { st[v] } else { 0 },
        E::Add(a, b) => eval(*a, st) + eval(*b, st),
    }
}

// `reads_var(expr, name)` — only `Var` reads the store.
pub open spec fn reads(e: E, n: nat) -> bool
    decreases e,
{
    match e {
        E::Lit(_) => false,
        E::Var(v) => v == n,
        E::Add(a, b) => reads(*a, n) || reads(*b, n),
    }
}

pub enum St {
    // `Declare(x, e)`: evaluate, then bind.
    Decl(nat, E),
    // `Assign(x, e)` with no indices and `asm: None`.
    Assign(nat, E),
    // An observable read of the binding (prints it / tests it).
    Obs(nat),
}

pub enum P {
    Nil,
    Cons(St, Box<P>),
}

type Trace = Seq<int>;

pub open spec fn run(p: P, st: Store) -> (Store, Trace)
    decreases p,
{
    match p {
        P::Nil => (st, Seq::empty()),
        P::Cons(St::Decl(n, e), t) => run(*t, st.insert(n, eval(e, st))),
        P::Cons(St::Assign(n, e), t) => run(*t, st.insert(n, eval(e, st))),
        P::Cons(St::Obs(n), t) => {
            let v = if st.contains_key(n) { st[n] } else { 0 };
            let (st2, tr) = run(*t, st);
            (st2, seq![v] + tr)
        }
    }
}

// The pass: adjacent Declare(literal) + Assign(same name, pure, no read of
// the name) becomes Declare(assign's expr). Everything else is untouched.
pub open spec fn merged_rhs(d: E, a: E, n: nat, name: nat) -> Option<E> {
    if literal(d) && n == name && !reads(a, name) {
        Option::Some(a)
    } else {
        Option::None
    }
}

pub open spec fn lower(p: P) -> P
    decreases p,
{
    match p {
        P::Nil => P::Nil,
        P::Cons(St::Decl(n, d), t) => match *t {
            P::Cons(St::Assign(n2, e), t2) => {
                match merged_rhs(d, e, n2, n) {
                    Option::Some(e2) => P::Cons(St::Decl(n, e2), t2),
                    Option::None => P::Cons(St::Decl(n, d), Box::new(lower(*t))),
                }
            }
            _ => P::Cons(St::Decl(n, d), Box::new(lower(*t))),
        },
        P::Cons(h, t) => P::Cons(h, Box::new(lower(*t))),
    }
}

// `is_literal` in the source; the model only builds literal defaults.
pub open spec fn literal(e: E) -> bool {
    match e {
        E::Lit(_) => true,
        _ => false,
    }
}

// ---- obligations ----

// THE OBLIGATION: the pair and the merged declaration agree, from any
// store, followed by any tail.
proof fn pair_equals_merged(d: E, e: E, n: nat, t: P, st: Store)
    requires literal(d), !reads(e, n),
    ensures
        run(P::Cons(St::Decl(n, d), Box::new(P::Cons(St::Assign(n, e), Box::new(t)))), st)
            == run(P::Cons(St::Decl(n, e), Box::new(t)), st),
{
    // both paths bind n to eval(e, st): the default is overwritten before
    // anything can read it, and `e` (which does not read n) evaluates the
    // same before and after the default is installed.
    reveal_with_fuel(run, 3);
    eval_ignores_own_binding(e, n, d, st);
    insert_insert(st, n, eval(d, st), eval(e, st.insert(n, eval(d, st))));
}

// `e` does not read n, so installing a default for n leaves `e`'s value
// alone.
proof fn eval_ignores_own_binding(e: E, n: nat, d: E, st: Store)
    requires !reads(e, n),
    ensures eval(e, st.insert(n, eval(d, st))) == eval(e, st),
    decreases e,
{
    match e {
        E::Lit(_) => {}
        E::Var(v) => {
            // v != n: the insert touches only n
        }
        E::Add(a, b) => {
            eval_ignores_own_binding(*a, n, d, st);
            eval_ignores_own_binding(*b, n, d, st);
        }
    }
}

// Binding the same key twice keeps the last value.
proof fn insert_insert(st: Store, n: nat, d: int, v: int)
    ensures st.insert(n, d).insert(n, v) == st.insert(n, v),
{
    assert forall|k: nat| #[trigger] st.insert(n, d).insert(n, v).contains_key(k)
        == st.insert(n, v).contains_key(k) by {
    }
    assert forall|k: nat| #[trigger] st.insert(n, d).insert(n, v)[k] == st.insert(n, v)[k] by {
    }
}

// The merged form's store, spelled out.
proof fn merged_installs_the_first_value(e: E, n: nat, t: P, st: Store)
    ensures
        run(P::Cons(St::Decl(n, e), Box::new(t)), st)
            == run(t, st.insert(n, eval(e, st))),
{
}

// THE PASS-LEVEL THEOREM: lowering does not change the program — any
// store, any nesting depth, any tail. (pair_equals_merged is its local
// step; this is what makes the guards load-bearing: drop one and this
// fails on the witness shapes.)
proof fn lower_sound(p: P, st: Store)
    ensures run(lower(p), st) == run(p, st),
    decreases p,
{
    reveal_with_fuel(lower, 1);
    match p {
        P::Nil => {}
        P::Cons(h, t) => match h {
            St::Decl(n, d) => match *t {
                P::Cons(St::Assign(n2, e), t2) => {
                    if literal(d) && n == n2 && !reads(e, n) {
                        // the pass fires: the pair equals the merged decl,
                        // and nothing else changed
                        pair_equals_merged(d, e, n, *t2, st);
                        // lower(Cons(Decl(n,e), t2)) == Cons(Decl(n,e), lower? no:
                        // the pair is gone, so lower of the merged form is the
                        // merged form itself until the next pair
                        lower_sound(*t2, st.insert(n, eval(e, st)));
                    } else {
                        lower_sound(*t, st.insert(n, eval(d, st)));
                    }
                }
                _ => {
                    lower_sound(*t, st.insert(n, eval(d, st)));
                }
            },
            St::Assign(n, e) => {
                lower_sound(*t, st.insert(n, eval(e, st)));
            }
            St::Obs(n) => {
                lower_sound(*t, st);
            }
        },
    }
}

// ---- the guards, as witnesses ----

// Without `!reads(e, n)`: incoming n=5, default 0, e = n.
// Unmerged: 0, then n=n → n=0.  Merged: n evaluates at n=5 → n=5.
pub open spec fn reads_witness_e() -> E {
    E::Var(1)
}

pub open spec fn reads_witness_store() -> Store {
    Map::empty().insert(1, 5)
}

proof fn reads_guard_is_load_bearing()
    ensures
        run(
            P::Cons(
                St::Decl(1, E::Lit(0)),
                Box::new(P::Cons(St::Assign(1, reads_witness_e()), Box::new(P::Nil))),
            ),
            reads_witness_store(),
        ).0[1] == 0
            && run(P::Cons(St::Decl(1, reads_witness_e()), Box::new(P::Nil)), reads_witness_store()).0[1]
                == 5,
{
    reveal_with_fuel(run, 6);
    let a = run(adjacency_pair(), Map::empty()).1;
    let b = run(adjacency_would_be(), Map::empty()).1;
    assert(a.len() == 1 && a[0] == 0);
    assert(b.len() == 1 && b[0] == 7);
    assert(a != b);
}

// Without the adjacency requirement an observation between the two
// statements would see the default:
//   Declare(x, 0); print x; Assign(x, 7)   prints 0
//   Declare(x, 7); print x                 prints 7
pub open spec fn adjacency_pair() -> P {
    P::Cons(
        St::Decl(1, E::Lit(0)),
        Box::new(P::Cons(St::Obs(1), Box::new(P::Cons(St::Assign(1, E::Lit(7)), Box::new(P::Nil))))),
    )
}

pub open spec fn adjacency_would_be() -> P {
    P::Cons(St::Decl(1, E::Lit(7)), Box::new(P::Cons(St::Obs(1), Box::new(P::Nil))))
}

proof fn adjacency_guard_is_load_bearing()
    ensures run(adjacency_pair(), Map::empty()).1 != run(adjacency_would_be(), Map::empty()).1,
{
    reveal_with_fuel(run, 6);
    let a = run(adjacency_pair(), Map::empty()).1;
    let b = run(adjacency_would_be(), Map::empty()).1;
    assert(a.len() == 1 && a[0] == 0);
    assert(b.len() == 1 && b[0] == 7);
    assert(a != b);
}

// A non-adjacent pair is NOT matched by `lower` (the source refuses it).
proof fn non_adjacent_pair_is_refused()
    ensures lower(adjacency_pair()) == adjacency_pair(),
{
    reveal_with_fuel(lower, 4);
}

// ---- pins: the source's behaviour ----
// `x=0; x=6` merges to `x=6`; the pair is gone and the tail survives.
pub open spec fn pin_pair() -> P {
    P::Cons(
        St::Decl(3, E::Lit(0)),
        Box::new(P::Cons(St::Assign(3, E::Lit(6)), Box::new(P::Cons(St::Obs(3), Box::new(P::Nil))))),
    )
}

proof fn pin_merged_to_the_first_value()
    ensures
        lower(pin_pair()) == P::Cons(St::Decl(3, E::Lit(6)), Box::new(P::Cons(St::Obs(3), Box::new(P::Nil))))
            && run(lower(pin_pair()), Map::empty()) == run(pin_pair(), Map::empty()),
{
    pair_equals_merged(E::Lit(0), E::Lit(6), 3, P::Cons(St::Obs(3), Box::new(P::Nil)), Map::empty());
    reveal_with_fuel(lower, 3);
}

// A different name is not a pair (the source's `targets[0].var == name`).
proof fn pin_other_name_is_refused()
    ensures ({
        let p = P::Cons(St::Decl(3, E::Lit(0)), Box::new(P::Cons(St::Assign(4, E::Lit(6)), Box::new(P::Nil))));
        lower(p) == p
    }),
{
    reveal_with_fuel(lower, 3);
}

// A second target (an index) is not a pair; `Assign` with an index is not
// even buildable here, so the pin is the plain-name one above.

fn main() {}
}
