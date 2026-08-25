//! Handler for AffixStrip — delegates to all_nodes.

use crate::shir_nodes::AffixStrip;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &AffixStrip, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::affix_strip(node, ctx)
}
