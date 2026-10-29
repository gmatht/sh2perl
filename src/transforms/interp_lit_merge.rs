//! interp-lit-merge — canonicalise the parts of an `Interpolate`:
//!   * an `Expr` part whose value is a frozen literal string (`IrExpr::Str(text,
//!     DoubleQuoted | SingleQuoted)`) becomes a `Lit` part carrying that text;
//!   * adjacent `Lit` parts merge into one `Lit`.
//!
//! name: interp-lit-merge
//! prereqs: [const-capture-fold, text-capture-fold, copy-propagation — those
//!   are what freeze a substituted word into a `Str` and leave it sitting in
//!   an `Expr` part; registered after them, before decl-value-rejoin, which
//!   then gets more single-part words to canonicalise]
//! invariant: `Interpolate([…, Lit(a), Lit(b), …])` → `Interpolate([…, Lit(a++
//!   b), …])`, and `Interpolate([…, Expr(Str(t, DoubleQuoted|SingleQuoted)),
//!   …])` → `Interpolate([…, Lit(t), …])`. NOTHING else: `Str` with style
//!   `Command` (an unrendered substitution), `Heredoc` (Perl still
//!   interpolates `$`/`@` in it) and `Raw` are refused, as is any expression
//!   that is not a frozen literal — refuse > guess.
//! scope: offered to all backends. Every renderer prints an `Interpolate` by
//!   appending its parts in order, so two adjacent literals and one literal
//!   spelled as a folded expression produce the same bytes either way; the
//!   fold only removes the SLOT BOUNDARY between them.
//! updates: none (first offer)
//!
//! ## Need
//! Measured on the 650-example corpus (2026-10-27): 58 `Interpolate`s carry
//! an `Expr` part that is already a frozen literal — `const-capture-fold`
//! folds `echo "Echo result: $x"`'s capture into a `Str` but leaves it an
//! `Expr` part — and 10 more hold literally adjacent `Lit` pairs. Every
//! renderer then emits a format string with one slot per part for text that
//! is a single constant:
//!
//! ```text
//! print("{s}{s}\n", .{"Echo result: ", "Hello from backticks"})
//! ```
//!
//! instead of the one-slot print a constant deserves. Measured yield:
//! 63 of those 58 `Interpolate`s collapse to a single `Lit` part, and with
//! `echo_fold` (which only folds an echo whose word is "a bare literal, or a
//! quoted string whose only part is a literal") this is what lets a whole
//! constant `echo` reach the native `Output` lowering.
//!
//! ## Soundness
//! The concatenation order is preserved and no text is added or dropped:
//! a renderer that appends part[0] + part[1] + … sees exactly the same byte
//! sequence as one with the merged part. The conversion of an `Expr` part is
//! the load-bearing half, so it is restricted to the two plain-quoting
//! styles whose rendered text IS the value; the other three styles carry
//! semantics the renderer must still see (see the invariant above).
//!
//! ## Placement
//! Registered after the folds that freeze the operands (copy-propagation,
//! const-capture-fold) and before `decl-value-rejoin` — that pass turns a
//! single-`Lit` `Interpolate` into a plain `Str`, so merging first lets it
//! canonicalise more words. Pure structural rewrite: no verdict statics,
//! idempotent by construction, shell-agnostic (any source language feeding
//! the shared IR joins its literal parts the same way).

use crate::ir::{InterpPart, IrExpr, IrStmt, StrStyle};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt_pass(st);
    }
    c
}

/// Rewrite one `Interpolate`'s parts: fold frozen-literal `Expr` parts into
/// `Lit`, then merge adjacent `Lit`s. Returns whether anything changed.
fn merge_parts(parts: &mut Vec<InterpPart>) -> bool {
    let mut changed = false;
    // 1. Expr(Str, plain-quoted) → Lit
    for p in parts.iter_mut() {
        if let InterpPart::Expr(x) = p {
            if let IrExpr::Str(text, style) = &**x {
                if matches!(style, StrStyle::DoubleQuoted | StrStyle::SingleQuoted) {
                    *p = InterpPart::Lit(text.clone());
                    changed = true;
                }
            }
        }
    }
    if changed {
        // 2. adjacent lits merge — rebuild, since a fold in step 1 can mint
        //    a pair that was not adjacent before.
        let mut out: Vec<InterpPart> = Vec::with_capacity(parts.len());
        for p in parts.drain(..) {
            match (out.last_mut(), &p) {
                (Some(InterpPart::Lit(prev)), InterpPart::Lit(cur)) => {
                    prev.push_str(cur);
                }
                _ => out.push(p),
            }
        }
        *parts = out;
    } else {
        // 2'. still merge pre-existing adjacent lits (no fold happened)
        let mut out: Vec<InterpPart> = Vec::with_capacity(parts.len());
        for p in parts.drain(..) {
            match (out.last_mut(), &p) {
                (Some(InterpPart::Lit(prev)), InterpPart::Lit(cur)) => {
                    prev.push_str(cur);
                    changed = true;
                }
                _ => out.push(p),
            }
        }
        *parts = out;
    }
    changed
}

fn stmt_pass(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Ext(e) => {
            let mut c = false;
            for s in e.children_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::Output { value, .. } => expr_pass(value),
        IrStmt::WriteFile { path, content, .. } => expr_pass(path) | expr_pass(content),
        IrStmt::Assign { targets, expr, .. } => {
            let mut c = expr_pass(expr);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    c |= expr_pass(i);
                }
            }
            c
        }
        IrStmt::Declare { init, .. } => init.as_mut().map(expr_pass).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for e in elements.iter_mut() {
                c |= expr_pass(e);
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
            let mut c = expr_pass(cond);
            for s in then.iter_mut() {
                c |= stmt_pass(s);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr_pass(ec);
                for s in eb.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            for s in else_.iter_mut() {
                c |= stmt_pass(s);
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
                c |= stmt_pass(s);
            }
            for ex in excepts.iter_mut() {
                if let Some(m) = ex.match_expr.as_mut() {
                    c |= expr_pass(m);
                }
                for s in ex.body.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            for s in else_body.iter_mut() {
                c |= stmt_pass(s);
            }
            for s in finally_body.iter_mut() {
                c |= stmt_pass(s);
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
            let mut c = expr_pass(cond);
            for v in [init, step, body] {
                for s in v.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = expr_pass(cond);
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut c = expr_pass(cond);
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = expr_pass(iter);
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } | IrStmt::SetChildError(expr) => {
            expr_pass(expr)
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => expr_pass(e),
        IrStmt::Return(None) | IrStmt::Exit(None) => false,
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            let mut c = expr_pass(cmd);
            for a in args.iter_mut().chain(redirects.iter_mut()) {
                c |= expr_pass(a);
            }
            for (_, e) in env.iter_mut() {
                c |= expr_pass(e);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stg in stages.iter_mut() {
                for s in stg.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            c
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = expr_pass(discriminant);
            for cl in clauses.iter_mut() {
                for s in cl.body.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            c
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= stmt_pass(s);
            }
            for r in redirects.iter_mut() {
                c |= expr_pass(&mut r.target);
            }
            c
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            for (_, blk) in named_blocks.iter_mut() {
                for s in blk.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            c
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) | IrStmt::Block(v) => {
            let mut c = false;
            for s in v.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::Select { clauses } => {
            let mut c = false;
            for cl in clauses.iter_mut() {
                for e in cl.ch.iter_mut().chain(cl.value.iter_mut()) {
                    c |= expr_pass(e);
                }
                for s in cl.body.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            c
        }
        IrStmt::Asm { outputs, inputs, .. } => {
            let mut c = false;
            for (_, e) in outputs.iter_mut().chain(inputs.iter_mut()) {
                c |= expr_pass(e);
            }
            c
        }
        IrStmt::Expr(e) => expr_pass(e),
        IrStmt::Shaderize { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s);
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

fn expr_pass(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Interpolate(parts) => merge_parts(parts),
        IrExpr::Arith(_) => false, // no IrExpr children inside Arith text
        IrExpr::Index { key, .. } => expr_pass(key),
        IrExpr::BinOp { lhs, rhs, .. } => expr_pass(lhs) | expr_pass(rhs),
        IrExpr::Call { args, .. } | IrExpr::Array(args) => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr_pass(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = expr_pass(obj);
            for a in args.iter_mut() {
                c |= expr_pass(a);
            }
            c
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_pass(cond) | expr_pass(then) | expr_pass(else_)
        }
        IrExpr::DefinedOr { expr, default } => expr_pass(expr) | expr_pass(default),
        IrExpr::Capture { expr, .. } => expr_pass(expr),
        IrExpr::Arrow(v) | IrExpr::Lambda { body: v, .. } => {
            let mut c = false;
            for s in v.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            let mut c = expr_pass(iter) | expr_pass(elem);
            if let Some(x) = cond.as_mut() {
                c |= expr_pass(x);
            }
            c
        }
        IrExpr::Splice(x) => expr_pass(x),
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, x) in pairs.iter_mut() {
                c |= expr_pass(x);
            }
            c
        }
        IrExpr::Ext(x) => {
            let mut c = false;
            for x in x.children_mut() {
                c |= expr_pass(x);
            }
            c
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
        | IrExpr::Ident(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn lit(s: &str) -> InterpPart {
        InterpPart::Lit(s.to_string())
    }
    fn expr(e: IrExpr) -> InterpPart {
        InterpPart::Expr(Box::new(e))
    }
    fn dq(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn merged(parts: Vec<InterpPart>) -> Vec<InterpPart> {
        let mut p = parts;
        merge_parts(&mut p);
        p
    }

    #[test]
    fn folds_frozen_literal_expr_part_and_merges() {
        // the measured corpus shape: "Echo result: " + Expr(Str)
        let out = merged(vec![lit("Echo result: "), expr(dq("Hello from backticks"))]);
        assert_eq!(out, vec![lit("Echo result: Hello from backticks")]);
    }

    #[test]
    fn merges_preexisting_adjacent_lits() {
        let out = merged(vec![lit("\\.txt"), lit("$")]);
        assert_eq!(out, vec![lit("\\.txt$")]);
    }

    #[test]
    fn middle_literal_between_expr_parts_merges_after_the_fold() {
        let out = merged(vec![
            expr(dq("a")),
            lit(" "),
            expr(dq("b")),
        ]);
        assert_eq!(out, vec![lit("a b")]);
    }

    #[test]
    fn refuses_command_style() {
        // an unrendered substitution — its text is not the value
        let p = vec![lit("x="), expr(IrExpr::Str("date".into(), StrStyle::Command))];
        assert_eq!(merged(p.clone()), p);
    }

    #[test]
    fn refuses_heredoc_and_raw_styles() {
        for style in [StrStyle::Heredoc, StrStyle::Raw] {
            let p = vec![lit("x="), expr(IrExpr::Str("$v".into(), style))];
            assert_eq!(merged(p.clone()), p);
        }
    }

    #[test]
    fn refuses_non_literal_expr_and_is_idempotent() {
        let p = vec![lit("a"), expr(IrExpr::Var("v".into(), None))];
        assert_eq!(merged(p.clone()), p);

        let mut once = vec![lit("a "), expr(dq("b"))];
        assert!(merge_parts(&mut once));
        assert_eq!(once, vec![lit("a b")]);
        assert!(!merge_parts(&mut once));
    }

    #[test]
    fn walks_statements_to_the_interpolate() {
        // the fold must reach an Interpolate nested in an expr inside an if
        let interp = IrExpr::Interpolate(vec![lit("n="), expr(dq("1"))]);
        let mut stmts = vec![IrStmt::If {
            cond: IrExpr::Bool(true),
            then: vec![IrStmt::Expr(interp)],
            elsifs: vec![],
            else_: vec![],
        }];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::If { then, .. } => match &then[0] {
                IrStmt::Expr(e) => assert_eq!(*e, IrExpr::Interpolate(vec![lit("n=1")])),
                other => panic!("expected Expr, got {other:?}"),
            },
            other => panic!("expected If, got {other:?}"),
        }
        assert!(!transform(&mut stmts)); // idempotent
    }
}
