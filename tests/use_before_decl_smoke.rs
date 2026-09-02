//! Standalone verification of the use-before-declaration analysis.
//!
//! This lives in `tests/` (a separate test target) on purpose: this
//! repo's `cargo test --lib` harness only collects `bc_native` and
//! `src/lib.rs`'s own `tests` module, so the `shir_passes` unit-test
//! modules are not exercised by `--lib`. The integration-test target
//! compiles against the public API and runs normally.

use debashl::ir::{AssignTarget, IrExpr, IrProgram, IrStmt};
use debashl::shir_passes::used_before_decl::analyze_use_before_decl;

fn empty() -> IrProgram {
    IrProgram {
        var_nospace: vec![],
        var_storage: vec![],
        var_bash_env: vec![],
        imports: vec![],
        requires: vec![],
        stmts: vec![],
        subs: vec![],
        var_types: vec![],
        stmt_lines: vec![],
        var_lengths: vec![],
        var_const: vec![],
        var_lifetimes: vec![],
    }
}

fn v(s: &str) -> IrExpr {
    IrExpr::Var(s.to_string(), None)
}

fn out(e: IrExpr) -> IrStmt {
    IrStmt::Output {
        value: e,
        newline: true,
        target: None,
    }
}

fn assign(name: &str, e: IrExpr) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: name.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: e,
        asm: None,
    }
}

#[test]
fn straight_line_use_before_decl() {
    // echo $x; x=1  -> x read at pos 1 before its def at pos 2.
    let f = analyze_use_before_decl(&IrProgram {
        stmts: vec![out(v("x")), assign("x", IrExpr::Int(1))],
        ..empty()
    });
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].var, "x");
    assert_eq!(f[0].stmt_pos, 1);
}

#[test]
fn defined_before_use_is_clean() {
    // x=1; echo $x  -> no finding.
    let f = analyze_use_before_decl(&IrProgram {
        stmts: vec![assign("x", IrExpr::Int(1)), out(v("x"))],
        ..empty()
    });
    assert!(f.is_empty());
}

#[test]
fn branch_meet_suppresses_when_only_one_side_defines() {
    // if true; then x=1; fi; echo $x  -> not must-defined after the if.
    let f = analyze_use_before_decl(&IrProgram {
        stmts: vec![
            IrStmt::If {
                cond: IrExpr::Bool(true),
                then: vec![assign("x", IrExpr::Int(1))],
                elsifs: vec![],
                else_: vec![],
            },
            out(v("x")),
        ],
        ..empty()
    });
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].var, "x");
    assert_eq!(f[0].stmt_pos, 2);
}

#[test]
fn branch_both_sides_define_is_clean() {
    // if true; then x=1; else x=2; fi; echo $x  -> must-defined everywhere.
    let f = analyze_use_before_decl(&IrProgram {
        stmts: vec![
            IrStmt::If {
                cond: IrExpr::Bool(true),
                then: vec![assign("x", IrExpr::Int(1))],
                elsifs: vec![],
                else_: vec![assign("x", IrExpr::Int(2))],
            },
            out(v("x")),
        ],
        ..empty()
    });
    assert!(f.is_empty());
}

#[test]
fn loop_body_def_does_not_propagate() {
    // while true; do x=1; done; echo $x  -> loop may run 0 times.
    let f = analyze_use_before_decl(&IrProgram {
        stmts: vec![
            IrStmt::While {
                cond: IrExpr::Bool(true),
                body: vec![assign("x", IrExpr::Int(1))],
            },
            out(v("x")),
        ],
        ..empty()
    });
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].var, "x");
    assert_eq!(f[0].stmt_pos, 2);
}

#[test]
fn function_capture_of_top_level_is_clean() {
    // x=5; f(){ echo $x }  -> x is a program global; no warning.
    let f = analyze_use_before_decl(&IrProgram {
        stmts: vec![
            assign("x", IrExpr::Int(5)),
            IrStmt::Function {
                name: "f".to_string(),
                body: vec![out(v("x"))],
                named_blocks: vec![],
            },
        ],
        ..empty()
    });
    assert!(f.is_empty());
}

#[test]
fn function_body_read_of_never_assigned_warns() {
    // f(){ echo $typo }  -> $typo never assigned anywhere.
    let f = analyze_use_before_decl(&IrProgram {
        stmts: vec![IrStmt::Function {
            name: "f".to_string(),
            body: vec![out(IrExpr::Var("typo".to_string(), None))],
            named_blocks: vec![],
        }],
        ..empty()
    });
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].var, "typo");
}

#[test]
fn deterministic_sorted_output() {
    let f1 = analyze_use_before_decl(&IrProgram {
        stmts: vec![out(v("zeta")), out(v("alpha"))],
        ..empty()
    });
    let f2 = analyze_use_before_decl(&IrProgram {
        stmts: vec![out(v("zeta")), out(v("alpha"))],
        ..empty()
    });
    assert_eq!(f1, f2);
    let names: Vec<&str> = f1.iter().map(|x| x.var.as_str()).collect();
    assert_eq!(names, vec!["alpha", "zeta"]);
}
