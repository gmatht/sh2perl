//! Handler: `ForEachLine` on the perl backend.
//!
//! STREAMING line iteration — `while (my $l = <$fh>) { … }` over a lexical
//! handle. O(1) memory: the file is never slurped. Body statements are
//! rendered by the perl renderer's own emitters.

use crate::render_ext::RenderCtx;
use crate::shir_nodes::ForEachLine;

pub(crate) fn render(ctx: &mut RenderCtx, n: &ForEachLine) -> bool {
    use crate::ir::{emit_stmt, ir_expr_to_perl};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static FH: AtomicUsize = AtomicUsize::new(0);
    let k = FH.fetch_add(1, Ordering::Relaxed);
    let indent = ctx.indent;
    for _ in 0..indent { ctx.out.push_str("    "); }
    let path = ir_expr_to_perl(&n.source);
    // A dedicated lexical handle per loop keeps nesting safe.
    ctx.out.push_str(&format!(
        "open my $_fl_fh{k}, '<', {}; unless (defined $_fl_fh{k}) {{ $! = 1; croak \"open failed\\n\"; }}\n",
        path
    ));
    for _ in 0..indent { ctx.out.push_str("    "); }
    match &n.limit {
        None => ctx.out.push_str(&format!("while (my ${} = <$_fl_fh{k}>) {{\n", n.var)),
        Some(lim) => {
            // streaming head: counter + last after K lines (O(K) memory)
            let lim_p = ir_expr_to_perl(lim);
            ctx.out.push_str(&format!("my $__fl_n{k} = 0;\n"));
            for _ in 0..indent { ctx.out.push_str("    "); }
            let hdr = format!(
                "while ($__fl_n{k} < ({lim_p}) && defined(my ${} = <$_fl_fh{k}>)) {{\n",
                n.var
            );
            ctx.out.push_str(&hdr);
        }
    }
    for _ in 0..indent + 1 { ctx.out.push_str("    "); }
    // bash/cut/grep see lines WITHOUT the trailing newline — chomp it.
    ctx.out.push_str(&format!("chomp ${};\n", n.var));
    if n.limit.is_some() {
        for _ in 0..indent + 1 { ctx.out.push_str("    "); }
        ctx.out.push_str(&format!("$__fl_n{k}++;\n"));
    }
    for b in &n.body {
        emit_stmt(ctx.out, b, indent + 1);
    }
    for _ in 0..indent { ctx.out.push_str("    "); }
    ctx.out.push_str("}\n");
    true
}
