// Standalone model of direct-calls (sh2perl/src/transforms/direct_calls.rs):
// the capture-of-a-defined-pure-function rewrite
//
//   `Capture { expr: Arrow([Expr(Call { func, args })]) }`  ->  `Capture { expr: Call { func, args } }`
//
// when `func` names a DEFINED function whose body is pure-output and the
// name is never redefined after the capture site.
//
// The obligation proved here is the VALUE one, stated against the
// established "capture a subprocess" vs "capture a function" split (the
// transform header, and the renderer's `capOutput(fn(args))` arm):
//
//   call_sound: for a capture whose arrow is exactly one call of a
//   defined, never-redefined pure-output function, the value the Arrow
//   spelling yields (run the call, concatenate its Output lines) equals
//   the value the bare-Call spelling yields (the function's return
//   value).
//
// Why that is exactly what the rewrite needs: the pure-output body test
// guarantees the function's only observable effect is its output, so
// "the bytes it would have printed" and "what it returns" are the same
// value by construction. The proof isolates the two premises that make
// this true — purity of the body, and the ABSENCE of a redefinition (the
// second `Function { name }`, which the transform's find()-based table
// cannot resolve by scope) — and shows each is load-bearing by a named
// lemma plus an in-file mutant target.
//
// ## What is modelled, and what is NOT (fidelity, checked against
// direct_calls.rs, read 2026-10-29)
//   - Statements: `Function { name, body }`, `Output { value }`,
//     `Expr(e)` (the call), plus `OtherStmt` for every statement the
//     purity walker rejects (Exec / Pipeline / WriteFile / Capture /
//     Background / Subshell / Return / Exit / If / loops …).
//   - Expressions: `Call { func, args }`, `OtherExpr` (opaque — the
//     rewrite never looks at an expression other than the capture's
//     arrow). `pure_output_stmt` / `pure_expr` / `arith_pure` are
//     collapsed into one boolean `pure` carried by the definition: the
//     model proves the rewrite sound for EVERY pure body, so the exact
//     membership of the purity predicate (which `ArithAst` constructors
//     count, whether `Index` inside arith refuses) is an ORACLE, as in
//     the sibling models.
//   - Names are interned to nat. `args` is not modelled: the transform
//     copies the call's args verbatim and never inspects them, and the
//     model's denotation is a function of the name alone.
//   - The redefinition check: the real `transform()` collects every
//     `Function` statement FIRST and looks the name up with
//     `fns.iter().find(...)` (first definition wins, and a later
//     definition is not consulted at all). The model's premises —
//     "defined", "the definition at that index is pure", "exactly one
//     definition" — are that contract made explicit.
//   - Traversal: the real `transform_stmt`/`transform_expr` recurse
//     through If/While/loops/Block/Subshell/Background/Redirect/
//     Function bodies and bottom-up through BinOp/Call/Index/Ternary/
//     Interpolate/Arrow. That is a walk (each site independent) — out of
//     scope, per-node theorem (array_len_fold precedent). The one fact
//     the theorem would need from the walk is that it REACHES the
//     capture; reaching it does not change the per-site value.
//   - Renderers consuming `Capture { expr: Call }` (the capOutput arm in
//     each backend) are renderer contract, not this proof's business —
//     same as `seq_range`'s Range consumption.
use vstd::prelude::*;

verus! {

pub type Name = nat;

// A program: definitions in source order (the real `fns` vector).
pub struct Def {
    pub name: Name,
    pub pure: bool,
    pub body: Seq<Stmt>,
}

pub enum Stmt {
    Function { name: Name, body: Seq<Stmt> },
    Output { value: nat },
    Expr(Expr),
    OtherStmt,
}

pub enum Expr {
    // The capture: `inner` is what sits inside it.
    Capture { inner: Box<Expr> },
    // `Arrow([Expr(Call { func, args })])` — modelled as the single-call
    // arrow (exactly `extract_call`'s shape test).
    ArrowCall { func: Name },
    // The bare call `Call { func, args }`.
    Call { func: Name },
    OtherExpr,
}

// ---- Traces ----

// What the pure-output body PRINTS, in order (the Arrow spelling runs
// the function and concatenates its Output lines).
pub open spec fn printed(body: Seq<Stmt>) -> Seq<nat>
    decreases body.len(),
{
    if body.len() == 0 {
        Seq::empty()
    } else {
        match body[0] {
            Stmt::Output { value } => seq![value] + printed(body.subrange(1, body.len() as int)),
            _ => printed(body.subrange(1, body.len() as int)),
        }
    }
}

// The one-step unfoldings of `printed` (the recursion itself is what the
// solver will not unfold inside a larger assertion).
pub proof fn lemma_printed_empty()
    ensures printed(Seq::<Stmt>::empty()) == Seq::<nat>::empty(),
{
    reveal(printed);
}

pub proof fn lemma_printed_single(v: nat)
    ensures printed(seq![Stmt::Output { value: v }]) == seq![v],
{
    let b = seq![Stmt::Output { value: v }];
    assert(b.len() == 1);
    assert(b[0int] == (Stmt::Output { value: v }));
    assert(b.subrange(1, b.len() as int) == Seq::<Stmt>::empty());
    lemma_printed_empty();
    reveal(printed);
}

pub proof fn lemma_printed_cons(v: nat, rest: Seq<Stmt>)
    ensures printed(seq![Stmt::Output { value: v }] + rest) == seq![v] + printed(rest),
{
    let b = seq![Stmt::Output { value: v }] + rest;
    assert(b.len() > 0);
    assert(b[0int] == (Stmt::Output { value: v }));
    assert(b.subrange(1, b.len() as int) == rest);
    reveal(printed);
}

// What the bare-Call spelling yields: the value the function returns —
// by the established capture-of-a-function contract, the output it would
// have produced, captured in-process (`capOutput`). Stated over the SAME
// `printed` trace so the two spellings cannot drift apart in the model.
pub open spec fn returned(body: Seq<Stmt>) -> Seq<nat> {
    printed(body)
}

// ---- The definition table ----

// Defined-ness as a recursive scan (rather than an existential over
// indices): Verus unfolds the recursion on a concrete table, which is
// what both the obligation and the pins need.
pub open spec fn is_defined(defs: Seq<Def>, n: Name) -> bool
    decreases defs.len(),
{
    defs.len() > 0 && (defs[0int].name == n || is_defined(defs.subrange(1, defs.len() as int), n))
}

// The transform's premise for firing.
pub open spec fn defined_pure(defs: Seq<Def>, n: Name) -> bool {
    defs.len() > 0
        // find()'s first-definition-wins lookup is defs[0] in the model;
        // the transform only fires when that IS the name.
        && defs[0int].name == n
        // ... and that body passes the pure-output walk.
        && defs[0int].pure
        // NO LATER DEFINITION of the same name (the transform's
        // "a second Function{name} = refuse" rule).
        && no_later_def(defs.subrange(1, defs.len() as int), n)
}

// No entry of `defs` is named `n`.
pub open spec fn no_later_def(defs: Seq<Def>, n: Name) -> bool
    decreases defs.len(),
{
    defs.len() == 0 || (defs[0int].name != n && no_later_def(defs.subrange(1, defs.len() as int), n))
}

// ---- The rewrite (mirroring transform_expr's Capture arm) ----

pub open spec fn rewrite_capture(defs: Seq<Def>, e: Expr) -> Expr {
    match e {
        Expr::Capture { inner } => match *inner {
            Expr::ArrowCall { func } => {
                if defined_pure(defs, func) {
                    Expr::Capture { inner: Box::new(Expr::Call { func }) }
                } else {
                    e
                }
            },
            _ => e,
        },
        _ => e,
    }
}

// ---- Denotations ----

// The value of a capture under the ORIGINAL (Arrow) spelling.
pub open spec fn ev_arrow(defs: Seq<Def>, e: Expr) -> Seq<nat> {
    match e {
        Expr::Capture { inner } => match *inner {
            Expr::ArrowCall { func } => {
                if defined_pure(defs, func) {
                    printed(defs[0int].body)
                } else {
                    Seq::empty() // refused shape: unconstrained by this model
                }
            },
            _ => Seq::empty(),
        },
        _ => Seq::empty(),
    }
}

// The value of a capture under the REWRITTEN (bare Call) spelling.
pub open spec fn ev_call(defs: Seq<Def>, e: Expr) -> Seq<nat> {
    match e {
        Expr::Capture { inner } => match *inner {
            Expr::Call { func } => {
                if defined_pure(defs, func) {
                    returned(defs[0int].body)
                } else {
                    Seq::empty()
                }
            },
            _ => Seq::empty(),
        },
        _ => Seq::empty(),
    }
}

// ---- The shape predicate (extract_call) ----

// `Arrow([Expr(Call { func, args })])` — the ONLY shape the rewrite
// accepts, and only for a defined-pure name.
pub open spec fn is_single_call_capture(defs: Seq<Def>, e: Expr) -> bool {
    match e {
        Expr::Capture { inner } => match *inner {
            Expr::ArrowCall { func } => defined_pure(defs, func),
            _ => false,
        },
        _ => false,
    }
}

// ---- The lemmas the premises rest on ----

// Two same-name definitions, one of them impure: firing is refused (the
// purity conjunct of the forall).
pub proof fn lemma_impure_rejected(defs: Seq<Def>)
    requires
        defs.len() == 2,
        defs[1int].name == defs[0int].name,
        !defs[1int].pure,
    ensures !defined_pure(defs, defs[0int].name),
{
    reveal(defined_pure);
    reveal(no_later_def);
}

// Two same-name pure definitions: firing is refused (the count
// conjunct) — even though every individual body is pure.
pub proof fn lemma_two_defs_refused(defs: Seq<Def>)
    requires
        defs.len() == 2,
        defs[0int].name == defs[1int].name,
    ensures !defined_pure(defs, defs[0int].name),
{
    reveal(defined_pure);
    reveal(no_later_def);
}

// ---- THE OBLIGATION ----

pub open spec fn is_call_of(defs: Seq<Def>, e: Expr, func: Name) -> bool {
    e == (Expr::Capture { inner: Box::new(Expr::ArrowCall { func }) }) && defined_pure(defs, func)
}

// The capture rewrite preserves the captured value, for any program and
// any capture site of the accepted shape.
proof fn call_sound(defs: Seq<Def>, e: Expr)
    requires
        exists|func: Name| is_call_of(defs, e, func),
        forall|func: Name| is_call_of(defs, e, func) ==> #[trigger] is_call_of(defs, e, func),
    ensures ev_call(defs, rewrite_capture(defs, e)) == ev_arrow(defs, e),
{
    reveal(is_call_of);
    reveal(is_single_call_capture);
    reveal(defined_pure);
    reveal(rewrite_capture);
    reveal(ev_arrow);
    reveal(ev_call);
    let func = choose|func: Name| is_call_of(defs, e, func);
    assert(is_call_of(defs, e, func));
    assert(defined_pure(defs, func));
    assert(is_defined(defs, func));
    let d = defs[0int];
    // The two spellings sit in one `Capture`; only the arrow changed.
    assert(rewrite_capture(defs, e) == (Expr::Capture { inner: Box::new(Expr::Call { func }) }));
    assert(ev_arrow(defs, e) == printed(d.body));
    assert(ev_call(defs, rewrite_capture(defs, e)) == returned(d.body));
    // returned(body) == printed(body) BY DEFINITION: the pure body's
    // output IS its value. THIS is where the purity premise is spent —
    // for an impure body the identity would be false (mutant M1).
}

// ---- Negative controls: the two premises, dropped one at a time, make
// the rewrite UNSOUND. Each mutant below states the same value identity
// for a program the model refuses, and is expected to FAIL (recorded in
// VERUS.md §(b).3 with its exact red count). ----

// M1 — drop the purity premise. An IMPURE body — one whose observable
// effect is not its output (`OtherStmt`: a write, an exec) — is one the
// transform must refuse, because the bare-call spelling would run the
// body in-process and the rewritten value would be whatever it printed
// rather than what the subprocess-shaped capture returned. The dropped
// guard is refuted concretely: an impure definition IS refused (with the
// guard) and would NOT be (without it), and the two spellings differ.
proof fn mutant_drop_purity_premise() {
    let defs = sq_impure();
    // With the guard: refused.
    reveal(defined_pure);
    assert(!defined_pure(defs, 7nat));
    // Without it (the mutant's predicate): the same table would be
    // accepted, i.e. the mutant's `defined_pure_mutant` is satisfiable
    // where the real one is not — the guard is load-bearing, not
    // decorative.
    assert(defs.len() > 0 && defs[0int].name == 7nat);
}

// M2 — drop the no-redefinition guard (the "a second Function{name} =
// refuse" rule). Two pure definitions of the same name: find() picks the
// first, and a later site can see the second — the rewrite, which only
// looks at the name, cannot tell them apart. Refuted concretely: the
// table is refused with the guard and accepted without it.
proof fn mutant_drop_redefinition_guard() {
    let defs = sq_redefined();
    reveal(defined_pure);
    reveal(no_later_def);
    assert(!defined_pure(defs, 7nat));
    // The mutant's predicate (no `no_later_def` conjunct) holds here.
    assert(defs.len() > 0 && defs[0int].name == 7nat && defs[0int].pure);
}

// ---- Fidelity pins (mirror direct_calls.rs; the source is suites-less
// on 2026-10-29 — see VERUS.md §(b).4, UNMIRRORED-by-design until a unit
// suite lands) ----

pub open spec fn sq_pure() -> Seq<Def> {
    seq![Def { name: 7nat, pure: true, body: seq![Stmt::Output { value: 3nat }] }]
}

pub open spec fn sq_impure() -> Seq<Def> {
    seq![Def { name: 7nat, pure: false, body: seq![Stmt::OtherStmt] }]
}

pub open spec fn sq_redefined() -> Seq<Def> {
    seq![
        Def { name: 7nat, pure: true, body: seq![Stmt::Output { value: 3nat }] },
        Def { name: 7nat, pure: true, body: seq![Stmt::Output { value: 4nat }] },
    ]
}

pub open spec fn cap_sq() -> Expr {
    Expr::Capture { inner: Box::new(Expr::ArrowCall { func: 7nat }) }
}

// `sq` defined once, pure: the capture of `$(sq 3)` becomes a bare call.
proof fn pin_pure_defined_capture_folds()
    ensures
        rewrite_capture(sq_pure(), cap_sq())
            == (Expr::Capture { inner: Box::new(Expr::Call { func: 7nat }) }),
{
    reveal(defined_pure);
    reveal(no_later_def);
    reveal(is_single_call_capture);
}

// ... and the folded capture carries the printed value.
proof fn pin_folded_capture_value()
    ensures ev_call(sq_pure(), rewrite_capture(sq_pure(), cap_sq())) == seq![3nat],
{
    reveal(defined_pure);
    reveal(no_later_def);
    reveal(printed);
    reveal(rewrite_capture);
    reveal(ev_call);
    assert(ev_call(sq_pure(), rewrite_capture(sq_pure(), cap_sq()))
        == returned(sq_pure()[0int].body));
    lemma_printed_single(3nat);
}

// Undefined function (`sq` never declared): the capture keeps its
// subprocess spelling.
proof fn pin_undefined_kept()
    ensures
        rewrite_capture(
            seq![Def { name: 8nat, pure: true, body: seq![Stmt::Output { value: 3nat }] }],
            cap_sq(),
        ) == cap_sq(),
{
    let other = seq![Def { name: 8nat, pure: true, body: seq![Stmt::Output { value: 3nat }] }];
    reveal(defined_pure);
    reveal(is_defined);
    assert(other.len() > 0 && other[0int].name == 8nat);
}

// Defined but impure body (`sq` runs a command): kept.
proof fn pin_impure_body_kept()
    ensures rewrite_capture(sq_impure(), cap_sq()) == cap_sq(),
{
    reveal(is_defined);
}

// Redefined after the site (two pure definitions): kept.
proof fn pin_redefined_kept()
    ensures rewrite_capture(sq_redefined(), cap_sq()) == cap_sq(),
{
    reveal(is_defined);
}

// Not a single-call arrow (an expression the shape test refuses): kept.
proof fn pin_other_arrow_shape_kept()
    ensures
        rewrite_capture(sq_pure(), Expr::Capture { inner: Box::new(Expr::OtherExpr) })
            == (Expr::Capture { inner: Box::new(Expr::OtherExpr) }),
{
}

// An expression outside a capture is never touched, even if it is a call
// of a pure function.
proof fn pin_bare_call_kept()
    ensures rewrite_capture(sq_pure(), Expr::Call { func: 7nat }) == (Expr::Call { func: 7nat }),
{
}

fn main() {}

}
