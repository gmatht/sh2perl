//! Standalone verification of the `.lint` backend (`shir_to_lint`).
//! Like `use_before_decl_smoke.rs`, this lives in `tests/` because this
//! repo's `cargo test --lib` harness doesn't collect `shir_passes` unit
//! suites. It drives the renderer through the canonical pipeline exactly
//! as the `--shir-in-lint` CLI arm does.

use debashl::ir::{AssignTarget, IrExpr, IrProgram, IrStmt, StrStyle};
use debashl::lint_backend::shir_to_lint;

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

fn fncall(name: &str) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "fnCall".to_string(),
        args: vec![IrExpr::Str(name.to_string(), StrStyle::SingleQuoted)],
    })
}

#[test]
fn lint_report_covers_easy_lints() {
    use debashl::ir::ArithAst;
    let prog = IrProgram {
        stmts: vec![
            out(v("x")),                          // use-before-decl x @1
            assign("y", IrExpr::Int(5)),          // unused-var y
            assign("unused_var", IrExpr::Int(42)), // unused-var unused_var
            IrStmt::Function {
                name: "foo".to_string(),
                body: vec![out(IrExpr::Str("in foo".into(), StrStyle::DoubleQuoted))],
                named_blocks: vec![],
            },
            IrStmt::Function {
                name: "bar".to_string(),
                body: vec![out(IrExpr::Str("in bar".into(), StrStyle::DoubleQuoted))],
                named_blocks: vec![],
            },
            fncall("foo"),                        // foo is used; bar is not
            IrStmt::Assign {
                targets: vec![AssignTarget { var: "z".into(), sigil: None, indices: vec![] }],
                expr: IrExpr::Arith(Box::new(ArithAst::Bin {
                    op: "+".into(),
                    lhs: Box::new(ArithAst::Var("z".into())),
                    rhs: Box::new(ArithAst::Num(1)),
                })),
                asm: None,
            },                                  // use-before-decl z @7 (z read in arith)
            assign("readonly_const", IrExpr::Int(7)), // const-candidate
            out(v("readonly_const")),
        ],
        ..empty()
    };

    let report = shir_to_lint(&prog);
    println!("REPORT:\n{report}");

    assert!(report.contains("use-before-decl: `$x` read at statement 1"));
    assert!(report.contains("use-before-decl: `$z` read at statement 7"));
    assert!(report.contains("unused-var: `$unused_var`"));
    assert!(report.contains("unused-var: `$y`"));
    assert!(report.contains("unused-function: `bar()`"));
    assert!(report.contains("const-candidate: `$readonly_const`"));

    // foo is called, so it must NOT be flagged unused.
    assert!(!report.contains("unused-function: `foo()`"));
    // readonly_const is read, so it must NOT be flagged unused.
    assert!(!report.contains("unused-var: `$readonly_const`"));
}

#[test]
fn lint_report_is_clean_for_trivial_program() {
    // a is read before each reassignment, so no dead store / unused / const.
    let prog = IrProgram {
        stmts: vec![
            assign("a", IrExpr::Int(1)),
            out(v("a")),
            assign("a", IrExpr::Int(2)),
            out(v("a")),
        ],
        ..empty()
    };
    assert_eq!(shir_to_lint(&prog), "");
}

#[test]
fn lint_report_covers_more_lints() {
    use debashl::ir::ArithAst;
    let prog = IrProgram {
        stmts: vec![
            // dead store: a=1 is overwritten by a=2 before any read
            assign("a", IrExpr::Int(1)),
            assign("a", IrExpr::Int(2)),
            out(v("a")),
            // unreachable: echo after return
            IrStmt::Return(Some(IrExpr::Int(0))),
            out(v("x")),
            // discarded capture: $(echo hi) as a bare statement
            IrStmt::Expr(IrExpr::Capture {
                expr: Box::new(IrExpr::Call {
                    func: "echo".to_string(),
                    args: vec![IrExpr::Str("hi".to_string(), StrStyle::SingleQuoted)],
                }),
                native: false,
            }),
            // arith dead store: ((b=1)); ((b=2)) with no read between
            IrStmt::Expr(IrExpr::Arith(Box::new(ArithAst::Assign {
                var: "b".to_string(),
                op: "=".to_string(),
                rhs: Box::new(ArithAst::Num(1)),
            }))),
            IrStmt::Expr(IrExpr::Arith(Box::new(ArithAst::Assign {
                var: "b".to_string(),
                op: "=".to_string(),
                rhs: Box::new(ArithAst::Num(2)),
            }))),
            out(v("b")),
        ],
        ..empty()
    };

    let report = shir_to_lint(&prog);
    println!("REPORT:\n{report}");

    assert!(report.contains("dead-store: `$a` assigned at statement 1"));
    assert!(report.contains("dead-store: `$b` assigned at statement 7"));
    assert!(report.contains("unreachable-code: statement 5"));
    assert!(report.contains("discarded-capture: command-substitution result at statement 6"));
    // the `set -e` info lint was deliberately dropped — it must never appear
    assert!(!report.contains("set -e"));
}
