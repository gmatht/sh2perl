use crate::ast::*;
use std::collections::{HashSet, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};

// Static counter for generating truly unique IDs across all generator instances
static GLOBAL_UNIQUE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub mod commands;
pub mod control_flow;
pub mod words;
pub mod expansions;
pub mod redirects;
pub mod test_expressions;
pub mod utils;

pub struct Generator {
    pub indent_level: usize,
    pub declared_locals: HashSet<String>,
    pub declared_functions: HashSet<String>,
    pub file_handle_counter: usize,
    pub extglob_enabled: bool,
    pub nocasematch_enabled: bool,
    pub process_sub_files: HashMap<String, String>,
    pub current_process_sub_file: Option<String>,
    pub function_level_vars: HashSet<String>,
    pub constants: HashMap<String, i64>,
    pub translation_mode: bool, // New field for pure translation mode
    pub inline_mode: bool, // New field for inline mode (for backticks)
    pub original_script_name: Option<String>, // Original script name for $0 compatibility
    pub use_function_signatures: bool, // Control whether to use modern function signatures
}

impl Generator {
    pub fn new() -> Self {
        Self {
            indent_level: 0,
            declared_locals: HashSet::new(),
            declared_functions: HashSet::new(),
            file_handle_counter: 0,
            extglob_enabled: false,
            nocasematch_enabled: false,
            process_sub_files: HashMap::new(),
            current_process_sub_file: None,
            function_level_vars: HashSet::new(),
            constants: HashMap::new(),
            translation_mode: false,
            inline_mode: false,
            original_script_name: None,
            use_function_signatures: true, // Default to modern function signatures
        }
    }

    pub fn new_translation_mode() -> Self {
        Self {
            indent_level: 0,
            declared_locals: HashSet::new(), 
            declared_functions: HashSet::new(),
            file_handle_counter: 0,
            extglob_enabled: false,
            nocasematch_enabled: false,
            process_sub_files: HashMap::new(),
            current_process_sub_file: None,
            function_level_vars: HashSet::new(),
            constants: HashMap::new(),
            translation_mode: true,
            inline_mode: false,
            original_script_name: None,
            use_function_signatures: true, // Default to modern function signatures
        }
    }

    pub fn new_inline_mode() -> Self {
        Self {
            indent_level: 0,
            declared_locals: HashSet::new(),
            declared_functions: HashSet::new(),
            file_handle_counter: 0,
            extglob_enabled: false,
            nocasematch_enabled: false,
            process_sub_files: HashMap::new(),
            current_process_sub_file: None,
            function_level_vars: HashSet::new(),
            constants: HashMap::new(),
            translation_mode: false,
            inline_mode: true,
            original_script_name: None,
            use_function_signatures: true, // Default to modern function signatures
        }
    }

    pub fn set_original_script_name(&mut self, name: String) {
        self.original_script_name = Some(name);
    }


    pub fn generate(&mut self, ast: &[Command]) -> String {
        let mut output = String::new();
        
        // Pre-analysis pass: identify variables that are used after for loops
        self.analyze_variable_usage(ast);
        
        // Pre-analysis pass: identify constants needed for magic numbers
        self.analyze_constants_needed(ast);
        
        // In inline mode, skip the script header and just generate the command code
        if self.inline_mode {
            for command in ast {
                // Reset indentation level for each top-level command to prevent staircase effect
                self.indent_level = 0;
                let command_output = self.generate_command(command);
                output.push_str(&command_output);
                
                // Ensure proper newline separation between commands
                if !command_output.ends_with('\n') {
                    output.push('\n');
                }
            }
            
            // Ensure the output ends with a newline
            if !output.ends_with('\n') {
                output.push('\n');
            }
            
            return output;
        }
        
        // Analyze what imports and variables are needed
        let needs_basename = self.needs_basename_import(ast);
        let needs_exit_code = self.needs_exit_code_tracking(ast);
        let needs_ipc_open3 = self.needs_ipc_open3(ast);
        let needs_file_find = self.needs_file_find_import(ast);
        let needs_digest_sha = self.needs_digest_sha_import(ast);
        let needs_file_path = self.needs_file_path_import(ast);
        let needs_file_copy = self.needs_file_copy_import(ast);
        let needs_posix = self.needs_posix_import(ast);
        let needs_capture_tiny = self.needs_capture_tiny_import(ast);
        
        // Add Perl shebang and pragmas
        output.push_str("#!/usr/bin/env perl\n");
        output.push_str("use strict;\n");
        output.push_str("use warnings;\n");
        output.push_str("use Carp;\n");
        output.push_str("use English qw(-no_match_vars);\n");
        output.push_str("use locale;\n");
        
        if needs_basename {
            output.push_str("use File::Basename;\n");
        }
        if needs_ipc_open3 {
            output.push_str("use IPC::Open3;\n");
        }
        if needs_file_find {
            // No additional imports needed for glob-based approach
        }
        if needs_digest_sha {
            output.push_str("use Digest::SHA   qw(sha256_hex sha512_hex);\n");
        }
        if needs_file_path {
            // Align with other use statements - 2 spaces when Digest::SHA is present, 1 space otherwise
            if needs_digest_sha {
                output.push_str("use File::Path    qw(make_path remove_tree);\n");
            } else {
                output.push_str("use File::Path qw(make_path remove_tree);\n");
            }
        }
        if needs_file_copy {
            // Use 2 spaces when Digest::SHA is present for column alignment, 1 space otherwise
            if needs_digest_sha {
                output.push_str("use File::Copy  qw(copy move);\n");
            } else {
                output.push_str("use File::Copy qw(copy move);\n");
            }
        }
        if needs_posix {
            output.push_str("use POSIX      qw(time);\n");
        }
        if needs_capture_tiny {
            output.push_str("use Capture::Tiny qw(capture_stdout);\n");
        }
        output.push_str("\n");
        
        // Add main exit code variable for pipeline tracking
        // Always declare it since it's used in pipeline generation
        output.push_str("my $main_exit_code = 0;\n");
        output.push_str("my $ls_success     = 0;\n");
        
        // Add global CHILD_ERROR variable for command substitution
        output.push_str("our $CHILD_ERROR;\n\n");
        
        // Add declarations for variables that are used in arithmetic expressions
        for var in &self.function_level_vars {
            output.push_str(&format!("my ${} = 0;\n", var));
        }
        if !self.function_level_vars.is_empty() {
            output.push_str("\n");
        }
        
        // Add constant declarations
        if !self.constants.is_empty() {
            // Calculate the maximum length for alignment
            let max_name_len = self.constants.keys().map(|name| name.len()).max().unwrap_or(0);
            
            for (name, value) in &self.constants {
                let padding = max_name_len - name.len();
                let spaces = " ".repeat(padding);
                output.push_str(&format!("my ${}{} = {};\n", name, spaces, value));
            }
        }
        if !self.constants.is_empty() {
            output.push_str("\n");
        }
        
        for command in ast {
            // Reset indentation level for each top-level command to prevent staircase effect
            self.indent_level = 0;
            let command_output = self.generate_command(command);
            output.push_str(&command_output);
            
            // Ensure proper newline separation between commands
            if !command_output.ends_with('\n') {
                output.push('\n');
            }
        }
        
        // Add final exit statement
        // Always exit with $main_exit_code since it's always declared
        output.push_str("\nexit $main_exit_code;\n");
        
        // Ensure the output ends with a newline
        if !output.ends_with('\n') {
            output.push('\n');
        }
        
        // Remove trailing whitespace from all lines
        let lines: Vec<&str> = output.split('\n').collect();
        let cleaned_lines: Vec<String> = lines.iter()
            .map(|line| line.trim_end().to_string())
            .collect();
        output = cleaned_lines.join("\n");
        
        // Ensure the output ends with a newline (after cleanup)
        if !output.ends_with('\n') {
            output.push('\n');
        }
        
        output
    }

    pub fn generate_command(&mut self, command: &Command) -> String {
        commands::generate_command_impl(self, command, false)
    }

    pub fn generate_command_in_stdout_context(&mut self, command: &Command) -> String {
        commands::generate_command_impl(self, command, true)
    }

    // Delegate to submodules
    pub fn generate_simple_command(&mut self, cmd: &SimpleCommand) -> String {
        commands::generate_simple_command_impl(self, cmd)
    }

    pub fn generate_pipeline(&mut self, pipeline: &Pipeline) -> String {
        commands::generate_pipeline_impl(self, pipeline)
    }

    pub fn generate_subshell(&mut self, command: &Command) -> String {
        commands::generate_subshell_impl(self, command)
    }

    pub fn generate_background(&mut self, command: &Command) -> String {
        commands::generate_background_impl(self, command)
    }

    pub fn generate_command_string_for_system(&mut self, cmd: &Command) -> String {
        commands::generate_command_string_for_system_impl(self, cmd)
    }

    pub fn generate_if_statement(&mut self, if_stmt: &IfStatement) -> String {
        control_flow::generate_if_statement_impl(self, if_stmt)
    }

    pub fn generate_case_statement(&mut self, case_stmt: &CaseStatement) -> String {
        control_flow::generate_case_statement_impl(self, case_stmt)
    }

    pub fn generate_while_loop(&mut self, while_loop: &WhileLoop) -> String {
        control_flow::generate_while_loop_impl(self, while_loop)
    }

    pub fn generate_for_loop(&mut self, for_loop: &ForLoop) -> String {
        control_flow::generate_for_loop_impl(self, for_loop)
    }

    pub fn generate_function(&mut self, func: &Function) -> String {
        control_flow::generate_function_impl(self, func)
    }

    pub fn generate_block(&mut self, block: &Block) -> String {
        control_flow::generate_block_impl(self, block)
    }

    pub fn generate_break_statement(&mut self, level: &Option<String>) -> String {
        control_flow::generate_break_statement_impl(self, level)
    }

    pub fn generate_continue_statement(&mut self, level: &Option<String>) -> String {
        control_flow::generate_continue_statement_impl(self, level)
    }

    pub fn generate_return_statement(&mut self, value: &Option<Word>) -> String {
        control_flow::generate_return_statement_impl(self, value)
    }

    pub fn generate_assignment(&mut self, assignment: &Assignment) -> String {
        let mut output = String::new();
        
        
        // Only declare the variable if not already declared
        // This prevents redeclaring variables inside loops that shadow outer scope variables
        if !self.declared_locals.contains(&assignment.variable) && !self.function_level_vars.contains(&assignment.variable) {
            output.push_str(&self.indent());
            output.push_str(&format!("my ${};\n", assignment.variable));
            self.declared_locals.insert(assignment.variable.clone());
        }
        
        // Generate the assignment based on the operator
        output.push_str(&self.indent());
        match assignment.operator {
            AssignmentOperator::Assign => {
                let value_perl = words::word_to_perl_impl(self, &assignment.value);
                // If the value is a block, wrap it in do {...}
                if value_perl.starts_with('{') && value_perl.ends_with('}') {
                    output.push_str(&format!("${} = do {};\n", assignment.variable, value_perl));
                } else {
                    output.push_str(&format!("${} = {};\n", assignment.variable, value_perl));
                }
            }
            AssignmentOperator::PlusAssign => {
                output.push_str(&format!("${} += {};\n", 
                    assignment.variable, 
                    words::word_to_perl_impl(self, &assignment.value)));
            }
            AssignmentOperator::MinusAssign => {
                output.push_str(&format!("${} -= {};\n", 
                    assignment.variable, 
                    words::word_to_perl_impl(self, &assignment.value)));
            }
            AssignmentOperator::StarAssign => {
                output.push_str(&format!("${} *= {};\n", 
                    assignment.variable, 
                    words::word_to_perl_impl(self, &assignment.value)));
            }
            AssignmentOperator::SlashAssign => {
                output.push_str(&format!("${} /= {};\n", 
                    assignment.variable, 
                    words::word_to_perl_impl(self, &assignment.value)));
            }
            AssignmentOperator::PercentAssign => {
                output.push_str(&format!("${} %= {};\n", 
                    assignment.variable, 
                    words::word_to_perl_impl(self, &assignment.value)));
            }
        }
        
        output
    }

    pub fn generate_test_expression(&mut self, test_expr: &TestExpression) -> String {
        test_expressions::generate_test_expression_impl(self, test_expr)
    }

    pub fn generate_test_command(&mut self, cmd: &SimpleCommand, output: &mut String) {
        test_expressions::generate_test_command_impl(self, cmd, output)
    }

    pub fn generate_shopt_command(&mut self, cmd: &ShoptCommand) -> String {
        redirects::generate_shopt_command_impl(self, cmd)
    }

    pub fn generate_builtin_command(&mut self, cmd: &BuiltinCommand) -> String {
        redirects::generate_builtin_command_impl(self, cmd)
    }

    pub fn generate_redirect(&mut self, redirect: &Redirect) -> String {
        redirects::generate_redirect_impl(self, redirect)
    }

    pub fn word_to_perl(&mut self, word: &Word) -> String {
        words::word_to_perl_impl(self, word)
    }

    pub fn word_to_perl_for_test(&mut self, word: &Word) -> String {
        words::word_to_perl_for_test_impl(self, word)
    }

    pub fn generate_parameter_expansion(&mut self, pe: &ParameterExpansion) -> String {
        expansions::generate_parameter_expansion_impl(self, pe)
    }

    pub fn extract_array_key(&self, var: &str) -> Option<(String, String)> {
        utils::extract_array_key_impl(var)
    }

    pub fn extract_array_elements(&self, value: &str) -> Option<Vec<String>> {
        utils::extract_array_elements_impl(value)
    }

    pub fn perl_string_literal(&mut self, word: &Word) -> String {
        utils::perl_string_literal_impl(self, word)
    }

    pub fn strip_shell_quotes_and_convert_to_perl(&mut self, word: &Word) -> String {
        utils::strip_shell_quotes_and_convert_to_perl_impl(self, word)
    }

    pub fn strip_shell_quotes_for_regex(&mut self, word: &Word) -> String {
        utils::strip_shell_quotes_for_regex_impl(self, word)
    }

    pub fn get_unique_file_handle(&mut self) -> String {
        utils::get_unique_file_handle_impl(self)
    }

    pub fn get_unique_id(&mut self) -> String {
        let id = GLOBAL_UNIQUE_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{}", id)
    }

    pub fn get_unique_ipc_vars(&mut self) -> (String, String, String, String, String) {
        let id = self.get_unique_id();
        (
            format!("$in_{}", id),
            format!("$out_{}", id),
            format!("$err_{}", id),
            format!("$pid_{}", id),
            format!("$result_{}", id)
        )
    }

    pub fn add_constant(&mut self, name: &str, value: i64) {
        self.constants.insert(name.to_string(), value);
    }

    // Additional helper methods that are needed
    pub fn handle_range_expansion(&self, s: &str) -> String {
        words::handle_range_expansion_impl(self, s)
    }

    pub fn handle_comma_expansion(&self, s: &str) -> String {
        words::handle_comma_expansion_impl(self, s)
    }

    pub fn handle_brace_expansion(&mut self, expansion: &BraceExpansion) -> String {
        words::handle_brace_expansion_impl(self, expansion)
    }

    pub fn brace_item_to_word(&self, item: &BraceItem) -> Word {
        words::brace_item_to_word_impl(self, item)
    }

    pub fn convert_string_interpolation_to_perl(&mut self, interp: &StringInterpolation) -> String {
        words::convert_string_interpolation_to_perl_impl(self, interp)
    }

    pub fn convert_arithmetic_to_perl(&self, expr: &str) -> String {
        words::convert_arithmetic_to_perl_impl(self, expr)
    }

    pub fn convert_extglob_to_perl_regex(&self, pattern: &str) -> String {
        test_expressions::convert_extglob_to_perl_regex_impl(self, pattern)
    }

    pub fn convert_glob_to_regex(&self, pattern: &str) -> String {
        test_expressions::convert_glob_to_regex_impl(self, pattern)
    }

    pub fn convert_test_args_to_expression(&self, args: &[Word]) -> TestExpression {
        test_expressions::convert_test_args_to_expression_impl(self, args)
    }

    pub fn indent(&self) -> String {
        control_flow::indent_impl(self)
    }

    pub fn generate_block_commands(&mut self, block: &Block) -> String {
        control_flow::generate_block_commands_impl(self, block)
    }

    pub fn escape_perl_string(&self, s: &str) -> String {
        commands::utilities::escape_perl_string(s)
    }

    /// Optimizes a string argument by appending a newline if it's a simple string literal
    pub fn optimize_string_with_newline(&self, arg: &str) -> Option<String> {
        // Check if this is a simple quoted string that we can optimize
        let trimmed = arg.trim();
        if trimmed.starts_with("\"") && trimmed.ends_with("\"") {
            // Extract the content between quotes
            let content = &trimmed[1..trimmed.len()-1];
            // Check if it doesn't already end with \n
            if !content.ends_with("\\n") {
                // Create optimized version with newline appended
                return Some(format!("\"{}\\n\"", content));
            }
        }
        
        // Check if this is a single variable (like $i) that we can optimize
        if trimmed.starts_with("$") && !trimmed.contains(",") && !trimmed.contains(" ") {
            // Single variable, optimize to "$var\n"
            return Some(format!("\"{}\\n\"", trimmed));
        }
        
        None
    }

    /// Pre-analysis pass to identify variables that are used after for loops
    fn analyze_variable_usage(&mut self, ast: &[Command]) {
        for (i, command) in ast.iter().enumerate() {
            if let Command::For(for_loop) = command {
                // Check if this variable is used in subsequent commands
                let var_name = &for_loop.variable;
                for j in (i + 1)..ast.len() {
                    if self.is_variable_used_in_command(&ast[j], var_name) {
                        self.function_level_vars.insert(var_name.clone());
                        break;
                    }
                }
                
                // Also check for variables used in arithmetic expressions within the loop body
                self.analyze_variables_in_block(&for_loop.body);
            }
        }
    }
    
    /// Pre-analysis pass to identify constants needed for magic numbers
    fn analyze_constants_needed(&mut self, ast: &[Command]) {
        for command in ast {
            self.analyze_constants_in_command(command);
        }
    }
    
    /// Analyze constants needed in a command
    fn analyze_constants_in_command(&mut self, command: &Command) {
        match command {
            Command::For(for_loop) => {
                for word in &for_loop.items {
                    if let Word::BraceExpansion(expansion, _) = word {
                        if expansion.items.len() == 1 {
                            if let BraceItem::Range(range) = &expansion.items[0] {
                                if let (Ok(_start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
                                    if end_num > 2 {
                                        let const_name = format!("MAX_LOOP_{}", end_num);
                                        self.add_constant(&const_name, end_num);
                                    }
                                }
                            }
                        }
                    }
                }
                // Also analyze constants in the loop body
                self.analyze_constants_in_block(&for_loop.body);
            }
            Command::If(if_stmt) => {
                // Analyze test expressions for magic numbers
                self.analyze_constants_in_command(&if_stmt.condition);
                self.analyze_constants_in_command(&if_stmt.then_branch);
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.analyze_constants_in_command(else_branch);
                }
            }
            Command::While(while_loop) => {
                // Analyze test expressions for magic numbers
                self.analyze_constants_in_command(&while_loop.condition);
                self.analyze_constants_in_block(&while_loop.body);
            }
            Command::Case(case_stmt) => {
                for case in &case_stmt.cases {
                    for command in &case.body {
                        self.analyze_constants_in_command(command);
                    }
                }
            }
            Command::Function(func) => {
                self.analyze_constants_in_block(&func.body);
            }
            Command::Block(block) => {
                self.analyze_constants_in_block(block);
            }
            Command::Simple(cmd) => {
                // Analyze simple commands for magic numbers in arguments
                for arg in &cmd.args {
                    self.analyze_constants_in_word(arg);
                }
            }
            Command::TestExpression(test_expr) => {
                // Analyze test expressions for magic numbers
                self.analyze_constants_in_test_expression(test_expr);
            }
            _ => {} // Other commands don't need constant analysis
        }
    }
    
    /// Analyze constants in test expressions
    fn analyze_constants_in_test_expression(&mut self, test_expr: &TestExpression) {
        // Extract numbers from test expressions like "($i < 10)"
        let expr = &test_expr.expression;
        self.extract_magic_numbers_from_string(expr);
    }
    
    /// Analyze constants in words
    fn analyze_constants_in_word(&mut self, word: &Word) {
        match word {
            Word::Literal(s, _) => {
                self.extract_magic_numbers_from_string(s);
            }
            Word::StringInterpolation(interp, _) => {
                for part in &interp.parts {
                    match part {
                        StringPart::Literal(s) => {
                            self.extract_magic_numbers_from_string(s);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    
    /// Extract magic numbers from a string and add them as constants
    fn extract_magic_numbers_from_string(&mut self, s: &str) {
        // Simple regex-like extraction of numbers > 2
        let words: Vec<&str> = s.split_whitespace().collect();
        for word in words {
            if let Ok(num) = word.parse::<i64>() {
                if num > 2 {
                    let const_name = format!("MAGIC_{}", num);
                    self.add_constant(&const_name, num);
                }
            }
        }
    }
    
    /// Analyze constants needed in a block
    fn analyze_constants_in_block(&mut self, block: &Block) {
        for command in &block.commands {
            self.analyze_constants_in_command(command);
        }
    }
    
    /// Analyze variables used in a block (for arithmetic expressions, etc.)
    fn analyze_variables_in_block(&mut self, block: &Block) {
        for command in &block.commands {
            if let Command::Simple(cmd) = command {
                // Check environment variables for arithmetic expressions
                for (_env_var, value) in &cmd.env_vars {
                    if let Word::Arithmetic(arith_expr, _) = value {
                        // Extract variable names from arithmetic expression
                        // Simple approach: look for identifiers in the expression
                        let expr = &arith_expr.expression;
                        // Find all identifiers in the expression (simple regex-like approach)
                        let mut chars = expr.chars().peekable();
                        let mut current_var = String::new();
                        
                        while let Some(c) = chars.next() {
                            if c.is_alphabetic() || c == '_' {
                                current_var.push(c);
                                // Continue collecting characters until we hit a non-identifier character
                                while let Some(&next_c) = chars.peek() {
                                    if next_c.is_alphanumeric() || next_c == '_' {
                                        current_var.push(chars.next().unwrap());
                                    } else {
                                        break;
                                    }
                                }
                                if !current_var.is_empty() {
                                    self.function_level_vars.insert(current_var.clone());
                                    current_var.clear();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Check if a variable is used in a command
    fn is_variable_used_in_command(&self, command: &Command, var_name: &str) -> bool {
        match command {
            Command::While(while_loop) => {
                // Check if variable is used in while loop condition
                if let Command::TestExpression(test_expr) = &*while_loop.condition {
                    return test_expr.expression.contains(&format!("${}", var_name));
                }
                if let Command::Simple(cmd) = &*while_loop.condition {
                    // Check if variable is used in test command arguments
                    for arg in &cmd.args {
                        if let Word::Variable(var, _, _) = arg {
                            if var == var_name {
                                return true;
                            }
                        }
                    }
                }
            },
            Command::Simple(cmd) => {
                // Check if variable is used in simple command arguments
                for arg in &cmd.args {
                    if let Word::Variable(var, _, _) = arg {
                        if var == var_name {
                            return true;
                        }
                    }
                }
                
                // Check if variable is used in environment variable assignments (arithmetic expressions)
                for (env_var, value) in &cmd.env_vars {
                    if env_var == var_name {
                        return true;
                    }
                    // Check if the variable is used in the value (e.g., arithmetic expressions)
                    if let Word::Arithmetic(arith_expr, _) = value {
                        if arith_expr.expression.contains(var_name) {
                            return true;
                        }
                    }
                }
            },
            _ => {
                // For other command types, we could add more sophisticated analysis
                // but for now, we'll be conservative
            }
        }
        false
    }
    
    /// Check if the AST needs File::Basename import
    fn needs_basename_import(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_basename(command) {
                return true;
            }
        }
        false
    }
    
    /// Check if a specific command needs File::Basename
    fn command_needs_basename(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if it's a basename command
                if let Word::Literal(name, _) = &cmd.name {
                    if name == "basename" {
                        return true;
                    }
                }
                // Check for basename parameter expansion in arguments
                for arg in &cmd.args {
                    if self.word_needs_basename(arg) {
                        return true;
                    }
                }
                false
            },
            Command::Pipeline(pipeline) => {
                for cmd in &pipeline.commands {
                    if self.command_needs_basename(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::And(left, right) | Command::Or(left, right) => {
                self.command_needs_basename(left) || self.command_needs_basename(right)
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_basename(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_basename(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_basename(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_basename(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_basename(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }
    
    /// Check if a word needs basename functionality
    fn word_needs_basename(&self, word: &Word) -> bool {
        match word {
            Word::ParameterExpansion(pe, _) => {
                matches!(pe.operator, ParameterExpansionOperator::Basename)
            },
            Word::Array(_, _elements, _) => {
                // Array elements are strings, not words, so no basename needed
                false
            },
            Word::StringInterpolation(interp, _) => {
                for part in &interp.parts {
                    match part {
                        StringPart::Literal(_) => {},
                        StringPart::Variable(_) => {}, // Variables are strings, not words
                        StringPart::CommandSubstitution(_) => {}, // Command substitutions don't need basename
                        StringPart::ParameterExpansion(pe) => {
                            if matches!(pe.operator, ParameterExpansionOperator::Basename) {
                                return true;
                            }
                        },
                        StringPart::MapAccess(_, _) => {}, // Map access doesn't need basename
                        StringPart::MapKeys(_) => {}, // Map keys don't need basename
                        StringPart::MapLength(_) => {}, // Map length doesn't need basename
                        StringPart::ArraySlice(_, _, _) => {}, // Array slice doesn't need basename
                        StringPart::Arithmetic(_) => {}, // Arithmetic expressions don't need basename
                    }
                }
                false
            },
            _ => false
        }
    }
    
    /// Check if the AST needs exit code tracking
    fn needs_exit_code_tracking(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_exit_code_tracking(command) {
                return true;
            }
        }
        false
    }
    
    /// Check if a specific command needs exit code tracking
    fn command_needs_exit_code_tracking(&self, command: &Command) -> bool {
        match command {
            Command::Pipeline(pipeline) => {
                // Only complex pipelines need exit code tracking
                // Simple pipelines like "cmd1 | cmd2" don't need it
                pipeline.commands.len() > 2
            },
            Command::And(_left, _right) | Command::Or(_left, _right) => {
                // Logical operators need exit code tracking
                true
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_exit_code_tracking(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_exit_code_tracking(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_exit_code_tracking(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_exit_code_tracking(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_exit_code_tracking(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }
    
    /// Check if the AST needs IPC::Open3
    fn needs_ipc_open3(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_ipc_open3(command) {
                return true;
            }
        }
        false
    }

    fn needs_file_find_import(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_file_find(command) {
                return true;
            }
        }
        false
    }

    /// Check if a specific command needs File::Find
    fn command_needs_file_find(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if this is a find command
                if let Word::Literal(name, _) = &cmd.name {
                    if name == "find" {
                        return true;
                    }
                }
                // Also check arguments for find commands
                for arg in &cmd.args {
                    if self.word_needs_file_find(arg) {
                        return true;
                    }
                }
                // Check env_vars for command substitutions with find commands
                for (_, env_value) in &cmd.env_vars {
                    if self.word_needs_file_find(env_value) {
                        return true;
                    }
                }
                false
            },
            Command::Pipeline(pipeline) => {
                for cmd in &pipeline.commands {
                    if self.command_needs_file_find(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_file_find(&redirect_cmd.command)
            },
            Command::Background(bg_cmd) => {
                if self.command_needs_file_find(bg_cmd) {
                    return true;
                }
                false
            },
            Command::Subshell(sub_cmd) => {
                if self.command_needs_file_find(sub_cmd) {
                    return true;
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_file_find(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_file_find(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }

    /// Check if a word needs File::Find (e.g., command substitution with find)
    fn word_needs_file_find(&self, word: &Word) -> bool {
        match word {
            Word::CommandSubstitution(cmd, _) => {
                self.command_needs_file_find(cmd)
            },
            Word::StringInterpolation(interp, _) => {
                for part in &interp.parts {
                    if self.string_part_needs_file_find(part) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }

    /// Check if a string interpolation part needs File::Find
    fn string_part_needs_file_find(&self, part: &StringPart) -> bool {
        match part {
            StringPart::CommandSubstitution(cmd) => {
                self.command_needs_file_find(cmd)
            },
            _ => false
        }
    }
    
    /// Check if a specific command needs IPC::Open3
    fn command_needs_ipc_open3(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if it's a command that uses open3
                if let Word::Literal(name, _) = &cmd.name {
                    // Commands that typically use open3 for IPC
                    match name.as_str() {
                        "pwd" | "whoami" | "date" | "id" | "uname" | "hostname" | "uptime" | "w" | "who" | "perl" => true,
                        _ => {
                            // Check if it's a pipeline command or has complex arguments
                            cmd.args.len() > 0 || self.is_pipeline_command(name)
                        }
                    }
                } else {
                    // Non-literal command names often need open3
                    true
                }
            },
            Command::Pipeline(_pipeline) => {
                // Pipelines need IPC::Open3
                true
            },
            Command::And(left, right) | Command::Or(left, right) => {
                self.command_needs_ipc_open3(left) || self.command_needs_ipc_open3(right)
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_ipc_open3(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_ipc_open3(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_ipc_open3(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_ipc_open3(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_ipc_open3(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }
    
    /// Check if a command name is typically used in pipelines
    fn is_pipeline_command(&self, name: &str) -> bool {
        matches!(name, "grep" | "sed" | "awk" | "cut" | "sort" | "uniq" | "head" | "tail" | "wc" | "tr" | "cat" | "echo" | "printf")
    }
    
    /// Generate a properly formatted regex pattern with appropriate flags
    pub fn format_regex_pattern(&self, pattern: &str) -> String {
        utils::format_regex_pattern(pattern)
    }
    
    /// Generate a regex pattern for checking if string ends with newline
    pub fn newline_end_regex(&self) -> String {
        utils::newline_end_regex()
    }
    
    /// Convert postfix unless statement to block form
    pub fn convert_postfix_unless_to_block(&self, condition: &str, statement: &str) -> String {
        utils::convert_postfix_unless_to_block_no_indent(condition, statement)
    }

    /// Check if the AST needs Digest::SHA import
    fn needs_digest_sha_import(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_digest_sha(command) {
                return true;
            }
        }
        false
    }

    /// Check if a specific command needs Digest::SHA
    fn command_needs_digest_sha(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if it's a sha256sum or sha512sum command
                if let Word::Literal(name, _) = &cmd.name {
                    if name == "sha256sum" || name == "sha512sum" {
                        return true;
                    }
                }
                // Check arguments for command substitutions with sha commands
                for arg in &cmd.args {
                    if self.word_needs_digest_sha(arg) {
                        return true;
                    }
                }
                // Check env_vars for command substitutions with sha commands
                for (_, env_value) in &cmd.env_vars {
                    if self.word_needs_digest_sha(env_value) {
                        return true;
                    }
                }
                false
            },
            Command::Pipeline(pipeline) => {
                for cmd in &pipeline.commands {
                    if self.command_needs_digest_sha(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::And(left, right) | Command::Or(left, right) => {
                self.command_needs_digest_sha(left) || self.command_needs_digest_sha(right)
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_digest_sha(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_digest_sha(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_digest_sha(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_digest_sha(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_digest_sha(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }

    /// Check if a word needs Digest::SHA functionality
    fn word_needs_digest_sha(&self, word: &Word) -> bool {
        match word {
            Word::CommandSubstitution(cmd, _) => {
                self.command_needs_digest_sha(cmd)
            },
            Word::StringInterpolation(interp, _) => {
                for part in &interp.parts {
                    match part {
                        StringPart::CommandSubstitution(cmd) => {
                            if self.command_needs_digest_sha(cmd) {
                                return true;
                            }
                        },
                        _ => {}
                    }
                }
                false
            },
            _ => false
        }
    }

    /// Check if the AST needs File::Path import
    fn needs_file_path_import(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_file_path(command) {
                return true;
            }
        }
        false
    }

    /// Check if a specific command needs File::Path
    fn command_needs_file_path(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if it's a command that uses File::Path
                if let Word::Literal(name, _) = &cmd.name {
                    match name.as_str() {
                        "cp" | "mv" | "rm" | "mkdir" => true,
                        _ => false
                    }
                } else {
                    false
                }
            },
            Command::Pipeline(pipeline) => {
                for cmd in &pipeline.commands {
                    if self.command_needs_file_path(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::And(left, right) | Command::Or(left, right) => {
                self.command_needs_file_path(left) || self.command_needs_file_path(right)
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_file_path(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_file_path(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_file_path(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_file_path(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_file_path(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }

    /// Check if the AST needs File::Copy import
    fn needs_file_copy_import(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_file_copy(command) {
                return true;
            }
        }
        false
    }

    /// Check if the AST needs Capture::Tiny import
    fn needs_capture_tiny_import(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_capture_tiny(command) {
                return true;
            }
        }
        false
    }

    /// Check if a specific command needs File::Copy
    fn command_needs_file_copy(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if it's a command that uses File::Copy
                if let Word::Literal(name, _) = &cmd.name {
                    match name.as_str() {
                        "cp" | "mv" => return true,
                        _ => {}
                    }
                }
                
                // Check command substitutions in environment variables
                for (_, word) in &cmd.env_vars {
                    if self.word_needs_file_copy(word) {
                        return true;
                    }
                }
                
                false
            },
            Command::Pipeline(pipeline) => {
                for cmd in &pipeline.commands {
                    if self.command_needs_file_copy(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::And(left, right) | Command::Or(left, right) => {
                self.command_needs_file_copy(left) || self.command_needs_file_copy(right)
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_file_copy(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_file_copy(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_file_copy(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_file_copy(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_file_copy(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }
    
    /// Check if a word contains command substitutions that need File::Copy
    fn word_needs_file_copy(&self, word: &Word) -> bool {
        match word {
            Word::CommandSubstitution(cmd, _) => {
                self.command_needs_file_copy(cmd)
            },
            _ => false
        }
    }

    /// Check if a specific command needs Capture::Tiny
    fn command_needs_capture_tiny(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if it's a perl command with -e flag
                if let Word::Literal(name, _) = &cmd.name {
                    if name == "perl" && cmd.args.len() >= 2 {
                        if let Word::Literal(flag, _) = &cmd.args[0] {
                            if flag == "-e" {
                                return true;
                            }
                        }
                    }
                }
                
                // Check command substitutions in environment variables
                for (_, word) in &cmd.env_vars {
                    if self.word_needs_capture_tiny(word) {
                        return true;
                    }
                }
                
                false
            },
            Command::Pipeline(pipeline) => {
                for cmd in &pipeline.commands {
                    if self.command_needs_capture_tiny(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::And(left, right) | Command::Or(left, right) => {
                self.command_needs_capture_tiny(left) || self.command_needs_capture_tiny(right)
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_capture_tiny(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_capture_tiny(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_capture_tiny(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_capture_tiny(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_capture_tiny(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }

    /// Check if a word contains command substitutions that need Capture::Tiny
    fn word_needs_capture_tiny(&self, word: &Word) -> bool {
        match word {
            Word::CommandSubstitution(cmd, _) => {
                self.command_needs_capture_tiny(cmd)
            },
            _ => false
        }
    }

    /// Check if the AST needs POSIX import
    fn needs_posix_import(&self, ast: &[Command]) -> bool {
        for command in ast {
            if self.command_needs_posix(command) {
                return true;
            }
        }
        false
    }

    /// Check if a specific command needs POSIX
    fn command_needs_posix(&self, command: &Command) -> bool {
        match command {
            Command::Simple(cmd) => {
                // Check if it's a command that uses POSIX
                if let Word::Literal(name, _) = &cmd.name {
                    match name.as_str() {
                        "touch" => true,
                        _ => false
                    }
                } else {
                    false
                }
            },
            Command::Pipeline(pipeline) => {
                for cmd in &pipeline.commands {
                    if self.command_needs_posix(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::And(left, right) | Command::Or(left, right) => {
                self.command_needs_posix(left) || self.command_needs_posix(right)
            },
            Command::Redirect(redirect_cmd) => {
                self.command_needs_posix(&redirect_cmd.command)
            },
            Command::For(for_loop) => {
                for cmd in &for_loop.body.commands {
                    if self.command_needs_posix(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::While(while_loop) => {
                for cmd in &while_loop.body.commands {
                    if self.command_needs_posix(cmd) {
                        return true;
                    }
                }
                false
            },
            Command::If(if_stmt) => {
                if self.command_needs_posix(&if_stmt.then_branch) {
                    return true;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    if self.command_needs_posix(else_branch) {
                        return true;
                    }
                }
                false
            },
            _ => false
        }
    }
    
    /// Adjust file paths for Perl execution context
    /// Since Perl code runs from the examples directory, we need to strip the examples/ prefix
    pub fn adjust_file_path_for_perl_execution(&self, filename: &str) -> String {
        // Remove quotes if present
        let unquoted = filename.trim_matches('"').trim_matches('\'');
        
        // If the path starts with "examples/", remove that prefix since Perl runs from examples directory
        if unquoted.starts_with("examples/") {
            let adjusted = unquoted.strip_prefix("examples/").unwrap_or(unquoted);
            // Re-add quotes if the original had them
            if filename.starts_with('"') && filename.ends_with('"') {
                format!("\"{}\"", adjusted)
            } else if filename.starts_with('\'') && filename.ends_with('\'') {
                format!("'{}'", adjusted)
            } else {
                adjusted.to_string()
            }
        } else {
            filename.to_string()
        }
    }
}

