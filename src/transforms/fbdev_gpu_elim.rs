//! fbdev-gpu-elim — eliminate redundant GpuOps.
//!
//! ## Need
//!
//! After `fbdev-gpu` lifts writes to `/dev/fb0` into `GpuOp` nodes, a
//! shell program may contain redundant sequences:
//!
//! A `GpuOp` that is an **exact, consecutive** duplicate of its
//! predecessor — same `device`, `verb`, `slot`, AND payload, with no
//! statement between them — is a no-op: no observer can see the
//! intermediate device state, so dropping the second write leaves every
//! execution identical. Examples: two `echo "swap" > /dev/fb0/call` in
//! a row, or the same `blocks` payload submitted twice.
//!
//! ## Why only consecutive (REFUSE > GUESS)
//!
//! A duplicate with an intervening statement is NOT removed. The code
//! between the two writes could read the device back, or the second
//! write could be the deliberate re-assert after a `call` that consumed
//! state — the transform cannot see either, so it leaves the pair
//! alone. Same rule for a duplicate across a branch or loop boundary:
//! only one flat statement list is ever scanned.
//!
//! ## Placement
//!
//! Registered in `transforms.rs`, SH2_TRANSFORMS-gated, applying to every
//! backend. Runs AFTER `fbdev-gpu` and `fbdev-gpu-batch`.

use crate::ir::IrStmt;
use crate::shir_nodes::GpuOp;

/// Extract the GpuOp from an IrStmt::Ext wrapper.
fn as_gpu_op(st: &IrStmt) -> Option<&GpuOp> {
    match st {
        IrStmt::Ext(n) => n.as_any().downcast_ref::<GpuOp>(),
        _ => None,
    }
}

/// Returns true if two GpuOps are exact duplicates (same device, verb, slot, payload).
fn is_duplicate(a: &GpuOp, b: &GpuOp) -> bool {
    a.device == b.device
        && a.verb == b.verb
        && a.slot == b.slot
        && a.payload == b.payload
}

/// Drop consecutive exact duplicates in one flat statement list, then
/// recurse into each surviving child (mutual with `elim_stmt` — a
/// one-level recursor misses duplicates nested three or more levels
/// deep and leaves `Ext` nodes behind that poison array-size proofs).
fn elim_vec(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    let mut i = 1;
    while i < stmts.len() {
        let dup = match (as_gpu_op(&stmts[i - 1]), as_gpu_op(&stmts[i])) {
            (Some(a), Some(b)) => is_duplicate(a, b),
            _ => false,
        };
        if dup {
            stmts.remove(i);
            changed = true;
            // Don't advance — the next statement shifted into slot `i`
            // and may itself duplicate the survivor at `i - 1`.
        } else {
            i += 1;
        }
    }
    for st in stmts.iter_mut() {
        changed |= elim_stmt(st);
    }
    changed
}

/// Recurse from one statement into its composite bodies.
fn elim_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Block(b)
        | IrStmt::Function { body: b, .. }
        | IrStmt::While { body: b, .. }
        | IrStmt::For { body: b, .. }
        | IrStmt::ForInit { body: b, .. }
        | IrStmt::DoWhile { body: b, .. } => elim_vec(b),
        IrStmt::Subshell(b) | IrStmt::Background(b) => elim_vec(b),
        IrStmt::If { then: b, elsifs, else_, .. } => {
            let mut changed = elim_vec(b);
            for (_, eb) in elsifs.iter_mut() {
                changed |= elim_vec(eb);
            }
            changed |= elim_vec(else_);
            changed
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut changed = false;
            for stage in stages.iter_mut() {
                changed |= elim_vec(stage);
            }
            changed
        }
        _ => false,
    }
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    elim_vec(stmts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, StrStyle};

    fn s(t: &str) -> IrExpr {
        IrExpr::Str(t.to_string(), StrStyle::DoubleQuoted)
    }

    fn gpu_op(device: &str, verb: &str, payload: &str) -> IrStmt {
        IrStmt::Ext(Box::new(GpuOp {
            device: device.to_string(),
            verb: verb.to_string(),
            slot: String::new(),
            payload: s(payload),
            id: format!("sh_gpu_elim_test_{}", device.len() + verb.len()),
        }))
    }

    #[test]
    fn removes_exact_duplicates() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/fb0", "blocks", "a"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1, "the duplicate should be removed");
    }

    #[test]
    fn keeps_different_payloads() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/fb0", "blocks", "b"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn keeps_different_verbs() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/fb0", "call", "a"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn keeps_different_devices() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/webgl", "blocks", "a"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn removes_a_run_of_three() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/fb0", "blocks", "a"),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1, "a run of three collapses to one");
    }

    #[test]
    fn keeps_non_consecutive_duplicates() {
        // the code between the two writes could observe the device —
        // only consecutive pairs are safe to collapse.
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            gpu_op("/dev/fb0", "blocks", "b"),
            gpu_op("/dev/fb0", "blocks", "a"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn intervening_statement_breaks_the_pair() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
            IrStmt::Expr(s("something")),
            gpu_op("/dev/fb0", "blocks", "a"),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn collapses_three_levels_deep() {
        // draws inside an `if` inside a function — a one-level recursor
        // leaves the duplicate (and its `Ext` node) behind.
        let mut stmts = vec![IrStmt::Function {
            name: "render".into(),
            params: vec![],
            body: vec![IrStmt::If {
                cond: IrExpr::Int(1),
                then: vec![
                    gpu_op("/dev/fb0", "call", "swap"),
                    gpu_op("/dev/fb0", "call", "swap"),
                ],
                elsifs: vec![],
                else_: vec![],
            }],
            named_blocks: vec![],
        }];
        assert!(transform(&mut stmts));
        let IrStmt::Function { body, .. } = &stmts[0] else { panic!() };
        let IrStmt::If { then, .. } = &body[0] else { panic!() };
        assert_eq!(then.len(), 1, "the deep duplicate should collapse");
    }

    #[test]
    fn recurses_into_a_loop_body() {
        let mut stmts = vec![IrStmt::While {
            cond: IrExpr::Int(1),
            body: vec![
                gpu_op("/dev/fb0", "call", "swap"),
                gpu_op("/dev/fb0", "call", "swap"),
            ],
        }];
        assert!(transform(&mut stmts));
        let IrStmt::While { body, .. } = &stmts[0] else { panic!() };
        assert_eq!(body.len(), 1, "the inner duplicate should collapse");
    }

    #[test]
    fn non_gpu_ops_are_untouched() {
        let mut stmts = vec![
            IrStmt::Expr(s("hello")),
            gpu_op("/dev/fb0", "blocks", "a"),
            IrStmt::Expr(s("world")),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 3);
    }

    #[test]
    fn is_idempotent() {
        let mut stmts = vec![
            gpu_op("/dev/fb0", "blocks", "a"),
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

    #[test]
    fn duplicate_with_different_slot_is_kept() {
        let mut stmts = vec![
            IrStmt::Ext(Box::new(GpuOp {
                device: "/dev/fb0".to_string(),
                verb: "uniform".to_string(),
                slot: "1f/uCamYaw".to_string(),
                payload: s("42"),
                id: "a".to_string(),
            })),
            IrStmt::Ext(Box::new(GpuOp {
                device: "/dev/fb0".to_string(),
                verb: "uniform".to_string(),
                slot: "1f/uCamPitch".to_string(),
                payload: s("42"),
                id: "b".to_string(),
            })),
        ];
        assert!(!transform(&mut stmts), "different slots must not be deduplicated");
        assert_eq!(stmts.len(), 2);
    }
}
