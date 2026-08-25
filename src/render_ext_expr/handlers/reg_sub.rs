//! Handler for RegSub — delegates to all_nodes.

use crate::shir_nodes::RegSub;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &RegSub, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::reg_sub(node, ctx)
}
