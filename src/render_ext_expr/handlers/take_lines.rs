//! Handler for TakeLines — delegates to all_nodes.

use crate::shir_nodes::TakeLines;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &TakeLines, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::take_lines(node, ctx)
}
