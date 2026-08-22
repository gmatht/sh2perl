//! MapLiteral → ObjectExpression: parallel keys/values lists zip into
//! properties; keys are computed (stringify at runtime, like the host
//! object model and Go's map key coercion).

use crate::estree::{Expr, Property};
use crate::shir_nodes::MapLiteral;

pub(crate) fn render(node: &MapLiteral) -> Option<Expr> {
    if node.keys.len() != node.values.len() {
        return None; // malformed — fall through to the sh2.* fallback
    }
    let properties = node
        .keys
        .iter()
        .zip(node.values.iter())
        .map(|(k, v)| Property {
            type_: "Property",
            key: crate::shir::expr_to_estree_pub(k),
            value: crate::shir::expr_to_estree_pub(v),
            kind: "init",
            computed: true,
            shorthand: false,
        })
        .collect();
    Some(Expr::ObjectExpression { properties })
}
