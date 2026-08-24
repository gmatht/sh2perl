//! Spread → ESTree SpreadElement (`...expr`). Valid only in call args /
//! collection literals; the estree generator enforces the same surface.

use crate::estree::Expr;
use crate::shir_nodes::Spread;

pub(crate) fn render(node: &Spread) -> Option<Expr> {
    Some(Expr::SpreadElement {
        argument: Box::new(crate::shir::expr_to_estree_pub(&node.expr)),
    })
}
