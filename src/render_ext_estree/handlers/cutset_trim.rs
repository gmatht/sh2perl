//! CutsetTrim → native JS IIFE: scan past leading/trailing characters
//! present in the cutset, slice the remainder (see cutset_trim.node).
//! The arrow evaluates TEXT and CUTSET exactly once.

use crate::estree::Expr;
use crate::shir_nodes::CutsetTrim;

pub(crate) fn render(node: &CutsetTrim) -> Option<Expr> {
    let text = crate::shir::expr_to_estree_pub(&node.text);
    let cutset = crate::shir::expr_to_estree_pub(&node.cutset);

    let ident = |n: &str| Expr::Identifier {
        name: n.to_string(),
    };
    let num = |v: i64| Expr::Literal {
        value: serde_json::Value::from(v),
        raw: None,
        regex: None,
    };
    let assign = |l: Expr, r: Expr| {
        Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(l),
            right: Box::new(r),
        }
    };
    let bin = |op: &str, l: Expr, r: Expr| Expr::BinaryExpression {
        operator: op.to_string(),
        left: Box::new(l),
        right: Box::new(r),
    };
    let char_at = |idx: Expr| {
        Expr::MemberExpression {
            object: Box::new(ident("r")),
            property: Box::new(idx),
            computed: true,
            optional: false,
        }
    };
    let includes = |arg: Expr| {
        crate::estree::method_call(ident("_cs"), "includes", vec![arg])
    };
    // r.length is a PROPERTY read, not a call
    let r_len = || Expr::MemberExpression {
        object: Box::new(ident("r")),
        property: Box::new(ident("length")),
        computed: false,
        optional: false,
    };

    use crate::estree::{ArrowBody, Stmt, VariableDeclarator};
    // let r = String(TEXT); let i = 0; let j = 0;
    let decl_r = |init: Expr| Stmt::VariableDeclaration {
        declarations: vec![VariableDeclarator {
            type_: "VariableDeclarator",
            id: ident("r"),
            init: Some(init),
        }],
        kind: "let",
    };
    // side: "left" = leading only, "right" = trailing only,
    // absent/other = both ends (strings.Trim)
    let trim_leading = node.side.as_deref() != Some("right");
    let trim_trailing = node.side.as_deref() != Some("left");
    let mut stmts: Vec<Stmt> = vec![
        decl_r(Expr::CallExpression {
            callee: Box::new(ident("String")),
            arguments: vec![ident("_t")],
            optional: false,
        }),
        Stmt::VariableDeclaration {
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: ident("i"),
                init: Some(num(0)),
            }],
            kind: "let",
        },
        Stmt::VariableDeclaration {
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: ident("j"),
                init: Some(num(0)),
            }],
            kind: "let",
        },
    ];
    // while (i < r.length && _cs.includes(r[i])) i = i + 1;
    // (skipped when side == "right": leading chars are kept)
    if trim_leading {
        stmts.push(Stmt::WhileStatement {
            test: bin(
                "&&",
                bin("<", ident("i"), r_len()),
                includes(char_at(ident("i"))),
            ),
            body: Box::new(Stmt::BlockStatement {
                body: vec![Stmt::ExpressionStatement {
                    expression: inc("i"),
                }],
            }),
        });
    }
    stmts.push(Stmt::ExpressionStatement {
        expression: assign(ident("j"), r_len()),
    });
    // while (j > i && _cs.includes(r[j - 1])) j = j - 1;
    // (skipped when side == "left": trailing chars are kept)
    if trim_trailing {
        stmts.push(Stmt::WhileStatement {
            test: bin(
                "&&",
                bin(">", ident("j"), ident("i")),
                includes(char_at(bin("-", ident("j"), num(1)))),
            ),
            body: Box::new(Stmt::BlockStatement {
                body: vec![Stmt::ExpressionStatement {
                    expression: dec("j"),
                }],
            }),
        });
    }
    stmts.push(Stmt::ReturnStatement {
        argument: Some(crate::estree::method_call(
            ident("r"),
            "slice",
            vec![ident("i"), ident("j")],
        )),
    });
    let arrow = Expr::ArrowFunctionExpression {
        params: vec![ident("_t"), ident("_cs")],
        body: ArrowBody::Block(Box::new(Stmt::BlockStatement { body: stmts })),
        expression: false,
        r#async: false,
    };
    Some(Expr::CallExpression {
        callee: Box::new(arrow),
        arguments: vec![text, cutset],
        optional: false,
    })
}

// inc/dec as assignments (no UpdateExpression in the ESTree subset)
fn inc(v: &str) -> Expr {
    Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(Expr::Identifier { name: v.to_string() }),
        right: Box::new(Expr::BinaryExpression {
            operator: "+".to_string(),
            left: Box::new(Expr::Identifier { name: v.to_string() }),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
                regex: None,
            }),
        }),
    }
}

fn dec(v: &str) -> Expr {
    Expr::AssignmentExpression {
        operator: "=".to_string(),
        left: Box::new(Expr::Identifier { name: v.to_string() }),
        right: Box::new(Expr::BinaryExpression {
            operator: "-".to_string(),
            left: Box::new(Expr::Identifier { name: v.to_string() }),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(1),
                raw: None,
                regex: None,
            }),
        }),
    }
}
