//! Handler for FsSameFile — delegates to all_nodes.

use crate::shir_nodes::FsSameFile;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &FsSameFile, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::fs_same_file(node, ctx)
}
