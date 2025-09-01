use crate::ast::*;
use crate::generator::Generator;
use crate::generator::commands::builtins::{is_builtin, generate_generic_builtin, pipeline_supports_linebyline};

/// Helper function to generate Perl code for a command using the builtins registry
fn generate_command_using_builtins(
    generator: &mut Generator, 
    command: &Command, 
    input_var: &str, 
    output_var: &str,
    command_index: &str,
    linebyline: bool
) -> String {
    match command {
        Command::Simple(cmd) => {
            let cmd_name = match &cmd.name {
                Word::Literal(s) => s,
                _ => "unknown_command"
            };
            
            if is_builtin(cmd_name) {
                // Route to specialized modules via generate_generic_builtin
                if input_var.is_empty() {
                    // First command in pipeline - generate without input
                    generate_generic_builtin(generator, cmd, "", output_var, command_index, linebyline)
                } else {
                    // Subsequent command - use previous output as input
                    generate_generic_builtin(generator, cmd, input_var, output_var, command_index, linebyline)
                }
            } else {
                // Non-builtin command - use centralized fallback logic
                generate_generic_builtin(generator, cmd, input_var, output_var, command_index, linebyline)
            }
        },
        Command::For(for_loop) => {
            // Handle for loops in pipeline context
            if input_var.is_empty() {
                // First command in pipeline - generate for loop that outputs to the output variable
                let mut output = String::new();
                output.push_str(&format!("${} = '';\n", output_var));
                output.push_str(&format!("my @{}_items = (", output_var));
                
                // Generate the items list
                let mut all_items = Vec::new();
                for word in &for_loop.items {
                    match word {
                        Word::StringInterpolation(interp) => {
                            if interp.parts.len() == 1 {
                                if let StringPart::Variable(var) = &interp.parts[0] {
                                    match var.as_str() {
                                        "@" => all_items.push("@ARGV".to_string()),
                                        "*" => all_items.push("@ARGV".to_string()),
                                        _ => all_items.push(generator.word_to_perl(word))
                                    }
                                } else if let StringPart::ParameterExpansion(pe) = &interp.parts[0] {
                                    if pe.operator == ParameterExpansionOperator::ArraySlice("@".to_string(), None) {
                                        if pe.variable.starts_with('!') {
                                            let map_name = &pe.variable[1..];
                                            all_items.push(format!("keys %{}", map_name));
                                        } else {
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
                        _ => all_items.push(generator.word_to_perl(word))
                    }
                }
                output.push_str(&all_items.join(", "));
                output.push_str(");\n");
                
                // Generate the for loop body that outputs to the output variable
                output.push_str(&format!("for my ${} (@{}_items) {{\n", for_loop.variable, output_var));
                generator.indent_level += 1;
                
                // Generate the body commands, but capture their output instead of printing
                for cmd in &for_loop.body.commands {
                    if let Command::Simple(simple_cmd) = cmd {
                        if let Word::Literal(cmd_name) = &simple_cmd.name {
                            if cmd_name == "echo" {
                                // For echo commands, append to output variable
                                let echo_args: Vec<String> = simple_cmd.args.iter()
                                    .map(|arg| generator.perl_string_literal(arg))
                                    .collect();
                                let echo_output = echo_args.join(" . ");
                                output.push_str(&generator.indent());
                                output.push_str(&format!("${} .= {} . \"\\n\";\n", output_var, echo_output));
                            } else {
                                // For other commands, execute and capture output
                                output.push_str(&generator.indent());
                                output.push_str(&format!("${} .= `{}`;\n", output_var, generator.generate_command_string_for_system(cmd)));
                            }
                        } else {
                            // For other command types, execute and capture output
                            output.push_str(&generator.indent());
                            output.push_str(&format!("${} .= `{}`;\n", output_var, generator.generate_command_string_for_system(cmd)));
                        }
                    } else {
                        // For non-simple commands, execute and capture output
                        output.push_str(&generator.indent());
                        output.push_str(&format!("${} .= `{}`;\n", output_var, generator.generate_command_string_for_system(cmd)));
                    }
                }
                
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                
                output
            } else {
                // Subsequent command - this shouldn't happen for for loops, but handle gracefully
                format!("# For loop as subsequent command in pipeline not supported\n")
            }
        },
        _ => {
            // Other non-simple commands - use system call fallback
            if input_var.is_empty() {
                // First command in pipeline
                format!("${} = `{}`;\n", 
                    output_var, 
                    generator.generate_command_string_for_system(command))
            } else {
                // Subsequent command
                format!("${} = `echo \"${}\" | {}`;\n", 
                    output_var, input_var, 
                    generator.generate_command_string_for_system(command))
            }
        }
    }
}



/// Generate a simple pipe pipeline (no logical operators)
pub fn generate_pipeline_impl(generator: &mut Generator, pipeline: &Pipeline) -> String {
    // This is now a pure pipe pipeline since logical operators are handled separately
    generate_simple_pipe_pipeline(generator, pipeline, true)
}

/// Generate a simple pipe pipeline with print option
pub fn generate_pipeline_with_print_option(generator: &mut Generator, pipeline: &Pipeline, should_print: bool) -> String {
    let mut output = String::new();
    
    if pipeline.commands.len() == 1 {
        // Single command, no pipeline needed
        output.push_str(&generator.generate_command(&pipeline.commands[0]));
    } else {
        // Multiple commands, implement proper Perl pipeline
        output.push_str(&generate_simple_pipe_pipeline(generator, pipeline, should_print));
    }
    
    output
}

/// Generate a simple pipe pipeline (commands connected with |)
fn generate_simple_pipe_pipeline(generator: &mut Generator, pipeline: &Pipeline, should_print: bool) -> String {
    // Check if we can use line-by-line processing
    if pipeline_supports_linebyline(pipeline) {
        generate_streaming_pipeline(generator, pipeline, should_print)
    } else {
        generate_buffered_pipeline(generator, pipeline, should_print)
    }
}

/// Generate a streaming pipeline that processes one line at a time
fn generate_streaming_pipeline(generator: &mut Generator, pipeline: &Pipeline, should_print: bool) -> String {
    let mut output = String::new();
    
    // Add original bash command as comment if available
    if let Some(source_text) = &pipeline.source_text {
        // Handle multiline source text by only taking the first line (the actual pipeline)
        let first_line = source_text.lines().next().unwrap_or(source_text);
        output.push_str(&generator.indent());
        output.push_str(&format!("# Original bash: {}\n", first_line));
    }
    
    // Check if the first command is 'cat filename' or 'echo' and handle it specially
    let mut start_index = 0;
    if let Command::Simple(first_cmd) = &pipeline.commands[0] {
        if let Word::Literal(name) = &first_cmd.name {
            if name == "cat" && !first_cmd.args.is_empty() {
                // First command is 'cat filename', so read from the file instead of STDIN
                let filename = generator.perl_string_literal(&first_cmd.args[0]);
                // For relative filenames, use current directory
                let adjusted_filename = if !filename.contains('/') && !filename.starts_with('.') && filename != "\"\"" {
                    format!("\"./{}\"", filename.trim_matches('"'))
                } else {
                    filename.clone()
                };
                output.push_str(&generator.indent());
                output.push_str(&format!("if (open(my $fh, '<', {})) {{\n", adjusted_filename));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("while (my $line = <$fh>) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("chomp $line;\n");
                start_index = 1; // Skip the cat command since we're handling it
            }
        }
    }
    
    if start_index == 0 {
        // No special handling, read from STDIN
        output.push_str(&generator.indent());
        output.push_str("while (my $line = <STDIN>) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("chomp $line;\n");
        
        // No variable declarations needed for streaming pipeline - we process each line directly
        
        // Process each line through the remaining pipeline commands
        for (i, command) in pipeline.commands[start_index..].iter().enumerate() {
            if let Command::Simple(cmd) = command {
                let cmd_name = match &cmd.name {
                    Word::Literal(s) => s,
                    _ => "unknown_command"
                };
                
                // Generate line-by-line version of each command
                output.push_str(&generator.indent());
                output.push_str(&generate_linebyline_command(generator, cmd, "line", start_index + i));
            }
        }
        
        // Output the processed line
        if should_print {
            output.push_str(&generator.indent());
            output.push_str("print $line . \"\\n\";\n");
        }
        
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else if start_index == 1 {
        // For cat commands, we need to add the command processing inside the while loop
        // No variable declarations needed for streaming pipeline - we process each line directly
        
        // Process each line through the remaining pipeline commands
        for (i, command) in pipeline.commands[start_index..].iter().enumerate() {
            if let Command::Simple(cmd) = command {
                let cmd_name = match &cmd.name {
                    Word::Literal(s) => s,
                    _ => "unknown_command"
                };
                
                // Generate line-by-line version of each command
                output.push_str(&generator.indent());
                output.push_str(&generate_linebyline_command(generator, cmd, "line", start_index + i));
            }
        }
        
        // Output the processed line
        if should_print {
            output.push_str(&generator.indent());
            output.push_str("print $line . \"\\n\";\n");
        }
        
        // Close the while loop and file handle
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        output.push_str(&generator.indent());
        output.push_str("close($fh);\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("} else {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("warn \"cat: can't open file\";\n");
        output.push_str(&generator.indent());
        output.push_str("exit(1);\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }
    
    output
}

/// Generate line-by-line processing for a single command
fn generate_linebyline_command(generator: &mut Generator, cmd: &SimpleCommand, line_var: &str, cmd_index: usize) -> String {
    let cmd_name = match &cmd.name {
        Word::Literal(s) => s,
        _ => "unknown_command"
    };
    
    match cmd_name {
        "tr" => {
            crate::generator::commands::tr::generate_tr_command(generator, cmd, line_var, &format!("{}", cmd_index), true)
        },
        "grep" => {
            // For grep, we need to check if the line matches and skip if it doesn't
            let mut output = String::new();
            if let Some(pattern_arg) = cmd.args.iter().find(|arg| {
                if let Word::Literal(s) = arg { !s.starts_with('-') } else { true }
            }) {
                let pattern = generator.strip_shell_quotes_for_regex(pattern_arg);
                output.push_str(&format!("next unless $line =~ /{}/;\n", pattern));
            }
            output
        },
        "sed" => {
            // For sed, we'll use basic substitution for now
            let mut output = String::new();
            if let Some(sed_expr) = cmd.args.iter().find(|arg| {
                if let Word::Literal(s) = arg { s.starts_with('s') } else { false }
            }) {
                let expr = generator.word_to_perl(sed_expr);
                output.push_str(&format!("$line =~ {expr};\n"));
            }
            output
        },
        "cut" => {
            // For cut, extract specific fields
            let mut output = String::new();
            if let Some(fields_arg) = cmd.args.iter().find(|arg| {
                if let Word::Literal(s) = arg { s.starts_with('-') && s.contains('f') } else { false }
            }) {
                // Extract field specification and apply cut logic
                output.push_str(&format!("# cut processing for {}\n", fields_arg));
            }
            output
        },
        "wc" => {
            // For wc, count characters/words in the line
            let mut output = String::new();
            output.push_str("$char_count += length($line);\n");
            output.push_str("$word_count += scalar(split(/\\s+/, $line));\n");
            output.push_str("$line_count++;\n");
            output
        },
        _ => {
            // Fallback for unsupported commands
            format!("# {} doesn't support line-by-line processing\n", cmd_name)
        }
    }
}

/// Generate a buffered pipeline that processes all input at once
fn generate_buffered_pipeline(generator: &mut Generator, pipeline: &Pipeline, should_print: bool) -> String {
    let mut output = String::new();
    
    // Add original bash command as comment if available
    if let Some(source_text) = &pipeline.source_text {
        // Handle multiline source text by only taking the first line (the actual pipeline)
        let first_line = source_text.lines().next().unwrap_or(source_text);
        output.push_str(&generator.indent());
        output.push_str(&format!("# Original bash: {}\n", first_line));
    }
    
    if should_print {
        // Wrap the entire pipeline in a block scope to prevent variable contamination
        output.push_str("{\n");
        generator.indent_level += 1;
        
        // For printing pipelines, use proper command chaining
        let unique_id = generator.get_unique_id();
        output.push_str(&generator.indent());
        output.push_str(&format!("my $output_{};\n", unique_id));
        
        // Declare result variables only for commands that emit separate result vars
        for (i, command) in pipeline.commands.iter().enumerate() {
            if let Command::Simple(cmd) = command {
                if let Word::Literal(cmd_name) = &cmd.name {
                    if matches!(cmd_name.as_str(), "grep" | "wc" | "xargs" | "tr") {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${}_result_{}_{};\n", cmd_name, unique_id, i));
                    }
                }
            }
        }
        
        // Track pipeline success for proper exit code handling
        output.push_str(&generator.indent());
        output.push_str(&format!("my $pipeline_success_{} = 1;\n", unique_id));
        
        for (i, command) in pipeline.commands.iter().enumerate() {
            if i > 0 {
                output.push_str("\n");
            }
            
            if i == 0 {
                // First command - generate output
                output.push_str(&generator.indent());
                if matches!(command, Command::Redirect(_)) {
                    output.push_str(&generator.generate_command(command));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("$output_{} = $output;\n", unique_id));
                } else {
                    // Handle the first command - use generate_command_using_builtins for all command types
                    let command_output = generate_command_using_builtins(generator, command, "", &format!("output_{}", unique_id), &format!("{}_{}", unique_id, i), false);
                    
                    // Split the output into lines and apply indentation
                    for line in command_output.lines() {
                        if !line.trim().is_empty() {
                            output.push_str(&generator.indent());
                            output.push_str(line);
                            if !line.ends_with('\n') {
                                output.push_str("\n");
                            }
                        }
                    }
                    
                    // Check if the first command failed (e.g., cat with non-existent file)
                    // If the output is empty, the command likely failed
                    if let Command::Simple(cmd) = command {
                        if let Word::Literal(cmd_name) = &cmd.name {
                            if cmd_name == "cat" {
                                output.push_str(&generator.indent());
                                output.push_str(&format!("if ($output_{} eq '') {{\n", unique_id));
                                output.push_str(&generator.indent());
                                output.push_str(&format!("    $pipeline_success_{} = 0;\n", unique_id));
                                output.push_str(&generator.indent());
                                output.push_str("}\n");
                            }
                        }
                    }
                }
            } else {
                // Handle subsequent commands - they should use the previous command's output
                output.push_str(&generator.indent());
                if matches!(command, Command::Redirect(_)) {
                    output.push_str("{\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str("local *STDOUT;\n");
                    output.push_str(&generator.indent());
                    output.push_str(&format!("open(STDOUT, '>', \\{}) or die \"Cannot redirect STDOUT\";\n", format!("$output_{}", unique_id)));
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(command));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                } else {
                    // Use proper command chaining without echo
                    if let Command::Simple(cmd) = command {
                        let cmd_name = match &cmd.name {
                            Word::Literal(s) => s,
                            _ => "unknown_command"
                        };
                        
                        // Use the builtins registry to handle command generation
                        // Pass the previous command's output as input to this command
                        let command_output = generate_command_using_builtins(generator, command, &format!("output_{}", unique_id), &format!("output_{}", unique_id), &format!("{}_{}", unique_id, i), false);
                        
                        // Split the output into lines and apply indentation
                        for line in command_output.lines() {
                            if !line.trim().is_empty() {
                                output.push_str(&generator.indent());
                                output.push_str(line);
                                if !line.ends_with('\n') {
                                    output.push_str("\n");
                                }
                            }
                        }
                        
                        // For builtin commands, ensure output assignment for those with separate result vars
                        if is_builtin(cmd_name) {
                            if matches!(cmd_name, "grep" | "wc" | "xargs" | "tr") {
                                let result_var = format!("{}_result_{}_{}", cmd_name, unique_id, i);
                                output.push_str(&generator.indent());
                                output.push_str(&format!("$output_{} = ${};\n", unique_id, result_var));
                                if cmd_name == "grep" {
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("if (scalar(@grep_filtered_{}_{}) == 0) {{\n", unique_id, i));
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("    $pipeline_success_{} = 0;\n", unique_id));
                                    output.push_str(&generator.indent());
                                    output.push_str("}\n");
                                }
                            }
                        } else {
                            // For non-builtin commands, use centralized fallback logic
                            if let Command::Simple(cmd) = command {
                                let fallback_output = generate_generic_builtin(generator, cmd, &format!("output_{}", unique_id), &format!("output_{}", unique_id), &format!("{}_{}", unique_id, i), false);
                                for line in fallback_output.lines() {
                                    if !line.trim().is_empty() {
                                        output.push_str(&generator.indent());
                                        output.push_str(line);
                                        if !line.ends_with('\n') {
                                            output.push_str("\n");
                                        }
                                    }
                                }
                            } else {
                                // Non-simple command - use system call fallback
                                output.push_str(&generator.indent());
                                output.push_str(&format!("$output_{} = `echo \"$output_{}\" | {}`;\n", 
                                    unique_id, unique_id, generator.generate_command_string_for_system(command)));
                            }
                        }
                    } else {
                        // Non-simple command - use system call fallback
                        output.push_str(&generator.indent());
                        output.push_str(&format!("$output_{} = `echo \"$output_{}\" | ", unique_id, unique_id));
                        output.push_str(&generator.generate_command_string_for_system(command));
                        output.push_str("`;\n");
                    }
                }
            }
        }
        
        // Output the final result
        if should_print {
            output.push_str(&generator.indent());
            output.push_str(&format!("print $output_{};\n", unique_id));
            // Ensure output ends with newline to match shell behavior
            output.push_str(&generator.indent());
            output.push_str(&format!("print \"\\n\" unless $output_{} =~ /\\n$/;\n", unique_id));
        }
        
        // Track pipeline success for overall script exit code
        output.push_str(&generator.indent());
        output.push_str(&format!("$main_exit_code = 1 unless $pipeline_success_{};\n", unique_id));
        
        generator.indent_level -= 1;
        output.push_str("}\n");
    } else {
        // For command substitution, use streaming approach
        // Wrap in block scope to prevent variable contamination
        output.push_str("{\n");
        generator.indent_level += 1;
        
        if let (Command::Simple(cmd1), Command::Simple(cmd2)) = (&pipeline.commands[0], &pipeline.commands[1]) {
            let cmd1_name = match &cmd1.name {
                Word::Literal(s) => s,
                _ => "unknown_command"
            };
            let cmd2_name = match &cmd2.name {
                Word::Literal(s) => s,
                _ => "unknown_command"
            };

            if cmd1_name == "ls" && cmd2_name == "grep" {
                // Use the builtins registry for ls+grep combination
                let unique_id = generator.get_unique_id();
                output.push_str(&generator.indent());
                output.push_str(&format!("my $output_{};\n", unique_id));
                
                // Track pipeline success for proper exit code handling
                output.push_str(&generator.indent());
                output.push_str(&format!("my $pipeline_success_{} = 1;\n", unique_id));
                
                // Generate ls command using builtins
                let ls_output = generate_command_using_builtins(generator, &pipeline.commands[0], "", &format!("output_{}", unique_id), &format!("{}_0", unique_id), false);
                for line in ls_output.lines() {
                    if !line.trim().is_empty() {
                        output.push_str(&generator.indent());
                        output.push_str(line);
                        if !line.ends_with('\n') {
                            output.push_str("\n");
                        }
                    }
                }
                
                // Now apply grep filtering using builtins
                let grep_output = generate_command_using_builtins(generator, &pipeline.commands[1], &format!("output_{}", unique_id), &format!("output_{}", unique_id), &format!("{}_1", unique_id), false);
                for line in grep_output.lines() {
                    if !line.trim().is_empty() {
                        output.push_str(&generator.indent());
                        output.push_str(line);
                        if !line.ends_with('\n') {
                            output.push_str("\n");
                        }
                    }
                }
                
                // Track exit code for grep (exit 1 if no matches found)
                output.push_str(&generator.indent());
                output.push_str(&format!("if (scalar(@grep_filtered_{}_1) == 0) {{\n", unique_id));
                output.push_str(&generator.indent());
                output.push_str(&format!("    $pipeline_success_{} = 0;\n", unique_id));
                output.push_str(&generator.indent());
                output.push_str("}\n");
                
                output.push_str(&generator.indent());
                output.push_str(&format!("$output_{};\n", unique_id));
                
                // Track pipeline success for overall script exit code
                output.push_str(&generator.indent());
                output.push_str(&format!("$main_exit_code = 1 unless $pipeline_success_{};\n", unique_id));
            } else {
                // Generic 2-command pipeline
                let unique_id = generator.get_unique_id();
                output.push_str(&generator.indent());
                output.push_str(&format!("my $output_{};\n", unique_id));
                
                // Track pipeline success for proper exit code handling
                output.push_str(&generator.indent());
                output.push_str(&format!("my $pipeline_success_{} = 1;\n", unique_id));
                
                // Handle the first command
                output.push_str(&generator.indent());
                if matches!(&pipeline.commands[0], Command::Redirect(_)) {
                    output.push_str(&generator.generate_command(&pipeline.commands[0]));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("$output_{} = $output;\n", unique_id));
                } else {
                    // Use centralized fallback logic for the first command
                    let fallback_output = generate_command_using_builtins(generator, &pipeline.commands[0], "", &format!("output_{}", unique_id), &format!("{}_0", unique_id), false);
                    for line in fallback_output.lines() {
                        if !line.trim().is_empty() {
                            output.push_str(&generator.indent());
                            output.push_str(line);
                            if !line.ends_with('\n') {
                                output.push_str("\n");
                            }
                        }
                    }
                }
                
                // Process remaining commands in the pipeline
                for (i, command) in pipeline.commands[1..].iter().enumerate() {
                    if let Command::Simple(cmd) = command {
                        let cmd_name = match &cmd.name {
                            Word::Literal(s) => s,
                            _ => "unknown_command"
                        };
                        
                        // Use the builtins registry for all commands
                        let command_output = generate_command_using_builtins(generator, command, &format!("output_{}", unique_id), &format!("output_{}", unique_id), &format!("{}_{}", unique_id, i + 1), false);
                        
                        // Split the output into lines and apply indentation
                        for line in command_output.lines() {
                            if !line.trim().is_empty() {
                                output.push_str(&generator.indent());
                                output.push_str(line);
                                if !line.ends_with('\n') {
                                    output.push_str("\n");
                                }
                            }
                        }
                        
                        // Track exit code for grep commands (exit 1 if no matches found)
                        if cmd_name == "grep" {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("if (scalar(@grep_filtered_{}_{}) == 0) {{\n", unique_id, i + 1));
                            output.push_str(&generator.indent());
                            output.push_str(&format!("    $pipeline_success_{} = 0;\n", unique_id));
                            output.push_str(&generator.indent());
                            output.push_str("}\n");
                        }
                    }
                }
                
                output.push_str(&generator.indent());
                output.push_str(&format!("$output_{};\n", unique_id));
                
                // Track pipeline success for overall script exit code
                output.push_str(&generator.indent());
                output.push_str(&format!("$main_exit_code = 1 unless $pipeline_success_{};\n", unique_id));
            }
        }
        generator.indent_level -= 1;
        output.push_str("}\n");
    }
    
    output
}
