//! Handler for ArrayLen — delegates to all_nodes.

use crate::shir_nodes::ArrayLen;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &ArrayLen, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::array_len(node, ctx)
}
