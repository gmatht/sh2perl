//! test-simplification — fold self-comparison `test` conditions
//! (`[ "$a" -eq "$a" ]` → true, `[ "$a" -lt "$a" ]` → false) into a
//! `Bool`, feeding const-condition-elim (which prunes the resulting
//! constant condition). Distinct from const-condition-elim's LITERAL
//! evaluation — this fires on an operand compared to ITSELF.
//!
//! ## Need
//! Generated and template code emits `[ "$x" -eq "$x" ]`/`[ "$x" = "$x" ]`
//! (e.g. a guard that always holds); every backend renders the compare.
//! A self-comparison has a deterministic truth value (a value equals
//! itself), so it can be decided at IR time.
//!
//! ## Scope — the sound rule
//! A `test` condition of the form `<op> <X> <X>` (the SAME token on both
//! sides — variable or literal) folds to a constant:
//!   `-eq  -le  -ge  =`   → true
//!   `-ne  -lt  -gt  !=`  → false
//! The two operands must be textually IDENTICAL (a self-comparison reads
//! the same value at both positions — no write intervenes in a single
//! test), and must contain no `(`/`)`/arithmetic (a compound edit).
//! `-n`/`-z` on a literal are const-condition-elim's domain, not this.
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest). Registered in
//! `transforms.rs` (DEBASHC_TRANSFORMS gated). Prereq: const-condition-
//! elim (consumes the emitted `Bool`).

use crate::ir::{ArithAst, BinOpKind, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt_pass(st);
    }
    c
}

fn stmt_pass(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::If { cond, then, elsifs, else_, .. } => {
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
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, var: _ } => {
            let mut c = expr_pass(cond);
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        _ => false,
    }
}

/// Rewrite a self-comparison `test` cond to a `Bool`; recurse into And/Or.
fn expr_pass(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::BinOp { op: BinOpKind::And | BinOpKind::Or, lhs, rhs } => {
            expr_pass(lhs) | expr_pass(rhs)
        }
        IrExpr::Call { func, args } if func == "test" => {
            if let Some(IrExpr::Str(s, _)) = args.first() {
                if let Some(b) = self_compare(s) {
                    *e = IrExpr::Bool(b);
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// `"$a -eq $a"` / `"$a = $a"` → Some(truth); `None` if not a self-comparison.
fn self_compare(s: &str) -> Option<bool> {
    let words: Vec<&str> = s.split_whitespace().collect();
    let (a, op, b) = match words.as_slice() {
        [a, op, b] => (a, op, b),
        _ => return None,
    };
    if a != b {
        return None; // not a self-comparison
    }
    if a.contains('(') || a.contains(')') {
        return None; // a compound expression — leave it
    }
    let result = match *op {
        "-eq" | "-le" | "-ge" | "=" | "==" => true,
        "-ne" | "-lt" | "-gt" | "!=" => false,
        _ => return None,
    };
    Some(result)
}
