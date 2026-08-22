//! TypeAssert → checked-passthrough on JS/ESTree: the dynamic value
//! flows through unchanged (see type_assert.node; the comma-ok boolean
//! is the frontend's `typeof(x) == kind` comparison, not this node).

use crate::shir_nodes::TypeAssert;

pub(crate) fn render(node: &TypeAssert) -> Option<crate::estree::Expr> {
    Some(crate::shir::expr_to_estree_pub(&node.expr))
}
