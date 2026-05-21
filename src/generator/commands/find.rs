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
            }
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
            _ => result.push(*c),
        }
    }

    // Add end anchor for proper matching
    result.push('$');
    result
}

pub fn generate_find_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    generate_output: bool,
    input_var: &str,
) -> String {
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
    eprintln!(
        "DEBUG: generate_find_command called with generate_output: {}, input_var: '{}'",
        generate_output, input_var
    );
    if generate_output && input_var != "" {
        eprintln!(
            "DEBUG: Using generate_find_for_substitution with input_var: '{}'",
            input_var
        );
        return generate_find_for_substitution(generator, cmd, input_var);
    }
    eprintln!("DEBUG: Using complex find generation instead");

    // For now, use the simple substitution for all cases to avoid complexity
    generate_find_for_substitution(generator, cmd, input_var)
}

fn generate_system_find_fallback(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    generate_output: bool,
    input_var: &str,
) -> String {
    let mut output = String::new();

    // Build the find command arguments for open3
    let mut find_args = Vec::new();
    for arg in &cmd.args {
        match arg {
            Word::Literal(s, _) => {
                let word = Word::Literal(s.clone(), Default::default());
                find_args.push(generator.perl_string_literal(&word));
            }
            Word::StringInterpolation(interp, _) => {
                // Use the convert_string_interpolation_to_perl function directly
                find_args.push(generator.convert_string_interpolation_to_perl(interp));
            }
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
        output.push_str(&format!(
            "my ({}, {}, {});
my {} = open3({}, {}, {}, 'find', {});
close {} or croak 'Close failed: $OS_ERROR';
${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $OS_ERROR';
waitpid {}, 0;\n",
            in_var,
            out_var,
            err_var,
            pid_var,
            in_var,
            out_var,
            err_var,
            formatted_args,
            in_var,
            input_var,
            out_var,
            out_var,
            pid_var
        ));
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

pub fn generate_find_for_substitution(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    _input_var: &str,
) -> String {
    // For simple find commands, use a much simpler approach
    let mut start_path = String::from(".");
    let mut name_pattern = None;
    let mut file_type = None;
    let mut max_depth: Option<usize> = None;

    // Simple argument parsing
    let mut i = 0;
    while i < cmd.args.len() {
        // Helper: extract a plain string from either a Literal or a
        // StringInterpolation that contains only a single Literal part.
        let plain_str: Option<String> = match &cmd.args[i] {
            Word::Literal(s, _) => Some(s.clone()),
            Word::StringInterpolation(interp, _) => {
                if interp.parts.len() == 1 {
                    if let crate::ast_words::StringPart::Literal(s) = &interp.parts[0] {
                        Some(s.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(s) = plain_str {
            match s.as_str() {
                "-name" => {
                    if i + 1 < cmd.args.len() {
                        // Accept Literal or single-literal StringInterpolation for the pattern.
                        let pat: Option<String> = match &cmd.args[i + 1] {
                            Word::Literal(p, _) => Some(p.clone()),
                            Word::StringInterpolation(interp, _) => {
                                if interp.parts.len() == 1 {
                                    if let crate::ast_words::StringPart::Literal(p) =
                                        &interp.parts[0]
                                    {
                                        Some(p.clone())
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        if let Some(p) = pat {
                            name_pattern = Some(p);
                        }
                        i += 1;
                    }
                }
                "-type" => {
                    if i + 1 < cmd.args.len() {
                        if let Word::Literal(type_str, _) = &cmd.args[i + 1] {
                            file_type = Some(type_str.clone());
                        }
                        i += 1;
                    }
                }
                "-maxdepth" => {
                    if i + 1 < cmd.args.len() {
                        if let Word::Literal(depth_str, _) = &cmd.args[i + 1] {
                            if let Ok(d) = depth_str.parse::<usize>() {
                                max_depth = Some(d);
                            }
                        }
                        i += 1;
                    }
                }
                _ => {
                    if i == 0 {
                        start_path = s;
                    }
                }
            }
        }
        i += 1;
    }

    // Generate recursive Perl code that matches bash's find traversal order.
    // bash find recurses into subdirectories immediately upon encountering them,
    // while Perl's File::Find defers subdirectory processing to the end.
    // Using a recursive closure ensures the same interleaved order.
    let unique_id = generator.get_unique_id();
    let mut result = format!(
        "do {{\n    use File::Basename;\n    my @files_{} = ();\n",
        unique_id
    );
    result.push_str(&format!(
        "    my $start_{} = q{{{}}};\n",
        unique_id, start_path
    ));

    // Declare a recursive closure variable (set to undef first so the closure
    // can refer to itself without a forward-declaration issue).
    // The closure takes ($dir, $depth) so -maxdepth can be respected.
    let max_depth_check = match max_depth {
        Some(d) => format!("        return if $depth_{u} > {};\n", d, u = unique_id),
        None => String::new(),
    };
    result.push_str(&format!("    my $_find_{u};\n    $_find_{u} = sub {{\n        my ($dir_{u}, $depth_{u}) = @_;\n{max_depth_check}        opendir(my $dh_{u}, $dir_{u}) or return;\n        my @entries_{u} = readdir($dh_{u});\n        closedir($dh_{u});\n        for my $entry_{u} (@entries_{u}) {{\n            next if $entry_{u} eq q{{.}} || $entry_{u} eq q{{..}};\n            my $file_{u} = \"$dir_{u}/$entry_{u}\";\n",
        u = unique_id, max_depth_check = max_depth_check));

    // Recurse into directories (matches bash find's immediate-recursion order).
    result.push_str(&format!(
        "            if (-d $file_{u}) {{\n                $_find_{u}->($file_{u}, $depth_{u} + 1);\n            }}\n",
        u = unique_id
    ));

    // Build the `elsif` block for files matching the criteria.
    result.push_str("            elsif (-f $file_");
    result.push_str(&unique_id);
    result.push_str(") {\n");

    if let Some(pattern) = &name_pattern {
        let glob_pattern = pattern.replace("*", ".*");
        let filename = if pattern.contains('/') {
            format!("$file_{}", unique_id)
        } else {
            format!("basename($file_{})", unique_id)
        };
        result.push_str(&format!(
            "                next if !( {} =~ m/^{}$/xms );\n",
            filename, glob_pattern
        ));
    }

    if let Some(ftype) = &file_type {
        // We already have -f or -d guard above, so add additional type filters.
        if ftype == "d" {
            // If type is directory only (handled above — skip files).
            result.push_str("                next;\n");
        }
        // ftype == "f" is already handled by the -f test
    }

    result.push_str(&format!(
        "                push @files_{}, $file_{};\n",
        unique_id, unique_id
    ));
    result.push_str("            }\n");
    result.push_str("        }\n");
    result.push_str("    };\n");
    result.push_str(&format!("    $_find_{}->($start_{}, 0);\n", unique_id, unique_id));
    result.push_str(&format!("    join \"\\n\", @files_{};\n}}", unique_id));
    result
}

fn parse_size_to_bytes(size_str: &str) -> u64 {
    if size_str.is_empty() {
        return 0;
    }

    let (number_part, unit_multiplier) = if size_str.ends_with('c') {
        (&size_str[..size_str.len() - 1], 1)
    } else if size_str.ends_with('w') {
        (&size_str[..size_str.len() - 1], 2)
    } else if size_str.ends_with('k') {
        (&size_str[..size_str.len() - 1], 1024)
    } else if size_str.ends_with('M') {
        (&size_str[..size_str.len() - 1], 1024 * 1024)
    } else if size_str.ends_with('G') {
        (&size_str[..size_str.len() - 1], 1024 * 1024 * 1024)
    } else {
        (size_str, 1) // default to bytes
    };

    number_part.parse::<u64>().unwrap_or(0) * unit_multiplier
}
