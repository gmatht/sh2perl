//! ElementRead → computed MemberExpression: coll[key] with both sides
//! evaluated natively (see element_read.node).

use crate::estree::Expr;
use crate::shir_nodes::ElementRead;

pub(crate) fn render(node: &ElementRead) -> Option<Expr> {
    Some(Expr::MemberExpression {
        object: Box::new(crate::shir::expr_to_estree_pub(&node.coll)),
        property: Box::new(crate::shir::expr_to_estree_pub(&node.key)),
        computed: true,
        optional: false,
    })
}
