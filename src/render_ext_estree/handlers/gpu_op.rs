//! Handler: `GpuOp` on the ESTree backend.
//!
//! A device write lifted from the shell (`transforms/fbdev_gpu.rs`, the
//! §4.2 stage ② "gpu-prep" pass). This backend has no GPU leg, so the
//! CORRECTNESS FIREWALL applies: render the ORIGINAL write through the
//! runtime's own filesystem, which is byte-equivalent to the shell's
//! `echo P > /dev/fb0/verb`.
//!
//! A backend WITH a dispatch would emit `sh2.gpuDispatch(verb, slot,
//! payload)` here and keep this write as the fall-through (docs/BASH-O4.md
//! §4.4: the CPU path is always emitted, always correct).

use crate::estree::Expr;
use crate::shir_nodes::GpuOp;

pub(crate) fn render(node: &GpuOp) -> Option<Expr> {
    let path = if node.slot.is_empty() {
        format!("{}/{}", node.device, node.verb)
    } else {
        format!("{}/{}/{}", node.device, node.slot, node.verb)
    };
    Some(crate::estree::sh2_call(
        "writeFile",
        vec![
            crate::estree::str_lit(&path),
            crate::shir::expr_to_estree_pub(&node.payload),
        ],
    ))
}
