//! fbdev-gpu — lift a write to the fb0 device into a GPU operation.
//!
//! ## Need
//!
//! `docs/BASH-O4.md`'s end state is `bash → ShIR → C → tcc → native binary`
//! with "hot pure-compute loops → Vulkan compute shaders (SPIR-V)" hanging
//! off it. Stage ② of §4.2 names **"gpu-prep passes (§4.3)"** as the
//! transform step, and §4.4 ("Codegen: host split + device shader") is the
//! `_sh_gpu_*` emit. Neither exists in the tree: the GPU leg is specified
//! and unimplemented (the status note records that the dispatch "remains").
//!
//! What DOES exist is the device-write shape this pass consumes. A shell
//! program draws by writing **structured text to a device file** — the
//! Plan 9 model that MIMEcroft's fb0 port is built on:
//!
//!     echo "$blk_p" > /dev/fb0/blocks     # or /dev/webgl/blocks (the
//!     echo "0"      > /dev/fb0/depthmask  #  pre-port spelling)
//!     echo "swap"   > /dev/fb0/call
//!
//! Today those are `IrStmt::WriteFile` (a static device path) or
//! `IrStmt::Redirect { fd: 1, target: "/dev/fb0..." }`, and every backend
//! renders them as a real file write at run time. This pass recognises them
//! and lifts the *write itself* into a `GpuOp` node, so a backend with a
//! GPU leg can emit a dispatch instead of a filesystem write.
//!
//! ## Why this is NOT what candidacy's `body-redirect` veto forbids
//!
//! `bash-o4/src/candidacy.rs` vetoes a loop whose **body performs** device
//! I/O (`body-redirect`, line ~599): such a body cannot be offloaded as-is
//! because the write is a host effect *inside* the iteration space. That is
//! a statement about loop bodies. This pass is a different transformation at
//! a different layer: it lifts the **write operation** out of the shell
//! program and into the device boundary. The two are orthogonal, and
//! conflating them is how the feature was believed not to exist.
//!
//! ## The shape of a device write
//!
//! A device write is `verb + payload`, where the verb is the LAST path
//! segment and the payload is the written value:
//!
//!     /dev/fb0/blocks          blocks   ← a batched draw payload
//!     /dev/fb0/depthmask       depthmask← "0"/"1"
//!     /dev/fb0/call            call     ← "clear" | "swap" | "draw …"
//!     /dev/fb0/uniform/1f/x    uniform  ← a named scalar, sub-segment = name
//!
//! The node records the verb, the payload expression and the device path, so
//! a renderer that has no GPU leg can still fall back to the original write
//! (the correctness firewall, §4.4's "CPU loop (always emitted, always
//! correct)").
//!
//! ## Guards (REFUSE > GUESS)
//!
//! - Only a **static** device path is lifted. A path with an interpolation
//!   (`"$DEV/blocks"`) is not a device this pass can name, so it is left
//!   alone — the same refuse-over-guess rule the fb0 device lowering follows
//!   in `c_backend.rs` (`fb_word_one_value`).
//! - Only the **fb0 / webgl draw devices**, never a generic path that merely
//!   starts with `/dev/` (`/dev/null`, `/dev/fd/N`). The device set is a
//!   closed list, so a new one is an explicit edit rather than an accident.
//! - The payload must be present. `echo -n > /dev/fb0` writes nothing and is
//!   left as-is rather than lifted into an op with no argument.
//!
//! ## Placement
//!
//! Registered in `transforms.rs`, SH2_TRANSFORMS-gated, applying to every
//! backend (a backend without a GPU leg simply renders the node through its
//! fallback). It is the §4.2 stage ② "gpu-prep" pass.

use crate::ir::{IrExpr, IrStmt};
use crate::shir_nodes::GpuOp;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// The device roots this pass will lift a write from. A closed list: adding
/// a device is a deliberate edit here, not a side effect of a prefix test.
const DEVICE_ROOTS: [&str; 2] = ["/dev/fb0", "/dev/webgl"];

/// Is `path` a static path under a known draw device? Returns the device
/// root and the remaining segments (e.g. `["uniform", "1f", "uCamYaw"]`).
fn device_path(path: &IrExpr) -> Option<(&'static str, Vec<String>)> {
    let IrExpr::Str(s, _style) = path else { return None };
    // A path carrying a shell expansion is not a device this pass can name.
    // (`static_needs_shell`-style refusal, done locally so the transform does
    // not depend on c_backend internals.)
    if s.contains('$') || s.contains('`') || s.contains('*') || s.contains('?') {
        return None;
    }
    for root in DEVICE_ROOTS {
        if let Some(rest) = s.strip_prefix(root) {
            // must be the device itself or a path UNDER it, not a longer
            // name that merely shares the prefix (e.g. "/dev/fb01").
            if !rest.is_empty() && !rest.starts_with('/') {
                continue;
            }
            let segs: Vec<String> = rest
                .split('/')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect();
            return Some((root, segs));
        }
    }
    None
}

/// Build the lifted node, or None when the shape is not liftable.
fn build_op(root: &'static str, segs: &[String], payload: IrExpr) -> Option<IrStmt> {
    // The verb is the last segment ("blocks", "call", "depthmask"); the
    // sub-segments (a uniform's type/name) ride along for a renderer that
    // needs to name a slot. No verb at all (a write to the device root) is
    // not a drawing operation this pass can express.
    let verb = segs.last()?.clone();
    // A payload is required: `echo -n > /dev/fb0/x` is not a draw.
    if matches!(&payload, IrExpr::Str(t, _) if t.is_empty()) {
        return None;
    }
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    Some(IrStmt::Ext(Box::new(GpuOp {
        device: root.to_string(),
        verb,
        slot: segs[..segs.len().saturating_sub(1)].join("/"),
        payload,
        id: format!("sh_gpu_dev_{seq}"),
    })))
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= lift_stmt(st);
    }
    changed
}

fn lift_stmt(st: &mut IrStmt) -> bool {
    // The two spellings of a device write.
    match st {
        // `echo "$payload" > /dev/fb0/blocks` — a static device path, fd 1,
        // write mode, with the payload as the redirect's inner statement.
        IrStmt::Redirect { inner, redirects } => {
            // Recurse first so a nested write is still lifted.
            let changed = lift_stmt_vec(inner);
            if redirects.len() == 1 {
                let r = &redirects[0];
                let is_dev_write = r.mode == "w"
                    && r.fd.map(|fd| fd == 1).unwrap_or(true);
                if is_dev_write {
                    if let Some((root, segs)) = device_path(&r.target) {
                        if let Some(payload) = payload_of(inner) {
                            if let Some(op) = build_op(root, &segs, payload) {
                                *st = op;
                                return true;
                            }
                        }
                    }
                }
            }
            changed
        }
        // `WriteFile { path: "/dev/fb0/x", content }` — the non-redirect form.
        IrStmt::WriteFile { path, content, append } => {
            if *append {
                return false; // an append is a stream, not a single op
            }
            if let Some((root, segs)) = device_path(path) {
                if let Some(op) = build_op(root, &segs, content.clone()) {
                    *st = op;
                    return true;
                }
            }
            false
        }
        // Recurse into every composite body, so a device write nested in an
        // `if`/loop/function is lifted too (candidacy skips a body that
        // performs I/O, but this pass is about the write, not the loop).
        IrStmt::Block(b)
        | IrStmt::Function { body: b, .. }
        | IrStmt::While { body: b, .. }
        | IrStmt::For { body: b, .. }
        | IrStmt::ForInit { body: b, .. }
        | IrStmt::DoWhile { body: b, .. } => lift_stmt_vec(b),
        IrStmt::Subshell(b) | IrStmt::Background(b) => lift_stmt_vec(b),
        IrStmt::If { then: b, elsifs, else_, .. } => {
            let mut changed = lift_stmt_vec(b);
            for (_, eb) in elsifs.iter_mut() {
                changed |= lift_stmt_vec(eb);
            }
            changed |= lift_stmt_vec(else_);
            changed
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                changed |= lift_stmt_vec(stage);
            }
            changed
        }
        _ => false,
    }
}

fn lift_stmt_vec(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= lift_stmt(st);
    }
    changed
}

/// The payload of a redirect's inner statement: a single-argument `echo`,
/// or a `printf` with one format argument. Anything else (a pipeline, a
/// multi-word echo, a capture) is not a device payload this pass can name.
fn payload_of(inner: &[IrStmt]) -> Option<IrExpr> {
    let [IrStmt::Expr(IrExpr::Call { func, args })] = inner else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(items)] = args.as_slice() else {
        return None;
    };
    if cmd != "echo" && cmd != "printf" {
        return None;
    }
    // `echo "$x"` lowers to a one-element array. A multi-word echo
    // ("$a $b") is two elements and is refused: joining them is a guess
    // about spacing that the device would have to make.
    if items.len() != 1 {
        return None;
    }
    Some(items[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn s(t: &str) -> IrExpr {
        IrExpr::Str(t.to_string(), StrStyle::DoubleQuoted)
    }

    /// `echo "<payload>" > <path>` as the IR spells it.
    fn echo_redirect(payload: IrExpr, path: &str) -> IrStmt {
        IrStmt::Redirect {
            inner: vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".into(),
                args: vec![
                    s("echo"),
                    IrExpr::Array(vec![payload]),
                ],
            })],
            redirects: vec![crate::ir::IrRedirect {
                fd: Some(1),
                mode: "w".into(),
                target: s(path),
                interpolate: false,
            }],
        }
    }

    fn op_of(st: &IrStmt) -> Option<&crate::shir_nodes::GpuOp> {
        match st {
            IrStmt::Ext(n) => n.as_any().downcast_ref::<crate::shir_nodes::GpuOp>(),
            _ => None,
        }
    }

    #[test]
    fn lifts_a_device_write_to_a_gpu_op() {
        let mut stmts = vec![echo_redirect(s("$blk_p"), "/dev/fb0/blocks")];
        assert!(transform(&mut stmts), "the write should have been lifted");
        let op = op_of(&stmts[0]).expect("statement should be a GpuOp");
        assert_eq!(op.device, "/dev/fb0");
        assert_eq!(op.verb, "blocks");
        assert_eq!(op.slot, "", "a top-level verb has no slot");
    }

    #[test]
    fn keeps_a_uniform_slot() {
        let mut stmts = vec![echo_redirect(s("1"), "/dev/fb0/uniform/1f/uCamYaw")];
        assert!(transform(&mut stmts));
        let op = op_of(&stmts[0]).unwrap();
        assert_eq!(op.verb, "uCamYaw");
        assert_eq!(op.slot, "uniform/1f");
    }

    #[test]
    fn lifts_the_webgl_spelling_too() {
        let mut stmts = vec![echo_redirect(s("swap"), "/dev/webgl/call")];
        assert!(transform(&mut stmts));
        let op = op_of(&stmts[0]).unwrap();
        assert_eq!(op.device, "/dev/webgl");
        assert_eq!(op.verb, "call");
    }

    // ── the refusals are the point (REFUSE > GUESS) ──────────────────

    #[test]
    fn refuses_a_non_device_path() {
        // /dev/null is not a draw device: the closed list must not widen
        // into a prefix test.
        for p in ["/dev/null", "/dev/fd/3", "/tmp/out.txt", "/dev/fb01/x"] {
            let mut stmts = vec![echo_redirect(s("x"), p)];
            assert!(!transform(&mut stmts), "should refuse {p}");
            assert!(op_of(&stmts[0]).is_none(), "{p} must not become a GpuOp");
        }
    }

    #[test]
    fn refuses_an_interpolated_path() {
        // `echo x > "$DEV/blocks"` — the path is not a device this pass can
        // name, even though its text starts with the right prefix.
        let mut stmts = vec![echo_redirect(s("x"), "$DEV/blocks")];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn refuses_an_empty_payload() {
        // `echo -n > /dev/fb0/x` is not a draw.
        let mut stmts = vec![echo_redirect(s(""), "/dev/fb0/blocks")];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn refuses_a_multiword_echo() {
        // joining "$a $b" is a spacing guess the device would have to make
        let mut stmts = vec![echo_redirect(
            IrExpr::Interpolate(vec![
                crate::ir::InterpPart::Lit("a".into()),
                crate::ir::InterpPart::Lit(" b".into()),
            ]),
            "/dev/fb0/blocks",
        )];
        // a single Interpolate IS one element, so this one is liftable;
        // the refusal is for a genuinely multi-element array:
        assert!(transform(&mut stmts));
        let mut multi = vec![IrStmt::Redirect {
            inner: vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".into(),
                args: vec![
                    s("echo"),
                    IrExpr::Array(vec![s("a"), s("b")]),
                ],
            })],
            redirects: vec![crate::ir::IrRedirect {
                fd: Some(1),
                mode: "w".into(),
                target: s("/dev/fb0/blocks"),
                interpolate: false,
            }],
        }];
        assert!(!transform(&mut multi), "a two-word echo must be refused");
    }

    #[test]
    fn recurses_into_a_loop_body() {
        // The write is lifted even though it sits in a loop body —
        // candidacy's body-redirect veto is about offloading the LOOP,
        // which is a different transformation (see the module docs).
        let mut stmts = vec![IrStmt::While {
            cond: IrExpr::Int(1),
            body: vec![echo_redirect(s("$blk_p"), "/dev/fb0/blocks")],
        }];
        assert!(transform(&mut stmts));
        let IrStmt::While { body, .. } = &stmts[0] else { panic!() };
        assert!(op_of(&body[0]).is_some(), "the inner write should be a GpuOp");
    }

    #[test]
    fn is_idempotent() {
        let mut stmts = vec![echo_redirect(s("$blk_p"), "/dev/fb0/blocks")];
        assert!(transform(&mut stmts));
        assert!(!transform(&mut stmts), "a second run must change nothing");
    }
}

#[cfg(test)]
mod payload_tests {
    //! Payload fidelity: the lifted node must carry the SAME value the shell
    //! wrote, so a fallback render is byte-identical to the original write.
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn s(t: &str) -> IrExpr {
        IrExpr::Str(t.to_string(), StrStyle::DoubleQuoted)
    }

    #[test]
    fn payload_is_carried_verbatim() {
        for p in ["swap", "0", "1", "x y z 1 1 1 1 0 0 5 0", ""] {
            if p.is_empty() {
                continue; // empty is refused, covered elsewhere
            }
            let mut stmts = vec![IrStmt::Redirect {
                inner: vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".into(),
                    args: vec![s("echo"), IrExpr::Array(vec![s(p)])],
                })],
                redirects: vec![crate::ir::IrRedirect {
                    fd: Some(1),
                    mode: "w".into(),
                    target: s("/dev/fb0/blocks"),
                    interpolate: false,
                }],
            }];
            assert!(transform(&mut stmts));
            let IrStmt::Ext(n) = &stmts[0] else { panic!() };
            let op = n.as_any().downcast_ref::<crate::shir_nodes::GpuOp>().unwrap();
            match &op.payload {
                IrExpr::Str(t, _) => assert_eq!(t, p, "payload must survive verbatim"),
                other => panic!("expected a Str payload, got {other:?}"),
            }
        }
    }

    #[test]
    fn ids_are_unique_per_op() {
        let mut stmts = vec![
            IrStmt::Redirect {
                inner: vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".into(),
                    args: vec![s("echo"), IrExpr::Array(vec![s("a")])],
                })],
                redirects: vec![crate::ir::IrRedirect {
                    fd: Some(1), mode: "w".into(), target: s("/dev/fb0/blocks"), interpolate: false,
                }],
            },
            IrStmt::Redirect {
                inner: vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".into(),
                    args: vec![s("echo"), IrExpr::Array(vec![s("b")])],
                })],
                redirects: vec![crate::ir::IrRedirect {
                    fd: Some(1), mode: "w".into(), target: s("/dev/fb0/call"), interpolate: false,
                }],
            },
        ];
        assert!(transform(&mut stmts));
        let id = |st: &IrStmt| {
            let IrStmt::Ext(n) = st else { panic!() };
            n.as_any().downcast_ref::<crate::shir_nodes::GpuOp>().unwrap().id.clone()
        };
        assert_ne!(id(&stmts[0]), id(&stmts[1]), "each op needs its own dispatch id");
    }
}
