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
        body: Box<Expr>,
        expression: bool,
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
        Command::Simple(sc) => Stmt::ExpressionStatement {
            expression: exec_call(&sc.name, &sc.args),
        },
        Command::BuiltinCommand(bc) => Stmt::ExpressionStatement {
            expression: exec_call(&Word::Literal(bc.name.clone(), None), &bc.args),
        },
        // v0: operator (e.g. +=) not yet represented; structural gate flags
        // non-Assign operators when they matter.
        Command::Assignment(a) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "setVar",
                vec![str_lit(&a.variable), word_to_expr(&a.value)],
            ),
        },
        Command::If(if_stmt) => Stmt::IfStatement {
            test: command_to_expr(&if_stmt.condition),
            consequent: Box::new(body_stmt(&if_stmt.then_branch)),
            alternate: if_stmt.else_branch.as_ref().map(|b| Box::new(body_stmt(b))),
        },
        Command::While(w) => Stmt::WhileStatement {
            test: {
                let t = command_to_expr(&w.condition);
                if w.is_until {
                    Expr::UnaryExpression {
                        operator: "!",
                        argument: Box::new(t),
                        prefix: true,
                    }
                } else {
                    t
                }
            },
            body: Box::new(block_stmt(&w.body.commands)),
        },
        Command::For(f) => Stmt::ForOfStatement {
            left: Box::new(Stmt::VariableDeclaration {
                declarations: vec![VariableDeclarator {
                    type_: "VariableDeclarator",
                    id: Expr::Identifier {
                        name: f.variable.clone(),
                    },
                    init: None,
                }],
                kind: "let",
            }),
            right: Expr::ArrayExpression {
                elements: f.items.iter().map(|w| Some(word_to_expr(w))).collect(),
            },
            body: Box::new(block_stmt(&f.body.commands)),
        },
        Command::Block(b) => Stmt::BlockStatement {
            body: b.commands.iter().filter_map(stmt_for_command).collect(),
        },
        Command::Pipeline(p) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "pipeline",
                vec![Expr::ArrayExpression {
                    elements: p
                        .commands
                        .iter()
                        .map(|c| {
                            Some(Expr::ArrowFunctionExpression {
                                params: vec![],
                                body: Box::new(command_to_expr(c)),
                                expression: true,
                            })
                        })
                        .collect(),
                }],
            ),
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
        Command::Break(lvl) => Stmt::BreakStatement { label: lvl.clone() },
        Command::Continue(lvl) => Stmt::ContinueStatement { label: lvl.clone() },
        Command::Return(w) => Stmt::ReturnStatement {
            argument: w.as_ref().map(word_to_expr),
        },
        other => Stmt::ExpressionStatement {
            expression: unsupported(other),
        },
    })
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

/// Lower a command to a boolean/expression context (conditions, pipeline stages).
fn command_to_expr(cmd: &Command) -> Expr {
    match cmd {
        Command::TestExpression(t) => sh2_call("test", vec![str_lit(&t.expression)]),
        Command::Simple(sc) => exec_call(&sc.name, &sc.args),
        Command::BuiltinCommand(bc) => {
            exec_call(&Word::Literal(bc.name.clone(), None), &bc.args)
        }
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
        other => unsupported(other),
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
    sh2_call(
        "exec",
        vec![
            word_to_expr(name),
            Expr::ArrayExpression {
                elements: args.iter().map(|w| Some(word_to_expr(w))).collect(),
            },
        ],
    )
}

fn word_to_expr(word: &Word) -> Expr {
    match word {
        Word::Literal(s, _) => Expr::Literal {
            value: serde_json::Value::String(s.clone()),
            raw: None,
        },
        Word::Variable(name, _, _) => sh2_call("getVar", vec![str_lit(name)]),
        Word::CommandSubstitution(cmd, _) => Expr::AwaitExpression {
            argument: Box::new(sh2_call("capture", vec![command_to_expr(cmd)])),
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
            argument: Box::new(sh2_call("capture", vec![command_to_expr(cmd)])),
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
        let json = to_json("echo $(date)");
        assert!(json.contains("\"type\":\"AwaitExpression\""));
        assert!(json.contains("\"name\":\"capture\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn unsupported_constructs_are_marked() {
        let json = to_json("case $x in a) echo a;; esac");
        assert!(json.contains("\"name\":\"unsupported\""));
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
