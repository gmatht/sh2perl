use crate::ast::*;
use crate::generator::Generator;
use super::grep::generate_grep_command;
use crate::generator::utils::get_temp_dir;
use super::cat::generate_cat_command;
use super::paste::generate_paste_command;

// Helper function to recursively collect all redirects from nested RedirectCommands
fn collect_all_redirects(command: &Command) -> (Vec<Redirect>, Command) {
    match command {
        Command::Redirect(redirect_cmd) => {
            let mut all_redirects = Vec::new();
            let (mut inner_redirects, base_cmd) = collect_all_redirects(&redirect_cmd.command);
            // For nested RedirectCommand, we want inner redirects first, then outer redirects
            // This matches the order they appear in the original Bash command
            all_redirects.extend(inner_redirects);
            all_redirects.extend(redirect_cmd.redirects.clone());
            (all_redirects, base_cmd)
        }
        _ => (Vec::new(), command.clone())
    }
}

pub fn generate_command_impl(generator: &mut Generator, command: &Command, in_stdout_context: bool) -> String {
    generate_command_impl_with_input(generator, command, in_stdout_context, None)
}

pub fn generate_command_impl_with_input(generator: &mut Generator, command: &Command, in_stdout_context: bool, input_data: Option<&str>) -> String {
//     eprintln!("DEBUG: generate_command_impl called with command: {:?}, in_stdout_context: {}", command, in_stdout_context);
    match command {
        Command::Simple(cmd) => {
//             eprintln!("DEBUG: Dispatching Simple command: {:?}", cmd);
            let result = generator.generate_simple_command(cmd);
//             eprintln!("DEBUG: Simple command result: {}", result);
            result
        },
        Command::ShoptCommand(cmd) => generator.generate_shopt_command(cmd),
        Command::TestExpression(test_expr) => {
            generator.generate_test_expression(test_expr)
        },
        Command::Pipeline(pipeline) => {
//             eprintln!("DEBUG: Found Pipeline, commands: {:?}", pipeline.commands);
            // This is now a pure pipe pipeline since logical operators are handled separately
            if pipeline.commands.len() == 1 {
                // Single command in pipeline, just generate it
                generator.generate_command(&pipeline.commands[0])
            } else {
                // Multiple commands, implement proper Perl pipeline
                super::pipeline_commands::generate_pipeline_impl(generator, pipeline)
            }
        },
        Command::And(left, right) => {
            // Handle logical AND operation
            super::logic_commands::generate_logical_and(generator, left, right)
        },
        Command::Or(left, right) => {
            // Handle logical OR operation
            super::logic_commands::generate_logical_or(generator, left, right)
        },
        Command::If(if_stmt) => generator.generate_if_statement(if_stmt),
        Command::Case(case_stmt) => generator.generate_case_statement(case_stmt),
        Command::While(while_loop) => generator.generate_while_loop(while_loop),
        Command::For(for_loop) => generator.generate_for_loop(for_loop),
        Command::Function(func) => generator.generate_function(func),
        Command::Subshell(cmd) => generator.generate_subshell(cmd),
        Command::Background(cmd) => generator.generate_background(cmd),
        Command::Block(block) => generator.generate_block(block),
        Command::BuiltinCommand(cmd) => generator.generate_builtin_command(cmd),
        Command::Break(level) => generator.generate_break_statement(level),
        Command::Continue(level) => generator.generate_continue_statement(level),
        Command::Return(value) => generator.generate_return_statement(value),
        Command::Assignment(assignment) => {
            eprintln!("DEBUG: Processing Assignment command: {} = {:?}", assignment.variable, assignment.value);
            generator.generate_assignment(assignment)
        },
        Command::BlankLine => "\n".to_string(),
        Command::Redirect(redirect_cmd) => {
//             eprintln!("DEBUG: Processing Redirect command with {} redirects", redirect_cmd.redirects.len());
            
                    // Check if the base command is a Pipeline with logical operators
        let (all_redirects, base_command) = collect_all_redirects(command);
//         eprintln!("DEBUG: Collected {} total redirects from nested structure", all_redirects.len());
        
        // If the base command is a Pipeline with logical operators, handle it specially
//         eprintln!("DEBUG: Base command type: {:?}", std::mem::discriminant(&base_command));
        if let Command::Pipeline(pipeline) = &base_command {
//             eprintln!("DEBUG: Found Pipeline, commands: {:?}", pipeline.commands);
            // This is now a pure pipe pipeline since logical operators are handled separately
            if pipeline.commands.len() == 1 {
//                 eprintln!("DEBUG: Found Pipeline with logical operators inside Redirect, delegating to pipeline generator");
                return generator.generate_pipeline(pipeline);
            }
        }
        
        // Check if the command structure contains a Pipeline with logical operators
        // This handles the case where the parser didn't correctly identify the || operator
        if let Command::Redirect(redirect_cmd) = command {
//             eprintln!("DEBUG: Checking RedirectCommand for nested Pipeline with logical operators");
            if let Command::Pipeline(pipeline) = &*redirect_cmd.command {
//                 eprintln!("DEBUG: Found Pipeline in nested Redirect, commands: {:?}", pipeline.commands);
                // This is now a pure pipe pipeline since logical operators are handled separately
                if pipeline.commands.len() > 1 {
//                     eprintln!("DEBUG: Delegating to pipeline generator for logical operators");
                    return generator.generate_pipeline(pipeline);
                }
            }
        }
            
            // Check if this is a cat command with heredocs
            if let Command::Simple(cat_cmd) = &base_command {
                if let Word::Literal(cmd_name, _) = &cat_cmd.name {
                    if cmd_name == "cat" {
                        // Check if any of the redirects are heredocs
                        let has_heredoc = all_redirects.iter().any(|r| {
                            matches!(r.operator, RedirectOperator::Heredoc | RedirectOperator::HeredocTabs)
                        });
                        
                        if has_heredoc {
                            // Use the dedicated cat command generator for heredocs
                            return generate_cat_command(generator, cat_cmd, &all_redirects, "$output");
                        }
                    }
                }
            }
            
            // Handle redirects first and collect information
            let mut result = String::new();
            let mut has_here_string = false;
            let mut here_string_content = String::new();
            let mut process_sub_files = Vec::new();
            
            for redirect in &all_redirects {
                match &redirect.operator {
                    RedirectOperator::HereString => {
//                         eprintln!("DEBUG: Found HereString redirect, heredoc_body: {:?}", redirect.heredoc_body);
                        has_here_string = true;
                        if let Some(content) = &redirect.heredoc_body {
                            // The content is already a string, just use it directly
                            here_string_content = format!("\"{}\"", content);
                        } else {
                            // Fallback: try to extract from target
                            match &redirect.target {
                                Word::Literal(s, _) => {
                                    // Remove surrounding quotes if they exist
                                    let content = if (s.starts_with('"') && s.ends_with('"')) || 
                                                   (s.starts_with('\'') && s.ends_with('\'')) {
                                        &s[1..s.len()-1]
                                    } else {
                                        s
                                    };
                                    here_string_content = format!("\"{}\"", content);
                                }
                                _ => {
                                    // Fallback to empty string
                                    here_string_content = "\"\"".to_string();
                                }
                            }
                        }
                    }
                    RedirectOperator::ProcessSubstitutionInput(cmd) => {
                        // Process substitution input: <(command)
                        let global_counter = generator.get_unique_file_handle();
                        let temp_file = format!("{}/process_sub_{}.tmp", get_temp_dir(), global_counter);
                        let temp_var = format!("temp_file_ps_{}", global_counter);
                        
                        result.push_str(&generator.indent());
                        result.push_str(&format!("my ${} = {} . '/process_sub_{}.tmp';\n", temp_var, get_temp_dir(), global_counter));
                        
                        // Execute the command and capture its output
                        // Check if this is a complex command that should use Perl code generation
                        let is_complex_command = matches!(**cmd, Command::Pipeline(_) | Command::Redirect(_));
                        
                        if in_stdout_context || is_complex_command {
                            // If we're already in a STDOUT context, or if this is a complex command, generate the actual Perl code
                            result.push_str(&generator.indent());
                            result.push_str(&format!("my $output_ps_{};\n", global_counter));
                            result.push_str(&generator.indent());
                            result.push_str(&format!("{{\n"));
                            result.push_str(&generator.indent());
                            result.push_str(&format!("    local *STDOUT;\n"));
                            result.push_str(&generator.indent());
                            result.push_str(&format!("    open(STDOUT, '>', \\$output_ps_{}) or die \"Cannot redirect STDOUT\";\n", global_counter));
                            result.push_str(&generator.indent());
                            result.push_str(&format!("    {}\n", generator.generate_command(cmd)));
                            result.push_str(&generator.indent());
                            result.push_str(&format!("}}\n"));
                        } else {
                            // Use backticks when not in STDOUT context for simple commands
                            let cmd_str = generator.generate_command_string_for_system(cmd);
                            result.push_str(&generator.indent());
                            result.push_str(&format!("my $output_ps_{} = `{}`;\n", global_counter, cmd_str));
                        }
                        
                        // Write the output to the temporary file
                        let fh_var = format!("fh_ps_{}", global_counter);
                        result.push_str(&generator.indent());
                        result.push_str(&format!("use File::Path qw(make_path);\n"));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("my $temp_dir_{} = dirname(${});\n", global_counter, temp_var));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("if (!-d $temp_dir_{}) {{ make_path($temp_dir_{}); }}\n", global_counter, global_counter));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("open(my ${}, '>', ${}) or die \"Cannot create temp file: $!\\n\";\n", fh_var, temp_var));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("print ${} $output_ps_{};\n", fh_var, global_counter));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("close(${});\n", fh_var));
                        
                        process_sub_files.push((temp_var, format!("{} . '/process_sub_{}.tmp'", get_temp_dir(), global_counter)));
                    }
                    _ => {
                        // Handle other redirect types, but not here-strings or output redirects
                        if !matches!(redirect.operator, RedirectOperator::HereString) && 
                           !matches!(redirect.operator, RedirectOperator::Output | RedirectOperator::Append) {
                            result.push_str(&generator.generate_redirect(redirect));
                        }
                    }
                }
            }
            
            // Now handle the base command with redirect context
            if let Command::Simple(cmd) = &base_command {
                if let Word::Literal(cmd_name, _) = &cmd.name {
                    if cmd_name.is_empty() {
                        // This is a process substitution command with no base command
                        // The redirects have already been processed above
                        return result;
                    }
                    
                    // Special handling for comm command with process substitution
                    if cmd_name == "comm" && !process_sub_files.is_empty() {
//                         eprintln!("DEBUG: Handling comm command with {} process substitution files", process_sub_files.len());
                        if process_sub_files.len() >= 2 {
                            let file1 = &process_sub_files[0];
                            let file2 = &process_sub_files[1];
                            
                            result.push_str(&generator.indent());
                            result.push_str("my @file1_lines;\n");
                            result.push_str(&generator.indent());
                            result.push_str("my @file2_lines;\n");
                            
                            // Read first file
                            result.push_str(&generator.indent());
                            result.push_str(&format!("if (open(my $fh1, '<', ${})) {{\n", file1.0));
                            result.push_str(&generator.indent());
                            result.push_str("    while (my $line = <$fh1>) {\n");
                            result.push_str(&generator.indent());
                            result.push_str("        chomp($line);\n");
                            result.push_str(&generator.indent());
                            result.push_str("        push @file1_lines, $line;\n");
                            result.push_str(&generator.indent());
                            result.push_str("    }\n");
                            result.push_str(&generator.indent());
                            result.push_str("    close($fh1);\n");
                            result.push_str(&generator.indent());
                            result.push_str("}\n");
                            
                            // Read second file
                            result.push_str(&generator.indent());
                            result.push_str(&format!("if (open(my $fh2, '<', ${})) {{\n", file2.0));
                            result.push_str(&generator.indent());
                            result.push_str("    while (my $line = <$fh2>) {\n");
                            result.push_str(&generator.indent());
                            result.push_str("        chomp($line);\n");
                            result.push_str(&generator.indent());
                            result.push_str("        push @file2_lines, $line;\n");
                            result.push_str(&generator.indent());
                            result.push_str("    }\n");
                            result.push_str(&generator.indent());
                            result.push_str("    close($fh2);\n");
                            result.push_str(&generator.indent());
                            result.push_str("}\n");
                            
                            // Create hashes for efficient lookup
                            result.push_str(&generator.indent());
                            result.push_str("my %file1_set = map { $_ => 1 } @file1_lines;\n");
                            result.push_str(&generator.indent());
                            result.push_str("my %file2_set = map { $_ => 1 } @file2_lines;\n");
                            
                            // Find common lines
                            result.push_str(&generator.indent());
                            result.push_str("my @common_lines;\n");
                            result.push_str(&generator.indent());
                            result.push_str("foreach my $line (@file1_lines) {\n");
                            result.push_str(&generator.indent());
                            result.push_str("    if (exists($file2_set{$line})) {\n");
                            result.push_str(&generator.indent());
                            result.push_str("        push @common_lines, $line;\n");
                            result.push_str(&generator.indent());
                            result.push_str("    }\n");
                            result.push_str(&generator.indent());
                            result.push_str("}\n");
                            
                            // Generate output based on suppression flags
                            let mut suppress_col1 = false;
                            let mut suppress_col2 = false;
                            let mut suppress_col3 = false;
                            
                            // Parse options
                            for arg in &cmd.args {
                                if let Word::Literal(s, _) = arg {
                                    if s.starts_with('-') {
                                        if s.contains('1') { suppress_col1 = true; }
                                        if s.contains('2') { suppress_col2 = true; }
                                        if s.contains('3') { suppress_col3 = true; }
                                    }
                                }
                            }
                            
                            result.push_str(&generator.indent());
                            result.push_str("my $result = \"\";\n");
                            
                            if !suppress_col1 {
                                result.push_str(&generator.indent());
                                result.push_str("foreach my $line (@file1_lines) {\n");
                                result.push_str(&generator.indent());
                                result.push_str("    if (!exists($file2_set{$line})) {\n");
                                result.push_str(&generator.indent());
                                result.push_str("        $result .= $line . \"\\n\";\n");
                                result.push_str(&generator.indent());
                                result.push_str("    }\n");
                                result.push_str(&generator.indent());
                                result.push_str("}\n");
                            }
                            
                            if !suppress_col2 {
                                result.push_str(&generator.indent());
                                result.push_str("foreach my $line (@file2_lines) {\n");
                                result.push_str(&generator.indent());
                                result.push_str("    if (!exists($file1_set{$line})) {\n");
                                result.push_str(&generator.indent());
                                result.push_str("        $result .= $line . \"\\n\";\n");
                                result.push_str(&generator.indent());
                                result.push_str("    }\n");
                                result.push_str(&generator.indent());
                                result.push_str("}\n");
                            }
                            
                            if !suppress_col3 {
                                result.push_str(&generator.indent());
                                result.push_str("$result .= join(\"\\n\", @common_lines) . \"\\n\";\n");
                            }
                            
                            // Remove trailing newline and print result
                            result.push_str(&generator.indent());
                            result.push_str("chomp($result);\n");
                            result.push_str(&generator.indent());
                            result.push_str("print $result;\n");
                            result.push_str(&generator.indent());
                            result.push_str("print \"\\n\";\n");
                            
                            return result;
                        }
                    }
                    
                    // Special handling for mapfile command with process substitution
                    if cmd_name == "mapfile" && !process_sub_files.is_empty() {
//                         eprintln!("DEBUG: Handling mapfile command with {} process substitution files", process_sub_files.len());
                        if process_sub_files.len() >= 1 {
                            let input_file = &process_sub_files[0];
                            
                            // Extract the variable name from the args
                            let mut var_name = "MAPFILE".to_string(); // default name
                            let mut trim_trailing = false;
                            
                            for arg in &cmd.args {
                                if let Word::Literal(s, _) = arg {
                                    if s == "-t" {
                                        trim_trailing = true;
                                    } else if !s.starts_with('-') {
                                        var_name = s.clone();
                                    }
                                }
                            }
                            
                            result.push_str(&generator.indent());
                            result.push_str(&format!("my @{} = ();\n", var_name));
                            result.push_str(&generator.indent());
                            result.push_str(&format!("if (open(my $mapfile_fh, '<', ${})) {{\n", input_file.0));
                            result.push_str(&generator.indent());
                            result.push_str("    while (my $line = <$mapfile_fh>) {\n");
                            if trim_trailing {
                                result.push_str(&generator.indent());
                                result.push_str("        chomp($line);\n");
                            }
                            result.push_str(&generator.indent());
                            result.push_str(&format!("        push @{}, $line;\n", var_name));
                            result.push_str(&generator.indent());
                            result.push_str("    }\n");
                            result.push_str(&generator.indent());
                            result.push_str("    close($mapfile_fh);\n");
                            result.push_str(&generator.indent());
                            result.push_str("}\n");
                            
                            return result;
                        }
                    }
                    
                    // For grep with here-string, pass the here-string content
                    if cmd_name == "grep" && has_here_string {
//                         eprintln!("DEBUG: Generating grep with here-string, content: {}", here_string_content);
                        let mut grep_cmd = cmd.clone();
                        // Create a temporary variable for the here-string content
                        let temp_var = format!("here_string_content_{}", generator.get_unique_file_handle());
                        result.push_str(&generator.indent());
                        result.push_str(&format!("my ${} = {};\n", temp_var, here_string_content));
                        
                        // Call the grep generator with the here-string content (pass with $ prefix)
                        let specific_output = generate_grep_command(generator, &grep_cmd, &format!("${}", temp_var), "0", true);
                        // No need to replace input_data since we're passing the full variable name
                        let modified_output = specific_output;
                        result.push_str(&modified_output);
                        
//                         eprintln!("DEBUG: Final grep result: {}", result);
                        return result;
                    }
                    
                    // Special handling for grep -f command with process substitution
                    if cmd_name == "grep" && !process_sub_files.is_empty() {
                        // Check if this is a grep -f command
                        let has_f_flag = cmd.args.iter().any(|arg| {
                            if let Word::Literal(s, _) = arg {
                                s == "-f"
                            } else {
                                false
                            }
                        });
                        
                        if has_f_flag && process_sub_files.len() >= 1 {
//                             eprintln!("DEBUG: Handling grep -f command with {} process substitution files", process_sub_files.len());
                            let pattern_file = &process_sub_files[0];
                            
                            // Create a modified grep command that uses the temporary file as the pattern file
                            let mut modified_grep_cmd = cmd.clone();
                            
                            // Insert the file argument after the -f flag
                            for i in 0..modified_grep_cmd.args.len() {
                                if let Word::Literal(s, _) = &modified_grep_cmd.args[i] {
                                    if s == "-f" {
                                        // Insert the file argument after the -f flag
                                        modified_grep_cmd.args.insert(i + 1, Word::literal(format!("${}", pattern_file.0)));
//                                         eprintln!("DEBUG: Inserted file argument: ${} at position {}", pattern_file.0, i + 1);
                                        break;
                                    }
                                }
                            }
                            
                            let input_var = input_data.unwrap_or("input_data");
//                             eprintln!("DEBUG: Calling generate_grep_command with input_var: {}", input_var);
                            let specific_output = generate_grep_command(generator, &modified_grep_cmd, input_var, "0", true);
                            result.push_str(&specific_output);
                            
//                             eprintln!("DEBUG: Final grep -f result: {}", result);
                            return result;
                        }
                    }
                    
                    // Special handling for diff command with process substitution
                    if cmd_name == "diff" && !process_sub_files.is_empty() {
//                         eprintln!("DEBUG: Handling diff command with {} process substitution files", process_sub_files.len());
                        if process_sub_files.len() >= 2 {
                            let file1 = &process_sub_files[0];
                            let file2 = &process_sub_files[1];
                            
                            // Set environment variables for the diff command
                            result.push_str(&generator.indent());
                            result.push_str(&format!("$ENV{{DIFF_TEMP_FILE1}} = {};\n", file1.1));
                            result.push_str(&generator.indent());
                            result.push_str(&format!("$ENV{{DIFF_TEMP_FILE2}} = {};\n", file2.1));
                            
                            // Generate the actual diff command
                            let diff_output = super::diff::generate_diff_command(generator, cmd, "$output", 0, true);
                            result.push_str(&diff_output);
                            
                            return result;
                        }
                    }
                    
                    // Special handling for paste command with process substitution
                    if cmd_name == "paste" && !process_sub_files.is_empty() {
//                         eprintln!("DEBUG: Handling paste command with {} process substitution files", process_sub_files.len());
                        if process_sub_files.len() >= 2 {
                            let file1 = &process_sub_files[0];
                            let file2 = &process_sub_files[1];
                            
                            // Use the paste generator for proper output handling
                            let paste_output = generate_paste_command(generator, cmd, &process_sub_files);
                            result.push_str(&paste_output);
                            
                            return result;
                        }
                    }
                }
            }
            
            // For other commands, generate normally but don't call recursively
            // Instead, generate the base command directly
//             eprintln!("DEBUG: Generating base command for redirect, has_here_string: {}, command: {:?}", has_here_string, &base_command);
            
            // Check if we have output redirects that need to be wrapped in a local STDOUT block
            let has_output_redirect = all_redirects.iter().any(|r| {
                matches!(r.operator, RedirectOperator::Output | RedirectOperator::Append)
            });
            
            if has_output_redirect {
                result.push_str(&generator.indent());
                result.push_str("{\n");
                generator.indent_level += 1;
                result.push_str(&generator.indent());
                result.push_str("open(my $original_stdout, '>&', STDOUT) or die \"Cannot save STDOUT: $!\";\n");
                
                // Find the output redirect target
                let output_redirect = all_redirects.iter().find(|r| {
                    matches!(r.operator, RedirectOperator::Output | RedirectOperator::Append)
                });
                
                if let Some(redirect) = output_redirect {
                    let target = generator.word_to_perl(&redirect.target);
                    let mode = if matches!(redirect.operator, RedirectOperator::Append) { ">>" } else { ">" };
                    result.push_str(&generator.indent());
                    result.push_str(&format!("open(STDOUT, '{}', '{}') or die \"Cannot open file: $!\";\n", mode, target));
                } else {
                    result.push_str(&generator.indent());
                    result.push_str("open(STDOUT, '>', 'temp_file.txt') or die \"Cannot open file: $!\";\n");
                }
            }
            
            match &base_command {
                Command::Simple(cmd) => {
                    // Special handling for heredocs with perl commands
                    if let Word::Literal(cmd_name, _) = &cmd.name {
                        if cmd_name == "perl" {
                            // Check if we have heredoc redirects
                            let has_heredoc = all_redirects.iter().any(|r| {
                                matches!(r.operator, RedirectOperator::Heredoc | RedirectOperator::HeredocTabs)
                            });
                            
                            if has_heredoc {
                                // For perl heredocs, execute the heredoc content directly as Perl code
                                for redirect in &all_redirects {
                                    if matches!(redirect.operator, RedirectOperator::Heredoc | RedirectOperator::HeredocTabs) {
                                        if let Some(body) = &redirect.heredoc_body {
                                            // Execute the heredoc content directly as Perl code
                                            result.push_str(&generator.indent());
                                            result.push_str(&format!("{}\n", body));
                                            return result;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Special handling for grep -f with process substitution
                    if let Word::Literal(cmd_name, _) = &cmd.name {
//                         eprintln!("DEBUG: Processing simple command: {}", cmd_name);
                        if cmd_name == "grep" {
                            // Check if this is a grep -f command
                            let has_f_flag = cmd.args.iter().any(|arg| {
                                if let Word::Literal(s, _) = arg {
                                    s == "-f"
                                } else {
                                    false
                                }
                            });
                            
//                             eprintln!("DEBUG: has_f_flag: {}, process_sub_files.len(): {}", has_f_flag, process_sub_files.len());
                            
                            if has_f_flag && !process_sub_files.is_empty() {
//                                 eprintln!("DEBUG: Handling grep -f redirect command with {} process substitution files", process_sub_files.len());
                                let pattern_file = &process_sub_files[0];
                                
                                // Create a modified grep command that uses the temporary file as the pattern file
                                let mut modified_grep_cmd = cmd.clone();
                                
                                // Find the -f flag and insert the file argument after it
                                for i in 0..modified_grep_cmd.args.len() {
                                    if let Word::Literal(s, _) = &modified_grep_cmd.args[i] {
                                        if s == "-f" {
                                            // Insert the file argument after the -f flag
                                            modified_grep_cmd.args.insert(i + 1, Word::literal(format!("${}", pattern_file.0)));
//                                             eprintln!("DEBUG: Inserted file argument: ${}", pattern_file.0);
//                                             eprintln!("DEBUG: Modified grep command args: {:?}", modified_grep_cmd.args);
                                            break;
                                        }
                                    }
                                }
                                
                                let input_var = input_data.unwrap_or("input_data");
//                                 eprintln!("DEBUG: Calling generate_grep_command with input_var: {}", input_var);
                                let specific_output = generate_grep_command(generator, &modified_grep_cmd, input_var, "0", true);
                                result.push_str(&specific_output);
                                
//                                 eprintln!("DEBUG: Final grep -f redirect result: {}", result);
                                return result;
                            } else if has_f_flag {
                                // Try to find the temporary file variable from the generated redirects
                                // Look for temp_file_ps_ variables in the current result
                                let lines: Vec<&str> = result.lines().collect();
                                for line in &lines {
                                    if line.contains("temp_file_ps_") && line.contains("=") {
//                                         eprintln!("DEBUG: Examining line for temp file: {}", line);
                                        if let Some(start) = line.find("$temp_file_ps_") {
                                            let var_part = &line[start..];
                                            if let Some(end) = var_part.find([' ', ';', '\'', '=']) {
                                                let temp_var = &var_part[..end];
//                                                 eprintln!("DEBUG: Found process substitution temp file variable: {}", temp_var);
                                                
                                                // Create a modified grep command that uses the temporary file
                                                let mut modified_grep_cmd = cmd.clone();
                                                modified_grep_cmd.args.push(Word::literal(temp_var.to_string()));
                                                
                                                let specific_output = generate_grep_command(generator, &modified_grep_cmd, "input_data", "0", true);
                                                result.push_str(&specific_output);
                                                
//                                                 eprintln!("DEBUG: Final grep -f redirect result with found temp file: {}", result);
                                                return result;
                                            }
                                        }
                                    }
                                }
//                                 eprintln!("DEBUG: No temp_file_ps_ variable found in result: {}", result);
                                
                                // If we can't find the temp file, fall back to generating an error
                                result.push_str("carp \"grep: no pattern specified\";\n");
                                result.push_str("exit(1);\n");
                                return result;
                            }
                        }
                    }
                    
                    result.push_str(&generator.generate_simple_command(cmd));
                }
                Command::BuiltinCommand(cmd) => {
                    result.push_str(&generator.generate_builtin_command(cmd));
                }
                _ => {
                    // For other command types, use the recursive call
                    result.push_str(&generate_command_impl_with_input(generator, &base_command, false, input_data));
                }
            }
            
            if has_output_redirect {
                result.push_str(&generator.indent());
                result.push_str("open(STDOUT, '>&', $original_stdout) or die \"Cannot restore STDOUT: $!\";\n");
                result.push_str(&generator.indent());
                result.push_str("close($original_stdout);\n");
                generator.indent_level -= 1;
                result.push_str(&generator.indent());
                result.push_str("}\n");
            }
//             eprintln!("DEBUG: Final redirect result: {}", result);
            result
        }
    }
}
