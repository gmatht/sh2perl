use crate::ast::*;
use crate::generator::Generator;
use std::sync::atomic::{AtomicUsize, Ordering};

// Static counter for generating unique temp file names
static TEMP_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn generate_simple_command_impl(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // Handle array assignments first (these need to be in the main scope)
    for (var, value) in &cmd.env_vars {
        if let Word::Array(_, elements) = value {
            // Handle array assignment like arr=(one two three)
            let elements_perl: Vec<String> = elements.iter()
                .map(|s| format!("\"{}\"", generator.escape_perl_string(s)))
                .collect();
            output.push_str(&generator.indent());
            output.push_str(&format!("my @{} = ({});\n", var, elements_perl.join(", ")));
            // Mark array as declared
            if !generator.declared_locals.contains(var) {
                generator.declared_locals.insert(var.clone());
            }
        } else if let Word::Literal(s) = value {
            if let Some(elements) = generator.extract_array_elements(s) {
                // Check if this is an indexed array assignment like arr=(one two three)
                let elements_perl: Vec<String> = elements.iter()
                    .map(|s| format!("\"{}\"", generator.escape_perl_string(s)))
                    .collect();
                output.push_str(&generator.indent());
                output.push_str(&format!("my @{} = ({});\n", var, elements_perl.join(", ")));
            }
        }
    }
    
    // Check if there are any non-array environment variables to process
    // But exclude standalone assignments (cmd.name == "true")
    let is_standalone_assignment = if let Word::Literal(ref name) = cmd.name {
        name == "true" && !cmd.env_vars.is_empty() && cmd.args.is_empty()
    } else {
        false
    };
    
    let has_non_array_env = !is_standalone_assignment && cmd.env_vars.iter().any(|(var, value)| {
        !matches!(value, Word::Array(_, _)) && 
        !matches!(value, Word::Literal(s) if generator.extract_array_elements(s).is_some())
    });
    
    if has_non_array_env {
        for (var, value) in &cmd.env_vars {
            // Check if this is an associative array assignment like map[foo]=bar
            if let Some((array_name, key)) = generator.extract_array_key(var) {
                let val = generator.perl_string_literal(value);
                // For associative array assignments, generate $array{key} = value instead of $ENV{var}
                // Quote the key to avoid bareword errors in strict mode
                let quoted_key = format!("\"{}\"", generator.escape_perl_string(&key));
                output.push_str(&generator.indent());
                output.push_str(&format!("${}{{{}}} = {};\n", array_name, quoted_key, val));
            } else if let Word::Array(_, _) = value {
                // Skip array assignments here - they're handled above
                continue;
            } else if let Word::Literal(s) = value {
                if let Some(_) = generator.extract_array_elements(s) {
                    // Skip array assignments here - they're handled above
                    continue;
                } else {
                    // Regular string assignment
                    let val = generator.perl_string_literal(value);
                    // Always assign the value, but only declare if not already declared
                    if !generator.declared_locals.contains(var) {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${} = {};\n", var, val));
                        generator.declared_locals.insert(var.clone());
                    } else {
                        // Variable already declared, just assign the value
                        output.push_str(&generator.indent());
                        output.push_str(&format!("${} = {};\n", var, val));
                    }
                    // Only set environment variable if this is not a standalone variable assignment
                    if let Word::Literal(ref name) = cmd.name {
                        if name != "true" {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("local $ENV{{{}}} = {};;\n", var, val));
                        }
                    }
                }
            } else {
                // Handle other Word types
                let val = generator.perl_string_literal(value);
                // Always assign the value, but only declare if not already declared
                if !generator.declared_locals.contains(var) {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("my ${} = {};\n", var, val));
                    generator.declared_locals.insert(var.clone());
                } else {
                    // Variable already declared, just assign the value
                    output.push_str(&generator.indent());
                    output.push_str(&format!("${} = {};\n", var, val));
                }
                // Only set environment variable if this is not a standalone variable assignment
                if let Word::Literal(ref name) = cmd.name {
                    if name != "true" {
                        output.push_str(&generator.indent());
                        output.push_str(&format!("local $ENV{{{}}} = {};;\n", var, val));
                    }
                }
            }
        }
    }

    // Pre-process process substitution and here-string redirects to create temporary files
    let mut process_sub_files = Vec::new();
    let mut temp_file_counter = 0;
    for redir in &cmd.redirects {
        match &redir.operator {
            RedirectOperator::ProcessSubstitutionInput(cmd) => {
                // Process substitution input: <(command)
                temp_file_counter += 1;
                let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_file = format!("/tmp/process_sub_{}_{}.tmp", global_counter, temp_file_counter);
                let temp_var = format!("temp_file_ps_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = '{}';\n", temp_var, temp_file));
                
                // Execute the command and capture its output
                let fh_var = format!("fh_ps_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${};\n", fh_var));
                output.push_str(&generator.indent());
                output.push_str(&format!("{{\n"));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("local $/;  # Read entire input at once\n"));
                
                // Store the command string in a local variable to avoid borrowing issues
                let cmd_str = generator.generate_command_string_for_system(&**cmd);
                output.push_str(&generator.indent());
                output.push_str(&format!("open(my $pipe, '-|', 'bash', '-c', {});\n", 
                    generator.perl_string_literal(&Word::Literal(cmd_str))));
                output.push_str(&generator.indent());
                output.push_str(&format!("my $output_ps_{} = <$pipe>;\n", global_counter));
                output.push_str(&generator.indent());
                output.push_str(&format!("close($pipe);\n"));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("}}\n"));
                
                // Write the output to the temporary file
                output.push_str(&generator.indent());
                output.push_str(&format!("open(my ${}, '>', ${}) or die \"Cannot create temp file: $!\\n\";\n", fh_var, temp_var));
                output.push_str(&generator.indent());
                output.push_str(&format!("print ${} $output_ps_{};\n", fh_var, global_counter));
                output.push_str(&generator.indent());
                output.push_str(&format!("close(${});\n", fh_var));
                
                process_sub_files.push((temp_var, temp_file));
            }
            RedirectOperator::ProcessSubstitutionOutput(_cmd) => {
                // Process substitution output: >(command)
                temp_file_counter += 1;
                let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_file = format!("/tmp/process_sub_out_{}_{}.tmp", global_counter, temp_file_counter);
                let temp_var = format!("temp_file_out_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = '{}';\n", temp_var, temp_file));
                process_sub_files.push((temp_var, temp_file));
            }
            RedirectOperator::HereString => {
                // Here-string: <<< content
                temp_file_counter += 1;
                let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_file = format!("/tmp/here_string_{}_{}.tmp", global_counter, temp_file_counter);
                let temp_var = format!("temp_file_hs_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = '{}';\n", temp_var, temp_file));
                
                // Create the temporary file with the here-string content
                if let Some(content) = &redir.heredoc_body {
                    let fh_var = format!("fh_hs_{}_{}", global_counter, temp_file_counter);
                    output.push_str(&generator.indent());
                    output.push_str(&format!("open(my ${}, '>', ${}) or die \"Cannot create temp file: $!\\n\";\n", fh_var, temp_var));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("print ${} {};\n", fh_var, generator.perl_string_literal(&Word::Literal(content.clone()))));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("close(${});\n", fh_var));
                }
                
                process_sub_files.push((temp_var, temp_file));
            }
            _ => {}
        }
    }

    // Generate the actual command
    if let Word::Literal(ref name) = cmd.name {
        if name == "echo" {
            // Simplified echo command handling
            if cmd.args.is_empty() {
                output.push_str(&generator.indent());
                output.push_str("print \"\\n\";\n");
            } else {
                // Convert arguments to Perl format
                let args: Vec<String> = cmd.args.iter()
                    .map(|arg| generator.word_to_perl(arg))
                    .collect();
                
                if args.len() == 1 {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("print {}. \"\\n\";\n", args[0]));
                } else {
                    // For multiple arguments, join them with spaces
                    let args_str = args.join(", ");
                    output.push_str(&generator.indent());
                    output.push_str(&format!("print {}. \"\\n\";\n", args_str));
                }
            }
        } else if name == "true" && !cmd.env_vars.is_empty() && cmd.args.is_empty() {
            // This is a standalone assignment (e.g., i=$((i + 1)))
            for (var, value) in &cmd.env_vars {
                match value {
                    Word::Arithmetic(expr) => {
                        // Convert arithmetic expression to Perl
                        let perl_expr = generator.convert_arithmetic_to_perl(&expr.expression);
                        if !generator.declared_locals.contains(var) {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("my ${} = {};\n", var, perl_expr));
                            generator.declared_locals.insert(var.clone());
                        } else {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("${} = {};\n", var, perl_expr));
                        }
                    },
                    _ => {
                        // Handle other value types
                        let val = generator.perl_string_literal(value);
                        if !generator.declared_locals.contains(var) {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("my ${} = {};\n", var, val));
                            generator.declared_locals.insert(var.clone());
                        } else {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("${} = {};\n", var, val));
                        }
                    }
                }
            }
        } else {
            // Handle other commands - delegate to builtins system or use fallback
            if generator.declared_functions.contains(name) || *name == "greet" {
                // Function call
                if cmd.args.is_empty() {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("{}();\n", name));
                } else {
                    let args: Vec<String> = cmd.args.iter()
                        .map(|arg| generator.word_to_perl(arg))
                        .collect();
                    output.push_str(&generator.indent());
                    output.push_str(&format!("{}({});\n", name, args.join(", ")));
                }
            } else {
                // System call fallback
                if cmd.args.is_empty() {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("system('{}');\n", name));
                } else {
                    let args: Vec<String> = cmd.args.iter()
                        .map(|arg| generator.word_to_perl(arg))
                        .collect();
                    output.push_str(&generator.indent());
                    output.push_str(&format!("system('{}', {});\n", name, args.join(", ")));
                }
            }
        }
    } else {
        // Handle non-literal command names
        let cmd_name = "unknown_command";
        
        // Fallback to system call
        if cmd.args.is_empty() {
            output.push_str(&generator.indent());
            output.push_str(&format!("system('{}');\n", cmd_name));
        } else {
            let args: Vec<String> = cmd.args.iter()
                .map(|arg| generator.word_to_perl(arg))
                .collect();
            output.push_str(&generator.indent());
            output.push_str(&format!("system('{}', {});\n", cmd_name, args.join(", ")));
        }
    }

    output
}

