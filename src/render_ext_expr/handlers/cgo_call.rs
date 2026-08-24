//! Handler for CgoCall — delegates to all_nodes.

use crate::shir_nodes::CgoCall;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &CgoCall, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::cgo_call(node, ctx)
}
