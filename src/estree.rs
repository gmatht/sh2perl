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

// ── Entry: shell AST → ESTree via the ShIR (PLAN.md §3) ───────────
// The lowering itself lives in src/shir.rs (ast_to_ir + shir_to_estree);
// this module only owns the ESTree node model + the sh2.* helpers.

/// Lower a parsed shell program to an ESTree `Program` (via the ShIR).
pub fn ast_to_estree(commands: &[Command]) -> Program {
    let ir = crate::shir::ast_to_ir(commands);
    crate::shir::shir_to_estree(&ir)
}

/// Convenience: lower + serialize (deterministic, compact JSON).
pub fn ast_to_estree_json(commands: &[Command]) -> Result<String, serde_json::Error> {
    let ir = crate::shir::ast_to_ir(commands);
    crate::shir::shir_to_estree_json(&ir)
}

// ── sh2.* namespace helpers ─────────────────────────────────────────

pub(crate) fn sh2_member(name: &str) -> Expr {
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

pub(crate) fn sh2_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::CallExpression {
        callee: Box::new(sh2_member(name)),
        arguments: args,
        optional: false,
    }
}

pub(crate) fn str_lit(s: &str) -> Expr {
    Expr::Literal {
        value: serde_json::Value::String(s.to_string()),
        raw: None,
    }
}

pub(crate) fn prop(key: &str, value: Expr) -> Property {
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

pub(crate) fn quasi_element(raw: &mut String, tail: bool) -> TemplateElement {
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
        // Regression guard: complex array/param/arith input must contain NO
        // sh2.unsupported (every construct the parser emits is lowered).
        let json = to_json(
            "numbers=(1 2 3)\necho ${#numbers[@]} ${numbers[1]} ${numbers[@]:1:2}\necho $((2+3)) ${x:-d}",
        );
        assert!(!json.contains("\"name\":\"unsupported\""));
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
