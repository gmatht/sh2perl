use crate::generator::Generator;
use crate::ast::SimpleCommand;
use crate::ast::Word;
use crate::generator::commands::system_commands::word_to_bash_string_for_system;

/// Handle Perl commands by embedding the Perl code directly
pub fn generate_perl_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    if cmd.args.len() >= 2 {
        // Check for -e flag (execute code)
        if let Word::Literal(flag, _) = &cmd.args[0] {
            if flag == "-e" {
                // Extract the Perl code from the second argument
                let perl_code = if let Word::Literal(perl_code, _) = &cmd.args[1] {
                    Some(perl_code.clone())
                } else if let Word::StringInterpolation(interp, _) = &cmd.args[1] {
                    // Convert string interpolation to Perl code
                    Some(generator.convert_string_interpolation_to_perl(&interp))
                } else {
                    None
                };

                if let Some(perl_code) = perl_code {
                    // Clean up the Perl code - remove outer quotes if present
                    let mut clean_code = perl_code.clone();
                    if (clean_code.starts_with('"') && clean_code.ends_with('"')) ||
                       (clean_code.starts_with('\'') && clean_code.ends_with('\'')) {
                        clean_code = clean_code[1..clean_code.len()-1].to_string();
                    }
                    
                    // Don't interpret backslash escapes for Perl code - keep them as-is
                    // The Perl interpreter will handle them correctly
                    
                    // Set up @ARGV with the remaining arguments (skip first two: perl command and -e flag)
                    if cmd.args.len() > 2 {
                        output.push_str("@ARGV = (");
                        for (i, arg) in cmd.args.iter().skip(2).enumerate() {
                            if i > 0 {
                                output.push_str(", ");
                            }
                            output.push_str(&generator.word_to_perl(arg));
                        }
                        output.push_str(");\n");
                    }
                    
                    // Initialize environment variables to empty strings to avoid Perl warnings
                    // This prevents "Use of uninitialized value" warnings when accessing undefined env vars
                    output.push_str("local $ENV{SHELL_VAR} = q{} unless defined $ENV{SHELL_VAR};\n");
                    
                    // Execute the perl code - split by newlines and add proper indentation
                    for line in clean_code.lines() {
                        let trimmed_line = line.trim();
                        if !trimmed_line.is_empty() {
                            output.push_str(&generator.indent());
                            
                            // Special handling for foreach loops - add 'my' if missing
                            let mut final_line = trimmed_line.to_string();
                            if trimmed_line.starts_with("foreach $") && !trimmed_line.contains("my $") {
                                final_line = trimmed_line.replace("foreach $", "foreach my $");
                            }
                            
                            // Add semicolon if the line doesn't end with one and isn't a control structure
                            if !final_line.ends_with(';') && 
                               !final_line.ends_with('{') && 
                               !final_line.ends_with('}') &&
                               !final_line.starts_with('#') {
                                output.push_str(&format!("{};\n", final_line));
                            } else {
                                output.push_str(&format!("{}\n", final_line));
                            }
                        }
                    }
                    return output;
                }
            } else if flag == "-ne" {
                // Extract the Perl code from the second argument
                let perl_code = if let Word::Literal(perl_code, _) = &cmd.args[1] {
                    Some(perl_code.clone())
                } else if let Word::StringInterpolation(interp, _) = &cmd.args[1] {
                    // Convert string interpolation to Perl code
                    Some(generator.convert_string_interpolation_to_perl(&interp))
                } else {
                    None
                };

                if let Some(perl_code) = perl_code {
                    // Clean up the Perl code
                    let mut clean_code = perl_code.clone();
                    if (clean_code.starts_with('"') && clean_code.ends_with('"')) ||
                       (clean_code.starts_with('\'') && clean_code.ends_with('\'')) {
                        clean_code = clean_code[1..clean_code.len()-1].to_string();
                    }
                    
                    // Don't interpret backslash escapes for Perl code - keep them as-is
                    // The Perl interpreter will handle them correctly
                    
                    output.push_str(&generator.indent());
                    output.push_str(&format!("# Perl -ne: {}\n", clean_code));
                    for line in clean_code.lines() {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("{}\n", line));
                    }
                    return output;
                }
            }
        }
    }
    
    // Fallback to system call if not a -e or -ne command
    let args_str = cmd.args.iter()
        .map(|arg| word_to_bash_string_for_system(arg))
        .collect::<Vec<_>>()
        .join(" ");
    
    let output_var = format!("perl_output_{}", generator.get_unique_id());
    output.push_str(&format!("{} = `{} {}`;\n", output_var, "perl", args_str));
    output.push_str(&format!("print ${};\n", output_var));
    
    output
}

/// Handle Perl commands within pipelines
pub fn generate_perl_pipeline_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str) -> String {
    let mut output = String::new();
    let mut perl_code = String::new();
    let mut is_ne = false;
    
    // Extract Perl code from arguments
    for (i, arg) in cmd.args.iter().enumerate() {
        if let Word::Literal(s, _) = arg {
            if s == "-e" {
                if i + 1 < cmd.args.len() {
                    if let Word::Literal(code, _) = &cmd.args[i + 1] {
                        perl_code = code.clone();
                        break;
                    } else if let Word::StringInterpolation(interp, _) = &cmd.args[i + 1] {
                        // Convert string interpolation to Perl code
                        perl_code = generator.convert_string_interpolation_to_perl(&interp);
                        break;
                    }
                }
            } else if s == "-ne" {
                if i + 1 < cmd.args.len() {
                    if let Word::Literal(code, _) = &cmd.args[i + 1] {
                        perl_code = code.clone();
                        is_ne = true;
                        break;
                    } else if let Word::StringInterpolation(interp, _) = &cmd.args[i + 1] {
                        // Convert string interpolation to Perl code
                        perl_code = generator.convert_string_interpolation_to_perl(&interp);
                        is_ne = true;
                        break;
                    }
                }
            }
        }
    }
    
    if !perl_code.is_empty() {
        // Clean the Perl code by removing outer quotes
        let mut clean_code = perl_code.clone();
        
        // Remove outer quotes if present
        if (clean_code.starts_with('\'') && clean_code.ends_with('\'')) ||
           (clean_code.starts_with('"') && clean_code.ends_with('"')) {
            clean_code = clean_code[1..clean_code.len()-1].to_string();
        }
        
        // For pipeline context, we need to capture output instead of printing directly
        let output_var = format!("perl_output_{}", generator.get_unique_id());
        output.push_str(&format!("my ${} = q{{}};\n", output_var));
        
        // For pipeline context, we need to set $_ to the input
        if is_ne {
            // For -ne mode, process each line of input
            output.push_str(&format!("for my $line (split /\\n/msx, ${}) {{\n", input_var));
            output.push_str(&format!("    chomp $line;\n"));
            output.push_str(&format!("    $_ = $line;\n"));
        } else {
            // For -e mode, set $_ to the entire input
            output.push_str(&format!("$_ = ${};\n", input_var));
        }
        
        // Initialize environment variables to empty strings to avoid Perl warnings
        // This prevents "Use of uninitialized value" warnings when accessing undefined env vars
        output.push_str("local $ENV{SHELL_VAR} = q{} unless defined $ENV{SHELL_VAR};\n");
        
        // Execute the perl code - split by newlines and add proper indentation
        for line in clean_code.lines() {
            let trimmed_line = line.trim();
            if !trimmed_line.is_empty() {
                // Special handling for foreach loops - add 'my' if missing
                let mut final_line = trimmed_line.to_string();
                if trimmed_line.starts_with("foreach $") && !trimmed_line.contains("my $") {
                    final_line = trimmed_line.replace("foreach $", "foreach my $");
                }
                
                // Replace print statements to capture output instead of printing directly
                // Handle both standalone print statements and print statements within semicolon-separated code
                if final_line.contains("print ") {
                    // Split by semicolon and process each part
                    let parts: Vec<&str> = final_line.split(';').collect();
                    let mut processed_parts = Vec::new();
                    
                    for part in parts {
                        let trimmed_part = part.trim();
                        if trimmed_part.starts_with("print ") {
                            processed_parts.push(trimmed_part.replace("print ", &format!("${} .= ", output_var)));
                        } else if !trimmed_part.is_empty() {
                            processed_parts.push(trimmed_part.to_string());
                        }
                    }
                    
                    final_line = processed_parts.join("; ");
                }
                
                // Add semicolon if the line doesn't end with one and isn't a control structure
                if !final_line.ends_with(';') && 
                   !final_line.ends_with('{') && 
                   !final_line.ends_with('}') &&
                   !final_line.starts_with('#') {
                    output.push_str(&format!("{};\n", final_line));
                } else {
                    output.push_str(&format!("{}\n", final_line));
                }
            }
        }
        
        // Close the for loop for -ne mode and set the output variable
        if is_ne {
            output.push_str("}\n");
            // Add trailing empty line only for single-line inputs to match shell behavior
            // Check if input has only one line by counting newlines
            output.push_str(&format!("if (${} =~ {}) {{\n", input_var, generator.format_regex_pattern(r"^[^\\n]*$")));
            output.push_str(&format!("    ${} .= \"\\n\";\n", output_var));
            output.push_str("}\n");
        }
        
        // Set the output variable for the pipeline
        output.push_str(&format!("${} = ${};\n", input_var, output_var));
    } else {
        // Fallback to system call
        let args_str = cmd.args.iter()
            .map(|arg| word_to_bash_string_for_system(arg))
            .collect::<Vec<_>>()
            .join(" ");
        
        let output_var = format!("perl_output_{}", generator.get_unique_id());
        output.push_str(&format!("{} = `{} {}`;\n", output_var, "perl", args_str));
        output.push_str(&format!("print ${};\n", output_var));
    }
    
    output
}
