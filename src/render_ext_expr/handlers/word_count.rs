//! Handler for WordCount — delegates to all_nodes.

use crate::shir_nodes::WordCount;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &WordCount, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::word_count(node, ctx)
}
