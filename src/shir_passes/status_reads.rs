//! Status-liveness analysis — does the program READ the shell status (`$?`)?
//!
//! A renderer that maintains a status register (`__sh_rc`, `__SH_RC`, …)
//! emits a store after nearly every statement so a later `$?` sees the
//! right value. When the program contains NO `$?` read, every one of those
//! stores is unobservable except the one that becomes the process exit
//! code — so a backend may drop the register entirely (and render the
//! final `exit` from the statically-known last statement instead).
//!
//! This is an ANALYSIS, not a transform: it answers a question about the
//! program and leaves it untouched. It is language-neutral and lives here
//! so every backend can share one definition of "status is observable"
//! (the Python renderer consumes it today).
//!
//! ## Method (conservative)
//! `true` if ANY of:
//!   - the program's debug text contains `$?` (the test/param operand text
//!     spellings carry it verbatim — this is the cheap total fallback),
//!   - a `getVar("?")` / `Var("?", …)` read exists (a `$?` expansion),
//!   - a status-named call exists (`lastStatus`, `lastexit`, `exitCode`, …).
//! `false` is only returned when none of those appear, which is the sound
//! direction: a missed read would keep the register, a false positive only
//! costs the optimization.

use crate::ir::{IrExpr, IrProgram, IrStmt};

/// Names that read the shell status register in the shared call vocabulary.
fn status_func(func: &str) -> bool {
    matches!(
        func,
        "lastStatus" | "lastexit" | "lastExit" | "status" | "exitCode"
    )
}

fn expr_reads_status(e: &IrExpr) -> bool {
    match e {
        IrExpr::Var(n, _) => n == "?",
        IrExpr::Str(s, _) => s.contains("$?"),
        IrExpr::RawExpr(s) => s.contains("$?"),
        IrExpr::Index { key, .. } => expr_reads_status(key),
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads_status(lhs) || expr_reads_status(rhs),
        IrExpr::Call { func, args } => {
            status_func(func)
                || (func == "getVar"
                    && matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "?"))
                || args.iter().any(expr_reads_status)
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads_status(obj) || args.iter().any(expr_reads_status)
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads_status(cond) || expr_reads_status(then) || expr_reads_status(else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_reads_status(expr) || expr_reads_status(default)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Lit(t) => t.contains("$?"),
            crate::ir::InterpPart::Expr(x) => expr_reads_status(x),
        }),
        IrExpr::Capture { expr, .. } => expr_reads_status(expr),
        IrExpr::Arrow(b) => stmts_read_status(b),
        IrExpr::Array(items) => items.iter().any(expr_reads_status),
        _ => false,
    }
}

fn stmts_read_status(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|s| match s {
        IrStmt::Output { value, .. } => expr_reads_status(value),
        IrStmt::WriteFile { path, content, .. } => {
            expr_reads_status(path) || expr_reads_status(content)
        }
        IrStmt::Assign { targets, expr, .. } => {
            targets.iter().any(|t| t.indices.iter().any(expr_reads_status))
                || expr_reads_status(expr)
        }
        IrStmt::Declare { init, .. } => init.as_ref().map(expr_reads_status).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => elements.iter().any(expr_reads_status),
        IrStmt::Expr(e) => expr_reads_status(e),
        IrStmt::If { cond, then, elsifs, else_ } => {
            expr_reads_status(cond)
                || stmts_read_status(then)
                || elsifs
                    .iter()
                    .any(|(c, b)| expr_reads_status(c) || stmts_read_status(b))
                || stmts_read_status(else_)
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
            expr_reads_status(cond) || stmts_read_status(body)
        }
        IrStmt::DoWhile { body, cond, .. } => {
            stmts_read_status(body) || expr_reads_status(cond)
        }
        IrStmt::ForInit { init, cond, step, body } => {
            stmts_read_status(init)
                || expr_reads_status(cond)
                || stmts_read_status(step)
                || stmts_read_status(body)
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            stmts_read_status(body)
                || excepts.iter().any(|e| stmts_read_status(&e.body))
                || stmts_read_status(else_body)
                || stmts_read_status(finally_body)
        }
        IrStmt::Exit(e) => e.as_ref().map(expr_reads_status).unwrap_or(false),
        IrStmt::Function { body, .. } => stmts_read_status(body),
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => stmts_read_status(b),
        IrStmt::Redirect { inner, .. } => stmts_read_status(inner),
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|st| stmts_read_status(st)),
        IrStmt::Case { discriminant, clauses } => {
            expr_reads_status(discriminant)
                || clauses.iter().any(|c| stmts_read_status(&c.body))
        }
        _ => false,
    })
}

/// True when the program observes the shell status (`$?` / lastStatus).
pub fn program_reads_status(prog: &IrProgram) -> bool {
    // Total fallback: the lowered operand text (`test("-z \"$?\"")`,
    // `param("or", "?", …)`) carries `$?` verbatim, and every IR node's
    // Debug form includes its own strings.
    if format!("{:?}", prog).contains("$?") {
        return true;
    }
    if stmts_read_status(&prog.stmts) {
        return true;
    }
    prog.subs.iter().any(|s| stmts_read_status(&s.body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrProgram, IrStmt, StrStyle};

    fn program(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            stmts,
            subs: vec![],
            imports: vec![],
            requires: vec![],
            var_types: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            stmt_lines: vec![],
            var_bash_env: vec![],
            var_lengths: vec![],
            var_storage: vec![],
        }
    }

    #[test]
    fn detects_test_operand_status() {
        let stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "test".into(),
            args: vec![IrExpr::Str("-z \"$?\"".into(), StrStyle::DoubleQuoted)],
        })];
        assert!(program_reads_status(&program(stmts)));
    }

    #[test]
    fn detects_getvar_status() {
        let stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "getVar".into(),
            args: vec![IrExpr::Str("?".into(), StrStyle::DoubleQuoted)],
        })];
        assert!(program_reads_status(&program(stmts)));
    }

    #[test]
    fn status_free_program_is_false() {
        let stmts = vec![IrStmt::Output {
            value: IrExpr::Str("hi".into(), StrStyle::DoubleQuoted),
            newline: true,
            target: None,
        }];
        assert!(!program_reads_status(&program(stmts)));
    }
}