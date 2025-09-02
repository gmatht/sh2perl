use std::collections::HashMap;

// Forward declaration to avoid circular dependency
pub use crate::mir::Word;

/// Represents a span of source code with start/end positions and original text
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub original_text: String,
}

impl SourceSpan {
    pub fn new(start: usize, end: usize, original_text: String) -> Self {
        Self { start, end, original_text }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Command {
    Simple(SimpleCommand),
    BuiltinCommand(BuiltinCommand),
    ShoptCommand(ShoptCommand),
    TestExpression(TestExpression),
    Pipeline(Pipeline),
    And(Box<Command>, Box<Command>),      // left && right
    Or(Box<Command>, Box<Command>),       // left || right
    If(IfStatement),
    Case(CaseStatement),
    While(WhileLoop),
    For(ForLoop),
    Function(Function),
    Subshell(Box<Command>),
    Background(Box<Command>),
    Block(Block),
    Redirect(RedirectCommand),
    Break(Option<String>),      // Optional loop level
    Continue(Option<String>),   // Optional loop level
    Return(Option<Word>),       // Optional return value
    BlankLine,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SimpleCommand {
    pub name: Word,
    pub args: Vec<Word>,
    pub redirects: Vec<Redirect>,
    pub env_vars: HashMap<String, Word>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BuiltinCommand {
    pub name: String,
    pub args: Vec<Word>,
    pub redirects: Vec<Redirect>,
    pub env_vars: HashMap<String, Word>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ShoptCommand {
    pub option: String,
    pub enable: bool, // true for -s (set), false for -u (unset)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Pipeline {
    pub commands: Vec<Command>,
    pub source_text: Option<String>, // Original bash command text for comments
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IfStatement {
    pub condition: Box<Command>,
    pub then_branch: Box<Command>,
    pub else_branch: Option<Box<Command>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaseStatement {
    pub word: Word,
    pub cases: Vec<CaseClause>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaseClause {
    pub patterns: Vec<Word>,
    pub body: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WhileLoop {
    pub condition: Box<Command>,
    pub body: Block,
    pub variables_modified_in_loop: Vec<String>,  // Variables that are modified in the loop body
    pub variables_used_after_loop: Vec<String>,  // Variables that are used after the loop ends
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForLoop {
    pub variable: String,
    pub items: Vec<Word>,
    pub body: Block,
    pub variable_used_after: bool,  // Whether the loop variable is used after the loop
    pub variable_overwritten_before_use: bool,  // Whether the variable is overwritten before being used
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<String>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Block {
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RedirectCommand {
    pub command: Box<Command>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Redirect {
    pub fd: Option<i32>,
    pub operator: RedirectOperator,
    pub target: Word,
    pub heredoc_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum RedirectOperator {
    Input,     // <
    Output,    // >
    Append,    // >>
    InputOutput, // <>
    Heredoc,   // <<
    HeredocTabs, // <<-
    HereString, // <<<
    ProcessSubstitutionInput(Box<Command>),  // <(command)
    ProcessSubstitutionOutput(Box<Command>), // >(command)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TestExpression {
    pub expression: String,
    pub modifiers: TestModifiers,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TestModifiers {
    pub extglob: bool,
    pub nocasematch: bool,
    pub globstar: bool,
    pub nullglob: bool,
    pub failglob: bool,
    pub dotglob: bool,
}

impl Default for TestModifiers {
    fn default() -> Self {
        Self {
            extglob: false,
            nocasematch: false,
            globstar: false,
            nullglob: false,
            failglob: false,
            dotglob: false,
        }
    }
}