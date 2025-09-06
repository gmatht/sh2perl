use crate::ast::*;

/// MIR representation of a for loop with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirForLoop {
    pub variable: String,
    pub items: Vec<Word>,
    pub body: Vec<Command>,
    pub variable_used_after: bool,  // Whether the loop variable is used after the loop
    pub variable_overwritten_before_use: bool,  // Whether the variable is overwritten before being used
}

/// MIR representation of a while loop with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirWhileLoop {
    pub condition: Box<Command>,
    pub body: Vec<Command>,
    pub variables_modified_in_loop: Vec<String>,  // Variables that are modified in the loop body
    pub variables_used_after_loop: Vec<String>,  // Variables that are used after the loop ends
}

/// MIR representation of commands with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum MirCommand {
    Simple(SimpleCommand),
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
    /// Expand a brace expansion into a list of literal words
    fn expand_brace_expansion(expansion: &BraceExpansion) -> Vec<Word> {
        let mut expanded_words = Vec::new();
        let prefix = expansion.prefix.as_deref().unwrap_or("");
        let suffix = expansion.suffix.as_deref().unwrap_or("");
        
        if expansion.items.len() == 1 {
            // Single item expansion
            let item = &expansion.items[0];
            match item {
                BraceItem::Literal(s) => {
                    expanded_words.push(Word::literal(format!("{}{}{}", prefix, s, suffix)));
                }
                BraceItem::Range(range) => {
                    // Handle numeric ranges like {1..5}
                    if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i32>(), range.end.parse::<i32>()) {
                        let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                        let mut current = start_num;
                        while if step > 0 { current <= end_num } else { current >= end_num } {
                            expanded_words.push(Word::literal(format!("{}{}{}", prefix, current, suffix)));
                            current += step;
                        }
                    } else {
                        // Handle character ranges like {a..c}
                        if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
                            let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                            let mut current = start_char as i32;
                            let end_code = end_char as i32;
                            while if step > 0 { current <= end_code } else { current >= end_code } {
                                if let Some(c) = char::from_u32(current as u32) {
                                    expanded_words.push(Word::literal(format!("{}{}{}", prefix, c, suffix)));
                                }
                                current += step;
                            }
                        }
                    }
                }
                BraceItem::Sequence(seq) => {
                    // Handle sequence items like {one,two,three}
                    for item in seq {
                        expanded_words.push(Word::literal(format!("{}{}{}", prefix, item, suffix)));
                    }
                }
            }
        } else {
            // Multiple items - check if this is a sequence (all literals) or cartesian product
            let all_literals = expansion.items.iter().all(|item| matches!(item, BraceItem::Literal(_)));
            
            if all_literals {
                // This is a sequence like {a,b,c} - just expand each literal
                for item in &expansion.items {
                    if let BraceItem::Literal(s) = item {
                        expanded_words.push(Word::literal(format!("{}{}{}", prefix, s, suffix)));
                    }
                }
            } else {
                // Multiple items with ranges - generate cartesian product
                let mut expanded_items: Vec<Vec<String>> = Vec::new();
                for item in &expansion.items {
                    let mut item_expansions = Vec::new();
                    match item {
                        BraceItem::Literal(s) => {
                            item_expansions.push(s.clone());
                        }
                        BraceItem::Range(range) => {
                            // Handle numeric ranges
                            if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i32>(), range.end.parse::<i32>()) {
                                let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                                let mut current = start_num;
                                while if step > 0 { current <= end_num } else { current >= end_num } {
                                    item_expansions.push(current.to_string());
                                    current += step;
                                }
                            } else {
                                // Handle character ranges
                                if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
                                    let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                                    let mut current = start_char as i32;
                                    let end_code = end_char as i32;
                                    while if step > 0 { current <= end_code } else { current >= end_code } {
                                        if let Some(c) = char::from_u32(current as u32) {
                                            item_expansions.push(c.to_string());
                                        }
                                        current += step;
                                    }
                                }
                            }
                        }
                        BraceItem::Sequence(seq) => {
                            item_expansions.extend(seq.clone());
                        }
                    }
                    expanded_items.push(item_expansions);
                }
                
                // Generate cartesian product
                let cartesian = Self::generate_cartesian_product(&expanded_items);
                
                // Add prefix and suffix to each item
                for item in cartesian {
                    expanded_words.push(Word::literal(format!("{}{}{}", prefix, item, suffix)));
                }
            }
        }
        
        expanded_words
    }
    
    /// Generate cartesian product of multiple vectors
    fn generate_cartesian_product(items: &[Vec<String>]) -> Vec<String> {
        if items.is_empty() {
            return vec![];
        }
        if items.len() == 1 {
            return items[0].clone();
        }
        
        let mut result = Vec::new();
        let first = &items[0];
        let rest = Self::generate_cartesian_product(&items[1..]);
        
        for item in first {
            for rest_item in &rest {
                result.push(format!("{}{}", item, rest_item));
            }
        }
        
        result
    }
    
    /// Convert an AST command to a MIR command with optimization information
    pub fn from_ast_command(cmd: &Command) -> MirCommand {
        match cmd {
            Command::Simple(simple_cmd) => {
                // Convert AST words to MIR words and expand brace expansions
                let mut expanded_args = Vec::new();
                for arg in &simple_cmd.args {
                    let mir_word = arg.clone(); // Word is now in AST
                    match &mir_word {
                        Word::BraceExpansion(expansion, _) => {
                            // Expand the brace expansion into multiple literal words
                            expanded_args.extend(Self::expand_brace_expansion(expansion));
                        }
                        _ => {
                            expanded_args.push(mir_word);
                        }
                    }
                }
                
                // Create a new SimpleCommand with expanded arguments
                let expanded_simple_cmd = SimpleCommand {
                    name: simple_cmd.name.clone(),
                    args: expanded_args,
                    redirects: simple_cmd.redirects.clone(),
                    env_vars: simple_cmd.env_vars.clone(),
                    stdout_used: simple_cmd.stdout_used,
                    stderr_used: simple_cmd.stderr_used,
                };
                
                MirCommand::Simple(expanded_simple_cmd)
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
                // Convert AST ForLoop to MIR ForLoop with optimization analysis
                let body_commands: Vec<Command> = for_loop.body.commands.clone();
                let variable_used_after = Self::is_variable_used_after_for_loop(&for_loop.variable, &body_commands);
                let variable_overwritten_before_use = Self::is_variable_overwritten_before_use(&for_loop.variable, &body_commands);
                
                // Convert AST words to MIR words and expand brace expansions in the for loop items
                let mut expanded_items = Vec::new();
                for item in &for_loop.items {
                    let mir_word = item.clone(); // Word is now in AST
                    match &mir_word {
                        Word::BraceExpansion(expansion, _) => {
                            // Expand the brace expansion into multiple literal words
                            expanded_items.extend(Self::expand_brace_expansion(expansion));
                        }
                        _ => {
                            expanded_items.push(mir_word);
                        }
                    }
                }
                
                MirCommand::For(MirForLoop {
                    variable: for_loop.variable.clone(),
                    items: expanded_items,
                    body: body_commands,
                    variable_used_after,
                    variable_overwritten_before_use,
                })
            },
            Command::While(while_loop) => {
                // Convert AST WhileLoop to MIR WhileLoop with optimization analysis
                let body_commands: Vec<Command> = while_loop.body.commands.clone();
                let variables_modified_in_loop = Self::get_variables_modified_in_loop(&body_commands);
                let variables_used_after_loop = Self::get_variables_used_after_loop(&body_commands);
                
                MirCommand::While(MirWhileLoop {
                    condition: while_loop.condition.clone(),
                    body: body_commands,
                    variables_modified_in_loop,
                    variables_used_after_loop,
                })
            },
            Command::If(if_stmt) => MirCommand::If(if_stmt.clone()),
            Command::Case(case_stmt) => MirCommand::Case(case_stmt.clone()),
            Command::Function(func) => MirCommand::Function(func.clone()),
            Command::Subshell(cmd) => MirCommand::Subshell(Box::new(MirCommand::from_ast_command(cmd))),
            Command::Background(cmd) => {
                MirCommand::Background(Box::new(MirCommand::from_ast_command(cmd)))
            },
            // Handle other command types by converting them to simple MIR representations
            _ => {
                // For now, we'll create a simple representation for unsupported command types
                // In a full implementation, you'd want to handle each type appropriately
                MirCommand::Simple(SimpleCommand {
                    name: Word::literal("UNSUPPORTED".to_string()),
                    args: vec![],
                    redirects: vec![],
                    env_vars: std::collections::HashMap::new(),
                    stdout_used: true,
                    stderr_used: true,
                })
            }
        }
    }
    
    /// Check if a variable is used after a for loop
    fn is_variable_used_after_for_loop(variable: &str, commands: &[Command]) -> bool {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the entire script to see if the variable is used after the loop
        // For now, we'll return false as a placeholder
        false
    }
    
    /// Check if a variable is overwritten before being used
    fn is_variable_overwritten_before_use(variable: &str, commands: &[Command]) -> bool {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the entire script to see if the variable is overwritten before use
        // For now, we'll return false as a placeholder
        false
    }
    
    /// Get variables that are modified in a loop body
    fn get_variables_modified_in_loop(commands: &[Command]) -> Vec<String> {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the commands to find variable assignments
        // For now, we'll return an empty vector as a placeholder
        Vec::new()
    }
    
    /// Get variables that are used after a loop
    fn get_variables_used_after_loop(commands: &[Command]) -> Vec<String> {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the entire script to see which variables are used after the loop
        // For now, we'll return an empty vector as a placeholder
        Vec::new()
    }
}

