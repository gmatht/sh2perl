//! Handler for NewlineStrip — delegates to all_nodes.

use crate::render_ext_expr::ExprRenderCtx;
use crate::shir_nodes::NewlineStrip;

pub(crate) fn render(node: &NewlineStrip, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::newline_strip(node, ctx)
}
