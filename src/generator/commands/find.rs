use crate::ast::*;
use crate::generator::Generator;

fn escape_glob_pattern(pattern: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    
    for (i, c) in chars.iter().enumerate() {
        match c {
            '*' => {
                if i == 0 {
                    // At start of pattern, * means "any characters"
                    result.push_str(".*");
                } else {
                    // In middle/end, * means "any characters"
                    result.push_str(".*");
                }
            },
            '?' => result.push_str("."),
            '.' => result.push_str("[.]"),
            '[' => result.push_str("\\["),
            ']' => result.push_str("\\]"),
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '+' => result.push_str("\\+"),
            '^' => result.push_str("\\^"),
            '$' => result.push_str("\\$"),
            '|' => result.push_str("\\|"),
            '{' => result.push_str("\\{"),
            '}' => result.push_str("\\}"),
            '/' => result.push_str("\\/"),
            _ => result.push(*c)
        }
    }
    
    // Add end anchor for proper matching
    result.push('$');
    result
}

pub fn generate_find_command(generator: &mut Generator, cmd: &SimpleCommand, generate_output: bool, input_var: &str) -> String {
    let mut output = String::new();
    
    // Check if -ls is present - if so, use system fallback for better compatibility
    let has_ls = cmd.args.iter().any(|arg| {
        if let Word::Literal(s, _) = arg {
            s == "-ls"
        } else {
            false
        }
    });
    
    if has_ls {
        return generate_system_find_fallback(generator, cmd, generate_output, input_var);
    }
    
    // For command substitution, use a simpler approach that doesn't define subroutines
    eprintln!("DEBUG: generate_find_command called with generate_output: {}, input_var: '{}'", generate_output, input_var);
    if generate_output && input_var != "" {
        eprintln!("DEBUG: Using generate_find_for_substitution with input_var: '{}'", input_var);
        return generate_find_for_substitution(generator, cmd, input_var);
    }
    eprintln!("DEBUG: Using complex find generation instead");
    
    // Generate a unique subroutine name
    let subroutine_id = generator.get_unique_id();
    let subroutine_name = format!("find_files_{}", subroutine_id);
    
    // Parse find arguments to understand what we're looking for
    let mut _start_path = ".".to_string();
    let mut name_pattern = None;
    let mut file_type = None;
    let mut mtime_days = None;
    let mut mmin_minutes = None;
    let mut size_spec = None;
    let mut empty_only = false;
    let mut exec_command = None;
    let mut ls_format = false;
    let mut not_paths = Vec::new();
    
    // First, reconstruct split arguments (e.g., "-s" + "ize" = "-size")
    let mut reconstructed_args = Vec::new();
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(s, _) = &cmd.args[i] {
            // Check if this is a split argument that needs reconstruction
            if (s == "-s" || s == "-e" || s == "-p" || s == "-n") && i + 1 < cmd.args.len() {
                if let Word::Literal(next_s, _) = &cmd.args[i + 1] {
                    // Reconstruct common find arguments
                    match (s.as_str(), next_s.as_str()) {
                        ("-s", "ize") => {
                            reconstructed_args.push(Word::Literal("-size".to_string(), None));
                            i += 2;
                            continue;
                        },
                        ("-e", "mpty") => {
                            reconstructed_args.push(Word::Literal("-empty".to_string(), None));
                            i += 2;
                            continue;
                        },
                        ("-p", "ath") => {
                            reconstructed_args.push(Word::Literal("-path".to_string(), None));
                            i += 2;
                            continue;
                        },
                        ("-n", "ot") => {
                            reconstructed_args.push(Word::Literal("-not".to_string(), None));
                            i += 2;
                            continue;
                        },
                        _ => {}
                    }
                }
            }
        }
        reconstructed_args.push(cmd.args[i].clone());
        i += 1;
    }
    
    // Now parse the reconstructed arguments
    let mut i = 0;
    while i < reconstructed_args.len() {
        match &reconstructed_args[i] {
            Word::Literal(s, _) => {
                match s.as_str() {
                    "-name" => {
                        if i + 1 < reconstructed_args.len() {
                            match &reconstructed_args[i + 1] {
                                Word::StringInterpolation(interp, _) => {
                                    let pattern = interp.parts.iter()
                                        .map(|part| match part {
                                            StringPart::Literal(s) => s.clone(),
                                            _ => "*".to_string(),
                                        })
                                        .collect::<String>();
                                    name_pattern = Some(pattern);
                                },
                                Word::Literal(pattern, _) => {
                                    name_pattern = Some(pattern.clone());
                                },
                                _ => {}
                            }
                            i += 1;
                        }
                    },
                    "-type" => {
                        if i + 1 < reconstructed_args.len() {
                            if let Word::Literal(type_str, _) = &reconstructed_args[i + 1] {
                                file_type = Some(type_str.clone());
                            }
                            i += 1;
                        }
                    },
                    "-mtime" => {
                        if i + 1 < reconstructed_args.len() {
                            if let Word::Literal(time_str, _) = &reconstructed_args[i + 1] {
                                mtime_days = Some(time_str.clone());
                            }
                            i += 1;
                        }
                    },
                    "-mmin" => {
                        if i + 1 < reconstructed_args.len() {
                            if let Word::Literal(min_str, _) = &reconstructed_args[i + 1] {
                                mmin_minutes = Some(min_str.clone());
                            }
                            i += 1;
                        }
                    },
                    "-size" => {
                        if i + 1 < reconstructed_args.len() {
                            if let Word::Literal(size_str, _) = &reconstructed_args[i + 1] {
                                size_spec = Some(size_str.clone());
                            }
                            i += 1;
                        }
                    },
                    "-empty" => {
                        empty_only = true;
                    },
                    "-exec" => {
                        // Collect exec command arguments until semicolon
                        let mut exec_args = Vec::new();
                        i += 1;
                        while i < reconstructed_args.len() {
                            if let Word::Literal(exec_arg, _) = &reconstructed_args[i] {
                                // Check for various semicolon terminators
                                if exec_arg == ";" || exec_arg == "\\;" {
                                    break;
                                }
                                // Also check for backslash followed by semicolon as separate tokens
                                if exec_arg == "\\" && i + 1 < reconstructed_args.len() {
                                    if let Word::Literal(next_arg, _) = &reconstructed_args[i + 1] {
                                        if next_arg == ";" {
                                            i += 1; // Skip the semicolon
                                            break;
                                        }
                                    }
                                }
                                // Check for end of arguments (no semicolon found)
                                if exec_arg == "\\" && i + 1 >= reconstructed_args.len() {
                                    // Assume this is the end of exec command (missing semicolon)
                                    break;
                                }
                                exec_args.push(exec_arg.clone());
                            } else if let Word::BraceExpansion(_be, _) = &reconstructed_args[i] {
                                // Handle {} placeholder - even if empty
                                exec_args.push("{}".to_string());
                            } else {
                                exec_args.push(generator.word_to_perl(&reconstructed_args[i]));
                            }
                            i += 1;
                        }
                        if !exec_args.is_empty() {
                            exec_command = Some(exec_args);
                        }
                    },
                    "-ls" => {
                        ls_format = true;
                    },
                    "-not" => {
                        if i + 1 < reconstructed_args.len() && i + 2 < reconstructed_args.len() {
                            if let Word::Literal(not_arg, _) = &reconstructed_args[i + 1] {
                                if not_arg == "-path" {
                                    if let Word::StringInterpolation(interp, _) = &reconstructed_args[i + 2] {
                                        let path_pattern = interp.parts.iter()
                                .map(|part| match part {
                                                StringPart::Literal(s) => s.clone(),
                                                _ => "*".to_string(),
                                            })
                                            .collect::<String>();
                                        not_paths.push(path_pattern);
                                    }
                                    i += 2;
                                }
                            }
                        }
                    },
                    _ => {
                        // This might be the starting path
                        if i == 0 {
                            _start_path = s.clone();
                        }
                    }
                }
            },
            Word::StringInterpolation(interp, _) => {
                // This might be the starting path
                if i == 0 {
                    let path = interp.parts.iter()
                        .map(|part| match part {
                            StringPart::Literal(s) => s.clone(),
                            StringPart::Variable(var) => format!("$ENV{{{}}}", var),
                            _ => ".".to_string(),
                        })
                        .collect::<String>();
                    _start_path = path;
                }
            },
            _ => {}
        }
        i += 1;
    }
    
    // Generate native Perl code for find functionality
    let base_indent = generator.indent();
    let indent1 = format!("{}    ", base_indent);
    let indent2 = format!("{}        ", base_indent);
    let indent3 = format!("{}            ", base_indent);
    let indent4 = format!("{}                ", base_indent);
    let indent5 = format!("{}                    ", base_indent);
    
    output.push_str(&base_indent);
    output.push_str("{\n");
    
    // Generate recursive directory traversal
    output.push_str(&indent1);
    output.push_str(&format!("sub {} {{\n", subroutine_name));
    
    output.push_str(&indent2);
    output.push_str("my ($dir, $results) = @_;\n");
    
    output.push_str(&indent2);
    output.push_str("if (opendir my $dh, $dir) {\n");
    
    output.push_str(&indent3);
    output.push_str("while (my $file = readdir $dh) {\n");
    
    output.push_str(&indent4);
    output.push_str("next if $file eq q{.} || $file eq q{..};\n");
    
    output.push_str(&indent4);
    output.push_str("my $full_path = \"$dir/$file\";\n");
    
    // Recursive call for directories (do this first, before filtering)
    output.push_str(&indent4);
    output.push_str("if (-d $full_path) {\n");
    output.push_str(&format!("{}    ", indent4));
    output.push_str(&format!("{}($full_path, $results);\n", subroutine_name));
    output.push_str(&indent4);
    output.push_str("}\n");
    
    // Add file type check
    if let Some(ftype) = &file_type {
        match ftype.as_str() {
            "f" => {
                output.push_str(&indent4);
                output.push_str("if (!(-f $full_path)) {\n");
                output.push_str(&format!("{}    next;\n", indent4));
                output.push_str(&format!("{}}}\n", indent4));
            },
            "d" => {
                output.push_str(&indent4);
                output.push_str("if (!(-d $full_path)) {\n");
                output.push_str(&format!("{}    next;\n", indent4));
                output.push_str(&format!("{}}}\n", indent4));
            },
            _ => {}
        }
    }
    
    // Add name pattern check
    if let Some(pattern) = &name_pattern {
        output.push_str(&indent4);
        output.push_str(&format!("if (!($filename =~ {})) {{\n", generator.format_regex_pattern(&escape_glob_pattern(pattern))));
        output.push_str(&indent4);
        output.push_str("    next;\n");
        output.push_str(&indent4);
        output.push_str("}\n");
    }
    
    // Add empty check
    if empty_only {
        output.push_str(&indent4);
        output.push_str("if (-f $full_path) {\n");
        output.push_str(&indent5);
        output.push_str("if (!(-z $full_path)) {\n");
        output.push_str(&format!("{}    next;\n", indent5));
        output.push_str(&format!("{}}}\n", indent5));
        output.push_str(&indent4);
        output.push_str("} elsif (-d $full_path) {\n");
        output.push_str(&indent5);
        output.push_str("opendir my $empty_dh, $full_path or next;\n");
        output.push_str(&indent5);
        output.push_str("my @entries = grep { $_ ne q{.} && $_ ne q{..} } readdir $empty_dh;\n");
        output.push_str(&indent5);
        output.push_str("closedir $empty_dh;\n");
        output.push_str(&indent5);
        output.push_str("if (!(@entries == 0)) {\n");
        output.push_str(&format!("{}    next;\n", indent5));
        output.push_str(&format!("{}}}\n", indent5));
        output.push_str(&indent4);
        output.push_str("} else {\n");
        output.push_str(&indent5);
        output.push_str("next;\n");
        output.push_str(&indent4);
        output.push_str("}\n");
    }
    
    // Add mtime check
    if let Some(mtime) = &mtime_days {
        output.push_str(&indent4);
        if mtime.starts_with('-') {
            // Negative mtime means "less than N days old"
            let days = &mtime[1..];
            output.push_str(&format!("next if !((0 + -M $full_path) < {});\n", days));
        } else if mtime.starts_with('+') {
            // Positive mtime means "more than N days old"
            let days = &mtime[1..];
            output.push_str(&format!("next if !((0 + -M $full_path) > {});\n", days));
        } else {
            // Exact mtime means "exactly N days old"
            output.push_str(&format!("next if !((0 + -M $full_path) == {});\n", mtime));
        }
    }
    
    // Add mmin check
    if let Some(mmin) = &mmin_minutes {
        output.push_str(&indent4);
        if mmin.starts_with('-') {
            // Negative mmin means "less than N minutes old"
            let minutes = &mmin[1..];
            output.push_str(&format!("next if !((0 + -M $full_path) * 24 * 60 < {});\n", minutes));
        } else if mmin.starts_with('+') {
            // Positive mmin means "more than N minutes old"
            let minutes = &mmin[1..];
            output.push_str(&format!("next if !((0 + -M $full_path) * 24 * 60 > {});\n", minutes));
    } else {
            // Exact mmin means "exactly N minutes old"
            output.push_str(&format!("next if !((0 + -M $full_path) * 24 * 60 == {});\n", mmin));
        }
    }
    
    // Add size check
    if let Some(size) = &size_spec {
        if size.starts_with('+') {
            let size_val = &size[1..];
            let size_bytes = parse_size_to_bytes(size_val);
            output.push_str(&indent4);
            output.push_str(&format!("next if !((stat($full_path))[7] > {});\n", size_bytes));
        } else if size.starts_with('-') {
            let size_val = &size[1..];
            let size_bytes = parse_size_to_bytes(size_val);
            output.push_str(&indent4);
            output.push_str(&format!("next if !((stat($full_path))[7] < {});\n", size_bytes));
        }
    }
    
    // Add not path checks
    for not_path in &not_paths {
        output.push_str(&indent4);
        output.push_str(&format!("next if $full_path =~ {};\n", generator.format_regex_pattern(&escape_glob_pattern(not_path))));
    }
    
    // Handle exec command
    if let Some(exec_cmd) = &exec_command {
        output.push_str(&indent4);
        output.push_str("my $exec_cmd = \"");
        for (j, arg) in exec_cmd.iter().enumerate() {
            if j > 0 {
                output.push_str(" ");
            }
            if arg == "{}" {
                output.push_str("\" . $full_path . \"");
            } else {
                output.push_str(arg);
            }
        }
        output.push_str("\";\n");
        output.push_str(&indent4);
        output.push_str("system $exec_cmd;\n");
    } else {
        // Add to results
        if ls_format {
            output.push_str(&indent4);
            output.push_str("my @stat = stat($full_path);\n");
            output.push_str(&indent4);
            output.push_str("my $inode = $stat[1];\n");
            output.push_str(&indent4);
            output.push_str("my $blocks = int(($stat[7] + 511) / 512);\n");
            output.push_str(&indent4);
            output.push_str("my $perms = q{};\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0400) ? 'r' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0200) ? 'w' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0100) ? 'x' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0040) ? 'r' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0020) ? 'w' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0010) ? 'x' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0004) ? 'r' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0002) ? 'w' : '-';\n");
            output.push_str(&indent4);
            output.push_str("$perms .= ($stat[2] & 0001) ? 'x' : '-';\n");
            output.push_str(&indent4);
            output.push_str("my $owner = getpwuid($stat[4]) || $stat[4];\n");
            output.push_str(&indent4);
            output.push_str("my $group = getgrgid($stat[5]) || $stat[5];\n");
            output.push_str(&indent4);
            output.push_str("my $mtime = scalar localtime($stat[9]);\n");
            output.push_str(&indent4);
            output.push_str("push @{$results}, sprintf \"%d %d -%s %d %s %s %d %s %s\", $inode, $blocks, $perms, $stat[3], $owner, $group, $stat[7], $mtime, $full_path;\n");
        } else {
            output.push_str(&indent4);
            output.push_str("push @{$results}, $full_path;\n");
        }
    }
    
    output.push_str(&indent3);
    output.push_str("}\n");
    
    output.push_str(&indent2);
    output.push_str(&generator.indent());
    output.push_str("closedir $dh;\n");
    
    output.push_str(&indent2);
    output.push_str("}\n");
    
    output.push_str(&indent2);
    output.push_str("return;\n");
    
    output.push_str(&indent1);
    output.push_str("}\n");
    
    // Call the function
    output.push_str(&indent1);
    output.push_str("my @find_results;\n");
    output.push_str(&indent1);
    output.push_str(&format!("{}(q{{.}}, \\@find_results);\n", subroutine_name));
    
    if generate_output {
        output.push_str(&indent1);
        output.push_str(&format!("${} = join \"\\n\", @find_results;\n", input_var));
        output.push_str(&indent1);
        output.push_str(&format!("if (!(${} =~ {})) {{\n", input_var, generator.newline_end_regex()));
        output.push_str(&indent1);
        output.push_str(&format!("    ${} .= \"\\n\";\n", input_var));
        output.push_str(&indent1);
        output.push_str("}\n");
    } else {
        output.push_str(&indent1);
        output.push_str("print join \"\\n\", @find_results . \"\\n\";\n");
    }
    
    output.push_str(&base_indent);
    output.push_str("}\n");
    
    output
}

fn generate_system_find_fallback(generator: &mut Generator, cmd: &SimpleCommand, generate_output: bool, input_var: &str) -> String {
    let mut output = String::new();
    
    // Build the find command arguments for open3
    let mut find_args = Vec::new();
    for arg in &cmd.args {
        match arg {
            Word::Literal(s, _) => {
                let word = Word::Literal(s.clone(), Default::default());
                find_args.push(generator.perl_string_literal(&word));
            },
            Word::StringInterpolation(interp, _) => {
                // Use the convert_string_interpolation_to_perl function directly
                find_args.push(generator.convert_string_interpolation_to_perl(interp));
            },
            _ => {
                // For other word types, convert to Perl
                find_args.push(generator.perl_string_literal(arg));
            }
        }
    }
    
    if generate_output {
        // For pipeline context, capture output to variable
        output.push_str(&generator.indent());
        let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
        let formatted_args = find_args.join(", ");
        output.push_str(&format!("my ({});
my {} = open3({}, {}, {}, 'find', {});
close {} or croak 'Close failed: $!';
${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $!';
waitpid {}, 0;\n", in_var, pid_var, in_var, out_var, err_var, formatted_args, in_var, input_var, out_var, out_var, pid_var));
        output.push_str(&generator.indent());
        output.push_str(&format!("chomp ${};\n", input_var));
    } else {
        // For standalone commands, execute directly
        output.push_str(&generator.indent());
        let formatted_args = find_args.join(", ");
        output.push_str(&format!("system 'find', {};\n", formatted_args));
    }
    
    output
}

fn generate_find_for_substitution(generator: &mut Generator, cmd: &SimpleCommand, _input_var: &str) -> String {
    let mut output = String::new();
    
    // Parse find arguments to understand what we're looking for
    let mut _start_path = ".".to_string();
    let mut name_pattern = None;
    let mut file_type = None;
    
    // First, reconstruct split arguments (e.g., "-s" + "ize" = "-size")
    let mut reconstructed_args = Vec::new();
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(s, _) = &cmd.args[i] {
            // Check if this is a split argument that needs reconstruction
            if (s == "-s" || s == "-e" || s == "-p" || s == "-n") && i + 1 < cmd.args.len() {
                if let Word::Literal(next_s, _) = &cmd.args[i + 1] {
                    // Reconstruct common find arguments
                    match (s.as_str(), next_s.as_str()) {
                        ("-s", "ize") => {
                            reconstructed_args.push(Word::Literal("-size".to_string(), None));
                            i += 2;
                            continue;
                        },
                        ("-e", "mpty") => {
                            reconstructed_args.push(Word::Literal("-empty".to_string(), None));
                            i += 2;
                            continue;
                        },
                        ("-p", "ath") => {
                            reconstructed_args.push(Word::Literal("-path".to_string(), None));
                            i += 2;
                            continue;
                        },
                        ("-n", "ot") => {
                            reconstructed_args.push(Word::Literal("-not".to_string(), None));
                            i += 2;
                            continue;
                        },
                        _ => {}
                    }
                }
            }
        }
        reconstructed_args.push(cmd.args[i].clone());
        i += 1;
    }
    
    // Now parse the reconstructed arguments
    let mut i = 0;
    while i < reconstructed_args.len() {
        match &reconstructed_args[i] {
            Word::Literal(s, _) => {
                match s.as_str() {
                    "-name" => {
                        if i + 1 < reconstructed_args.len() {
                            match &reconstructed_args[i + 1] {
                                Word::StringInterpolation(interp, _) => {
                                    let pattern = interp.parts.iter()
                                        .map(|part| match part {
                                            StringPart::Literal(s) => s.clone(),
                                            _ => "*".to_string(),
                                        })
                                        .collect::<String>();
                                    name_pattern = Some(pattern);
                                },
                                Word::Literal(pattern, _) => {
                                    name_pattern = Some(pattern.clone());
                                },
                                _ => {}
                            }
                            i += 1;
                        }
                    },
                    "-type" => {
                        if i + 1 < reconstructed_args.len() {
                            if let Word::Literal(type_str, _) = &reconstructed_args[i + 1] {
                                file_type = Some(type_str.clone());
                            }
                            i += 1;
                        }
                    },
                    _ => {
                        // This might be the starting path
                        if i == 0 {
                            _start_path = s.clone();
                        }
                    }
                }
            },
            Word::StringInterpolation(interp, _) => {
                // This might be the starting path
                if i == 0 {
                    let path = interp.parts.iter()
                        .map(|part| match part {
                            StringPart::Literal(s) => s.clone(),
                            StringPart::Variable(var) => format!("$ENV{{{}}}", var),
                            _ => ".".to_string(),
                        })
                        .collect::<String>();
                    _start_path = path;
                }
            },
            _ => {}
        }
        i += 1;
    }
    
    // Generate simple Perl code for find functionality using glob
    output.push_str("do {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("my @results;\n");
    output.push_str(&generator.indent());
    output.push_str("my $start_path = ");
    output.push_str(&generator.perl_string_literal(&Word::Literal(_start_path, Default::default())));
    output.push_str(";\n");
    
    // Use recursive function for directory traversal (works better on Windows)
    output.push_str(&generator.indent());
    output.push_str("sub find_files {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("my ($dir) = @_;\n");
    output.push_str(&generator.indent());
    output.push_str("if (opendir my $dh, $dir) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("while (my $file = readdir $dh) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("next if $file eq q{.} or $file eq q{..};\n");
    output.push_str(&generator.indent());
    output.push_str("my $full_path = \"$dir/$file\";\n");
    output.push_str(&generator.indent());
    output.push_str("if (-d $full_path) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("find_files($full_path);\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("} else {\n");
    generator.indent_level += 1;
    
    // Add file type check
    if let Some(ftype) = &file_type {
        match ftype.as_str() {
            "f" => {
                output.push_str(&generator.indent());
                output.push_str("if (-f $full_path) {\n");
                generator.indent_level += 1;
            },
            "d" => {
                output.push_str(&generator.indent());
                output.push_str("if (-d $full_path) {\n");
                generator.indent_level += 1;
            },
            _ => {
                output.push_str(&generator.indent());
                output.push_str("{\n");
                generator.indent_level += 1;
            }
        }
    } else {
        output.push_str(&generator.indent());
        output.push_str("{\n");
        generator.indent_level += 1;
    }
    
    // Add name pattern check
    if let Some(pattern) = &name_pattern {
        let escaped_pattern = escape_glob_pattern(pattern);
        output.push_str(&generator.indent());
        output.push_str("if ($file =~ ");
        output.push_str(&generator.format_regex_pattern(&escaped_pattern));
        output.push_str(") {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("push @results, $full_path;\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else {
        output.push_str(&generator.indent());
        output.push_str("push @results, $full_path;\n");
    }
    
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("closedir $dh;\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("return;\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("find_files($start_path);\n");
    
    output.push_str(&generator.indent());
    output.push_str("join \"\\n\", @results;\n");
    output.push_str(&generator.indent());
    output.push_str("}");
    
    output
}

fn parse_size_to_bytes(size_str: &str) -> u64 {
    if size_str.is_empty() {
        return 0;
    }
    
    let (number_part, unit) = if size_str.ends_with('c') {
        (&size_str[..size_str.len()-1], 1) // bytes
    } else if size_str.ends_with('w') {
        (&size_str[..size_str.len()-1], 2) // 2-byte words
    } else if size_str.ends_with('b') {
        (&size_str[..size_str.len()-1], 512) // 512-byte blocks
    } else if size_str.ends_with('k') || size_str.ends_with('K') {
        (&size_str[..size_str.len()-1], 1024) // kilobytes
    } else if size_str.ends_with('M') {
        (&size_str[..size_str.len()-1], 1024 * 1024) // megabytes
    } else if size_str.ends_with('G') {
        (&size_str[..size_str.len()-1], 1024 * 1024 * 1024) // gigabytes
    } else {
        (size_str, 1) // default to bytes
    };
    
    number_part.parse::<u64>().unwrap_or(0) * unit
}


