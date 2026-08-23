//! Handler for PathName — delegates to all_nodes.

use crate::shir_nodes::PathName;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &PathName, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::path_name(node, ctx)
}
