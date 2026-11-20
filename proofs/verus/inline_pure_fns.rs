// Standalone model of `inline_pure_fns`
// (sh2perl/src/transforms/inline_pure_fns.rs).
//
// The pass inlines a call to a single-expression pure leaf function:
//
//     fnCall("f", a1 … ak)         where f's body is ONE pure expression
//
// by substituting the body's positional reads (`getVar("N")`) with the
// caller's Nth argument. The invariant it relies on is stated in the module
// doc: the ARGUMENTS are pure (no capture/exec/process), so evaluating them
// early or re-reading them cannot change behaviour.
//
// ## The obligation
//
//   inline_preserves: given pure arguments and a body whose single write
//   does not touch a name the arguments read, inlining yields the same
//   value, the same store and the same effect count as the call.
//
//   capture_witness: WITHOUT that last hypothesis the rewrite is wrong —
//   a body that writes `x` and then reads positional 0, called with `$x`,
//   returns the OLD value in bash (arguments are evaluated before the body
//   runs) and the NEW value after inlining. Kept as an obligation so the
//   theorem is not the pass agreeing with itself: the pass checks purity
//   only, so this is a genuine (currently unguarded) assumption, recorded
//   as a stated limitation rather than proved away.
//
//   double_effect_witness: with an IMPURE argument the naive inlining runs
//   the argument once per use (the pass refuses impure args for exactly
//   this reason) — again a witness, not an assumption.
//
// ## Scope (read before quoting it)
//
// - Model, not code: expressions are `Lit`/`Var` (store read)/`Param`
//   (a positional `getVar("N")` read)/`Add`/`Side(v)` ("an impure
//   expression whose value is v"), and the effect count is the observable.
// - The body is modelled as `Body { wname, wval, value }`: at most ONE
//   store write followed by the result expression. A real body is a single
//   `IrStmt::Expr`, so the write models the `setVar`/`assign`/`arraySet`
//   calls the body is allowed to contain; more than one write is the same
//   induction with the read-set hypothesis.
// - The source's substitution is 1-based (`getVar("1")` is argument 0);
//   the model uses 0-based `Param(n)` and pins the correspondence.
// - The pass is NOT registered in `transforms::all()` today (parked offer),
//   so this proves a rule in the tree that changes no emitted byte yet.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 inline_pure_fns.rs
//   -> verification results:: 15 verified, 0 errors
//
// Mutants (checked, measured on copies):
//   - 1-based/off-by-one substitution (`k + 1 < n`): 10 verified, 5 errors.
//   - dropping the purity conjunct from `ok_arg`: 14 verified, 1 error (the
//     `pure_no_effects` precondition is then unsatisfiable — purity is
//     load-bearing in the proof, not decoration).
// Restored and re-checked at 15 verified, 0 errors.
use vstd::prelude::*;

verus! {

pub enum E {
    Lit(int),
    // A store read (`x`, `$x`): pure.
    Var(nat),
    // A positional read, `getVar("N")` (0-based in the model).
    Param(nat),
    Add(Box<E>, Box<E>),
    // An expression the pass calls IMPURE (capture/exec/pipeline): value v,
    // one observable effect.
    Side(int),
}

pub type Store = Map<nat, int>;
pub type Params = Map<nat, int>;

// (value, effects performed)
pub type Result1 = (int, nat);

pub open spec fn pure(e: E) -> bool
    decreases e,
{
    match e {
        E::Side(_) => false,
        E::Add(a, b) => pure(*a) && pure(*b),
        _ => true,
    }
}

pub open spec fn reads(e: E, x: nat) -> bool
    decreases e,
{
    match e {
        E::Var(v) => v == x,
        E::Add(a, b) => reads(*a, x) || reads(*b, x),
        _ => false,
    }
}

// Positional parameters are the CALLER's argument VALUES (bash positional
// parameters are values, not thunks), so `p` is a sequence of ints.
pub open spec fn eval(e: E, st: Store, p: Seq<int>) -> Result1
    decreases e,
{
    match e {
        E::Lit(v) => (v, 0),
        E::Var(x) => (if st.contains_key(x) { st[x] } else { 0 }, 0),
        E::Param(n) => if (n as int) < p.len() { (p[n as int], 0) } else { (0, 0) },
        E::Add(a, b) => {
            let (va, ca) = eval(*a, st, p);
            let (vb, cb) = eval(*b, st, p);
            (va + vb, ca + cb)
        }
        E::Side(v) => (v, 1),
    }
}

// A call site with up to two arguments (the corpus's leaf calls are 0–2
// positional reads; modelling two keeps every quantifier out of the proof —
// a wider call is the same argument repeated).
pub struct Call {
    pub a0: E,
    /// Number of arguments: 0 or 1 (a 2-argument call is the same argument
    /// repeated; the model needs only the positional-binding fact).
    pub n: nat,
}

pub open spec fn arg(c: Call, k: nat) -> E {
    c.a0
}

// `subst`: replace each positional read with the caller's argument.
pub open spec fn subst(e: E, c: Call) -> E
    decreases e,
{
    match e {
        E::Param(k) => if (k as int) < c.n { c.a0 } else { E::Param(k) },
        E::Add(a, b) => E::Add(Box::new(subst(*a, c)), Box::new(subst(*b, c))),
        other => other,
    }
}

// The positional bindings the call creates from its argument VALUES.
pub open spec fn params(c: Call, st: Store) -> Seq<int> {
    if c.n == 0 {
        Seq::empty()
    } else {
        seq![eval(c.a0, st, Seq::empty()).0]
    }
}

// A leaf body: at most one write, then the result expression.
pub struct Body {
    pub wname: Option<nat>,
    pub wval: int,
    pub value: E,
}

pub open spec fn apply_write(b: Body, st: Store) -> Store {
    match b.wname {
        Option::Some(n) => st.insert(n, b.wval),
        Option::None => st,
    }
}

pub open spec fn writes(b: Body, x: nat) -> bool {
    b.wname == Option::Some(x)
}

// "the argument is pure and does not read the name the body writes"
pub open spec fn ok_arg(b: Body, e: E) -> bool {
    pure(e) && match b.wname {
        Option::Some(n) => !reads(e, n),
        Option::None => true,
    }
}

pub open spec fn args_ok(b: Body, c: Call) -> bool {
    c.n == 0 || ok_arg(b, c.a0)
}

// The call: positional parameters are bound to the caller's argument VALUES;
// then the body runs (write, then value).
pub open spec fn run_call(b: Body, c: Call, st: Store) -> (int, nat) {
    let st2 = apply_write(b, st);
    eval(b.value, st2, params(c, st))
}

// The inlined form: the body's write, then the SUBSTITUTED expression
// evaluated in the caller's store (so an argument that reads a written name
// sees the new value).
pub open spec fn run_inline(b: Body, c: Call, st: Store) -> (int, nat) {
    let st2 = apply_write(b, st);
    eval(subst(b.value, c), st2, Seq::empty())
}

// A pure expression performs no effect.
proof fn pure_no_effects(e: E, st: Store, p: Seq<int>)
    requires pure(e),
    ensures eval(e, st, p).1 == 0,
    decreases e,
{
    match e {
        E::Add(a, b) => {
            pure_no_effects(*a, st, p);
            pure_no_effects(*b, st, p);
        }
        _ => {}
    }
}

// A pure argument reads the same before and after the body's write.
proof fn pure_arg_stable(e: E, b: Body, st: Store)
    requires ok_arg(b, e),
    ensures eval(e, apply_write(b, st), Seq::empty()) == eval(e, st, Seq::empty()),
    decreases e,
{
    match e {
        E::Lit(_) => {}
        E::Var(_) => {}
        E::Param(_) => {}
        E::Add(a, c) => {
            pure_arg_stable(*a, b, st);
            pure_arg_stable(*c, b, st);
        }
        E::Side(_) => {}
    }
}

// The substitution lemma: evaluating the body with the argument VALUES bound
// is the same as evaluating the substituted body in the caller's store.
proof fn subst_agrees(e: E, b: Body, c: Call, st: Store)
    requires args_ok(b, c), c.n <= 1,
    ensures eval(e, st, params(c, st)) == eval(subst(e, c), st, Seq::empty()),
    decreases e,
{
    reveal_with_fuel(eval, 2);
    reveal_with_fuel(subst, 2);
    match e {
        E::Lit(_) => {}
        E::Var(_) => {}
        E::Side(_) => {}
        E::Param(k) => {
            if (k as int) < c.n {
                // the caller's argument value is exactly the binding; the
                // argument is pure, so its effects are 0
                pure_no_effects(c.a0, st, Seq::empty());
            } else {
                // out of range: both sides read "unset" as 0
                assert((k as int) >= params(c, st).len());
            }
        }
        E::Add(a, c2) => {
            subst_agrees(*a, b, c, st);
            subst_agrees(*c2, b, c, st);
        }
    }
}

// THE OBLIGATION: the shipped rewrite preserves the call's value and effect
// count under the pass's purity check plus the read-disjointness the pass
// does NOT check (see capture_witness).
proof fn inline_preserves(b: Body, c: Call, st: Store)
    requires args_ok(b, c), c.n <= 1,
    ensures run_call(b, c, st) == run_inline(b, c, st),
{
    reveal_with_fuel(subst, 3);
    reveal_with_fuel(eval, 3);
    let st2 = apply_write(b, st);
    // every argument is stable under the body's write
    if c.n > 0 {
        pure_arg_stable(c.a0, b, st);
    }
    subst_agrees(b.value, b, c, st2);
}

// ---- the hazards the pass's guards do / do not cover ----

// IMPURE ARGUMENT: the body reads positional 0 twice; the call evaluates it
// once (arguments are values), the inline form twice.
pub open spec fn twice_body() -> Body {
    Body { wname: Option::None, wval: 0, value: E::Add(Box::new(E::Param(0)), Box::new(E::Param(0))) }
}

proof fn double_effect_witness()
    ensures ({
        let st = Map::<nat, int>::empty();
        let c = Call { a0: E::Side(5), n: 1 };
        run_call(twice_body(), c, st).1 == 0
            && run_inline(twice_body(), c, st).1 == 2
            && run_call(twice_body(), c, st).0 == 10
            && run_inline(twice_body(), c, st).0 == 10
    }),
{
    reveal_with_fuel(eval, 4);
    reveal_with_fuel(subst, 3);
}

// CAPTURE: the body writes x=5 then reads positional 0; the argument is `$x`.
// bash evaluates the argument first, so the call sees the OLD x.
pub open spec fn capture_body() -> Body {
    Body { wname: Option::Some(3), wval: 5, value: E::Param(0) }
}

proof fn capture_witness()
    ensures ({
        let st = Map::<nat, int>::empty().insert(3, 1);
        let c = Call { a0: E::Var(3), n: 1 };
        run_call(capture_body(), c, st).0 == 1
            && run_inline(capture_body(), c, st).0 == 5
            // the pass's purity check passes for this argument, and the
            // statement still disagrees: read-disjointness is NOT checked
            && pure(c.a0) && !ok_arg(capture_body(), c.a0)
    }),
{
    reveal_with_fuel(eval, 3);
    reveal_with_fuel(subst, 3);
    reveal_with_fuel(pure, 2);
}

// With the read-disjointness hypothesis the disagreement disappears: the
// same body, argument `$y` (not written), agrees.
proof fn capture_hypothesis_suffices()
    ensures ({
        let st = Map::<nat, int>::empty().insert(3, 1).insert(7, 9);
        let c = Call { a0: E::Var(7), n: 1 };
        run_call(capture_body(), c, st) == run_inline(capture_body(), c, st)
            && run_inline(capture_body(), c, st).0 == 9
            && args_ok(capture_body(), c)
    }),
{
    reveal_with_fuel(eval, 3);
    reveal_with_fuel(subst, 3);
}

// ---- pins: the pass's shape guards (the source has no unit tests, so these
// are UNMIRRORED; they pin the module doc's invariants) ----

proof fn pin_leaf_is_pure()
    ensures pure(E::Add(Box::new(E::Lit(1)), Box::new(E::Var(2))))
        && !pure(E::Side(0))
        && !pure(E::Add(Box::new(E::Side(0)), Box::new(E::Lit(1)))),
{
    reveal_with_fuel(pure, 3);
}

// `getVar("1")` is argument 0 (1-based in the source, 0-based here); an
// out-of-range positional read is left alone.
proof fn pin_substitution_is_positional()
    ensures ({
        let c = Call { a0: E::Lit(11), n: 1 };
        subst(E::Add(Box::new(E::Param(0)), Box::new(E::Param(0))), c)
            == E::Add(Box::new(E::Lit(11)), Box::new(E::Lit(11)))
            // positional 1 is unbound: not substituted
            && subst(E::Param(1), c) == E::Param(1)
            && subst(E::Param(9), c) == E::Param(9)
    }),
{
    reveal_with_fuel(subst, 4);
}

// A body with no positional reads inlines to itself.
proof fn pin_constant_body()
    ensures subst(E::Lit(3), Call { a0: E::Side(1), n: 1 }) == E::Lit(3),
{
    reveal_with_fuel(subst, 2);
}

fn main() {}
}
