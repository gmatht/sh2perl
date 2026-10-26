//! not-identity — negation identities over `IrExpr`:
//!   `!(!x)` → `x` (double negation)
//!   `!(a OP b)` → `a (flip OP) b` for OP ∈ {==,!=,<,>,<=,>=}
//!
//! ## Need
//! The lowering layers wrap conditions in `Not` freely: the parser for
//! `! cmd`, `test_lowering`'s [`not_wrap`] for `-z`/`-n`/`!=` arms
//! (`-z X` → `Not(X ne "")`), case/pattern code for missed matches.
//! Renderers then compose their own negation idioms on top — rust's
//! `X != ""` empty-string arm already renders `!({X}).is_empty()`, so
//! the IR `Not` around it ships `!(!({X}).is_empty())` — measured: 11
//! corpus files carry the `!(!(` residue in Rust (unset-var tests,
//! tty-cmdsub, the parse-* probes), with the same nesting wherever the
//! other backends wrap twice. Flipping the comparison gives every
//! backend a single-level condition, and feeds `const-condition-elim`
//! a flatter cond to work with.
//!
//! ## Soundness
//! Double negation: `¬¬t(v) = t(v)` for the boolean truthiness `t`
//! every backend implements; operand side effects evaluate once either
//! way. This rule is renderer-agnostic and runs for EVERY target.
//!
//! Comparison flip: `¬(a==b) ≡ a!=b`, `¬(a<b) ≡ a>=b` — exact
//! complements over the SAME operands only when the renderer's two
//! operators are themselves exact complements. They are NOT in Perl:
//! `ir_expr_to_perl` renders `Eq` as NUMERIC `==` but `Ne` as STRING
//! `ne`, so flipping `¬(X ne "")` → `X == ""` turns a correct string
//! test into a numeric one (measured regression: 4 corpus files —
//! `double-equals-in-test`, `test-expr-true`, `heredoc-binary-data`,
//! `tty-cmdsub` — e.g. `Argument "value" isn't numeric in numeric eq`
//! and a false `match`). So the flip is its own transform,
//! `not-identity-flip`, TARGET-GATED in `transform_applies` to rust
//! (verified: `==`/`!=` and `<`/`>=` pairs, plus the `Ne(X,"")` empty-
//! string idiom that renders `!({X}).is_empty()` — the flip removes the
//! measured `!(!(` residue). Refuse > guess: every other target keeps
//! its `Not` until its renderer's op pairs are verified. `&&`/`||` are
//! NOT flipped (De Morgan changes the operand structure — out of scope).
//!
//! Well-formedness guards (refuse > guess, mirroring `not_wrap`'s
//! doc): a `Not` carries its operand DUPLICATED in `lhs` and `rhs`
//! because rust renders `lhs` while Perl renders `rhs` (a single-sided
//! `Not` was once the `if (!(0))` always-true bug). Both the outer and
//! — for the double-negation rule — the inner `Not` must be
//! duplicated-consistent (`lhs == rhs`); a single-sided `Not` is a
//! construction bug this transform must not paper over, so it is left
//! untouched.
//!
//! ## Placement
//! Registered FOUR times in `transforms::all()`: `not-identity` (the
//! fold) early and late, `not-identity-flip` (rust-gated) early and
//! late. Early = parser-minted Nots, before `const-status-cond` /
//! `const-condition-elim` can consume the flatter condition; late =
//! right after `case-lowering`, because `test-lowering` /
//! `case-lowering` run after the early slot and mint fresh
//! `Not(Ne)` pairs through `not_wrap` (`-z X` → `Not(X ne "")`) —
//! that is where the measured `!(!(` residue (12 corpus files) came
//! from. Pure structural rewrite: no verdict statics, idempotent by
//! construction, shell-agnostic (any source language feeding the shared
//! IR).

use crate::ir::{BinOpKind, IrExpr, IrStmt};
use std::cell::Cell;

/// Which negation rule the traversal applies (module doc: the fold is
/// renderer-agnostic, the flip is gated to rust).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Fold,
    Flip,
}

thread_local! {
    // Thread-local (not a traversal parameter) so the ~60 recursive
    // `rewrite_*` sites stay rule-agnostic; `run`/`fold_not` set it for
    // their traversal and restore the previous value. The Rust test
    // harness runs each test on its own thread, so no cross-test leak.
    static MODE: Cell<Mode> = const { Cell::new(Mode::Fold) };
}

/// Double-negation fold `!(!x)` → `x`. Every target (renderer-
/// agnostic: `¬¬t(v) = t(v)` for any backend's truthiness). Returns
/// whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    run(stmts, Mode::Fold)
}

/// Comparison flip `!(a OP b)` → `a (flip OP) b`. Registered under the
/// separate name `not-identity-flip` so `transform_applies` can gate it
/// to rust — Perl renders `Eq` as numeric `==` but `Ne` as string
/// `ne`, so flipping there inverts the operand semantics (module doc).
pub fn transform_flip(stmts: &mut Vec<IrStmt>) -> bool {
    run(stmts, Mode::Flip)
}

fn run(stmts: &mut Vec<IrStmt>, mode: Mode) -> bool {
    let prev = MODE.with(|m| m.replace(mode));
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= rewrite_stmt(st);
    }
    MODE.with(|m| m.set(prev));
    changed
}

/// Both rules in sequence — the TEST helper (production runs one rule
/// per registration via `run`, which sets `MODE`).
fn fold_not(e: &mut IrExpr) -> bool {
    let prev = MODE.with(|m| m.replace(Mode::Fold));
    let a = fold_not_in(e);
    MODE.with(|m| m.set(Mode::Flip));
    let b = fold_not_in(e);
    MODE.with(|m| m.set(prev));
    a || b
}

/// Negation identities, one rule per `MODE`. `!(!x)` → `x`;
/// `!(a OP b)` → `a (flip OP) b`. Requires the outer `Not` to be
/// duplicated-consistent; for the double-negation rule the inner `Not`
/// must be as well. Replaces the whole expression and recurses, so
/// `!(!(!(!x)))` unwinds completely in one pass.
fn fold_not_in(e: &mut IrExpr) -> bool {
    let mode = MODE.with(|m| m.get());
    let IrExpr::BinOp {
        op: BinOpKind::Not,
        lhs: ol,
        rhs: or,
    } = e
    else {
        return false;
    };
    if ol.as_ref() != or.as_ref() {
        // single-sided Not — construction bug, not ours to repair
        return false;
    }
    let IrExpr::BinOp { op, lhs, rhs } = or.as_ref() else {
        return false;
    };
    if mode == Mode::Flip {
        // ¬(a OP b) → a (flip OP) b
        let flipped = match op {
            BinOpKind::Eq => Some(BinOpKind::Ne),
            BinOpKind::Ne => Some(BinOpKind::Eq),
            BinOpKind::Lt => Some(BinOpKind::Ge),
            BinOpKind::Gt => Some(BinOpKind::Le),
            BinOpKind::Le => Some(BinOpKind::Gt),
            BinOpKind::Ge => Some(BinOpKind::Lt),
            _ => None,
        };
        let Some(new_op) = flipped else {
            return false;
        };
        let base = IrExpr::BinOp {
            lhs: lhs.clone(),
            op: new_op,
            rhs: rhs.clone(),
        };
        *e = base;
        rewrite_expr(e);
        return true;
    }
    // ¬(¬x) → x (only for a duplicated-consistent inner Not)
    if *op != BinOpKind::Not || lhs.as_ref() != rhs.as_ref() {
        return false;
    }
    let base = rhs.as_ref().clone();
    *e = base;
    // catch deeper nesting (`!(!(!x))` → `!x` needs one more turn)
    rewrite_expr(e);
    true
}

fn rewrite_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Ext(e) => {
            let mut c = false;
            for s in e.children_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Output { value, .. } => rewrite_expr(value),
        IrStmt::WriteFile { path, content, .. } => rewrite_expr(path) | rewrite_expr(content),
        IrStmt::Assign { targets, expr, .. } => {
            let mut c = rewrite_expr(expr);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    c |= rewrite_expr(i);
                }
            }
            c
        }
        IrStmt::Declare { init, .. } => match init {
            Some(e) => rewrite_expr(e),
            None => false,
        },
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for e in elements.iter_mut() {
                c |= rewrite_expr(e);
            }
            c
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
            ..
        } => {
            let mut c = rewrite_expr(cond);
            for s in then.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= rewrite_expr(ec);
                for s in eb.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            for s in else_.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for ex in excepts.iter_mut() {
                if let Some(m) = ex.match_expr.as_mut() {
                    c |= rewrite_expr(m);
                }
                for s in ex.body.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            for s in else_body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for s in finally_body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
            ..
        } => {
            let mut c = rewrite_expr(cond);
            for v in [init, step, body] {
                for s in v.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = rewrite_expr(cond);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut c = rewrite_expr(cond);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = rewrite_expr(iter);
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } | IrStmt::SetChildError(expr) => {
            rewrite_expr(expr)
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => rewrite_expr(e),
        IrStmt::Return(None) | IrStmt::Exit(None) => false,
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            let mut c = rewrite_expr(cmd);
            for a in args.iter_mut().chain(redirects.iter_mut()) {
                c |= rewrite_expr(a);
            }
            for (_, e) in env.iter_mut() {
                c |= rewrite_expr(e);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stg in stages.iter_mut() {
                for s in stg.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = rewrite_expr(discriminant);
            for cl in clauses.iter_mut() {
                for s in cl.body.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for r in redirects.iter_mut() {
                c |= rewrite_expr(&mut r.target);
            }
            c
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            for (_, blk) in named_blocks.iter_mut() {
                for s in blk.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) | IrStmt::Block(v) => {
            let mut c = false;
            for s in v.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        IrStmt::Select { clauses } => {
            let mut c = false;
            for cl in clauses.iter_mut() {
                for e in cl.ch.iter_mut().chain(cl.value.iter_mut()) {
                    c |= rewrite_expr(e);
                }
                for s in cl.body.iter_mut() {
                    c |= rewrite_stmt(s);
                }
            }
            c
        }
        IrStmt::Asm { outputs, inputs, .. } => {
            let mut c = false;
            for (_, e) in outputs.iter_mut().chain(inputs.iter_mut()) {
                c |= rewrite_expr(e);
            }
            c
        }
        IrStmt::Expr(e) => rewrite_expr(e),
        IrStmt::Shaderize { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_stmt(s);
            }
            c
        }
        // no expression children
        IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Label(_)
        | IrStmt::Goto(_)
        | IrStmt::Require(_)
        | IrStmt::RawText(_) => false,
    }
}

fn rewrite_expr(e: &mut IrExpr) -> bool {
    let mut changed = fold_not_in(e);
    match e {
        IrExpr::Arith(_) => {} // no IrExpr children inside Arith text
        IrExpr::Index { key, .. } => changed |= rewrite_expr(key),
        IrExpr::BinOp { lhs, rhs, .. } => {
            changed |= rewrite_expr(lhs) | rewrite_expr(rhs);
        }
        IrExpr::Call { args, .. } | IrExpr::Array(args) => {
            for a in args.iter_mut() {
                changed |= rewrite_expr(a);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            changed |= rewrite_expr(obj);
            for a in args.iter_mut() {
                changed |= rewrite_expr(a);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            changed |= rewrite_expr(cond) | rewrite_expr(then) | rewrite_expr(else_);
        }
        IrExpr::DefinedOr { expr, default } => {
            changed |= rewrite_expr(expr) | rewrite_expr(default);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    changed |= rewrite_expr(x);
                }
            }
        }
        IrExpr::Capture { expr, .. } => changed |= rewrite_expr(expr),
        IrExpr::Arrow(v) | IrExpr::Lambda { body: v, .. } => {
            for s in v.iter_mut() {
                changed |= rewrite_stmt(s);
            }
        }
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            changed |= rewrite_expr(iter) | rewrite_expr(elem);
            if let Some(x) = cond.as_mut() {
                changed |= rewrite_expr(x);
            }
        }
        IrExpr::Splice(x) => changed |= rewrite_expr(x),
        IrExpr::Object(pairs) => {
            for (_, x) in pairs.iter_mut() {
                changed |= rewrite_expr(x);
            }
        }
        IrExpr::Ext(x) => {
            for x in x.children_mut() {
                changed |= rewrite_expr(x);
            }
        }
        // leaves
        IrExpr::Int(_)
        | IrExpr::Str(..)
        | IrExpr::Var(..)
        | IrExpr::Regex { .. }
        | IrExpr::Range { .. }
        | IrExpr::RawExpr(_)
        | IrExpr::Bool(_)
        | IrExpr::Json(_)
        | IrExpr::Ident(_) => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn s(x: &str) -> IrExpr {
        IrExpr::Str(x.to_string(), StrStyle::SingleQuoted)
    }

    /// The `not_wrap` shape: operand duplicated into lhs and rhs.
    fn not(e: IrExpr) -> IrExpr {
        IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs: Box::new(e.clone()),
            rhs: Box::new(e),
        }
    }

    #[test]
    fn folds_double_not() {
        let mut e = not(not(s("x")));
        assert!(fold_not(&mut e));
        assert_eq!(e, s("x"));
    }

    #[test]
    fn flips_negated_comparison() {
        // Not(Ne(v, "")) — the `-z X` lowering — → Eq(v, ""): rust then
        // renders `({v}).is_empty()` instead of `!(!({v}).is_empty())`.
        let ne = IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var("v".into(), None)),
            op: BinOpKind::Ne,
            rhs: Box::new(IrExpr::Str("".into(), StrStyle::DoubleQuoted)),
        };
        let mut e = not(ne.clone());
        assert!(fold_not(&mut e));
        assert!(matches!(
            &e,
            IrExpr::BinOp { op: BinOpKind::Eq, .. }
        ));
        // idempotent
        assert!(!fold_not(&mut e));
    }

    #[test]
    fn flips_ordering_comparisons() {
        let lt = IrExpr::BinOp {
            lhs: Box::new(IrExpr::Int(1)),
            op: BinOpKind::Lt,
            rhs: Box::new(IrExpr::Int(2)),
        };
        let mut e = not(lt);
        assert!(fold_not(&mut e));
        assert!(matches!(
            &e,
            IrExpr::BinOp { op: BinOpKind::Ge, .. }
        ));
    }

    #[test]
    fn leaves_and_or_alone() {
        let and = IrExpr::BinOp {
            lhs: Box::new(s("a")),
            op: BinOpKind::And,
            rhs: Box::new(s("b")),
        };
        let mut e = not(and.clone());
        assert!(!fold_not(&mut e));
        assert_eq!(e, not(and));
    }

    #[test]
    fn single_not_is_untouched() {
        let mut e = not(s("x"));
        assert!(!fold_not(&mut e));
        assert_eq!(e, not(s("x")));
    }

    #[test]
    fn unbalanced_inner_not_is_refused() {
        // inner Not with lhs != rhs — a construction bug this fold
        // must not paper over (see module doc).
        let inner = IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs: Box::new(s("a")),
            rhs: Box::new(s("b")),
        };
        let mut e = not(inner.clone());
        assert!(!fold_not(&mut e));
        assert_eq!(e, not(inner));
    }

    #[test]
    fn quadruple_not_unwinds_fully() {
        let mut e = not(not(not(not(s("x")))));
        assert!(fold_not(&mut e));
        assert_eq!(e, s("x"));
    }

    #[test]
    fn folds_inside_a_statement() {
        let mut stmts = vec![IrStmt::If {
            cond: not(not(s("c"))),
            then: vec![IrStmt::Continue],
            elsifs: vec![],
            else_: vec![],
        }];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::If { cond, .. } => assert_eq!(*cond, s("c")),
            other => panic!("expected If, got {other:?}"),
        }
        // idempotent: a second run reports no change
        assert!(!transform(&mut stmts));
    }

    /// The two registrations are SEPARATE transforms: `transform`
    /// (every target) folds only, `transform_flip` (rust-gated) flips
    /// only. This pin keeps a future edit from silently re-merging them
    /// — that re-merge is exactly the Perl Eq/Ne regression.
    #[test]
    fn fold_and_flip_are_wired_separately() {
        let ne = IrExpr::BinOp {
            lhs: Box::new(IrExpr::Var("v".into(), None)),
            op: BinOpKind::Ne,
            rhs: Box::new(IrExpr::Str("".into(), StrStyle::DoubleQuoted)),
        };
        // fold: ¬¬ only — a single negated comparison is untouched
        let mut stmts = vec![IrStmt::If {
            cond: not(ne.clone()),
            then: vec![],
            elsifs: vec![],
            else_: vec![],
        }];
        assert!(!transform(&mut stmts), "fold must not flip");
        match &stmts[0] {
            IrStmt::If { cond, .. } => assert_eq!(*cond, not(ne.clone())),
            other => panic!("expected If, got {other:?}"),
        }
        // flip: ¬(OP) only — a doubled negation is left for the fold
        let double = not(not(s("x")));
        let mut stmts2 = vec![IrStmt::If {
            cond: double.clone(),
            then: vec![],
            elsifs: vec![],
            else_: vec![],
        }];
        assert!(!transform_flip(&mut stmts2), "flip must not fold");
        match &stmts2[0] {
            IrStmt::If { cond, .. } => assert_eq!(*cond, double),
            other => panic!("expected If, got {other:?}"),
        }
        // and the flip does apply to its own rule
        let mut stmts3 = vec![IrStmt::If {
            cond: not(ne.clone()),
            then: vec![],
            elsifs: vec![],
            else_: vec![],
        }];
        assert!(transform_flip(&mut stmts3));
        match &stmts3[0] {
            IrStmt::If { cond, .. } => assert!(matches!(
                cond,
                IrExpr::BinOp { op: BinOpKind::Eq, .. }
            )),
            other => panic!("expected If, got {other:?}"),
        }
    }
}
