//! `DefinedOr` → IR-level temp hoist (v136).
//!
//! `x=${a:-b}` reaches the shared IR as
//! `IrStmt::Assign { targets: [x], expr: IrExpr::DefinedOr { expr: a, default: b } }`.
//! Every renderer with no special case emits the subject *twice* — once for the
//! "is it set/non-empty" test and once for the value — so a subject with a side
//! effect (`$(cmd)`, `$((i++))`, a function call) runs twice. `estree` and `rust`
//! bind a temp locally; the other renderers do not.
//!
//! This pass makes the single evaluation explicit in the **shared** IR, so every
//! renderer inherits it: the owning assignment is split into
//!
//! ```text
//!   __sh2_dor_0 = a;              // the subject, evaluated exactly once
//!   x = ${__sh2_dor_0:-b};        // the original assignment, over the temp
//! ```
//!
//! Both halves are ordinary `IrStmt::Assign`/`IrExpr::Var` nodes, so no renderer
//! needs to learn a new node and no new runtime helper is introduced. The rewrite
//! is meaning-preserving under bash: `${a:-b}` evaluates `a` once at exactly that
//! point, and `t=a; x=${t:-b}` evaluates it once at the same point.
//!
//! # Scope (deliberately narrow)
//!
//! Fires only on a whole assigned expression — `Assign` with a single target —
//! because that is the only position where the temp can be bound by a preceding
//! statement without restructuring control flow. A `Declare` init (`local
//! x=${a:-b}`) and a `DefinedOr` nested deeper inside an expression are left
//! untouched. A subject that is already a leaf (`Var`/`Int`/`Str`/`Bool`) is also
//! left alone: re-reading a leaf is free and side-effect free, so a temp would
//! only add noise.
//!
//! Temp names are `__sh2_dor_<n>`, numbered per run from 0, so the transform is
//! deterministic (the corpus gates compare output text) and two hoists in one
//! program cannot collide.

use crate::ir::{AssignTarget, IrExpr, IrProgram, IrStmt};
use crate::shir_passes::{PassContext, Transform};

/// The shared `DefinedOr` temp-hoist transform.
pub struct DefinedOrHoist;

impl Transform for DefinedOrHoist {
    fn name(&self) -> &'static str {
        "defined_or_hoist"
    }

    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let mut n = 0usize;
        rewrite_block(&mut prog.stmts, &mut n);
        for sub in prog.subs.iter_mut() {
            rewrite_block(&mut sub.body, &mut n);
        }
    }
}

/// Is the subject cheap enough that a temp is pure overhead?
fn is_leaf(e: &IrExpr) -> bool {
    matches!(
        e,
        IrExpr::Var(_, _) | IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Bool(_)
    )
}

/// Rewrite one statement list, splicing in the hoist assignments.
fn rewrite_block(stmts: &mut Vec<IrStmt>, n: &mut usize) {
    let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
    for mut st in stmts.drain(..) {
        // `x = <subject> ?? <default>` with a single target and a non-leaf
        // subject: bind the subject to a temp first, then reuse the temp.
        if let IrStmt::Assign { targets, expr, asm } = &mut st {
            if targets.len() == 1 {
                if let IrExpr::DefinedOr { expr: subject, default } = expr {
                    if !is_leaf(subject) {
                        let tmp = format!("__sh2_dor_{n}");
                        *n += 1;
                        let subject = std::mem::replace(&mut **subject, IrExpr::Int(0));
                        let default = std::mem::replace(&mut **default, IrExpr::Int(0));
                        let target = targets[0].clone();
                        let asm = asm.clone();
                        out.push(IrStmt::Assign {
                            targets: vec![AssignTarget {
                                var: tmp.clone(),
                                sigil: None,
                                indices: Vec::new(),
                            }],
                            expr: subject,
                            asm: None,
                        });
                        out.push(IrStmt::Assign {
                            targets: vec![target],
                            expr: IrExpr::DefinedOr {
                                expr: Box::new(IrExpr::Var(tmp, None)),
                                default: Box::new(default),
                            },
                            asm,
                        });
                        continue;
                    }
                }
            }
        }
        // Recurse into nested statement lists. Everything else is a leaf here.
        descend(&mut st, n);
        out.push(st);
    }
    *stmts = out;
}

/// Recurse into every nested statement list of a statement.
fn descend(st: &mut IrStmt, n: &mut usize) {
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            rewrite_block(then, n);
            for (_, body) in elsifs.iter_mut() {
                rewrite_block(body, n);
            }
            rewrite_block(else_, n);
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            rewrite_block(body, n);
            for ex in excepts.iter_mut() {
                rewrite_block(&mut ex.body, n);
            }
            rewrite_block(else_body, n);
            rewrite_block(finally_body, n);
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => rewrite_block(body, n),
        IrStmt::For { body, .. } => rewrite_block(body, n),
        IrStmt::ForInit { init, step, body, .. } => {
            rewrite_block(init, n);
            rewrite_block(step, n);
            rewrite_block(body, n);
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses.iter_mut() {
                rewrite_block(&mut c.body, n);
            }
        }
        IrStmt::Redirect { inner, .. } => rewrite_block(inner, n),
        IrStmt::Function { body, named_blocks, .. } => {
            rewrite_block(body, n);
            for (_, b) in named_blocks.iter_mut() {
                rewrite_block(b, n);
            }
        }
        IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => rewrite_block(b, n),
        IrStmt::Pipeline { stages, .. } => {
            for s in stages.iter_mut() {
                rewrite_block(s, n);
            }
        }
        IrStmt::Select { clauses } => {
            for c in clauses.iter_mut() {
                rewrite_block(&mut c.body, n);
            }
        }
        IrStmt::Shaderize { body, .. } => rewrite_block(body, n),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, IrProgram, StrStyle};

    fn assign(target: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: target.to_string(),
                sigil: None,
                indices: Vec::new(),
            }],
            expr,
            asm: None,
        }
    }

    fn program(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_storage: vec![],
            var_nospace: vec![],
            var_bash_env: vec![],
        }
    }

    /// A capture subject (the `$(cmd)` shape) is bound once, before the
    /// assignment, and the assignment reads the temp.
    #[test]
    fn hoists_capture_subject() {
        let capture = IrExpr::Capture {
            expr: Box::new(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![],
            }),
            native: false,
        };
        let mut prog = program(vec![assign(
            "x",
            IrExpr::DefinedOr {
                expr: Box::new(capture),
                default: Box::new(IrExpr::Str("d".to_string(), StrStyle::DoubleQuoted)),
            },
        )]);
        DefinedOrHoist.run(&mut prog, &PassContext::default());
        assert_eq!(prog.stmts.len(), 2, "hoist must splice one binding + the assign");
        match &prog.stmts[0] {
            IrStmt::Assign { targets, expr, .. } => {
                assert_eq!(targets[0].var, "__sh2_dor_0");
                assert!(matches!(expr, IrExpr::Capture { .. }));
            }
            other => panic!("expected the temp binding, got {other:?}"),
        }
        match &prog.stmts[1] {
            IrStmt::Assign { targets, expr, .. } => {
                assert_eq!(targets[0].var, "x");
                match expr {
                    IrExpr::DefinedOr { expr, .. } => {
                        assert_eq!(**expr, IrExpr::Var("__sh2_dor_0".to_string(), None))
                    }
                    other => panic!("expected DefinedOr over the temp, got {other:?}"),
                }
            }
            other => panic!("expected the original assignment, got {other:?}"),
        }
    }

    /// A leaf subject needs no temp — the statement list is untouched.
    #[test]
    fn leaves_leaf_subject_alone() {
        let mut prog = program(vec![assign(
            "x",
            IrExpr::DefinedOr {
                expr: Box::new(IrExpr::Var("a".to_string(), None)),
                default: Box::new(IrExpr::Str("d".to_string(), StrStyle::DoubleQuoted)),
            },
        )]);
        DefinedOrHoist.run(&mut prog, &PassContext::default());
        assert_eq!(prog.stmts.len(), 1);
        match &prog.stmts[0] {
            IrStmt::Assign { expr: IrExpr::DefinedOr { expr, .. }, .. } => {
                assert_eq!(**expr, IrExpr::Var("a".to_string(), None));
            }
            other => panic!("expected the untouched DefinedOr, got {other:?}"),
        }
    }

    /// Nested statement lists are rewritten too (the `if` body here).
    #[test]
    fn hoists_inside_nested_block() {
        let inner = assign(
            "y",
            IrExpr::DefinedOr {
                expr: Box::new(IrExpr::Capture {
                    expr: Box::new(IrExpr::Int(1)),
                    native: false,
                }),
                default: Box::new(IrExpr::Str("d".to_string(), StrStyle::DoubleQuoted)),
            },
        );
        let mut prog = program(vec![IrStmt::If {
            cond: IrExpr::Bool(true),
            then: vec![inner],
            elsifs: vec![],
            else_: vec![],
        }]);
        DefinedOrHoist.run(&mut prog, &PassContext::default());
        match &prog.stmts[0] {
            IrStmt::If { then, .. } => assert_eq!(then.len(), 2, "inner assign must be split"),
            other => panic!("expected the If, got {other:?}"),
        }
    }
}
