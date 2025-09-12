use crate::ast::*;
use crate::generator::Generator;
use regex::Regex;

/// Generate Perl code for echo command
pub fn generate_echo_command(generator: &mut Generator, cmd: &SimpleCommand, _input_var: &str, output_var: &str) -> String {
    let mut output = String::new();
    
    if cmd.args.is_empty() {
        output.push_str(&format!("${} .= \"\\n\";\n", output_var));
    } else {
        // Check for -e flag
        let has_e_flag = cmd.args.iter().any(|arg| {
            if let Word::Literal(s, _) = arg {
                s == "-e"
            } else {
                false
            }
        });
        
        // Filter out the -e flag from arguments
        let filtered_args: Vec<&Word> = cmd.args.iter().filter(|&arg| {
            if let Word::Literal(s, _) = arg {
                s != "-e"
            } else {
                true
            }
        }).collect();
        
        // Convert arguments to Perl format
        let args: Vec<String> = filtered_args.iter()
            .map(|arg| {
                // For echo commands, handle special variables differently
                match arg {
                    Word::Variable(var, _, _) => {
                        match var.as_str() {
                            "#" => "scalar(@ARGV)".to_string(),
                            "@" => "@ARGV".to_string(),
                            _ => format!("${}", var)
                        }
                    }
                    Word::StringInterpolation(interp, _) => {
                        // Handle quoted variables like "$#" -> scalar(@ARGV)
                        if interp.parts.len() == 1 {
                            if let StringPart::Variable(var) = &interp.parts[0] {
                                match var.as_str() {
                                    "#" => "scalar(@ARGV)".to_string(),
                                    "@" => "@ARGV".to_string(),
                                    _ => format!("${}", var)
                                }
                            } else if let StringPart::ParameterExpansion(pe) = &interp.parts[0] {
                                // Handle parameter expansion like "${#arr[@]}" -> scalar(@arr)
                                generator.generate_parameter_expansion(&pe)
                            } else if let StringPart::Literal(literal) = &interp.parts[0] {
                                // Handle literal strings with -e flag
                                if has_e_flag {
                                    // If -e flag is present, interpret backslash escapes
                                    let mut interpreted = literal.clone();
                                    // Remove outer quotes if present
                                    if (interpreted.starts_with('"') && interpreted.ends_with('"')) ||
                                       (interpreted.starts_with('\'') && interpreted.ends_with('\'')) {
                                        interpreted = interpreted[1..interpreted.len()-1].to_string();
                                    }
                                    
                                    // Interpret backslash escapes
                                    interpreted = interpreted
                                        .replace("\\n", "\n")
                                        .replace("\\t", "\t")
                                        .replace("\\r", "\r")
                                        .replace("\\\\", "\\");
                                    
                                    // Return as a quoted string literal with proper escaping for Perl
                                    // Escape quotes, backslashes, newlines, and tabs
                                    format!("\"{}\"", interpreted.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r"))
                                } else {
                                    generator.perl_string_literal(arg)
                                }
                            } else {
                                generator.perl_string_literal(arg)
                            }
                        } else {
                            // For multi-part string interpolation with -e flag, handle each part
                            if has_e_flag {
                                // Process the string interpolation with -e flag interpretation
                                let mut result = String::new();
                                for part in &interp.parts {
                                    match part {
                                        crate::ast::StringPart::Literal(literal) => {
                                            // Interpret backslash escapes
                                            let mut interpreted = literal.clone();
                                            // Remove outer quotes if present
                                            if (interpreted.starts_with('"') && interpreted.ends_with('"')) ||
                                               (interpreted.starts_with('\'') && interpreted.ends_with('\'')) {
                                                interpreted = interpreted[1..interpreted.len()-1].to_string();
                                            }
                                            
                                            // Interpret backslash escapes
                                            interpreted = interpreted
                                                .replace("\\n", "\n")
                                                .replace("\\t", "\t")
                                                .replace("\\r", "\r")
                                                .replace("\\\\", "\\");
                                            
                                            result.push_str(&interpreted);
                                        },
                                        _ => {
                                            // For other parts, use default processing
                                            // This is a simplified approach - in reality, we'd need more complex handling
                                            result.push_str(&format!("{:?}", part));
                                        }
                                    }
                                }
                                // Return as a quoted string literal with proper escaping for Perl
                                // Escape quotes, backslashes, newlines, and tabs
                                format!("\"{}\"", result.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r"))
                            } else {
                                generator.perl_string_literal(arg)
                            }
                        }
                    }
                    Word::BraceExpansion(expansion, _) => {
                        // Handle brace expansion like {1..5} -> "1 2 3 4 5"
                        handle_brace_expansion_for_echo(generator, expansion)
                    }
                    Word::Literal(literal, _) => {
                        if has_e_flag {
                            // If -e flag is present, interpret backslash escapes
                            let mut interpreted = literal.clone();
                            // Remove outer quotes if present
                            if (interpreted.starts_with('"') && interpreted.ends_with('"')) ||
                               (interpreted.starts_with('\'') && interpreted.ends_with('\'')) {
                                interpreted = interpreted[1..interpreted.len()-1].to_string();
                            }
                            
                            // Interpret backslash escapes
                            interpreted = interpreted
                                .replace("\\n", "\n")
                                .replace("\\t", "\t")
                                .replace("\\r", "\r")
                                .replace("\\\\", "\\");
                            
                            // Return as a quoted string literal with proper escaping for Perl
                            // Escape quotes, backslashes, newlines, and tabs
                            format!("\"{}\"", interpreted.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r"))
                        } else {
                            generator.perl_string_literal(arg)
                        }
                    }
                    Word::CommandSubstitution(cmd, _) => {
                        // For command substitution in echo, preserve newlines instead of converting to spaces
                        handle_command_substitution_for_echo(generator, cmd)
                    }
                    _ => generator.perl_string_literal(arg)
                }
            })
            .collect();
        
        if args.is_empty() {
            output.push_str(&format!("${} .= \"\\n\";\n", output_var));
        } else if args.len() == 1 {
            // Check if the argument is a simple string literal that we can combine with newline
            if args[0].starts_with('"') && args[0].ends_with('"') && !args[0].contains("\\n") {
                // Extract the string content and add newline directly using double quotes for escape sequences
                let content = &args[0][1..args[0].len()-1]; // Remove quotes
                output.push_str(&format!("${} .= \"{}\\n\";\n", output_var, content));
            } else if args[0].contains("\\n") {
                output.push_str(&format!("${} .= {};\n", output_var, args[0]));
            } else {
                output.push_str(&format!("${} .= {} . \"\\n\";\n", output_var, args[0]));
            }
        } else {
            // For multiple arguments, join them with spaces
            let args_str = args.join(" . q{ } . ");
            output.push_str(&format!("${} .= {} . \"\\n\";\n", output_var, args_str));
        }
    }
    
    output
}

/// Handle brace expansion for echo commands
pub fn handle_brace_expansion_for_echo(_generator: &mut Generator, expansion: &BraceExpansion) -> String {
    let mut items = Vec::new();
    
    for item in &expansion.items {
        match item {
            BraceItem::Range(range) => {
                // Handle numeric ranges like {1..5} or {00..04..2}
                if let (Ok(start), Ok(end)) = (range.start.parse::<i32>(), range.end.parse::<i32>()) {
                    let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                    let mut current = start;
                    
                    // Check if we need to preserve leading zeros
                    let format_width = if range.start.starts_with('0') && range.start.len() > 1 {
                        Some(range.start.len())
                    } else {
                        None
                    };
                    
                    while if step > 0 { current <= end } else { current >= end } {
                        let formatted = if let Some(width) = format_width {
                            format!("{:0width$}", current, width = width)
                        } else {
                            current.to_string()
                        };
                        items.push(formatted);
                        current += step;
                    }
                } else {
                    // Handle character ranges like {a..c}
                    if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
                        let start_code = start_char as u32;
                        let end_code = end_char as u32;
                        let step = range.step.as_ref().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
                        
                        let mut current_code = start_code;
                        while if step > 0 { current_code <= end_code } else { current_code >= end_code } {
                            if let Some(c) = char::from_u32(current_code) {
                                items.push(c.to_string());
                            }
                            current_code = if step > 0 { 
                                current_code.saturating_add(step) 
                            } else { 
                                current_code.saturating_sub(step) 
                            };
                        }
                    }
                }
            }
            BraceItem::Literal(literal) => {
                items.push(literal.clone());
            }
            BraceItem::Sequence(sequence) => {
                for seq_item in sequence {
                    items.push(seq_item.clone());
                }
            }
        }
    }
    
    // Join all items with spaces and return as a quoted string
    let items_str = items.join(" ");
    format!("\"{}\"", items_str.replace("\"", "\\\""))
}

/// Handle command substitution specifically for echo commands, preserving newlines
fn handle_command_substitution_for_echo(generator: &mut Generator, cmd: &Command) -> String {
    match cmd {
        Command::Simple(simple_cmd) => {
            // Check if this is an ls command that we can convert properly
            if let Word::Literal(name, _) = &simple_cmd.name {
                if name == "ls" {
                    // Use the ls substitution function for proper conversion
                    return crate::generator::commands::ls::generate_ls_for_substitution(generator, simple_cmd);
                }
            }
            
            let cmd_name = generator.word_to_perl(&simple_cmd.name);
            let args: Vec<String> = simple_cmd.args.iter()
                .map(|arg| generator.word_to_perl(arg))
                .collect();
            
            // For simple commands, fall back to system command for now
            let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
            if args.is_empty() {
                format!(" my ({}); my {} = open3({}, {}, {}, '{}'); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {}", in_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
            } else {
                let formatted_args = args.iter().map(|arg| {
                    let word = Word::Literal(arg.clone(), Default::default());
                    generator.perl_string_literal(&word)
                }).collect::<Vec<_>>().join(", ");
                format!(" my ({}); my {} = open3({}, {}, {}, '{}', {}); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {}", in_var, pid_var, in_var, out_var, err_var, cmd_name, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
            }
        },
        Command::Pipeline(pipeline) => {
            // For command substitution pipelines in echo, preserve newlines instead of converting to spaces
            let pipeline_code = generator.generate_command(&Command::Pipeline(pipeline.clone()));
            
            // Find the actual output variable name that was generated
            let re = Regex::new(r"\$output_(\d+)").unwrap();
            let output_var = if let Some(cap) = re.captures(&pipeline_code) {
                format!("$output_{}", cap.get(1).unwrap().as_str())
            } else {
                // Generate a unique output variable if none found
                let unique_id = generator.get_unique_id();
                format!("$output_{}", unique_id)
            };
            
            // Find the pipeline success variable
            let success_var = if pipeline_code.contains("$pipeline_success_") {
                let re = Regex::new(r"\$pipeline_success_(\d+)").unwrap();
                if let Some(cap) = re.captures(&pipeline_code) {
                    format!("$pipeline_success_{}", cap.get(1).unwrap().as_str())
                } else {
                    "$pipeline_success_0".to_string()
                }
            } else {
                "$pipeline_success_0".to_string()
            };
            
            // Remove the print statements and exit code assignment using the actual variable names
            let mut captured_pipeline = pipeline_code
                .replace(&format!("print {};", output_var), "")
                .replace("print \"\\n\";", "")
                .replace(&format!("if (!({} =~ {})) {{ print \"\\n\"; }}", output_var, generator.newline_end_regex()), "")
                .replace(&format!("if (!{}) {{ $main_exit_code = 1; }}", success_var), "");
            
            // Remove conditional print blocks that are common in pipelines
            // Use a simpler approach with string replacement for the specific pattern
            let output_var_num = output_var.trim_start_matches("$output_");
            let print_block_to_remove = format!(
                "if ({} ne q{} && !defined($output_printed_{})) {{\n\n        print {};\n        if (!({} =~ {})) {{ print \"\\n\"; }}\n    }}", 
                output_var, "", output_var_num, output_var, output_var, generator.newline_end_regex()
            );
            captured_pipeline = captured_pipeline.replace(&print_block_to_remove, "");
            
            // Also try without the extra newlines in case formatting is different
            let print_block_compact = format!(
                "if ({} ne q{} && !defined($output_printed_{})) {{ print {}; if (!({} =~ {})) {{ print \"\\n\"; }} }}", 
                output_var, "", output_var_num, output_var, output_var, generator.newline_end_regex()
            );
            captured_pipeline = captured_pipeline.replace(&print_block_compact, "");
            
            // Remove the outer braces if they exist, as we'll wrap in our own do block
            captured_pipeline = captured_pipeline.trim().to_string();
            if captured_pipeline.starts_with('{') && captured_pipeline.ends_with('}') {
                captured_pipeline = captured_pipeline[1..captured_pipeline.len()-1].to_string();
            }
            
            // Return the code that executes the pipeline and captures output
            // For echo commands, preserve newlines instead of converting to spaces
            format!("do {{ {} {} }}", captured_pipeline.trim(), output_var)
        },
        _ => {
            // For other command types, use system command fallback
            let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
            format!(" my ({}); my {} = open3({}, {}, {}, 'bash', '-c', '{}'); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {}", in_var, pid_var, in_var, out_var, err_var, generator.generate_command_string_for_system(cmd), in_var, result_var, out_var, out_var, pid_var, result_var)
        }
    }
}
