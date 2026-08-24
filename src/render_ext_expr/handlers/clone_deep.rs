//! Handler for CloneDeep — Perl deep copy via Storable::dclone (takes a
//! REFERENCE and returns an independent copy); see clone_deep.node.

use crate::ir::ir_expr_to_perl;
use crate::render_ext_expr::ExprRenderCtx;
use crate::shir_nodes::{CloneDeep, ExtExpr};

pub(crate) fn render(node: &CloneDeep, _ctx: &ExprRenderCtx) -> Option<String> {
    let value = ir_expr_to_perl(&node.value);
    Some(format!(
        "(require Storable; Storable::dclone(\\{}))",
        value
    ))
}
