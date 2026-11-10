//! counted-while-forinit — recover the language-neutral C-style loop
//! already present in the A1 contract from the shell frontend's canonical
//! counter-while shape.
//!
//! ```text
//! i=INIT
//! while [ CONDITION(i) ]; do
//!     BODY
//!     i=i+STEP
//! done
//! ```
//!
//! becomes one `IrStmt::ForInit { init, cond, step, body }`. This is the
//! shIR counterpart of lower.js::nativeForLoops: the transformation does
//! not emit JavaScript and does not assume `sh2.*` runtime wrappers. Each
//! backend can render ForInit as its native counted loop or reject it.
//!
//! The implementation is deliberately strict. It only recognizes a plain
//! scalar assignment immediately before the loop and a final `i = i +/- K`
//! assignment. Any control escape, nested write, or `$?` observation causes
//! the original while form to remain intact.

use crate::ir::{ArithAst, BinOpKind, IrExpr, IrStmt};

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= recurse(st);
    }

    let mut i = 1;
    while i < stmts.len() {
        let init = match &stmts[i - 1] {
            IrStmt::Assign {
                targets, asm: None, ..
            } if targets.len() == 1 && targets[0].indices.is_empty() => targets[0].var.clone(),
            _ => {
                i += 1;
                continue;
            }
        };
        let (cond, body, step) = match &stmts[i] {
            IrStmt::While { cond, body } => {
                let Some(step) = body.last().and_then(|s| counter_step(s)) else {
                    i += 1;
                    continue;
                };
                (cond.clone(), body.clone(), step)
            }
            _ => {
                i += 1;
                continue;
            }
        };
        if step.0 != init
            || body[..body.len() - 1]
                .iter()
                .any(|s| writes_or_escapes(s, &init))
            || induction_guard(&cond, &stmts[i - 1], &step.1)
            || contains_status_read(&cond)
            || body[..body.len() - 1]
                .iter()
                .any(|s| contains_status_read_stmt(s))
        {
            i += 1;
            continue;
        }
        let body = body[..body.len() - 1].to_vec();
        let step_stmt = step.1;
        let init_stmt = stmts.remove(i - 1);
        // The while moved one slot left when init was removed.
        stmts[i - 1] = IrStmt::ForInit {
            init: vec![init_stmt],
            cond,
            step: vec![step_stmt],
            body,
        };
        changed = true;
        i = i.saturating_sub(1).max(1);
    }
    changed
}

/// Return (counter name, update statement) for `v = v +/- literal` or
/// `v +=/-= literal`. The literal gate keeps the loop's induction step
/// explicit and avoids changing dynamic shell arithmetic semantics.
fn counter_step(st: &IrStmt) -> Option<(String, IrStmt)> {
    let IrStmt::Assign {
        targets,
        expr,
        asm: None,
    } = st
    else {
        return None;
    };
    if targets.len() != 1 || !targets[0].indices.is_empty() {
        return None;
    }
    let name = targets[0].var.clone();
    // Shell arithmetic lowers to `Arith(Bin{…})`, never to a top-level
    // `BinOp` — matching only `BinOp` left every `$((i+1))` loop on the
    // floor (the `for-recovery` Arith-wrap class). Both spellings below
    // denote the same step: `v = v +/- K`, K a nonzero literal.
    let ok = match expr {
        IrExpr::BinOp {
            lhs,
            op: BinOpKind::Add | BinOpKind::Sub,
            rhs,
        } if matches!(lhs.as_ref(), IrExpr::Var(v, _) if v == &name)
            && matches!(rhs.as_ref(), IrExpr::Int(n) if *n != 0) =>
        {
            true
        }
        IrExpr::Arith(ast) => matches!(
            ast.as_ref(),
            ArithAst::Bin { op, lhs, rhs }
                if (op == "+" || op == "-")
                    && matches!(lhs.as_ref(), ArithAst::Var(v) if v == &name)
                    && matches!(rhs.as_ref(), ArithAst::Num(n) if *n != 0)
        ),
        _ => false,
    };
    if ok {
        Some((name, st.clone()))
    } else {
        None
    }
}

/// `while BODY; i=i+1; done` runs BODY's own updates on the final, failing
/// iteration; a `for` does not. For a loop-carried *induction* variable that
/// difference is unobservable, but not when the "counter" is also the body's
/// accumulator:
///
/// ```text
/// s=0
/// while ((v>1)); do ...; s=s+1; done   // s counts chain iterations
/// ```
///
/// Accepting that would make `ForInit{init:s=0, step:s=s+1}` — the header of
/// a degenerate `for`, not an induction variable. Consumers read `ForInit`'s
/// counter as induction: `lane_private::classify_seq_chain` then sees a chain
/// whose inits write the accumulated var, so collatz-style lane-nests stop
/// classifying (`non-seq-body`), and `ForInit` candidacy would infer
/// induction from an accumulator. A genuine counted loop's condition READS
/// its counter (`i < N`), while here it reads the chain's `v`; so decline
/// (refuse > guess) when the init and step share a variable the condition
/// never mentions.
fn induction_guard(cond: &IrExpr, init: &IrStmt, step: &IrStmt) -> bool {
    let name = |s: &IrStmt| match s {
        IrStmt::Assign { targets, .. } if targets.len() == 1 => Some(targets[0].var.clone()),
        _ => None,
    };
    let (Some(a), Some(b)) = (name(init), name(step)) else {
        return false;
    };
    // Same var, and the condition does not read it: an accumulator tally,
    // not an induction variable.
    a == b && !expr_mentions_text(cond, &a)
}

/// Does the condition text mention `name` as a word? Conditions arrive as
/// `let` text (`v > 1`), arithmetic, or a `test` call; this is a
/// conservative word-boundary scan over every string leaf, which errs
/// toward "mentions" (keeping the `while`, the safe direction).
fn expr_mentions_text(e: &IrExpr, name: &str) -> bool {
    fn walk(e: &IrExpr, name: &str) -> bool {
        match e {
            IrExpr::Str(s, _) => word_contains(s, name),
            IrExpr::Array(items) => items.iter().any(|i| walk(i, name)),
            IrExpr::Call { args, .. } => args.iter().any(|a| walk(a, name)),
            IrExpr::Arith(a) => arith_text_mentions(a, name),
            _ => true,
        }
    }
    fn word_contains(s: &str, name: &str) -> bool {
        let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        s.match_indices(name).any(|(i, _)| {
            let before = s[..i].chars().next_back();
            let after = s[i + name.len()..].chars().next();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
    }
    fn arith_text_mentions(a: &ArithAst, name: &str) -> bool {
        match a {
            ArithAst::Var(v) | ArithAst::Ident(v) => v == name,
            ArithAst::Num(_) | ArithAst::Sizeof(_) => false,
            _ => true,
        }
    }
    walk(e, name)
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
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            then.iter().any(|s| writes_or_escapes(s, name))
                || elsifs
                    .iter()
                    .any(|(_, b)| b.iter().any(|s| writes_or_escapes(s, name)))
                || else_.iter().any(|s| writes_or_escapes(s, name))
        }
        IrStmt::For { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. } => body.iter().any(|s| writes_or_escapes(s, name)),
        IrStmt::ForInit {
            init, step, body, ..
        } => init
            .iter()
            .chain(step)
            .chain(body)
            .any(|s| writes_or_escapes(s, name)),
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            body.iter().any(|s| writes_or_escapes(s, name))
                || named_blocks
                    .iter()
                    .any(|(_, b)| b.iter().any(|s| writes_or_escapes(s, name)))
        }
        IrStmt::Try {
            body,
            else_body,
            finally_body,
            excepts,
        } => {
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

fn expr_reads(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == name,
        IrExpr::Index { var, key } => var == name || expr_reads(key, name),
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads(lhs, name) || expr_reads(rhs, name),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads(cond, name) || expr_reads(then, name) || expr_reads(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => expr_reads(expr, name) || expr_reads(default, name),
        IrExpr::Call { args, .. } | IrExpr::Array(args) => args.iter().any(|e| expr_reads(e, name)),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Lit(s) => s.contains("$?") && name == "?",
            crate::ir::InterpPart::Expr(e) => expr_reads(e, name),
        }),
        IrExpr::Arrow(body) => body.iter().any(|s| writes_or_escapes(s, name)),
        IrExpr::Lambda { body, .. } => body.iter().any(|s| writes_or_escapes(s, name)),
        _ => false,
    }
}

fn contains_status_read(e: &IrExpr) -> bool {
    expr_reads(e, "?")
}

fn contains_status_read_stmt(st: &IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. }
        | IrStmt::Expr(expr)
        | IrStmt::SetChildError(expr)
        | IrStmt::Return(Some(expr))
        | IrStmt::Exit(Some(expr))
        | IrStmt::Die { expr, .. }
        | IrStmt::Warn { expr, .. } => contains_status_read(expr),
        IrStmt::Declare { init, .. } => init.as_ref().is_some_and(contains_status_read),
        IrStmt::DeclareArray { elements, .. } => elements.iter().any(contains_status_read),
        IrStmt::Output { value, .. } => contains_status_read(value),
        IrStmt::WriteFile { path, content, .. } => {
            contains_status_read(path) || contains_status_read(content)
        }
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            contains_status_read(cmd)
                || args.iter().any(contains_status_read)
                || redirects.iter().any(contains_status_read)
                || env.iter().any(|(_, e)| contains_status_read(e))
        }
        IrStmt::Pipeline { stages, .. } => stages
            .iter()
            .flat_map(|stage| stage.iter())
            .any(contains_status_read_stmt),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            contains_status_read(cond)
                || then.iter().any(contains_status_read_stmt)
                || elsifs.iter().any(|(c, b)| {
                    contains_status_read(c) || b.iter().any(contains_status_read_stmt)
                })
                || else_.iter().any(contains_status_read_stmt)
        }
        IrStmt::For { iter, body, .. } => {
            contains_status_read(iter) || body.iter().any(contains_status_read_stmt)
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            init.iter().any(contains_status_read_stmt)
                || contains_status_read(cond)
                || step.iter().any(contains_status_read_stmt)
                || body.iter().any(contains_status_read_stmt)
        }
        IrStmt::While { cond, body } => {
            contains_status_read(cond) || body.iter().any(contains_status_read_stmt)
        }
        IrStmt::DoWhile { body, cond, .. } => {
            body.iter().any(contains_status_read_stmt) || contains_status_read(cond)
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            contains_status_read(discriminant)
                || clauses
                    .iter()
                    .any(|c| c.body.iter().any(contains_status_read_stmt))
        }
        IrStmt::Redirect { inner, redirects } => {
            inner.iter().any(contains_status_read_stmt)
                || redirects.iter().any(|r| contains_status_read(&r.target))
        }
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            body.iter().any(contains_status_read_stmt)
                || named_blocks
                    .iter()
                    .any(|(_, b)| b.iter().any(contains_status_read_stmt))
        }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            body.iter().any(contains_status_read_stmt)
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            body.iter().any(contains_status_read_stmt)
                || excepts
                    .iter()
                    .any(|e| e.body.iter().any(contains_status_read_stmt))
                || else_body.iter().any(contains_status_read_stmt)
                || finally_body.iter().any(contains_status_read_stmt)
        }
        IrStmt::Select { clauses } => clauses
            .iter()
            .any(|c| c.body.iter().any(contains_status_read_stmt)),
        IrStmt::Asm {
            inputs, outputs, ..
        } => {
            inputs.iter().any(|(_, e)| contains_status_read(e))
                || outputs.iter().any(|(_, e)| contains_status_read(e))
        }
        _ => false,
    }
}

fn recurse(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
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
        IrStmt::ForInit {
            init, step, body, ..
        } => transform(init) | transform(step) | transform(body),
        IrStmt::Function {
            body, named_blocks, ..
        } => {
            let mut c = transform(body);
            for (_, b) in named_blocks {
                c |= transform(b);
            }
            c
        }
        IrStmt::Try {
            body,
            else_body,
            finally_body,
            excepts,
        } => {
            let mut c = transform(body) | transform(else_body) | transform(finally_body);
            for e in excepts {
                c |= transform(&mut e.body);
            }
            c
        }
        IrStmt::Case { clauses, .. } => clauses
            .iter_mut()
            .map(|c| transform(&mut c.body))
            .any(|x| x),
        IrStmt::Select { clauses } => clauses
            .iter_mut()
            .map(|c| transform(&mut c.body))
            .any(|x| x),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ArithAst, AssignTarget, Decl};

    fn target(var: &str) -> AssignTarget {
        AssignTarget { var: var.to_string(), sigil: None, indices: vec![] }
    }

    fn init(var: &str) -> IrStmt {
        IrStmt::Assign {
            targets: vec![target(var)],
            expr: IrExpr::Int(0),
            asm: None,
        }
    }

    fn arith_step(var: &str, op: &str, n: i64) -> IrStmt {
        IrStmt::Assign {
            targets: vec![target(var)],
            expr: IrExpr::Arith(Box::new(ArithAst::Bin {
                op: op.to_string(),
                lhs: Box::new(ArithAst::Var(var.to_string())),
                rhs: Box::new(ArithAst::Num(n)),
            })),
            asm: None,
        }
    }

    fn binop_step(var: &str, n: i64) -> IrStmt {
        IrStmt::Assign {
            targets: vec![target(var)],
            expr: IrExpr::BinOp {
                op: BinOpKind::Add,
                lhs: Box::new(IrExpr::Var(var.to_string(), None)),
                rhs: Box::new(IrExpr::Int(n)),
            },
            asm: None,
        }
    }

    fn cond(var: &str) -> IrExpr {
        IrExpr::Call {
            func: "test".to_string(),
            args: vec![IrExpr::Str(format!("${var} -lt 10"), crate::ir::StrStyle::DoubleQuoted)],
        }
    }

    fn body(var: &str, step: IrStmt) -> Vec<IrStmt> {
        vec![
            IrStmt::Expr(IrExpr::Var(var.to_string(), None)),
            step,
        ]
    }

    fn while_loop(var: &str, step: IrStmt) -> Vec<IrStmt> {
        vec![
            init(var),
            IrStmt::While { cond: cond(var), body: body(var, step) },
        ]
    }

    fn is_forinit(stmts: &[IrStmt]) -> bool {
        matches!(stmts.get(0), Some(IrStmt::ForInit { .. }))
    }

    /// `i=$((i+1))` — the shell-lowered Arith step folds (031 shape).
    #[test]
    fn arith_wrapped_step_folds() {
        let mut stmts = while_loop("i", arith_step("i", "+", 1));
        assert!(transform(&mut stmts));
        assert!(is_forinit(&stmts), "Arith step should lift: {stmts:?}");
    }

    /// The pre-existing BinOp arm keeps working.
    #[test]
    fn binop_step_still_folds() {
        let mut stmts = while_loop("i", binop_step("i", 1));
        assert!(transform(&mut stmts));
        assert!(is_forinit(&stmts), "BinOp step should lift: {stmts:?}");
    }

    /// A zero step (`i=$((i+0))`) never advances — refuse, like the BinOp arm.
    #[test]
    fn zero_arith_step_refuses() {
        let mut stmts = while_loop("i", arith_step("i", "+", 0));
        assert!(!transform(&mut stmts));
    }

    /// A dynamic rhs (`i=$((i+n))`) is not a literal step — refuse.
    #[test]
    fn dynamic_arith_step_refuses() {
        let step = IrStmt::Assign {
            targets: vec![target("i")],
            expr: IrExpr::Arith(Box::new(ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(ArithAst::Var("i".to_string())),
                rhs: Box::new(ArithAst::Var("n".to_string())),
            })),
            asm: None,
        };
        let mut stmts = while_loop("i", step);
        assert!(!transform(&mut stmts));
    }

    /// A body that writes the counter derails the sequence — refuse.
    #[test]
    fn body_write_refuses() {
        let mut stmts = vec![
            init("i"),
            IrStmt::While {
                cond: cond("i"),
                body: vec![
                    IrStmt::Assign {
                        targets: vec![target("i")],
                        expr: IrExpr::Int(9),
                        asm: None,
                    },
                    arith_step("i", "+", 1),
                ],
            },
        ];
        assert!(!transform(&mut stmts));
    }

    /// No matching shape — the transform reports false.
    #[test]
    fn plain_while_is_untouched() {
        let mut stmts = vec![IrStmt::While {
            cond: cond("i"),
            body: vec![IrStmt::Expr(IrExpr::Var("i".to_string(), None))],
        }];
        assert!(!transform(&mut stmts));
    }

    /// `declare`d counters disqualify the same way (writes_or_escapes).
    #[test]
    fn declare_in_body_refuses() {
        let mut stmts = vec![
            init("i"),
            IrStmt::While {
                cond: cond("i"),
                body: vec![
                    IrStmt::Declare {
                        vars: vec![Decl { name: "i".to_string(), sigil: None }],
                        init: None,
                        local: false,
                    },
                    arith_step("i", "+", 1),
                ],
            },
        ];
        assert!(!transform(&mut stmts));
    }
}
