//! else-if-flatten — an `else` whose ONLY statement is an `If` IS an
//! `elif`: flatten it into the parent's `elsifs` chain.
//!
//! ## Need
//!
//! `IrStmt::If` carries an explicit `elsifs` vector, but frontends differ in
//! how they express a chain. The sh frontend fills `elsifs` directly
//! (`elif`); the py frontend lowers `elif` — and any `else:` whose body is a
//! single `if` — as a nested `If` inside `else`, i.e.
//!
//! ```text
//! If { cond: x==1, then: [one],
//!      else: [ If { cond: x==2, then: [two], else: [other] } ] }
//! ```
//!
//! Every renderer then emits the nested form (`} else { if … { } else { } }`),
//! which is a level deeper than the source and, in Rust, a needless block.
//! Flattening is semantically free — `else { if c { A } else { B } }` and
//! `else if c { A } else { B }` are the same statement — and it lets every
//! backend's existing `elsifs` handling do the work (Rust's `} else if c {`,
//! and the `let __b` status protocol per condition).
//!
//! ## The sound rule
//!
//! `else` is flattened into `elsifs` only when it contains EXACTLY one
//! statement and that statement is an `If` with an EMPTY `elsifs` of its own
//! (a nested chain is flattened by recursing, so the whole chain lands in one
//! `elsifs` vector). The nested `If`'s then/else become the new branch, and
//! the parent's `else_` becomes the nested one's — which may itself be empty
//! (an `elif` with no trailing `else`), exactly the shape `elsifs` supports.
//!
//! Anything else (`else { if …; echo x; }`, a labelled body, a block) keeps
//! its nesting: the extra statement means the `else` is not a pure chain.

use crate::ir::IrStmt;

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= walk(st);
    }
    changed
}

fn walk(st: &mut IrStmt) -> bool {
    let mut changed = false;
    // The chain flattening LOOP below subsumes the nested case, so it runs
    // first (a chain of any depth collapses in one call), then the ordinary
    // recursion visits whatever is left in the branch bodies.
    changed |= flatten_chain(st);
    // descend into the shapes that carry child statements
    for child in stmts_of(st) {
        for s in child.iter_mut() {
            changed |= walk(s);
        }
    }
    changed
}

/// Collapse `else { if … }` into the parent's `elsifs`, repeatedly, so a
/// chain of any depth lands in one vector.
fn flatten_chain(st: &mut IrStmt) -> bool {
    let mut changed = false;
    let IrStmt::If { elsifs, else_, .. } = st else {
        return false;
    };
    loop {
        let flatten = matches!(
            else_.as_slice(),
            [IrStmt::If { elsifs: inner_elsifs, .. }] if inner_elsifs.is_empty()
        );
        if !flatten {
            break;
        }
        let nested = else_.remove(0);
        let IrStmt::If { cond: c2, then: t2, elsifs: _e2, else_: el2 } = nested else {
            unreachable!()
        };
        elsifs.push((c2, t2));
        *else_ = el2;
        changed = true;
    }
    changed
}

/// The statement vectors a node owns directly (for the recursive walk).
fn stmts_of(st: &mut IrStmt) -> Vec<&mut Vec<IrStmt>> {
    match st {
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::ForInit { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => vec![body],
        IrStmt::Function { body, named_blocks, .. } => {
            let mut v: Vec<&mut Vec<IrStmt>> = vec![body];
            for (_, b) in named_blocks.iter_mut() {
                v.push(b);
            }
            v
        }
        IrStmt::Pipeline { stages, .. } => stages.iter_mut().collect(),
        IrStmt::Redirect { inner, .. } => vec![inner],
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut v: Vec<&mut Vec<IrStmt>> = vec![body, else_body, finally_body];
            for e in excepts.iter_mut() {
                v.push(&mut e.body);
            }
            v
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BinOpKind, IrExpr, IrProgram, IrStmt};

    fn empty_prog() -> IrProgram {
        let mut p = IrProgram::from_raw_perl("");
        p.stmts.clear();
        p
    }

    fn t(cond: &str) -> IrExpr {
        IrExpr::Call {
            func: "test".to_string(),
            args: vec![IrExpr::Str(cond.to_string(), crate::ir::StrStyle::DoubleQuoted)],
        }
    }

    fn print(w: &str) -> IrStmt {
        IrStmt::Output {
            value: IrExpr::Str(w.to_string(), crate::ir::StrStyle::DoubleQuoted),
            newline: true,
            target: None,
        }
    }

    #[test]
    fn else_if_becomes_an_elsif() {
        // `if a { x } else { if b { y } else { z } }`
        let mut prog = empty_prog();
        prog.stmts.push(IrStmt::If {
            cond: t("a"),
            then: vec![print("x")],
            elsifs: vec![],
            else_: vec![IrStmt::If {
                cond: t("b"),
                then: vec![print("y")],
                elsifs: vec![],
                else_: vec![print("z")],
            }],
        });
        assert!(transform(&mut prog.stmts));
        match &prog.stmts[0] {
            IrStmt::If { elsifs, else_, .. } => {
                assert_eq!(elsifs.len(), 1, "the nested if became an elsif");
                assert_eq!(else_.len(), 1, "the trailing else survives");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn a_bare_nested_if_chain_flattens_fully() {
        // three levels: every else is a lone `if`
        let mut prog = empty_prog();
        let leaf = |c: &str, w: &str| IrStmt::If {
            cond: t(c),
            then: vec![print(w)],
            elsifs: vec![],
            else_: vec![],
        };
        prog.stmts.push(IrStmt::If {
            cond: t("a"),
            then: vec![print("1")],
            elsifs: vec![],
            else_: vec![IrStmt::If {
                cond: t("b"),
                then: vec![print("2")],
                elsifs: vec![],
                else_: vec![leaf("c", "3")],
            }],
        });
        assert!(transform(&mut prog.stmts));
        match &prog.stmts[0] {
            IrStmt::If { elsifs, else_, .. } => {
                assert_eq!(elsifs.len(), 2, "both nested ifs became elsifs");
                assert!(else_.is_empty(), "no trailing else");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn an_else_with_extra_statements_keeps_its_nesting() {
        // `else { if b { y }; echo tail }` is NOT a pure chain
        let mut prog = empty_prog();
        prog.stmts.push(IrStmt::If {
            cond: t("a"),
            then: vec![print("x")],
            elsifs: vec![],
            else_: vec![
                IrStmt::If {
                    cond: t("b"),
                    then: vec![print("y")],
                    elsifs: vec![],
                    else_: vec![],
                },
                print("tail"),
            ],
        });
        assert!(!transform(&mut prog.stmts));
        match &prog.stmts[0] {
            IrStmt::If { elsifs, else_, .. } => {
                assert!(elsifs.is_empty());
                assert_eq!(else_.len(), 2);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn a_non_test_condition_is_untouched() {
        let mut prog = empty_prog();
        prog.stmts.push(IrStmt::If {
            cond: IrExpr::BinOp {
                op: BinOpKind::And,
                lhs: Box::new(t("a")),
                rhs: Box::new(t("b")),
            },
            then: vec![print("x")],
            elsifs: vec![],
            else_: vec![IrStmt::If {
                cond: t("c"),
                then: vec![print("y")],
                elsifs: vec![],
                else_: vec![],
            }],
        });
        // still a lone-if else, so it flattens; the condition type is not
        // part of the rule
        assert!(transform(&mut prog.stmts));
    }
}
