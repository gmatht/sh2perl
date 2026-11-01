//! unreachable-after-terminator — drop straight-line statements that can
//! never run because the statement before them leaves the block.
//!
//! ## Need
//! Shell code (and generated/template code) commonly guards failures with
//! `exit 1` / `return 1` early-outs, loops forever (`while true; do … done`
//! with no `break`), or `break`s out of a loop mid-body — and authors (or
//! the frontend) leave live-looking but dead statements after them. Every
//! backend emits the dead statements: a compare-and-branch that can never
//! be taken, or a print that can never happen. The shIR carries the
//! terminators as ordinary nodes, so the unreachable tail is removable at
//! the IR level for all backends at once.
//!
//! ## Scope — the sound version
//! Within a statement LIST (a body / top-level block / function body):
//!   - after a DIRECT terminator, every later statement in that list is
//!     dropped. Only a statement that ITSELF ends control flow counts:
//!     `Exit`/`Die` unconditionally, `Break`/`Continue` for the list they
//!     sit in, `Return` inside a function body, and an INFINITE loop
//!     (`while true` / `for ((;;))`) that no `break` can escape;
//!   - anything nested inside an `If`/`Case`/loop is CONDITIONAL — it does
//!     NOT deaden the statements after the ENCLOSING statement (the
//!     recursion handles the branch, then the list continues normally).
//! Only a top-of-block unconditional terminator prunes. Anything
//! ambiguous is left alone.
//!
//! ## The infinite-loop arm
//! `while true; do read_forever; done` never falls through, so a print
//! after it is dead. The verdict must be careful: a `break` anywhere in
//! the body (that is not captured by a NESTED loop) makes the loop
//! escapable. A `break` inside a nested `while`/`for`/`case` belongs to
//! that construct (bash's `case` does not capture `break` — it breaks the
//! enclosing loop — so a `case` body is scanned, a nested loop's is not).
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Complements
//! dead-store-elim and const-condition-elim (different unreachable
//! classes).

use crate::ir::IrStmt;

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let (out, changed) = pass(stmts.drain(..).collect(), false);
    *stmts = out;
    changed
}/// The terminator verdict is SHARED ([`crate::shir::stmt_terminates`]): the
/// renderers consult the same one to skip an epilogue or a synthesized
/// restore, so "dead after `exit`" and "dead after `while true`" can never
/// drift apart.
use crate::shir::stmt_terminates as terminates;


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, StrStyle};

    fn str_(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn echo(s: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![str_("echo"), IrExpr::Array(vec![str_(s)])],
        })
    }

    fn while_true(body: Vec<IrStmt>) -> IrStmt {
        IrStmt::While { cond: IrExpr::Bool(true), body }
    }

    /// `while true; do …; done; echo dead` — the loop has no escaping
    /// break, so the tail can never run.
    #[test]
    fn tail_after_an_infinite_loop_is_dropped() {
        let mut stmts = vec![while_true(vec![echo("tick")]), echo("dead")];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    /// …but a loop the body can `break` out of falls through normally.
    #[test]
    fn an_escapable_loop_keeps_the_tail() {
        let mut stmts = vec![while_true(vec![echo("tick"), IrStmt::Break]), echo("live")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    /// bash's `case` captures nothing: a `break` in a clause body still
    /// breaks the enclosing loop, so the loop IS escapable.
    #[test]
    fn a_break_inside_a_case_still_escapes_the_loop() {
        let case = IrStmt::Case {
            discriminant: str_("x"),
            clauses: vec![crate::ir::IrCaseClause {
                patterns: vec!["a".to_string()],
                body: vec![IrStmt::Break],
            }],
        };
        let mut stmts = vec![while_true(vec![case]), echo("live")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    /// …and a nested loop's `break` belongs to THAT loop, so the outer one
    /// still never falls through.
    #[test]
    fn a_break_inside_a_nested_loop_does_not_escape() {
        let inner = while_true(vec![IrStmt::Break]);
        let mut stmts = vec![while_true(vec![inner]), echo("dead")];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }

    /// `while true; do break || exit 1; done` — here `break` is a CONTROL
    /// CALL inside a `BinOp`, not `IrStmt::Break`. Missing that form made
    /// the loop look infinite and pruned the live tail, so
    /// `parse-break-or` printed nothing at all.
    #[test]
    fn a_break_control_call_makes_the_loop_escapable() {
        let break_or = IrStmt::Expr(IrExpr::BinOp {
            lhs: Box::new(IrExpr::Call {
                func: "break".to_string(),
                args: vec![],
            }),
            op: crate::ir::BinOpKind::Or,
            rhs: Box::new(str_("x")),
        });
        let mut stmts = vec![while_true(vec![break_or]), echo("live")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2, "live tail pruned");
    }

    /// Statements after a `break` in the SAME body are dead even though
    /// the loop itself is escapable.
    #[test]
    fn statements_after_a_break_are_dropped_from_the_body() {
        let mut stmts = vec![while_true(vec![IrStmt::Break, echo("dead")])];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::While { body, .. } => assert_eq!(body.len(), 1),
            other => panic!("expected the loop, got {other:?}"),
        }
    }

    /// Rebuilding a `Function` by hand used to pass `named_blocks:
    /// Vec::new()`, which DELETED a Go-style named return block (and its
    /// statements, and its tail-to-return lift) on every program the
    /// transform touched.
    #[test]
    fn a_functions_named_blocks_survive() {
        let mut stmts = vec![IrStmt::Function {
            name: "f".to_string(),
            params: vec![],
            body: vec![echo("a")],
            named_blocks: vec![("done".to_string(), vec![echo("b")])],
        }];
        transform(&mut stmts);
        match &stmts[0] {
            IrStmt::Function { named_blocks, .. } => {
                assert_eq!(named_blocks.len(), 1, "named block dropped");
            }
            other => panic!("expected the function, got {other:?}"),
        }
    }
}

fn pass(stmts: Vec<IrStmt>, in_fn: bool) -> (Vec<IrStmt>, bool) {
    let mut out: Vec<IrStmt> = Vec::new();
    let mut changed = false;
    let mut dead = false;
    for st in stmts {
        if dead {
            changed = true;
            continue;
        }
        if terminates(&st, in_fn) {
            out.push(st);
            dead = true;
            continue;
        }
        // recurse into nested bodies (never outside a function for Return)
        match st {
            IrStmt::If { cond, then, elsifs, else_ } => {
                let (t2, c1) = pass(then, in_fn);
                let mut eds = Vec::new();
                let mut c2 = c1;
                for (c, b) in elsifs {
                    let (b2, cb) = pass(b, in_fn);
                    c2 |= cb;
                    eds.push((c, b2));
                }
                let (e2, c3) = pass(else_, in_fn);
                changed |= c2 | c3;
                out.push(IrStmt::If { cond, then: t2, elsifs: eds, else_: e2 });
            }
            IrStmt::For { var, iter, body } => {
                let (b2, c) = pass(body, in_fn);
                changed |= c;
                out.push(IrStmt::For { var, iter, body: b2 });
            }
            IrStmt::While { cond, body } => {
                let (b2, c) = pass(body, in_fn);
                changed |= c;
                out.push(IrStmt::While { cond, body: b2 });
            }
            IrStmt::DoWhile { body, cond, until } => {
                let (b2, c) = pass(body, in_fn);
                changed |= c;
                out.push(IrStmt::DoWhile { body: b2, cond, until });
            }
            // `..rest` keeps every other field: rebuilding the node by hand
            // used to DROP `named_blocks` (a Go-style named return block),
            // which silently deleted the block's statements.
            IrStmt::Function { name, params, body, named_blocks } => {
                let (b2, c) = pass(body, true);
                let mut changed_here = c;
                let nb2: Vec<(String, Vec<IrStmt>)> = named_blocks
                    .into_iter()
                    .map(|(n, b)| {
                        let (bb, cb) = pass(b, true);
                        changed_here |= cb;
                        (n, bb)
                    })
                    .collect();
                changed |= changed_here;
                out.push(IrStmt::Function {
                    name,
                    params,
                    body: b2,
                    named_blocks: nb2,
                });
            }
            IrStmt::Block(v) => {
                let (b2, c) = pass(v, in_fn);
                changed |= c;
                out.push(IrStmt::Block(b2));
            }
            IrStmt::Subshell(v) => {
                let (b2, c) = pass(v, in_fn);
                changed |= c;
                out.push(IrStmt::Subshell(b2));
            }
            IrStmt::Background(v) => {
                let (b2, c) = pass(v, in_fn);
                changed |= c;
                out.push(IrStmt::Background(b2));
            }
            IrStmt::Case { discriminant, clauses } => {
                let mut cl2 = Vec::new();
                let mut cc = false;
                for mut clause in clauses {
                    let (b2, c) = pass(clause.body, in_fn);
                    cc |= c;
                    clause.body = b2;
                    cl2.push(clause);
                }
                changed |= cc;
                out.push(IrStmt::Case { discriminant, clauses: cl2 });
            }
            other => out.push(other),
        }
    }
    (out, changed)
}
