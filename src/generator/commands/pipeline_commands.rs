use crate::ast::*;
use crate::generator::Generator;
use crate::generator::commands::builtins::{is_builtin, generate_generic_builtin, pipeline_supports_linebyline};

/// Helper function to generate Perl code for a command using the builtins registry
fn generate_command_using_builtins(
    generator: &mut Generator, 
    command: &Command, 
    input_var: &str, 
    output_var: &str,
    command_index: &str
) -> String {
    if let Command::Simple(cmd) = command {
        let cmd_name = match &cmd.name {
            Word::Literal(s) => s,
            _ => "unknown_command"
        };
        
        if is_builtin(cmd_name) {
            // Route to specialized modules via generate_generic_builtin
            if input_var.is_empty() {
                // First command in pipeline - generate without input
                generate_generic_builtin(generator, cmd, "", output_var, command_index)
            } else {
                // Subsequent command - use previous output as input
                generate_generic_builtin(generator, cmd, input_var, output_var, command_index)
            }
        } else {
            // Non-builtin command - use system call
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
    } else {
        // Non-simple command - use system call
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
    
    output.push_str("{\n");
    generator.indent_level += 1;
    
    // Generate streaming code that processes one line at a time
    output.push_str(&generator.indent());
    output.push_str("while (my $line = <STDIN>) {\n");
    generator.indent_level += 1;
    
    // Process each line through the entire pipeline
    for (i, command) in pipeline.commands.iter().enumerate() {
        if let Command::Simple(cmd) = command {
            let cmd_name = match &cmd.name {
                Word::Literal(s) => s,
                _ => "unknown_command"
            };
            
            // Generate line-by-line version of each command
            output.push_str(&generator.indent());
            output.push_str(&generate_linebyline_command(generator, cmd, "$line", i));
        }
    }
    
    // Output the processed line
    if should_print {
        output.push_str(&generator.indent());
        output.push_str("print $line;\n");
    }
    
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
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
            crate::generator::commands::tr::generate_tr_command(generator, cmd, line_var, cmd_index, true)
        },
        "grep" => {
            // For grep, we need to check if the line matches and skip if it doesn't
            let mut output = String::new();
            if let Some(pattern_arg) = cmd.args.iter().find(|arg| {
                if let Word::Literal(s) = arg { !s.starts_with('-') } else { true }
            }) {
                let pattern = generator.word_to_perl(pattern_arg);
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

/// Generate a buffered pipeline that processes entire input at once
fn generate_buffered_pipeline(generator: &mut Generator, pipeline: &Pipeline, should_print: bool) -> String {
    let mut output = String::new();
    
    // Wrap in scoping block for proper variable isolation
    output.push_str("{\n");
    generator.indent_level += 1;
    
            if should_print {
            // For printing pipelines, use proper command chaining
            let unique_id = generator.get_unique_id();
            output.push_str(&generator.indent());
            output.push_str(&format!("my $output_{};\n", unique_id));
            
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
                        // Use the builtins registry for the first command too
                        let command_output = generate_command_using_builtins(generator, command, "", &format!("output_{}", unique_id), &format!("cmd_{}", i));
                        
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
                            let command_output = generate_command_using_builtins(generator, command, &format!("output_{}", unique_id), &format!("output_{}", unique_id), &format!("cmd_{}", i));
                            
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
                            
                            // Update the output variable with the result from this command
                            // This ensures the next command in the pipeline uses this command's output
                            if let Command::Simple(cmd) = command {
                                let cmd_name = match &cmd.name {
                                    Word::Literal(s) => s,
                                    _ => "unknown_command"
                                };
                                
                                // For builtin commands, the output should already be in the correct variable
                                // For non-builtin commands, we need to ensure the output is properly assigned
                                if !is_builtin(cmd_name) {
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = `echo \"$output_{}\" | {}`;\n", 
                                        unique_id, unique_id, generator.generate_command_string_for_system(command)));
                                }
                            }
                        } else {
                            // Non-simple command
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
                output.push_str(&generator.indent());
                output.push_str("print \"\\n\";\n");
            }
        } else {
        // For command substitution, use streaming approach
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
                
                // Generate ls command using builtins
                let ls_output = generate_command_using_builtins(generator, &pipeline.commands[0], "", &format!("output_{}", unique_id), "cmd_0");
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
                let grep_output = generate_command_using_builtins(generator, &pipeline.commands[1], &format!("output_{}", unique_id), &format!("output_{}", unique_id), "cmd_1");
                for line in grep_output.lines() {
                    if !line.trim().is_empty() {
                        output.push_str(&generator.indent());
                        output.push_str(line);
                        if !line.ends_with('\n') {
                            output.push_str("\n");
                        }
                    }
                }
                
                output.push_str(&generator.indent());
                output.push_str(&format!("$output_{};\n", unique_id));
            } else {
                // Generic 2-command pipeline
                let unique_id = generator.get_unique_id();
                output.push_str(&generator.indent());
                output.push_str(&format!("my $output_{};\n", unique_id));
                
                // Handle the first command
                output.push_str(&generator.indent());
                if matches!(&pipeline.commands[0], Command::Redirect(_)) {
                    output.push_str(&generator.generate_command(&pipeline.commands[0]));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("$output_{} = $output;\n", unique_id));
                } else {
                    output.push_str(&format!("$output_{} = `", unique_id));
                    output.push_str(&generator.generate_command_string_for_system(&pipeline.commands[0]));
                    output.push_str("`;\n");
                }
                
                // Process remaining commands in the pipeline
                for (i, command) in pipeline.commands[1..].iter().enumerate() {
                    if let Command::Simple(cmd) = command {
                        let cmd_name = match &cmd.name {
                            Word::Literal(s) => s,
                            _ => "unknown_command"
                        };
                        
                        // Use the builtins registry for all commands
                        let command_output = generate_command_using_builtins(generator, command, &format!("output_{}", unique_id), &format!("output_{}", unique_id), &format!("cmd_{}", i + 1));
                        
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
                    }
                }
                
                output.push_str(&generator.indent());
                output.push_str(&format!("$output_{};\n", unique_id));
            }
        }
    }
    
    // Close the scoping block
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}
