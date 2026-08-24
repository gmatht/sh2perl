//! Handler for CutsetTrim — delegates to all_nodes.

use crate::shir_nodes::CutsetTrim;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &CutsetTrim, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::cutset_trim(node, ctx)
}
