//! FieldRead → non-computed MemberExpression: obj.name with the object
//! evaluated natively (see field_read.node).

use crate::estree::Expr;
use crate::shir_nodes::FieldRead;

pub(crate) fn render(node: &FieldRead) -> Option<Expr> {
    Some(Expr::MemberExpression {
        object: Box::new(crate::shir::expr_to_estree_pub(&node.object)),
        property: Box::new(Expr::Identifier { name: node.name.clone() }),
        computed: false,
        optional: false,
    })
}
