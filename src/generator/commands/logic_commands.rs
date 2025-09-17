use crate::ast::*;
use crate::generator::Generator;

/// Generate logical AND operation (left && right)
pub fn generate_logical_and(generator: &mut Generator, left: &Command, right: &Command) -> String {
    let mut output = String::new();
    
    // Generate: left && right
    output.push_str(&generator.indent());
    
    // For TestExpression, use the test expression directly as the condition
    if let Command::TestExpression(_) = left {
        output.push_str("if (");
        let test_result = generator.generate_command(left);
        output.push_str(&test_result);
        output.push_str(") {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
        output.push_str(&generator.indent());
        output.push_str("$CHILD_ERROR = 0;\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("} else {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("$CHILD_ERROR = 1;\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        return output;
    }
    
    // For other commands, use the original pattern with exit code checking
    output.push_str("if (");
    
    // For RedirectCommand, we need to check exit code
    if let Command::Redirect(_) = left {
        // Generate the redirect command first, then check exit code
        output.push_str("do {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(left));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("} == 0");
    } else if let Command::Simple(simple_cmd) = left {
        if let Word::Literal(name, _) = &simple_cmd.name {
            if name == "grep" {
                // For grep commands in logical AND, generate the command in a block
                // and check if it found any matches
                output.push_str("do {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                let grep_result = generator.generate_command(left);
                
                // Extract the grep_result variable name from the generated code
                let mut _grep_result_var = String::new();
                for line in grep_result.lines() {
                    if line.trim_start().starts_with("my $grep_result_") {
                        if let Some(end) = line.find(';') {
                            let var_decl = &line[3..end]; // Remove "my " prefix
                            _grep_result_var = var_decl.to_string();
                        }
                    }
                    if !line.trim().is_empty() {
                        output.push_str(&generator.indent());
                        output.push_str(line);
                        output.push_str("\n");
                    }
                }
                
                output.push_str(&generator.indent());
                // For grep commands, check if matches were found by looking at the filtered array
                // The grep command should have already set $CHILD_ERROR correctly
                output.push_str("$CHILD_ERROR == 0\n");
                
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}");
            } else {
                // For other simple commands, generate the command and check exit code
                output.push_str("do {\n");
                generator.indent_level += 1;
                // Temporarily save the current indent level and reset it for command generation
                let saved_indent_level = generator.indent_level;
                generator.indent_level = 0;
                let command = generator.generate_command(left);
                // Restore the indent level
                generator.indent_level = saved_indent_level;
                // The command generator already handles indentation, so we don't need to add extra indentation
                output.push_str(&command);
                output.push_str(&generator.indent());
                output.push_str("$CHILD_ERROR == 0\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}");
            }
        } else {
            // For non-literal command names, generate the command and check exit code
            output.push_str("do {\n");
            generator.indent_level += 1;
            // Temporarily save the current indent level and reset it for command generation
            let saved_indent_level = generator.indent_level;
            generator.indent_level = 0;
            let command = generator.generate_command(left);
            // Restore the indent level
            generator.indent_level = saved_indent_level;
            // The command generator already handles indentation, so we don't need to add extra indentation
            output.push_str(&command);
            output.push_str(&generator.indent());
            output.push_str("$CHILD_ERROR == 0\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}");
        }
    } else {
        // For other command types, generate the command and check exit code
        output.push_str("do {\n");
        generator.indent_level += 1;
        // Temporarily save the current indent level and reset it for command generation
        let saved_indent_level = generator.indent_level;
        generator.indent_level = 0;
        let command = generator.generate_command(left);
        // Restore the indent level
        generator.indent_level = saved_indent_level;
        // The command generator already handles indentation, so we don't need to add extra indentation
        output.push_str(&command);
        output.push_str(&generator.indent());
        output.push_str("$CHILD_ERROR == 0\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}");
    }
    
    output.push_str(") {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&generator.generate_command(right));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}

/// Generate logical OR operation (left || right)
pub fn generate_logical_or(generator: &mut Generator, left: &Command, right: &Command) -> String {
    let mut output = String::new();
    
    // Generate: left || right
    // OR operations should NEVER capture STDOUT - they're about conditional execution
    output.push_str(&generator.indent());
    
    // Check if left is a test expression
    if let Command::TestExpression(_) = left {
        // For test expressions, generate: if (!left) { right }
        output.push_str("if (!(");
        output.push_str(&generator.generate_command(left));
        output.push_str(")) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else if let Command::And(_and_left, _and_right) = left {
        // Special handling for AND operations in OR context
        // Use the logical AND generation function to handle the AND operation properly
        let and_result = generator.generate_command(left);
        output.push_str(&and_result);
        output.push_str(&generator.indent());
        output.push_str("if ($CHILD_ERROR != 0) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        return output;
    } else {
        // For commands that generate Perl code (like grep, ls), we need to handle them specially
        // to avoid embedding Perl code inside shell backticks
        if let Command::Simple(simple_cmd) = left {
            if let Word::Literal(name, _) = &simple_cmd.name {
                if name == "grep" {
                    // For grep commands in logical OR, generate the command and check exit code
                    output.push_str(&generator.generate_command(left));
                    output.push_str(&generator.indent());
                    output.push_str("if ($CHILD_ERROR != 0) {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(right));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    return output;
                } else if name == "ls" {
                    // For ls commands in logical OR, generate the command and check if files were found
                    output.push_str(&generator.generate_command(left));
                    output.push_str(&generator.indent());
                    output.push_str("if (!defined $ls_success || $ls_success == 0) {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&generator.generate_command(right));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    return output;
                }
            }
        } else if let Command::Redirect(redirect_cmd) = left {
            // Handle Redirect commands that might contain ls commands
            if let Command::Simple(simple_cmd) = &*redirect_cmd.command {
                if let Word::Literal(name, _) = &simple_cmd.name {
                    if name == "ls" {
                        // For ls commands in logical OR, generate the command and check if files were found
                        output.push_str(&generator.generate_command(left));
                        output.push_str(&generator.indent());
                        output.push_str("if (!defined $ls_success || $ls_success == 0) {\n");
                        generator.indent_level += 1;
                        output.push_str(&generator.indent());
                        output.push_str(&generator.generate_command(right));
                        generator.indent_level -= 1;
                        output.push_str(&generator.indent());
                        output.push_str("}\n");
                        return output;
                    }
                }
            }
        }
        
        // Execute left command and check exit code
        output.push_str(&generator.generate_command(left));
        
        // Execute right command if left command fails
        // For diff commands, check $diff_exit_code; for others, check $CHILD_ERROR
        let exit_code_var = if contains_diff_command(left) {
            "$diff_exit_code"
        } else {
            "$CHILD_ERROR"
        };
        
        output.push_str(&generator.indent());
        output.push_str(&format!("if ({} != 0) {{\n", exit_code_var));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }
    
    output
}

/// Check if a command is a diff command (for exit code handling)
fn contains_diff_command(cmd: &Command) -> bool {
    match cmd {
        Command::Simple(simple_cmd) => {
            if let Word::Literal(name, _) = &simple_cmd.name {
                name == "diff"
            } else {
                false
            }
        }
        Command::Redirect(redirect_cmd) => {
            contains_diff_command(&redirect_cmd.command)
        }
        _ => false,
    }
}
