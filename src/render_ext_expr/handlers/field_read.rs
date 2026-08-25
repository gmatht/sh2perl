//! Handler for FieldRead — Perl hashref field access ($obj->{name});
//! see field_read.node.

use crate::ir::ir_expr_to_perl;
use crate::render_ext_expr::ExprRenderCtx;
use crate::shir_nodes::{ExtExpr, FieldRead};

pub(crate) fn render(node: &FieldRead, _ctx: &ExprRenderCtx) -> Option<String> {
    let object = ir_expr_to_perl(&node.object);
    if node.name.parse::<u64>().is_ok() {
        return Some(format!("{}->[{}]", object, node.name));
    }
    Some(format!("{}->{{{}}}", object, node.name))
}
