//! fbdev-gpu-batch — batch consecutive same-verb GpuOps into one dispatch.
//!
//! ## Need
//!
//! The `fbdev-gpu` transform lifts writes to `/dev/fb0` into `GpuOp` nodes.
//! A shell program that draws a frame typically writes several payloads to
//! the same verb in sequence:
//!
//!     echo "$bg_p"  > /dev/fb0/blocks
//!     echo "$blk_p" > /dev/fb0/blocks
//!     echo "$hud_p"  > /dev/fb0/blocks
//!
//! Each becomes a separate `GpuOp` with the same `device` and `verb`. A GPU
//! leg that dispatches each individually pays three kernel round-trips for
//! what is logically one batched draw call. This transform merges consecutive
//! `GpuOp`s that share `device` + `verb` into a single `GpuOp` whose payload
//! is a concatenation of the original payloads, separated by newlines (the
//! device's batch separator).
//!
//! ## Why this is safe
//!
//! - Only **consecutive** `GpuOp`s with the **same** `device` and `verb`
//!   are merged. A `GpuOp` with a different verb (e.g. `call` vs `blocks`)
//!   breaks the run.
//! - The payloads are joined with `\n`, which is the device's line separator.
//!   A backend that splits on `\n` recovers the original payloads exactly.
//! - The `id` of the merged op is the `id` of the FIRST op in the run, so
//!   the SPIR-V blob name stays stable.
//! - Non-`GpuOp` statements between two `GpuOp`s break the run — the
//!   transform never reorders statements.
//!
//! ## Placement
//!
//! Registered in `transforms.rs`, SH2_TRANSFORMS-gated, applying to every
//! backend. Runs AFTER `fbdev-gpu` (which creates the `GpuOp` nodes) and
//! BEFORE `fbdev-gpu-elim` (which removes redundancy).

use crate::ir::{IrExpr, IrStmt};
use crate::shir_nodes::GpuOp;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// The device's batch separator. A backend that splits the payload on `\n`
/// recovers the original payloads exactly.
const BATCH_SEP: &str = "\n";

/// Verbs with append semantics: each payload line is an independent draw
/// command, so concatenating two payloads with `\n` is exactly the two
/// dispatches in sequence. Every other verb is refused: a state-setting
/// verb (`depthmask`, a `uniform` slot) is last-wins, and a `call` verb
/// (`swap`, `clear`) is a distinct action — joining `"swap"` + `"swap"`
/// into `"swap\nswap"` is a guess about the device's line protocol, and
/// joining `"0"` + `"1"` into `"0\n1"` is actively wrong.
const BATCHABLE_VERBS: [&str; 3] = ["blocks", "overlay", "overlay_flash"];

fn is_batchable(verb: &str) -> bool {
    BATCHABLE_VERBS.contains(&verb)
}

/// Extract the GpuOp from an IrStmt::Ext wrapper.
fn as_gpu_op(st: &IrStmt) -> Option<&GpuOp> {
    match st {
        IrStmt::Ext(n) => n.as_any().downcast_ref::<GpuOp>(),
        _ => None,
    }
}

/// Build the merged payload for a run of same device/verb/slot ops.
/// Returns None when the run is not mergeable (REFUSE > GUESS).
///
/// Two shapes, both line-identical to sequential dispatches (the device
/// parses payloads by lines, and every dispatch's bytes end with the
/// echo-added `"\n"` terminator — the join inserts exactly that byte):
/// - every payload a `Str`: join the TEXT with `"\n"`;
/// - every payload `payload_direct_ok` (a variable read, an
///   interpolation — the shape real programs write): value
///   concatenation `a + "\n" + b` as an `Interpolate`.
/// Anything else (a capture, a substitution, a computed word) is
/// refused: reordering evaluation around a side effect is a guess.
fn merge_payload(op_payloads: &[&GpuOp]) -> Option<IrExpr> {
    if !op_payloads
        .iter()
        .all(|op| super::fbdev_gpu_calls::payload_direct_ok(&op.payload))
    {
        return None;
    }
    if op_payloads
        .iter()
        .all(|op| matches!(&op.payload, IrExpr::Str(..)))
    {
        let texts: Vec<String> = op_payloads
            .iter()
            .map(|op| match &op.payload {
                IrExpr::Str(s, _) => s.clone(),
                _ => unreachable!("checked above"),
            })
            .collect();
        return Some(IrExpr::Str(
            texts.join(BATCH_SEP),
            crate::ir::StrStyle::DoubleQuoted,
        ));
    }
    let mut parts: Vec<crate::ir::InterpPart> = Vec::new();
    for (i, op) in op_payloads.iter().enumerate() {
        if i > 0 {
            parts.push(crate::ir::InterpPart::Lit(BATCH_SEP.to_string()));
        }
        parts.push(crate::ir::InterpPart::Expr(Box::new(op.payload.clone())));
    }
    Some(IrExpr::Interpolate(parts))
}

/// Build a merged GpuOp from a run of same device/verb/slot ops.
/// Returns None when the shape is not mergeable (REFUSE > GUESS):
/// a non-batchable verb, or a payload the join cannot carry exactly.
fn merge_ops(ops: &[&GpuOp]) -> Option<IrStmt> {
    if !is_batchable(&ops[0].verb) {
        return None;
    }
    let first = ops[0];
    let merged_payload = merge_payload(ops)?;
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    Some(IrStmt::Ext(Box::new(GpuOp {
        device: first.device.clone(),
        verb: first.verb.clone(),
        slot: first.slot.clone(),
        payload: merged_payload,
        id: format!("sh_gpu_batch_{seq}"),
    })))
}

/// Merge runs in one statement list, then recurse into each surviving
/// child for nested runs. Recursion is mutual (`batch_vec` ↔
/// `batch_stmt`): a vec-walker plus a one-level recursor silently
/// misses runs nested three or more levels deep (draws inside an `if`
/// inside a function), leaving separate dispatches — and `Ext` nodes —
/// behind.
fn batch_vec(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        // Find a run of consecutive GpuOps with the same device, verb
        // AND slot (a uniform's slot names a distinct register — merging
        // across slots would mix two scalars into one dispatch).
        let mut run_end = i + 1;
        if let Some(first_op) = as_gpu_op(&stmts[i]) {
            let (dev, verb, slot) = (
                first_op.device.clone(),
                first_op.verb.clone(),
                first_op.slot.clone(),
            );
            while run_end < stmts.len() {
                if let Some(op) = as_gpu_op(&stmts[run_end]) {
                    if op.device == dev && op.verb == verb && op.slot == slot {
                        run_end += 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        let run_len = run_end - i;
        if run_len > 1 {
            // Collect the ops in the run and merge them.
            let ops: Vec<&GpuOp> = stmts[i..run_end]
                .iter()
                .filter_map(as_gpu_op)
                .collect();
            if let Some(merged) = merge_ops(&ops) {
                // Replace the run with the merged op.
                stmts.drain(i..run_end);
                stmts.insert(i, merged);
                changed = true;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    // Then recurse into each surviving child, so runs nested inside an
    // `if`/loop/function merge too (mutual with `batch_stmt`).
    for st in stmts.iter_mut() {
        changed |= batch_stmt(st);
    }
    changed
}

/// Recurse from one statement into its composite bodies (a `GpuOp`
/// has no children, so this is a no-op for the merged node itself).
fn batch_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Block(b)
        | IrStmt::Function { body: b, .. }
        | IrStmt::While { body: b, .. }
        | IrStmt::For { body: b, .. }
        | IrStmt::ForInit { body: b, .. }
        | IrStmt::DoWhile { body: b, .. } => batch_vec(b),
        IrStmt::Subshell(b) | IrStmt::Background(b) => batch_vec(b),
        IrStmt::If { then: b, elsifs, else_, .. } => {
            let mut changed = batch_vec(b);
            for (_, eb) in elsifs.iter_mut() {
                changed |= batch_vec(eb);
            }
            changed |= batch_vec(else_);
            changed
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                changed |= batch_vec(stage);
            }
            changed
        }
        _ => false,
    }
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    batch_vec(stmts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn s(t: &str) -> IrExpr {
        IrExpr::Str(t.to_string(), StrStyle::DoubleQuoted)
    }

    fn gpu_op(device: &str, verb: &str, payload: &str) -> IrStmt {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        IrStmt::Ext(Box::new(GpuOp {
            device: device.to_string(),
            verb: verb.to_string(),
            slot: String::new(),
            payload: s(payload),
            id: format!("sh_gpu_test_{seq}"),
        }))
    }

    #[test]
    fn batches_consecutive_same_verb_ops() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "bg"),
            gpu_op("/dev/fb0", "blocks", "blk"),
            gpu_op("/dev/fb0", "blocks", "hud"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1, "three ops should merge into one");
        let op = as_gpu_op(&stmts[0]).unwrap();
        assert_eq!(op.device, "/dev/fb0");
        assert_eq!(op.verb, "blocks");
        match &op.payload {
            IrExpr::Str(t, _) => assert_eq!(t, "bg\nblk\nhud"),
            other => panic!("expected Str payload, got {other:?}"),
        }
    }

    #[test]
    fn does_not_batch_different_verbs() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "bg"),
            gpu_op("/dev/fb0", "call", "swap"),
            gpu_op("/dev/fb0", "blocks", "blk"),
        ];
        assert!(!transform(&mut stmts), "different verbs must not merge");
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn does_not_batch_different_devices() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/webgl", "blocks", "b"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn non_gpu_op_breaks_the_run() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            IrStmt::Expr(s("something")),
            gpu_op("/dev/fb0", "blocks", "b"),
        ];
        assert!(!transform(&mut stmts), "a non-GpuOp breaks the run");
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn single_op_is_unchanged() {
        let mut stmts = vec![gpu_op("/dev/fb0", "blocks", "a")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn is_idempotent() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/fb0", "blocks", "b"),
        ];
        assert!(transform(&mut stmts));
        assert!(!transform(&mut stmts), "a second run must change nothing");
    }

    #[test]
    fn handles_empty_list() {
        let mut stmts: Vec<IrStmt> = vec![];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn handles_no_gpu_ops() {
        let mut stmts = vec![IrStmt::Expr(s("hello"))];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    // ── the refusals are the point (REFUSE > GUESS) ──────────────────

    #[test]
    fn refuses_state_verbs() {
        // depthmask is last-wins: joining "0" + "1" into "0\n1" would
        // hand the device a two-line mask it never asked for.
        for verb in ["depthmask", "call", "uniform"] {
            let mut stmts = vec![
                gpu_op("/dev/fb0", verb, "a"),
                gpu_op("/dev/fb0", verb, "b"),
            ];
            assert!(!transform(&mut stmts), "{verb} must not batch");
            assert_eq!(stmts.len(), 2);
        }
    }

    #[test]
    fn refuses_slot_mismatch() {
        // two uniforms with different slots are different registers.
        let mut stmts = vec![
            IrStmt::Ext(Box::new(GpuOp {
                device: "/dev/fb0".into(),
                verb: "uniform".into(),
                slot: "1f/uCamYaw".into(),
                payload: s("1"),
                id: "a".into(),
            })),
            IrStmt::Ext(Box::new(GpuOp {
                device: "/dev/fb0".into(),
                verb: "uniform".into(),
                slot: "1f/uCamPitch".into(),
                payload: s("2"),
                id: "b".into(),
            })),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn refuses_a_capture_payload() {
        // a command substitution evaluates with side effects — joining
        // it into a concatenation reorders evaluation, so refuse.
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            IrStmt::Ext(Box::new(GpuOp {
                device: "/dev/fb0".into(),
                verb: "blocks".into(),
                slot: String::new(),
                payload: IrExpr::Capture {
                    expr: Box::new(s("gen")),
                    native: false,
                },
                id: "b".into(),
            })),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn batches_variable_payloads_by_concatenation() {
        // the MIMEcroft shape: `"$bg_p"` lowers to a getVar read, which
        // has no static text — the join is value concatenation.
        let getvar = |name: &str| {
            IrStmt::Ext(Box::new(GpuOp {
                device: "/dev/fb0".into(),
                verb: "blocks".into(),
                slot: String::new(),
                payload: IrExpr::Call {
                    func: "getVar".into(),
                    args: vec![s(name)],
                },
                id: format!("sh_gpu_test_{name}"),
            }))
        };
        let mut stmts = vec![getvar("bg_p"), getvar("blk_p")];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
        let op = as_gpu_op(&stmts[0]).unwrap();
        match &op.payload {
            IrExpr::Interpolate(parts) => {
                assert_eq!(parts.len(), 3, "read + sep + read");
                assert!(matches!(&parts[1], crate::ir::InterpPart::Lit(t) if t == "\n"));
            }
            other => panic!("expected concatenation, got {other:?}"),
        }
    }

    #[test]
    fn batches_overlay_too() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "overlay", "hud1"),
            gpu_op("/dev/fb0", "overlay", "hud2"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn merges_three_levels_deep() {
        // the MIMEcroft shape: draws inside an `if` inside a function.
        // A vec-walker plus a one-level recursor misses these and leaves
        // separate dispatches (and `Ext` nodes) behind.
        let mut stmts = vec![IrStmt::Function {
            name: "render".into(),
            params: vec![],
            body: vec![IrStmt::If {
                cond: IrExpr::Int(1),
                then: vec![
                    gpu_op("/dev/fb0", "blocks", "a"),
                    gpu_op("/dev/fb0", "blocks", "b"),
                ],
                elsifs: vec![],
                else_: vec![],
            }],
            named_blocks: vec![],
        }];
        assert!(transform(&mut stmts));
        let IrStmt::Function { body, .. } = &stmts[0] else { panic!() };
        let IrStmt::If { then, .. } = &body[0] else { panic!() };
        assert_eq!(then.len(), 1, "the deep pair should merge");
    }

    #[test]
    fn recurses_into_a_loop_body() {
        let mut stmts = vec![IrStmt::While {
            cond: IrExpr::Int(1),
            body: vec![
                gpu_op("/dev/fb0", "blocks", "a"),
                gpu_op("/dev/fb0", "blocks", "b"),
            ],
        }];
        assert!(transform(&mut stmts));
        let IrStmt::While { body, .. } = &stmts[0] else { panic!() };
        assert_eq!(body.len(), 1, "the inner pair should merge");
    }
}
