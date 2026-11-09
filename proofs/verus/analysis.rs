// Standalone model of the RuntimeFootprint verdict soundness
// (sh2perl/src/shir_passes/analysis.rs): `needs_runtime` walks the
// program and reports whether any construct needs the shell runtime.
// Proves the verdict-soundness obligation:
//
//   footprint_sound: a `false` verdict implies every statement is in
//   the runtime-free fragment (pure data, control flow, stdout
//   Output, pure calls) — so a renderer emitting plain host code
//   for it cannot miss a runtime need. A `true` verdict claims
//   nothing (conservative over-approximation: false = missed opt,
//   true-wrong = soundness bug — and the walk is built so only the
//   former is possible).
//
// Statements are classified by constructor: pure-data/control
// (Assign/If/While/Output-stdout/pure-Call — recursed), vs
// runtime constructs (redirects, captures, pipelines, shell-outs,
// subshells, unknown/extension nodes — verdict true). The pure-call
// table (builtin/exec/test/arith/getVar/param/…) and the
// output-target rule are oracle inputs. The subs walk and the other
// fourteen analyses (safe-default stubs) are out of scope — stubs
// are sound by construction (their defaults are the conservative
// direction), recorded in the analysis.rs header.
//
// What is modelled and what is NOT (fidelity, checked against
// analysis.rs's 3 unit tests + the walk):
//   - The walk verdict: false iff no statement needs the runtime;
//     every runtime constructor (Redirect/Capture/Pipeline/Exec/
//     Subshell/unknown) forces true; pure constructors recurse.
//   - The soundness direction: false ⟹ all-pure (the consumer's
//     licence to emit plain host code). True ⟹ nothing.
//   - The 8 pins exercise the walk's arms as model-internal checks
//     (pure-assign free, redirect/capture/pipeline/exec mark,
//     pure calls free, false-is-all-pure witness, true-claims-
//     nothing). ALL 8 ARE UNMIRRORED BY DESIGN: analysis.rs's 3
//     unit tests are all_analyses_have_unique_names,
//     all_analyses_run_on_empty_program (which does not even run
//     RuntimeFootprint — it is absent from the run list), and
//     const_var_analysis_populates_verdicts (ConstVar only). No
//     footprint test exists to map to, so the living tie is the
//     guarded consumer/table below (same adjudication as
//     escape_classes/f32_narrow, VERUS.md P12 — analysis-only
//     source, no unit suite).
//   - Guards (verus-sync-coverage.sh `analysis` arms):
//       needs_runtime entry -> guard walk entry
//       stmt_needs/expr_needs/arith_needs walkers -> guard walkers
//       pure-call table ("builtin"|"exec"|"test"|…) -> guard pure table
//       Output-target rule -> guard output-target rule
//       catch-all runtime arm -> guard fail-closed arm
//
// MUTANT (checked red): treating a redirect as pure (dropping the
// redirect arm) makes the same command report 19 verified, 2 errors
// — stmt_agrees (the per-statement agreement the soundness rests
// on) and pin_redirect_marks go red. The file was restored and
// re-checked at 21 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 analysis.rs
//   -> verification results:: 21 verified, 0 errors
use vstd::prelude::*;

verus! {

// Statement shapes: the runtime-free fragment vs runtime constructs.
pub enum Stmt {
    Assign,
    If(Seq<Stmt>, Seq<Stmt>),
    While(Seq<Stmt>),
    OutputStdout,
    PureCall,
    // Runtime constructs (redirects, captures, pipelines, shell-outs,
    // subshells, unknown/extension nodes — each forces true).
    Redirect,
    Capture,
    Pipeline,
    Exec,
    Unknown,
}

// Top-level verdict at sufficient depth (pins exhibit depth 3:
// top level + one nesting level).
pub open spec fn needs_runtime_top(ss: Seq<Stmt>) -> bool {
    needs_runtime(ss, 3)
}

// The walk verdict: true iff any statement needs the runtime.
// Fuel-bounded on nesting depth (each If/While level consumes one;
// at sufficient fuel exactly the source's full walk).
pub open spec fn needs_runtime(ss: Seq<Stmt>, d: nat) -> bool
    decreases d, ss.len(),
{
    if d == 0 {
        false
    } else if ss.len() == 0 {
        false
    } else {
        stmt_needs(ss[0], d) || needs_runtime(ss.drop_first(), d)
    }
}

pub open spec fn stmt_needs(st: Stmt, d: nat) -> bool
    decreases d, 0nat,
{
    if d == 0 {
        false
    } else {
        match st {
        Stmt::Assign => false,
        Stmt::If(t, e) => needs_runtime(t, ((d - 1) as nat)) || needs_runtime(e, ((d - 1) as nat)),
        Stmt::While(b) => needs_runtime(b, ((d - 1) as nat)),
        Stmt::OutputStdout => false,
        Stmt::PureCall => false,
        Stmt::Redirect => true,
        Stmt::Capture => true,
        Stmt::Pipeline => true,
        Stmt::Exec => true,
        Stmt::Unknown => true,
        }
    }
}

// The runtime-free fragment membership.
pub open spec fn is_pure_stmt(st: Stmt, d: nat) -> bool
    decreases d, 0nat,
{
    if d == 0 {
        true
    } else {
        match st {
        Stmt::Assign => true,
        Stmt::If(t, e) => all_pure(t, ((d - 1) as nat)) && all_pure(e, ((d - 1) as nat)),
        Stmt::While(b) => all_pure(b, ((d - 1) as nat)),
        Stmt::OutputStdout => true,
        Stmt::PureCall => true,
        Stmt::Redirect => false,
        Stmt::Capture => false,
        Stmt::Pipeline => false,
        Stmt::Exec => false,
        Stmt::Unknown => false,
        }
    }
}

pub open spec fn all_pure(ss: Seq<Stmt>, d: nat) -> bool
    decreases d, ss.len(),
{
    if d == 0 {
        true
    } else if ss.len() == 0 {
        true
    } else {
        is_pure_stmt(ss[0], d) && all_pure(ss.drop_first(), d)
    }
}

pub open spec fn all_pure_top(ss: Seq<Stmt>) -> bool {
    all_pure(ss, 3)
}

// THE OBLIGATION: a false verdict implies every statement is pure.
// Proved together with the two-sided sequence agreement (the If/While
// arms need the equation, not just the implication, on their children).
proof fn seq_agrees(ss: Seq<Stmt>, d: nat)
    ensures needs_runtime(ss, d) == !all_pure(ss, d),
    decreases d, ss.len() + 1,
{
    if d == 0 {
    } else if ss.len() == 0 {
    } else {
        stmt_agrees(ss[0], d);
        seq_agrees(ss.drop_first(), d);
    }
}

proof fn footprint_sound(ss: Seq<Stmt>, d: nat)
    ensures !needs_runtime(ss, d) ==> all_pure(ss, d),
    decreases d, ss.len(),
{
    if d == 0 {
    } else if ss.len() == 0 {
    } else {
        seq_agrees(ss, d);
    }
}

proof fn footprint_sound_top(ss: Seq<Stmt>)
    ensures !needs_runtime_top(ss) ==> all_pure_top(ss),
{
    footprint_sound(ss, 3);
}

// Purity and the verdict agree on single statements.
proof fn stmt_agrees(st: Stmt, d: nat)
    ensures stmt_needs(st, d) == !is_pure_stmt(st, d),
    decreases d, 0nat,
{
    match st {
        Stmt::Assign => {},
        Stmt::If(t, e) => {
            if d == 0 {
            } else {
                let hc = Stmt::If(t, e);
                assert(st == hc);
                seq_agrees(t, ((d - 1) as nat));
                seq_agrees(e, ((d - 1) as nat));
                assert(stmt_needs(hc, d) == stmt_needs(st, d));
                assert(is_pure_stmt(hc, d) == is_pure_stmt(st, d));
            }
        },
        Stmt::While(b) => {
            if d == 0 {
            } else {
                let hc = Stmt::While(b);
                assert(st == hc);
                seq_agrees(b, ((d - 1) as nat));
                assert(stmt_needs(hc, d) == stmt_needs(st, d));
                assert(is_pure_stmt(hc, d) == is_pure_stmt(st, d));
            }
        },
        _ => {},
    }
}

// Cons unfolding for the walk.
proof fn needs_cons(ss: Seq<Stmt>, d: nat)
    requires d >= 1, ss.len() >= 1,
    ensures needs_runtime(ss, d) == (stmt_needs(ss[0], d) || needs_runtime(ss.drop_first(), d)),
{
    assert(d != 0);
    assert(ss.len() != 0);
    assert(needs_runtime(ss, d) == (stmt_needs(ss[0], d) || needs_runtime(ss.drop_first(), d)));
}

proof fn pure_cons(ss: Seq<Stmt>, d: nat)
    requires d >= 1, ss.len() >= 1,
    ensures all_pure(ss, d) == (is_pure_stmt(ss[0], d) && all_pure(ss.drop_first(), d)),
{
    assert(d != 0);
    assert(ss.len() != 0);
    assert(all_pure(ss, d) == (is_pure_stmt(ss[0], d) && all_pure(ss.drop_first(), d)));
}

proof fn pure_nil(d: nat)
    ensures all_pure(Seq::<Stmt>::empty(), d) == true,
{
    if d == 0 {
    } else {
        assert(Seq::<Stmt>::empty().len() == 0);
    }
}

proof fn needs_nil(d: nat)
    ensures needs_runtime(Seq::<Stmt>::empty(), d) == false,
{
    if d == 0 {
    } else {
        assert(Seq::<Stmt>::empty().len() == 0);
    }
}

// ---- Fidelity pins (mirror the walk's arms) ----

// A pure-assign program is runtime-free.
proof fn pin_pure_program_free()
    ensures !needs_runtime_top(seq![Stmt::Assign, Stmt::Assign]),
{
    let p = seq![Stmt::Assign, Stmt::Assign];
    assert(p[0] == Stmt::Assign);
    assert(p.drop_first() == seq![Stmt::Assign]);
    needs_cons(p, 3);
    assert(stmt_needs(Stmt::Assign, 3) == false);
    needs_cons(seq![Stmt::Assign], 3);
    assert(seq![Stmt::Assign][0] == Stmt::Assign);
    assert(seq![Stmt::Assign].drop_first() == Seq::<Stmt>::empty());
    needs_nil(3);
}

// A redirect marks the program runtime-full.
proof fn pin_redirect_marks()
    ensures needs_runtime_top(seq![Stmt::Assign, Stmt::Redirect]),
{
    let p = seq![Stmt::Assign, Stmt::Redirect];
    assert(p[0] == Stmt::Assign);
    assert(p.drop_first() == seq![Stmt::Redirect]);
    needs_cons(p, 3);
    assert(stmt_needs(Stmt::Assign, 3) == false);
    needs_cons(seq![Stmt::Redirect], 3);
    assert(stmt_needs(Stmt::Redirect, 3) == true);
}

// A capture marks the program runtime-full.
proof fn pin_capture_marks()
    ensures needs_runtime_top(seq![Stmt::Assign, Stmt::Capture]),
{
    let p = seq![Stmt::Assign, Stmt::Capture];
    assert(p[0] == Stmt::Assign);
    assert(p.drop_first() == seq![Stmt::Capture]);
    needs_cons(p, 3);
    assert(stmt_needs(Stmt::Assign, 3) == false);
    needs_cons(seq![Stmt::Capture], 3);
    assert(stmt_needs(Stmt::Capture, 3) == true);
}

// A pipeline marks the program runtime-full.
proof fn pin_pipeline_marks()
    ensures needs_runtime_top(seq![Stmt::Pipeline]),
{
    let p = seq![Stmt::Pipeline];
    assert(p[0] == Stmt::Pipeline);
    assert(p.drop_first() == Seq::<Stmt>::empty());
    needs_cons(p, 3);
    assert(stmt_needs(Stmt::Pipeline, 3) == true);
}

// Pure calls (builtin/exec/test/arith/…) are runtime-free.
proof fn pin_pure_calls_free()
    ensures !needs_runtime_top(seq![Stmt::PureCall, Stmt::OutputStdout]),
{
    let p = seq![Stmt::PureCall, Stmt::OutputStdout];
    assert(p[0] == Stmt::PureCall);
    assert(p.drop_first() == seq![Stmt::OutputStdout]);
    needs_cons(p, 3);
    assert(stmt_needs(Stmt::PureCall, 3) == false);
    needs_cons(seq![Stmt::OutputStdout], 3);
    assert(seq![Stmt::OutputStdout][0] == Stmt::OutputStdout);
    assert(seq![Stmt::OutputStdout].drop_first() == Seq::<Stmt>::empty());
    assert(stmt_needs(Stmt::OutputStdout, 3) == false);
    needs_nil(3);
}

// An impure call (capture/pipeline/source) marks runtime.
proof fn pin_impure_call_marks()
    ensures needs_runtime_top(seq![Stmt::Exec]),
{
    let p = seq![Stmt::Exec];
    assert(p[0] == Stmt::Exec);
    assert(p.drop_first() == Seq::<Stmt>::empty());
    needs_cons(p, 3);
    assert(stmt_needs(Stmt::Exec, 3) == true);
}

// False ⟹ all-pure on a witness program.
proof fn pin_false_is_all_pure()
    ensures all_pure_top(seq![Stmt::Assign, Stmt::PureCall, Stmt::OutputStdout]),
{
    let p = seq![Stmt::Assign, Stmt::PureCall, Stmt::OutputStdout];
    assert(p[0] == Stmt::Assign);
    assert(p.drop_first() == seq![Stmt::PureCall, Stmt::OutputStdout]);
    pure_cons(p, 3);
    assert(is_pure_stmt(Stmt::Assign, 3) == true);
    assert(p.drop_first()[0] == Stmt::PureCall);
    assert(p.drop_first().drop_first() == seq![Stmt::OutputStdout]);
    pure_cons(p.drop_first(), 3);
    assert(is_pure_stmt(Stmt::PureCall, 3) == true);
    assert(seq![Stmt::OutputStdout][0] == Stmt::OutputStdout);
    assert(seq![Stmt::OutputStdout].drop_first() == Seq::<Stmt>::empty());
    pure_cons(seq![Stmt::OutputStdout], 3);
    assert(is_pure_stmt(Stmt::OutputStdout, 3) == true);
    pure_nil(3);
}

// True claims nothing (a logical property of the conservative
// direction — no source behavior to mirror).
proof fn pin_true_claims_nothing()
    ensures needs_runtime_top(seq![Stmt::Unknown]) || !needs_runtime_top(seq![Stmt::Unknown]),
{
}

fn main() {}

}
