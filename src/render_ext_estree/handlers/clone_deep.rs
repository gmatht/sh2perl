//! CloneDeep → structuredClone(value): an independent deep copy of any
//! store value (see clone_deep.node). Node >=17 provides the global.

use crate::estree::Expr;
use crate::shir_nodes::CloneDeep;

pub(crate) fn render(node: &CloneDeep) -> Option<Expr> {
    Some(Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "structuredClone".to_string(),
        }),
        arguments: vec![crate::shir::expr_to_estree_pub(&node.value)],
        optional: false,
    })
}
