//! FrontendCaptureFold — fold a command substitution whose body is a
//! constant `echo` into the literal it prints, for the A1 the frontends
//! emit.
//!
//! The shell pipeline folds this in `ast_to_ir` (`const_capture_fold`), so
//! a bash script's `v=$(echo hello)` never reaches a renderer as a
//! capture. A FRONTEND's A1 reaches the renderers unfolded: py-sh-go
//! spells `subprocess.run([...], capture_output=True).stdout` (and `$(…)`)
//! as `Call { func: "capture", args: [Arrow([…])] }`, and the shell
//! assignment's `Expr(Call{exec|builtin echo})` body is not an
//! `IrStmt::Output`, so nothing folds it.
//!
//! The renderers then bind the captured name in GLOBAL storage (a
//! `thread_local`, an environment read) and the value round-trips through
//! the runtime for a constant known at compile time:
//!
//! ```text
//! out = $(echo hello)      __SHV_out.with(|v| *v.borrow_mut() = "hello".to_string());
//! echo "$out"              print!("hello\n");          ← the fold, once
//! ```
//!
//! This pass is the narrow, target-neutral half of the existing
//! transform: it runs the SAME verdict (`const_capture_fold::fold_const_echo`
//! — one literal `echo`/`printf` body, no flags, no variables, no
//! redirects) and replaces the capture with the literal string. Every
//! other capture — a variable word, a real command, several statements, a
//! redirect, a `-n`/`-e` flag — is left exactly as it was.
//!
//! The fold is a pure rewrite of the IR, so no backend sees a new node:
//! the literal is the same `Str` the shell pipeline already produces for
//! the same source.

use crate::ir::{IrExpr, IrProgram, IrStmt, StrStyle};

use super::{PassContext, Transform};

pub struct FrontendCaptureFold;

fn fold_expr(e: &mut IrExpr) -> bool {
    let replacement = match e {
        IrExpr::Capture { expr, native } if !*native => {
            crate::transforms::const_capture_fold::fold_const_echo(expr)
        }
        IrExpr::Call { func, args } if func == "capture" => {
            args.first().and_then(crate::transforms::const_capture_fold::fold_const_echo)
        }
        _ => None,
    };
    if let Some(text) = replacement {
        *e = IrExpr::Str(text, StrStyle::DoubleQuoted);
        return true;
    }
    fold_children(e)
}

/// Descend into an expression's sub-expressions (the containers the
/// frontends put around a capture). Statements inside an `Arrow`/`Lambda`
/// are walked by [`fold_stmts`] at their own position.
fn fold_children(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Array(items) => items.iter_mut().map(fold_expr).fold(false, |a, b| a | b),
        IrExpr::Object(fields) => fields.iter_mut().map(|(_, v)| fold_expr(v)).fold(false, |a, b| a | b),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    c |= fold_expr(x);
                }
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => fold_expr(lhs) | fold_expr(rhs),
        IrExpr::Ternary { cond, then, else_, .. } => {
            fold_expr(cond) | fold_expr(then) | fold_expr(else_)
        }
        IrExpr::DefinedOr { expr, default, .. } => fold_expr(expr) | fold_expr(default),
        IrExpr::Index { key, .. } => fold_expr(key),
        IrExpr::MethodCall { obj, args, .. } => {
            fold_expr(obj) | args.iter_mut().map(fold_expr).fold(false, |a, b| a | b)
        }
        IrExpr::Call { args, .. } => args.iter_mut().map(fold_expr).fold(false, |a, b| a | b),
        IrExpr::Arrow(body) => fold_stmts(body),
        IrExpr::Splice(inner) => fold_expr(inner),
        _ => false,
    }
}

fn fold_stmts(stmts: &mut [IrStmt]) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= match st {
            IrStmt::Assign { expr, .. }
            | IrStmt::Output { value: expr, .. }
            | IrStmt::Expr(expr)
            | IrStmt::Die { expr, .. }
            | IrStmt::Warn { expr, .. }
            | IrStmt::SetChildError(expr)
            | IrStmt::Return(Some(expr))
            | IrStmt::Exit(Some(expr)) => fold_expr(expr),
            IrStmt::Declare { init: Some(init), .. } => fold_expr(init),
            IrStmt::DeclareArray { elements, .. } => {
                elements.iter_mut().map(fold_expr).fold(false, |a, b| a | b)
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                let mut c = fold_expr(cond);
                c |= fold_stmts(then);
                for (ec, eb) in elsifs {
                    c |= fold_expr(ec);
                    c |= fold_stmts(eb);
                }
                c | fold_stmts(else_)
            }
            IrStmt::While { cond, body } => fold_expr(cond) | fold_stmts(body),
            IrStmt::DoWhile { body, cond, .. } => fold_stmts(body) | fold_expr(cond),
            IrStmt::For { iter, body, .. } => fold_expr(iter) | fold_stmts(body),
            IrStmt::ForInit { init, cond, step, body } => {
                let mut c = fold_stmts(init);
                c |= fold_expr(cond);
                c |= fold_stmts(step);
                c | fold_stmts(body)
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => fold_stmts(b),
            IrStmt::Function { body, .. } => fold_stmts(body),
            IrStmt::Case { discriminant, clauses } => {
                let mut c = fold_expr(discriminant);
                for cl in clauses {
                    c |= fold_stmts(&mut cl.body);
                }
                c
            }
            IrStmt::WriteFile { path, content, .. } => fold_expr(path) | fold_expr(content),
            _ => false,
        };
    }
    c
}

impl Transform for FrontendCaptureFold {
    fn name(&self) -> &'static str {
        "frontend_capture_fold"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        fold_stmts(&mut prog.stmts);
        for sub in &mut prog.subs {
            fold_stmts(&mut sub.body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, IrExpr, IrStmt, IrType, StrStyle};

    fn str_(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn program(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
            subs: vec![],
            var_types: vec![("v".to_string(), IrType::Str)],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
        }
    }

    fn assign_capture(body: IrStmt) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget { var: "v".to_string(), sigil: None, indices: vec![] }],
            expr: IrExpr::Call {
                func: "capture".to_string(),
                args: vec![IrExpr::Arrow(vec![body])],
            },
            asm: None,
        }
    }

    fn const_echo() -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![str_("echo"), IrExpr::Array(vec![str_("hello")])],
        })
    }

    #[test]
    fn a_constant_echo_capture_folds_to_its_output() {
        let mut p = program(vec![assign_capture(const_echo())]);
        FrontendCaptureFold.run(&mut p, &PassContext::default());
        assert_eq!(
            p.stmts[0],
            IrStmt::Assign {
                targets: vec![AssignTarget { var: "v".to_string(), sigil: None, indices: vec![] }],
                expr: str_("hello"),
                asm: None,
            }
        );
    }

    #[test]
    fn a_variable_word_keeps_the_capture() {
        let body = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                str_("echo"),
                IrExpr::Array(vec![IrExpr::Call {
                    func: "getVar".to_string(),
                    args: vec![str_("x")],
                }]),
            ],
        });
        let mut p = program(vec![assign_capture(body)]);
        let before = format!("{:?}", p.stmts);
        FrontendCaptureFold.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_real_command_keeps_the_capture() {
        let body = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![str_("date"), IrExpr::Array(vec![])],
        });
        let mut p = program(vec![assign_capture(body)]);
        let before = format!("{:?}", p.stmts);
        FrontendCaptureFold.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn an_echo_with_a_flag_keeps_the_capture() {
        // `-n` changes the output the shell would produce.
        let body = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![str_("echo"), IrExpr::Array(vec![str_("-n"), str_("x")])],
        });
        let mut p = program(vec![assign_capture(body)]);
        let before = format!("{:?}", p.stmts);
        FrontendCaptureFold.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }
}
