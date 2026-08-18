//! Handler: `CountedFor` on the perl backend.
//!
//! A rich counted loop (`var = init; while cond { body; var += step }`)
//! renders as a native perl `for`. The children are rendered by the perl
//! renderer's OWN emitters — `ir_expr_to_perl` for the cond, `emit_stmt`
//! for each body statement — i.e. the recursion goes back through the
//! generated/extensible match, not a re-implementation.

use crate::render_ext::RenderCtx;
use crate::shir_nodes::CountedFor;

pub(crate) fn render(ctx: &mut RenderCtx, n: &CountedFor) -> bool {
    use crate::ir::{emit_stmt, ir_expr_to_perl};
    for _ in 0..ctx.indent {
        ctx.out.push_str("    ");
    }
    let cond = ir_expr_to_perl(&n.cond);
    ctx.out.push_str(&format!(
        "for (my ${} = {}; ${} < {}; ${} += {}) {{\n",
        n.var, n.init, n.var, cond, n.var, n.step
    ));
    for b in &n.body {
        emit_stmt(ctx.out, b, ctx.indent + 1);
    }
    for _ in 0..ctx.indent {
        ctx.out.push_str("    ");
    }
    ctx.out.push_str("}\n");
    true
}
