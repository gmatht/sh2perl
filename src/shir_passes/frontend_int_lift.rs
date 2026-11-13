//! FrontendIntLift — give a frontend-emitted numeric literal its INT home.
//!
//! The imperative frontends (py-sh-go first) spell a source numeric literal
//! as the string `"1"` and record the domain only in the A2 verdict
//! `prog.var_types[name] = Int`. Every renderer reads the SHELL convention
//! instead — an `Int` node, an `arith` call — so a name holding a python
//! integer stays an untyped string value in the shIR, and:
//!
//! * `x = 1 + 2; print(x)` emits `let _ = 1i64 + 2i64; println!("{}", x)`
//!   — an assignment whose value is discarded plus a read of a name nothing
//!   declared (E0425 in Rust, a dead `""` in every other backend);
//! * `x = 1; x += 1` folds to `+= 1` on a value nobody can add to.
//!
//! This pass rewrites the *numeric spelling* to the shIR numeric node, so
//! the existing (well-tested) numeric machinery does the rest: an `Int`-typed
//! name's single definition `Str("42")` becomes `Int(42)`, and an `Arith`
//! binary whose operands are all numeric literals/`Int` names becomes an
//! `Int` literal. The value is exact and static, so nothing about the
//! program's observable behaviour changes — only which node spells it.
//!
//! # Scope (refuse > guess)
//!
//! Only `prog.stmts` (the main body) and only when
//! * the name is typed `Int` by the A2 analysis,
//! * that name is assigned at most once in the whole program (a second
//!   write means the value flows through the renderer's own numeric
//!   lowering, which this pass must not pre-empt), and
//! * the definition is a plain top-level `Assign` of
//!   * a `Str` that parses as an `i64` (no sign tricks, no float, no
//!     `0x`/underscore spelling), or
//!   * an `Arith` tree whose every leaf is a literal or an `Int`-typed
//!     name, and whose every operator is `+ - *`, evaluated exactly
//!     (overflow → refuse: the source language's arithmetic is unbounded).
//!
//! Strings that merely CONTAIN digits (`"200"` concatenated, a date, a
//! padded id) are never touched unless the A2 verdict says the name is an
//! integer, and a name used in any string position keeps the pass away.

use crate::ir::{ArithAst, IrProgram, IrStmt};

use super::{PassContext, Transform};

pub struct FrontendIntLift;

fn count_writes(prog: &IrProgram) -> std::collections::HashMap<String, usize> {
    use std::collections::HashMap;
    let mut out: HashMap<String, usize> = HashMap::new();
    fn bump(out: &mut std::collections::HashMap<String, usize>, n: &str) {
        *out.entry(n.to_string()).or_insert(0) += 1;
    }
    fn stmt(s: &IrStmt, out: &mut std::collections::HashMap<String, usize>) {
        match s {
            IrStmt::Assign { targets, .. } => {
                for t in targets {
                    bump(out, &t.var);
                }
            }
            IrStmt::Declare { vars, .. } => {
                for v in vars {
                    bump(out, &v.name);
                }
            }
            IrStmt::DeclareArray { var, .. } => bump(out, var),
            IrStmt::If { then, elsifs, else_, .. } => {
                for s in then {
                    stmt(s, out);
                }
                for (_, b) in elsifs {
                    for s in b {
                        stmt(s, out);
                    }
                }
                for s in else_ {
                    stmt(s, out);
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::ForInit { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Function { body, .. } => {
                for s in body {
                    stmt(s, out);
                }
            }
            IrStmt::Try { body, excepts, else_body, finally_body } => {
                for s in body.iter().chain(else_body.iter()).chain(finally_body.iter()) {
                    stmt(s, out);
                }
                for e in excepts {
                    for s in &e.body {
                        stmt(s, out);
                    }
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for s in &c.body {
                        stmt(s, out);
                    }
                }
            }
            _ => {}
        }
    }
    for s in &prog.stmts {
        stmt(s, &mut out);
    }
    for sub in &prog.subs {
        for s in &sub.body {
            stmt(s, &mut out);
        }
    }
    out
}

/// Does this arith tree mention anything that is not a literal or an
/// INT-typed name?
fn arith_is_int(ast: &ArithAst, ints: &std::collections::HashSet<String>) -> bool {
    match ast {
        ArithAst::Num(_) => true,
        ArithAst::Var(n) | ArithAst::Ident(n) => ints.contains(n),
        ArithAst::Bin { op, lhs, rhs } => {
            matches!(op.as_str(), "+" | "-" | "*")
                && arith_is_int(lhs, ints)
                && arith_is_int(rhs, ints)
        }
        _ => false,
    }
}

/// Evaluate an arith tree already proven `arith_is_int`. `None` on
/// overflow (the value is then left to the renderer's own arithmetic).
fn arith_eval(ast: &ArithAst) -> Option<i64> {
    match ast {
        ArithAst::Num(n) => Some(*n),
        ArithAst::Bin { op, lhs, rhs } => {
            let l = arith_eval(lhs)?;
            let r = arith_eval(rhs)?;
            match op.as_str() {
                "+" => l.checked_add(r),
                "-" => l.checked_sub(r),
                "*" => l.checked_mul(r),
                _ => None,
            }
        }
        _ => None,
    }
}

impl Transform for FrontendIntLift {
    fn name(&self) -> &'static str {
        "frontend_int_lift"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let ints = super::frontend_types::int_names(prog);
        if ints.is_empty() {
            return;
        }
        let writes = count_writes(prog);
        for st in prog.stmts.iter_mut() {
            let (name, value) = match st {
                IrStmt::Assign { targets, expr, .. }
                    if targets.len() == 1 && targets[0].indices.is_empty() =>
                {
                    (targets[0].var.clone(), expr)
                }
                IrStmt::Declare { vars, init: Some(init), .. } if vars.len() == 1 => {
                    (vars[0].name.clone(), init)
                }
                _ => continue,
            };
            if !ints.contains(&name) || writes.get(&name).copied().unwrap_or(0) != 1 {
                continue;
            }
            let lifted = match &*value {
                crate::ir::IrExpr::Str(text, _) => {
                    super::frontend_types::int_literal(text).map(crate::ir::IrExpr::Int)
                }
                crate::ir::IrExpr::Arith(ast) if arith_is_int(ast, &ints) => {
                    arith_eval(ast).map(crate::ir::IrExpr::Int)
                }
                _ => None,
            };
            if let Some(v) = lifted {
                *value = v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, IrExpr, IrType, StrStyle};

    fn str_(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn assign(v: &str, e: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget { var: v.to_string(), sigil: None, indices: vec![] }],
            expr: e,
            asm: None,
        }
    }

    fn program(stmts: Vec<IrStmt>, ints: bool) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
            subs: vec![],
            var_types: if ints { vec![("x".to_string(), IrType::Int)] } else { vec![] },
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
        }
    }

    #[test]
    fn an_int_typed_literal_becomes_an_int_node() {
        let mut p = program(vec![assign("x", str_("42"))], true);
        FrontendIntLift.run(&mut p, &PassContext::default());
        assert_eq!(p.stmts[0], assign("x", IrExpr::Int(42)));
    }

    #[test]
    fn an_untyped_literal_is_untouched() {
        let mut p = program(vec![assign("x", str_("42"))], false);
        let before = format!("{:?}", p.stmts);
        FrontendIntLift.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_literal_arithmetic_expression_folds_exactly() {
        let ast = ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(ArithAst::Num(1)),
            rhs: Box::new(ArithAst::Num(2)),
        };
        let mut p = program(vec![assign("x", IrExpr::Arith(Box::new(ast)))], true);
        FrontendIntLift.run(&mut p, &PassContext::default());
        assert_eq!(p.stmts[0], assign("x", IrExpr::Int(3)));
    }

    #[test]
    fn overflow_refuses() {
        let ast = ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(ArithAst::Num(i64::MAX)),
            rhs: Box::new(ArithAst::Num(1)),
        };
        let mut p = program(vec![assign("x", IrExpr::Arith(Box::new(ast)))], true);
        let before = format!("{:?}", p.stmts);
        FrontendIntLift.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }

    #[test]
    fn a_float_spelling_is_not_an_int() {
        let mut p = program(vec![assign("x", str_("1.5"))], true);
        let before = format!("{:?}", p.stmts);
        FrontendIntLift.run(&mut p, &PassContext::default());
        assert_eq!(before, format!("{:?}", p.stmts));
    }
}
