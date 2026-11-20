//! Handler for SortLines — delegates to all_nodes.

use crate::shir_nodes::SortLines;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &SortLines, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::sort_lines(node, ctx)
}
