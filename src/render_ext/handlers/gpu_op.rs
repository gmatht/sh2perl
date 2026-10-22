//! Handler: `GpuOp` on the perl backend.
//!
//! A device write lifted from the shell (`transforms/fbdev_gpu.rs`, the
//! §4.2 stage ② "gpu-prep" pass). This backend has no GPU leg, so the
//! CORRECTNESS FIREWALL applies: render the ORIGINAL write.
//!
//! `echo P > /dev/fb0/verb` opens the device path and prints the payload.
//! `open my $fh, '>', PATH` is the same syscall bash makes, so a run
//! against a real `/dev/fb0` (or an absent one — this box has none)
//! behaves identically to the untransformed shell, including the non-fatal
//! "cannot create" warning and the non-zero status. That equivalence is the
//! point: with the lift enabled, an example's output must not change.
//!
//! A backend WITH a dispatch would branch to `_sh_gpu_*` here on
//! (`n.verb`, `n.slot`) and keep this write as the fall-through.

use crate::render_ext::RenderCtx;
use crate::shir_nodes::GpuOp;

pub(crate) fn render(ctx: &mut RenderCtx, n: &GpuOp) -> bool {
    use crate::ir::ir_expr_to_perl;
    let indent = ctx.indent;
    for _ in 0..indent { ctx.out.push_str("    "); }
    let path = if n.slot.is_empty() {
        format!("{}/{}", n.device, n.verb)
    } else {
        format!("{}/{}/{}", n.device, n.slot, n.verb)
    };
    let payload = ir_expr_to_perl(&n.payload);
    // The path is a static device path (the transform refuses anything with
    // a shell expansion), but it came from user source, so quote it rather
    // than splicing it into a Perl literal.
    let p = path.replace('\\', "\\\\").replace('"', "\\\"");
    // `open my $fh, '>', P` leaves $fh DEFINED-but-closed when the open
    // fails, so a bare `if ($fh)` then prints to a closed handle ("print()
    // on closed filehandle"). Clear it on failure so the guard means what
    // it says.
    //
    // The status matches bash: a failed redirect is bash's 2 ("cannot
    // create"), a successful one is 0. $CHILD_ERROR is the Perl renderer's
    // status byte, so the gate sees the same code the shell does.
    ctx.out.push_str(&format!(
        "{{ my $__gfh; open $__gfh, '>', \"{p}\"; \
         if ($__gfh) {{ print $__gfh {payload}; close $__gfh; }} \
         else {{ $CHILD_ERROR = 2; }} }}\n"
    ));
    true
}
