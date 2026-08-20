//! Handler for Split — delegates to all_nodes.

use crate::shir_nodes::Split;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &Split, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::split(node, ctx)
}
