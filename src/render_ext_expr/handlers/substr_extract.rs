//! Handler for SubStrExtract — delegates to all_nodes.

use crate::shir_nodes::SubStrExtract;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &SubStrExtract, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::substr_extract(node, ctx)
}
