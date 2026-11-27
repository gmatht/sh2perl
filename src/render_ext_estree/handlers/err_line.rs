//! ErrLine → `sh2.warn(value)`: the runtime's stderr channel (it writes
//! `String(value ?? "") + "\n"` to process.stderr and leaves stdout alone).
//! Emitted code may only call whitelisted `sh2.<name>` members, so this is
//! the only honest destination for a diagnostic write.

use crate::estree::Expr;
use crate::shir_nodes::ErrLine;

pub(crate) fn render(node: &ErrLine) -> Option<Expr> {
    Some(crate::estree::sh2_call(
        "warn",
        vec![crate::shir::expr_to_estree_pub(&node.value)],
    ))
}
