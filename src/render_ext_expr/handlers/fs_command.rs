//! Handler for FsCommand — delegates to all_nodes.

use crate::shir_nodes::FsCommand;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &FsCommand, ctx: &ExprRenderCtx) -> Option<String> {
    super::all_nodes::fs_command(node, ctx)
}
