//! counted-arith-forinit — recover `IrStmt::ForInit` from a counted
//! `while` written with STRUCTURED arithmetic, the shape frontends
//! (py-sh-go, c-sh-go, …) emit for `for i in range(N):` /
//! `for (i=0; i<n; i++)`.
//!
//! ```text
//! i = 0                     i = 0
//! while (i < N) {            while (i < N) {
//!     BODY             →         BODY
//!     i = i + 1 / i++        }   i = i + 1 / i++
//! }
//! ```
//! becomes one `IrStmt::ForInit { init, cond, step, body }`.
//!
//! This is the structured-`ArithAst` sibling of `counted-while-forinit`
//! (which matches shell `BinOp`/text forms). Keeping it separate means
//! neither pass can regress the other's shapes; both feed the renderers'
//! native counted-loop arms and the CUDA candidacy.
//!
//! REFUSE > GUESS: the condition must actually read the counter; the
//! trailing update must be exactly `v += 1` / `v++` / `v = v + 1`; the
//! body must not write or escape the counter (a nested write would make
//! the trailing step ambiguous, and `continue` skips the step in a
//! `while` but not in a `for`); `$?` reads veto (the step's deletion
//! would shift status observation).

use crate::ir::{ArithAst, AssignTarget, IrExpr, IrStmt};

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= recurse(st);
    }

    let mut i = 1;
    while i < stmts.len() {
        // The step's update form (Assign or expression statement).
        let counter = match &stmts[i - 1] {
            IrStmt::Assign { targets, asm: None, .. }
                if targets.len() == 1 && targets[0].indices.is_empty() =>
            {
                targets[0].var.clone()
            }
            _ => {
                i += 1;
                continue;
            }
        };
        let IrStmt::While { cond, body } = &stmts[i] else {
            i += 1;
            continue;
        };
        // Only the structured-Arith condition (the shell/text form is
        // counted-while-forinit's job).
        let extracted = (|| -> Option<(IrExpr, IrStmt, Vec<IrStmt>)> {
            let IrExpr::Arith(a) = cond else { return None };
            if !arith_mentions(a, &counter) {
                return None;
            }
            if reads_status(a) {
                return None;
            }
            let step = body.last().and_then(|s| counter_update(s, &counter))?;
            if body[..body.len() - 1]
                .iter()
                .any(|s| writes_or_escapes(s, &counter))
            {
                return None;
            }
            Some((cond.clone(), step, body[..body.len() - 1].to_vec()))
        })();
        let Some((cond, step, body)) = extracted else {
            i += 1;
            continue;
        };
        let init = stmts.remove(i - 1);
        stmts[i - 1] = IrStmt::ForInit {
            init: vec![init],
            cond,
            step: vec![step],
            body,
        };
        changed = true;
        i = i.saturating_sub(1).max(1);
    }
    changed
}

/// Normalize a trailing counter update to an `Assign`, or None.
fn counter_update(st: &IrStmt, counter: &str) -> Option<IrStmt> {
    let arith = match st {
        IrStmt::Assign {
            targets,
            expr: IrExpr::Arith(a),
            asm: None,
        } if targets.len() == 1 && targets[0].var == counter && targets[0].indices.is_empty() => a,
        IrStmt::Expr(IrExpr::Arith(a)) => a,
        _ => return None,
    };
    let ok = match arith.as_ref() {
        ArithAst::IncDec { var, delta, .. } => var == counter && *delta == 1,
        ArithAst::Assign { var, op, rhs } if var == counter && op == "+=" => num_of(rhs) == Some(1),
        ArithAst::Bin { op, lhs, rhs } if op == "+" => {
            matches!(lhs.as_ref(), ArithAst::Var(v) | ArithAst::Ident(v) if v == counter)
                && num_of(rhs) == Some(1)
        }
        _ => false,
    };
    ok.then(|| IrStmt::Assign {
        targets: vec![AssignTarget {
            var: counter.to_string(),
            sigil: None,
            indices: Vec::new(),
        }],
        expr: IrExpr::Arith(arith.clone()),
        asm: None,
    })
}

/// Numeric literal under Cast/Assign(=) wrapping.
fn num_of(a: &ArithAst) -> Option<i64> {
    match a {
        ArithAst::Num(n) => Some(*n),
        ArithAst::Cast { arg, .. } => num_of(arg),
        ArithAst::Assign { op, rhs, .. } if op == "=" => num_of(rhs),
        _ => None,
    }
}

fn arith_mentions(a: &ArithAst, name: &str) -> bool {
    let own = match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => v == name,
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => var == name,
        ArithAst::Index { var, .. } => var == name,
        _ => false,
    };
    own || arith_kids(a).iter().any(|k| arith_mentions(k, name))
}

fn arith_kids(a: &ArithAst) -> Vec<&ArithAst> {
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => vec![],
        ArithAst::Index { key, .. } => vec![key],
        ArithAst::Bin { lhs, rhs, .. } => vec![lhs, rhs],
        ArithAst::Un { arg, .. } => vec![arg],
        ArithAst::Cond { test, then, else_ } => vec![test, then, else_],
        ArithAst::Assign { rhs, .. } => vec![rhs],
        ArithAst::IncDec { .. } => vec![],
        ArithAst::Cast { arg, .. } => vec![arg],
    }
}

/// `$?` read anywhere in the condition text/nodes (status-shift veto).
fn reads_status(a: &ArithAst) -> bool {
    let own = match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => v == "?",
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => var == "?",
        ArithAst::Index { var, .. } => var == "?",
        _ => false,
    };
    own || arith_kids(a).iter().any(|k| reads_status(k))
}

fn writes_or_escapes(st: &IrStmt, name: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => targets.iter().any(|t| t.var == name),
        IrStmt::Declare { vars, .. } => vars.iter().any(|v| v.name == name),
        IrStmt::DeclareArray { var, .. } => var == name,
        IrStmt::Break
        | IrStmt::Continue
        | IrStmt::Return(_)
        | IrStmt::Exit(_)
        | IrStmt::Goto(_)
        | IrStmt::Label(_) => true,
        IrStmt::If { then, elsifs, else_, .. } => {
            then.iter().any(|s| writes_or_escapes(s, name))
                || elsifs.iter().any(|(_, b)| b.iter().any(|s| writes_or_escapes(s, name)))
                || else_.iter().any(|s| writes_or_escapes(s, name))
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. } => body.iter().any(|s| writes_or_escapes(s, name)),
        IrStmt::ForInit { init, step, body, .. } => init
            .iter()
            .chain(step)
            .chain(body)
            .any(|s| writes_or_escapes(s, name)),
        IrStmt::Function { body, named_blocks, .. } => {
            body.iter().any(|s| writes_or_escapes(s, name))
                || named_blocks
                    .iter()
                    .any(|(_, b)| b.iter().any(|s| writes_or_escapes(s, name)))
        }
        IrStmt::Try { body, else_body, finally_body, excepts } => {
            body.iter()
                .chain(else_body)
                .chain(finally_body)
                .any(|s| writes_or_escapes(s, name))
                || excepts
                    .iter()
                    .any(|e| e.body.iter().any(|s| writes_or_escapes(s, name)))
        }
        IrStmt::Case { clauses, .. } => clauses
            .iter()
            .any(|c| c.body.iter().any(|s| writes_or_escapes(s, name))),
        IrStmt::Select { clauses } => clauses
            .iter()
            .any(|c| c.body.iter().any(|s| writes_or_escapes(s, name))),
        _ => false,
    }
}

fn recurse(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            let mut c = transform(then) | transform(else_);
            for (_, b) in elsifs {
                c |= transform(b);
            }
            c
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. } => transform(body),
        IrStmt::ForInit { init, step, body, .. } => {
            transform(init) | transform(step) | transform(body)
        }
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = transform(body);
            for (_, b) in named_blocks {
                c |= transform(b);
            }
            c
        }
        IrStmt::Try { body, else_body, finally_body, excepts } => {
            let mut c = transform(body) | transform(else_body) | transform(finally_body);
            for e in excepts {
                c |= transform(&mut e.body);
            }
            c
        }
        IrStmt::Case { clauses, .. } => {
            clauses.iter_mut().map(|c| transform(&mut c.body)).fold(false, |a, b| a | b)
        }
        IrStmt::Select { clauses } => {
            clauses.iter_mut().map(|c| transform(&mut c.body)).fold(false, |a, b| a | b)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::AssignTarget;

    fn scalar(var: &str) -> AssignTarget {
        AssignTarget { var: var.to_string(), sigil: None, indices: vec![] }
    }
    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign { targets: vec![scalar(var)], expr, asm: None }
    }
    fn v(name: &str) -> ArithAst {
        ArithAst::Var(name.to_string())
    }
    fn arith(a: ArithAst) -> IrExpr {
        IrExpr::Arith(Box::new(a))
    }
    fn cond() -> IrExpr {
        arith(ArithAst::Bin {
            op: "<".to_string(),
            lhs: Box::new(v("i")),
            rhs: Box::new(v("N")),
        })
    }

    fn counted_while(step: IrStmt) -> Vec<IrStmt> {
        vec![
            assign("i", arith(ArithAst::Num(0))),
            IrStmt::While {
                cond: cond(),
                body: vec![assign("s", arith(v("s"))), step],
            },
        ]
    }

    #[test]
    fn prefix_incdec_step_recovers() {
        let step = assign(
            "i",
            arith(ArithAst::IncDec { var: "i".to_string(), delta: 1, prefix: false }),
        );
        let mut prog = counted_while(step);
        assert!(transform(&mut prog));
        assert!(matches!(prog[0], IrStmt::ForInit { .. }), "{:?}", prog[0]);
        assert_eq!(prog.len(), 1, "init + while fused: {prog:?}");
    }

    #[test]
    fn expr_incdec_step_recovers() {
        let step = IrStmt::Expr(arith(ArithAst::IncDec {
            var: "i".to_string(),
            delta: 1,
            prefix: false,
        }));
        let mut prog = counted_while(step);
        assert!(transform(&mut prog));
        let IrStmt::ForInit { step, .. } = &prog[0] else { panic!() };
        assert!(matches!(step.as_slice(), [IrStmt::Assign { .. }]), "normalized: {step:?}");
    }

    #[test]
    fn plus_one_assign_recovers() {
        let step = assign(
            "i",
            arith(ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(v("i")),
                rhs: Box::new(ArithAst::Num(1)),
            }),
        );
        let mut prog = counted_while(step);
        assert!(transform(&mut prog));
    }

    #[test]
    fn cond_not_reading_counter_vetoes() {
        let step = assign(
            "i",
            arith(ArithAst::IncDec { var: "i".to_string(), delta: 1, prefix: false }),
        );
        let mut prog = counted_while(step);
        let IrStmt::While { cond, .. } = &mut prog[1] else { panic!() };
        *cond = arith(ArithAst::Bin {
            op: ">".to_string(),
            lhs: Box::new(v("v")),
            rhs: Box::new(ArithAst::Num(1)),
        });
        assert!(!transform(&mut prog));
    }

    #[test]
    fn continue_in_body_vetoes() {
        let step = assign(
            "i",
            arith(ArithAst::IncDec { var: "i".to_string(), delta: 1, prefix: false }),
        );
        let mut prog = counted_while(step);
        let IrStmt::While { body, .. } = &mut prog[1] else { panic!() };
        body.insert(0, IrStmt::Continue);
        assert!(!transform(&mut prog));
    }

    #[test]
    fn shell_text_cond_left_alone() {
        // The `builtin("let", …)` form is counted-while-forinit's shape.
        let step = assign(
            "i",
            arith(ArithAst::IncDec { var: "i".to_string(), delta: 1, prefix: false }),
        );
        let mut prog = counted_while(step);
        let IrStmt::While { cond, .. } = &mut prog[1] else { panic!() };
        *cond = IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("let".to_string(), crate::ir::StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str(
                    "i<N".to_string(),
                    crate::ir::StrStyle::DoubleQuoted,
                )]),
            ],
        };
        assert!(!transform(&mut prog), "must not touch the shell/text form");
    }
}
