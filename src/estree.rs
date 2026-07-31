//! ESTree JSON emitter (v0).
//!
//! Lowers the shell AST directly to **standard ESTree JSON**, with shell
//! semantics expressed as calls into a documented `sh2.*` runtime namespace
//! (see PLAN.md §1.2). v0 covers a subset; any construct not yet lowered emits
//! a call to `sh2.unsupported(...)` so output is ALWAYS valid, deterministic
//! ESTree — the structural gate flags `sh2.unsupported` and the corpus gate
//! counts it as a not-implemented failure until lowered.
//!
//! v0 lowers from the raw shell AST (stable surface). When the ShIR
//! generalization lands (PLAN.md §3), this module reroutes to consume ShIR;
//! the ESTree node model below stays.
//!
//! Constraints (enforced at the structural gate, PLAN.md §2.2):
//! - standard ESTree node types only (never custom node types);
//! - async-only codegen (top-level `await`; no `*Sync` callees);
//! - every callee is in the `sh2.*` whitelist.

use crate::ast::*;
use serde::Serialize;
use std::collections::BTreeMap;

// ── ESTree node model (standard subset) ─────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Program {
    #[serde(rename = "type")]
    pub type_: &'static str,
    #[serde(rename = "sourceType")]
    pub source_type: &'static str,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Stmt {
    ExpressionStatement {
        expression: Expr,
    },
    BlockStatement {
        body: Vec<Stmt>,
    },
    IfStatement {
        test: Expr,
        consequent: Box<Stmt>,
        alternate: Option<Box<Stmt>>,
    },
    SwitchStatement {
        discriminant: Expr,
        cases: Vec<SwitchCase>,
    },
    WhileStatement {
        test: Expr,
        body: Box<Stmt>,
    },
    ForOfStatement {
        left: Box<Stmt>,
        right: Expr,
        body: Box<Stmt>,
    },
    VariableDeclaration {
        declarations: Vec<VariableDeclarator>,
        kind: &'static str,
    },
    BreakStatement {
        label: Option<String>,
    },
    ContinueStatement {
        label: Option<String>,
    },
    ReturnStatement {
        argument: Option<Expr>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SwitchCase {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub test: Option<Expr>,
    pub consequent: Vec<Stmt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariableDeclarator {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub id: Expr,
    pub init: Option<Expr>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Expr {
    Identifier {
        name: String,
    },
    Literal {
        value: serde_json::Value,
        raw: Option<String>,
    },
    TemplateLiteral {
        quasis: Vec<TemplateElement>,
        expressions: Vec<Expr>,
    },
    CallExpression {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
        optional: bool,
    },
    MemberExpression {
        object: Box<Expr>,
        property: Box<Expr>,
        computed: bool,
        optional: bool,
    },
    AwaitExpression {
        argument: Box<Expr>,
    },
    ArrowFunctionExpression {
        params: Vec<Expr>,
        body: ArrowBody,
        expression: bool,
        // Always async: closure bodies may contain `await` (nested execs),
        // and the runtime awaits every closure it invokes.
        r#async: bool,
    },
    ObjectExpression {
        properties: Vec<Property>,
    },
    ArrayExpression {
        elements: Vec<Option<Expr>>,
    },
    LogicalExpression {
        operator: &'static str,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    UnaryExpression {
        operator: &'static str,
        argument: Box<Expr>,
        prefix: bool,
    },
}

/// Arrow function body: an expression (`x => expr`) or a block
/// (`() => { ... }`). Serializes to standard ESTree (`Expression` or
/// `BlockStatement`).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Box<Stmt>),
}

#[derive(Debug, Clone, Serialize)]
pub struct Property {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub key: Expr,
    pub value: Expr,
    pub kind: &'static str,
    pub computed: bool,
    pub shorthand: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateElement {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub value: TemplateElementValue,
    pub tail: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateElementValue {
    pub raw: String,
    pub cooked: Option<String>,
}

// ── Lowering: shell AST → ESTree ────────────────────────────────────

/// Lower a parsed shell program to an ESTree `Program`.
pub fn ast_to_estree(commands: &[Command]) -> Program {
    Program {
        type_: "Program",
        source_type: "module",
        body: commands.iter().filter_map(stmt_for_command).collect(),
    }
}

/// Convenience: lower + serialize (deterministic, compact JSON).
pub fn ast_to_estree_json(commands: &[Command]) -> Result<String, serde_json::Error> {
    serde_json::to_string(&ast_to_estree(commands))
}

fn stmt_for_command(cmd: &Command) -> Option<Stmt> {
    Some(match cmd {
        Command::BlankLine => return None,
        Command::TestExpression(t) => Stmt::ExpressionStatement {
            expression: sh2_call("test", vec![str_lit(&t.expression)]),
        },
        Command::Simple(sc) => Stmt::ExpressionStatement {
            expression: exec_call_with_env(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        },
        Command::BuiltinCommand(bc) => Stmt::ExpressionStatement {
            expression: exec_call_with_env(
                &Word::Literal(bc.name.clone(), None),
                &bc.args,
                &bc.env_vars,
                &bc.redirects,
            ),
        },
        // v0: operator (e.g. +=) not yet represented; structural gate flags
        // non-Assign operators when they matter.
        Command::Assignment(a) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "setVar",
                vec![str_lit(&a.variable), word_to_expr_quoted(&a.value)],
            ),
        },
        Command::If(if_stmt) => Stmt::IfStatement {
            test: command_to_expr(&if_stmt.condition),
            consequent: Box::new(body_stmt(&if_stmt.then_branch)),
            alternate: if_stmt.else_branch.as_ref().map(|b| Box::new(body_stmt(b))),
        },
        Command::Case(c) => case_to_switch(c),
        Command::While(w) => {
            let t = command_to_expr(&w.condition);
            let test = if w.is_until {
                Expr::UnaryExpression {
                    operator: "!",
                    argument: Box::new(t),
                    prefix: true,
                }
            } else {
                t
            };
            Stmt::ExpressionStatement {
                expression: async_sh2_call(
                    "whileLoop",
                    vec![
                        // Condition must be a closure — a bare expression would
                        // be evaluated once when building the arguments.
                        arrow(vec![], ArrowBody::Expr(Box::new(test))),
                        arrow(
                            vec![],
                            ArrowBody::Block(Box::new(block_stmt(&w.body.commands))),
                        ),
                    ],
                ),
            }
        },
        Command::For(f) => {
            let js_var = safe_ident(&f.variable);
            Stmt::ExpressionStatement {
                expression: async_sh2_call(
                    "forLoop",
                    vec![
                        for_items_expr(&f.items),
                        arrow(
                            vec![Expr::Identifier { name: js_var.clone() }],
                            ArrowBody::Block(Box::new(for_loop_body(
                                &f.variable,
                                &js_var,
                                &f.body.commands,
                            ))),
                        ),
                    ],
                ),
            }
        }
        Command::Block(b) => Stmt::BlockStatement {
            body: b.commands.iter().filter_map(stmt_for_command).collect(),
        },
        Command::Pipeline(p) => Stmt::ExpressionStatement {
            expression: pipeline_expr(p),
        },
        Command::ShoptCommand(s) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "shopt",
                vec![
                    str_lit(&s.option),
                    Expr::Literal {
                        value: serde_json::Value::Bool(s.enable),
                        raw: None,
                    },
                ],
            ),
        },
        Command::CStyleFor(cf) => Stmt::ExpressionStatement {
            expression: async_sh2_call(
                "cstyleFor",
                vec![
                    str_lit(&cf.arith_content),
                    arrow(vec![], ArrowBody::Block(Box::new(block_stmt(&cf.body.commands)))),
                ],
            ),
        },
        Command::Function(f) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "define",
                vec![
                    str_lit(&f.name),
                    arrow(vec![], ArrowBody::Block(Box::new(block_stmt(&f.body.commands)))),
                ],
            ),
        },
        Command::Subshell(c) => Stmt::ExpressionStatement {
            expression: async_sh2_call("subshell", vec![arrow(vec![], command_arrow_body(c))]),
        },
        Command::Background(c) => Stmt::ExpressionStatement {
            expression: sh2_call("background", vec![arrow(vec![], command_arrow_body(c))]),
        },
        Command::Redirect(rc) => Stmt::ExpressionStatement {
            expression: apply_redirects(command_to_expr(&rc.command), &rc.redirects),
        },
        Command::And(l, r) => Stmt::ExpressionStatement {
            expression: Expr::LogicalExpression {
                operator: "&&",
                left: Box::new(command_to_expr(l)),
                right: Box::new(command_to_expr(r)),
            },
        },
        Command::Or(l, r) => Stmt::ExpressionStatement {
            expression: Expr::LogicalExpression {
                operator: "||",
                left: Box::new(command_to_expr(l)),
                right: Box::new(command_to_expr(r)),
            },
        },
        Command::Not(c) => Stmt::ExpressionStatement {
            expression: Expr::UnaryExpression {
                operator: "!",
                argument: Box::new(command_to_expr(c)),
                prefix: true,
            },
        },
        Command::Break(_) => Stmt::ExpressionStatement {
            expression: sh2_call("break", vec![]),
        },
        Command::Continue(_) => Stmt::ExpressionStatement {
            expression: sh2_call("continue", vec![]),
        },
        Command::Return(w) => Stmt::ReturnStatement {
            argument: w.as_ref().map(word_to_expr),
        },
        other => Stmt::ExpressionStatement {
            expression: unsupported(other),
        },
    })
}

/// Shell variables used as JS identifiers may collide with reserved words
/// (`for var in ...` → `async (var) => ...` is a SyntaxError). Suffix with `_`.
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

/// `for i in ...` — the JS loop variable must be mirrored into the shell var
/// store each iteration (`sh2.setVar("i", i)`) so `$i` inside the body works.
/// `js_name` may differ from `shell_name` (reserved-word avoidance).
fn for_loop_body(shell_name: &str, js_name: &str, commands: &[Command]) -> Stmt {
    let mut stmts = vec![Stmt::ExpressionStatement {
        expression: sh2_call(
            "setVar",
            vec![
                str_lit(shell_name),
                Expr::Identifier {
                    name: js_name.to_string(),
                },
            ],
        ),
    }];
    stmts.extend(commands.iter().filter_map(stmt_for_command));
    Stmt::BlockStatement { body: stmts }
}

/// Items of a `for i in ...` list. `$@`/`$*` (and `"$@"`) lower to
/// `sh2.listVar(name)` so an empty positional list yields ZERO iterations
/// (plain `sh2.getVar("@")` would yield one empty-string item).
fn for_items_expr(items: &[Word]) -> Expr {
    Expr::ArrayExpression {
        elements: items.iter().map(|w| Some(for_item_expr(w))).collect(),
    }
}

fn for_item_expr(w: &Word) -> Expr {
    match w {
        Word::Variable(name, _, _) if name == "@" || name == "*" => {
            sh2_call("listVar", vec![str_lit(name)])
        }
        Word::StringInterpolation(interp, _) => match pure_at_parts(interp) {
            Some(name) => sh2_call("listVar", vec![str_lit(name)]),
            None => word_to_expr(w),
        },
        _ => word_to_expr(w),
    }
}

/// A `"$@"`-style interpolation (a single `@`/`*` variable part, all literal
/// parts empty) → the variable name; otherwise None.
fn pure_at_parts(interp: &StringInterpolation) -> Option<&str> {
    let mut var: Option<&str> = None;
    for part in &interp.parts {
        match part {
            StringPart::Literal(s) if s.is_empty() => {}
            StringPart::Variable(name) if (name == "@" || name == "*") && var.is_none() => {
                var = Some(name);
            }
            _ => return None,
        }
    }
    var
}

/// Body of an `if`/`while`/`for`: unwrap `Block` into its statements.
fn body_stmt(cmd: &Command) -> Stmt {
    match cmd {
        Command::Block(b) => block_stmt(&b.commands),
        _ => stmt_for_command(cmd).unwrap_or_else(|| Stmt::BlockStatement { body: vec![] }),
    }
}

fn block_stmt(commands: &[Command]) -> Stmt {
    Stmt::BlockStatement {
        body: commands.iter().filter_map(stmt_for_command).collect(),
    }
}

/// Async sh2.* call — wrapped in `await` so the runtime's async work
/// completes before the next statement (statement order must be preserved).
/// `background` is intentionally NOT awaited (fire-and-forget).
fn async_sh2_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::AwaitExpression {
        argument: Box::new(sh2_call(name, args)),
    }
}

fn pipeline_expr(p: &Pipeline) -> Expr {
    async_sh2_call(
        "pipeline",
        vec![Expr::ArrayExpression {
            elements: p
                .commands
                .iter()
                .map(|c| Some(arrow(vec![], command_arrow_body(c))))
                .collect(),
        }],
    )
}

/// Lower a command to a boolean/expression context (conditions, pipeline stages).
fn command_to_expr(cmd: &Command) -> Expr {
    match cmd {
        Command::TestExpression(t) => sh2_call("test", vec![str_lit(&t.expression)]),
        Command::Simple(sc) => exec_call_with_env(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        Command::BuiltinCommand(bc) => exec_call_with_env(
            &Word::Literal(bc.name.clone(), None),
            &bc.args,
            &bc.env_vars,
            &bc.redirects,
        ),
        Command::Redirect(rc) => apply_redirects(command_to_expr(&rc.command), &rc.redirects),
        Command::Pipeline(p) => pipeline_expr(p),
        Command::Subshell(c) => async_sh2_call("subshell", vec![arrow(vec![], command_arrow_body(c))]),
        Command::Block(b) => async_sh2_call(
            "block",
            vec![arrow(vec![], ArrowBody::Block(Box::new(block_stmt(&b.commands))))],
        ),
        Command::While(w) => async_sh2_call(
            "whileLoop",
            vec![
                // Condition must be a closure — a bare expression would be
                // evaluated once when building the arguments.
                arrow(vec![], ArrowBody::Expr(Box::new(command_to_expr(&w.condition)))),
                arrow(vec![], ArrowBody::Block(Box::new(block_stmt(&w.body.commands)))),
            ],
        ),
        Command::Assignment(a) => sh2_call(
            "setVar",
            vec![str_lit(&a.variable), word_to_expr_quoted(&a.value)],
        ),
        Command::ShoptCommand(s) => sh2_call(
            "shopt",
            vec![
                str_lit(&s.option),
                Expr::Literal {
                    value: serde_json::Value::Bool(s.enable),
                    raw: None,
                },
            ],
        ),
        Command::And(l, r) => Expr::LogicalExpression {
            operator: "&&",
            left: Box::new(command_to_expr(l)),
            right: Box::new(command_to_expr(r)),
        },
        Command::Or(l, r) => Expr::LogicalExpression {
            operator: "||",
            left: Box::new(command_to_expr(l)),
            right: Box::new(command_to_expr(r)),
        },
        Command::Not(c) => Expr::UnaryExpression {
            operator: "!",
            argument: Box::new(command_to_expr(c)),
            prefix: true,
        },
        Command::Return(w) => sh2_call(
            "return",
            match w {
                Some(word) => vec![word_to_expr(word)],
                None => vec![Expr::Literal {
                    value: serde_json::Value::Null,
                    raw: None,
                }],
            },
        ),
        Command::Break(_) => sh2_call("break", vec![]),
        Command::Continue(_) => sh2_call("continue", vec![]),
        other => unsupported(other),
    }
}

/// `case` → a standard `SwitchStatement`. The discriminant is
/// `sh2.caseMatch(value, patterns)` — a runtime helper that performs
/// shell glob matching (bash's `case` uses globs, not JS `===`) and
/// returns the first matching pattern; the `switch` then dispatches on
/// the pattern string. A `*)` pattern is an ordinary glob that matches
/// everything, so it naturally acts as the default case. Each clause gets
/// a trailing `break` (shell `;;`).
fn case_to_switch(c: &CaseStatement) -> Stmt {
    let mut cases: Vec<SwitchCase> = Vec::new();
    for clause in &c.cases {
        let mut consequent: Vec<Stmt> =
            clause.body.iter().filter_map(stmt_for_command).collect();
        consequent.push(Stmt::BreakStatement { label: None });
        for pat in &clause.patterns {
            cases.push(SwitchCase {
                type_: "SwitchCase",
                test: Some(word_to_expr(pat)),
                consequent: consequent.clone(),
            });
        }
    }
    let patterns: Vec<Expr> = c
        .cases
        .iter()
        .flat_map(|cl| cl.patterns.iter())
        .map(word_to_expr)
        .collect();
    Stmt::SwitchStatement {
        discriminant: sh2_call(
            "caseMatch",
            vec![
                word_to_expr(&c.word),
                Expr::ArrayExpression {
                    elements: patterns.into_iter().map(Some).collect(),
                },
            ],
        ),
        cases,
    }
}

/// Wrap a lowered command with its redirections:
/// `sh2.redirect(() => <cmd>, [ {fd, mode, target, interpolate?}, ... ])`.
/// The closure defers execution so the runtime can set up fds first.
fn apply_redirects(inner: Expr, redirects: &[Redirect]) -> Expr {
    if redirects.is_empty() {
        return inner;
    }
    async_sh2_call(
        "redirect",
        vec![
            arrow(vec![], ArrowBody::Expr(Box::new(inner))),
            Expr::ArrayExpression {
                elements: redirects.iter().map(|r| Some(redirect_spec(r))).collect(),
            },
        ],
    )
}

fn redirect_spec(r: &Redirect) -> Expr {
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
        // Process substitution needs a real subprocess; the executor throws
        // on mode "unsupported" until it is lowered.
        RedirectOperator::ProcessSubstitutionInput(_) => ("unsupported", 0),
        RedirectOperator::ProcessSubstitutionOutput(_) => ("unsupported", 0),
    };
    let target: Expr = match &r.operator {
        RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => {
            str_lit(r.heredoc_body.as_deref().unwrap_or(""))
        }
        _ => word_to_expr(&r.target),
    };
    let mut props = vec![
        prop(
            "fd",
            Expr::Literal {
                value: serde_json::Value::from(r.fd.unwrap_or(default_fd)),
                raw: None,
            },
        ),
        prop("mode", str_lit(mode)),
        prop("target", target),
    ];
    if matches!(
        r.operator,
        RedirectOperator::Heredoc | RedirectOperator::HeredocTabs
    ) {
        props.push(prop(
            "interpolate",
            Expr::Literal {
                value: serde_json::Value::Bool(!r.heredoc_quoted),
                raw: None,
            },
        ));
    }
    Expr::ObjectExpression { properties: props }
}

/// `sh2.exec(name, args[, env])` — optional third arg carries command-scoped
/// env vars (`VAR=x cmd`).
fn exec_call_with_env(name: &Word, args: &[Word], env: &BTreeMap<String, Word>, redirects: &[Redirect]) -> Expr {
    let mut call_args = vec![word_to_expr(name), args_array(args)];
    if !env.is_empty() {
        call_args.push(Expr::ObjectExpression {
            properties: env
                .iter()
                .map(|(k, v)| prop(k, word_to_expr_quoted(v)))
                .collect(),
        });
    }
    apply_redirects(async_sh2_call("exec", call_args), redirects)
}

/// Arrow helper: sets `expression` correctly for block vs expression bodies.
/// All arrows are async (closure bodies may await nested calls; the runtime
/// awaits every closure it invokes).
fn arrow(params: Vec<Expr>, body: ArrowBody) -> Expr {
    let expression = matches!(body, ArrowBody::Expr(_));
    Expr::ArrowFunctionExpression {
        params,
        body,
        expression,
        r#async: true,
    }
}

/// Body for closures wrapping a command: a `Block` lowers to a block body,
/// simple commands to an expression body; any other compound command (while,
/// for, case, if, subshell, ...) becomes a block body too — otherwise it would
/// fall through `command_to_expr` to `sh2.unsupported` when used as a pipeline
/// stage or subshell/background body.
fn command_arrow_body(c: &Command) -> ArrowBody {
    match c {
        Command::Block(b) => ArrowBody::Block(Box::new(block_stmt(&b.commands))),
        Command::Simple(_)
        | Command::BuiltinCommand(_)
        | Command::TestExpression(_)
        | Command::Redirect(_)
        | Command::Pipeline(_)
        | Command::And(_, _)
        | Command::Or(_, _)
        | Command::Not(_)
        | Command::Assignment(_)
        | Command::ShoptCommand(_) => ArrowBody::Expr(Box::new(command_to_expr(c))),
        other => ArrowBody::Block(Box::new(Stmt::BlockStatement {
            body: vec![stmt_for_command(other)
                .unwrap_or_else(|| Stmt::BlockStatement { body: vec![] })],
        })),
    }
}

fn prop(key: &str, value: Expr) -> Property {
    Property {
        type_: "Property",
        key: Expr::Identifier {
            name: key.to_string(),
        },
        value,
        kind: "init",
        computed: false,
        shorthand: false,
    }
}

/// Not-yet-lowered construct → `sh2.unsupported("<what>")` so output stays
/// valid, deterministic ESTree. The structural gate flags these.
fn unsupported(cmd: &Command) -> Expr {
    let what = variant_name(cmd);
    sh2_call(
        "unsupported",
        vec![str_lit(&format!("{what}: not yet lowered to ESTree"))],
    )
}

fn variant_name(cmd: &Command) -> &'static str {
    match cmd {
        Command::Simple(_) => "simple",
        Command::BuiltinCommand(_) => "builtin",
        Command::ShoptCommand(_) => "shopt",
        Command::TestExpression(_) => "test",
        Command::Pipeline(_) => "pipeline",
        Command::And(_, _) => "and",
        Command::Or(_, _) => "or",
        Command::If(_) => "if",
        Command::Case(_) => "case",
        Command::While(_) => "while",
        Command::For(_) => "for",
        Command::Function(_) => "function",
        Command::Subshell(_) => "subshell",
        Command::Background(_) => "background",
        Command::Block(_) => "block",
        Command::Redirect(_) => "redirect",
        Command::Assignment(_) => "assignment",
        Command::CStyleFor(_) => "c-style-for",
        Command::Not(_) => "not",
        Command::Break(_) => "break",
        Command::Continue(_) => "continue",
        Command::Return(_) => "return",
        Command::BlankLine => "blank",
    }
}

fn exec_call(name: &Word, args: &[Word]) -> Expr {
    exec_call_with_env(name, args, &BTreeMap::new(), &[])
}

fn args_array(args: &[Word]) -> Expr {
    Expr::ArrayExpression {
        elements: args.iter().map(|w| Some(word_to_expr(w))).collect(),
    }
}

/// Like `word_to_expr` but command substitutions do NOT word-split
/// (assignment values, env values — bash keeps internal spaces there).
fn word_to_expr_quoted(word: &Word) -> Expr {
    match word {
        Word::CommandSubstitution(cmd, _) => Expr::AwaitExpression {
            argument: Box::new(sh2_call(
                "capture",
                vec![arrow(vec![], command_arrow_body(cmd))],
            )),
        },
        _ => word_to_expr(word),
    }
}

fn word_to_expr(word: &Word) -> Expr {
    match word {
        Word::Literal(s, _) => Expr::Literal {
            value: serde_json::Value::String(s.clone()),
            raw: None,
        },
        Word::Variable(name, _, _) => sh2_call("getVar", vec![str_lit(name)]),
        Word::CommandSubstitution(cmd, _) => Expr::AwaitExpression {
            // Unquoted $(...) / `...`: bash word-splits the captured output, so
            // the runtime returns an ARRAY of words (exec flattens arrays).
            argument: Box::new(sh2_call(
                "captureWords",
                vec![arrow(vec![], command_arrow_body(cmd))],
            )),
        },
        Word::StringInterpolation(interp, _) => template_from_parts(&interp.parts),
        other => sh2_call("unsupported", vec![str_lit(&other.to_string())]),
    }
}

fn template_from_parts(parts: &[StringPart]) -> Expr {
    let mut quasis = Vec::new();
    let mut expressions = Vec::new();
    let mut raw = String::new();
    for part in parts {
        match part {
            StringPart::Literal(s) => raw.push_str(s),
            _ => {
                quasis.push(quasi_element(&mut raw, false));
                expressions.push(part_to_expr(part));
            }
        }
    }
    quasis.push(quasi_element(&mut raw, true));
    Expr::TemplateLiteral { quasis, expressions }
}

fn quasi_element(raw: &mut String, tail: bool) -> TemplateElement {
    let r = std::mem::take(raw);
    TemplateElement {
        type_: "TemplateElement",
        value: TemplateElementValue {
            raw: r.clone(),
            cooked: Some(r),
        },
        tail,
    }
}

fn part_to_expr(part: &StringPart) -> Expr {
    match part {
        StringPart::Literal(_) => unreachable!("Literal parts are handled in template_from_parts"),
        StringPart::Variable(name) => sh2_call("getVar", vec![str_lit(name)]),
        StringPart::CommandSubstitution(cmd) => Expr::AwaitExpression {
            argument: Box::new(sh2_call(
                "capture",
                vec![arrow(vec![], command_arrow_body(cmd))],
            )),
        },
        other => sh2_call("unsupported", vec![str_lit(&format!("{other:?}"))]),
    }
}

// ── sh2.* namespace helpers ─────────────────────────────────────────

fn sh2_member(name: &str) -> Expr {
    Expr::MemberExpression {
        object: Box::new(Expr::Identifier {
            name: "sh2".to_string(),
        }),
        property: Box::new(Expr::Identifier {
            name: name.to_string(),
        }),
        computed: false,
        optional: false,
    }
}

fn sh2_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::CallExpression {
        callee: Box::new(sh2_member(name)),
        arguments: args,
        optional: false,
    }
}

fn str_lit(s: &str) -> Expr {
    Expr::Literal {
        value: serde_json::Value::String(s.to_string()),
        raw: None,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn to_json(input: &str) -> String {
        let commands = Parser::new(input).parse().unwrap();
        serde_json::to_string(&ast_to_estree(&commands)).unwrap()
    }

    #[test]
    fn echo_lowers_to_exec_call() {
        let json = to_json("echo hello world");
        assert!(json.contains("\"type\":\"Program\""));
        assert!(json.contains("\"name\":\"exec\""));
        assert!(json.contains("hello"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn assignment_lowers_to_setvar() {
        let json = to_json("x=42");
        assert!(json.contains("\"name\":\"setVar\""));
        assert!(json.contains("x"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn if_then_else_lowers_to_if_statement() {
        let json = to_json("if [ -f /tmp/x ]; then echo yes; else echo no; fi");
        assert!(json.contains("\"type\":\"IfStatement\""));
        assert!(json.contains("\"name\":\"test\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn variable_and_interpolation() {
        let json = to_json("name=world\necho \"Hello $name\"");
        assert!(json.contains("\"type\":\"TemplateLiteral\""));
        assert!(json.contains("\"name\":\"getVar\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn pipeline_lowers_to_pipeline_call() {
        let json = to_json("ls | grep foo");
        assert!(json.contains("\"name\":\"pipeline\""));
        assert!(json.contains("\"type\":\"ArrowFunctionExpression\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn command_substitution_uses_await_capture() {
        // Unquoted $(...) word-splits: captureWords returns an arg array.
        let json = to_json("echo $(date)");
        assert!(json.contains("\"type\":\"AwaitExpression\""));
        assert!(json.contains("\"name\":\"captureWords\""));
        assert!(!json.contains("unsupported"));
        // Quoted "$(...)" stays a plain template capture (no word splitting).
        let json2 = to_json("echo \"$(date)\"");
        assert!(json2.contains("\"name\":\"capture\""));
        assert!(!json2.contains("captureWords"));
    }

    #[test]
    fn unsupported_constructs_are_marked() {
        // Arithmetic words are not yet lowered → must be flagged.
        let json = to_json("echo $((1+2))");
        assert!(json.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn case_lowers_to_switch_statement() {
        let json = to_json("case $x in a) echo a;; *) echo other;; esac");
        assert!(json.contains("\"type\":\"SwitchStatement\""));
        assert!(json.contains("\"type\":\"SwitchCase\""));
        assert!(json.contains("\"name\":\"caseMatch\""));
        assert!(json.contains("\"type\":\"BreakStatement\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn redirect_lowers_to_redirect_call() {
        let json = to_json("echo hi > out.txt");
        assert!(json.contains("\"name\":\"redirect\""));
        // Property keys serialize as {key: Identifier{name}, value: Literal}.
        assert!(json.contains("\"name\":\"mode\""));
        assert!(json.contains("\"value\":\"w\""));
        assert!(json.contains("\"name\":\"fd\""));
        assert!(json.contains("\"value\":1"));
        assert!(json.contains("\"type\":\"ObjectExpression\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn stderr_redirect_uses_fd_2() {
        let json = to_json("ls 2> err.txt");
        assert!(json.contains("\"value\":\"w\""));
        assert!(json.contains("\"value\":2"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn heredoc_lowers_to_redirect_with_body() {
        let json = to_json("cat << 'EOF'\nhi there\nEOF");
        assert!(json.contains("\"value\":\"heredoc\""));
        assert!(json.contains("hi there"));
        assert!(json.contains("\"value\":false"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn function_lowers_to_define() {
        let json = to_json("greet() { echo hi; }");
        assert!(json.contains("\"name\":\"define\""));
        assert!(json.contains("greet"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn subshell_lowers_to_subshell_call() {
        let json = to_json("(echo hi)");
        assert!(json.contains("\"name\":\"subshell\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn background_lowers_to_background_call() {
        let json = to_json("sleep 1 &");
        assert!(json.contains("\"name\":\"background\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn env_var_prefix_is_exec_env_arg() {
        let json = to_json("FOO=bar echo hi");
        assert!(json.contains("\"name\":\"exec\""));
        assert!(json.contains("FOO"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn compound_stages_lower_to_block_bodies() {
        // A while loop as a pipeline stage must not fall through to
        // `sh2.unsupported` — it becomes a block-bodied arrow.
        let json = to_json("yes | while read l; do echo $l; done | head -2");
        assert!(json.contains("\"name\":\"pipeline\""));
        assert!(json.contains("\"type\":\"BlockStatement\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn control_flow_as_expression_operand() {
        let json = to_json("true && return");
        assert!(json.contains("\"name\":\"return\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn shopt_and_cstyle_for_lower() {
        let json = to_json("shopt -s extglob");
        assert!(json.contains("\"name\":\"shopt\""));
        assert!(!json.contains("unsupported"));
        let json2 = to_json("for ((i=0; i<3; i++)); do echo $i; done");
        assert!(json2.contains("\"name\":\"cstyleFor\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn deterministic_output() {
        let input = "x=1\nif [ -f /tmp/x ]; then echo $x; fi\nls | wc -l";
        let commands = Parser::new(input).parse().unwrap();
        let a = serde_json::to_string(&ast_to_estree(&commands)).unwrap();
        let b = serde_json::to_string(&ast_to_estree(&commands)).unwrap();
        assert_eq!(a, b);
    }
}
