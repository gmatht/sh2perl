//! loop-int-lift — a `for` loop over a literal list of decimal strings whose
//! loop variable is INT-typed takes integer elements instead.
//!
//! ## Need
//! Shell loops iterate TEXT: `for i in 1 2 3` binds the strings "1".."3".
//! A frontend that proved `i` numeric (its A2 verdict says `Int`) still
//! emits the string list, so every backend pays a conversion per iteration:
//! Python renders `for i in ["1", "2", "3"]` and then `int(i)` at each use,
//! Go/Rust parse each element, and so on. The list is a literal, and the
//! loop variable is provably numeric, so the integers are the same values.
//!
//! ## Scope — the sound rule
//! Rewrite `For { var, iter: Array(items) }` to integer elements iff ALL of:
//!   - `int_typed` contains `var` (the caller's type verdict; no verdict, no
//!     rewrite — the plain `transform` entry therefore does nothing),
//!   - every item is `Str(s, _)` where `s` is a CANONICAL decimal literal:
//!     optional `-`, digits, NO leading zero (except "0"), and it parses as
//!     `i64`. Leading zeros matter: bash `echo "$i"` prints `007`, and the
//!     integer 7 would print `7` — a wrong answer, so refuse.
//! The rewrite is then value-preserving for arithmetic and for `str()`
//! rendering of the same number; `"+1"`/`" 1"`/`"1x"` are refused.
//!
//! ## Placement
//! Typed entry (`transform_with_types`) only: the shared `transforms::apply`
//! pipeline has no `var_types` (it runs before the A2 analysis), so a plain
//! registration would be a no-op. The Python ingress calls the typed entry
//! after its analysis; other backends may adopt it the same way. This is the
//! §11 marketplace shape — a shared rewrite with an opt-in consumer.

use std::collections::HashSet;

use crate::ir::{IrExpr, IrStmt};

/// The plain registry shape: no type verdicts, so nothing can be proven.
pub fn transform(_stmts: &mut Vec<IrStmt>) -> bool {
    false
}

/// Canonical decimal literal (no leading zeros, i64 range).
fn canonical_int(s: &str) -> Option<i64> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    let v: i64 = s.parse().ok()?;
    if neg && v == 0 {
        return None; // "-0" prints differently as text
    }
    Some(v)
}

/// Does `s` spell a reference to `var` (`$i`, `${i}`)? A Str carrying the
/// shell text `$i` is a TEXT read of the loop variable.
fn mentions_var(s: &str, var: &str) -> bool {
    s.contains(&format!("${var}")) || s.contains(&format!("${{{var}")) || s == var
}

/// Would replacing the loop variable's bound value by an integer change how
/// the body OBSERVES it? The lift is only value-preserving where the value is
/// used numerically. Every non-arithmetic mention (a Str that spells `$i`, a
/// `getVar("i")`, a bare `Ident("i")`, an interpolated part) is a text read,
/// and this walker refuses the whole loop when it finds one. Unknown nodes
/// refuse too — REFUSE > GUESS.
fn expr_reads_as_text(e: &IrExpr, var: &str) -> bool {
    match e {
        // numeric context: the lifted integer IS the value bash would parse
        IrExpr::Arith(_) => false,
        IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Json(_) | IrExpr::RawExpr(_) => false,
        IrExpr::Regex { .. } | IrExpr::Range { .. } => false,
        IrExpr::Str(s, _) => mentions_var(s, var),
        IrExpr::Var(n, _) | IrExpr::Ident(n) => n == var,
        IrExpr::Index { var: base, key } => base == var || expr_reads_as_text(key, var),
        IrExpr::BinOp { lhs, op: _, rhs }
        | IrExpr::DefinedOr { expr: lhs, default: rhs } => {
            expr_reads_as_text(lhs, var) || expr_reads_as_text(rhs, var)
        }
        IrExpr::Call { args, .. } | IrExpr::Array(args) => {
            args.iter().any(|a| expr_reads_as_text(a, var))
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads_as_text(obj, var) || args.iter().any(|a| expr_reads_as_text(a, var))
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads_as_text(cond, var)
                || expr_reads_as_text(then, var)
                || expr_reads_as_text(else_, var)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Lit(t) => mentions_var(t, var),
            crate::ir::InterpPart::Expr(x) => expr_reads_as_text(x, var),
        }),
        IrExpr::Capture { expr, .. } | IrExpr::Splice(expr) => expr_reads_as_text(expr, var),
        IrExpr::Object(fields) => fields.iter().any(|(k, v)| {
            mentions_var(k, var) || expr_reads_as_text(v, var)
        }),
        IrExpr::Arrow(body) | IrExpr::Lambda { body, .. } => stmts_read_as_text(body, var),
        IrExpr::ArrayComp { var: bind, iter, elem, cond } => {
            bind == var
                || expr_reads_as_text(iter, var)
                || expr_reads_as_text(elem, var)
                || cond.as_ref().is_some_and(|c| expr_reads_as_text(c, var))
        }
        IrExpr::Ext(_) => true,
    }
}

fn stmts_read_as_text(stmts: &[IrStmt], var: &str) -> bool {
    stmts.iter().any(|st| match st {
        IrStmt::Output { value, .. } => expr_reads_as_text(value, var),
        IrStmt::Assign { targets, expr, .. } => {
            targets.iter().any(|t| t.indices.iter().any(|i| expr_reads_as_text(i, var)))
                || expr_reads_as_text(expr, var)
        }
        IrStmt::Declare { init, .. } => {
            init.as_ref().is_some_and(|i| expr_reads_as_text(i, var))
        }
        IrStmt::DeclareArray { elements, .. } => {
            elements.iter().any(|e| expr_reads_as_text(e, var))
        }
        IrStmt::Expr(e) | IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e))
        | IrStmt::SetChildError(e) => expr_reads_as_text(e, var),
        IrStmt::Ext(_) => true,
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_reads_as_text(expr, var),
        IrStmt::If { cond, then, elsifs, else_ } => {
            expr_reads_as_text(cond, var)
                || stmts_read_as_text(then, var)
                || elsifs.iter().any(|(c, b)| {
                    expr_reads_as_text(c, var) || stmts_read_as_text(b, var)
                })
                || stmts_read_as_text(else_, var)
        }
        IrStmt::For { var: v, iter, body } => {
            v == var || expr_reads_as_text(iter, var) || stmts_read_as_text(body, var)
        }
        IrStmt::ForInit { init, cond, step, body } => {
            expr_reads_as_text(cond, var)
                || stmts_read_as_text(init, var)
                || stmts_read_as_text(step, var)
                || stmts_read_as_text(body, var)
        }
        IrStmt::While { cond, body } => {
            expr_reads_as_text(cond, var) || stmts_read_as_text(body, var)
        }
        IrStmt::DoWhile { body, cond, .. } => {
            expr_reads_as_text(cond, var) || stmts_read_as_text(body, var)
        }
        IrStmt::Case { discriminant, clauses } => {
            expr_reads_as_text(discriminant, var)
                || clauses.iter().any(|c| stmts_read_as_text(&c.body, var))
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            stmts_read_as_text(body, var)
                || excepts.iter().any(|e| stmts_read_as_text(&e.body, var))
                || stmts_read_as_text(else_body, var)
                || stmts_read_as_text(finally_body, var)
        }
        IrStmt::Function { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => stmts_read_as_text(body, var),
        IrStmt::Redirect { inner, .. } => stmts_read_as_text(inner, var),
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| stmts_read_as_text(s, var)),
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            expr_reads_as_text(cmd, var)
                || args.iter().any(|a| expr_reads_as_text(a, var))
                || redirects.iter().any(|r| expr_reads_as_text(r, var))
                || env.iter().any(|(k, v)| mentions_var(k, var) || expr_reads_as_text(v, var))
        }
        IrStmt::WriteFile { path, content, .. } => {
            expr_reads_as_text(path, var) || expr_reads_as_text(content, var)
        }
        IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Return(None)
        | IrStmt::Exit(None)
        | IrStmt::Require(_)
        | IrStmt::RawText(_)
        | IrStmt::Label(_)
        | IrStmt::Goto(_) => false,
        _ => true,
    })
}

fn lift(stmts: &mut [IrStmt], int_typed: &HashSet<String>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        match st {
            IrStmt::For { var, iter, body } => {
                if int_typed.contains(var) && !stmts_read_as_text(body, var) {
                    if let IrExpr::Array(items) = iter {
                        let lifted: Option<Vec<IrExpr>> = items
                            .iter()
                            .map(|e| match e {
                                IrExpr::Str(s, _) => canonical_int(s).map(IrExpr::Int),
                                _ => None,
                            })
                            .collect();
                        if let Some(new_items) = lifted {
                            *items = new_items;
                            changed = true;
                        }
                    }
                }
                changed |= lift(body, int_typed);
            }
            IrStmt::ForInit { init, step, body, .. } => {
                changed |= lift(init, int_typed);
                changed |= lift(step, int_typed);
                changed |= lift(body, int_typed);
            }
            IrStmt::While { body, .. } => changed |= lift(body, int_typed),
            IrStmt::DoWhile { body, .. } => changed |= lift(body, int_typed),
            IrStmt::If { then, elsifs, else_, .. } => {
                changed |= lift(then, int_typed);
                for (_, b) in elsifs.iter_mut() {
                    changed |= lift(b, int_typed);
                }
                changed |= lift(else_, int_typed);
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses.iter_mut() {
                    changed |= lift(&mut c.body, int_typed);
                }
            }
            IrStmt::Try { body, excepts, else_body, finally_body } => {
                changed |= lift(body, int_typed);
                for e in excepts.iter_mut() {
                    changed |= lift(&mut e.body, int_typed);
                }
                changed |= lift(else_body, int_typed);
                changed |= lift(finally_body, int_typed);
            }
            IrStmt::Function { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => changed |= lift(body, int_typed),
            IrStmt::Redirect { inner, .. } => changed |= lift(inner, int_typed),
            IrStmt::Pipeline { stages, .. } => {
                for s in stages.iter_mut() {
                    changed |= lift(s, int_typed);
                }
            }
            _ => {}
        }
    }
    changed
}

/// Apply with the caller's INT verdicts attached.
pub fn transform_with_types(stmts: &mut Vec<IrStmt>, int_typed: &HashSet<String>) -> bool {
    if int_typed.is_empty() {
        return false;
    }
    lift(stmts, int_typed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ArithAst, StrStyle};

    fn for_list(var: &str, items: &[&str]) -> Vec<IrStmt> {
        vec![IrStmt::For {
            var: var.into(),
            iter: IrExpr::Array(
                items
                    .iter()
                    .map(|s| IrExpr::Str((*s).into(), StrStyle::DoubleQuoted))
                    .collect(),
            ),
            body: vec![],
        }]
    }

    #[test]
    fn lifts_int_typed_loop() {
        let mut stmts = for_list("i", &["1", "2", "3"]);
        let ints: HashSet<String> = ["i".to_string()].into_iter().collect();
        assert!(transform_with_types(&mut stmts, &ints));
        match &stmts[0] {
            IrStmt::For { iter: IrExpr::Array(items), .. } => {
                assert!(matches!(items.as_slice(), [IrExpr::Int(1), IrExpr::Int(2), IrExpr::Int(3)]));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn refuses_without_verdict() {
        let mut stmts = for_list("i", &["1", "2"]);
        assert!(!transform_with_types(&mut stmts, &HashSet::new()));
    }

    #[test]
    fn refuses_when_the_body_uses_the_var_as_text() {
        // `key="$i,$j"` is a TEXT read: lifting i to 1 would build "1,j" vs
        // bash's identical text … but the array element would print as `1`
        // without the shell's quoting rules, so the lift stays out.
        let mut stmts = for_list("i", &["1", "2"]);
        if let IrStmt::For { body, .. } = &mut stmts[0] {
            *body = vec![IrStmt::Expr(IrExpr::Call {
                func: "arrayIndex".into(),
                args: vec![
                    IrExpr::Str("m".into(), StrStyle::DoubleQuoted),
                    IrExpr::Str("$i,$j".into(), StrStyle::DoubleQuoted),
                ],
            })];
        }
        let ints: HashSet<String> = ["i".to_string()].into_iter().collect();
        assert!(!transform_with_types(&mut stmts, &ints));
    }

    #[test]
    fn lifts_when_the_body_is_arithmetic() {
        let mut stmts = for_list("i", &["1", "2"]);
        if let IrStmt::For { body, .. } = &mut stmts[0] {
            *body = vec![IrStmt::Expr(IrExpr::Arith(Box::new(ArithAst::Bin {
                op: "+".into(),
                lhs: Box::new(ArithAst::Var("i".into())),
                rhs: Box::new(ArithAst::Num(1)),
            })))];
        }
        let ints: HashSet<String> = ["i".to_string()].into_iter().collect();
        assert!(transform_with_types(&mut stmts, &ints));
    }

    #[test]
    fn refuses_leading_zero_and_non_decimal() {
        for bad in ["007", "+1", "1 ", "1x", "-0"] {
            let mut stmts = for_list("i", &["1", bad]);
            let ints: HashSet<String> = ["i".to_string()].into_iter().collect();
            assert!(!transform_with_types(&mut stmts, &ints), "accepted {bad:?}");
        }
    }
}