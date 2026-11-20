//! Handler for FsModifiedSinceRead — delegates to all_nodes.

use crate::shir_nodes::FsModifiedSinceRead;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &FsModifiedSinceRead, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::fs_modified_since_read(node, ctx)
}
