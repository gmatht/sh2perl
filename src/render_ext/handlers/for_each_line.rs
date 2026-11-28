//! Handler: `ForEachLine` on the perl backend.
//!
//! STREAMING line iteration — `while (my $l = <$fh>) { … }` over a lexical
//! handle. O(1) memory: the file is never slurped. Body statements are
//! rendered by the perl renderer's own emitters.

use crate::render_ext::RenderCtx;
use crate::shir_nodes::ForEachLine;

thread_local! {
    /// Handle-name counter for ONE render. A process-wide `static` made the
    /// emitted handle number depend on how many other scripts this process had
    /// already rendered, so the same source produced `$_fl_fh4` or `$_fl_fh12`
    /// depending on test scheduling — golden fixtures can only be stable if a
    /// render is a pure function of its input (`perl_render_purity`). Reset by
    /// every render entry point (`reset_render_counters`).
    static FH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Start a fresh render: every generated-name counter goes back to zero, so
/// two renders of the same program are byte-identical no matter what this
/// thread (or this process) rendered before.
pub(crate) fn reset_render_counters() {
    FH.with(|c| c.set(0));
}

pub(crate) fn render(ctx: &mut RenderCtx, n: &ForEachLine) -> bool {
    use crate::ir::{emit_stmt, ir_expr_to_perl};
    let k = FH.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
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
