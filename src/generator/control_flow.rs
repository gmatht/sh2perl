use crate::ast::*;
use super::Generator;
use regex::Regex;

pub fn generate_if_statement_impl(generator: &mut Generator, if_stmt: &IfStatement) -> String {
    let mut output = String::new();
    
    // Generate condition
    output.push_str("if (");
    match &*if_stmt.condition {
        Command::Simple(cmd) if cmd.name == "[" || cmd.name == "test" => {
            generator.generate_test_command(cmd, &mut output);
        }
        _ => {
            output.push_str(&generator.generate_command(&if_stmt.condition));
        }
    }
    output.push_str(") {\n");
    
    // Generate then branch
    generator.indent_level += 1;
    
    // Check if the then branch is a single command that doesn't need block wrapping
    match &*if_stmt.then_branch {
        Command::Block(block) if block.commands.len() == 1 => {
            // Single command in block - generate it directly without block wrapper
            // The command will add its own indentation, so we don't add it here
            output.push_str(&generator.generate_command(&block.commands[0]));
        }
        _ => {
            // Multiple commands or complex structure - use block generation
            output.push_str(&generator.indent());
            output.push_str(&generator.generate_command(&if_stmt.then_branch));
        }
    }
    
    generator.indent_level -= 1;
    
    // Generate else branch if present
    if let Some(else_branch) = &if_stmt.else_branch {
        output.push_str(&generator.indent());
        output.push_str("} else {\n");
        generator.indent_level += 1;
        
        // Check if the else branch is a single command that doesn't need block wrapping
        match &**else_branch {
            Command::Block(block) if block.commands.len() == 1 => {
                // Single command in block - generate it directly without block wrapper
                // The command will add its own indentation, so we don't add it here
                output.push_str(&generator.generate_command(&block.commands[0]));
            }
            _ => {
                // Multiple commands or complex structure - use block generation
                output.push_str(&generator.indent());
                output.push_str(&generator.generate_command(else_branch));
            }
        }
        
        generator.indent_level -= 1;
    }
    
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}

pub fn generate_case_statement_impl(generator: &mut Generator, case_stmt: &CaseStatement) -> String {
    let mut output = String::new();
    
    // Convert bash case statement to Perl if/elsif/else
    let mut first_case = true;
    
    for case_clause in &case_stmt.cases {
        if first_case {
            // First case becomes 'if'
            output.push_str("if (");
            first_case = false;
        } else {
            // Subsequent cases become 'elsif'
            output.push_str(&generator.indent());
            output.push_str("} elsif (");
        }
        
        // Handle multiple patterns in a single case clause
        let mut pattern_conditions = Vec::new();
        for pattern in &case_clause.patterns {
            let pattern_str = generator.perl_string_literal(pattern);
            if pattern_str == "\"*\"" {
                // Default case - this should be the last one
                pattern_conditions.push("1".to_string()); // Always true
            } else {
                // Convert bash glob patterns to Perl regex
                let mut perl_pattern = pattern_str.trim_matches('"').to_string();
                perl_pattern = perl_pattern.replace("*", ".*");
                perl_pattern = perl_pattern.replace("?", ".");
                perl_pattern = perl_pattern.replace("[", "\\[");
                perl_pattern = perl_pattern.replace("]", "\\]");
                
                // Create condition: $operation =~ /^pattern$/
                let word_str = generator.word_to_perl(&case_stmt.word);
                
                // Handle positional parameters in case statements
                let processed_word = if word_str.contains("$1") || word_str.contains("$2") || word_str.contains("$3") {
                    // Replace positional parameters with generic names that will be replaced later
                    word_str.replace("$1", "$arg1").replace("$2", "$arg2").replace("$3", "$arg3")
                } else if word_str.contains("$name") {
                    // The word_to_perl converted $1 to $name, but we need $arg1 for parameter replacement
                    word_str.replace("$name", "$arg1")
                } else {
                    word_str
                };
                
                pattern_conditions.push(format!("{} =~ /^{}$/", processed_word, perl_pattern));
            }
        }
        
        // Join multiple patterns with 'or'
        output.push_str(&pattern_conditions.join(" or "));
        output.push_str(") {\n");
        
        generator.indent_level += 1;
        // Generate body commands
        for command in &case_clause.body {
            output.push_str(&generator.indent());
            output.push_str(&generator.generate_command(command));
        }
        generator.indent_level -= 1;
    }
    
    // Close the if/elsif chain
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}

pub fn generate_while_loop_impl(generator: &mut Generator, while_loop: &WhileLoop) -> String {
    let mut output = String::new();
    
    // Check if the while loop condition uses variables that might need initialization
    // This is needed for shell compatibility where loop variables persist
    if let Command::Simple(cmd) = &*while_loop.condition {
        if cmd.name == "[" || cmd.name == "test" {
            // For test commands, check if variables need initialization
            if cmd.args.len() >= 3 {
                // Check both operands for variables that need initialization
                let operand1 = &cmd.args[0];
                let operand2 = &cmd.args[2];
                
                // Initialize first operand if it's a variable
                if let Word::Variable(var_name, _, _) = operand1 {
                    if !generator.declared_locals.contains(var_name) {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${} = 0;\n", var_name));
                        generator.declared_locals.insert(var_name.to_string());
                    }
                    // Mark this variable as used at function level so for loops know to preserve it
                    generator.function_level_vars.insert(var_name.to_string());
                }
                
                // Initialize second operand if it's a variable
                if let Word::Variable(var_name, _, _) = operand2 {
                    if !generator.declared_locals.contains(var_name) {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${} = 0;\n", var_name));
                        generator.declared_locals.insert(var_name.to_string());
                    }
                    // Mark this variable as used at function level so for loops know to preserve it
                    generator.function_level_vars.insert(var_name.to_string());
                }
            }
        }
    } else if let Command::TestExpression(test_expr) = &*while_loop.condition {
        // For test expressions, check if variables are used in the expression
        // and mark them as function-level variables so for loops know to preserve them
        // Extract variable names from the test expression
        let re = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        for cap in re.captures_iter(&test_expr.expression) {
            if let Some(var_name) = cap.get(1) {
                generator.function_level_vars.insert(var_name.as_str().to_string());
            }
        }
    }
    
    // Generate while loop
    output.push_str("while (");
    match &*while_loop.condition {
        Command::Simple(cmd) if cmd.name == "[" || cmd.name == "test" => {
            generator.generate_test_command(cmd, &mut output);
        }
        Command::TestExpression(test_expr) => {
            output.push_str(&generator.generate_test_expression(test_expr));
        }
        _ => {
            output.push_str(&generator.generate_command(&while_loop.condition));
        }
    }
    output.push_str(") {\n");
    
    // Generate body
    generator.indent_level += 1;
    output.push_str(&generator.generate_block_commands(&while_loop.body));
    generator.indent_level -= 1;
    
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}

// Helper function to analyze if a variable is used after a for loop
fn is_variable_used_after_for_loop(commands: &[Command], for_loop_var: &str, for_loop_index: usize) -> bool {
    for (i, command) in commands.iter().enumerate() {
        if i <= for_loop_index {
            continue; // Skip commands before and including the for loop
        }
        
        match command {
            Command::While(while_loop) => {
                // Check if variable is used in while loop condition
                if let Command::TestExpression(test_expr) = &*while_loop.condition {
                    if test_expr.expression.contains(&format!("${}", for_loop_var)) {
                        return true;
                    }
                }
            },
            Command::Simple(cmd) => {
                // Check if variable is used in simple commands
                for arg in &cmd.args {
                    if let Word::Variable(var_name, _, _) = arg {
                        if var_name == for_loop_var {
                            return true;
                        }
                    }
                }
            },
            _ => {
                // For other command types, we could add more analysis here
            }
        }
    }
    false
}

pub fn generate_for_loop_impl(generator: &mut Generator, for_loop: &ForLoop) -> String {
    let mut output = String::new();
    
    // Declare the loop variable outside the loop so it persists after the loop ends
    // But only if it hasn't already been declared and it's not a function-level variable
    let loop_var = &for_loop.variable;
    if !generator.declared_locals.contains(loop_var) && !generator.function_level_vars.contains(loop_var) {
        output.push_str(&generator.indent());
        output.push_str(&format!("my ${};\n", loop_var));
        generator.declared_locals.insert(loop_var.clone());
    }
    
    // Generate for loop using the actual variable name from the AST (with 'my' for lexical scoping)
    // We need to store the last value to mimic shell behavior
    output.push_str(&generator.indent());
    output.push_str(&format!("for my ${} (", for_loop.variable));
    
    // Handle different types of for loop items
    let mut all_items = Vec::new();
    
    for word in &for_loop.items {
        match word {
            Word::StringInterpolation(interp, _) => {
                // Check if this is just a single array variable like "$@" or "$*"
                if interp.parts.len() == 1 {
                    if let StringPart::Variable(var) = &interp.parts[0] {
                        match var.as_str() {
                            "@" => all_items.push("@ARGV".to_string()),  // $@ -> @ARGV (no quotes)
                            "*" => all_items.push("@ARGV".to_string()),  // $* -> @ARGV (no quotes)
                            _ => all_items.push(generator.word_to_perl(word))
                        }
                    } else if let StringPart::ParameterExpansion(pe) = &interp.parts[0] {
                        // Handle ${arr[@]} -> @arr for array iteration or ${!map[@]} -> keys %map for map keys
                        if pe.operator == ParameterExpansionOperator::ArraySlice("@".to_string(), None) {
                            if pe.variable.starts_with('!') {
                                // ${!map[@]} -> keys %map (map keys iteration)
                                let map_name = &pe.variable[1..]; // Remove ! prefix
                                all_items.push(format!("keys %{}", map_name));
                            } else {
                                // ${arr[@]} -> @arr (array iteration)
                                all_items.push(format!("@{}", pe.variable));
                            }
                        } else {
                            all_items.push(generator.word_to_perl(word));
                        }
                    } else {
                        all_items.push(generator.word_to_perl(word));
                    }
                } else {
                    all_items.push(generator.word_to_perl(word));
                }
            }
            Word::BraceExpansion(expansion, _) => {
                // Handle brace expansion directly
                if expansion.items.len() == 1 {
                    match &expansion.items[0] {
                        BraceItem::Range(range) => {
                            // Convert {1..5} to Perl range syntax (1..5)
                            if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
                                let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
                                if step == 1 {
                                    // Simple range: 1..5 - use constants for magic numbers
                                    if end_num > 2 {
                                        // Use constant for magic numbers > 2
                                        let const_name = format!("MAX_LOOP_{}", end_num);
                                        all_items.push(format!("{}..{}", start_num, const_name));
                                    } else {
                                        all_items.push(format!("{}..{}", start_num, end_num));
                                    }
                                } else {
                                    // Step range: use list with step
                                    let mut values = Vec::new();
                                    let mut current = start_num;
                                    if step > 0 {
                                        while current <= end_num {
                                            values.push(current.to_string());
                                            current += step;
                                        }
                                    } else {
                                        while current >= end_num {
                                            values.push(current.to_string());
                                            current += step;
                                        }
                                    }
                                    all_items.push(format!("({})", values.join(", ")));
                                }
                            } else {
                                // Fallback for non-numeric ranges
                                all_items.push(generator.word_to_perl(word));
                            }
                        }
                        BraceItem::Literal(s) => {
                            // Single literal item
                            all_items.push(format!("\"{}\"", s));
                        }
                        BraceItem::Sequence(seq) => {
                            // Convert {a,b,c} to separate quoted items
                            for item in seq {
                                all_items.push(format!("\"{}\"", item));
                            }
                        }
                    }
                } else {
                    // Multiple brace items - expand each one
                    for item in &expansion.items {
                        match item {
                            BraceItem::Literal(s) => all_items.push(format!("\"{}\"", s)),
                            BraceItem::Range(range) => {
                                if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
                                    let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
                                    if step == 1 {
                                        all_items.push(format!("{}..{}", start_num, end_num));
                                    } else {
                                        let mut values = Vec::new();
                                        let mut current = start_num;
                                        if step > 0 {
                                            while current <= end_num {
                                                values.push(current.to_string());
                                                current += step;
                                            }
                                        } else {
                                            while current >= end_num {
                                                values.push(current.to_string());
                                                current += step;
                                            }
                                        }
                                        all_items.push(format!("({})", values.join(", ")));
                                    }
                                } else {
                                    all_items.push(format!("\"{}\"", range.start));
                                }
                            }
                            BraceItem::Sequence(seq) => {
                                for item in seq {
                                    all_items.push(format!("\"{}\"", item));
                                }
                            }
                        }
                    }
                }
            }
            Word::Literal(s, _) => {
                // Check if this literal contains space-separated values (likely from brace expansion)
                if s.contains(' ') && s.chars().all(|c| c.is_ascii_digit() || c.is_ascii_whitespace()) {
                    // Split by whitespace and add each item separately
                    let items: Vec<String> = s.split_whitespace()
                        .map(|item| format!("\"{}\"", item))
                        .collect();
                    all_items.extend(items);
                } else {
                    all_items.push(generator.word_to_perl(word));
                }
            }
            _ => all_items.push(generator.word_to_perl(word))
        }
    }
    
    let items_str = all_items.join(", ");
    output.push_str(&items_str);
    output.push_str(") {\n");
    
    // Generate body
    generator.indent_level += 1;
    output.push_str(&generator.generate_block_commands(&for_loop.body));
    generator.indent_level -= 1;
    
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    // After the loop, set the variable to the last value to mimic shell behavior
    // But only if the variable is used later (to avoid unnecessary assignments)
    // This is important for shell compatibility where loop variables retain their last value
    // However, we should only do this if the variable is actually used after the loop
    if generator.function_level_vars.contains(&for_loop.variable) && !all_items.is_empty() {
        // For simple ranges like 1..3, the last value is 3
        if all_items.len() == 1 && items_str.contains("..") {
            // This is a range like "1..3"
            let range_parts: Vec<&str> = items_str.split("..").collect();
            if range_parts.len() == 2 {
                let end_value = range_parts[1];
                output.push_str(&generator.indent());
                output.push_str(&format!("${} = {};\n", for_loop.variable, end_value));
            }
        } else if all_items.len() > 1 {
            // For multiple items, set to the last item
            if let Some(last_item) = all_items.last() {
                output.push_str(&generator.indent());
                output.push_str(&format!("${} = {};\n", for_loop.variable, last_item));
            }
        }
    }
    
    output
}

pub fn generate_function_impl(generator: &mut Generator, func: &Function) -> String {
    let mut output = String::new();
    
    // Generate function definition
    output.push_str(&format!("sub {} {{\n", func.name));
    
    // Handle function parameters
    if !func.parameters.is_empty() {
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("my (");
        let params: Vec<String> = func.parameters.iter()
            .map(|param| format!("${}", param))
            .collect();
        output.push_str(&params.join(", "));
        output.push_str(") = @_;\n");
        
        // Generate function body
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_block_commands(&func.body));
        
        generator.indent_level -= 1;
    } else {
        // No parameters
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_block_commands(&func.body));
        generator.indent_level -= 1;
    }
    
    output.push_str("}\n");
    
    // Mark function as declared
    generator.declared_functions.insert(func.name.clone());
    
    output
}

pub fn generate_block_impl(generator: &mut Generator, block: &Block) -> String {
    let mut output = String::new();
    
    // Generate block
    output.push_str("{\n");
    
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&generator.generate_block_commands(block));
    generator.indent_level -= 1;
    
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}

pub fn generate_break_statement_impl(_generator: &Generator, level: &Option<String>) -> String {
    match level {
        Some(level_str) => format!("last LABEL{};", level_str),
        None => "last;".to_string(),
    }
}

pub fn generate_continue_statement_impl(_generator: &Generator, level: &Option<String>) -> String {
    match level {
        Some(level_str) => format!("next LABEL{};", level_str),
        None => "next;".to_string(),
    }
}

pub fn generate_return_statement_impl(generator: &mut Generator, value: &Option<Word>) -> String {
    match value {
        Some(word) => {
            let perl_value = generator.perl_string_literal(word);
            format!("return {};", perl_value)
        }
        None => "return;".to_string(),
    }
}

// Helper method for indentation
pub fn indent_impl(generator: &Generator) -> String {
    "    ".repeat(generator.indent_level)
}

pub fn generate_block_commands_impl(generator: &mut Generator, block: &Block) -> String {
    let mut output = String::new();
    for command in &block.commands {
        output.push_str(&generator.generate_command(command));
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    output
}