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
    
    // For now, use the simple substitution for all cases to avoid complexity
    generate_find_for_substitution(generator, cmd, input_var)
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
        output.push_str(&format!("my ({}, {}, {});
my {} = open3({}, {}, {}, 'find', {});
close {} or croak 'Close failed: $!';
${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $!';
waitpid {}, 0;\n", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, formatted_args, in_var, input_var, out_var, out_var, pid_var));
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

pub fn generate_find_for_substitution(generator: &mut Generator, cmd: &SimpleCommand, _input_var: &str) -> String {
    // For simple find commands, use a much simpler approach
    let mut start_path = ".";
    let mut name_pattern = None;
    let mut file_type = None;
    
    // Simple argument parsing
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(s, _) = &cmd.args[i] {
                match s.as_str() {
                    "-name" => {
                    if i + 1 < cmd.args.len() {
                        if let Word::Literal(pattern, _) = &cmd.args[i + 1] {
                                    name_pattern = Some(pattern.clone());
                            }
                            i += 1;
                        }
                    },
                    "-type" => {
                    if i + 1 < cmd.args.len() {
                        if let Word::Literal(type_str, _) = &cmd.args[i + 1] {
                                file_type = Some(type_str.clone());
                            }
                            i += 1;
                        }
                    },
                    _ => {
                        if i == 0 {
                        start_path = s;
                    }
                }
            }
        }
        i += 1;
    }
    
    // Generate recursive Perl code using File::Find
    let unique_id = generator.get_unique_id();
    let mut result = format!("do {{\n    use File::Find;\n    use File::Basename;\n    my @files_{} = ();\n", unique_id);
    result.push_str(&format!("    my $start_{} = q{{{}}};\n", unique_id, start_path));
    
    // Create a recursive find function
    result.push_str(&format!("    sub find_files_{} {{\n", unique_id));
    result.push_str(&format!("        my $file_{} = $File::Find::name;\n", unique_id));
    
    if let Some(ftype) = &file_type {
        if ftype == "f" {
            result.push_str(&format!("        if (!(-f $file_{})) {{\n", unique_id));
            result.push_str(&format!("            return;\n"));
            result.push_str(&format!("        }}\n"));
        } else if ftype == "d" {
            result.push_str(&format!("        if (!(-d $file_{})) {{\n", unique_id));
            result.push_str(&format!("            return;\n"));
            result.push_str(&format!("        }}\n"));
        }
    }
    
    if let Some(pattern) = &name_pattern {
        let glob_pattern = pattern.replace("*", ".*");
        let filename = if pattern.contains('/') {
            // If pattern contains path separators, match against full path
            format!("$file_{}", unique_id)
        } else {
            // If pattern doesn't contain path separators, match against basename
            format!("basename($file_{})", unique_id)
        };
        result.push_str(&format!("        if (!({} =~ m/^{}$/xms)) {{\n", filename, glob_pattern));
        result.push_str(&format!("            return;\n"));
        result.push_str(&format!("        }}\n"));
    }
    
    result.push_str(&format!("        push @files_{}, $file_{};\n", unique_id, unique_id));
    result.push_str(&format!("        return;\n"));
    
    result.push_str("    }\n");
    result.push_str(&format!("    find(\\&find_files_{}, $start_{});\n", unique_id, unique_id));
    result.push_str(&format!("    join \"\\n\", @files_{};\n}}", unique_id));
    result
}

fn parse_size_to_bytes(size_str: &str) -> u64 {
    if size_str.is_empty() {
        return 0;
    }
    
    let (number_part, unit_multiplier) = if size_str.ends_with('c') {
        (&size_str[..size_str.len()-1], 1)
    } else if size_str.ends_with('w') {
        (&size_str[..size_str.len()-1], 2)
    } else if size_str.ends_with('k') {
        (&size_str[..size_str.len()-1], 1024)
    } else if size_str.ends_with('M') {
        (&size_str[..size_str.len()-1], 1024 * 1024)
    } else if size_str.ends_with('G') {
        (&size_str[..size_str.len()-1], 1024 * 1024 * 1024)
    } else {
        (size_str, 1) // default to bytes
    };
    
    number_part.parse::<u64>().unwrap_or(0) * unit_multiplier
}