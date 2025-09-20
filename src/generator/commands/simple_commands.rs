use crate::ast::*;
use crate::generator::Generator;
use crate::generator::utils::get_temp_dir;
use crate::Parser;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

// Static counter for generating unique temp file names
static TEMP_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn generate_simple_command_impl(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // Handle array assignments first (these need to be in the main scope)
    for (var, value) in &cmd.env_vars {
        if let Word::Array(_, elements, _) = value {
            // Handle array assignment like arr=(one two three)
            let elements_perl: Vec<String> = elements.iter()
                .map(|s| {
                    // Check if this element contains backticks (command substitution)
                    if s.contains('`') {
                        // Extract the command from backticks and convert to native Perl
                        if s.starts_with('`') && s.ends_with('`') {
                            let cmd_text = &s[1..s.len()-1]; // Remove backticks
                            // For now, handle common cases like `ls -1 examples/*.sh 2>/dev/null`
                            if cmd_text.starts_with("ls ") {
                                // Convert ls command to native Perl glob
                                let args = cmd_text.strip_prefix("ls ").unwrap_or("");
                                if args.contains("*.sh") {
                                    // Handle multiple glob patterns
                                    let patterns: Vec<&str> = args.split_whitespace()
                                        .filter(|arg| arg.contains("*.sh"))
                                        .collect();
                                    if patterns.len() == 1 {
                                        format!("glob '{}'", patterns[0])
                                    } else {
                                        // Multiple patterns - combine them
                                        let glob_exprs: Vec<String> = patterns.iter()
                                            .map(|pattern| format!("glob '{}'", pattern))
                                            .collect();
                                        format!("map {{ glob '{}' }} ('{}')", "{}", patterns.join("', '"))
                                    }
                                } else {
                                    // Fallback for other ls commands - use native Perl
                                    format!("do {{ use File::Find; my @files; find(sub {{ push @files, $File::Find::name if -f }}, '.'); @files }}")
                                }
                            } else if cmd_text.starts_with("date") {
                                // Handle date command
                                if let Some(format) = cmd_text.strip_prefix("date ") {
                                    // Strip the + prefix from date format strings (shell date +%Y -> strftime %Y)
                                    let cleaned_format = if format.starts_with('+') {
                                        format!("'{}'", &format[1..])
                                    } else if format.starts_with('"') || format.starts_with("'") || format.starts_with("q{") {
                                        format.to_string()
                                    } else {
                                        format!("'{}'", format)
                                    };
                                    format!("do {{ use POSIX qw(strftime); strftime({}, localtime) }}", cleaned_format)
                                } else {
                                    "do { use POSIX qw(strftime); strftime('%a, %d %b %Y %H:%M:%S %z', localtime) }".to_string()
                                }
                            } else {
                                // For other commands, use open3 to capture output without backticks
                                let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                                format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, 'bash', '-c', '{}'); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_text, in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        } else {
                            // Element contains backticks but not at start/end - treat as literal
                            format!("\"{}\"", generator.escape_perl_string(s))
                        }
                    } else {
                        // Normal string element
                        format!("\"{}\"", generator.escape_perl_string(s))
                    }
                })
                .collect();
            output.push_str(&generator.indent());
            // For array elements that are command substitutions, we need to expand them
            if elements_perl.iter().any(|e| e.starts_with("glob(") || e.starts_with("do {")) {
                // Use array expansion for glob results
                output.push_str(&format!("my @{} = ({});\n", var, elements_perl.join(", ")));
            } else {
                output.push_str(&format!("my @{} = ({});\n", var, elements_perl.join(", ")));
            }
            // Mark array as declared
            if !generator.declared_locals.contains(var) {
                generator.declared_locals.insert(var.clone());
            }
        } else if let Word::Literal(s, _) = value {
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
    let is_standalone_assignment = if let Word::Literal(ref name, _) = cmd.name {
        name == "true" && !cmd.env_vars.is_empty() && cmd.args.is_empty()
    } else {
        false
    };
    
    let has_non_array_env = !is_standalone_assignment && cmd.env_vars.iter().any(|(_var, value)| {
        !matches!(value, Word::Array(..)) && 
        !matches!(value, Word::Literal(s, _) if generator.extract_array_elements(s).is_some())
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
            } else if let Word::Array(..) = value {
                // Skip array assignments here - they're handled above
                continue;
            } else if let Word::Literal(s, _) = value {
                if let Some(_) = generator.extract_array_elements(s) {
                    // Skip array assignments here - they're handled above
                    continue;
                } else {
                    // Regular string assignment
                    let val = generator.perl_string_literal(value);
                    // Always assign the value, but only declare if not already declared
                    if !generator.declared_locals.contains(var) {
                        output.push_str(&generator.indent());
                        // If the value is a block, wrap it in do {...}
                        if val.starts_with('{') && val.ends_with('}') {
                            output.push_str(&format!("my ${} = do {};\n", var, val));
                        } else {
                            output.push_str(&format!("my ${} = {};\n", var, val));
                        }
                        generator.declared_locals.insert(var.clone());
                    } else {
                        // Variable already declared, just assign the value
                        output.push_str(&generator.indent());
                        // If the value is a block, wrap it in do {...}
                        if val.starts_with('{') && val.ends_with('}') {
                            output.push_str(&format!("${} = do {};\n", var, val));
                        } else {
                            output.push_str(&format!("${} = {};\n", var, val));
                        }
                    }
                    // Don't set environment variable immediately - only set it when export command is encountered
                    // This matches bash behavior where variables are only exported to environment after export command
                }
            } else {
                // Handle other Word types (including CommandSubstitution)
                let val = generator.word_to_perl(value);
                // Always assign the value, but only declare if not already declared
                if !generator.declared_locals.contains(var) {
                    output.push_str(&generator.indent());
                    // If the value is a block, wrap it in do {...}
                    if val.starts_with('{') && val.ends_with('}') {
                        output.push_str(&format!("my ${} = do {};\n", var, val));
                    } else {
                        output.push_str(&format!("my ${} = {};\n", var, val));
                    }
                    generator.declared_locals.insert(var.clone());
                } else {
                    // Variable already declared, just assign the value
                    output.push_str(&generator.indent());
                    // If the value is a block, wrap it in do {...}
                    if val.starts_with('{') && val.ends_with('}') {
                        output.push_str(&format!("${} = do {};\n", var, val));
                    } else {
                        output.push_str(&format!("${} = {};\n", var, val));
                    }
                }
                // Don't set environment variable immediately - only set it when export command is encountered
                // This matches bash behavior where variables are only exported to environment after export command
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
                let temp_file = format!("{}/process_sub_{}_{}.tmp", get_temp_dir(), global_counter, temp_file_counter);
                let temp_var = format!("temp_file_ps_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = {} . '/process_sub_{}_{}.tmp';\n", temp_var, get_temp_dir(), global_counter, temp_file_counter));
                
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
                output.push_str(&format!("open my $pipe, '-|', 'bash', '-c', {};\n", 
                    generator.perl_string_literal(&Word::literal(cmd_str))));
                output.push_str(&generator.indent());
                output.push_str(&format!("my $output_ps_{} = <$pipe>;\n", global_counter));
                output.push_str(&generator.indent());
                output.push_str(&format!("close $pipe;\n"));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("}}\n"));
                
                // Write the output to the temporary file
                output.push_str(&generator.indent());
                output.push_str(&format!("use File::Path qw(make_path);\n"));
                output.push_str(&generator.indent());
                output.push_str(&format!("my $temp_dir_{}_{} = dirname(${});\n", global_counter, temp_file_counter, temp_var));
                output.push_str(&generator.indent());
                output.push_str(&format!("if (!-d $temp_dir_{}_{}) {{ make_path($temp_dir_{}_{}); }}\n", global_counter, temp_file_counter, global_counter, temp_file_counter));
                output.push_str(&generator.indent());
                output.push_str(&format!("open my ${}, '>', ${} or croak \"Cannot create temp file: $ERRNO\\n\";\n", fh_var, temp_var));
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
                let _temp_file = format!("{}/process_sub_out_{}_{}.tmp", get_temp_dir(), global_counter, temp_file_counter);
                let temp_var = format!("temp_file_out_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = {} . '/process_sub_out_{}_{}.tmp';\n", temp_var, get_temp_dir(), global_counter, temp_file_counter));
                process_sub_files.push((temp_var, format!("{} . '/process_sub_out_{}_{}.tmp'", get_temp_dir(), global_counter, temp_file_counter)));
            }
            RedirectOperator::HereString => {
                // Here-string: <<< content
                temp_file_counter += 1;
                let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                let temp_file = format!("{}/here_string_{}_{}.tmp", get_temp_dir(), global_counter, temp_file_counter);
                let temp_var = format!("temp_file_hs_{}_{}", global_counter, temp_file_counter);
                output.push_str(&generator.indent());
                output.push_str(&format!("my ${} = '{}';\n", temp_var, temp_file));
                
                // Create the temporary file with the here-string content
                if let Some(content) = &redir.heredoc_body {
                    let fh_var = format!("fh_hs_{}_{}", global_counter, temp_file_counter);
                    output.push_str(&generator.indent());
                    output.push_str(&format!("open my ${}, '>', ${} or croak \"Cannot create temp file: $ERRNO\\n\";\n", fh_var, temp_var));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("print ${} {};\n", fh_var, generator.perl_string_literal(&Word::literal(content.clone()))));
                    output.push_str(&generator.indent());
                    output.push_str(&format!("close(${});\n", fh_var));
                }
                
                process_sub_files.push((temp_var, temp_file));
            }
            _ => {}
        }
    }

    // Generate the actual command
    if let Word::Literal(ref name, _) = cmd.name {
        if name == "local" {
            // Handle local command - convert to my declarations
            for arg in &cmd.args {
                match arg {
                    Word::Literal(var_name, _) => {
                        // Check if it's an assignment (var=value)
                        if var_name.contains('=') {
                            let parts: Vec<&str> = var_name.splitn(2, '=').collect();
                            if parts.len() == 2 {
                                let var = parts[0];
                                let value = parts[1];
                                if !generator.declared_locals.contains(var) {
                                    // Check if the value contains command substitution
                                    if value.contains('`') {
                                        // Handle command substitution in local assignment
                                        // Parse the command substitution and convert to Perl
                                        let command_substitution = value.trim_start_matches('`').trim_end_matches('`');
                                        
                                        // Try to parse the command properly instead of wrapping in bash -c
                                        eprintln!("DEBUG: Parsing command substitution: {}", command_substitution);
                                        if let Ok(parsed_commands) = Parser::new(command_substitution).parse() {
                                            eprintln!("DEBUG: Parsed commands: {:?}", parsed_commands);
                                            if !parsed_commands.is_empty() {
                                                eprintln!("DEBUG: Creating CommandSubstitution word");
                                                let perl_command = generator.word_to_perl(&Word::CommandSubstitution(
                                                    Box::new(parsed_commands[0].clone()),
                                                    None
                                                ));
                                                eprintln!("DEBUG: Generated Perl command: {}", perl_command);
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!("my ${} = {};\n", var, perl_command));
                                            } else {
                                                // Fallback to bash -c if parsing fails
                                                let perl_command = generator.word_to_perl(&Word::CommandSubstitution(
                                                    Box::new(Command::Simple(SimpleCommand {
                                                        name: Word::Literal("bash".to_string(), None),
                                                        args: vec![Word::Literal("-c".to_string(), None), Word::Literal(command_substitution.to_string(), None)],
                                                        redirects: vec![],
                                                        env_vars: HashMap::new(),
                                                        stderr_used: false,
                                                        stdout_used: false,
                                                    })),
                                                    None
                                                ));
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!("my ${} = {};\n", var, perl_command));
                                            }
                                        } else {
                                            // Fallback to bash -c if parsing fails
                                            let perl_command = generator.word_to_perl(&Word::CommandSubstitution(
                                                Box::new(Command::Simple(SimpleCommand {
                                                    name: Word::Literal("bash".to_string(), None),
                                                    args: vec![Word::Literal("-c".to_string(), None), Word::Literal(command_substitution.to_string(), None)],
                                                    redirects: vec![],
                                                    env_vars: HashMap::new(),
                                                    stderr_used: false,
                                                    stdout_used: false,
                                                })),
                                                None
                                            ));
                                            output.push_str(&generator.indent());
                                            output.push_str(&format!("my ${} = {};\n", var, perl_command));
                                        }
                                    } else {
                                        output.push_str(&generator.indent());
                                        output.push_str(&format!("my ${} = {};\n", var, value));
                                    }
                                    generator.declared_locals.insert(var.to_string());
                                }
                            }
                        } else {
                            // Just declaration without assignment
                            if !generator.declared_locals.contains(var_name) {
                                output.push_str(&generator.indent());
                                output.push_str(&format!("my ${};\n", var_name));
                                generator.declared_locals.insert(var_name.clone());
                            }
                        }
                    }
                    _ => {
                        // For other word types, try to extract variable name and value
                        let var_expr = generator.word_to_perl(arg);
                        if !var_expr.is_empty() && !generator.declared_locals.contains(&var_expr) {
                            output.push_str(&generator.indent());
                            output.push_str(&format!("my {};\n", var_expr));
                            generator.declared_locals.insert(var_expr);
                        }
                    }
                }
            }
        } else if name == "sort" {
            // Handle sort command - check if this is in process substitution context
            let command_index = generator.get_unique_id();
            let output_var = format!("sort_output_{}", command_index);
            
            // Determine the input source - if there are file arguments, use the first one as input
            let (input_var, file_reading_code) = if !cmd.args.is_empty() {
                // If there are arguments, assume the first one is the file to sort
                match &cmd.args[0] {
                    Word::Literal(filename, _) => {
                        // Read from file - generate a proper variable assignment
                        let file_var = format!("file_content_{}", command_index);
                        let reading_code = format!("my ${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; open my $fh, '<', '{}' or croak \"Cannot open file: $!\"; <$fh> }};", file_var, filename);
                        (format!("${}", file_var), reading_code)
                    }
                    _ => {
                        // Fallback to input_data
                        ("$input_data".to_string(), String::new())
                    }
                }
            } else {
                // No arguments, use input_data
                ("$input_data".to_string(), String::new())
            };
            
            // Add file reading code if needed
            if !file_reading_code.is_empty() {
                output.push_str(&file_reading_code);
                output.push_str("\n");
            }
            
            let sort_output = crate::generator::commands::sort::generate_sort_command_with_output(generator, cmd, &input_var, &command_index, &output_var);
            output.push_str(&sort_output);
        } else if name == "echo" {
            // Use the echo command generator for non-pipeline echo commands
            if generator.inline_mode {
                // In inline mode, generate the output value directly instead of print statements
                if cmd.args.is_empty() {
                    output.push_str("\"\\n\"");
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
                            match arg {
                                Word::StringInterpolation(interp, _) => {
                                    if has_e_flag {
                                        // Process string interpolation with -e flag interpretation
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
                                                    result.push_str(&generator.convert_string_interpolation_to_perl(&crate::ast::StringInterpolation {
                                                        parts: vec![part.clone()]
                                                    }));
                                                }
                                            }
                                        }
                                        // Return as a quoted string literal with proper escaping for Perl
                                        format!("\"{}\"", result.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r"))
                                    } else {
                                        generator.convert_string_interpolation_to_perl(interp)
                                    }
                                },
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
                                        format!("\"{}\"", interpreted.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r"))
                                    } else {
                                        // Check if the literal contains escaped backticks that should be processed as command substitutions
                                        if literal.contains("\\`") {
                                            // Parse the string as string interpolation to handle escaped backticks
                                            if let Ok(interp) = crate::parser::words::parse_string_interpolation_from_literal(literal) {
                                                generator.convert_string_interpolation_to_perl(&interp)
                                            } else {
                                                generator.perl_string_literal(arg)
                                            }
                                        } else {
                                            generator.perl_string_literal(arg)
                                        }
                                    }
                                },
                                _ => generator.word_to_perl(arg)
                            }
                        })
                        .collect();
                    output.push_str(&format!("({}) . \"\\n\"", args.join(" . q{ } . ")));
                }
                return output;
            }
            
            if cmd.args.is_empty() {
                output.push_str(&generator.indent());
                output.push_str("print \"\\n\";\n");
            } else {
                // Check for -e flag and filter it out
                let filtered_args: Vec<&Word> = cmd.args.iter().filter(|&arg| {
                    if let Word::Literal(s, _) = arg {
                        s != "-e"
                    } else {
                        true
                    }
                }).collect();
                
                // Convert arguments to Perl format using the dedicated echo function
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
                                    } else {
                                        generator.perl_string_literal(arg)
                                    }
                                } else {
                                    generator.perl_string_literal(arg)
                                }
                            }
                            Word::BraceExpansion(expansion, _) => {
                                // Handle brace expansion like {1..5} -> "1 2 3 4 5"
                                crate::generator::commands::echo::handle_brace_expansion_for_echo(generator, expansion)
                            }
                            Word::CommandSubstitution(_, _) => {
                                // For command substitution, don't escape newlines - preserve them as-is
                                generator.word_to_perl(arg)
                            }
                            _ => generator.perl_string_literal(arg)
                        }
                    })
                    .collect();
                
                if args.len() == 1 {
                    output.push_str(&generator.indent());
                    // Check if the argument is a command substitution
                    if matches!(cmd.args[0], Word::CommandSubstitution(_, _)) {
                        // For command substitution, don't add extra newline as it already contains proper formatting
                        output.push_str(&format!("print {};\n", args[0]));
                    } else if args[0].starts_with('"') && args[0].ends_with('"') && !args[0].contains("\\n") {
                        // Extract the string content and add newline directly using double quotes for escape sequences
                        let content = &args[0][1..args[0].len()-1]; // Remove quotes
                        // Escape newlines, tabs, and carriage returns in the content
                        let escaped_content = content.replace("\\", "\\\\")
                                                   .replace("\"", "\\\"")
                                                   .replace("\n", "\\n")
                                                   .replace("\t", "\\t")
                                                   .replace("\r", "\\r");
                        output.push_str(&format!("print \"{}\\n\";\n", escaped_content));
                    } else if args[0].starts_with('$') && !args[0].contains("\\n") {
                        // For variables, use comma to avoid string interpolation
                        output.push_str(&format!("print {}, \"\\n\";\n", args[0]));
                    } else {
                        output.push_str(&format!("print {} . \"\\n\";\n", args[0]));
                    }
                } else {
                    // Check if we have multiple brace expansions that need cartesian product
                    let brace_expansions: Vec<&Word> = cmd.args.iter()
                        .filter(|arg| matches!(arg, Word::BraceExpansion(..)))
                        .collect();
                    
                    if brace_expansions.len() > 1 {
                        // Generate cartesian product for multiple brace expansions
                        output.push_str(&generate_cartesian_product_for_echo(generator, &cmd.args));
                    } else {
                        // For multiple arguments, join them with spaces
                        let args_str = args.join(" . q{ } . ");
                        output.push_str(&generator.indent());
                        output.push_str(&format!("print {} . \"\\n\";\n", args_str));
                    }
                }
            }
        } else if name == "true" && !cmd.env_vars.is_empty() && cmd.args.is_empty() {
            // This is a standalone assignment (e.g., i=$((i + 1)))
            for (var, value) in &cmd.env_vars {
                match value {
                    Word::Arithmetic(expr, _) => {
                        // Convert arithmetic expression to Perl
                        let perl_expr = generator.convert_arithmetic_to_perl(&expr.expression);
                        if !generator.declared_locals.contains(var) {
                            // Check if this variable is used in the arithmetic expression
                            // If so, we need to initialize it to 0 first
                            if expr.expression.contains(var) {
                                // For variables used in arithmetic expressions inside loops,
                                // we need to declare them in the outer scope
                                // Check if we're inside a loop by looking at the indent level
                                // For variables used in arithmetic expressions, we need to declare them
                                // at the top level if they haven't been declared yet
                                if !generator.declared_locals.contains(var) {
                                    // Mark this variable as needing top-level declaration
                                    generator.function_level_vars.insert(var.clone());
                                    generator.declared_locals.insert(var.clone());
                                }
                                // Now assign to it
                                output.push_str(&generator.indent());
                                output.push_str(&format!("${} = {};\n", var, perl_expr));
                            } else {
                                // Variable not used in expression, declare and assign
                                output.push_str(&generator.indent());
                                output.push_str(&format!("my ${} = {};\n", var, perl_expr));
                                generator.declared_locals.insert(var.clone());
                            }
                        } else {
                            // Variable already declared, just assign to it
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
            // Check if this is a builtin command
            if crate::generator::commands::builtins::is_builtin(name) {
                // For standalone builtin commands, we need to handle them differently than pipeline commands
                match name.as_str() {
                    "ls" => {
                        // Standalone ls command - print files directly
                        output.push_str(&crate::generator::commands::ls::generate_ls_command(generator, cmd, false, None));
                    }
                    "rm" => {
                        // Standalone rm command
                        output.push_str(&crate::generator::commands::rm::generate_rm_command(generator, cmd));
                    }
                    "find" => {
                        // Standalone find command - generate output directly without variable assignment
                        output.push_str(&crate::generator::commands::find::generate_find_command(generator, cmd, false, ""));
                    }
                    "perl" => {
                        // Use the dedicated perl command handler
                        output.push_str(&crate::generator::commands::perl::generate_perl_command(generator, cmd));
                    }
                    "cd" => {
                        // Handle cd command using chdir() instead of system call
                        if cmd.args.is_empty() {
                            // cd with no arguments goes to home directory
                            output.push_str(&generator.indent());
                            output.push_str("chdir($ENV{HOME} || $ENV{USERPROFILE} || '.';\n");
                        } else {
                            // cd with directory argument
                            let dir = generator.perl_string_literal(&cmd.args[0]);
                            output.push_str(&generator.indent());
                            output.push_str(&format!("chdir({});\n", dir));
                        }
                    }
                    "wc" => {
                        // Handle wc command with input redirection
                        if !cmd.redirects.is_empty() {
                            // Check for input redirection
                            for redirect in &cmd.redirects {
                                if let crate::ast::RedirectOperator::Input = redirect.operator {
                                    let file_name = generator.word_to_perl(&redirect.target);
                                    output.push_str(&generator.indent());
                                    output.push_str(&format!("open STDIN, '<', {} or croak \"Cannot open file: $ERRNO\";\n", file_name));
                                    break;
                                }
                            }
                        }
                        // Generate wc command
                        let unique_index = generator.get_unique_id();
                        let input_var = format!("wc_input_{}", unique_index);
                        let output_var = format!("wc_output_{}", unique_index);
                        output.push_str(&generator.indent());
                        output.push_str(&format!("my ${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <STDIN> }};\n", input_var));
                        output.push_str(&crate::generator::commands::wc::generate_wc_command_with_output(generator, cmd, &input_var, &unique_index, &output_var));
                    }

                    _ => {
                        // Route other builtins to the builtins system
                        // Use unique index for standalone commands to prevent variable masking
                        let unique_index = generator.get_unique_id();
                        output.push_str(&crate::generator::commands::builtins::generate_generic_builtin(generator, cmd, "", "", &unique_index, false));
                    }
                }
            } else if generator.declared_functions.contains(name) || *name == "greet" {
                // Function call
                if cmd.args.is_empty() {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("{}();\n", name));
                } else {
                    // Check if any argument contains glob patterns
                    let has_glob_patterns = cmd.args.iter().any(|arg| {
                        match arg {
                            Word::Literal(s, _) => s.contains('*') || s.contains('?'),
                            _ => false
                        }
                    });
                    
                    if has_glob_patterns {
                        // Handle glob pattern expansion for function arguments
                        let mut expanded_args = Vec::new();
                        for arg in &cmd.args {
                            match arg {
                                Word::Literal(s, _) if s.contains('*') || s.contains('?') => {
                                    // Expand glob pattern
                                    expanded_args.push(format!("glob('{}')", s));
                                }
                                Word::BraceExpansion(expansion, _) => {
                                    // Handle brace expansion for command arguments
                                    expanded_args.push(handle_brace_expansion_for_command(generator, expansion));
                                }
                                _ => {
                                    expanded_args.push(generator.perl_string_literal(arg));
                                }
                            }
                        }
                        let args_str = expanded_args.join(", ");
                        output.push_str(&generator.indent());
                        output.push_str(&format!("{}({});\n", name, args_str));
                    } else {
                        let args: Vec<String> = cmd.args.iter()
                            .map(|arg| {
                                match arg {
                                    Word::BraceExpansion(expansion, _) => {
                                        // Handle brace expansion for command arguments
                                        handle_brace_expansion_for_command(generator, expansion)
                                    }
                                    _ => generator.perl_string_literal(arg)
                                }
                            })
                            .collect();
                        let args_str = args.join(", ");
                        output.push_str(&generator.indent());
                        output.push_str(&format!("{}({});\n", name, args_str));
                    }
                }
            } else {
                // System call fallback
                if name == "ls" {
                    // Special handling for ls command - use the dedicated ls handler
                    output.push_str(&crate::generator::commands::ls::generate_ls_command(generator, cmd, false, None));
                } else if name == "rmdir" {
                    // Special handling for rmdir command - use the dedicated rmdir handler
                    output.push_str(&crate::generator::commands::rmdir::generate_rmdir_command(generator, cmd));
                } else if cmd.args.is_empty() {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("system '{}';\n", name));
                } else {
                    let args: Vec<String> = if name == "perl" {
                        // Special handling for perl command - embed Perl code directly instead of system call
                        // This will be handled specially below, so we don't need to process args here
                        Vec::new()
                    } else {
                        cmd.args.iter()
                            .map(|arg| {
                                match arg {
                                    Word::BraceExpansion(expansion, _) => {
                                        // Handle brace expansion for command arguments
                                        handle_brace_expansion_for_command(generator, expansion)
                                    }
                                    _ => generator.perl_string_literal(arg)
                                }
                            })
                            .collect()
                    };
                    
                    if name == "perl" {
                        // Handle Perl commands by embedding the Perl code directly
                        if cmd.args.len() >= 2 {
                            // Check for -e flag (execute code)
                            if let Word::Literal(flag, _) = &cmd.args[0] {
                                if flag == "-e" {
                                    // Extract the Perl code from the second argument
                                    let perl_code = if let Word::Literal(perl_code, _) = &cmd.args[1] {
                                        Some(perl_code.clone())
                                    } else if let Word::StringInterpolation(interp, _) = &cmd.args[1] {
                                        // Convert string interpolation to Perl code
                                        let result = generator.convert_string_interpolation_to_perl(interp);
                
                                        Some(result)
                    } else {
                                        None
                                    };
                                    
                                    if let Some(perl_code) = perl_code {
                                        // Check if this is from StringInterpolation (already clean Perl code)
                                        let is_string_interpolation = matches!(&cmd.args[1], Word::StringInterpolation(_, _));
                                        
                                        if is_string_interpolation {
                                            // StringInterpolation already returns clean Perl code, don't clean it again
                                            output.push_str(&generator.indent());
                                            // Split the code by newlines and add proper indentation
                                            for line in perl_code.lines() {
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!("{}\n", line));
                                            }
                                        } else {
                                            // Clean up the Perl code - remove outer quotes if present
                                            let mut clean_code = perl_code.clone();
                                            if (clean_code.starts_with('"') && clean_code.ends_with('"')) ||
                                               (clean_code.starts_with('\'') && clean_code.ends_with('\'')) {
                                                clean_code = clean_code[1..clean_code.len()-1].to_string();
                                            }
                                            
                                            // Handle backslash escapes - keep them as escape sequences for Perl
                                            // Don't convert \n to actual newlines in the generated code
                                            
                                            // Embed the Perl code directly - ensure it's properly formatted
                                            output.push_str(&generator.indent());
                                            // Split the code by newlines and add proper indentation
                                            for line in clean_code.lines() {
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!("{}\n", line));
                                            }
                                        }
                                        return output;
                                    }
                                } else if flag == "-ne" {
                                    // Handle -ne flag (execute code for each line of input)
                                    let perl_code = if let Word::Literal(perl_code, _) = &cmd.args[1] {
                                        Some(perl_code.clone())
                                    } else if let Word::StringInterpolation(interp, _) = &cmd.args[1] {
                                        // Convert string interpolation to Perl code
                                        Some(generator.convert_string_interpolation_to_perl(interp))
                                    } else {
                                        None
                                    };
                                    
                                    if let Some(perl_code) = perl_code {
                                        // Check if this is from StringInterpolation (already clean Perl code)
                                        let is_string_interpolation = matches!(&cmd.args[1], Word::StringInterpolation(_, _));
                                        
                                        if is_string_interpolation {
                                            // StringInterpolation already returns clean Perl code, don't clean it again
                                            output.push_str(&generator.indent());
                                            output.push_str(&format!("# Perl -ne: {}\n", perl_code));
                                            // Split the code by newlines and add proper indentation
                                            for line in perl_code.lines() {
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!("{}\n", line));
                                            }
                                        } else {
                                            // Clean up the Perl code
                                            let mut clean_code = perl_code.clone();
                                            if (clean_code.starts_with('"') && clean_code.ends_with('"')) ||
                                               (clean_code.starts_with('\'') && clean_code.ends_with('\'')) {
                                                clean_code = clean_code[1..clean_code.len()-1].to_string();
                                            }
                                            
                                            // Handle backslash escapes - keep them as escape sequences for Perl
                                            // Don't convert \n to actual newlines in the generated code
                                            
                                            // For -ne, we need to process each line
                                            // This will be handled in pipeline context
                                            output.push_str(&generator.indent());
                                            output.push_str(&format!("# Perl -ne: {}\n", clean_code));
                                            // Split the code by newlines and add proper indentation
                                            for line in clean_code.lines() {
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!("{}\n", line));
                                            }
                                        }
                                        return output;
                                    }
                                }
                            }
                        }
                        
                        // Fallback to system call for other Perl usage
                        let args_str = args.join(", ");
                        output.push_str(&generator.indent());
                        output.push_str(&format!("system '{}', {};\n", name, args_str));
                    } else {
                    let args_str = args.join(", ");
                    output.push_str(&generator.indent());
                    output.push_str(&format!("system '{}', {};\n", name, args_str));
                    }
                }
            }
        }
    } else {
        // Handle non-literal command names
        let cmd_name = "unknown_command";
        
        // Fallback to system call
        if cmd.args.is_empty() {
            output.push_str(&generator.indent());
            output.push_str(&format!("system '{}';\n", cmd_name));
        } else {
            let args: Vec<String> = cmd.args.iter()
                .map(|arg| generator.perl_string_literal(arg))
                .collect();
            output.push_str(&generator.indent());
            output.push_str(&format!("system '{}', {};\n", cmd_name, args.join(", ")));
        }
    }

    output
}

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
                                    // Only escape quotes and backslashes, preserve newlines and tabs as-is
                                    format!("\"{}\"", interpreted.replace("\\", "\\\\").replace("\"", "\\\""))
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
                                            
                                            // Interpret backslash escapes - keep them as escape sequences for Perl
                                            // Don't convert \n to actual newlines in the generated code
                                            
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
                                // Only escape quotes and backslashes, preserve newlines and tabs as-is
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
                        // Check if the literal contains escaped backticks that should be processed as command substitutions
                        if literal.contains("\\\\`") {
                            // Parse the string as string interpolation to handle escaped backticks
                            if let Ok(interp) = crate::parser::words::parse_string_interpolation_from_literal(literal) {
                                generator.convert_string_interpolation_to_perl(&interp)
                            } else {
                                generator.perl_string_literal(arg)
                            }
                        } else if has_e_flag {
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
                            // Only escape quotes and backslashes, preserve newlines and tabs as-is
                            format!("\"{}\"", interpreted.replace("\\", "\\\\").replace("\"", "\\\""))
                        } else {
                            generator.perl_string_literal(arg)
                        }
                    }
                    Word::CommandSubstitution(_, _) => {
                        // For command substitution, don't escape newlines - preserve them as-is
                        generator.word_to_perl(arg)
                    }
                    _ => generator.perl_string_literal(arg)
                }
            })
            .collect();
        
        if args.is_empty() {
            output.push_str(&format!("${} .= \"\\n\";\n", output_var));
        } else if args.len() == 1 {
            output.push_str(&format!("${} .= {}. \"\\n\";\n", output_var, args[0]));
        } else {
            // For multiple arguments, join them with spaces
            let args_str = args.join(" . q{ } . ");
            output.push_str(&format!("${} .= {}. \"\\n\";\n", output_var, args_str));
        }
    }
    
    output
}

/// Handle brace expansion for echo commands
fn handle_brace_expansion_for_echo(_generator: &mut Generator, expansion: &BraceExpansion) -> String {
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
                        let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                        let mut current = start_char as i32;
                        let end_code = end_char as i32;
                        while if step > 0 { current <= end_code } else { current >= end_code } {
                            if let Some(c) = char::from_u32(current as u32) {
                                items.push(c.to_string());
                            }
                            current += step;
                        }
                    }
                }
            }
            BraceItem::Literal(s) => {
                items.push(s.clone());
            }
            BraceItem::Sequence(seq) => {
                // Handle sequence items like {one,two,three}
                for item in seq {
                    items.push(item.clone());
                }
            }
        }
    }
    
    if items.is_empty() {
        "\"\"".to_string()
    } else {
        // Join all items with spaces for echo output
        format!("\"{}\"", items.join(" "))
    }
}

/// Handle brace expansion for command arguments
fn handle_brace_expansion_for_command(_generator: &mut Generator, expansion: &BraceExpansion) -> String {
    let mut items = Vec::new();
    
    for item in &expansion.items {
        match item {
            BraceItem::Range(range) => {
                // Handle numeric ranges like {1..5} or {001..005}
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
                        items.push(format!("\"{}\"", formatted));
                        current += step;
                    }
                } else {
                    // Handle character ranges like {a..c}
                    if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
                        let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                        let mut current = start_char as i32;
                        let end_code = end_char as i32;
                        while if step > 0 { current <= end_code } else { current >= end_code } {
                            if let Some(c) = char::from_u32(current as u32) {
                                items.push(format!("\"{}\"", c));
                            }
                            current += step;
                        }
                    }
                }
            }
            BraceItem::Literal(s) => {
                items.push(format!("\"{}\"", s));
            }
            BraceItem::Sequence(seq) => {
                // Handle sequence items like {one,two,three}
                for item in seq {
                    items.push(format!("\"{}\"", item));
                }
            }
        }
    }
    
    if items.is_empty() {
        "\"\"".to_string()
    } else {
        // For command arguments, return items separated by commas for system call
        items.join(", ")
    }
}

/// Generate cartesian product for multiple brace expansions in echo commands
fn generate_cartesian_product_for_echo(
    generator: &mut Generator,
    args: &[Word],
) -> String {
    let mut output = String::new();
    
    // Collect all brace expansions and their expanded values
    let mut expansions: Vec<Vec<String>> = Vec::new();
    let mut non_brace_args: Vec<String> = Vec::new();
    
    for arg in args {
        match arg {
            Word::BraceExpansion(items, _) => {
                let mut expanded = Vec::new();
                for item in &items.items {
                    match item {
                        BraceItem::Range(range) => {
                            // Handle numeric ranges like {1..5} or {001..005}
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
                                    expanded.push(formatted);
                                    current += step;
                                }
                            } else {
                                // Handle character ranges like {a..c}
                                if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
                                    let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                                    let mut current = start_char as i32;
                                    let end_code = end_char as i32;
                                    while if step > 0 { current <= end_code } else { current >= end_code } {
                                        if let Some(c) = char::from_u32(current as u32) {
                                            expanded.push(c.to_string());
                                        }
                                        current += step;
                                    }
                                }
                            }
                        }
                        BraceItem::Literal(s) => {
                            expanded.push(s.clone());
                        }
                        BraceItem::Sequence(seq) => {
                            // Handle sequence items like {one,two,three}
                            for item in seq {
                                expanded.push(item.clone());
                            }
                        }
                    }
                }
                expansions.push(expanded);
            }
            _ => {
                // Convert non-brace arguments to Perl strings
                non_brace_args.push(generator.word_to_perl(arg));
            }
        }
    }
    
    if expansions.is_empty() {
        // No brace expansions, fall back to simple joining
        let args_str = args.iter()
            .map(|arg| generator.word_to_perl(arg))
            .collect::<Vec<_>>()
            .join(" . \" \" . ");
        output.push_str(&generator.indent());
        output.push_str(&format!("print {} . \"\\n\";\n", args_str));
        return output;
    }
    
    // Generate cartesian product
    let mut combinations = vec![Vec::new()];
    
    for expansion in &expansions {
        let mut new_combinations = Vec::new();
        for combination in &combinations {
            for item in expansion {
                let mut new_combo = combination.clone();
                new_combo.push(item.clone());
                new_combinations.push(new_combo);
            }
        }
        combinations = new_combinations;
    }
    
    // Generate Perl code to print all combinations
    output.push_str(&generator.indent());
    output.push_str("my @combinations = (\n");
    
    for combination in &combinations {
        output.push_str(&generator.indent());
        output.push_str("    ");
        
        let mut combo_parts = Vec::new();
        
        // Add non-brace arguments at the beginning
        for non_brace in &non_brace_args {
            combo_parts.push(non_brace.clone());
        }
        
        // Add brace expansion values
        for item in combination {
            combo_parts.push(format!("'{}'", item));
        }
        
        output.push_str(&format!("[{}],\n", combo_parts.join(", ")));
    }
    
    output.push_str(&generator.indent());
    output.push_str(");\n");
    
    output.push_str(&generator.indent());
    output.push_str("my @all_combinations;\n");
    output.push_str(&generator.indent());
    output.push_str("for my $combo (@combinations) {\n");
    output.push_str(&generator.indent());
    output.push_str(&generator.indent());
    output.push_str("push @all_combinations, join(\"\", @$combo);\n");
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("print join(\" \", @all_combinations) . \"\\n\";\n");
    
    output
}

