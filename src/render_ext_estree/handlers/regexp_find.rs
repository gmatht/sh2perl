//! RegexpFind → native JS: first match of PAT in TEXT, "" when absent
//! (see regexp_find.node).

use crate::estree::Expr;
use crate::shir_nodes::RegexpFind;

pub(crate) fn render(node: &RegexpFind) -> Option<Expr> {
    let text = crate::shir::expr_to_estree_pub(&node.text);
    let pat = crate::shir::expr_to_estree_pub(&node.pattern);
    // (TEXT.match(new RegExp(PAT)) || [""])[0]
    let matched = Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(text.clone()),
            property: Box::new(ident("match")),
            computed: false,
            optional: false,
        }),
        arguments: vec![Expr::NewExpression {
            callee: Box::new(ident("RegExp")),
            arguments: vec![pat],
        }],
        optional: false,
    };
    let fallback = Expr::ArrayExpression {
        elements: vec![Some(str_lit(""))],
    };
    let picked = Expr::MemberExpression {
        object: Box::new(Expr::LogicalExpression {
            operator: "||".to_string(),
            left: Box::new(matched),
            right: Box::new(fallback),
        }),
        property: Box::new(num(0)),
        computed: true,
        optional: false,
    };
    Some(picked)
}

fn ident(n: &str) -> Expr {
    Expr::Identifier { name: n.to_string() }
}
fn str_lit(s: &str) -> Expr {
    Expr::Literal {
        value: serde_json::Value::from(s),
        raw: None,
        regex: None,
    }
}
fn num(v: i64) -> Expr {
    Expr::Literal {
        value: serde_json::Value::from(v),
        raw: None,
        regex: None,
    }
}
