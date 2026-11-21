//! Handler for TextFilter — delegates to all_nodes.

use crate::shir_nodes::TextFilter;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &TextFilter, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::text_filter(node, ctx)
}
