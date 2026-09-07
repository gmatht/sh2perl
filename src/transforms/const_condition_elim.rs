//! const-condition-elim — evaluate fully-literal `If`/`While`/`DoWhile`
//! conditions and prune to the taken branch (a natural follow-on to
//! copy-propagation, which turns const variables into literals first).
//!
//! ## Need
//! Renderers emit a literal `test` as a runtime check; `if [ "$CONST" -eq
//! 1 ]` (or a copy-propagated literal) still compiles to a compare + the
//! `_g`/lastExit dance. Evaluating the condition at IR time and keeping
//! only the taken branch removes the check entirely, in every backend.
//! It also composes with dead-code passes: pruned branches are gone
//! before any emitter sees them.
//!
//! ## Scope — the sound version
//! Only a condition that is a CONSTANT with NO side effects and NO
//! variables is evaluated:
//!   - `Bool(b)` / `Int(n)` (n != 0 = true),
//!   - a `test` call whose argument is an all-literal comparison
//!     (`"1 -eq 1"`, `"-n foo"`, …),
//!   - a `BinOp` comparison/logic over two `Int` literals
//!     (`Lt/Le/Eq/Ne/Gt/Ge/And/Or`).
//! Anything referencing a variable, a capture, a string interpolation or
//! an arith with a var is left alone (refuse > guess).
//!
//! Then:
//!   - `If` with cond `true` → the `then` block (pruning dead elsifs/else);
//!     cond `false` → the first `true` elsif, else the `else_` block.
//!   - `While` with cond `false` → the whole loop (never runs) is removed.
//!     `While` with cond `true` is LEFT (a non-constant trip `while true`
//!     needs its runtime body — refusing avoids an infinite-loop rewrite).
//!   - `DoWhile` with cond `false` after the body → the body runs exactly
//!     once, so it is replaced by its body.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Prereq:
//! copy-propagation (turn const vars into literals first). The pass
//! rebuilds statement lists, so it nests cleanly into any body.

use crate::ir::{BinOpKind, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let (out, changed) = pass(stmts.drain(..).collect());
    *stmts = out;
    changed
}

fn pass(stmts: Vec<IrStmt>) -> (Vec<IrStmt>, bool) {
    let mut changed = false;
    let mut out = Vec::new();
    for st in stmts {
        match st {
            IrStmt::If { cond, then, elsifs, else_ } => {
                let (then2, c1) = pass(then);
                let mut elsifs2 = Vec::new();
                let mut c2 = c1;
                for (c, b) in elsifs {
                    let (b2, cb) = pass(b);
                    c2 |= cb;
                    elsifs2.push((c, b2));
                }
                let (else2, c3) = pass(else_);
                changed |= c2 | c3;
                match eval_cond(&cond) {
                    Some(true) => {
                        out.extend(then2);
                        changed = true;
                    }
                    Some(false) => {
                        // take the first constant-true elsif; else the else-
                        let mut taken = else2;
                        for (c, b) in elsifs2 {
                            if eval_cond(&c) == Some(true) {
                                taken = b;
                                break;
                            }
                            // a constant-false elsif is dead — but it was
                            // already skipped by `taken` fallthrough; only
                            // the first true (or the else) is kept
                        }
                        out.extend(taken);
                        changed = true;
                    }
                    None => {
                        // prune any elsif with a literal-false cond; if a
                        // literal-true elsif appears, everything after it
                        // (incl. else) is dead
                        let mut kept: Vec<(IrExpr, Vec<IrStmt>)> = Vec::new();
                        let mut done = false;
                        for (c, b) in elsifs2 {
                            match eval_cond(&c) {
                                Some(false) => {
                                    changed = true;
                                }
                                Some(true) => {
                                    kept.push((c, b));
                                    done = true;
                                    changed = true;
                                    break;
                                }
                                None => kept.push((c, b)),
                            }
                            if done {
                                break;
                            }
                        }
                        let else2 = if done { Vec::new() } else { else2 };
                        out.push(IrStmt::If { cond, then: then2, elsifs: kept, else_: else2 });
                    }
                }
            }
            IrStmt::While { cond, body } => {
                let (body2, c) = pass(body);
                changed |= c;
                if eval_cond(&cond) == Some(false) {
                    changed = true; // the loop never runs — drop it
                } else {
                    out.push(IrStmt::While { cond, body: body2 });
                }
            }
            IrStmt::DoWhile { body, cond, until } => {
                let (body2, c) = pass(body);
                changed |= c;
                let _base = if until { false } else { true };
                // until=false, cond=false -> body once; until=true, cond=true
                // -> body once (the `while`-sense is inverted)
                if (until && eval_cond(&cond) == Some(true)) || (!until && eval_cond(&cond) == Some(false)) {
                    out.extend(body2);
                    changed = true;
                } else {
                    out.push(IrStmt::DoWhile { body: body2, cond, until });
                }
            }
            _ => out.push(st),
        }
    }
    (out, changed)
}

/// Evaluate a condition to a constant; `None` = not (provably) constant.
fn eval_cond(e: &IrExpr) -> Option<bool> {
    match e {
        IrExpr::Bool(b) => Some(*b),
        IrExpr::Int(n) => Some(*n != 0),
        IrExpr::Call { func, args } if func == "test" => eval_test(args.first()?),
        IrExpr::BinOp { op, lhs, rhs } => {
            let (a, b) = (const_i64(lhs)?, const_i64(rhs)?);
            match op {
                BinOpKind::Lt => Some(a < b),
                BinOpKind::Le => Some(a <= b),
                BinOpKind::Eq => Some(a == b),
                BinOpKind::Ne => Some(a != b),
                BinOpKind::Gt => Some(a > b),
                BinOpKind::Ge => Some(a >= b),
                BinOpKind::And => Some(a != 0 && b != 0),
                BinOpKind::Or => Some(a != 0 || b != 0),
                _ => None,
            }
        }
        _ => None,
    }
}

fn const_i64(e: &IrExpr) -> Option<i64> {
    match e {
        IrExpr::Int(n) => Some(*n),
        IrExpr::Str(s, _) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Evaluate an all-literal `test` string: `"1 -eq 1"`, `"-n foo"`, … Only
/// fires on literal operands — a `$var` in the string is refused.
fn eval_test(s: &IrExpr) -> Option<bool> {
    let s = match s {
        IrExpr::Str(s, _) => s,
        _ => return None,
    };
    let s = s.trim();
    let words: Vec<&str> = s.split_whitespace().collect();
    match words.as_slice() {
        [a, op, b] => {
            // a `$var` ANYWHERE in an operand (quoted or bare) makes the
            // test non-constant — `"$x"` starts with a quote, so the
            // old starts_with('$') check missed it and evaluated
            // `-z "$never_set2"` as the literal `-z \"\"$never_set2\"\"`
            // (non-empty → false), eliminating the if (unset-var-empty-
            // string-test regression).
            if a.contains('$') || b.contains('$') {
                return None; // a variable — not constant
            }
            let (x, y) = (a.parse::<i64>().ok()?, b.parse::<i64>().ok()?);
            Some(match *op {
                "-eq" => x == y,
                "-ne" => x != y,
                "-lt" => x < y,
                "-le" => x <= y,
                "-gt" => x > y,
                "-ge" => x >= y,
                _ => return None,
            })
        }
        // `-n`/`-z` only on a BARE literal word. A QUOTED word (
        // `-z "$var"`, `-n ""`) is NOT literal: the quotes are part of
        // the raw test string, a `"$var"` expansion is a variable (unset
        // → EMPTY, so `-z "$unset"` is TRUE — never foldable), and
        // `-n ""` / `-z ""` are empty-ARG semantics ("-n" on an empty
        // arg is false, "-z" is true) — the inverse of the bare-word
        // reading. Only `-z foo` / `-n bar` (no quotes, no `$`) fold.
        ["-n", w] if is_bare_literal(w) => Some(!w.is_empty()),
        ["-z", w] if is_bare_literal(w) => Some(w.is_empty()),
        _ => None,
    }
}

/// A likely single literal word: no `$` (a variable reference) and no
/// quote characters (a quoted empty string or quoted var).
fn is_bare_literal(w: &str) -> bool {
    !w.contains('$') && !w.contains('"') && !w.contains('\'')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn test_cond(s: &str) -> IrExpr {
        IrExpr::Call {
            func: "test".to_string(),
            args: vec![IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)],
        }
    }

    fn if_stmt(cond: IrExpr, then_txt: &str, else_txt: &str) -> IrStmt {
        IrStmt::If {
            cond,
            then: vec![IrStmt::Output {
                value: IrExpr::Str(then_txt.to_string(), StrStyle::DoubleQuoted),
                newline: true,
                target: None,
            }],
            elsifs: vec![],
            else_: vec![IrStmt::Output {
                value: IrExpr::Str(else_txt.to_string(), StrStyle::DoubleQuoted),
                newline: true,
                target: None,
            }],
        }
    }

    /// `[ -z "$never_set2" ]` — a QUOTED variable is not constant (unset
    /// expands to EMPTY, so `-z` is TRUE — never foldable to false).
    #[test]
    fn refuses_quoted_var_in_z() {
        let cond = test_cond(" -z \"$never_set2\" ");
        assert_eq!(eval_cond(&cond), None, "quoted $var must not fold");
    }

    /// `[ -z foo ]` — a bare literal folds (foo is non-empty → false).
    #[test]
    fn folds_bare_literal_z() {
        let cond = test_cond("-z foo");
        assert_eq!(eval_cond(&cond), Some(false));
    }

    /// `[ -z "" ]` — the quoted EMPTY string has empty-arg semantics
    /// (`-z ""` is true) — the raw quote chars are not a bare literal.
    #[test]
    fn refuses_quoted_empty_in_z() {
        let cond = test_cond("-z \"\"");
        assert_eq!(eval_cond(&cond), None);
    }

    /// `[ -n "" ]` — quote chars make it non-literal (deferred to the
    /// runtime, which decides the empty-arg truth).
    #[test]
    fn refuses_quoted_empty_in_n() {
        let cond = test_cond("-n \"\"");
        assert_eq!(eval_cond(&cond), None);
    }

    /// `[ -n "hello" ]` — quoted literal stays out of the bare-literal
    /// fold (conservative; the runtime decides).
    #[test]
    fn refuses_quoted_literal_in_n() {
        let cond = test_cond("-n \"hello\"");
        assert_eq!(eval_cond(&cond), None);
    }

    /// `[ 5 -eq 5 ]` still folds to true.
    #[test]
    fn still_folds_numeric() {
        let cond = test_cond("5 -eq 5");
        assert_eq!(eval_cond(&cond), Some(true));
    }

    /// `[ "$x" -eq 5 ]` — quoted var in a numeric compare is refused by
    /// the parse (never mis-folded).
    #[test]
    fn refuses_quoted_var_numeric() {
        let cond = test_cond("\"$x\" -eq 5");
        assert_eq!(eval_cond(&cond), None);
    }

    /// End-to-end: `if [ -z "$v" ]; then A; else B; fi` stays an If (the
    /// condition is not constant), never pruned to the else branch.
    #[test]
    fn keeps_if_with_quoted_var_cond() {
        let mut stmts = vec![if_stmt(test_cond(" -z \"$v\" "), "A", "B")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], IrStmt::If { .. }));
    }
}
