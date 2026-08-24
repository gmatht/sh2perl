//! FieldRead → non-computed MemberExpression: obj.name with the object
//! evaluated natively (see field_read.node).

use crate::estree::Expr;
use crate::shir_nodes::FieldRead;

pub(crate) fn render(node: &FieldRead) -> Option<Expr> {
    // numeric names are TUPLE INDICES -> computed member access;
    // anything else is a plain property read
    let computed = node.name.parse::<u64>().is_ok();
    let property = if computed {
        Box::new(Expr::Literal {
            value: serde_json::Value::from(node.name.parse::<u64>().ok()?),
            raw: None,
            regex: None,
        })
    } else {
        Box::new(Expr::Identifier { name: node.name.clone() })
    };
    Some(Expr::MemberExpression {
        object: Box::new(crate::shir::expr_to_estree_pub(&node.object)),
        property,
        computed,
        optional: false,
    })
}
