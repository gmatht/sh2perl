//! interp-fold — canonicalise single-part `Interpolate` nodes.
//!
//! ## Need
//! The parser wraps a quoted word that carries an expansion in
//! `Interpolate`, and sometimes a part list that never needed wrapping
//! survives all the way to the backends: `Interpolate [Lit("text")]` —
//! a word with no expansion in it at all. Every backend then renders a
//! concat/format wrapper around what is provably ONE literal (the rust
//! `format!` machinery, JS string concat, C snprintf-class helpers, …),
//! and every shIR matcher that keys on `Str` (const-word tests,
//! static-storage verdicts, the typeset-attrs literal fold, the A1
//! byte contract) has to spell the all-literal Interpolate case out a
//! second time — several already do (`literal_text`,
//! `static_word_text`, echo-fold's const-word check each re-implement
//! "a bare Str, or an Interpolate whose parts are ALL literal").
//!
//! ## Scope — the sound rewrite
//!   * `Interpolate([Lit(s)])` → `Str(s, DoubleQuoted)`
//!
//! Nothing else. A single-`Expr` part (`Interpolate([Expr(e)])`) is
//! deliberately NOT folded: in WORD-LIST positions a quoted word and a
//! bare expansion differ (`for x in "$v"` is one word, `for x in $v`
//! splits on IFS), and proving the position non-word would require the
//! transform to model every call-argument list — REFUSE > GUESS.
//! Multi-part lists stay too: their concat is real.
//!
//! `DoubleQuoted` is the faithful style tag: a literal inside an
//! Interpolate came from a quoted context, so a renderer that re-quotes
//! from the tag must not word-split or glob it.
//!
//! ## The refusal that keeps shape-keyed consumers working
//! The rust `local`/`declare` value join reads `name=` + the NEXT word
//! and decides whether that next word IS the value by its SHAPE: the
//! parser lexes `local msg="hello"` as `msg=` + an Interpolate (quoted
//! piece) and `local a= b` as `name=` + a bare `Str` — the quote mark
//! is what distinguishes "the value" from "the next declaration". A
//! literal Interpolate sitting directly AFTER a `…=`-ended word is
//! therefore load-bearing, so the fold REFUSES to canonicalise it
//! (measured: 081_nested_functions printed an empty `msg` when the
//! fold erased the distinction; REFUSE > GUESS).
//!
//! ## Placement
//! Registered in `transforms.rs` (`interp-fold`, the shared
//! `ast_to_ir` channel), early so later folds see the canonical `Str`.

use crate::ir::{InterpPart, IrExpr, IrProgram, IrStmt, StrStyle};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    stmts_list(stmts)
}

#[allow(dead_code)]
pub fn run_program(prog: &mut IrProgram) -> bool {
    let mut c = stmts_list(&mut prog.stmts);
    for sub in &mut prog.subs {
        c |= stmts_list(&mut sub.body);
    }
    c
}

fn stmts_list(stmts: &mut [IrStmt]) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    c
}

fn stmt(st: &mut IrStmt) -> bool {
    let mut c = false;
    match st {
        IrStmt::Output { value, .. } => c |= expr(value),
        IrStmt::WriteFile { path, content, .. } => {
            c |= expr(path);
            c |= expr(content);
        }
        IrStmt::Assign { targets, expr: e, .. } => {
            c |= expr(e);
            for t in targets {
                for i in &mut t.indices {
                    c |= expr(i);
                }
            }
        }
        IrStmt::Declare { init: Some(e), .. } => c |= expr(e),
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                c |= expr(e);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            c |= expr(cond);
            c |= stmts_list(then);
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr(ec);
                c |= stmts_list(eb);
            }
            c |= stmts_list(else_);
        }
        IrStmt::For { iter, body, .. } => {
            c |= expr(iter);
            c |= stmts_list(body);
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            c |= stmts_list(init);
            c |= expr(cond);
            c |= stmts_list(step);
            c |= stmts_list(body);
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            c |= expr(cond);
            c |= stmts_list(body);
        }
        IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => c |= expr(e),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
            c |= expr(e)
        }
        IrStmt::Expr(e) => c |= expr(e),
        IrStmt::Function { body, .. } | IrStmt::Block(body) => c |= stmts_list(body),
        IrStmt::Redirect { inner, .. } => c |= stmts_list(inner),
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            c |= expr(cmd);
            for a in args.iter_mut().chain(redirects.iter_mut()) {
                c |= expr(a);
            }
            for (_, v) in env.iter_mut() {
                c |= expr(v);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages.iter_mut() {
                c |= stmts_list(s);
            }
        }
        // Try / Ext / Case and friends: no folding (safe direction —
        // an unfolded node is the status quo, never a wrong rewrite).
        _ => {}
    }
    c
}

fn expr(e: &mut IrExpr) -> bool {
    fold_in_value_position(e, false)
}

/// `prev_was_assign_head`: the previous WORD in this list ended in `=`
/// (`local msg=` + the piece that follows) — the split-assignment shape
/// where a consumer may be reading the word's Str-vs-Interpolate shape
/// to tell the VALUE from the NEXT declaration.
fn fold_in_value_position(e: &mut IrExpr, prev_was_assign_head: bool) -> bool {
    let mut c = false;
    match e {
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= expr(x);
                }
            }
            // single literal part → the plain Str it always was — unless
            // the shape itself is the signal (see module docs)
            if prev_was_assign_head {
                return c;
            }
            let lit = match parts.as_slice() {
                [InterpPart::Lit(s)] => Some(s.clone()),
                _ => None,
            };
            if let Some(s) = lit {
                *e = IrExpr::Str(s, StrStyle::DoubleQuoted);
                return true;
            }
            c
        }
        IrExpr::Array(items) => {
            let mut prev_head = false;
            for i in items.iter_mut() {
                c |= fold_in_value_position(i, prev_head);
                // this word IS a `name=` head (value lexed separately)
                prev_head = match i {
                    IrExpr::Str(s, _) => s.ends_with('='),
                    _ => false,
                };
            }
            c
        }
        IrExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            c |= expr(obj);
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            c |= expr(iter);
            c |= expr(elem);
            if let Some(cond) = cond {
                c |= expr(cond);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            c |= expr(lhs);
            c |= expr(rhs);
            c
        }
        IrExpr::Index { key, .. } => expr(key),
        IrExpr::Ternary { cond, then, else_ } => {
            c |= expr(cond);
            c |= expr(then);
            c |= expr(else_);
            c
        }
        IrExpr::DefinedOr { expr: x, default } => {
            c |= expr(x);
            c |= expr(default);
            c
        }
        IrExpr::Capture { expr: x, .. } => expr(x),
        IrExpr::Arrow(body) | IrExpr::Lambda { body, .. } => stmts_list(body),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::AssignTarget;

    fn assign_str(expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "x".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn lit_interp(s: &str) -> IrExpr {
        IrExpr::Interpolate(vec![InterpPart::Lit(s.to_string())])
    }

    #[test]
    fn single_literal_interpolate_folds_to_str() {
        let mut stmts = vec![assign_str(lit_interp("hello"))];
        assert!(transform(&mut stmts));
        let IrStmt::Assign { expr, .. } = &stmts[0] else { panic!() };
        assert_eq!(*expr, IrExpr::Str("hello".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn empty_literal_interpolate_folds_to_empty_str() {
        let mut stmts = vec![assign_str(lit_interp(""))];
        assert!(transform(&mut stmts));
        let IrStmt::Assign { expr, .. } = &stmts[0] else { panic!() };
        assert_eq!(*expr, IrExpr::Str(String::new(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn multi_part_interpolate_is_left_alone() {
        let mut stmts = vec![assign_str(IrExpr::Interpolate(vec![
            InterpPart::Lit("a".to_string()),
            InterpPart::Expr(Box::new(IrExpr::Var("v".to_string(), None))),
            InterpPart::Lit("b".to_string()),
        ]))];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn single_expr_interpolate_is_refused() {
        // a quoted word and a bare expansion differ in word-list
        // positions — folding this would be a semantics change
        let mut stmts = vec![assign_str(IrExpr::Interpolate(vec![InterpPart::Expr(
            Box::new(IrExpr::Var("v".to_string(), None)),
        )]))];
        assert!(!transform(&mut stmts));
    }

    #[test]
    fn a_literal_after_an_assign_head_stays_interpolated() {
        // `local msg="hello"` lexes as `msg=` + the quoted piece; the
        // rust local-join reads that piece's Str-vs-Interpolate SHAPE to
        // tell the value from the next declaration (`local a= b`), so
        // folding it here would silently empty the init (081).
        let mut stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("local".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![
                    IrExpr::Str("msg=".to_string(), StrStyle::DoubleQuoted),
                    lit_interp("hello"),
                ]),
            ],
        })];
        assert!(!transform(&mut stmts));
        // …and the shape survives intact
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &stmts[0] else { panic!() };
        assert!(matches!(&args[1], IrExpr::Array(w) if matches!(&w[1], IrExpr::Interpolate(_))));
    }

    #[test]
    fn a_literal_after_a_plain_word_still_folds() {
        let mut stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![
                    IrExpr::Str("plain".to_string(), StrStyle::DoubleQuoted),
                    lit_interp("second"),
                ]),
            ],
        })];
        assert!(transform(&mut stmts));
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &stmts[0] else { panic!() };
        assert!(matches!(&args[1], IrExpr::Array(w) if matches!(&w[1], IrExpr::Str(s, _) if s == "second")));
    }

    #[test]
    fn fold_reaches_nested_call_args_and_interior_exprs() {
        let mut stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![lit_interp("word")]),
            ],
        })];
        assert!(transform(&mut stmts));
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &stmts[0] else { panic!() };
        assert_eq!(args[1], IrExpr::Array(vec![IrExpr::Str("word".to_string(), StrStyle::DoubleQuoted)]));
    }
}
