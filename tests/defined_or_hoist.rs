//! v136 `DefinedOr` temp hoist — the shared IR transform plus the per-backend
//! bindings that keep the subject evaluated exactly once.
//!
//! The shell frontend lowers `${a:-b}` textually, so `IrExpr::DefinedOr` only
//! arrives from the A1 ingress path — these tests therefore build the IR node
//! directly and pin the rendered shape, which a corpus example cannot reach.

use debashl::ir::{AssignTarget, IrExpr, IrProgram, IrStmt, StrStyle};
use debashl::shir_passes::{definedor_hoist::DefinedOrHoist, PassContext, Pipeline, Transform};

fn program(stmts: Vec<IrStmt>) -> IrProgram {
    IrProgram {
        imports: vec![],
        requires: vec![],
        stmts,
        subs: vec![],
        var_types: vec![],
        stmt_lines: vec![],
        var_lengths: vec![],
        var_const: vec![],
        var_lifetimes: vec![],
        var_nospace: vec![],
        var_storage: vec![],
        var_bash_env: vec![],
    }
}

/// The `$(cmd)`-shaped subject: a call with an observable result.
fn subject() -> IrExpr {
    IrExpr::Call {
        func: "getVar".to_string(),
        args: vec![IrExpr::Str("y".to_string(), StrStyle::DoubleQuoted)],
    }
}

fn sample() -> IrProgram {
    program(vec![IrStmt::Assign {
        targets: vec![AssignTarget {
            var: "x".to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: IrExpr::DefinedOr {
            expr: Box::new(subject()),
            default: Box::new(IrExpr::Str("d".to_string(), StrStyle::DoubleQuoted)),
        },
        asm: None,
    }])
}

/// The transform binds the subject to a temp and leaves the assignment over it.
#[test]
fn hoist_splits_the_assignment() {
    let mut p = sample();
    DefinedOrHoist.run(&mut p, &PassContext::default());
    assert_eq!(p.stmts.len(), 2, "expected a temp binding + the assignment");
    match &p.stmts[0] {
        IrStmt::Assign { targets, expr, .. } => {
            assert_eq!(targets[0].var, "__sh2_dor_0");
            assert!(matches!(expr, IrExpr::Call { func, .. } if func == "getVar"));
        }
        other => panic!("expected the temp binding, got {other:?}"),
    }
    match &p.stmts[1] {
        IrStmt::Assign { expr: IrExpr::DefinedOr { expr, .. }, .. } => {
            assert_eq!(**expr, IrExpr::Var("__sh2_dor_0".to_string(), None));
        }
        other => panic!("expected the DefinedOr over the temp, got {other:?}"),
    }
}

/// The registration in `Pipeline::canonical` is what every backend inherits.
#[test]
fn canonical_pipeline_hoists() {
    let (_ctx, out, _m) = Pipeline::canonical().run(&sample());
    assert_eq!(out.stmts.len(), 2, "the canonical pipeline must run the hoist");
}

/// The live Perl path renders ONE subject: the temp is declared and reused.
#[test]
fn perl_renders_the_subject_once() {
    let mut p = sample();
    DefinedOrHoist.run(&mut p, &PassContext::default());
    let out = debashl::ir::shir_to_perl(&p);
    assert!(
        out.matches("__sh2_dor_v").count() >= 3,
        "expected a Perl temp declaration plus its two uses, got:\n{out}"
    );
    assert!(
        !out.contains("// "),
        "the old `(subject // default)` lowering is gone:\n{out}"
    );
}

/// The C renderer binds the subject in a statement-expression temp.
#[test]
fn c_renders_a_temp() {
    let mut p = sample();
    DefinedOrHoist.run(&mut p, &PassContext::default());
    let out = debashl::c_backend::shir_to_c(&p);
    assert!(
        out.contains("__sh2_dor_t"),
        "expected the C statement-expression temp, got:\n{out}"
    );
}

/// The Java renderer uses the lazy `shDefinedOr` helper (definition + call).
#[test]
fn java_renders_the_lazy_helper() {
    let mut p = sample();
    DefinedOrHoist.run(&mut p, &PassContext::default());
    let out = debashl::java_backend::shir_to_java(&p).expect("java render");
    assert!(
        out.matches("shDefinedOr").count() >= 2,
        "expected the helper definition and its call site, got:\n{out}"
    );
    assert!(
        out.contains("Supplier<String>"),
        "the default must stay lazy (a supplier), got:\n{out}"
    );
}

/// The A1 ingress is what actually reaches these arms — the shell frontend
/// lowers `${a:-b}` textually, so no corpus example can produce a
/// `DefinedOr`. Every assertion above starts from a hand-built `IrExpr`; this
/// one starts from A1 JSON, so a drift in the ingress (the `sigil`/`indices`
/// shape, a renamed node key) or in `Pipeline::canonical`'s registration fails
/// here instead of silently leaving three renderers untested by any gate.
#[test]
fn a1_json_reaches_the_renderer_arms() {
    let json = debashl::shir_json::shir_to_shir_json(&sample());
    let ingested = debashl::shir_json_in::shir_json_to_ir(&json)
        .expect("the A1 writer's own output must re-ingress");

    // The ingress really produced the node (not, say, a refused/unknown expr).
    match &ingested.stmts[0] {
        IrStmt::Assign { expr: IrExpr::DefinedOr { .. }, .. } => {}
        other => panic!("the A1 ingress did not decode the DefinedOr: {other:?}"),
    }

    let (_ctx, p, _m) = Pipeline::canonical().run(&ingested);
    assert_eq!(p.stmts.len(), 2, "canonical must hoist the INGESTED DefinedOr");
    match &p.stmts[1] {
        IrStmt::Assign { expr: IrExpr::DefinedOr { expr, .. }, .. } => {
            assert_eq!(**expr, IrExpr::Var("__sh2_dor_0".to_string(), None));
        }
        other => panic!("expected DefinedOr over the hoist temp, got {other:?}"),
    }

    let perl = debashl::ir::shir_to_perl(&p);
    assert!(perl.contains("__sh2_dor_v"), "perl temp missing:\n{perl}");
    let c = debashl::c_backend::shir_to_c(&p);
    assert!(c.contains("__sh2_dor_t"), "c temp missing:\n{c}");
    let java = debashl::java_backend::shir_to_java(&p).expect("java render");
    assert!(
        java.matches("shDefinedOr").count() >= 2,
        "java helper missing:\n{java}"
    );
}
