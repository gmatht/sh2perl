//! Handler for StringTrim — delegates to all_nodes.

use crate::shir_nodes::StringTrim;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &StringTrim, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::string_trim(node, ctx)
}
