use crate::ast::*;
// use crate::ast_annotations::*; // TODO: Re-implement annotations

/// Simple MIR that wraps AST structures with analysis information
/// This approach doesn't change existing structures, just adds analysis on top

/// MIR representation of a word - wraps AST Word with analysis information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirWord {
    pub ast_word: Word,
    // pub bounds: Option<Bounds>, // TODO: Re-implement bounds
    // pub variable_analysis: Option<VariableAnalysis>, // TODO: Re-implement variable analysis
}

impl MirWord {
    /// Create a MIR word from an AST word with default analysis
    pub fn from_ast_word(ast_word: Word) -> Self {
        MirWord {
            ast_word,
            // bounds: None, // TODO: Re-implement bounds
            // variable_analysis: None, // TODO: Re-implement variable analysis
        }
    }
    
    // /// Create a MIR word with specific bounds
    // pub fn with_bounds(ast_word: Word, bounds: Bounds) -> Self {
    //     MirWord {
    //         ast_word,
    //         bounds: Some(bounds),
    //         variable_analysis: None,
    //     }
    // }
    
    /// Get the underlying AST word
    pub fn ast_word(&self) -> &Word {
        &self.ast_word
    }
    
    // /// Get the bounds information
    // pub fn bounds(&self) -> Option<&Bounds> {
    //     self.bounds.as_ref()
    // }
    
    // /// Update the bounds
    // pub fn set_bounds(&mut self, bounds: Bounds) {
    //     self.bounds = Some(bounds);
    // }
}

impl std::fmt::Display for MirWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ast_word)
    }
}

impl std::ops::Deref for MirWord {
    type Target = Word;
    
    fn deref(&self) -> &Self::Target {
        &self.ast_word
    }
}

impl PartialEq<Word> for MirWord {
    fn eq(&self, other: &Word) -> bool {
        &self.ast_word == other
    }
}

impl PartialEq<str> for MirWord {
    fn eq(&self, other: &str) -> bool {
        self.ast_word == other
    }
}

impl PartialEq<&str> for MirWord {
    fn eq(&self, other: &&str) -> bool {
        self.ast_word == *other
    }
}

impl PartialEq<String> for MirWord {
    fn eq(&self, other: &String) -> bool {
        self.ast_word == other.as_str()
    }
}

/// MIR representation of a simple command with analysis information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirSimpleCommand {
    pub name: MirWord,
    pub args: Vec<MirWord>,
    pub redirects: Vec<Redirect>,
    pub env_vars: std::collections::HashMap<String, MirWord>,
    pub stdout_used: bool,
    pub stderr_used: bool,
}

/// MIR representation of a for loop with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirForLoop {
    pub variable: String,
    pub items: Vec<MirWord>,
    pub body: Vec<Command>,
    // pub loop_analysis: Option<LoopAnalysis>, // TODO: Re-implement loop analysis
}

/// MIR representation of a while loop with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirWhileLoop {
    pub condition: Box<Command>,
    pub body: Vec<Command>,
    // pub loop_analysis: Option<LoopAnalysis>, // TODO: Re-implement loop analysis
}

/// MIR representation of commands with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum MirCommand {
    Simple(MirSimpleCommand),
    Pipeline(Pipeline),
    Redirect(RedirectCommand),
    And(Box<MirCommand>, Box<MirCommand>),
    Or(Box<MirCommand>, Box<MirCommand>),
    For(MirForLoop),
    While(MirWhileLoop),
    If(IfStatement),
    Case(CaseStatement),
    Function(Function),
    Subshell(Box<MirCommand>),
    Background(Box<MirCommand>),
}

impl MirCommand {
    /// Convert an AST command to a MIR command with analysis information
    pub fn from_ast_command(cmd: &Command) -> MirCommand {
        match cmd {
            Command::Simple(simple_cmd) => {
                // Convert AST words to MIR words
                let mir_name = MirWord::from_ast_word(simple_cmd.name.clone());
                let mir_args: Vec<MirWord> = simple_cmd.args.iter()
                    .map(|arg| MirWord::from_ast_word(arg.clone()))
                    .collect();
                let mir_env_vars: std::collections::HashMap<String, MirWord> = simple_cmd.env_vars.iter()
                    .map(|(k, v)| (k.clone(), MirWord::from_ast_word(v.clone())))
                    .collect();
                
                MirCommand::Simple(MirSimpleCommand {
                    name: mir_name,
                    args: mir_args,
                    redirects: simple_cmd.redirects.clone(),
                    env_vars: mir_env_vars,
                    stdout_used: simple_cmd.stdout_used,
                    stderr_used: simple_cmd.stderr_used,
                })
            },
            Command::Pipeline(pipeline) => MirCommand::Pipeline(pipeline.clone()),
            Command::Redirect(redirect) => MirCommand::Redirect(redirect.clone()),
            Command::And(left, right) => {
                MirCommand::And(
                    Box::new(MirCommand::from_ast_command(left)),
                    Box::new(MirCommand::from_ast_command(right))
                )
            },
            Command::Or(left, right) => {
                MirCommand::Or(
                    Box::new(MirCommand::from_ast_command(left)),
                    Box::new(MirCommand::from_ast_command(right))
                )
            },
            Command::For(for_loop) => {
                let mir_items: Vec<MirWord> = for_loop.items.iter()
                    .map(|item| MirWord::from_ast_word(item.clone()))
                    .collect();
                
                MirCommand::For(MirForLoop {
                    variable: for_loop.variable.clone(),
                    items: mir_items,
                    body: for_loop.body.commands.clone(),
                    // loop_analysis: None, // TODO: Re-implement loop analysis
                })
            },
            Command::While(while_loop) => {
                MirCommand::While(MirWhileLoop {
                    condition: while_loop.condition.clone(),
                    body: while_loop.body.commands.clone(),
                    // loop_analysis: None, // TODO: Re-implement loop analysis
                })
            },
            Command::If(if_stmt) => MirCommand::If(if_stmt.clone()),
            Command::Case(case_stmt) => MirCommand::Case(case_stmt.clone()),
            Command::Function(func) => MirCommand::Function(func.clone()),
            Command::Subshell(cmd) => {
                MirCommand::Subshell(Box::new(MirCommand::from_ast_command(cmd)))
            },
            Command::Background(cmd) => {
                MirCommand::Background(Box::new(MirCommand::from_ast_command(cmd)))
            },
            // TODO: Handle other command types
            _ => {
                // For now, create a simple command as fallback
                MirCommand::Simple(MirSimpleCommand {
                    name: MirWord::from_ast_word(Word::literal("UNSUPPORTED".to_string())),
                    args: vec![],
                    redirects: vec![],
                    env_vars: std::collections::HashMap::new(),
                    stdout_used: false,
                    stderr_used: false,
                })
            },
        }
    }
}
