//! AddressOf → snapshot passthrough on the JS/ESTree backend: value-only
//! surface, so the current value passes through (see address_of.node for
//! the pair contract with Deref; Perl renders a true reference).

use crate::shir_nodes::AddressOf;

pub(crate) fn render(node: &AddressOf) -> Option<crate::estree::Expr> {
    Some(crate::shir::expr_to_estree_pub(&node.operand))
}
