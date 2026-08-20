//! Handler for RegCount — delegates to all_nodes.

use crate::shir_nodes::RegCount;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &RegCount, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::reg_count(node, ctx)
}
