//! Handler for RegexpFind — delegates to all_nodes.

use crate::shir_nodes::RegexpFind;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &RegexpFind, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::regexp_find(node, ctx)
}
