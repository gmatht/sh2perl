//! Handler for StringAffix — delegates to all_nodes.

use crate::shir_nodes::StringAffix;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &StringAffix, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::string_affix(node, ctx)
}
