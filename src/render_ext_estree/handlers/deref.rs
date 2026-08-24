//! Deref → snapshot passthrough on the JS/ESTree backend (matching read
//! side of AddressOf; see deref.node).

use crate::shir_nodes::Deref;

pub(crate) fn render(node: &Deref) -> Option<crate::estree::Expr> {
    Some(crate::shir::expr_to_estree_pub(&node.pointer))
}
