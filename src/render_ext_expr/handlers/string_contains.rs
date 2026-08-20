//! Handler for StringContains — delegates to all_nodes.

use crate::shir_nodes::StringContains;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &StringContains, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::string_contains(node, ctx)
}
