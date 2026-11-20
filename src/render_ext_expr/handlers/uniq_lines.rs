//! Handler for UniqLines — delegates to all_nodes.

use crate::shir_nodes::UniqLines;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &UniqLines, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::uniq_lines(node, ctx)
}
