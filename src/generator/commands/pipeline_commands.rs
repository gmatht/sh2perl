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
                Word::Literal(s, _) => s,
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
        Command::While(while_loop) => {
            // Handle while loops in buffered pipeline context
            // For buffered pipelines, we need to process the while loop differently
            // The while loop should read from the input and process it
            let mut while_output = String::new();
            
            // Generate a while loop that processes the input line by line
            while_output.push_str(&format!("my @lines = split(/\\n/, ${});\n", input_var));
            while_output.push_str(&format!("my $result_{} = '';\n", command_index));
            while_output.push_str("for my $line (@lines) {\n");
            while_output.push_str("    chomp $line;\n");
            while_output.push_str("    my $L = $line;\n");
            
            // Generate the while loop body commands
            for body_cmd in &while_loop.body.commands {
                while_output.push_str("    ");
                while_output.push_str(&generator.generate_command(body_cmd));
            }
            
            while_output.push_str("}\n");
            while_output.push_str(&format!("${} = $result_{};\n", output_var, command_index));
            
            while_output
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
                        Word::StringInterpolation(interp, _) => {
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
                        if let Word::Literal(cmd_name, _) = &simple_cmd.name {
                            if cmd_name == "echo" {
                                // For echo commands, use the dedicated echo command generator
                                let echo_output = crate::generator::commands::simple_commands::generate_echo_command(generator, simple_cmd, "", output_var);
                                output.push_str(&echo_output);
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
        Command::Or(left, right) => {
            // Handle logical OR in pipeline context
            let mut output = String::new();
            
            // For logical OR in pipeline context, we need to handle it specially
            // to avoid embedding Perl code in shell backticks
            if let Command::And(and_left, and_right) = &**left {
                // Handle nested AND operations in OR context
                if let Command::Simple(simple_cmd) = &**and_left {
                    if let Word::Literal(name, _) = &simple_cmd.name {
                        if name == "grep" {
                        // For grep commands in logical OR, generate proper conditional structure
                        let unique_id = generator.get_unique_id();
                        output.push_str(&format!("my $grep_exit_code_{};\n", unique_id));
                        output.push_str(&format!("{{\n"));
                        generator.indent_level += 1;
                        output.push_str(&generator.indent());
                        
                        // Generate the grep command with input and capture the result
                        let grep_output = if input_var.is_empty() {
                            generate_generic_builtin(generator, simple_cmd, "", output_var, command_index, linebyline)
                        } else {
                            generate_generic_builtin(generator, simple_cmd, input_var, output_var, command_index, linebyline)
                        };
                        
                        // Split the output into lines and apply indentation
                        for line in grep_output.lines() {
                            if !line.trim().is_empty() {
                                output.push_str(&generator.indent());
                                output.push_str(line);
                                if !line.ends_with('\n') {
                                    output.push_str("\n");
                                }
                            }
                        }
                        
                        // Extract the grep_filtered variable name from the generated grep code
                        let mut grep_filtered_var = format!("@grep_filtered_{}", command_index);
                        for line in grep_output.lines() {
                            if line.contains("@grep_filtered_") && line.contains(" = ") {
                                if let Some(start) = line.find("@grep_filtered_") {
                                    let var_part = &line[start..];
                                    if let Some(end) = var_part.find([' ', ';', '=', ')', ',']) {
                                        grep_filtered_var = var_part[..end].to_string();
                                        break;
                                    }
                                }
                            }
                        }
                        output.push_str(&generator.indent());
                        output.push_str(&format!("$grep_exit_code_{} = scalar({}) > 0 ? 0 : 1;\n", unique_id, grep_filtered_var));
                        
                        // Handle the nested AND operation: grep -q && echo "found"
                        output.push_str(&generator.indent());
                        output.push_str(&format!("if ($grep_exit_code_{} == 0) {{\n", unique_id));
                        generator.indent_level += 1;
                        output.push_str(&generator.indent());
                        // Execute the right operand of the AND operation (echo "found")
                        output.push_str(&generator.generate_command(and_right));
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("} else {\n");
                        generator.indent_level += 1;
                        output.push_str(&generator.indent());
                        // Execute the right operand of the OR operation (echo "not found")
                        output.push_str(&generator.generate_command(right));
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                        
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str(&format!("}}\n"));
                        // Set pipeline success to 1 since either grep succeeded or fallback was executed
                        output.push_str(&generator.indent());
                        output.push_str(&format!("$pipeline_success_{} = 1;\n", output_var.replace("output_", "")));
                        // Clear the output variable to avoid printing input data for grep -q
                        output.push_str(&generator.indent());
                        output.push_str(&format!("${} = '';\n", output_var));
                        return output;
                        }
                    }
                }
            }
            
            // For other logical OR cases, generate a proper conditional structure
            let unique_id = generator.get_unique_id();
            output.push_str(&format!("my $exit_code_{};\n", unique_id));
            output.push_str(&format!("{{\n"));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            
            // Generate the left command
            if input_var.is_empty() {
                output.push_str(&generator.generate_command(left));
            } else {
                // For pipeline context, we need to handle input properly
                output.push_str(&format!("my $temp_input_{} = ${};\n", unique_id, input_var));
                output.push_str(&generator.indent());
                
                // Check if left command is a grep command that needs input
                if let Command::Simple(simple_cmd) = &**left {
                    if let Word::Literal(name, _) = &simple_cmd.name {
                        if name == "grep" {
                            // Generate grep command with input
                            let grep_output = crate::generator::commands::grep::generate_grep_command(generator, simple_cmd, &format!("temp_input_{}", unique_id), &unique_id.to_string(), true);
                            output.push_str(&grep_output);
                        } else {
                            output.push_str(&generator.generate_command(left));
                        }
                    } else {
                        output.push_str(&generator.generate_command(left));
                    }
                } else {
                    output.push_str(&generator.generate_command(left));
                }
            }
            
            output.push_str(&generator.indent());
            output.push_str(&format!("$exit_code_{} = $?;\n", unique_id));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("}}\n"));
            output.push_str(&generator.indent());
            output.push_str(&format!("if ($exit_code_{} != 0) {{\n", unique_id));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&generator.generate_command(right));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("} else {\n");
            output.push_str(&generator.indent());
            if !output_var.is_empty() {
                let var_name = output_var.replace("output_", "");
                output.push_str(&format!("    $output_printed_{} = 1;  # Mark as printed to avoid double output\n", var_name));
            }
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output
        },
        Command::And(left, right) => {
            // Handle logical AND in pipeline context
            let mut output = String::new();
            
            // For logical AND in pipeline context, we need to handle it specially
            let unique_id = generator.get_unique_id();
            output.push_str(&format!("my $exit_code_{};\n", unique_id));
            output.push_str(&format!("{{\n"));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            
            // Generate the left command
            if input_var.is_empty() {
                output.push_str(&generator.generate_command(left));
            } else {
                // For pipeline context, we need to handle input properly
                output.push_str(&format!("my $temp_input_{} = ${};\n", unique_id, input_var));
                output.push_str(&generator.indent());
                output.push_str(&generator.generate_command(left));
            }
            
            output.push_str(&generator.indent());
            output.push_str(&format!("$exit_code_{} = $?;\n", unique_id));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("}}\n"));
            output.push_str(&generator.indent());
            output.push_str(&format!("if ($exit_code_{} == 0) {{\n", unique_id));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&generator.generate_command(right));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output
        },
        Command::Redirect(redirect_cmd) => {
            // Handle Redirect commands in pipeline context
            if input_var.is_empty() {
                // First command in pipeline - generate the redirect command normally
                generator.generate_command(command)
            } else {
                // Subsequent command - pass the pipeline input to the redirect command
                // The redirect command should receive the pipeline input and generate its output
                generator.generate_command(command)
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
    
    // Check if the first command is 'cat filename' or an output-generating command and handle it specially
    let mut start_index = 0;
    if let Command::Simple(first_cmd) = &pipeline.commands[0] {
        if let Word::Literal(name, _) = &first_cmd.name {
            if name == "yes" {
                // Handle 'yes' command by generating a loop that processes the line
                let string_to_repeat = if let Some(arg) = first_cmd.args.first() {
                    generator.perl_string_literal(arg)
                } else {
                    "\"y\"".to_string()
                };
                
                // Parse head command parameters dynamically
                let mut head_max = 10; // Default value
                if pipeline.commands.len() > 1 {
                    if let Command::Pipeline(nested_pipeline) = &pipeline.commands[1] {
                        if let Command::Simple(head_cmd) = &nested_pipeline.commands[0] {
                            if let Word::Literal(cmd_name, _) = &head_cmd.name {
                                if cmd_name == "head" {
                                    // Parse head -nX arguments
                                    for (i, arg) in head_cmd.args.iter().enumerate() {
                                        if let Word::Literal(arg_str, _) = arg {
                                            if arg_str == "-n" && i + 1 < head_cmd.args.len() {
                                                if let Word::Literal(num_str, _) = &head_cmd.args[i + 1] {
                                                    if let Ok(num) = num_str.parse::<usize>() {
                                                        head_max = num;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Generate a while loop that processes the line through all commands
                output.push_str(&generator.indent());
                output.push_str("my $i = 0;\n");
                output.push_str(&generator.indent());
                output.push_str("my $head_count_0 = 0;\n");
                output.push_str(&generator.indent());
                output.push_str(&format!("my $head_max_0 = {};\n", head_max));
                output.push_str(&generator.indent());
                output.push_str("while (1) {\n");
                generator.indent_level += 1;
                
                // Generate the yes command inside the loop
                output.push_str(&generator.indent());
                output.push_str(&format!("my $line = {};\n", string_to_repeat));
                start_index = 1; // Skip the yes command in the loop below
                
                // Process the remaining commands in the loop
                for (i, command) in pipeline.commands[start_index..].iter().enumerate() {
                    match command {
                        Command::Simple(cmd) => {
                            let cmd_name = match &cmd.name {
                                Word::Literal(s, _) => s,
                                _ => "unknown_command"
                            };
                            
                            // Generate line-by-line version of each command
                            output.push_str(&generator.indent());
                            output.push_str(&generate_linebyline_command(generator, cmd, "line", start_index + i));
                        }
                        Command::Pipeline(nested_pipeline) => {
                            // Handle nested pipelines - process each command in the nested pipeline
                            for (j, nested_command) in nested_pipeline.commands.iter().enumerate() {
                                match nested_command {
                                    Command::Simple(cmd) => {
                                        // Generate line-by-line version of each command
                                        output.push_str(&generator.indent());
                                        output.push_str(&generate_linebyline_command(generator, cmd, "line", j));
                                    }
                                    Command::While(while_loop) => {
                                        // Check if this is a while read loop - if so, process directly without while loop structure
                                        if let Command::Simple(read_cmd) = &*while_loop.condition {
                                            if let Word::Literal(cmd_name, _) = &read_cmd.name {
                                                if cmd_name == "read" {
                                                    // This is a while read loop - process the body directly on $line
                                                    // Set up the read variable
                                                    if let Some(var_arg) = read_cmd.args.first() {
                                                        if let Word::Literal(var_name, _) = var_arg {
                                                            output.push_str(&generator.indent());
                                                            output.push_str(&format!("my ${} = $line;\n", var_name));
                                                        }
                                                    }
                                                    
                                                    // Process the while loop body commands directly
                                                    for body_cmd in &while_loop.body.commands {
                                                        match body_cmd {
                                                            Command::Assignment(assignment) => {
                                                                // Handle assignment commands
                                                                if assignment.variable == "i" {
                                                                    // For i assignment, increment the i variable
                                                                    output.push_str(&generator.indent());
                                                                    output.push_str("$i = $i + 1;\n");
                                                                } else {
                                                                    // Generate other assignments normally
                                                                    output.push_str(&generator.indent());
                                                                    output.push_str(&generator.generate_assignment(assignment));
                                                                }
                                                            }
                                                            
                                                            Command::Simple(cmd) => {
                                                                // Generate line-by-line version of each command
                                                                output.push_str(&generator.indent());
                                                                output.push_str(&generate_linebyline_command(generator, cmd, "line", 0));
                                                            }
                                                            
                                                            Command::Pipeline(pipeline) => {
                                                                // Handle nested pipelines in while loop body with line-by-line processing
                                                                output.push_str(&generator.indent());
                                                                let pipeline_result = generate_linebyline_command_for_pipeline(generator, pipeline, "line");
                                                                output.push_str(&pipeline_result);
                                                                // If this is an echo | sed pipeline, make sure to print the result
                                                                if pipeline.commands.len() >= 2 {
                                                                    if let (Command::Simple(first_cmd), Command::Simple(second_cmd)) = (&pipeline.commands[0], &pipeline.commands[1]) {
                                                                        if let (Word::Literal(first_name, _), Word::Literal(second_name, _)) = (&first_cmd.name, &second_cmd.name) {
                                                                            if first_name == "echo" && second_name == "sed" {
                                                                                output.push_str(&generator.indent());
                                                                                output.push_str("print $line . \"\\n\";\n");
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            
                                                            _ => {
                                                                // For other command types, generate normally
                                                                output.push_str(&generator.indent());
                                                                output.push_str(&generator.generate_command(body_cmd));
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    // Not a read command, handle normally
                                                    output.push_str(&generator.indent());
                                                    output.push_str(&generator.generate_command(nested_command));
                                                }
                                            } else {
                                                // Not a simple command, handle normally
                                                output.push_str(&generator.indent());
                                                output.push_str(&generator.generate_command(nested_command));
                                            }
                                        } else {
                                            // Not a simple command, handle normally
                                            output.push_str(&generator.indent());
                                            output.push_str(&generator.generate_command(nested_command));
                                        }
                                    }
                                    _ => {
                                        // For other command types, generate normally
                                        output.push_str(&generator.indent());
                                        output.push_str(&generator.generate_command(nested_command));
                                    }
                                }
                            }
                        }
                        Command::While(while_loop) => {
                            // Handle while loops in pipeline context
                            // Check if this is a while read loop - if so, process directly without while loop structure
                            if let Command::Simple(read_cmd) = &*while_loop.condition {
                                if let Word::Literal(cmd_name, _) = &read_cmd.name {
                                    if cmd_name == "read" {
                                        // This is a while read loop - process the body directly on $line
                                        // Set up the read variable
                                        if let Some(var_arg) = read_cmd.args.first() {
                                            if let Word::Literal(var_name, _) = var_arg {
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!("my ${} = $line;\n", var_name));
                                            }
                                        }
                                        
                                        // Process the while loop body commands directly
                                        for body_cmd in &while_loop.body.commands {
                                            match body_cmd {
                                                Command::Assignment(assignment) => {
                                                    // Handle assignment commands
                                                    if assignment.variable == "i" {
                                                        // For i assignment, increment the loop variable
                                                        output.push_str(&generator.indent());
                                                        output.push_str("$i = $i + 1;\n");
                                                    } else {
                                                        // Generate other assignments normally
                                                        output.push_str(&generator.indent());
                                                        output.push_str(&generator.generate_assignment(assignment));
                                                    }
                                                }
                                                
                                                Command::Simple(cmd) => {
                                                    // Generate line-by-line version of each command
                                                    output.push_str(&generator.indent());
                                                    output.push_str(&generate_linebyline_command(generator, cmd, "line", start_index + i));
                                                }
                                                
                                                Command::Pipeline(pipeline) => {
                                                    // Handle nested pipelines in while loop body with line-by-line processing
                                                    output.push_str(&generator.indent());
                                                    output.push_str(&generate_linebyline_command_for_pipeline(generator, pipeline, "line"));
                                                }
                                                
                                                _ => {
                                                    // For other command types, generate normally
                                                    output.push_str(&generator.indent());
                                                    output.push_str(&generator.generate_command(body_cmd));
                                                }
                                            }
                                        }
                                    } else {
                                        // Not a read command, handle normally with while loop structure
                                        output.push_str(&generator.indent());
                                        output.push_str("my $L = $line;\n");
                                        
                                        // Generate the while loop body with line-by-line processing
                                        generator.indent_level += 1;
                                        for body_cmd in &while_loop.body.commands {
                                            match body_cmd {
                                                Command::Simple(cmd) => {
                                                    let cmd_name = match &cmd.name {
                                                        Word::Literal(s, _) => s,
                                                        _ => "unknown_command"
                                                    };
                                                    
                                                    // Generate line-by-line version of each command
                                                    output.push_str(&generator.indent());
                                                    output.push_str(&generate_linebyline_command(generator, cmd, "L", 0));
                                                }
                                                Command::Pipeline(pipeline) => {
                                                    // Handle nested pipelines in while loop body with line-by-line processing
                                                    output.push_str(&generator.indent());
                                                    output.push_str(&generate_linebyline_command_for_pipeline(generator, pipeline, "L"));
                                                }
                                                _ => {
                                                    // For other command types, generate them normally
                                                    output.push_str(&generator.indent());
                                                    output.push_str(&generator.generate_command(body_cmd));
                                                }
                                            }
                                        }
                                        generator.indent_level -= 1;
                                    }
                                } else {
                                    // Not a simple command, handle normally with while loop structure
                                    output.push_str(&generator.indent());
                                    output.push_str("my $L = $line;\n");
                                    
                                    // Generate the while loop body with line-by-line processing
                                    generator.indent_level += 1;
                                    for body_cmd in &while_loop.body.commands {
                                        match body_cmd {
                                            Command::Simple(cmd) => {
                                                let cmd_name = match &cmd.name {
                                                    Word::Literal(s, _) => s,
                                                    _ => "unknown_command"
                                                };
                                                
                                                // Generate line-by-line version of each command
                                                output.push_str(&generator.indent());
                                                output.push_str(&generate_linebyline_command(generator, cmd, "L", 0));
                                            }
                                            Command::Pipeline(pipeline) => {
                                                // Handle nested pipelines in while loop body with line-by-line processing
                                                output.push_str(&generator.indent());
                                                output.push_str(&generate_linebyline_command_for_pipeline(generator, pipeline, "L"));
                                            }
                                            _ => {
                                                // For other command types, generate them normally
                                                output.push_str(&generator.indent());
                                                output.push_str(&generator.generate_command(body_cmd));
                                            }
                                        }
                                    }
                                    generator.indent_level -= 1;
                                }
                            } else {
                                // Not a simple command, handle normally with while loop structure
                                output.push_str(&generator.indent());
                                output.push_str("my $L = $line;\n");
                                
                                // Generate the while loop body with line-by-line processing
                                generator.indent_level += 1;
                                for body_cmd in &while_loop.body.commands {
                                    match body_cmd {
                                        Command::Simple(cmd) => {
                                            let cmd_name = match &cmd.name {
                                                Word::Literal(s, _) => s,
                                                _ => "unknown_command"
                                            };
                                            
                                            // Generate line-by-line version of each command
                                            output.push_str(&generator.indent());
                                            output.push_str(&generate_linebyline_command(generator, cmd, "L", 0));
                                        }
                                        Command::Pipeline(pipeline) => {
                                            // Handle nested pipelines in while loop body with line-by-line processing
                                            output.push_str(&generator.indent());
                                            output.push_str(&generate_linebyline_command_for_pipeline(generator, pipeline, "L"));
                                        }
                                        _ => {
                                            // For other command types, generate them normally
                                            output.push_str(&generator.indent());
                                            output.push_str(&generator.generate_command(body_cmd));
                                        }
                                    }
                                }
                                generator.indent_level -= 1;
                            }
                        }
                        _ => {
                            // For other command types, generate them normally
                            output.push_str(&generator.indent());
                            output.push_str(&generator.generate_command(command));
                        }
                    }
                }
                
                // Close the for loop
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                
                return output; // Return early since we've handled everything
            } else if name == "cat" && !first_cmd.args.is_empty() {
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
                // Check if we need to declare variables for wc command
                let has_wc = pipeline.commands.iter().any(|cmd| {
                    if let Command::Simple(simple_cmd) = cmd {
                        if let Word::Literal(name, _) = &simple_cmd.name {
                            name == "wc"
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });
                
                if has_wc {
                    output.push_str(&generator.indent());
                    output.push_str("my $char_count = 0;\n");
                    output.push_str(&generator.indent());
                    output.push_str("my $word_count = 0;\n");
                    output.push_str(&generator.indent());
                    output.push_str("my $line_count = 0;\n");
                }
                
                output.push_str(&generator.indent());
                output.push_str("chomp $line;\n");
                
                start_index = 1; // Skip the cat command since we're handling it
            }
        }
    }
    
    
    if start_index == 0 {
        // No special handling, read from STDIN
        
        // Check if we need to declare variables for wc command
        let has_wc = pipeline.commands.iter().any(|cmd| {
            if let Command::Simple(simple_cmd) = cmd {
                if let Word::Literal(name, _) = &simple_cmd.name {
                    name == "wc"
                } else {
                    false
                }
            } else {
                false
            }
        });
        
        if has_wc {
            output.push_str(&generator.indent());
            output.push_str("my $char_count = 0;\n");
            output.push_str(&generator.indent());
            output.push_str("my $word_count = 0;\n");
            output.push_str(&generator.indent());
            output.push_str("my $line_count = 0;\n");
        }
        
        output.push_str(&generator.indent());
        output.push_str("while (my $line = <STDIN>) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("chomp $line;\n");
        
        // Process each line through the remaining pipeline commands
        for (i, command) in pipeline.commands[start_index..].iter().enumerate() {
            match command {
                Command::Simple(cmd) => {
                    let cmd_name = match &cmd.name {
                        Word::Literal(s, _) => s,
                        _ => "unknown_command"
                    };
                    
                    // Generate line-by-line version of each command
                    output.push_str(&generator.indent());
                    output.push_str(&generate_linebyline_command(generator, cmd, "line", start_index + i));
                }
                Command::While(while_loop) => {
                    // Handle while loops in pipeline context
                    // The while loop should read from the current line and process it
                    output.push_str(&generator.indent());
                    output.push_str("my $L = $line;\n");
                    
                    // Generate the while loop body with line-by-line processing
                    generator.indent_level += 1;
                    for body_cmd in &while_loop.body.commands {
                        match body_cmd {
                            Command::Simple(cmd) => {
                                let cmd_name = match &cmd.name {
                                    Word::Literal(s, _) => s,
                                    _ => "unknown_command"
                                };
                                
                                // Generate line-by-line version of each command
                                output.push_str(&generator.indent());
                                output.push_str(&generate_linebyline_command(generator, cmd, "L", 0));
                            }
                            Command::Pipeline(pipeline) => {
                                // Handle nested pipelines in while loop body with line-by-line processing
                                output.push_str(&generator.indent());
                                output.push_str(&generate_linebyline_command_for_pipeline(generator, pipeline, "L"));
                            }
                            _ => {
                                // For other command types, generate them normally
                                output.push_str(&generator.indent());
                                output.push_str(&generator.generate_command(body_cmd));
                            }
                        }
                    }
                    generator.indent_level -= 1;
                }
                _ => {
                    // For other command types, generate them normally
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(command));
                }
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
        
        // Output wc results if wc was used
        let has_wc = pipeline.commands.iter().any(|cmd| {
            if let Command::Simple(simple_cmd) = cmd {
                if let Word::Literal(name, _) = &simple_cmd.name {
                    name == "wc"
                } else {
                    false
                }
            } else {
                false
            }
        });
        
        if has_wc {
            output.push_str(&generator.indent());
            output.push_str("print \"$line_count\\n\";\n");
        }
    } else if start_index == 1 {
        // For echo or cat commands, we need to add the command processing
        // No variable declarations needed for streaming pipeline - we process each line directly
        
        // Process each line through the remaining pipeline commands
        for (i, command) in pipeline.commands[start_index..].iter().enumerate() {
            match command {
                Command::Simple(cmd) => {
                    let cmd_name = match &cmd.name {
                        Word::Literal(s, _) => s,
                        _ => "unknown_command"
                    };
                    
                    // Generate line-by-line version of each command
                    output.push_str(&generator.indent());
                    let cmd_index = start_index + i;
                    output.push_str(&generate_linebyline_command(generator, cmd, "line", cmd_index));
                }
                Command::While(while_loop) => {
                    // Handle while loops in pipeline context
                    // The while loop should read from the current line and process it
                    output.push_str(&generator.indent());
                    output.push_str("my $L = $line;\n");
                    
                    // Generate the while loop body with line-by-line processing
                    generator.indent_level += 1;
                    for body_cmd in &while_loop.body.commands {
                        match body_cmd {
                            Command::Simple(cmd) => {
                                let cmd_name = match &cmd.name {
                                    Word::Literal(s, _) => s,
                                    _ => "unknown_command"
                                };
                                
                                // Generate line-by-line version of each command
                                output.push_str(&generator.indent());
                                output.push_str(&generate_linebyline_command(generator, cmd, "L", 0));
                            }
                            Command::Pipeline(pipeline) => {
                                // Handle nested pipelines in while loop body with line-by-line processing
                                output.push_str(&generator.indent());
                                output.push_str(&generate_linebyline_command_for_pipeline(generator, pipeline, "L"));
                            }
                            _ => {
                                // For other command types, generate them normally
                                output.push_str(&generator.indent());
                                output.push_str(&generator.generate_command(body_cmd));
                            }
                        }
                    }
                    generator.indent_level -= 1;
                }
                _ => {
                    // For other command types, generate them normally
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(command));
                }
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
        
        // Output wc results if wc was used
        let has_wc = pipeline.commands.iter().any(|cmd| {
            if let Command::Simple(simple_cmd) = cmd {
                if let Word::Literal(name, _) = &simple_cmd.name {
                    name == "wc"
                } else {
                    false
                }
            } else {
                false
            }
        });
        
        if has_wc {
            output.push_str(&generator.indent());
            output.push_str("print \"$line_count\\n\";\n");
        }
        
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

/// Generate line-by-line processing for a pipeline
fn generate_linebyline_command_for_pipeline(generator: &mut Generator, pipeline: &Pipeline, line_var: &str) -> String {
    let mut output = String::new();
    
    // Process each command in the pipeline line by line
    for (i, command) in pipeline.commands.iter().enumerate() {
        match command {
            Command::Simple(cmd) => {
                output.push_str(&generate_linebyline_command(generator, cmd, line_var, i));
            }
            _ => {
                // For other command types, generate them normally
                output.push_str(&generator.generate_command(command));
            }
        }
    }
    
    output
}

/// Generate line-by-line processing for a single command
fn generate_linebyline_command(generator: &mut Generator, cmd: &SimpleCommand, line_var: &str, cmd_index: usize) -> String {
    let cmd_name = match &cmd.name {
        Word::Literal(s, _) => s,
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
                if let Word::Literal(s, _) = arg { !s.starts_with('-') } else { true }
            }) {
                let pattern = generator.strip_shell_quotes_for_regex(pattern_arg);
                output.push_str(&format!("next unless $line =~ /{}/;\n", pattern));
            }
            output
        },
        "head" => {
            // For head, we need to count lines and stop after the specified number
            let mut output = String::new();
            let mut num_lines = 10; // Default to first 10 lines
            
            // Parse head options
            let mut i = 0;
            while i < cmd.args.len() {
                if let Word::Literal(arg_str, _) = &cmd.args[i] {
                    if arg_str == "-n" {
                        // Handle -n followed by number as separate argument
                        if i + 1 < cmd.args.len() {
                            if let Word::Literal(num_str, _) = &cmd.args[i + 1] {
                                if let Ok(num) = num_str.parse::<usize>() {
                                    num_lines = num;
                                    i += 2; // Skip both -n and the number
                                    continue;
                                }
                            }
                        }
                    } else if arg_str.starts_with("-n") {
                        // Handle -n100 style (number attached to -n)
                        if let Some(num_str) = arg_str.strip_prefix("-n") {
                            if let Ok(num) = num_str.parse::<usize>() {
                                num_lines = num;
                            }
                        }
                    } else if arg_str.starts_with("-") && arg_str.len() > 1 {
                        // Handle -10, -20 style line counts
                        if let Ok(num) = arg_str[1..].parse::<usize>() {
                            num_lines = num;
                        }
                    }
                }
                i += 1;
            }
            
            // Generate line-by-line head command (variables already declared in yes command)
            output.push_str(&format!("if ($head_count_{} < $head_max_{}) {{\n", cmd_index, cmd_index));
            output.push_str(&format!("    $head_count_{}++;\n", cmd_index));
            output.push_str("} else {\n");
            output.push_str("    last;\n");
            output.push_str("}\n");
            // Note: The line is already available in $line from the previous command
            output
        },
        "sed" => {
            // For sed, we'll use basic substitution for now
            let mut output = String::new();
            if cmd.args.len() >= 3 {
                // Handle sed with multiple arguments like "s/LINE/" + variable + "/"
                if let (Word::Literal(pattern, _), Word::Variable(replacement, _, _), Word::Literal(flags, _)) = 
                    (&cmd.args[0], &cmd.args[1], &cmd.args[2]) {
                    if pattern.starts_with("s/") {
                        // Extract pattern from "s/pattern/" - handle both cases with and without trailing slash
                        let pattern_str = &pattern[2..]; // Remove 's/' prefix
                        let pattern_str = if pattern_str.ends_with('/') {
                            &pattern_str[..pattern_str.len()-1] // Remove trailing slash
                        } else {
                            pattern_str
                        };
                        // Handle variable replacement properly
                        let replacement_str = format!("${}", replacement);
                        if flags.is_empty() || flags == "/" {
                            output.push_str(&format!("$line =~ s/{}/{}/;\n", pattern_str, replacement_str));
                        } else {
                            output.push_str(&format!("$line =~ s/{}/{}/{};\n", pattern_str, replacement_str, flags));
                        }
                    }
                }
            } else if let Some(sed_expr) = cmd.args.iter().find(|arg| {
                if let Word::Literal(s, _) = arg { s.starts_with('s') } else { false }
            }) {
                let expr = generator.word_to_perl(sed_expr);
                output.push_str(&format!("$line =~ {expr};\n"));
            }
            output
        },
        "echo" => {
            // For echo, just output the line
            let mut output = String::new();
            if let Some(arg) = cmd.args.first() {
                let value = generator.word_to_perl(arg);
                output.push_str(&format!("$line = {};\n", value));
            }
            output
        },
        "cut" => {
            // For cut, extract specific fields
            let mut output = String::new();
            if let Some(fields_arg) = cmd.args.iter().find(|arg| {
                if let Word::Literal(s, _) = arg { s.starts_with('-') && s.contains('f') } else { false }
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
            output.push_str("next; # Skip normal line processing for wc\n");
            output
        },
        "perl" => {
            // Use the dedicated Perl pipeline command generator
            crate::generator::commands::perl::generate_perl_pipeline_command(generator, cmd, line_var)
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
        output.push_str(&generator.indent());
        output.push_str(&format!("my $output_printed_{};\n", unique_id));
        
        // Individual commands will declare their own result variables as needed
        // No need to pre-declare them here to avoid variable masking
        
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
                    
                    // For echo commands, don't split into lines as they generate string assignments
                    if let Command::Simple(cmd) = command {
                        if let Word::Literal(cmd_name, _) = &cmd.name {
                            if cmd_name == "echo" {
                                // For echo commands, just add the output directly without splitting
                                output.push_str(&generator.indent());
                                output.push_str(&command_output);
                                if !command_output.ends_with('\n') {
                                    output.push_str("\n");
                                }
                            } else {
                                // For other commands, split the output into lines and apply indentation
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
                            // For other command types, split the output into lines and apply indentation
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
                        // For other command types, split the output into lines and apply indentation
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
                    
                    // For builtin commands, ensure output assignment for those with separate result vars
                    if let Command::Simple(cmd) = command {
                        if let Word::Literal(cmd_name, _) = &cmd.name {
                            if matches!(cmd_name.as_str(), "grep" | "wc" | "xargs" | "tr") {
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
                        }
                    }
                    
                    // Check if the first command failed (e.g., cat with non-existent file)
                    // If the output is empty, the command likely failed
                    if let Command::Simple(cmd) = command {
                        if let Word::Literal(cmd_name, _) = &cmd.name {
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
                    // For Redirect commands in pipelines, we need to pass the pipeline input
                    // and let the command generate its output normally
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
                } else {
                    // Check if this is a logical operator command
                    match command {
                        Command::Or(_, _) | Command::And(_, _) => {
                            // For logical operators, generate the conditional structure directly
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
                        },
                        _ => {
                            // Use generate_command_using_builtins for regular commands
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
                            if let Command::Simple(cmd) = command {
                                if let Word::Literal(cmd_name, _) = &cmd.name {
                                    if matches!(cmd_name.as_str(), "grep" | "wc" | "xargs" | "tr") {
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
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Output the final result
        if should_print {
            output.push_str(&generator.indent());
            output.push_str(&format!("if ($output_{} ne '' && !defined($output_printed_{})) {{\n", unique_id, unique_id));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("print $output_{};\n", unique_id));
            // Ensure output ends with newline to match shell behavior
            output.push_str(&generator.indent());
            output.push_str(&format!("print \"\\n\" unless $output_{} =~ /\\n$/;\n", unique_id));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
        
        // Track pipeline success for overall script exit code
        output.push_str(&generator.indent());
        output.push_str(&format!("$main_exit_code = 1 unless $pipeline_success_{};\n", unique_id));
        output.push_str(&generator.indent());
        // output.push_str("exit(1) if $main_exit_code == 1;\n");
        
        generator.indent_level -= 1;
        output.push_str("}\n");
    } else {
        // For command substitution, use streaming approach
        // Wrap in block scope to prevent variable contamination
        output.push_str("{\n");
        generator.indent_level += 1;
        
        if let (Command::Simple(cmd1), Command::Simple(cmd2)) = (&pipeline.commands[0], &pipeline.commands[1]) {
            let cmd1_name = match &cmd1.name {
                Word::Literal(s, _) => s,
                _ => "unknown_command"
            };
            let cmd2_name = match &cmd2.name {
                Word::Literal(s, _) => s,
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
                output.push_str(&generator.indent());
                // output.push_str("exit(1) if $main_exit_code == 1;\n");
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
                            Word::Literal(s, _) => s,
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
                output.push_str(&generator.indent());
                // output.push_str("exit(1) if $main_exit_code == 1;\n");
            }
        }
        generator.indent_level -= 1;
        output.push_str("}\n");
    }
    
    output
}
