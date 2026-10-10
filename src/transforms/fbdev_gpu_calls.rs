//! fbdev-gpu-calls — rewrite a lifted GpuOp into the fb_* call it names.
//!
//! ## Need
//!
//! `fbdev-gpu` lifts `echo "$p" > /dev/fb0/blocks` into a `GpuOp`, and every
//! backend renders that node as the original file write (the correctness
//! firewall). On the C backend that fallback is a `fopen`/`fputs`/`fclose`
//! per write against a device path that does not exist on this box —
//! strictly worse than the in-process `fb_blocks(p)` call the same program
//! would have emitted had it named the primitive directly. This transform
//! closes that gap: a `GpuOp` whose verb names a single-payload `fb_*`
//! primitive is rewritten into the `Call` node the parser would have built
//! for the direct call, so the C backend's `fb_call_opt` hook fires and the
//! write becomes one in-process device call — with `need_fb` linkage, void
//! status bypass, and all — byte-identical to naming `fb_blocks` yourself.
//!
//! The mapping (closed list — a new primitive is an explicit edit):
//!
//!     GpuOp{verb: "blocks",        slot: ""}  →  fb_blocks(payload)
//!     GpuOp{verb: "overlay",       slot: ""}  →  fb_overlay(payload)
//!     GpuOp{verb: "overlay_flash", slot: ""}  →  fb_overlay_flash(payload)
//!
//! ## Why ONLY these three (REFUSE > GUESS)
//!
//! - All three take exactly one string payload and are declared `void` in
//!   `runtime/fbdev.h` (`fb_is_void`), so the rewrite preserves statement
//!   status exactly: neither the file-write fallback nor the bare call
//!   touches `_sh_rc`.
//! - `call`/`swap` maps to `fb_present()`, which is NOT void — its return
//!   polarity (`(ret - 1)`) would set `_sh_rc` where the fallback sets
//!   nothing. Refused.
//! - `depthmask`, `uniform`, `texture`, `call/clear`, camera state: no
//!   single-payload primitive names them. Refused — the fallback stays.
//! - A non-empty `slot` names a register the call form cannot address.
//!   Refused.
//! - The payload must be a plain word (`Str`/`Var`/`Ident`/`Int` or an
//!   `Interpolate` of those): the shapes `fb_call_opt` provably renders
//!   in-process. A `Capture`, `Call` (command substitution, `split`),
//!   `Index`, or `Arith` payload MIGHT still lower directly, but where
//!   `fb_call_opt` refuses, the C backend falls back to a
//!   `system("bash -c …")` fork+exec per write — noisier than the silent
//!   file-write fallback. Static text carrying the glob sentinel
//!   (`SH2GLOB`), a brace (`{`), or a leading tilde (`~`) is refused for
//!   the same reason (mirrors `word_needs_glob`/`static_needs_shell`,
//!   done locally so the transform does not depend on c_backend
//!   internals).
//!
//! ## Gating
//!
//! Only the C backend understands `fb_*` calls (no other backend renders
//! them in-process). The transform is gated in `transform_applies` to the
//! explicit `c` target — the `text-ops-planner` precedent — so perl/estree
//! output keeps the file-write fallback. `bash-o4/src/pipeline.rs` sets
//! that target, which is the path MIMEcroft compiles through.
//!
//! ## Placement
//!
//! Registered in `transforms.rs` AFTER `fbdev-gpu-elim`: batching merges
//! consecutive same-verb payloads first, so one `fb_blocks` call carries
//! the whole frame's draws.

use crate::ir::{InterpPart, IrExpr, IrStmt};

/// (device verb, slot, fb_* primitive). Slot must be empty — a uniform's
/// `1f/uCamYaw` names a register the call form cannot address.
fn primitive_for(device: &str, verb: &str, slot: &str) -> Option<&'static str> {
    if device != "/dev/fb0" && device != "/dev/webgl" {
        return None;
    }
    if !slot.is_empty() {
        return None;
    }
    match verb {
        "blocks" => Some("fb_blocks"),
        "overlay" => Some("fb_overlay"),
        "overlay_flash" => Some("fb_overlay_flash"),
        _ => None,
    }
}

/// Static text the C hook provably renders in-process (mirrors
/// `word_needs_glob`'s sentinel and `static_needs_shell`'s brace/tilde).
fn static_text_ok(s: &str) -> bool {
    !s.contains("SH2GLOB") && !s.contains('{') && !s.starts_with('~')
}

/// Is `payload` a plain word the direct call renders without a shell
/// fallback? A faithful port of the C backend's `fb_word_one_value`
/// (one value in a C argument position), tightened in exactly one way:
/// static text additionally passes `static_text_ok` (the glob-sentinel
/// / brace / tilde refusal of `word_needs_glob`+`static_needs_shell`,
/// which the backend checks separately in `fb_call_opt`). Anything this
/// refuses keeps the file-write fallback — a fork+exec per write is
/// worse than a silent device write.
pub(crate) fn payload_direct_ok(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(s, _) => static_text_ok(s),
        IrExpr::Int(..) | IrExpr::Var(..) | IrExpr::Ident(..) => true,
        IrExpr::Arith(..) => true,
        IrExpr::Index { key, .. } => payload_direct_ok(key),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(s) => static_text_ok(s),
            InterpPart::Expr(x) => payload_direct_ok(x),
        }),
        IrExpr::Capture { .. } => false,
        IrExpr::BinOp { lhs, rhs, .. } => payload_direct_ok(lhs) && payload_direct_ok(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            payload_direct_ok(cond) && payload_direct_ok(then) && payload_direct_ok(else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            payload_direct_ok(expr) && payload_direct_ok(default)
        }
        IrExpr::Call { func, args } => match func.as_str() {
            "brace" | "listVar" | "arrayItems" | "pipeline" | "capture" => false,
            _ => args.iter().all(payload_direct_ok),
        },
        _ => false,
    }
}

/// Rewrite one statement: a mapped `GpuOp` becomes the `Call` node the
/// parser builds for the direct call, then recurse into composite
/// bodies. One function does both (the `fbdev-gpu` layering): a split
/// vec-walker + one-level recursor silently misses anything nested
/// three or more levels deep (a draw inside an `if` inside a function
/// — the MIMEcroft shape), leaving an `Ext` behind that poisons the
/// C backend's array-size proofs globally.
fn calls_stmt(st: &mut IrStmt) -> bool {
    let rewrite = match st {
        IrStmt::Ext(n) => n
            .as_any()
            .downcast_ref::<crate::shir_nodes::GpuOp>()
            .filter(|op| {
                primitive_for(&op.device, &op.verb, &op.slot).is_some()
                    && payload_direct_ok(&op.payload)
            })
            .map(|op| {
                let cmd = primitive_for(&op.device, &op.verb, &op.slot).unwrap();
                (cmd.to_string(), op.payload.clone())
            }),
        _ => None,
    };
    if let Some((cmd, payload)) = rewrite {
        // The exact node the parser builds for `fb_blocks "$p"`: the C
        // backend routes it through `fb_call_opt` indistinguishably
        // from a direct call.
        *st = IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str(cmd, crate::ir::StrStyle::DoubleQuoted),
                IrExpr::Array(vec![payload]),
            ],
        });
        return true;
    }
    match st {
        IrStmt::Block(b)
        | IrStmt::Function { body: b, .. }
        | IrStmt::While { body: b, .. }
        | IrStmt::For { body: b, .. }
        | IrStmt::ForInit { body: b, .. }
        | IrStmt::DoWhile { body: b, .. } => calls_vec(b),
        IrStmt::Subshell(b) | IrStmt::Background(b) => calls_vec(b),
        IrStmt::If { then: b, elsifs, else_, .. } => {
            let mut changed = calls_vec(b);
            for (_, eb) in elsifs.iter_mut() {
                changed |= calls_vec(eb);
            }
            changed |= calls_vec(else_);
            changed
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                changed |= calls_vec(stage);
            }
            changed
        }
        _ => false,
    }
}

fn calls_vec(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= calls_stmt(st);
    }
    changed
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    calls_vec(stmts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn s(t: &str) -> IrExpr {
        IrExpr::Str(t.to_string(), StrStyle::DoubleQuoted)
    }

    fn gpu_op(device: &str, verb: &str, slot: &str, payload: IrExpr) -> IrStmt {
        IrStmt::Ext(Box::new(crate::shir_nodes::GpuOp {
            device: device.to_string(),
            verb: verb.to_string(),
            slot: slot.to_string(),
            payload,
            id: "sh_gpu_calls_test".to_string(),
        }))
    }

    /// The rewritten node, as the C backend's `exec_call` sees it.
    fn as_exec_call(st: &IrStmt) -> Option<(&str, &Vec<IrExpr>)> {
        match st {
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
                let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
                    return None;
                };
                Some((cmd.as_str(), words))
            }
            _ => None,
        }
    }

    #[test]
    fn rewrites_blocks_to_fb_blocks() {
        for (verb, prim) in [
            ("blocks", "fb_blocks"),
            ("overlay", "fb_overlay"),
            ("overlay_flash", "fb_overlay_flash"),
        ] {
            for device in ["/dev/fb0", "/dev/webgl"] {
                let mut stmts = vec![gpu_op(device, verb, "", s("payload"))];
                assert!(transform(&mut stmts), "{device}/{verb} should rewrite");
                let (cmd, words) = as_exec_call(&stmts[0]).expect("a Call node");
                assert_eq!(cmd, prim);
                assert_eq!(words.len(), 1, "one payload word");
                assert_eq!(words[0], s("payload"), "payload carried verbatim");
            }
        }
    }

    #[test]
    fn keeps_a_variable_payload() {
        // the MIMEcroft shape: `echo "$blk_p" > /dev/fb0/blocks`.
        let mut stmts = vec![gpu_op(
            "/dev/fb0",
            "blocks",
            "",
            IrExpr::Var("blk_p".into(), None),
        )];
        assert!(transform(&mut stmts));
        let (cmd, words) = as_exec_call(&stmts[0]).unwrap();
        assert_eq!(cmd, "fb_blocks");
        assert!(matches!(&words[0], IrExpr::Var(..)));
    }

    // ── the refusals are the point (REFUSE > GUESS) ──────────────────

    #[test]
    fn refuses_unmapped_verbs() {
        // `call`/`swap` is fb_present (non-void: status divergence);
        // depthmask/uniform/texture name no single-payload primitive.
        for verb in ["call", "depthmask", "uniform", "texture", "hud"] {
            let mut stmts = vec![gpu_op("/dev/fb0", verb, "", s("x"))];
            assert!(!transform(&mut stmts), "{verb} must keep its fallback");
            assert!(as_exec_call(&stmts[0]).is_none());
        }
    }

    #[test]
    fn refuses_a_slot() {
        let mut stmts = vec![gpu_op("/dev/fb0", "blocks", "1f/uCamYaw", s("x"))];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn refuses_an_unknown_device() {
        let mut stmts = vec![gpu_op("/dev/dri", "blocks", "", s("x"))];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn refuses_a_capture_payload() {
        // `echo "$(gen)" > /dev/fb0/blocks`: the direct call might need a
        // shell fallback (a fork per write) — worse than the file write.
        let mut stmts = vec![gpu_op(
            "/dev/fb0",
            "blocks",
            "",
            IrExpr::Capture {
                expr: Box::new(s("gen")),
                native: false,
            },
        )];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn refuses_glob_brace_tilde_statics() {
        for p in ["a{b,c}", "~/x", "SH2GLOB*"] {
            let mut stmts = vec![gpu_op("/dev/fb0", "blocks", "", s(p))];
            assert!(!transform(&mut stmts), "{p} must keep its fallback");
        }
    }

    #[test]
    fn non_gpu_ops_are_untouched() {
        let mut stmts = vec![IrStmt::Expr(s("hello"))];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn rewrites_three_levels_deep() {
        // the MIMEcroft shape: a draw inside an `if` inside a function.
        // A one-level recursor leaves the `GpuOp` behind, and a leftover
        // `Ext` node poisons the C backend's array-size proofs globally.
        let mut stmts = vec![IrStmt::Function {
            name: "render".into(),
            params: vec![],
            body: vec![IrStmt::If {
                cond: IrExpr::Int(1),
                then: vec![gpu_op("/dev/fb0", "blocks", "", s("a"))],
                elsifs: vec![],
                else_: vec![],
            }],
            named_blocks: vec![],
        }];
        assert!(transform(&mut stmts));
        let IrStmt::Function { body, .. } = &stmts[0] else { panic!() };
        let IrStmt::If { then, .. } = &body[0] else { panic!() };
        let (cmd, _) = as_exec_call(&then[0]).expect("rewritten call");
        assert_eq!(cmd, "fb_blocks");
    }

    #[test]
    fn recurses_into_a_loop_body() {
        let mut stmts = vec![IrStmt::While {
            cond: IrExpr::Int(1),
            body: vec![gpu_op("/dev/fb0", "blocks", "", s("a"))],
        }];
        assert!(transform(&mut stmts));
        let IrStmt::While { body, .. } = &stmts[0] else { panic!() };
        let (cmd, _) = as_exec_call(&body[0]).expect("rewritten call");
        assert_eq!(cmd, "fb_blocks");
    }

    #[test]
    fn gated_to_the_c_target() {
        // Only the C backend renders fb_* calls in-process; every other
        // backend must keep the file-write fallback.
        use crate::transforms::{set_target, transform_applies};
        set_target(Some("c"));
        assert!(transform_applies("fbdev-gpu-calls"));
        set_target(Some("perl"));
        assert!(!transform_applies("fbdev-gpu-calls"));
        set_target(None);
        assert!(!transform_applies("fbdev-gpu-calls"));
        set_target(None);
    }

    #[test]
    fn is_idempotent() {
        let mut stmts = vec![gpu_op("/dev/fb0", "blocks", "", s("a"))];
        assert!(transform(&mut stmts));
        assert!(!transform(&mut stmts), "a Call is not a GpuOp");
        assert!(as_exec_call(&stmts[0]).is_some());
    }
}
