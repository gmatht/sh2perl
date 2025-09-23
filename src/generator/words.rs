use crate::ast::*;
use super::Generator;
use regex::Regex;

pub fn word_to_perl_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Handle literal strings
            if s.contains("..") {
                generator.handle_range_expansion(s)
            } else if s.contains(',') {
                generator.handle_comma_expansion(s)
            } else {
                // For literal strings, only replace constants in specific contexts
                // Don't replace numbers that are part of arithmetic expressions
                s.clone()
            }
        },
        Word::ParameterExpansion(pe, _) => generator.generate_parameter_expansion(pe),
        Word::Array(name, elements, _) => {
            let elements_str = elements.iter()
                .map(|e| format!("'{}'", e.replace("'", "\\'")))
                .collect::<Vec<_>>()
                .join(", ");
            format!("@{} = ({});", name, elements_str)
        },
        Word::StringInterpolation(interp, _) => generator.convert_string_interpolation_to_perl(interp),
        Word::Arithmetic(expr, _) => generator.convert_arithmetic_to_perl(&expr.expression),
        Word::BraceExpansion(expansion, _) => {
            let expanded = generator.handle_brace_expansion(expansion);
            // Quote the result since it's used in contexts where quotes are needed
            format!("\"{}\"", expanded)
        },
        Word::CommandSubstitution(cmd, _) => {
            // Handle command substitution
            eprintln!("DEBUG: CommandSubstitution called with command: {:?}", cmd);
            let result = match cmd.as_ref() {
                Command::Simple(simple_cmd) => {
                    let cmd_name = generator.word_to_perl(&simple_cmd.name);
                    
                    // Check if this is a builtin command that we can convert properly
                    if let Word::Literal(name, _) = &simple_cmd.name {
                        if name == "ls" {
                            // Use the ls substitution function for proper conversion
                            let perl_code = crate::generator::commands::ls::generate_ls_for_substitution(generator, simple_cmd);
                            
                            // For backtick commands, we need to return the value, not print it
                            // The generate_ls_for_substitution already returns the joined string
                            perl_code
                        } else if name == "find" {
                            // Use the find command handler for proper conversion
                            let perl_code = crate::generator::commands::find::generate_find_command(generator, simple_cmd, true, "found_files");
                            
                            // For backtick commands, we need to return the value, not print it
                            // The generate_find_command already returns the joined string
                            perl_code
                        } else if name == "paste" {
                            // Special handling for paste command
                            // Check if this command has process substitution redirects
                            let mut has_process_sub = false;
                            for redirect in &simple_cmd.redirects {
                                if matches!(redirect.operator, crate::ast::RedirectOperator::ProcessSubstitutionInput(_)) {
                                    has_process_sub = true;
                                    break;
                                }
                            }
                            
                            if has_process_sub {
                                // Handle paste command with process substitution
                                // This should be handled as a regular command, not command substitution
                                // We need to generate the proper paste command with process substitution
                                let mut process_sub_files = Vec::new();
                                let mut process_sub_code = String::new();
                                
                                for redirect in &simple_cmd.redirects {
                                    if let crate::ast::RedirectOperator::ProcessSubstitutionInput(cmd) = &redirect.operator {
                                        // Generate the process substitution command and create temp file
                                        let temp_file_id = generator.get_unique_id();
                                        let temp_file = format!("temp_file_ps_{}", temp_file_id);
                                        
                                        // Check if this is an echo command and use the dedicated echo generator
                                        let process_sub_output = if let crate::ast::Command::Simple(echo_cmd) = &**cmd {
                                            if let crate::ast::Word::Literal(name, _) = &echo_cmd.name {
                                                if name == "echo" {
                                                    // Use the dedicated echo command generator
                                                    crate::generator::commands::echo::generate_echo_command(generator, echo_cmd, "", "temp_output")
                                                } else {
                                                    generator.generate_command(cmd)
                                                }
                                            } else {
                                                generator.generate_command(cmd)
                                            }
                                        } else {
                                            generator.generate_command(cmd)
                                        };
                                        
                                        // Generate code to execute the process substitution and save to temp file
                                        process_sub_code.push_str(&format!("my ${} = ($ENV{{TEMP}} || $ENV{{TMP}} || \"C:\\\\temp\") . '/process_sub_{}.tmp';\n", 
                                            temp_file, temp_file_id));
                                        process_sub_code.push_str(&format!("{{\n"));
                                        process_sub_code.push_str(&format!("    open my $fh, '>', ${} or croak \"Cannot create temp file: $ERRNO\\n\";\n", temp_file));
                                        
                                        // Check if this is an echo command and handle it specially
                                        if let crate::ast::Command::Simple(echo_cmd) = &**cmd {
                                            if let crate::ast::Word::Literal(name, _) = &echo_cmd.name {
                                                if name == "echo" {
                                                    // For echo commands, we need to execute the echo command and capture its output
                                                    process_sub_code.push_str(&format!("    my $temp_output = \"\";\n"));
                                                    process_sub_code.push_str(&format!("    {}\n", process_sub_output));
                                                    process_sub_code.push_str(&format!("    print $fh $temp_output;\n"));
                                                } else {
                                                    process_sub_code.push_str(&format!("    print $fh {};\n", process_sub_output));
                                                }
                                            } else {
                                                process_sub_code.push_str(&format!("    print $fh {};\n", process_sub_output));
                                            }
                                        } else {
                                            process_sub_code.push_str(&format!("    print $fh {};\n", process_sub_output));
                                        }
                                        process_sub_code.push_str(&format!("    close $fh or croak \"Close failed: $ERRNO\\n\";\n"));
                                        process_sub_code.push_str(&format!("}}\n"));
                                        
                                        process_sub_files.push((temp_file.clone(), process_sub_output));
                                    }
                                }
                                
                                // Use the paste generator for proper output handling
                                let paste_output = crate::generator::commands::paste::generate_paste_command(generator, simple_cmd, &process_sub_files);
                                format!("do {{ {} {} }}", process_sub_code, paste_output)
                            } else {
                                // Regular paste command without process substitution - use dedicated implementation
                                crate::generator::commands::paste::generate_paste_command(generator, simple_cmd, &[])
                            }
                        } else if name == "comm" {
                            // Special handling for comm command with process substitution
                            // Check if this command has process substitution redirects
                            eprintln!("DEBUG: comm command detected, checking for process substitution");
                            let mut has_process_sub = false;
                            for redirect in &simple_cmd.redirects {
                                if matches!(redirect.operator, crate::ast::RedirectOperator::ProcessSubstitutionInput(_)) {
                                    has_process_sub = true;
                                    eprintln!("DEBUG: comm command has process substitution redirects");
                                    break;
                                }
                            }
                            
                            if has_process_sub {
                                eprintln!("DEBUG: Using builtin comm command generator for process substitution");
                                // Handle comm command with process substitution like paste command
                                let mut process_sub_code = String::new();
                                let mut process_sub_files = Vec::new();
                                
                                for redirect in &simple_cmd.redirects {
                                    if let crate::ast::RedirectOperator::ProcessSubstitutionInput(sub_cmd) = &redirect.operator {
                                        let temp_file_id = generator.get_unique_id();
                                        let temp_file = format!("temp_file_ps_{}", temp_file_id);
                                        
                                        let process_sub_output = match sub_cmd.as_ref() {
                                            Command::Simple(simple_sub_cmd) => {
                                                generator.generate_simple_command(simple_sub_cmd)
                                            }
                                            _ => {
                                                // For non-simple commands, we need to generate the command differently
                                                // This is a placeholder - we may need to implement this properly
                                                format!("\"Command not supported in process substitution\"")
                                            }
                                        };
                                        
                                        // Generate code to execute the process substitution and save to temp file
                                        process_sub_code.push_str(&format!("my ${} = ($ENV{{TEMP}} || $ENV{{TMP}} || \"C:\\\\temp\") . '/process_sub_{}.tmp';\n", 
                                            temp_file, temp_file_id));
                                        process_sub_code.push_str(&format!("{{\n"));
                                        process_sub_code.push_str(&format!("    open my $fh, '>', ${} or croak \"Cannot create temp file: $ERRNO\\n\";\n", temp_file));
                                        process_sub_code.push_str(&format!("    my $temp_output = \"\";\n"));
                                        process_sub_code.push_str(&format!("    $temp_output .= {};\n", process_sub_output));
                                        process_sub_code.push_str(&format!("    print $fh $temp_output;\n"));
                                        process_sub_code.push_str(&format!("    close $fh or croak \"Close failed: $ERRNO\\n\";\n"));
                                        process_sub_code.push_str(&format!("}}\n"));
                                        
                                        process_sub_files.push((temp_file.clone(), process_sub_output));
                                    }
                                }
                                
                                // Use the comm generator for proper output handling
                                let comm_output = crate::generator::commands::comm::generate_comm_command(generator, simple_cmd, "cmd_result", &process_sub_files);
                                format!("do {{ {} {} }}", process_sub_code, comm_output)
                            } else {
                                eprintln!("DEBUG: comm command has no process substitution, using dedicated implementation");
                                // Regular comm command without process substitution - use dedicated implementation
                                let comm_output = crate::generator::commands::comm::generate_comm_command(generator, simple_cmd, "comm_result", &[]);
                                format!("do {{ {} }}", comm_output)
                            }
                        } else if name == "diff" {
                            // Special handling for diff command in command substitution
                            eprintln!("DEBUG: Processing diff command in command substitution with args: {:?}", simple_cmd.args);
                            
                            // Use the dedicated diff command implementation
                            let diff_output = crate::generator::commands::diff::generate_diff_command(generator, simple_cmd, "diff_result", 0, false);
                            format!("do {{ {} }}", diff_output)
                        } else if name == "xargs" {
                            // Special handling for xargs command in command substitution
                            eprintln!("DEBUG: Processing xargs command in command substitution with args: {:?}", simple_cmd.args);
                            
                            // Use the dedicated xargs command generator
                            let unique_id = generator.get_unique_id();
                            let xargs_output = crate::generator::commands::xargs::generate_xargs_command_with_output(generator, simple_cmd, "input_data", &unique_id.to_string(), "xargs_result");
                            
                            // For command substitution, we need to return the result, not print it
                            format!("do {{ my $input_data = q{{}}; {} }}", xargs_output)
                        } else if name == "tr" {
                            // Special handling for tr command in command substitution
                            eprintln!("DEBUG: Processing tr command in command substitution with args: {:?}", simple_cmd.args);
                            
                            // Use the dedicated tr command generator for substitution (no newline)
                            let unique_id = generator.get_unique_id();
                            let tr_output = crate::generator::commands::tr::generate_tr_command_for_substitution(generator, simple_cmd, "input_data", &unique_id.to_string());
                            
                            // For command substitution, we need to return the result, not print it
                            format!("do {{ my $input_data = q{{}}; {} }}", tr_output)
                        } else if name == "perl" {
                            // Special handling for perl in command substitution - execute as external command
                            eprintln!("DEBUG: Processing perl command in command substitution with args: {:?}", simple_cmd.args);
                            
                            // Use IPC::Open3 instead of qx to avoid Perl::Critic violations
                            let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                            
                            if simple_cmd.args.len() >= 2 {
                                if let (Word::Literal(flag, _), Word::Literal(code, _)) = (&simple_cmd.args[0], &simple_cmd.args[1]) {
                                    if flag == "-e" {
                                        // Use temporary file approach for perl -e commands with IPC::Open3
                                        let temp_file = format!("temp_perl_{}.pl", std::process::id());
                                        let code_literal = generator.perl_string_literal(&Word::Literal(code.clone(), None));
                                        format!("do {{ 
                                            open my $fh, '>', '{}' or croak 'Cannot create temp file: $!'; 
                                            print {{$fh}} {}; 
                                            close $fh or croak 'Close failed: $!'; 
                                            my ({}, {}, {}); 
                                            my {} = open3({}, {}, {}, 'perl', '{}'); 
                                            close {} or croak 'Close failed: $!'; 
                                            my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; 
                                            close {} or croak 'Close failed: $!'; 
                                            waitpid {}, 0; 
                                            unlink '{}' or carp 'Cannot remove temp file: $!'; 
                                            {};
                                        }}", 
                                            temp_file, code_literal, in_var, out_var, err_var, pid_var, in_var, out_var, err_var, temp_file, in_var, result_var, out_var, out_var, pid_var, temp_file, result_var)
                                    } else {
                                        // Use IPC::Open3 for other perl commands
                                        let args: Vec<String> = simple_cmd.args.iter()
                                            .map(|arg| generator.perl_string_literal(arg))
                                            .collect();
                                        let formatted_args = args.join(", ");
                                        format!("do {{ 
                                            my ({}, {}, {}); 
                                            my {} = open3({}, {}, {}, 'perl', {}); 
                                            close {} or croak 'Close failed: $!'; 
                                            my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; 
                                            close {} or croak 'Close failed: $!'; 
                                            waitpid {}, 0; 
                                            {};
                                        }}", 
                                            in_var, out_var, err_var, pid_var, in_var, out_var, err_var, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                                    }
                                } else {
                                    // Use IPC::Open3 for other perl commands
                                    let args: Vec<String> = simple_cmd.args.iter()
                                        .map(|arg| generator.perl_string_literal(arg))
                                        .collect();
                                    let formatted_args = args.join(", ");
                                    format!("do {{ 
                                        my ({}, {}, {}); 
                                        my {} = open3({}, {}, {}, 'perl', {}); 
                                        close {} or croak 'Close failed: $!'; 
                                        my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; 
                                        close {} or croak 'Close failed: $!'; 
                                        waitpid {}, 0; 
                                        {};
                                    }}", 
                                        in_var, out_var, err_var, pid_var, in_var, out_var, err_var, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                                }
                            } else {
                                // Use IPC::Open3 for perl commands with no arguments
                                format!("do {{ 
                                    my ({}, {}, {}); 
                                    my {} = open3({}, {}, {}, 'perl'); 
                                    close {} or croak 'Close failed: $!'; 
                                    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; 
                                    close {} or croak 'Close failed: $!'; 
                                    waitpid {}, 0; 
                                    {};
                                }}", 
                                    in_var, out_var, err_var, pid_var, in_var, out_var, err_var, in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        } else if name == "wc" {
                            // Special handling for wc in command substitution
                            if simple_cmd.args.len() >= 1 {
                                if let Word::Literal(flag, _) = &simple_cmd.args[0] {
                                    if flag == "-c" {
                                        // Check for input redirects first
                                        for redirect in &simple_cmd.redirects {
                                            if let crate::ast::RedirectOperator::Input = redirect.operator {
                                                let file_name = generator.word_to_perl(&redirect.target);
                                                return format!("-s {}", file_name);
                                            }
                                        }
                                        
                                        // If no redirects, check if there's a file argument
                                        if simple_cmd.args.len() >= 2 {
                                            if let Word::Literal(file, _) = &simple_cmd.args[1] {
                                                // Handle wc -c < file pattern
                                                if file.starts_with('<') {
                                                    let file_name = file.strip_prefix('<').unwrap_or("").trim();
                                                    format!("-s {}", file_name)
                                                } else {
                                                    format!("-s {}", file)
                                                }
                                            } else {
                                                format!("-s {}", generator.word_to_perl(&simple_cmd.args[1]))
                                            }
                                        } else {
                                            // No file argument, use stdin
                                            "do { local $INPUT_RECORD_SEPARATOR = undef; <STDIN> }".to_string()
                                        }
                                    } else {
                                        // Fallback to bash execution for other wc options
                                        let args_str = simple_cmd.args.iter()
                                            .map(|arg| generator.word_to_perl(arg))
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        format!("do {{ my ($in, $out, $err); my $pid = open3($in, $out, $err, 'wc', {}); close $in or croak 'Close failed: $!'; my $result = do {{ local $INPUT_RECORD_SEPARATOR = undef; <$out> }}; close $out or croak 'Close failed: $!'; waitpid $pid, 0; $result }}", args_str)
                                    }
                                } else {
                                    // Fallback to bash execution
                                    let args_str = simple_cmd.args.iter()
                                        .map(|arg| generator.word_to_perl(arg))
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    format!("do {{ my ($in, $out, $err); my $pid = open3($in, $out, $err, 'wc', {}); close $in or croak 'Close failed: $!'; my $result = do {{ local $INPUT_RECORD_SEPARATOR = undef; <$out> }}; close $out or croak 'Close failed: $!'; waitpid $pid, 0; $result }}", args_str)
                                }
                            } else if simple_cmd.args.len() == 1 {
                                // Check for wc -c with redirect: wc -c < file
                                if let Word::Literal(flag, _) = &simple_cmd.args[0] {
                                    if flag == "-c" && !simple_cmd.redirects.is_empty() {
                                        // Look for input redirect
                                        for redirect in &simple_cmd.redirects {
                                            if let crate::ast::RedirectOperator::Input = redirect.operator {
                                                let file_name = generator.word_to_perl(&redirect.target);
                                                return format!("-s {}", file_name);
                                            }
                                        }
                                    }
                                }
                                // Fallback to bash execution
                                let args_str = simple_cmd.args.iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("do {{ my ($in, $out, $err); my $pid = open3($in, $out, $err, 'wc', {}); close $in or croak 'Close failed: $!'; my $result = do {{ local $INPUT_RECORD_SEPARATOR = undef; <$out> }}; close $out or croak 'Close failed: $!'; waitpid $pid, 0; $result }}", args_str)
                            } else {
                                // Fallback to bash execution
                                let args_str = simple_cmd.args.iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("do {{ my ($in, $out, $err); my $pid = open3($in, $out, $err, 'wc', {}); close $in or croak 'Close failed: $!'; my $result = do {{ local $INPUT_RECORD_SEPARATOR = undef; <$out> }}; close $out or croak 'Close failed: $!'; waitpid $pid, 0; $result }}", args_str)
                            }
                        } else if name == "echo" {
                            // Special handling for echo in command substitution
                            if simple_cmd.args.is_empty() {
                                "\"\\n\"".to_string()
                            } else {
                                // Process arguments with proper string interpolation handling
                                let args: Vec<String> = simple_cmd.args.iter()
                                    .map(|arg| {
                                        match arg {
                                            Word::StringInterpolation(interp, _) => {
                                                generator.convert_string_interpolation_to_perl(interp)
                                            },
                                            Word::Literal(literal, _) => {
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
                                            },
                                            _ => generator.word_to_perl(arg)
                                        }
                                    })
                                    .collect();
                                // For command substitution, don't add newline as it will be added by the consuming command
                                format!("({})", args.join(" . q{ } . "))
                            }
                        } else if name == "sha256sum" {
                            // Use the sha256sum command handler for proper conversion
                            eprintln!("DEBUG: words.rs - Using native sha256sum implementation for command substitution");
                            crate::generator::commands::sha256sum::generate_sha256sum_command(generator, simple_cmd, "")
                        } else if name == "sha512sum" {
                            // Use the sha512sum command handler for proper conversion
                            eprintln!("DEBUG: words.rs - Using native sha512sum implementation for command substitution");
                            crate::generator::commands::sha512sum::generate_sha512sum_command(generator, simple_cmd, "")
                        } else if name == "grep" {
                            // Special handling for grep in command substitution
                            // Use a simplified approach similar to ls substitution
                            eprintln!("DEBUG: words.rs - Using native grep implementation for command substitution");
                            let unique_id = generator.get_unique_id();
                            let args: Vec<String> = simple_cmd.args.iter()
                                .map(|arg| generator.word_to_perl(arg))
                                .collect();
                            
                            if args.is_empty() {
                                "\"\"".to_string()
                            } else {
                                // Parse grep arguments properly
                                let mut pattern_idx = 0;
                                let mut file_idx = 1;
                                let mut show_line_numbers = false;
                                
                                // Skip flags like -n, -i, etc.
                                while pattern_idx < args.len() && args[pattern_idx].starts_with('-') {
                                    if args[pattern_idx] == "-n" {
                                        show_line_numbers = true;
                                    }
                                    pattern_idx += 1;
                                    file_idx += 1;
                                }
                                
                                if pattern_idx >= args.len() {
                                    return "\"\"".to_string();
                                }
                                
let pattern = &args[pattern_idx];
                                let files = if file_idx < args.len() { &args[file_idx..] } else { &[] };
                                
                                if files.is_empty() {
                                    // No files specified, grep will fail (no input)
                                    format!("do {{ carp \"grep: {}: No such file or directory\"; \"\" }}", pattern)
                                } else {
                                    let file = &files[0];
                                    // Adjust file path for Perl execution context (runs from examples directory)
                                    let adjusted_file = generator.adjust_file_path_for_perl_execution(file);
                                    // Ensure the file is properly quoted
                                    let quoted_file = if adjusted_file.starts_with('\'') || adjusted_file.starts_with('"') {
                                        adjusted_file.clone()
                                    } else {
                                        format!("'{}'", adjusted_file)
                                    };
                                    
                                    if show_line_numbers {
                                        format!("do {{ my @grep_lines_{}; my $fh_{}; my $line_num_{} = 0; if (-f {}) {{ open $fh_{}, '<', {} or croak \"Cannot open file: $OS_ERROR\"; while (my $line = <$fh_{}>) {{ $line_num_{}++; chomp $line; if ($line =~ /{}/msx) {{ push @grep_lines_{}, \"$line_num_{}:$line\"; }} }} close $fh_{} or croak \"Close failed: $OS_ERROR\"; }} join \"\\n\", @grep_lines_{}; }}", 
                                            unique_id, unique_id, unique_id, quoted_file, unique_id, quoted_file, unique_id, unique_id, pattern.trim_matches('\'').trim_matches('"'), unique_id, unique_id, unique_id, unique_id)
                                    } else {
                                        format!("do {{ my @grep_lines_{}; my $fh_{}; if (-f {}) {{ open $fh_{}, '<', {} or croak \"Cannot open file: $OS_ERROR\"; @grep_lines_{} = <$fh_{}>; close $fh_{} or croak \"Close failed: $OS_ERROR\"; chomp @grep_lines_{}; @grep_lines_{} = grep {{ /{}/msx }} @grep_lines_{}; }} join \"\\n\", @grep_lines_{}; }}", 
                                            unique_id, unique_id, quoted_file, unique_id, quoted_file, unique_id, unique_id, unique_id, unique_id, unique_id, pattern.trim_matches('\'').trim_matches('"'), unique_id, unique_id)
                                    }
                                }
                            }
                        } else if name == "printf" {
                            // Special handling for printf in command substitution
                            let mut format_string = String::new();
                            let mut args = Vec::new();
                            
                            for (i, arg) in simple_cmd.args.iter().enumerate() {
                                if i == 0 {
                                    // For printf format strings, handle string interpolation specially
                                    match arg {
                                        Word::StringInterpolation(interp, _) => {
                                            // For printf format strings, we want the raw string without escape processing
                                            // Reconstruct the original string from the interpolation parts
                                            format_string = interp.parts.iter()
                                                .map(|part| match part {
                                                    StringPart::Literal(s) => s.clone(),
                                                    _ => "".to_string(), // Skip variables in format strings for now
                                                })
                                                .collect::<Vec<_>>()
                                                .join("");
                                        },
                                        Word::Literal(s, _) => {
                                            format_string = s.clone();
                                        },
                                        _ => {
                                            format_string = generator.word_to_perl(arg);
                                        }
                                    }
                                    // Remove quotes if they exist around the format string
                                    if format_string.starts_with('\'') && format_string.ends_with('\'') {
                                        format_string = format_string[1..format_string.len()-1].to_string();
                                    } else if format_string.starts_with('"') && format_string.ends_with('"') {
                                        format_string = format_string[1..format_string.len()-1].to_string();
                                    }
                                } else {
                                    args.push(generator.word_to_perl(arg));
                                }
                            }
                            
                            if format_string.is_empty() {
                                "\"\"".to_string()
                            } else {
                                if args.is_empty() {
                                    format!("do {{ my $result = sprintf \"{}\"; chomp $result; $result; }}", 
                                        format_string.replace("\"", "\\\"").replace("\\\\", "\\"))
                                } else {
                                    // Properly quote string arguments for sprintf
                                    let formatted_args = args.iter()
                                        .map(|arg| {
                                            // Check if the argument is already quoted
                                            if (arg.starts_with('"') && arg.ends_with('"')) ||
                                               (arg.starts_with('\'') && arg.ends_with('\'')) ||
                                               arg.starts_with("q{") {
                                                arg.clone()
                                            } else {
                                                // Quote unquoted arguments
                                                format!("\"{}\"", arg.replace("\"", "\\\""))
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    format!("do {{ my $result = sprintf \"{}\", {}; chomp $result; $result; }}", 
                                        format_string.replace("\"", "\\\"").replace("\\\\", "\\"),
                                        formatted_args)
                                }
                            }
                        } else if name == "date" {
                            // Special handling for date in command substitution
                            if let Some(format) = simple_cmd.args.first() {
                                let format_str = generator.word_to_perl(format);
                                
                                // Check for special formats that need custom handling
                                if let Word::Literal(format_lit, _) = format {
                                    if format_lit == "+%rms" {
                                        // Special case for +%rms format - 12-hour time with leading zeros
                                        return "do { my ($sec, $min, $hour, $mday, $mon, $year, $wday, $yday, $isdst) = localtime(); my $ampm = $hour >= 12 ? 'PM' : 'AM'; $hour = $hour % 12; $hour = 12 if $hour == 0; sprintf \"%02d:%02d:%02d %sms\", $hour, $min, $sec, $ampm }".to_string();
                                    }
                                }
                                
                                // Strip the + prefix from date format strings (shell date +%Y -> strftime %Y)
                                let cleaned_format = if format_str.starts_with("'+") && format_str.ends_with("'") {
                                    // Remove quotes, strip +, add quotes back
                                    let inner = &format_str[1..format_str.len()-1];
                                    if inner.starts_with('+') {
                                        format!("'{}'", &inner[1..])
                                    } else {
                                        format_str
                                    }
                                } else if format_str.starts_with('+') {
                                    // No quotes, just strip the +
                                    format!("'{}'", &format_str[1..])
                                } else {
                                    // Ensure the format string is properly quoted for strftime
                                    if format_str.starts_with('"') || format_str.starts_with("'") || format_str.starts_with("q{") {
                                        format_str
                                    } else {
                                        format!("'{}'", format_str)
                                    }
                                };
                                format!("do {{ use POSIX qw(strftime); strftime({}, localtime); }}", cleaned_format)
                            } else {
                                "do { use POSIX qw(strftime); strftime('%a, %d %b %Y %H:%M:%S %z', localtime); }".to_string()
                            }
                        } else if name == "pwd" {
                            // Special handling for pwd in command substitution
                            "do { use Cwd; getcwd(); }".to_string()
                        } else if name == "basename" {
                            // Special handling for basename in command substitution
                            if let Some(path) = simple_cmd.args.first() {
                                let path_str = generator.word_to_perl(path);
                                let suffix = if simple_cmd.args.len() > 1 {
                                    generator.word_to_perl(&simple_cmd.args[1])
                                } else {
                                    "q{}".to_string()
                                };
                                format!("do {{ my $basename_path; my $basename_suffix; $basename_path = {}; $basename_suffix = {}; if ($basename_suffix ne q{{}}) {{ $basename_path =~ s/\\Q$basename_suffix\\E$//msx; }} $basename_path =~ s/.*\\///msx; $basename_path; }}", path_str.replace("$0", "$PROGRAM_NAME"), suffix)
                            } else {
                                "\".\"".to_string()
                            }
                        } else if name == "dirname" {
                            // Special handling for dirname in command substitution
                            if let Some(path) = simple_cmd.args.first() {
                                let path_str = generator.word_to_perl(path);
                                format!("do {{ my $path; $path = {}; if ($path =~ /\\//msx) {{ $path =~ s/\\/[^\\/]*$//msx; if ($path eq q{{}}) {{ $path = q{{.}}; }} }} else {{ $path = q{{.}}; }} $path; }}", path_str.replace("$0", "$PROGRAM_NAME"))
                            } else {
                                "\".\"".to_string()
                            }
                        } else if name == "which" {
                            // Special handling for which in command substitution
                            if let Some(command) = simple_cmd.args.first() {
                                let command_str = generator.word_to_perl(command);
                                format!("do {{ my $command; my $found; my $result; my $dir; my $full_path; $command = {}; $found = 0; $result = q{{}}; foreach my $dir (split /:/msx, $ENV{{PATH}}) {{ $full_path = \"$dir/$command\"; if (-x $full_path) {{ $result = $full_path; $found = 1; last; }} }} $result; }}", generator.perl_string_literal(command))
                            } else {
                                "q{}".to_string()
                            }
                        } else if name == "seq" {
                            // Special handling for seq in command substitution
                            if simple_cmd.args.is_empty() {
                                "\"1\"".to_string()
                            } else if simple_cmd.args.len() == 1 {
                                let last_str = generator.word_to_perl(&simple_cmd.args[0]);
                                format!("do {{ my $last; $last = {}; join \"\\n\", 1..$last; }}", last_str)
                            } else if simple_cmd.args.len() == 2 {
                                let first_str = generator.word_to_perl(&simple_cmd.args[0]);
                                let last_str = generator.word_to_perl(&simple_cmd.args[1]);
                                format!("do {{ my $first; my $last; $first = {}; $last = {}; join \"\\n\", $first..$last; }}", first_str, last_str)
                            } else if simple_cmd.args.len() == 3 {
                                let first_str = generator.word_to_perl(&simple_cmd.args[0]);
                                let increment_str = generator.word_to_perl(&simple_cmd.args[1]);
                                let last_str = generator.word_to_perl(&simple_cmd.args[2]);
                                format!("do {{ my $first; my $increment; my $last; my @result; my $i; $first = {}; $increment = {}; $last = {}; for ($i = $first; $i <= $last; $i += $increment) {{ push @result, $i; }} join \"\\n\", @result; }}", first_str, increment_str, last_str)
                            } else {
                                "\"\"".to_string()
                            }
                        } else if name == "perl" {
                            // Special handling for perl in command substitution
                            // For perl -e 'print "..."' commands, capture the output instead of printing
                            if simple_cmd.args.len() >= 2 {
                                if let (Word::Literal(flag, _), Word::Literal(code, _)) = (&simple_cmd.args[0], &simple_cmd.args[1]) {
                                    if flag == "-e" {
                                        // Clean the code by removing outer quotes and fixing escaping
                                        let mut clean_code = code.clone();
                                        if (clean_code.starts_with('"') && clean_code.ends_with('"')) ||
                                           (clean_code.starts_with('\'') && clean_code.ends_with('\'')) {
                                            clean_code = clean_code[1..clean_code.len()-1].to_string();
                                        }
                                        // Fix double-escaped quotes and newlines
                                        clean_code = clean_code.replace("\\\"", "\"").replace("\\\\n", "\\n");
                                        
                                        // For command substitution, capture output instead of printing
                                        // Build the code manually to avoid quote escaping issues
                                        let mut result = String::new();
                                        result.push_str("do { use Capture::Tiny qw(capture_stdout); capture_stdout(sub { ");
                                        result.push_str(&clean_code);
                                        result.push_str("; }); }");
                                        result
                                    } else {
                                        // Fallback to system command for other perl flags
                                        let args: Vec<String> = simple_cmd.args.iter()
                                            .map(|arg| generator.word_to_perl(arg))
                                            .collect();
                                        let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                                        let formatted_args = args.iter().map(|arg| {
                                            let word = Word::Literal(arg.clone(), Default::default());
                                            generator.perl_string_literal(&word)
                                        }).collect::<Vec<_>>().join(", ");
                                        format!(" my ({}, {}, {});\n    my {} = open3({}, {}, {}, 'perl', {});\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                                    }
                                } else {
                                    // Fallback to system command for non-literal args
                                    let args: Vec<String> = simple_cmd.args.iter()
                                        .map(|arg| generator.word_to_perl(arg))
                                        .collect();
                                    let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                                    let formatted_args = args.iter().map(|arg| {
                                        let word = Word::Literal(arg.clone(), Default::default());
                                        generator.perl_string_literal(&word)
                                    }).collect::<Vec<_>>().join(", ");
                                    format!(" my ({}, {}, {});\n    my {} = open3({}, {}, {}, 'perl', {});\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                                }
                            } else {
                                // No arguments, fallback to system command
                                let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                                format!(" my ({}, {}, {});\n    my {} = open3({}, {}, {}, 'perl');\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        } else if generator.inline_mode && name == "echo" {
                            // In inline mode for echo, generate the output value directly
                            if simple_cmd.args.is_empty() {
                                "\"\\n\"".to_string()
                            } else {
                                let args: Vec<String> = simple_cmd.args.iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect();
                                format!("({}) . \"\\n\"", args.join(" . q{ } . "))
                            }
                        } else if name == "cp" {
                            // Use native Perl cp implementation for command substitution
                            eprintln!("DEBUG: words.rs - Using native cp implementation for command substitution");
                            let cp_code = crate::generator::commands::cp::generate_cp_command(generator, simple_cmd);
                            let formatted_code = cp_code.trim_end_matches('\n')
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = generator.indent();
                            let indent2 = format!("{}{}", generator.indent(), "    ");
                            let _indent3 = format!("{}{}", indent2, "    ");
                            format!("do {{\n{}    local $CHILD_ERROR = 0;\n{}    my $eval_result = eval {{\n{}{};\n{}        local $CHILD_ERROR = 0;\n{}        1;\n{}    }};\n{}    if (!$eval_result) {{\n{}        local $CHILD_ERROR = 256;\n{}    }};\n{}    q{{}};\n}}", 
                                indent1, indent1, indent2, formatted_code, indent2, indent2, 
                                indent1, indent1, indent1, indent1, indent1)
                        } else if name == "mv" {
                            // Use native Perl mv implementation for command substitution
                            eprintln!("DEBUG: words.rs - Using native mv implementation for command substitution");
                            let mv_code = crate::generator::commands::mv::generate_mv_command(generator, simple_cmd);
                            let formatted_code = mv_code.trim_end_matches('\n')
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = generator.indent();
                            let indent2 = format!("{}{}", generator.indent(), "    ");
                            let _indent3 = format!("{}{}", indent2, "    ");
                            format!("do {{\n{}    local $CHILD_ERROR = 0;\n{}    my $eval_result = eval {{\n{}{};\n{}        local $CHILD_ERROR = 0;\n{}        1;\n{}    }};\n{}    if (!$eval_result) {{\n{}        local $CHILD_ERROR = 256;\n{}    }};\n{}    q{{}};\n}}", 
                                indent1, indent1, indent2, formatted_code, indent2, indent2, 
                                indent1, indent1, indent1, indent1, indent1)
                        } else if name == "rm" {
                            // Use native Perl rm implementation for command substitution
                            eprintln!("DEBUG: words.rs - Using native rm implementation for command substitution");
                            let rm_code = crate::generator::commands::rm::generate_rm_command(generator, simple_cmd);
                            let formatted_code = rm_code.trim_end_matches('\n')
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = generator.indent();
                            let indent2 = format!("{}{}", generator.indent(), "    ");
                            let _indent3 = format!("{}{}", indent2, "    ");
                            format!("do {{\n{}    local $CHILD_ERROR = 0;\n{}    my $eval_result = eval {{\n{}{};\n{}        local $CHILD_ERROR = 0;\n{}        1;\n{}    }};\n{}    if (!$eval_result) {{\n{}        local $CHILD_ERROR = 256;\n{}    }};\n{}    q{{}};\n}}", 
                                indent1, indent1, indent2, formatted_code, indent2, indent2, 
                                indent1, indent1, indent1, indent1, indent1)
                        } else if name == "mkdir" {
                            // Use native Perl mkdir implementation for command substitution
                            eprintln!("DEBUG: words.rs - Using native mkdir implementation for command substitution");
                            let mkdir_code = crate::generator::commands::mkdir::generate_mkdir_command(generator, simple_cmd);
                            let formatted_code = mkdir_code.trim_end_matches('\n')
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = generator.indent();
                            let indent2 = format!("{}{}", generator.indent(), "    ");
                            // Don't use eval wrapper for mkdir - let it set $CHILD_ERROR directly
                            let indented_code = format!("{}{}", indent2, formatted_code.replace("\n", &format!("\n{}", indent2)));
                            format!("do {{\n{}{};\n{}    q{{}};\n}}", 
                                indent1, indented_code, indent1)
                        } else if name == "touch" {
                            // Use native Perl touch implementation for command substitution
                            eprintln!("DEBUG: words.rs - Using native touch implementation for command substitution");
                            let touch_code = crate::generator::commands::touch::generate_touch_command(generator, simple_cmd);
                            let formatted_code = touch_code.trim_end_matches('\n')
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = generator.indent();
                            let indent2 = format!("{}{}", generator.indent(), "    ");
                            let _indent3 = format!("{}{}", indent2, "    ");
                            format!("do {{\n{}    local $CHILD_ERROR = 0;\n{}    my $eval_result = eval {{\n{}{};\n{}        local $CHILD_ERROR = 0;\n{}        1;\n{}    }};\n{}    if (!$eval_result) {{\n{}        local $CHILD_ERROR = 256;\n{}    }};\n{}    q{{}};\n}}", 
                                indent1, indent1, indent2, formatted_code, indent2, indent2, 
                                indent1, indent1, indent1, indent1, indent1)
                        } else if name == "time" {
                            // Special handling for time in command substitution
                            // Use custom time implementation instead of open3
                            let mut time_output = String::new();
                            time_output.push_str("use Time::HiRes qw(gettimeofday tv_interval);\n");
                            time_output.push_str("my $start_time = [gettimeofday];\n");
                            
                            // Execute the command (if any arguments provided)
                            if !simple_cmd.args.is_empty() {
                                let args: Vec<String> = simple_cmd.args.iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect();
                                let command_str = args.join(" ");
                                // Properly escape quotes in the command string
                                let escaped_command = command_str.replace("\"", "\\\"");
                                time_output.push_str(&format!("system \"{}\";\n", escaped_command));
                            }
                            
                            time_output.push_str("my $end_time = [gettimeofday];\n");
                            time_output.push_str("my $elapsed = tv_interval($start_time, $end_time);\n");
                            time_output.push_str("my $time_output = sprintf \"real\\t0m%.3fs\\nuser\\t0m0.000s\\nsys\\t0m0.000s\\n\", $elapsed;\n");
                            time_output.push_str("print STDERR $time_output;\n");
                            time_output.push_str("q{};\n");
                            
                            format!("do {{ {} }}", time_output)
                        } else {
                            // Fall back to system command for non-builtin commands
                            let args: Vec<String> = simple_cmd.args.iter()
                                .map(|arg| generator.word_to_perl(arg))
                                .collect();
                            
                            let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                            if args.is_empty() {
                                format!(" my ({}, {}, {});\n    my {} = open3({}, {}, {}, '{}');\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
                            } else {
                                let formatted_args = args.iter().map(|arg| {
                                    let word = Word::Literal(arg.clone(), Default::default());
                                    generator.perl_string_literal(&word)
                                }).collect::<Vec<_>>().join(", ");
                                format!(" my ({}, {}, {});\n    my {} = open3({}, {}, {}, '{}', {});\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        }
                    } else {
                        // Fall back to system command for non-literal command names
                        let args: Vec<String> = simple_cmd.args.iter()
                            .map(|arg| generator.word_to_perl(arg))
                            .collect();
                        
                        let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                        if args.is_empty() {
                            format!(" my ({}, {}, {});\n    my {} = open3({}, {}, {}, '{}');\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
                        } else {
                            let formatted_args = args.iter().map(|arg| {
                                let word = Word::Literal(arg.clone(), Default::default());
                                generator.perl_string_literal(&word)
                            }).collect::<Vec<_>>().join(", ");
                            format!(" my ({}, {}, {});\n    my {} = open3({}, {}, {}, '{}', {});\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                        }
                    }
                },
                Command::Pipeline(pipeline) => {
                    // For command substitution pipelines, use the specialized function
                    crate::generator::commands::pipeline_commands::generate_pipeline_for_substitution(generator, pipeline)
                },
                Command::And(left_cmd, right_cmd) => {
                    // Handle And commands in command substitution
                    // Execute left command, if it succeeds (exit code 0), execute right command
                    // Return the combined output from both commands
                    let unique_id = generator.get_unique_id();
                    let left_result = word_to_perl_impl(generator, &Word::CommandSubstitution(left_cmd.clone(), Default::default()));
                    let right_result = word_to_perl_impl(generator, &Word::CommandSubstitution(right_cmd.clone(), Default::default()));
                    
                    // Generate code that executes left command, checks exit code, then executes right if successful
                    // The result is the concatenation of outputs from both commands (if both succeed)
                    // If left command fails, return empty string (shell behavior)
                    format!("do {{\n    my $left_result_{} = {};\n    if ($CHILD_ERROR == 0) {{\n        my $right_result_{} = {};\n        $left_result_{} . $right_result_{};\n    }} else {{\n        q{{}};\n    }}\n}}", 
                        unique_id, left_result, unique_id, right_result, unique_id, unique_id)
                },
                _ => {
                    // For other command types, use system command fallback
                    let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                    format!("do {{\n    my ({}, {}, {});\n    my {} = open3({}, {}, {}, 'bash', '-c', 'echo ERROR: Command substitution not implemented');\n    close {} or croak 'Close failed: $!';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $!';\n    waitpid {}, 0;\n    {};\n}}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, in_var, result_var, out_var, out_var, pid_var, result_var)
                }
            };
            // For simple expressions, avoid unnecessary wrapping
            if result.contains("use POSIX qw(strftime)") || 
               result.contains("use Cwd; getcwd()") ||
               result.starts_with("do { my $") ||
               result.contains("chomp $result") ||
               result.len() < 100 {
                // Simple expressions don't need wrapping
                result
            } else {
                // Check if this is a pipeline result that already returns a value directly
                if result.contains("$output_0") && result.contains("for (my $i = 0") {
                    // This is a pipeline that returns its result directly, so just wrap it in do block
                    format!("do {{\n{}\n{}}}", result, generator.indent())
                } else {
                    // Wrap complex results with chomp to strip trailing newlines (bash behavior)
                    let unique_id = generator.get_unique_id();
                    format!("do {{\n{}    my $cmd_result_{} = {};\n{}    chomp $cmd_result_{};\n{}    $cmd_result_{};\n{}}}", 
                        generator.indent(), unique_id, result, generator.indent(), unique_id, generator.indent(), unique_id, generator.indent())
                }
            }
        },
        Word::Variable(var, _, _) => {
            // Handle special shell variables
            match var.as_str() {
                "#" => "scalar(@ARGV)".to_string(),  // $# -> scalar(@ARGV) for argument count
                "@" => "@ARGV".to_string(),          // $@ -> @ARGV for arguments array
                "*" => "@ARGV".to_string(),          // $* -> @ARGV for arguments array
                "0" => "$PROGRAM_NAME".to_string(),  // $0 -> $PROGRAM_NAME (Perl::Critic compliant)
                _ => format!("${}", var)             // Regular variable
            }
        },
        Word::MapAccess(map_name, key, _) => {
            // Handle array/map access like arr[1] or map[foo]
            // Check if the key is numeric (indexed array) or string (associative array)
            if key.parse::<usize>().is_ok() {
                // Indexed array access: arr[1] -> $arr[1]
                format!("${}[{}]", map_name, key)
            } else {
                // Associative array access: map[foo] -> $map{foo}
                format!("${}{{{}}}", map_name, key)
            }
        },
        Word::MapKeys(map_name, _) => {
            // Handle map keys like !map[@] -> keys %map
            format!("keys %{}", map_name)
        },
        Word::MapLength(map_name, _) => {
            // Handle array length like #arr[@] -> scalar(@arr)
            format!("scalar(@{})", map_name)
        },
        Word::ArraySlice(array_name, offset, length, _) => {
            // Handle array slicing like arr[@]:1:3 -> @arr[1..3]
            if let Some(length_str) = length {
                format!("@{}[{}..{}]", array_name, offset, length_str)
            } else {
                format!("@{}[{}..]", array_name, offset)
            }
        }
    }
}

pub fn word_to_perl_for_test_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => generator.perl_string_literal(word),
        Word::ParameterExpansion(pe, _) => generator.generate_parameter_expansion(pe),
        _ => format!("{:?}", word)
    }
}

// Helper methods
pub fn handle_range_expansion_impl(_generator: &Generator, s: &str) -> String {
    let parts: Vec<&str> = s.split("..").collect();
    if parts.len() == 2 {
        if let (Ok(start), Ok(end)) = (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
            let values: Vec<String> = (start..=end)
                .map(|i| i.to_string())
                .collect();
            // Format as Perl array: (1, 2, 3, 4, 5)
            format!("({})", values.join(", "))
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    }
}

pub fn handle_comma_expansion_impl(_generator: &Generator, s: &str) -> String {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() > 1 {
        parts.join(" ")
    } else {
        s.to_string()
    }
}

pub fn handle_brace_expansion_impl(generator: &mut Generator, expansion: &BraceExpansion) -> String {
    // Handle prefix and suffix
    let prefix = expansion.prefix.as_deref().unwrap_or("");
    let suffix = expansion.suffix.as_deref().unwrap_or("");
    
    if expansion.items.len() == 1 {
        let expanded = generator.word_to_perl(&generator.brace_item_to_word(&expansion.items[0]));
        if !prefix.is_empty() || !suffix.is_empty() {
            // Split the expanded items and add prefix/suffix to each
            let items: Vec<String> = expanded.split_whitespace()
                .map(|item| format!("{}{}{}", prefix, item, suffix))
                .collect();
            items.join(" ")
        } else {
            expanded
        }
    } else {
        // Handle cartesian product for multiple brace items
        let expanded_items: Vec<Vec<String>> = expansion.items.iter()
            .map(|item| {
                let word = generator.brace_item_to_word(item);
                match word {
                    Word::Literal(s, _) => vec![s],
                    _ => vec![generator.word_to_perl(&word)],
                }
            })
            .collect();
        
        // Generate cartesian product
        let cartesian = generate_cartesian_product(&expanded_items);
        
        // Add prefix and suffix to each item
        let items: Vec<String> = cartesian.iter()
            .map(|item| format!("{}{}{}", prefix, item, suffix))
            .collect();
        
        // Join all combinations with spaces
        items.join(" ")
    }
}

fn generate_cartesian_product(items: &[Vec<String>]) -> Vec<String> {
    if items.is_empty() {
        return vec![];
    }
    if items.len() == 1 {
        return items[0].clone();
    }
    
    let mut result = Vec::new();
    let first = &items[0];
    let rest = generate_cartesian_product(&items[1..]);
    
    for item in first {
        for rest_item in &rest {
            result.push(format!("{}{}", item, rest_item));
        }
    }
    
    result
}

pub fn brace_item_to_word_impl(_generator: &Generator, item: &BraceItem) -> Word {
    match item {
        BraceItem::Literal(s) => Word::literal(s.clone()),
        BraceItem::Range(range) => {
            // Expand the range to actual values
            let expanded = expand_range(range);
            Word::literal(expanded)
        },
        BraceItem::Sequence(seq) => Word::literal(seq.join(" ")),
    }
}

fn expand_range(range: &BraceRange) -> String {
    // Check if this is a numeric range
    if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
        let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
        
        let mut values = Vec::new();
        let mut current = start_num;
        
        if step > 0 {
            while current <= end_num {
                // Preserve leading zeros by formatting with the same width as the original
                let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                    format!("{:0width$}", current, width = range.start.len())
                } else {
                    current.to_string()
                };
                values.push(formatted);
                current += step;
            }
        } else {
            while current >= end_num {
                // Preserve leading zeros by formatting with the same width as the original
                let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                    format!("{:0width$}", current, width = range.start.len())
                } else {
                    current.to_string()
                };
                values.push(formatted);
                current += step;
            }
        }
        
        values.join(" ")
    } else {
        // Character range (e.g., a..c)
        if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
            let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
            
            let mut values = Vec::new();
            let mut current = start_char as i64;
            let end = end_char as i64;
            
            if step > 0 {
                while current <= end {
                    values.push((current as u8 as char).to_string());
                    current += step;
                }
            } else {
                while current >= end {
                    values.push((current as u8 as char).to_string());
                    current += step;
                }
            }
            
            values.join(" ")
        } else {
            // Fallback: just return the range as-is
            format!("{}..{}", range.start, range.end)
        }
    }
}

pub fn convert_string_interpolation_to_perl_impl(generator: &mut Generator, interp: &StringInterpolation) -> String {
    // Convert string interpolation to Perl concatenation when command substitutions are present
    let mut parts = Vec::new();
    let mut current_string = String::new();
    
    for part in &interp.parts {
        match part {
            StringPart::Literal(s) => {
                // Accumulate literal parts into the current string
                current_string.push_str(s);
            },
            StringPart::Variable(var) => {
                // Handle special shell variables
                match var.as_str() {
                    "#" => current_string.push_str("${scalar(@ARGV)}"),  // $# -> ${scalar(@ARGV)} for interpolation
                    "@" => current_string.push_str("@ARGV"),             // Arrays don't need $ in interpolation
                    "*" => current_string.push_str("@ARGV"),             // Arrays don't need $ in interpolation
                    _ => {
                        // Check if this is a shell positional parameter ($1, $2, etc.)
                        if var.chars().all(|c| c.is_digit(10)) {
                            // Convert $1 to $_[0], $2 to $_[1], etc.
                            let index = var.parse::<usize>().unwrap_or(0);
                            current_string.push_str(&format!("$_[{}]", index - 1)); // Perl arrays are 0-indexed
                        } else {
                            // Regular variable - add directly for interpolation
                            current_string.push_str(&format!("${}", var));
                        }
                    }
                }
            },
            StringPart::MapAccess(map_name, key) => {
                if map_name == "map" {
                    current_string.push_str(&format!("$map{{{}}}", key));
                } else {
                    current_string.push_str(&format!("${}{{{}}}", map_name, key));
                }
            }
            StringPart::CommandSubstitution(cmd) => {
                // Command substitutions require concatenation, not interpolation
                // First, add any accumulated string as a quoted part
                if !current_string.is_empty() {
                    parts.push(format!("\"{}\"", current_string.replace("\\", "\\\\").replace("\"", "\\\"")));
                    current_string.clear();
                }
                // Add the command substitution as a separate part
                let cmd_result = generator.word_to_perl(&Word::CommandSubstitution(cmd.clone(), None));
                parts.push(format!("({})", cmd_result));
            },
            StringPart::ParameterExpansion(pe) => {
                // Handle parameter expansions like ${arr[1]}, ${#arr[@]}, etc.
                // We need to convert the ParameterExpansion to Perl code
                // For now, let's handle the common cases directly
                
                // Check for special array operations first
                match &pe.operator {
                    ParameterExpansionOperator::ArraySlice(offset, length) => {
                        if offset == "@" {
                            // This is ${#arr[@]} or ${arr[@]} - array length or array iteration
                            if pe.variable.starts_with('#') {
                                // ${#arr[@]} -> scalar(@arr)
                                let array_name = &pe.variable[1..];
                                    current_string.push_str(&format!("${{scalar(@{})}}", array_name));
                            } else if pe.variable.starts_with('!') {
                                // ${!map[@]} -> keys %map (map keys iteration)
                                let map_name = &pe.variable[1..]; // Remove ! prefix
                                current_string.push_str(&format!("keys %{}", map_name));
                            } else {
                                // ${arr[@]} -> @arr (for array iteration)
                                let array_name = &pe.variable;
                                current_string.push_str(&format!("@{}", array_name));
                            }
                        } else {
                            // Regular array slice
                            if let Some(length_str) = length {
                                current_string.push_str(&format!("@${{{}}}[{}..{}]", pe.variable, offset, length_str));
                            } else {
                                current_string.push_str(&format!("@${{{}}}[{}..]", pe.variable, offset));
                            }
                        }
                    }
                    _ => {
                        // Handle other cases
                        if pe.variable.contains('[') && pe.variable.contains(']') {
                            if let Some(bracket_start) = pe.variable.find('[') {
                                if let Some(bracket_end) = pe.variable.rfind(']') {
                                    let var_name = &pe.variable[..bracket_start];
                                    let key = &pe.variable[bracket_start + 1..bracket_end];
                                    
                                    // Check if the key is numeric (indexed array) or string (associative array)
                                    if key.parse::<usize>().is_ok() {
                                        // Indexed array access: arr[1] -> $arr[1]
                                        current_string.push_str(&format!("${}[{}]", var_name, key));
                                    } else {
                                        // Associative array access: map[foo] -> $map{foo}
                                        current_string.push_str(&format!("${}{{{}}}", var_name, key));
                                    }
                                } else {
                                    current_string.push_str(&format!("${{{}}}", pe.variable));
                                }
                            } else {
                                current_string.push_str(&format!("${{{}}}", pe.variable));
                            }
                        } else {
                            // Simple variable reference - use the proper parameter expansion generation
                            current_string.push_str(&generator.generate_parameter_expansion(pe));
                        }
                    }
                }
            }
            _ => {
                // Handle other StringPart variants by converting them to debug format for now
                current_string.push_str(&format!("{:?}", part));
            }
        }
    }
    
    // Add any remaining string content
    if !current_string.is_empty() {
        parts.push(format!("\"{}\"", current_string.replace("\\", "\\\\").replace("\"", "\\\"")));
    }
    
    // Return the result
    if parts.is_empty() {
        // No parts, return empty string
        "\"\"".to_string()
    } else if parts.len() == 1 {
        // Single part, return it directly
        parts.into_iter().next().unwrap()
    } else {
        // Multiple parts, concatenate them
        parts.join(" . ")
    }
}

pub fn convert_arithmetic_to_perl_impl(_generator: &Generator, expr: &str) -> String {
    // Convert shell arithmetic expression to Perl syntax
    let result = expr.to_string();
    
    // Convert shell variables to Perl variables (e.g., i -> $i) first
    // Use regex to find variable names and replace them with Perl variable syntax
    
    // Create a regex to match variable names (letters followed by alphanumeric/underscore)
    let var_regex = Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\b").unwrap();
    
    // Replace variable names with Perl variable syntax
    let converted = var_regex.replace_all(&result, |caps: &regex::Captures| {
        let var_name = &caps[1];
        format!("${}", var_name)
    });
    
    converted.to_string()
}
