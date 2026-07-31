//! ShIR — the language-neutral layer between the shell AST and the backends.
//!
//! `ast_to_ir` builds an `IrProgram` from the parsed shell AST using neutral
//! IR nodes (plus `sh2.*`-namespace calls expressed via `IrExpr::Call`); the
//! ESTree emitter consumes this IR via `shir_to_estree`, so the shell→ESTree
//! lowering logic lives in one place (PLAN.md §3). The Perl generator builds
//! its own IR flavor for `ir_to_perl`; the neutral nodes here
//! (Case/Redirect/Function/Subshell/Background/Arrow/...) are ESTree-path only.

use crate::ast::*;
use crate::estree::*;
use crate::ir::*;

fn call(func: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        func: func.to_string(),
        args,
    }
}

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}

// ── AST → IR ─────────────────────────────────────────────────────────

pub fn ast_to_ir(commands: &[Command]) -> IrProgram {
    IrProgram {
        imports: vec![],
        requires: vec![],
        stmts: commands.iter().filter_map(stmt_for_command).collect(),
        subs: vec![],
    }
}

fn stmt_for_command(cmd: &Command) -> Option<IrStmt> {
    Some(match cmd {
        Command::BlankLine => return None,
        Command::TestExpression(t) => {
            IrStmt::Expr(call("test", vec![st(&t.expression)]))
        }
        Command::Simple(sc) => exec_stmt(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        Command::BuiltinCommand(bc) => exec_stmt(
            &Word::Literal(bc.name.clone(), None),
            &bc.args,
            &bc.env_vars,
            &bc.redirects,
        ),
        Command::Assignment(a) => IrStmt::Assign {
            targets: vec![AssignTarget {
                var: a.variable.clone(),
                sigil: None,
                indices: vec![],
            }],
            expr: assignment_value_ir(a),
        },
        Command::If(if_stmt) => IrStmt::If {
            cond: command_to_ir(&if_stmt.condition),
            then: body_stmts(&if_stmt.then_branch),
            elsifs: vec![],
            else_: if_stmt
                .else_branch
                .as_ref()
                .map(|b| body_stmts(b.as_ref()))
                .unwrap_or_default(),
        },
        Command::Case(c) => case_to_ir(c),
        Command::While(w) => {
            let cond = command_to_ir(&w.condition);
            IrStmt::While {
                cond: if w.is_until {
                    not_ir(cond)
                } else {
                    cond
                },
                body: body_stmts(&Command::Block(w.body.clone())),
            }
        }
        Command::For(f) => IrStmt::For {
            var: f.variable.clone(),
            iter: IrExpr::Array(f.items.iter().map(for_item_ir).collect()),
            body: body_stmts(&Command::Block(f.body.clone())),
        },
        Command::Block(b) => IrStmt::Block(
            b.commands.iter().filter_map(stmt_for_command).collect(),
        ),
        Command::Pipeline(p) => IrStmt::Expr(call(
            "pipeline",
            vec![IrExpr::Array(
                p.commands
                    .iter()
                    .map(|c| IrExpr::Arrow(command_arrow_stmts(c)))
                    .collect(),
            )],
        )),
        Command::ShoptCommand(s) => {
            IrStmt::Expr(call("shopt", vec![st(&s.option), IrExpr::Bool(s.enable)]))
        }
        Command::CStyleFor(cf) => IrStmt::Expr(call(
            "cstyleFor",
            vec![
                st(&cf.arith_content),
                IrExpr::Arrow(body_stmts(&Command::Block(cf.body.clone()))),
            ],
        )),
        Command::Function(f) => IrStmt::Function {
            name: f.name.clone(),
            body: body_stmts(&Command::Block(f.body.clone())),
        },
        Command::Subshell(c) => IrStmt::Subshell(command_arrow_stmts(c)),
        Command::Background(c) => IrStmt::Background(command_arrow_stmts(c)),
        Command::Redirect(rc) => IrStmt::Redirect {
            inner: vec![stmt_for_command(&rc.command).unwrap_or(IrStmt::Expr(call("true", vec![])))],
            redirects: rc.redirects.iter().map(redirect_to_ir).collect(),
        },
        Command::And(l, r) => IrStmt::Expr(IrExpr::BinOp {
            op: BinOpKind::And,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        }),
        Command::Or(l, r) => IrStmt::Expr(IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        }),
        Command::Not(c) => IrStmt::Expr(not_ir(command_to_ir(c))),
        Command::Break(_) => IrStmt::Expr(call("break", vec![])),
        Command::Continue(_) => IrStmt::Expr(call("continue", vec![])),
        Command::Return(w) => IrStmt::Return(w.as_ref().map(word_ir_quoted)),
        other => IrStmt::Expr(call(
            "unsupported",
            vec![st(&format!("{other:?}"))],
        )),
    })
}

fn not_ir(inner: IrExpr) -> IrExpr {
    IrExpr::BinOp {
        op: BinOpKind::Not,
        lhs: Box::new(inner.clone()),
        rhs: Box::new(inner),
    }
}

fn command_arrow_stmts(c: &Command) -> Vec<IrStmt> {
    match c {
        Command::Block(b) => b.commands.iter().filter_map(stmt_for_command).collect(),
        // expression-bodied arrows
        Command::Simple(_)
        | Command::BuiltinCommand(_)
        | Command::TestExpression(_)
        | Command::Redirect(_)
        | Command::Pipeline(_)
        | Command::And(_, _)
        | Command::Or(_, _)
        | Command::Not(_)
        | Command::Assignment(_)
        | Command::ShoptCommand(_) => vec![IrStmt::Expr(command_to_ir(c))],
        // compound commands → block-bodied arrows
        other => vec![stmt_for_command(other).unwrap_or(IrStmt::Expr(call("true", vec![])))],
    }
}

fn body_stmts(cmd: &Command) -> Vec<IrStmt> {
    match cmd {
        Command::Block(b) => b.commands.iter().filter_map(stmt_for_command).collect(),
        _ => stmt_for_command(cmd).map(|s| vec![s]).unwrap_or_default(),
    }
}

fn exec_stmt(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
    redirects: &[Redirect],
) -> IrStmt {
    let exec_call = exec_call_ir(name, args, env);
    if redirects.is_empty() {
        IrStmt::Expr(exec_call)
    } else {
        IrStmt::Redirect {
            inner: vec![IrStmt::Expr(exec_call)],
            redirects: redirects.iter().map(redirect_to_ir).collect(),
        }
    }
}

fn exec_call_ir(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
) -> IrExpr {
    let mut call_args = vec![word_ir(name), IrExpr::Array(args.iter().map(word_ir).collect())];
    if !env.is_empty() {
        call_args.push(IrExpr::Object(
            env.iter()
                .map(|(k, v)| (k.clone(), word_ir_quoted(v)))
                .collect(),
        ));
    }
    call("exec", call_args)
}

fn assignment_value_ir(a: &Assignment) -> IrExpr {
    match &a.value {
        Word::Array(name, elements, _) => call(
            "setArray",
            vec![
                st(name),
                IrExpr::Array(
                    elements
                        .iter()
                        .map(|e| st(e))
                        .collect(),
                ),
            ],
        ),
        _ => word_ir_quoted(&a.value),
    }
}

fn redirect_to_ir(r: &Redirect) -> IrRedirect {
    let (mode, default_fd) = match &r.operator {
        RedirectOperator::Input => ("r", 0),
        RedirectOperator::Output => ("w", 1),
        RedirectOperator::Append => ("a", 1),
        RedirectOperator::InputOutput => ("r+", 0),
        RedirectOperator::Heredoc => ("heredoc", 0),
        RedirectOperator::HeredocTabs => ("heredoc-tabs", 0),
        RedirectOperator::HereString => ("herestring", 0),
        RedirectOperator::StderrOutput => ("w", 2),
        RedirectOperator::StderrAppend => ("a", 2),
        RedirectOperator::StderrInput => ("r", 2),
        RedirectOperator::ProcessSubstitutionInput(_) => ("unsupported", 0),
        RedirectOperator::ProcessSubstitutionOutput(_) => ("unsupported", 0),
    };
    IrRedirect {
        fd: r.fd.or(Some(default_fd)),
        mode: mode.to_string(),
        target: match &r.operator {
            RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => {
                st(r.heredoc_body.as_deref().unwrap_or(""))
            }
            _ => word_ir(&r.target),
        },
        interpolate: !r.heredoc_quoted,
    }
}

fn case_to_ir(c: &CaseStatement) -> IrStmt {
    IrStmt::Case {
        discriminant: word_ir(&c.word),
        clauses: c
            .cases
            .iter()
            .map(|cl| IrCaseClause {
                patterns: cl.patterns.iter().map(|p| p.to_string()).collect(),
                body: cl.body.iter().filter_map(stmt_for_command).collect(),
            })
            .collect(),
    }
}

fn command_to_ir(cmd: &Command) -> IrExpr {
    match cmd {
        Command::TestExpression(t) => call("test", vec![st(&t.expression)]),
        Command::Simple(sc) => exec_expr(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        Command::BuiltinCommand(bc) => exec_expr(
            &Word::Literal(bc.name.clone(), None),
            &bc.args,
            &bc.env_vars,
            &bc.redirects,
        ),
        Command::Redirect(rc) => call(
            "redirect",
            vec![
                IrExpr::Arrow(vec![IrStmt::Expr(command_to_ir(&rc.command))]),
                IrExpr::Array(rc.redirects.iter().map(redirect_spec_object).collect()),
            ],
        ),
        Command::Pipeline(p) => call(
            "pipeline",
            vec![IrExpr::Array(
                p.commands
                    .iter()
                    .map(|c| IrExpr::Arrow(command_arrow_stmts(c)))
                    .collect(),
            )],
        ),
        Command::Subshell(c) => call("subshell", vec![IrExpr::Arrow(command_arrow_stmts(c))]),
        Command::Block(b) => call(
            "block",
            vec![IrExpr::Arrow(
                b.commands.iter().filter_map(stmt_for_command).collect(),
            )],
        ),
        Command::While(w) => call(
            "whileLoop",
            vec![
                IrExpr::Arrow(vec![IrStmt::Expr(if w.is_until {
                    not_ir(command_to_ir(&w.condition))
                } else {
                    command_to_ir(&w.condition)
                })]),
                IrExpr::Arrow(body_stmts(&Command::Block(w.body.clone()))),
            ],
        ),
        Command::Assignment(a) => assignment_value_ir(a),
        Command::ShoptCommand(s) => call("shopt", vec![st(&s.option), IrExpr::Bool(s.enable)]),
        Command::And(l, r) => IrExpr::BinOp {
            op: BinOpKind::And,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        },
        Command::Or(l, r) => IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        },
        Command::Not(c) => not_ir(command_to_ir(c)),
        Command::Return(w) => {
            let mut args = vec![];
            if let Some(w) = w {
                args.push(word_ir_quoted(w));
            }
            call("return", args)
        }
        Command::Break(_) => call("break", vec![]),
        Command::Continue(_) => call("continue", vec![]),
        other => call("unsupported", vec![st(&format!("{other:?}"))]),
    }
}

fn exec_expr(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
    redirects: &[Redirect],
) -> IrExpr {
    let exec_call = exec_call_ir(name, args, env);
    if redirects.is_empty() {
        exec_call
    } else {
        call(
            "redirect",
            vec![
                IrExpr::Arrow(vec![IrStmt::Expr(exec_call)]),
                IrExpr::Array(redirects.iter().map(redirect_spec_object).collect()),
            ],
        )
    }
}

fn redirect_spec_object(r: &Redirect) -> IrExpr {
    let ir = redirect_to_ir(r);
    let mut props = vec![
        ("fd".to_string(), IrExpr::Int(ir.fd.unwrap_or(0) as i64)),
        ("mode".to_string(), st(&ir.mode)),
        ("target".to_string(), ir.target),
    ];
    if ir.mode == "heredoc" || ir.mode == "heredoc-tabs" {
        props.push(("interpolate".to_string(), IrExpr::Bool(ir.interpolate)));
    }
    IrExpr::Object(props)
}

// ── words → IR ───────────────────────────────────────────────────────

fn word_ir_quoted(w: &Word) -> IrExpr {
    match w {
        Word::CommandSubstitution(cmd, _) => call(
            "capture",
            vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
        ),
        _ => word_ir(w),
    }
}

fn word_ir(w: &Word) -> IrExpr {
    match w {
        Word::Literal(s, _) => st(s),
        Word::Variable(name, _, _) => call("getVar", vec![st(name)]),
        Word::CommandSubstitution(cmd, _) => call(
            "captureWords",
            vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
        ),
        Word::ParameterExpansion(pe, _) => param_ir(pe),
        Word::Arithmetic(ae, _) => call("arith", vec![st(&ae.expression)]),
        Word::BraceExpansion(be, _) => brace_ir(be),
        Word::Array(name, _, _) => call("getVar", vec![st(name)]),
        Word::MapAccess(name, key, _) => call("arrayIndex", vec![st(name), st(key)]),
        Word::MapKeys(name, _) => call("arrayItems", vec![st(name)]),
        Word::MapLength(name, _) => call("arrayLen", vec![st(name)]),
        Word::ArraySlice(name, offset, length, _) => call(
            "param",
            vec![
                st("slice"),
                st(name),
                st(offset),
                st(length.as_deref().unwrap_or("")),
            ],
        ),
        Word::StringInterpolation(interp, _) => {
            if let Some(part) = pure_template_part(interp) {
                part
            } else {
                interpolate_ir(&interp.parts)
            }
        }
        other => call("unsupported", vec![st(&other.to_string())]),
    }
}

fn param_ir(pe: &ParameterExpansion) -> IrExpr {
    let (op, extra): (String, Vec<IrExpr>) = match &pe.operator {
        ParameterExpansionOperator::None => (String::new(), vec![]),
        ParameterExpansionOperator::UppercaseAll => ("^^".into(), vec![]),
        ParameterExpansionOperator::LowercaseAll => (",,".into(), vec![]),
        ParameterExpansionOperator::UppercaseFirst => ("^".into(), vec![]),
        ParameterExpansionOperator::RemoveLongestPrefix(p) => ("##".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveShortestPrefix(p) => ("#".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveLongestSuffix(p) => ("%%".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveShortestSuffix(p) => ("%".into(), vec![st(p)]),
        ParameterExpansionOperator::SubstituteAll(p, r) => ("//".into(), vec![st(p), st(r)]),
        ParameterExpansionOperator::DefaultValue(d) => (":-".into(), vec![st(d)]),
        ParameterExpansionOperator::AssignDefault(d) => (":=".into(), vec![st(d)]),
        ParameterExpansionOperator::ErrorIfUnset(e) => (":?".into(), vec![st(e)]),
        ParameterExpansionOperator::Basename => ("basename".into(), vec![]),
        ParameterExpansionOperator::Dirname => ("dirname".into(), vec![]),
        ParameterExpansionOperator::ArraySlice(off, len) => (
            "slice".into(),
            vec![st(off), st(len.as_deref().unwrap_or(""))],
        ),
    };
    let mut args = vec![st(&op), st(&pe.variable)];
    args.extend(extra);
    call("param", args)
}

fn brace_ir(be: &BraceExpansion) -> IrExpr {
    call(
        "brace",
        vec![
            st(be.prefix.as_deref().unwrap_or("")),
            IrExpr::Json(serde_json::Value::Array(vec![brace_items_json(&be.items)])),
            IrExpr::Json(serde_json::Value::Array(vec![])),
            st(be.suffix.as_deref().unwrap_or("")),
        ],
    )
}

fn brace_items_json(items: &[BraceItem]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|it| match it {
                BraceItem::Literal(s) => serde_json::Value::String(s.clone()),
                BraceItem::Range(r) => serde_json::json!({
                    "range": [r.start, r.end, r.step, r.format]
                }),
                BraceItem::Sequence(seq) => serde_json::Value::Array(
                    seq.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
                ),
                BraceItem::Nested(n) => serde_json::json!({ "nested": brace_items_json(&n.items) }),
                BraceItem::Compound(c) => serde_json::json!({ "nested": brace_items_json(c) }),
            })
            .collect(),
    )
}

fn pure_template_part(interp: &StringInterpolation) -> Option<IrExpr> {
    let mut non_literal: Option<&StringPart> = None;
    for p in &interp.parts {
        match p {
            StringPart::Literal(s) if s.is_empty() => {}
            StringPart::Literal(_) => return None,
            other => {
                if non_literal.is_some() {
                    return None;
                }
                non_literal = Some(other);
            }
        }
    }
    non_literal.map(part_ir)
}

fn interpolate_ir(parts: &[StringPart]) -> IrExpr {
    IrExpr::Interpolate(
        parts
            .iter()
            .map(|p| match p {
                StringPart::Literal(s) => InterpPart::Lit(s.clone()),
                other => InterpPart::Expr(Box::new(part_ir(other))),
            })
            .collect(),
    )
}

fn part_ir(part: &StringPart) -> IrExpr {
    match part {
        StringPart::Literal(_) => unreachable!("Literal parts handled in interpolate_ir"),
        StringPart::Variable(name) => call("getVar", vec![st(name)]),
        StringPart::ParameterExpansion(pe) => param_ir(pe),
        StringPart::Arithmetic(ae) => call("arith", vec![st(&ae.expression)]),
        StringPart::MapAccess(name, key) => call("arrayIndex", vec![st(name), st(key)]),
        StringPart::MapKeys(name) => call("arrayItems", vec![st(name)]),
        StringPart::MapLength(name) => call("arrayLen", vec![st(name)]),
        StringPart::ArraySlice(name, offset, length) => call(
            "param",
            vec![
                st("slice"),
                st(name),
                st(offset),
                st(length.as_deref().unwrap_or("")),
            ],
        ),
        StringPart::CommandSubstitution(cmd) => call(
            "capture",
            vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
        ),
        other => call("unsupported", vec![st(&format!("{other:?}"))]),
    }
}

fn for_item_ir(w: &Word) -> IrExpr {
    match w {
        Word::Variable(name, _, _) if name == "@" || name == "*" => call("listVar", vec![st(name)]),
        Word::StringInterpolation(interp, _) => {
            if let Some(part) = pure_template_part(interp) {
                return part;
            }
            word_ir(w)
        }
        _ => word_ir(w),
    }
}

// ── IR → ESTree ──────────────────────────────────────────────────────

fn is_async_call(name: &str) -> bool {
    matches!(
        name,
        "exec" | "redirect" | "pipeline" | "subshell" | "block" | "whileLoop" | "cstyleFor"
            | "capture" | "captureWords" | "forLoop"
    )
}

pub fn shir_to_estree(prog: &IrProgram) -> Program {
    Program {
        type_: "Program",
        source_type: "module",
        body: prog.stmts.iter().filter_map(stmt_to_estree).collect(),
    }
}

pub fn shir_to_estree_json(prog: &IrProgram) -> Result<String, serde_json::Error> {
    serde_json::to_string(&shir_to_estree(prog))
}

fn stmt_to_estree(stmt: &IrStmt) -> Option<Stmt> {
    Some(match stmt {
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "break" => {
            Stmt::BreakStatement { label: None }
        }
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "continue" => {
            Stmt::ContinueStatement { label: None }
        }
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "return" => {
            Stmt::ReturnStatement {
                argument: args.first().map(expr_to_estree),
            }
        }
        IrStmt::Expr(e) => Stmt::ExpressionStatement {
            expression: expr_to_estree(e),
        },
        IrStmt::Assign { targets, expr } => {
            let target = &targets[0];
            match expr {
                // arr=(...) → sh2.setArray already sets the array
                IrExpr::Call { func, .. } if func == "setArray" => Stmt::ExpressionStatement {
                    expression: expr_to_estree(expr),
                },
                _ => Stmt::ExpressionStatement {
                    expression: sh2_call("setVar", vec![str_lit(&target.var), expr_to_estree(expr)]),
                },
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            let consequent = Box::new(Stmt::BlockStatement {
                body: then.iter().filter_map(stmt_to_estree).collect(),
            });
            let alternate: Option<Box<Stmt>> = if else_.is_empty() {
                None
            } else {
                Some(match else_.as_slice() {
                    [IrStmt::If { .. }] => Box::new(
                        stmt_to_estree(&else_[0]).unwrap_or(Stmt::BlockStatement { body: vec![] }),
                    ),
                    _ => Box::new(Stmt::BlockStatement {
                        body: else_.iter().filter_map(stmt_to_estree).collect(),
                    }),
                })
            };
            Stmt::IfStatement {
                test: expr_to_estree(cond),
                consequent,
                alternate,
            }
        }
        IrStmt::While { cond, body } => Stmt::ExpressionStatement {
            expression: await_call(
                "whileLoop",
                vec![
                    arrow(vec![], IrExpr::Arrow(vec![IrStmt::Expr(cond.clone())])),
                    arrow(vec![], IrExpr::Arrow(body.clone())),
                ],
            ),
        },
        IrStmt::For { var, iter, body } => {
            let js_var = safe_ident(var);
            let mut body_stmts = vec![IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: var.clone(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Ident(js_var.clone()),
            }];
            body_stmts.extend(body.clone());
            Stmt::ExpressionStatement {
                expression: await_call(
                    "forLoop",
                    vec![
                        expr_to_estree(iter),
                        arrow_with_param(js_var, IrExpr::Arrow(body_stmts)),
                    ],
                ),
            }
        }
        IrStmt::Function { name, body } => Stmt::ExpressionStatement {
            expression: sh2_call(
                "define",
                vec![str_lit(name), arrow(vec![], IrExpr::Arrow(body.clone()))],
            ),
        },
        IrStmt::Subshell(stmts) => Stmt::ExpressionStatement {
            expression: await_call(
                "subshell",
                vec![arrow(vec![], IrExpr::Arrow(stmts.clone()))],
            ),
        },
        IrStmt::Background(stmts) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "background",
                vec![arrow(vec![], IrExpr::Arrow(stmts.clone()))],
            ),
        },
        IrStmt::Block(stmts) => Stmt::BlockStatement {
            body: stmts.iter().filter_map(stmt_to_estree).collect(),
        },
        IrStmt::Redirect { inner, redirects } => Stmt::ExpressionStatement {
            expression: await_call(
                "redirect",
                vec![
                    arrow(vec![], IrExpr::Arrow(inner.clone())),
                    array(redirects.iter().map(redirect_spec_to_estree).collect()),
                ],
            ),
        },
        IrStmt::Case { discriminant, clauses } => {
            let patterns: Vec<Expr> = clauses
                .iter()
                .flat_map(|c| c.patterns.iter())
                .map(|p| str_lit(p))
                .collect();
            let cases: Vec<SwitchCase> = clauses
                .iter()
                .flat_map(|c| {
                    let consequent: Vec<Stmt> = c
                        .body
                        .iter()
                        .filter_map(stmt_to_estree)
                        .chain(std::iter::once(Stmt::BreakStatement { label: None }))
                        .collect();
                    c.patterns
                        .iter()
                        .map(move |p| SwitchCase {
                            type_: "SwitchCase",
                            test: Some(str_lit(p)),
                            consequent: consequent.clone(),
                        })
                })
                .collect();
            Stmt::SwitchStatement {
                discriminant: sh2_call(
                    "caseMatch",
                    vec![expr_to_estree(discriminant), array(patterns)],
                ),
                cases,
            }
        }
        IrStmt::Return(opt) => Stmt::ReturnStatement {
            argument: opt.as_ref().map(expr_to_estree),
        },
        IrStmt::Exec { cmd, args, capture: _, redirects: _, env } => {
            let mut call_args = vec![
                expr_to_estree(cmd),
                expr_to_estree(&IrExpr::Array(args.clone())),
            ];
            if !env.is_empty() {
                call_args.push(Expr::ObjectExpression {
                    properties: env
                        .iter()
                        .map(|(k, v)| prop(k, expr_to_estree(v)))
                        .collect(),
                });
            }
            Stmt::ExpressionStatement {
                expression: await_expr(sh2_call("exec", call_args)),
            }
        }
        other => unreachable!("Perl-only IR statement reached the ESTree renderer: {other:?}"),
    })
}

fn redirect_spec_to_estree(r: &IrRedirect) -> Expr {
    let mut props = vec![
        prop(
            "fd",
            Expr::Literal {
                value: serde_json::Value::from(r.fd.unwrap_or(0)),
                raw: None,
            },
        ),
        prop("mode", str_lit(&r.mode)),
        prop("target", expr_to_estree(&r.target)),
    ];
    if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
        props.push(prop(
            "interpolate",
            Expr::Literal {
                value: serde_json::Value::Bool(r.interpolate),
                raw: None,
            },
        ));
    }
    Expr::ObjectExpression { properties: props }
}

fn expr_to_estree(e: &IrExpr) -> Expr {
    match e {
        IrExpr::Int(i) => Expr::Literal {
            value: serde_json::Value::from(*i),
            raw: None,
        },
        IrExpr::Str(s, _) => Expr::Literal {
            value: serde_json::Value::String(s.clone()),
            raw: None,
        },
        IrExpr::Bool(b) => Expr::Literal {
            value: serde_json::Value::Bool(*b),
            raw: None,
        },
        IrExpr::Json(v) => Expr::Literal {
            value: v.clone(),
            raw: None,
        },
        IrExpr::Var(name, _) => sh2_call("getVar", vec![str_lit(name)]),
        IrExpr::Ident(name) => Expr::Identifier { name: name.clone() },
        IrExpr::Array(elems) => Expr::ArrayExpression {
            elements: elems.iter().map(|e| Some(expr_to_estree(e))).collect(),
        },
        IrExpr::Object(props) => Expr::ObjectExpression {
            properties: props
                .iter()
                .map(|(k, v)| prop(k, expr_to_estree(v)))
                .collect(),
        },
        IrExpr::Interpolate(parts) => interpolate_to_estree(parts),
        IrExpr::Call { func, args } => {
            let call = sh2_call(func, args.iter().map(expr_to_estree).collect());
            if is_async_call(func) {
                await_expr(call)
            } else {
                call
            }
        }
        IrExpr::BinOp { op: BinOpKind::And, lhs, rhs } => Expr::LogicalExpression {
            operator: "&&",
            left: Box::new(expr_to_estree(lhs)),
            right: Box::new(expr_to_estree(rhs)),
        },
        IrExpr::BinOp { op: BinOpKind::Or, lhs, rhs } => Expr::LogicalExpression {
            operator: "||",
            left: Box::new(expr_to_estree(lhs)),
            right: Box::new(expr_to_estree(rhs)),
        },
        IrExpr::BinOp { op: BinOpKind::Not, lhs, .. } => Expr::UnaryExpression {
            operator: "!",
            argument: Box::new(expr_to_estree(lhs)),
            prefix: true,
        },
        IrExpr::Arrow(stmts) => arrow(vec![], IrExpr::Arrow(stmts.clone())),
        other => unreachable!("Perl-only IR expression reached the ESTree renderer: {other:?}"),
    }
}

fn interpolate_to_estree(parts: &[InterpPart]) -> Expr {
    let mut quasis = Vec::new();
    let mut expressions = Vec::new();
    let mut raw = String::new();
    for part in parts {
        match part {
            InterpPart::Lit(s) => raw.push_str(s),
            InterpPart::Expr(e) => {
                quasis.push(quasi_element(&mut raw, false));
                expressions.push(expr_to_estree(e));
            }
        }
    }
    quasis.push(quasi_element(&mut raw, true));
    Expr::TemplateLiteral { quasis, expressions }
}

fn array(elements: Vec<Expr>) -> Expr {
    Expr::ArrayExpression {
        elements: elements.into_iter().map(Some).collect(),
    }
}

fn arrow(params: Vec<Expr>, body: IrExpr) -> Expr {
    match &body {
        IrExpr::Arrow(stmts) if stmts.len() == 1 && matches!(stmts[0], IrStmt::Expr(_)) => {
            let inner = match &stmts[0] {
                IrStmt::Expr(e) => expr_to_estree(e),
                _ => unreachable!(),
            };
            Expr::ArrowFunctionExpression {
                params,
                body: ArrowBody::Expr(Box::new(inner)),
                expression: true,
                r#async: true,
            }
        }
        IrExpr::Arrow(stmts) => Expr::ArrowFunctionExpression {
            params,
            body: ArrowBody::Block(Box::new(Stmt::BlockStatement {
                body: stmts.iter().filter_map(stmt_to_estree).collect(),
            })),
            expression: false,
            r#async: true,
        },
        other => Expr::ArrowFunctionExpression {
            params,
            body: ArrowBody::Expr(Box::new(expr_to_estree(other))),
            expression: true,
            r#async: true,
        },
    }
}

fn arrow_with_param(param: String, body: IrExpr) -> Expr {
    arrow(vec![Expr::Identifier { name: param }], body)
}

fn await_call(name: &str, args: Vec<Expr>) -> Expr {
    await_expr(sh2_call(name, args))
}

fn await_expr(inner: Expr) -> Expr {
    Expr::AwaitExpression {
        argument: Box::new(inner),
    }
}

fn safe_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "var", "let", "const", "function", "class", "if", "else", "for", "while",
        "do", "switch", "case", "break", "continue", "return", "new", "delete",
        "typeof", "instanceof", "in", "of", "try", "catch", "finally", "throw",
        "this", "super", "import", "export", "default", "extends", "static",
        "yield", "await", "null", "true", "false", "void", "debugger", "arguments",
    ];
    if RESERVED.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}
