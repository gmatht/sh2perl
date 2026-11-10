//! Handler for CutAt — delegates to all_nodes.

use crate::shir_nodes::CutAt;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &CutAt, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::cut_at(node, ctx)
}
