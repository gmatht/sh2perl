//! Handler for CaseTransform — delegates to all_nodes.

use crate::shir_nodes::CaseTransform;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &CaseTransform, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::case_transform(node, ctx)
}
