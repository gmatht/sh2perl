// ── strip_cfor — lower the RICH C-style ForInit to the shell-flavored A1 ──
//
// A1 carries the rich `ForInit { init, cond, step, body }` for imperative
// frontends (C, C++). The shell-flavored renderers never see it: this pass
// lowers it to `init; while(cond){ body-with-step-before-every-continue }`
// — the same shape the frontends used to emit themselves. The step is
// re-inserted before every top-level `continue` in the body (a shell
// `continue` skips to the loop end, so the trailing step would be skipped
// and the loop would spin on the second iteration) — recursing into
// If/Block (where continues can appear) but NOT into nested While/For/
// DoWhile/ForInit (their `continue` binds to themselves).
//
// A renderer that encounters an UNSTRIPPED ForInit refuses (REFUSE > GUESS)
// — the pipeline runs this pass before every renderer, so reaching one
// means a pipeline forgot the strip.
use crate::ir::{IrExpr, IrProgram, IrStmt};

pub fn strip_cfor(prog: &mut IrProgram) {
    for s in &mut prog.stmts {
        strip_stmt(s);
    }
}

fn strip_stmt(s: &mut IrStmt) {
    match s {
        // A ForInit nested in EXPRESSION position (pipeline stages,
        // whileLoop/forLoop cond/body arrows, capture bodies, subshell
        // stages): the statement arms below recurse into statements, but
        // an expression-carried ForInit (e.g. a `for ((...))` inside a
        // `( ... ) | sort` stage — the subshell body lives in the
        // pipeline Call's Arrow arg) would survive to a renderer.
        IrStmt::Expr(e) => strip_expr(e),
        IrStmt::Assign { expr, .. } => strip_expr(expr),
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                strip_expr(i);
            }
        }
        IrStmt::Output { value, .. } => strip_expr(value),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            strip_expr(cond);
            for st in then.iter_mut() {
                strip_stmt(st);
            }
            for (_, b) in elsifs.iter_mut() {
                for st in b.iter_mut() {
                    strip_stmt(st);
                }
            }
            for st in else_.iter_mut() {
                strip_stmt(st);
            }
        }
        IrStmt::While { cond, body, .. } => {
            strip_expr(cond);
            for st in body.iter_mut() {
                strip_stmt(st);
            }
        }
        IrStmt::For { iter, body, .. } => {
            strip_expr(iter);
            for st in body.iter_mut() {
                strip_stmt(st);
            }
        }
        IrStmt::DoWhile { cond, body, .. } => {
            strip_expr(cond);
            for st in body.iter_mut() {
                strip_stmt(st);
            }
        }
        IrStmt::ForInit { init, cond, step, body } => {
            // body with the step spliced before every top-level continue
            let step = std::mem::take(step);
            let mut lowered_body = Vec::with_capacity(body.len() + step.len() + 1);
            for st in body.drain(..) {
                lowered_body.push(splice_continue(st, step.clone()));
            }
            // the NORMAL path: the step runs at the end of every iteration
            // (the continue-path steps were spliced before each continue)
            lowered_body.extend(step.clone());
            let init = std::mem::take(init);
            // IrExpr has no Default — replace with an empty Str placeholder
            let cond = std::mem::replace(
                cond,
                crate::ir::IrExpr::Str(String::new(), crate::ir::StrStyle::DoubleQuoted),
            );
            *s = IrStmt::Block(
                init
                    .into_iter()
                    .chain(std::iter::once(IrStmt::While {
                        cond,
                        body: lowered_body,
                    }))
                    .collect(),
            );
            // the While's body may itself contain ForInit (nested) — recurse
            if let IrStmt::Block(b) = s {
                for st in b.iter_mut() {
                    strip_stmt(st);
                }
            }
        }
        IrStmt::Block(b)
        | IrStmt::Function { body: b, .. }
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Redirect { inner: b, .. } => {
            for st in b.iter_mut() {
                strip_stmt(st);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            for st in body.iter_mut() {
                strip_stmt(st);
            }
            for e in excepts.iter_mut() {
                for st in e.body.iter_mut() {
                    strip_stmt(st);
                }
            }
            for st in else_body.iter_mut() {
                strip_stmt(st);
            }
            for st in finally_body.iter_mut() {
                strip_stmt(st);
            }
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses.iter_mut() {
                for st in c.body.iter_mut() {
                    strip_stmt(st);
                }
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages.iter_mut() {
                for st in stage.iter_mut() {
                    strip_stmt(st);
                }
            }
        }
        _ => {}
    }
}

/// Expression-level recursion for the strip (a ForInit can hide in an
/// `IrExpr::Arrow` — pipeline-stage/loop-cond/loop-body/capture bodies —
/// or behind Call/Array/BinOp/Ternary wrappers). Arith ASTs never carry
/// statements, so they need no recursion.
fn strip_expr(e: &mut IrExpr) {
    match e {
        IrExpr::Arrow(stmts) => {
            for st in stmts.iter_mut() {
                strip_stmt(st);
            }
        }
        IrExpr::Call { args, .. } | IrExpr::MethodCall { args, .. } => {
            for a in args.iter_mut() {
                strip_expr(a);
            }
        }
        IrExpr::Array(items) => {
            for a in items.iter_mut() {
                strip_expr(a);
            }
        }
        IrExpr::Object(props) => {
            for (_, v) in props.iter_mut() {
                strip_expr(v);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            strip_expr(lhs);
            strip_expr(rhs);
        }
        IrExpr::Ternary {
            cond, then, else_, ..
        } => {
            strip_expr(cond);
            strip_expr(then);
            strip_expr(else_);
        }
        IrExpr::DefinedOr { expr, default, .. } => {
            strip_expr(expr);
            strip_expr(default);
        }
        IrExpr::Index { key, .. } => strip_expr(key),
        IrExpr::Capture { expr, .. } => strip_expr(expr),
        _ => {}
    }
}

// one body statement: `continue` gets the step spliced BEFORE it; everything
// else is walked (so a continue inside an If/Block arm still gets the step,
// but a continue inside a nested loop binds to that loop, not this for).
// both the first-class Continue AND the legacy `Expr(Call(func:
// "continue"))` form need the step spliced before them.
fn is_continue(st: &IrStmt) -> bool {
    match st {
        IrStmt::Continue => true,
        IrStmt::Expr(crate::ir::IrExpr::Call { func, .. }) => func == "continue",
        _ => false,
    }
}

fn splice_continue(st: IrStmt, step: Vec<IrStmt>) -> IrStmt {
    match st {
        IrStmt::Continue if true => {
            let mut seq = step;
            seq.push(IrStmt::Continue);
            IrStmt::Block(seq)
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => IrStmt::If {
            cond,
            then: then
                .into_iter()
                .map(|s| splice_continue(s, step.clone()))
                .collect(),
            elsifs: elsifs
                .into_iter()
                .map(|(c, b)| {
                    (
                        c,
                        b.into_iter()
                            .map(|s| splice_continue(s, step.clone()))
                            .collect(),
                    )
                })
                .collect(),
            else_: else_
                .into_iter()
                .map(|s| splice_continue(s, step.clone()))
                .collect(),
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrProgram, IrStmt, StrStyle};

    #[test]
    fn forinit_lowers_to_while_with_step_and_continue_splice() {
        let mut prog = IrProgram { imports: vec![], requires: vec![], stmts: vec![],
            subs: vec![], var_types: vec![], stmt_lines: vec![], var_const: vec![],
            var_lengths: vec![], var_lifetimes: vec![], var_bash_env: vec![], var_nospace: vec![] };
        prog.stmts = vec![IrStmt::ForInit {
            init: vec![IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: "i".into(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Str("0".into(), StrStyle::DoubleQuoted),
            }],
            cond: IrExpr::Str("$i -lt 3".into(), StrStyle::DoubleQuoted),
            step: vec![IrStmt::Assign {
                targets: vec![crate::ir::AssignTarget {
                    var: "i".into(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Str("i + 1".into(), StrStyle::DoubleQuoted),
            }],
            body: vec![IrStmt::Continue, IrStmt::Break],
        }];
        strip_cfor(&mut prog);
        // the result: Block([ init, While(cond, [ Block([step, Continue]), Break ]) ])
        match &prog.stmts[0] {
            IrStmt::Block(body) => {
                assert_eq!(body.len(), 2, "init + While");
                assert!(matches!(&body[1], IrStmt::While { .. }), "While");
                if let IrStmt::While { body: wb, .. } = &body[1] {
                    // [ Block([step, Continue]), Break, step ]
                    assert_eq!(wb.len(), 3, "spliced continue + break + trailing step");
                    assert!(matches!(&wb[0], IrStmt::Block(b) if b.len() == 2), "step+continue splice");
                    assert!(matches!(&wb[1], IrStmt::Break), "break kept");
                    assert!(matches!(&wb[2], IrStmt::Assign { .. }), "trailing step");
                } else {
                    panic!("not a While");
                }
            }
            other => panic!("not a Block: {other:?}"),
        }
    }
}
