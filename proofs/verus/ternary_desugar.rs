// Standalone model of the ternary-desugar Tier-1 obligations
// (`sh2perl/src/transforms/ternary_desugar.rs`): rewriting the C
// frontend's `ternary(cond, a, b)` call into the backend-neutral
// `Ternary{cond, then, else}` node (plus the `testArith(s)`-to-`arith(s)`
// normalization) preserves the selected branch.
//
// Proves the soundness obligations:
//
//   ternary_sound: the lowered Ternary selects the same branch as the
//   source `ternary()` call for every predicate value.
//   testarith_sound: normalizing `testArith(s)` to `arith(s)` denotes
//   the same predicate.
//
// Branch values are nat ids; conditions are opaque predicates over
// strings. The `test`/`arith` string grammars are backend renderers'
// business — the model takes already-classified condition shapes as
// input (TestStr vs ArithStr classification via `is_test_string` is
// string parsing, out of scope) and constrains both sides to call the
// SAME oracle, exactly as the glob arm of case_lowering does.
//
// What is modelled and what is NOT (fidelity, checked against the source):
//   - the exact cond table: Str test-string -> `test(s)`; Str
//     non-test-string -> `arith(s)`; `test`/`arith` calls pass through;
//     `testArith(c)` normalizes to `arith(c)` (the universally-rendered
//     form); anything else refuses (`None` — the call stays for the
//     backend's own render, which on most backends is an undefined
//     sub, i.e. the pre-existing behavior).
//   - the standalone `testArith(s)` -> `arith(s)` rewrite (same
//     normalization, statement-position rule).
//   - traversal (`walk_stmt`/`walk_expr` recursion, including the
//     ternary-before-descend ordering that keeps a nested testArith
//     cond from pre-rewriting) is out of scope: each `ternary()` site
//     is an independent local rewrite — the theorem is per-site, as in
//     every fold here.
//   - the estree-path exemption (estree renders `ternary` natively, so
//     the transform does not run there) is a pipeline placement fact,
//     not a value claim.
//   - backend agreement on the `test`/`arith` string grammars is out of
//     scope (renderer contract): the obligation is that desugaring
//     preserves the predicate call, not what any engine computes.
use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

// Backend predicate engines (shared oracles — the rewrite preserves the
// call, it does not decide what any engine computes).
pub open spec fn test_den(s: Str) -> bool {
    arbitrary()
}

pub open spec fn arith_den(s: Str) -> bool {
    arbitrary()
}

// Normalized condition forms (what every renderer lowers natively).
pub enum CondForm {
    Test(Str),
    Arith(Str),
}

pub open spec fn ev_cond(c: CondForm) -> bool {
    match c {
        CondForm::Test(s) => test_den(s),
        CondForm::Arith(s) => arith_den(s),
    }
}

// Classified source condition shapes (the is_test_string split and the
// call-form match are string/shape parsing — inputs here).
pub enum TArg {
    TestStr(Str),
    ArithStr(Str),
    TestCall(Str),
    ArithCall(Str),
    TestArithCall(Str),
    Other,
}

// The source ternary's branch predicate.
pub open spec fn src_pred(a: TArg) -> bool {
    match a {
        TArg::TestStr(s) => test_den(s),
        TArg::ArithStr(s) => arith_den(s),
        TArg::TestCall(s) => test_den(s),
        TArg::ArithCall(s) => arith_den(s),
        TArg::TestArithCall(s) => arith_den(s),
        TArg::Other => false,
    }
}

// Normalization: every accepted shape maps to its native call form.
pub open spec fn norm_arg(a: TArg) -> Option<CondForm> {
    match a {
        TArg::TestStr(s) => Some(CondForm::Test(s)),
        TArg::ArithStr(s) => Some(CondForm::Arith(s)),
        TArg::TestCall(s) => Some(CondForm::Test(s)),
        TArg::ArithCall(s) => Some(CondForm::Arith(s)),
        TArg::TestArithCall(s) => Some(CondForm::Arith(s)),
        TArg::Other => None,
    }
}

// Source and lowered ternary nodes (branch values are nat ids).
pub struct SrcTern {
    pub arg: TArg,
    pub then: nat,
    pub els: nat,
}

pub struct LowTern {
    pub cond: CondForm,
    pub then: nat,
    pub els: nat,
}

pub open spec fn ev_src(t: SrcTern) -> nat {
    if src_pred(t.arg) {
        t.then
    } else {
        t.els
    }
}

pub open spec fn ev_low(t: LowTern) -> nat {
    if ev_cond(t.cond) {
        t.then
    } else {
        t.els
    }
}

pub open spec fn lower_tern(t: SrcTern) -> Option<LowTern> {
    match norm_arg(t.arg) {
        Some(c) => Some(LowTern { cond: c, then: t.then, els: t.els }),
        None => None,
    }
}

// THE OBLIGATION (ternary): lowering preserves the selected branch.
proof fn ternary_sound(t: SrcTern)
    requires lower_tern(t).is_Some(),
    ensures ev_low(lower_tern(t).get_Some_0()) == ev_src(t),
{
    match t.arg {
        TArg::TestStr(_) => {},
        TArg::ArithStr(_) => {},
        TArg::TestCall(_) => {},
        TArg::ArithCall(_) => {},
        TArg::TestArithCall(_) => {},
        TArg::Other => {},
    }
}

// Standalone testArith normalization.
pub open spec fn lower_testarith(a: TArg) -> Option<CondForm> {
    match a {
        TArg::TestArithCall(s) => Some(CondForm::Arith(s)),
        _ => None,
    }
}

// THE OBLIGATION (testarith): the normalization denotes the same
// predicate.
proof fn testarith_sound(a: TArg)
    requires lower_testarith(a).is_Some(),
    ensures ev_cond(lower_testarith(a).get_Some_0()) == src_pred(a),
{
    match a {
        TArg::TestArithCall(_) => {},
        _ => {},
    }
}

// ---- Fidelity pins (model-internal: the transform has no unit suite;
// the pins record the exact mapping table, UNMIRRORED by design) ----

// A test-string Str becomes `test(s)`.
proof fn pin_teststr_becomes_test()
    ensures norm_arg(TArg::TestStr(seq!['a'])) == Some(CondForm::Test(seq!['a'])),
{
}

// A non-test-string Str becomes `arith(s)`.
proof fn pin_arithstr_becomes_arith()
    ensures norm_arg(TArg::ArithStr(seq!['1', '+', '1'])) == Some(CondForm::Arith(
        seq!['1', '+', '1'],
    )),
{
}

// test/arith calls pass through unchanged.
proof fn pin_calls_pass_through()
    ensures norm_arg(TArg::TestCall(seq!['a'])) == Some(CondForm::Test(seq!['a'])) && norm_arg(
        TArg::ArithCall(seq!['x']),
    ) == Some(CondForm::Arith(seq!['x'])),
{
}

// testArith normalizes to the universally-rendered arith form.
proof fn pin_testarith_normalizes()
    ensures norm_arg(TArg::TestArithCall(seq!['c'])) == Some(CondForm::Arith(seq!['c'])) && lower_testarith(
        TArg::TestArithCall(seq!['c']),
    ) == Some(CondForm::Arith(seq!['c'])),
{
}

// Anything else refuses (the call stays for the backend's own render).
proof fn pin_other_refuses()
    ensures norm_arg(TArg::Other).is_None() && lower_tern(SrcTern {
        arg: TArg::Other,
        then: 1nat,
        els: 2nat,
    }).is_None() && lower_testarith(TArg::TestStr(seq!['a'])).is_None(),
{
}

// Branch selection is preserved on a live predicate.
proof fn pin_branch_preserved()
    ensures ev_low(lower_tern(SrcTern {
        arg: TArg::TestStr(seq!['a']),
        then: 1nat,
        els: 2nat,
    }).get_Some_0()) == ev_src(SrcTern { arg: TArg::TestStr(seq!['a']), then: 1nat, els: 2nat }),
{
    ternary_sound(SrcTern { arg: TArg::TestStr(seq!['a']), then: 1nat, els: 2nat });
}

fn main() {}

}
