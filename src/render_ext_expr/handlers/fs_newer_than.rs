//! Handler for FsNewerThan — delegates to all_nodes.

use crate::shir_nodes::FsNewerThan;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &FsNewerThan, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::fs_newer_than(node, ctx)
}
