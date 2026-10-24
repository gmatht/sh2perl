//! pin_mirrors.rs — regression tests mirroring the Verus fidelity pins.
//!
//! Each test below asserts on the REAL transform the behavior the
//! corresponding `*_pins` lemma pins on the MODEL (see VERUS.md §(b) and
//! `sh2perl/proofs/verus/*.rs`). The models are hand-duplicates that can
//! drift; these tests are the living half of the tie — if model and code
//! disagree, these fail. Added 2026-10-24 (the DFE pins were already
//! mirrored by `dead_fn_elim.rs`'s own tests; the optimize and DSE pins
//! were not).

use debashl::ir::{AsmSpec, AssignTarget, Decl, IrExpr, IrStmt, StrStyle, optimize_stmts};
use debashl::transforms::dead_store_elim;
use debashl::transforms::redundant_store_elim;

// ---- shared builders (plain scalar shapes only — the modeled class) ----

fn assign(var: &str, expr: IrExpr) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: var.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr,
        asm: None,
    }
}

fn int_(n: i64) -> IrExpr {
    IrExpr::Int(n)
}

fn var_(name: &str) -> IrExpr {
    IrExpr::Var(name.to_string(), None)
}

fn str_(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}

fn output_(e: IrExpr) -> IrStmt {
    IrStmt::Output {
        value: e,
        newline: true,
        target: None,
    }
}

// ---- optimize_stmts Pass 1 (model: proofs/verus/optimize_stmts.rs) ----

/// Pin `declare_pins`: a lone `Declare[x]` is KEPT — the real census
/// (`collect_vars_in_stmt`) counts a `Declare`'s OWN names as references,
/// so the keep rule holds.
#[test]
fn pin_lone_declare_is_kept() {
    let stmts = vec![IrStmt::Declare {
        vars: vec![Decl {
            name: "x".to_string(),
            sigil: None,
        }],
        init: None,
        local: false,
    }];
    let out = optimize_stmts(&stmts);
    assert_eq!(
        out.len(),
        1,
        "lone Declare[x] survives: its own name counts as a reference"
    );
}

/// Pin `declare_pins`: an empty `Declare[]` is DROPPED (no names to keep it).
#[test]
fn pin_empty_declare_drops() {
    let stmts = vec![IrStmt::Declare {
        vars: vec![],
        init: None,
        local: false,
    }];
    let out = optimize_stmts(&stmts);
    assert!(
        out.is_empty(),
        "empty Declare[] drops: nothing references nothing"
    );
}

/// Pin `declare_pins`: a self-assign `x = x` (no asm) is DROPPED.
#[test]
fn pin_self_assign_drops() {
    let stmts = vec![IrStmt::Assign {
        targets: vec![AssignTarget {
            var: "x".to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: var_("x"),
        asm: None,
    }];
    let out = optimize_stmts(&stmts);
    assert!(out.is_empty(), "self-assign x=x is the identity update: drops");
}

/// Pin `declare_pins`: an asm-labeled self-assign is KEPT — the label is a
/// symbol declaration and would be dropped with the statement.
#[test]
fn pin_asm_self_assign_kept() {
    let stmts = vec![IrStmt::Assign {
        targets: vec![AssignTarget {
            var: "x".to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: var_("x"),
        asm: Some(AsmSpec {
            template: "nop".to_string(),
            volatile: false,
            outputs: vec![],
            inputs: vec![],
            clobbers: vec![],
        }),
    }];
    let out = optimize_stmts(&stmts);
    assert_eq!(
        out.len(),
        1,
        "asm self-assign kept: the label is load-bearing"
    );
}

// ---- dead_store_elim core (model: proofs/verus/dead_store_elim.rs) ----

/// Pin `pin_lone_store_drops`: a lone pure store to a never-read Const var
/// drops. (`y` here plays the role the model's pin gives its dropped name:
/// single static site, pure `Int` RHS, no reads anywhere.)
#[test]
fn pin_lone_pure_store_drops() {
    let mut prog = vec![assign("y", int_(5)), output_(str_("hi"))];
    assert!(
        dead_store_elim::transform(&mut prog),
        "lone pure dead store must drop"
    );
    assert_eq!(prog.len(), 1, "only the Output survives");
}

/// Pin `pin_read_store_kept`: a store whose var is read later is kept.
#[test]
fn pin_read_later_store_kept() {
    let mut prog = vec![assign("x", int_(5)), output_(var_("x"))];
    assert!(
        !dead_store_elim::transform(&mut prog),
        "read store must survive (returns false: nothing removed)"
    );
    assert_eq!(prog.len(), 2, "both statements survive");
}

/// Pin `pin_impure_rhs_kept`: a dead store with an IMPURE RHS is kept —
/// the call may do work even though its value is never read. (`echo`
/// is impure per `expr_pure` but not a dynamic write, so this exercises
/// exactly the purity gate, not the `has_dynamic_write` early return.)
#[test]
fn pin_impure_rhs_store_kept() {
    let mut prog = vec![
        assign(
            "x",
            IrExpr::Call {
                func: "echo".to_string(),
                args: vec![],
            },
        ),
        output_(str_("hi")),
    ];
    assert!(
        !dead_store_elim::transform(&mut prog),
        "dead store with impure RHS must survive the purity gate"
    );
    assert_eq!(prog.len(), 2, "the call stays: dropping it would skip its effects");
}

/// Pin `pin_multi_write_kept`: a twice-written var is Var, not Const, so
/// neither store is dead and both survive.
#[test]
fn pin_twice_written_stores_kept() {
    let mut prog = vec![assign("x", int_(5)), assign("x", int_(6))];
    assert!(
        !dead_store_elim::transform(&mut prog),
        "multi-write var is not Const: both stores survive"
    );
    assert_eq!(prog.len(), 2, "no store removed");
}

/// Pin `pin_dyn_freezes`: a dynamic write (`eval`) freezes the whole
/// transform — even the droppable `y` store survives, because the pass
/// returns before computing the dead set.
#[test]
fn pin_dynamic_write_freezes_all() {
    let mut prog = vec![
        assign("y", int_(5)),
        IrStmt::Expr(IrExpr::Call {
            func: "eval".to_string(),
            args: vec![str_("echo hi")],
        }),
    ];
    assert!(
        !dead_store_elim::transform(&mut prog),
        "dynamic write freezes the transform (early return, nothing removed)"
    );
    assert_eq!(prog.len(), 2, "even the dead store survives the freeze");
}

// ---- redundant_store_elim (model: proofs/verus/redundant_store_elim.rs) ----
//
// The three bug-scenario pins (side-effecting arith RHS, index read,
// Str-embedded `$x`) already live as regression tests in the transform's
// own `tests` module (they were the failing-first repros of the fix).
// Mirrored here are the model pins with NO in-module coverage: the
// cascade and the pure-observer stop. Builders below extend the shared
// scalar shapes with indexed targets and observer statements.

fn assign_idx(arr: &str, idx: IrExpr, rhs: IrExpr) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: arr.to_string(),
            sigil: None,
            indices: vec![idx],
        }],
        expr: rhs,
        asm: None,
    }
}

/// Pin `pin_cascade_drops`: `x=1; x=2; x=3` drops the first two stores —
/// each is overwritten by a later scalar store with a clean middle.
#[test]
fn pin_rse_cascade_drops_first_two() {
    let mut prog = vec![
        assign("x", int_(5)),
        assign("x", int_(6)),
        assign("x", int_(7)),
    ];
    assert!(
        redundant_store_elim::transform(&mut prog),
        "cascading overwrites must drop"
    );
    assert_eq!(prog.len(), 1, "only the last store survives");
    assert_eq!(prog[0], assign("x", int_(7)));
}

/// Pin `pin_observer_keeps`: `x=5; <call>; x=6` keeps the head — a call
/// is an indirect observer, so no justifier is clean.
#[test]
fn pin_rse_observer_stops_scan() {
    let mut prog = vec![
        assign("x", int_(5)),
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![str_("echo"), str_("hi")],
        }),
        assign("x", int_(6)),
    ];
    assert!(
        !redundant_store_elim::transform(&mut prog),
        "indirect observer between the stores: nothing removed"
    );
    assert_eq!(prog.len(), 3, "all three statements survive");
}

/// Pin `pin_bug2_idx_read_keeps` (mirror of the in-module
/// `keeps_store_read_via_assign_target_index`): `x=5; arr[x]=1; x=6`
/// keeps all three — the middle store's target index reads `x`.
#[test]
fn pin_rse_index_read_keeps_head() {
    let mut prog = vec![
        assign("x", int_(5)),
        assign_idx("arr", var_("x"), int_(1)),
        assign("x", int_(6)),
    ];
    assert!(
        !redundant_store_elim::transform(&mut prog),
        "index read observes the earlier store"
    );
    assert_eq!(prog.len(), 3, "all three statements survive");
    assert_eq!(prog[0], assign("x", int_(5)));
}
