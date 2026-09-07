//! Handler for RepeatStr — delegates to all_nodes.

use crate::shir_nodes::RepeatStr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &RepeatStr, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::repeat_str(node, ctx)
}
