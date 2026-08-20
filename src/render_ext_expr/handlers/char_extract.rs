//! Handler for CharExtract — renders `cut -cN` / `cut -bN` natively per backend.

use crate::shir_nodes::CharExtract;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::{ExprRenderCtx, Backend};

pub(crate) fn render(node: &CharExtract, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            Some(format!("substr({}, 0, 1)", text))
        }
        Backend::Go => Some(format!("string([]rune(text)[0:1]) /* TODO */")),
        Backend::Rust => Some(format!("text.chars().next().unwrap_or_default().to_string() /* TODO */")),
        Backend::C => None,
        Backend::Zig => None,
        Backend::Estree => Some(format!("text[0] /* TODO */")),
        _ => None,
    }
}
