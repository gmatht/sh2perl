use crate::ast::*;
use crate::generator::Generator;

/// Generate logical AND operation (left && right)
pub fn generate_logical_and(generator: &mut Generator, left: &Command, right: &Command) -> String {
    let mut output = String::new();
    
    // Generate: left && right
    output.push_str(&generator.indent());
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
        if let Word::Literal(name) = &simple_cmd.name {
            if name == "grep" {
                // For grep commands in logical AND, generate the command in a block
                // and check if it found any matches
                output.push_str("do {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                let grep_result = generator.generate_command(left);
                
                // Extract the grep_result variable name from the generated code
                let mut grep_result_var = String::new();
                for line in grep_result.lines() {
                    if line.trim_start().starts_with("my $grep_result_") {
                        if let Some(end) = line.find(';') {
                            let var_decl = &line[3..end]; // Remove "my " prefix
                            grep_result_var = var_decl.to_string();
                        }
                    }
                    if !line.trim().is_empty() {
                        output.push_str(&generator.indent());
                        output.push_str(line);
                        output.push_str("\n");
                    }
                }
                
                output.push_str(&generator.indent());
                // Check if grep found matches by checking if the result is non-empty
                if !grep_result_var.is_empty() {
                    output.push_str(&format!("$? = {} ne '' ? 0 : 1;\n", grep_result_var));
                    output.push_str(&format!("{} ne ''\n", grep_result_var));
                } else {
                    output.push_str("$? = 1;\n");
                    output.push_str("1\n"); // Default to true if we can't find the variable
                }
                
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}");
            } else {
                // For other simple commands, use the default approach
                output.push_str(&generator.generate_command(left));
            }
        } else {
            // For non-literal command names, use the default approach
            output.push_str(&generator.generate_command(left));
        }
    } else {
        output.push_str(&generator.generate_command(left));
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
    } else {
        // For commands that generate Perl code (like grep), we need to handle them specially
        // to avoid embedding Perl code inside shell backticks
        if let Command::Simple(simple_cmd) = left {
            if let Word::Literal(name) = &simple_cmd.name {
                if name == "grep" {
                    // For grep commands in logical OR, we need to generate the complete conditional structure
                    // including proper variable declarations and exit code checking
                    let unique_id = generator.get_unique_id();
                    output.push_str(&format!("my $grep_exit_code_{};\n", unique_id));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("{{\n"));
                    generator.indent_level += 1;
                    
                    // Pre-declare all the variables that the grep command might use
                    // The grep command will use variables like $grep_result_X, where X is some identifier
                    // We need to analyze the grep output to see what variables it's trying to use
                    let grep_result = generator.generate_command(left);
                    
                    // Extract variable names from the grep result and declare them
                    let mut declared_vars = std::collections::HashSet::new();
                    for line in grep_result.lines() {
                        // Look for patterns like $grep_result_... that are not declared
                        if line.contains("$grep_result_") && !line.trim_start().starts_with("my ") {
                            // Extract the variable name
                            if let Some(start) = line.find("$grep_result_") {
                                let var_part = &line[start..];
                                if let Some(end) = var_part.find([' ', ';', '=', ')', ',', '\n']) {
                                    let var_name = &var_part[1..end]; // Remove the $
                                    if !declared_vars.contains(var_name) {
                                        output.push_str(&generator.indent());
                                        output.push_str(&format!("my ${};\n", var_name));
                                        declared_vars.insert(var_name.to_string());
                                    }
                                }
                            }
                        }
                    }
                    
                    // Now output the grep command
                    for line in grep_result.lines() {
                        if !line.trim().is_empty() {
                            output.push_str(&generator.indent());
                            output.push_str(line);
                            output.push_str("\n");
                        }
                    }
                    
                    // Set the exit code based on grep result
                    // Extract the grep_filtered variable name from the generated grep code
                    let mut grep_filtered_var = format!("@grep_filtered_{}", unique_id);
                    for line in grep_result.lines() {
                        if line.contains("@grep_filtered_") {
                            if let Some(start) = line.find("@grep_filtered_") {
                                let var_part = &line[start..];
                                if let Some(end) = var_part.find([' ', ';', '=', ')', ',', '\n']) {
                                    grep_filtered_var = var_part[..end].to_string();
                                    break;
                                }
                            }
                        }
                    }
                    output.push_str(&generator.indent());
                    output.push_str(&format!("$grep_exit_code_{} = scalar({}) > 0 ? 0 : 1;\n", unique_id, grep_filtered_var));
                    
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str(&format!("}}\n"));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if ($grep_exit_code_{} != 0) {{\n", unique_id));
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
        
        // Execute left command and check exit code
        output.push_str(&generator.generate_command(left));
        
        // Execute right command if left command fails
        // For diff commands, check $diff_exit_code; for others, check $?
        let exit_code_var = if contains_diff_command(left) {
            "$diff_exit_code"
        } else {
            "$?"
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
            if let Word::Literal(name) = &simple_cmd.name {
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
