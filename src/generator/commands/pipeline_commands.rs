use crate::generator::Generator;
use crate::ast::*;
use crate::ast::PipeOperator;

// Helper function to check if a command is a diff command
fn is_diff_command(command: &Command) -> bool {
    match command {
        Command::Simple(cmd) => {
            match &cmd.name {
                Word::Literal(name) => name == "diff",
                _ => false,
            }
        }
        Command::Redirect(redirect_cmd) => {
            is_diff_command(&redirect_cmd.command)
        }
        _ => false,
    }
}

// Helper function to check if a command contains a diff command (recursively)
fn contains_diff_command(command: &Command) -> bool {
    match command {
        Command::Simple(cmd) => {
            match &cmd.name {
                Word::Literal(name) => name == "diff",
                _ => false,
            }
        }
        Command::Redirect(redirect_cmd) => {
            contains_diff_command(&redirect_cmd.command)
        }
        _ => false,
    }
}

pub fn generate_pipeline_impl(generator: &mut Generator, pipeline: &Pipeline) -> String {
    // Check if this is a logical pipeline (&& or ||) - these should never capture STDOUT
    let has_logical_operators = pipeline.operators.iter().any(|op| matches!(op, PipeOperator::And | PipeOperator::Or));
    let should_print = !has_logical_operators;
    generate_pipeline_with_print_option(generator, pipeline, should_print)
}

pub fn generate_pipeline_with_print_option(generator: &mut Generator, pipeline: &Pipeline, should_print: bool) -> String {
    let mut output = String::new();
    
    if pipeline.commands.len() == 1 {
        // Single command, no pipeline needed
        output.push_str(&generator.generate_command(&pipeline.commands[0]));
    } else {
        // Multiple commands, implement proper Perl pipeline
        // Check if this is a logical pipeline (&& or ||) or a pipe pipeline
        let has_logical_operators = pipeline.operators.iter().any(|op| matches!(op, PipeOperator::And | PipeOperator::Or));
        let has_pipe_operators = pipeline.operators.iter().any(|op| matches!(op, PipeOperator::Pipe));
        

        
        if has_logical_operators && has_pipe_operators && pipeline.commands.len() == 2 {
            // Mixed pipeline with exactly 2 commands - handle as pipe pipeline first, then apply logical operators
            // This is a special case: echo "text" | grep "pattern" || echo "fallback"
            output.push_str(&generator.indent());
            output.push_str("{\n");
            generator.indent_level += 1;
            
            // Use unique variable name to avoid masking
            let unique_id = generator.get_unique_id();
            output.push_str(&generator.indent());
            output.push_str(&format!("my $output_{};\n", unique_id));
            
            // Generate the pipe pipeline first (first command)
            let first_cmd = &pipeline.commands[0];
            if let Command::Simple(cmd) = first_cmd {
                let cmd_name = match &cmd.name {
                    Word::Literal(s) => s,
                    _ => "unknown_command"
                };
                
                if cmd_name == "echo" {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("$output_{} = `", unique_id));
                    output.push_str(&generator.generate_command_string_for_system(first_cmd));
                    output.push_str("`;\n");
                } else {
                    // Generic first command
                    output.push_str(&generator.indent());
                    output.push_str(&format!("$output_{} = `", unique_id));
                    output.push_str(&generator.generate_command_string_for_system(first_cmd));
                    output.push_str("`;\n");
                }
            } else {
                // Non-simple first command
                output.push_str(&generator.indent());
                output.push_str(&format!("$output_{} = `", unique_id));
                output.push_str(&generator.generate_command_string_for_system(first_cmd));
                output.push_str("`;\n");
            }
            
            // Now handle the logical operators for the second command
            let last_operator = pipeline.operators.last().unwrap();
            let last_cmd = pipeline.commands.last().unwrap();
            
            match last_operator {
                PipeOperator::And => {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if ($output_{} ne '') {{\n", unique_id));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(last_cmd));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                }
                PipeOperator::Or => {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if ($output_{} eq '') {{\n", unique_id));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(last_cmd));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                }
                _ => {
                    // Fall back to generic approach
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(last_cmd));
                    output.push_str("\n");
                }
            }
            
            // For mixed pipelines, we don't print the output by default
            // The logical operator determines what gets executed
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        } else if has_logical_operators && has_pipe_operators {
            // Mixed pipeline - handle as pipe pipeline first, then apply logical operators

            output.push_str(&generator.indent());
            output.push_str("{\n");
            generator.indent_level += 1;
            
            // Use unique variable name to avoid masking
            let unique_id = generator.get_unique_id();
            output.push_str(&generator.indent());
            output.push_str(&format!("my $output_{};\n", unique_id));
            output.push_str(&generator.indent());
            output.push_str(&format!("my $exit_code_{};\n", unique_id));
            
            // Find the first logical operator to split the pipeline
            let mut logical_split_index = None;
            for (i, op) in pipeline.operators.iter().enumerate() {
                if matches!(op, PipeOperator::And | PipeOperator::Or) {
                    logical_split_index = Some(i);
                    break;
                }
            }
            
            if let Some(split_idx) = logical_split_index {
                // Generate the pipe pipeline part (commands up to the logical operator)
                let pipe_commands = &pipeline.commands[..=split_idx];
                let pipe_operators = &pipeline.operators[..split_idx];
                
                // Generate pipe pipeline if there are multiple commands in the pipe part
                if pipe_commands.len() > 1 {
                    for (i, command) in pipe_commands.iter().enumerate() {
                        if i == 0 {
                            // First command - generate output
                            if let Command::Simple(cmd) = command {
                                let cmd_name = match &cmd.name {
                                    Word::Literal(s) => s,
                                    _ => "unknown_command"
                                };
                                
                                if cmd_name == "echo" {
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = `", unique_id));
                                    output.push_str(&generator.generate_command_string_for_system(command));
                                    output.push_str("`;\n");
                                } else {
                                    // Generic first command
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = `", unique_id));
                                    output.push_str(&generator.generate_command_string_for_system(command));
                                    output.push_str("`;\n");
                                }
                            } else {
                                // Non-simple first command
                                output.push_str(&generator.indent());
                                output.push_str(&format!("$output_{} = `", unique_id));
                                output.push_str(&generator.generate_command_string_for_system(command));
                                output.push_str("`;\n");
                            }
                        } else {
                            // Subsequent commands in pipe part
                            if let Command::Simple(cmd) = command {
                                let cmd_name = match &cmd.name {
                                    Word::Literal(s) => s,
                                    _ => "unknown_command"
                                };
                                
                                if cmd_name == "grep" {
                                    let grep_unique_id = generator.get_unique_id();
                                    output.push_str(&generator.indent());
                                    output.push_str(&generate_grep_command(generator, cmd, &format!("$output_{}", unique_id), &grep_unique_id, false));
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = $grep_result_{};\n", unique_id, grep_unique_id));
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$exit_code_{} = (@grep_filtered_{} > 0) ? 0 : 1;\n", unique_id, grep_unique_id));
                                } else {
                                    // Generic command in pipe
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = `", unique_id));
                                    output.push_str(&generator.generate_command_string_for_system(command));
                                    output.push_str("`;\n");
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$exit_code_{} = $?;\n", unique_id));
                                }
                            }
                        }
                    }
                } else {
                    // Single command before logical operator
                    let cmd = &pipe_commands[0];
                    if let Command::Simple(simple_cmd) = cmd {
                        let cmd_name = match &simple_cmd.name {
                            Word::Literal(s) => s,
                            _ => "unknown_command"
                        };
                        
                        if cmd_name == "grep" {
                            // Special handling for grep to capture exit code
                            output.push_str(&generator.generate_command(cmd));
                            output.push_str(&generator.indent());
                            output.push_str(&format!("$exit_code_{} = $?;\n", unique_id));
                        } else {
                            output.push_str(&generator.generate_command(cmd));
                            output.push_str(&generator.indent());
                            output.push_str(&format!("$exit_code_{} = $?;\n", unique_id));
                        }
                    } else {
                        output.push_str(&generator.generate_command(cmd));
                        output.push_str(&generator.indent());
                        output.push_str(&format!("$exit_code_{} = $?;\n", unique_id));
                    }
                }
                
                // Now handle the logical operator and remaining commands
                let logical_op = &pipeline.operators[split_idx];
                let remaining_commands = &pipeline.commands[split_idx + 1..];
                
                match logical_op {
                    PipeOperator::And => {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("if ($exit_code_{} == 0) {{\n", unique_id));
                        generator.indent_level += 1;
                        for cmd in remaining_commands {
                            output.push_str(&generator.indent());
                            output.push_str(&generator.generate_command(cmd));
                        }
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                    }
                    PipeOperator::Or => {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("if ($exit_code_{} != 0) {{\n", unique_id));
                        generator.indent_level += 1;
                        for cmd in remaining_commands {
                            output.push_str(&generator.indent());
                            output.push_str(&generator.generate_command(cmd));
                        }
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                    }
                    _ => {}
                }
            }
            
            // For mixed pipelines, we don't print the output by default
            // The logical operator determines what gets executed
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        } else if has_logical_operators && !has_pipe_operators && pipeline.commands.len() == 2 && pipeline.operators.len() == 1 {
            // Pure logical pipeline (&& and || only) - exactly 2 commands with 1 logical operator
            if pipeline.commands.len() == 2 && pipeline.operators.len() == 1 {
                let operator = &pipeline.operators[0];
                let left_cmd = &pipeline.commands[0];
                let right_cmd = &pipeline.commands[1];
                
                match operator {
                    PipeOperator::And => {
                        // Generate: left_cmd && right_cmd
                        output.push_str(&generator.indent());
                        output.push_str("if (");
                        // For RedirectCommand, we need to check exit code
                        if let Command::Redirect(_) = left_cmd {
                            // Generate the redirect command first, then check exit code
                            output.push_str("do {\n");
                            generator.indent_level += 1;
                            output.push_str(&generator.indent());
                            output.push_str(&generator.generate_command(left_cmd));
                            generator.indent_level -= 1;
                            output.push_str(&generator.indent());
                            output.push_str("} == 0");
                        } else {
                            output.push_str(&generator.generate_command(left_cmd));
                        }
                        output.push_str(") {\n");
                        generator.indent_level += 1;
                        output.push_str(&generator.indent());
                        output.push_str(&generator.generate_command(right_cmd));
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                    }
                    PipeOperator::Or => {
                        // Generate: left_cmd || right_cmd
                        // OR pipelines should NEVER capture STDOUT - they're about conditional execution
                        output.push_str(&generator.indent());
                        
                        // Execute left command and check exit code
                        output.push_str(&generator.generate_command(left_cmd));
                        
                        // Execute right command if left command fails
                        // For diff commands, check $diff_exit_code; for others, check $?
                        let exit_code_var = if contains_diff_command(left_cmd) {
                            "$diff_exit_code"
                        } else {
                            "$?"
                        };
                        
                        output.push_str(&generator.indent());
                        output.push_str(&format!("if ({} != 0) {{\n", exit_code_var));
                        generator.indent_level += 1;
                        output.push_str(&generator.indent());
                        output.push_str(&generator.generate_command(right_cmd));
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                    }
                    _ => {
                        // Fall back to generic approach
                        output.push_str(&generator.indent());
                        output.push_str(&generator.generate_command(left_cmd));
                        output.push_str("\n");
                        output.push_str(&generator.indent());
                        output.push_str(&generator.generate_command(right_cmd));
                        output.push_str("\n");
                    }
                }
            } else {
                // Multiple logical operators - handle recursively
                output.push_str(&generator.indent());
                output.push_str("do {\n");
                generator.indent_level += 1;
                
                for (i, (command, operator)) in pipeline.commands.iter().zip(pipeline.operators.iter()).enumerate() {
                    if i > 0 {
                        output.push_str(&generator.indent());
                        match operator {
                            PipeOperator::And => { 
                                output.push_str("if ($? == 0) {\n");
                                generator.indent_level += 1;
                                output.push_str(&generator.indent());
                            }
                            PipeOperator::Or => { 
                                output.push_str("if ($? != 0) {\n");
                                generator.indent_level += 1;
                                output.push_str(&generator.indent());
                            }
                            PipeOperator::Pipe => { output.push_str("| "); }
                        }
                    }
                    output.push_str(&generator.generate_command(command));
                    if i > 0 && matches!(operator, PipeOperator::And | PipeOperator::Or) {
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                    } else {
                        output.push_str("\n");
                    }
                }
                
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
        } else if has_logical_operators && has_pipe_operators {
            // Mixed pipeline with pipe and logical operators - handle as pipe pipeline first, then apply logical operators
            output.push_str(&generator.indent());
            output.push_str("{\n");
            generator.indent_level += 1;
            
            // Use unique variable name to avoid masking
            let unique_id = generator.get_unique_id();
            output.push_str(&generator.indent());
            output.push_str(&format!("my $output_{};\n", unique_id));
            
            // Generate the pipe pipeline first (all commands except the last one)
            for (i, command) in pipeline.commands.iter().take(pipeline.commands.len() - 1).enumerate() {
                if i == 0 {
                    // First command - generate output
                    if let Command::Simple(cmd) = command {
                        let cmd_name = match &cmd.name {
                            Word::Literal(s) => s,
                            _ => "unknown_command"
                        };
                        
                        if cmd_name == "ls" {
                            output.push_str(&generator.indent());
                            output.push_str(&generate_ls_command(generator, cmd, true, Some(&format!("$output_{}", unique_id))));
                        } else if cmd_name == "cat" {
                            output.push_str(&generator.indent());
                            output.push_str(&generate_cat_command(generator, cmd, &cmd.redirects, &format!("$output_{}", unique_id)));
                        } else if cmd_name == "find" {
                            output.push_str(&generator.indent());
                            output.push_str(&generate_find_command(generator, cmd, true, &format!("$output_{}", unique_id)));
                        } else {
                            // Generic first command
                            output.push_str(&generator.indent());
                            if matches!(command, Command::Redirect(_)) {
                                output.push_str(&generator.generate_command(command));
                                output.push_str(&generator.indent());
                                output.push_str(&format!("$output_{} = $output;\n", unique_id));
                            } else {
                                output.push_str(&format!("$output_{} = `", unique_id));
                                output.push_str(&generator.generate_command_string_for_system(command));
                                output.push_str("`;\n");
                            }
                        }
                    } else {
                        // Non-simple first command
                        if matches!(command, Command::Redirect(_)) {
                            output.push_str(&generator.generate_command(command));
                            output.push_str(&generator.indent());
                            output.push_str(&format!("$output_{} = $output;\n", unique_id));
                        } else {
                            output.push_str(&format!("$output_{} = `", unique_id));
                            output.push_str(&generator.generate_command_string_for_system(command));
                            output.push_str("`;\n");
                        }
                    }
                } else {
                    // Subsequent commands - process the output from previous command
                    if let Command::Simple(cmd) = command {
                        let cmd_name = match &cmd.name {
                            Word::Literal(s) => s,
                            _ => "unknown_command"
                        };
                        
                                                if cmd_name == "grep" {
                            let grep_unique_id = generator.get_unique_id();
                            output.push_str(&generator.indent());
                            output.push_str(&generate_grep_command(generator, cmd, &format!("$output_{}", unique_id), &grep_unique_id, false));
                            output.push_str(&generator.indent());
                            output.push_str(&format!("$output_{} = $grep_result_{};\n", unique_id, grep_unique_id));
                        } else if cmd_name == "wc" {
                                let wc_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_wc_command(generator, cmd, &format!("$output_{}", unique_id), &wc_unique_id));
                            } else if cmd_name == "sort" {
                                let sort_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_sort_command(generator, cmd, &format!("$output_{}", unique_id), &sort_unique_id));
                            } else if cmd_name == "uniq" {
                                let uniq_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_uniq_command(generator, cmd, &format!("$output_{}", unique_id), &uniq_unique_id));
                        } else {
                            // Generic command
                            output.push_str(&generator.indent());
                            if matches!(command, Command::Redirect(_)) {
                                output.push_str(&generator.generate_command(command));
                                output.push_str(&generator.indent());
                                output.push_str(&format!("$output_{} = $output;\n", unique_id));
                            } else {
                                output.push_str(&format!("$output_{} = `", unique_id));
                                output.push_str(&generator.generate_command_string_for_system(command));
                                output.push_str("`;\n");
                            }
                        }
                    } else {
                        // Non-simple command
                        if matches!(command, Command::Redirect(_)) {
                            output.push_str(&generator.generate_command(command));
                            output.push_str(&generator.indent());
                            output.push_str(&format!("$output_{} = $output;\n", unique_id));
                        } else {
                            output.push_str(&format!("$output_{} = `", unique_id));
                            output.push_str(&generator.generate_command_string_for_system(command));
                            output.push_str("`;\n");
                        }
                    }
                }
            }
            
            // Now handle the logical operators for the last command
            let last_operator = pipeline.operators.last().unwrap();
            let last_cmd = pipeline.commands.last().unwrap();
            
            match last_operator {
                PipeOperator::And => {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if ($output_{} ne '') {{\n", unique_id));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(last_cmd));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                }
                PipeOperator::Or => {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if ($output_{} eq '') {{\n", unique_id));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(last_cmd));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                }
                _ => {
                    // Fall back to generic approach
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(last_cmd));
                    output.push_str("\n");
                }
            }
            
            // For mixed pipelines, we don't print the output by default
            // The logical operator determines what gets executed
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        } else {
            // Regular pipe-based pipeline
            // Wrap in scoping block for proper variable isolation
            output.push_str("{\n");
            generator.indent_level += 1;
            
            if should_print {
                // For printing pipelines, use array-based approach
                // Use unique variable name to avoid masking
                let unique_id = generator.get_unique_id();
                output.push_str(&generator.indent());
                output.push_str(&format!("my $output_{};\n", unique_id));
                
                // Create a chain of output variables for the pipeline
                let mut current_output_var = format!("$output_{}", unique_id);
                
                for (i, command) in pipeline.commands.iter().enumerate() {
                    if i > 0 {
                        output.push_str("\n");
                    }
                    
                    if i == 0 {
                        // First command - generate output
                        if let Command::Simple(cmd) = command {
                            let cmd_name = match &cmd.name {
                                Word::Literal(s) => s,
                                _ => "unknown_command"
                            };
                            
                            // Check if this is the final command
                            let is_final_command = i == pipeline.commands.len() - 1;
                            
                            if cmd_name == "ls" {
                                output.push_str(&generator.indent());
                                output.push_str(&generate_ls_command(generator, cmd, true, Some(&current_output_var)));
                            } else if cmd_name == "cat" {
                                output.push_str(&generator.indent());
                                output.push_str(&generate_cat_command(generator, cmd, &cmd.redirects, &current_output_var));
                                // cat command already sets the output variable
                            } else if cmd_name == "find" {
                                output.push_str(&generator.indent());
                                output.push_str(&generate_find_command(generator, cmd, true, &current_output_var));
                                // find command already sets the output variable
                            } else {
                                // Generic first command
                                output.push_str(&generator.indent());
                                // For RedirectCommand and other complex commands, use the command dispatcher
                                if matches!(command, Command::Redirect(_)) {
                                    output.push_str(&generator.generate_command(command));
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("{} = $output;\n", current_output_var));
                                } else {
                                    output.push_str(&format!("{} = `", current_output_var));
                                    output.push_str(&generator.generate_command_string_for_system(command));
                                    output.push_str("`;\n");
                                }
                            }
                        } else {
                            // Non-simple first command - handle control flow commands specially
                            match command {
                                Command::For(for_loop) => {
                                    // Generate the for loop code and capture its output
                                    output.push_str(&generator.indent());
                                    output.push_str("{\n");
                                    generator.indent_level += 1;
                                    output.push_str(&generator.indent());
                                    output.push_str("local *STDOUT;\n");
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("open(STDOUT, '>', \\{}) or die \"Cannot redirect STDOUT\";\n", current_output_var));
                                    output.push_str(&generator.indent());
                                    output.push_str(&generator.generate_for_loop(for_loop));
                                    generator.indent_level -= 1;
                                    output.push_str(&generator.indent());
                                    output.push_str("}\n");
                                },
                                _ => {
                                    // Other non-simple commands
                                    output.push_str(&generator.indent());
                                    // Special handling for RedirectCommand - don't use backticks
                                    if let Command::Redirect(_) = command {
                                        output.push_str(&generator.indent());
                                        output.push_str("{\n");
                                        generator.indent_level += 1;
                                        output.push_str(&generator.indent());
                                        output.push_str("local *STDOUT;\n");
                                        output.push_str(&generator.indent());
                                        output.push_str(&format!("open(STDOUT, '>', \\{}) or die \"Cannot redirect STDOUT\";\n", current_output_var));
                                        output.push_str(&generator.indent());
                                        output.push_str(&generator.generate_command_in_stdout_context(command));
                                        generator.indent_level -= 1;
                                        output.push_str(&generator.indent());
                                        output.push_str("}\n");
                                    } else {
                                        // For RedirectCommand and other complex commands, use the command dispatcher
                                        if matches!(command, Command::Redirect(_)) {
                                            output.push_str(&generator.generate_command(command));
                                            // No need for assignment since RedirectCommand handles $output internally  
                                        } else {
                                            output.push_str(&format!("{} = `", current_output_var));
                                            output.push_str(&generator.generate_command_string_for_system(command));
                                            output.push_str("`;\n");
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Handle subsequent commands - they should use the previous command's output
                        if let Command::Simple(cmd) = command {
                            let cmd_name = match &cmd.name {
                                Word::Literal(s) => s,
                                _ => "unknown_command"
                            };
                            
                            if cmd_name == "grep" {
                                let grep_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_grep_command(generator, cmd, &current_output_var, &grep_unique_id, false));
                                output.push_str(&generator.indent());
                                output.push_str(&format!("{} = $grep_result_{};\n", current_output_var, grep_unique_id));
                            } else if cmd_name == "wc" {
                                let wc_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_wc_command(generator, cmd, &current_output_var, &wc_unique_id));
                            } else if cmd_name == "sort" {
                                let sort_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_sort_command(generator, cmd, &current_output_var, &sort_unique_id));
                            } else if cmd_name == "uniq" {
                                let uniq_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_uniq_command(generator, cmd, &current_output_var, &uniq_unique_id));
                            } else if cmd_name == "xargs" {
                                let xargs_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_xargs_command(generator, cmd, &current_output_var, xargs_unique_id.parse().unwrap_or(0)));
                                output.push_str(&generator.indent());
                                output.push_str(&format!("{} = $xargs_result_{};\n", current_output_var, xargs_unique_id));
                            } else if cmd_name == "tr" {
                                let tr_unique_id = generator.get_unique_id();
                                output.push_str(&generator.indent());
                                output.push_str(&generate_tr_command(generator, cmd, &current_output_var, tr_unique_id.parse().unwrap_or(0)));
                                output.push_str(&generator.indent());
                                output.push_str(&format!("{} = $tr_result_{};\n", current_output_var, tr_unique_id));
                            } else {
                                // Generic command
                                output.push_str(&generator.indent());
                                // Special handling for RedirectCommand - don't use backticks
                                if let Command::Redirect(_) = command {
                                    output.push_str("{\n");
                                    generator.indent_level += 1;
                                    output.push_str(&generator.indent());
                                    output.push_str("local *STDOUT;\n");
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("open(STDOUT, '>', \\{}) or die \"Cannot redirect STDOUT\";\n", current_output_var));
                                    output.push_str(&generator.indent());
                                    output.push_str(&generator.generate_command_in_stdout_context(command));
                                    generator.indent_level -= 1;
                                    output.push_str(&generator.indent());
                                    output.push_str("}\n");
                                } else {
                                    // For RedirectCommand and other complex commands, use the command dispatcher
                                    if matches!(command, Command::Redirect(_)) {
                                        output.push_str(&generator.generate_command(command));
                                        // No need for assignment since RedirectCommand handles output internally
                                    } else {
                                        output.push_str(&format!("{} = `", current_output_var));
                                        output.push_str(&generator.generate_command_string_for_system(command));
                                        output.push_str("`;\n");
                                    }
                                }
                            }
                        } else {
                            // Non-simple second command
                            output.push_str(&generator.indent());
                            // Special handling for RedirectCommand - don't use backticks
                            if let Command::Redirect(_) = command {
                                output.push_str("{\n");
                                generator.indent_level += 1;
                                output.push_str(&generator.indent());
                                output.push_str("local *STDOUT;\n");
                                output.push_str(&generator.indent());
                                output.push_str(&format!("open(STDOUT, '>', \\{}) or die \"Cannot redirect STDOUT\";\n", current_output_var));
                                output.push_str(&generator.indent());
                                output.push_str(&generator.generate_command(command));
                                generator.indent_level -= 1;
                                output.push_str(&generator.indent());
                                output.push_str("}\n");
                                                            } else {
                                    // For RedirectCommand and other complex commands, use the command dispatcher
                                    if matches!(command, Command::Redirect(_)) {
                                        output.push_str(&generator.generate_command(command));
                                        // No need for assignment since RedirectCommand handles output internally
                                    } else {
                                        output.push_str(&format!("{} = `", current_output_var));
                                        output.push_str(&generator.generate_command_string_for_system(command));
                                        output.push_str("`;\n");
                                    }
                                }
                        }
                    }
                    
                    // Output the final result only
                    if should_print {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("print $output_{};\n", unique_id));
                        output.push_str(&generator.indent());
                        output.push_str("print \"\\n\";\n");
                    }
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
                        // Use the specialized ls command generation to handle flags like -1
                        let unique_id = generator.get_unique_id();
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my $output_{};\n", unique_id));
                        
                        // Generate ls command with proper flag handling
                        output.push_str(&generator.indent());
                        output.push_str(&generate_ls_command(generator, cmd1, true, Some(&format!("$output_{}", unique_id))));
                        
                        // Now apply grep filtering
                        let mut pattern = String::new();
                        let mut invert_match = false;

                        for arg in &cmd2.args {
                            if let Word::Literal(s) = arg {
                                if s.starts_with('-') {
                                    if s.contains('v') { invert_match = true; }
                                } else {
                                    pattern = s.clone();
                                }
                            } else {
                                pattern = generator.word_to_perl(arg);
                            }
                        }

                        if !pattern.is_empty() {
                            // Remove quotes if they exist around the pattern
                            let regex_pattern = if pattern.starts_with('"') && pattern.ends_with('"') {
                                &pattern[1..pattern.len()-1]
                            } else {
                                &pattern
                            };

                            output.push_str(&generator.indent());
                            output.push_str(&format!("my @lines = split(/\\n/, $output_{});\n", unique_id));
                            
                            if invert_match {
                                // Negative grep: exclude lines that match the pattern
                                output.push_str(&generator.indent());
                                output.push_str(&format!("my @filtered = grep {{ $_ !~ /{}/ }} @lines;\n", regex_pattern));
                            } else {
                                // Positive grep: include lines that match the pattern
                                output.push_str(&generator.indent());
                                output.push_str(&format!("my @filtered = grep {{ $_ =~ /{}/ }} @lines;\n", regex_pattern));
                            }
                            
                            output.push_str(&generator.indent());
                            output.push_str(&format!("$output_{} = join(\"\\n\", @filtered);\n", unique_id));
                        }
                        
                        output.push_str(&generator.indent());
                        output.push_str(&format!("$output_{};\n", unique_id));
                    } else {
                        // Generic 2-command pipeline
                        // Use unique variable name to avoid masking
                        let unique_id = generator.get_unique_id();
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my $output_{};\n", unique_id));
                        
                        // Handle the first command
                        if let Command::For(for_loop) = &pipeline.commands[0] {
                            // Generate the for loop code and capture its output
                            output.push_str(&generator.indent());
                            output.push_str("{\n");
                            generator.indent_level += 1;
                            output.push_str(&generator.indent());
                            output.push_str("local *STDOUT;\n");
                            output.push_str(&generator.indent());
                            output.push_str(&format!("open(STDOUT, '>', \\$output_{}) or die \"Cannot redirect STDOUT\";\n", unique_id));
                            output.push_str(&generator.indent());
                            output.push_str(&generator.generate_for_loop(for_loop));
                            generator.indent_level -= 1;
                            output.push_str(&generator.indent());
                            output.push_str("}\n");
                        } else {
                            output.push_str(&generator.indent());
                            // For RedirectCommand and other complex commands, use the command dispatcher
                            if matches!(&pipeline.commands[0], Command::Redirect(_)) {
                                output.push_str(&generator.generate_command(&pipeline.commands[0]));
                                output.push_str(&generator.indent());
                                output.push_str(&format!("$output_{} = $output;\n", unique_id));
                            } else {
                                output.push_str(&format!("$output_{} = `", unique_id));
                                output.push_str(&generator.generate_command_string_for_system(&pipeline.commands[0]));
                                output.push_str("`;\n");
                            }
                        }
                        
                        // Process remaining commands in the pipeline
                        for command in &pipeline.commands[1..] {
                            if let Command::Simple(cmd) = command {
                                let cmd_name = match &cmd.name {
                                    Word::Literal(s) => s,
                                    _ => "unknown_command"
                                };
                                
                                if cmd_name == "sort" {
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("my @lines = split(/\\n/, $output_{});\n", unique_id));
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = join(\"\\n\", sort @lines);\n", unique_id));
                                } else if cmd_name == "grep" {
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("my @lines = split(/\\n/, $output_{});\n", unique_id));
                                    output.push_str(&generator.indent());
                                    output.push_str("my @filtered = grep { $_ ne '' } @lines;\n");
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = join(\"\\n\", @filtered);\n", unique_id));
                                } else if cmd_name == "xargs" {
                                    let xargs_unique_id = generator.get_unique_id();
                                    output.push_str(&generator.indent());
                                    output.push_str(&generate_xargs_command(generator, cmd, &format!("$output_{}", unique_id), xargs_unique_id.parse().unwrap_or(0)));
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = $xargs_result_{};\n", unique_id, xargs_unique_id));
                                } else if cmd_name == "tr" {
                                    let tr_unique_id = generator.get_unique_id();
                                    output.push_str(&generator.indent());
                                    output.push_str(&generate_tr_command(generator, cmd, &format!("$output_{}", unique_id), tr_unique_id.parse().unwrap_or(0)));
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("$output_{} = $tr_result_{};\n", unique_id, tr_unique_id));
                                } else {
                                    // Generic command processing
                                    output.push_str(&generator.indent());
                                    // For RedirectCommand and other complex commands, use the command dispatcher
                                    if matches!(command, Command::Redirect(_)) {
                                        output.push_str(&generator.generate_command(command));
                                        output.push_str(&generator.indent());
                                        output.push_str(&format!("$output_{} = $output;\n", unique_id));
                                    } else {
                                        output.push_str(&format!("$output_{} = `echo \"$output_{}\" | ", unique_id, unique_id));
                                        output.push_str(&generator.generate_command_string_for_system(command));
                                        output.push_str("`;\n");
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
        }
    }
    
    output
}

// Import all the command generation functions
use super::cat::generate_cat_command;
use super::find::generate_find_command;
use super::ls::generate_ls_command;
use super::grep::generate_grep_command;
use super::wc::generate_wc_command;
use super::sort::generate_sort_command;
use super::uniq::generate_uniq_command;
use super::awk::generate_awk_command;
use super::sed::generate_sed_command;
use super::comm::generate_comm_command;
use super::tr::generate_tr_command;
use super::cut::generate_cut_command;
use super::basename::generate_basename_command;
use super::dirname::generate_dirname_command;
use super::strings::generate_strings_command;
use super::tee::generate_tee_command;
use super::sha256sum::generate_sha256sum_command;
use super::sha512sum::generate_sha512sum_command;
use super::gzip::generate_gzip_command;
use super::kill::generate_kill_command;
use super::nohup::generate_nohup_command;
use super::nice::generate_nice_command;
use super::curl::generate_curl_command;
use super::mkdir::generate_mkdir_command;
use super::rm::generate_rm_command;
use super::cp::generate_cp_command;
use super::mv::generate_mv_command;
use super::touch::generate_touch_command;
use super::head::generate_head_command;
use super::tail::generate_tail_command;
use super::xargs::generate_xargs_command;
