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
            '.' => result.push_str("\\."),
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
            _ => result.push(*c)
        }
    }
    
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
    
    // Generate a unique subroutine name
    let subroutine_id = generator.get_unique_id();
    let subroutine_name = format!("find_files_{}", subroutine_id);
    
    // Parse find arguments to understand what we're looking for
    let mut start_path = ".".to_string();
    let mut name_pattern = None;
    let mut file_type = None;
    let mut mtime_days = None;
    let mut mmin_minutes = None;
    let mut size_spec = None;
    let mut empty_only = false;
    let mut exec_command = None;
    let mut ls_format = false;
    let mut not_paths = Vec::new();
    
    let mut i = 0;
    while i < cmd.args.len() {
        match &cmd.args[i] {
            Word::Literal(s, _) => {
                match s.as_str() {
                    "-name" => {
                        if i + 1 < cmd.args.len() {
                            if let Word::StringInterpolation(interp, _) = &cmd.args[i + 1] {
                                let pattern = interp.parts.iter()
                                    .map(|part| match part {
                                        StringPart::Literal(s) => s.clone(),
                                        _ => "*".to_string(),
                                    })
                                    .collect::<String>();
                                name_pattern = Some(pattern);
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
                    "-mtime" => {
                        if i + 1 < cmd.args.len() {
                            if let Word::Literal(time_str, _) = &cmd.args[i + 1] {
                                mtime_days = Some(time_str.clone());
                            }
                            i += 1;
                        }
                    },
                    "-mmin" => {
                        if i + 1 < cmd.args.len() {
                            if let Word::Literal(min_str, _) = &cmd.args[i + 1] {
                                mmin_minutes = Some(min_str.clone());
                            }
                            i += 1;
                        }
                    },
                    "-size" => {
                        if i + 1 < cmd.args.len() {
                            if let Word::Literal(size_str, _) = &cmd.args[i + 1] {
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
                        while i < cmd.args.len() {
                            if let Word::Literal(exec_arg, _) = &cmd.args[i] {
                                if exec_arg == ";" {
                                    break;
                                }
                                exec_args.push(exec_arg.clone());
                            } else if let Word::BraceExpansion(be, _) = &cmd.args[i] {
                                // Handle {} placeholder
                                if be.items.len() == 1 {
                                    if let BraceItem::Literal(s) = &be.items[0] {
                                        if s == "{}" {
                                            exec_args.push("{}".to_string());
                                        }
                                    }
                                }
                            } else {
                                exec_args.push(generator.word_to_perl(&cmd.args[i]));
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
                        if i + 1 < cmd.args.len() && i + 2 < cmd.args.len() {
                            if let Word::Literal(not_arg, _) = &cmd.args[i + 1] {
                                if not_arg == "-path" {
                                    if let Word::StringInterpolation(interp, _) = &cmd.args[i + 2] {
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
                            start_path = s.clone();
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
                    start_path = path;
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
    output.push_str("if (opendir(my $dh, $dir)) {\n");
    
    output.push_str(&indent3);
    output.push_str("while (my $file = readdir($dh)) {\n");
    
    output.push_str(&indent4);
    output.push_str("next if $file eq '.' || $file eq '..';\n");
    
    output.push_str(&indent4);
    output.push_str("my $full_path = \"$dir/$file\";\n");
    
    // Add file type check
    if let Some(ftype) = &file_type {
        match ftype.as_str() {
            "f" => {
                output.push_str(&indent4);
                output.push_str("next unless -f $full_path;\n");
            },
            "d" => {
                output.push_str(&indent4);
                output.push_str("next unless -d $full_path;\n");
            },
            _ => {}
        }
    }
    
    // Add name pattern check
    if let Some(pattern) = &name_pattern {
        output.push_str(&indent4);
        output.push_str(&format!("next unless $file =~ /{}/;\n", escape_glob_pattern(pattern)));
    }
    
    // Add empty check
    if empty_only {
        output.push_str(&indent4);
        output.push_str("if (-f $full_path) {\n");
        output.push_str(&indent5);
        output.push_str("next unless -z $full_path;\n");
        output.push_str(&indent4);
        output.push_str("} elsif (-d $full_path) {\n");
        output.push_str(&indent5);
        output.push_str("opendir(my $empty_dh, $full_path) or next;\n");
        output.push_str(&indent5);
        output.push_str("my @entries = grep { $_ ne '.' && $_ ne '..' } readdir($empty_dh);\n");
        output.push_str(&indent5);
        output.push_str("closedir($empty_dh);\n");
        output.push_str(&indent5);
        output.push_str("next unless @entries == 0;\n");
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
            output.push_str(&format!("next unless (0 + -M $full_path) < {};\n", days));
        } else if mtime.starts_with('+') {
            // Positive mtime means "more than N days old"
            let days = &mtime[1..];
            output.push_str(&format!("next unless (0 + -M $full_path) > {};\n", days));
        } else {
            // Exact mtime means "exactly N days old"
            output.push_str(&format!("next unless (0 + -M $full_path) == {};\n", mtime));
        }
    }
    
    // Add mmin check
    if let Some(mmin) = &mmin_minutes {
        output.push_str(&indent4);
        if mmin.starts_with('-') {
            // Negative mmin means "less than N minutes old"
            let minutes = &mmin[1..];
            output.push_str(&format!("next unless (0 + -M $full_path) * 24 * 60 < {};\n", minutes));
        } else if mmin.starts_with('+') {
            // Positive mmin means "more than N minutes old"
            let minutes = &mmin[1..];
            output.push_str(&format!("next unless (0 + -M $full_path) * 24 * 60 > {};\n", minutes));
    } else {
            // Exact mmin means "exactly N minutes old"
            output.push_str(&format!("next unless (0 + -M $full_path) * 24 * 60 == {};\n", mmin));
        }
    }
    
    // Add size check
    if let Some(size) = &size_spec {
        if size.starts_with('+') {
            let size_val = &size[1..];
            output.push_str(&indent4);
            output.push_str(&format!("next unless -s $full_path > {};\n", size_val));
        } else if size.starts_with('-') {
            let size_val = &size[1..];
            output.push_str(&indent4);
            output.push_str(&format!("next unless -s $full_path < {};\n", size_val));
        }
    }
    
    // Add not path checks
    for not_path in &not_paths {
        output.push_str(&indent4);
        output.push_str(&format!("next if $full_path =~ /{}/;\n", escape_glob_pattern(not_path)));
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
        output.push_str("system($exec_cmd);\n");
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
            output.push_str("my $perms = '';\n");
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
            output.push_str("push @$results, sprintf(\"%d %d -%s %d %s %s %d %s %s\", $inode, $blocks, $perms, $stat[3], $owner, $group, $stat[7], $mtime, $full_path);\n");
        } else {
            output.push_str(&indent4);
            output.push_str("push @$results, $full_path;\n");
        }
    }
    
    // Recursive call for directories
    output.push_str(&indent4);
    output.push_str("if (-d $full_path) {\n");
    output.push_str(&format!("{}    ", indent4));
    output.push_str(&format!("{}($full_path, $results);\n", subroutine_name));
    output.push_str(&indent4);
    output.push_str("}\n");
    
    output.push_str(&indent3);
    output.push_str("}\n");
    
    output.push_str(&indent2);
    output.push_str("closedir($dh);\n");
    
    output.push_str(&indent2);
    output.push_str("}\n");
    
    output.push_str(&indent1);
    output.push_str("}\n");
    
    // Call the function
    output.push_str(&indent1);
    output.push_str("my @find_results;\n");
    output.push_str(&indent1);
    output.push_str(&format!("{}(\"{}\", \\@find_results);\n", subroutine_name, start_path));
    
    if generate_output {
        output.push_str(&indent1);
        output.push_str(&format!("${} = join(\"\\n\", @find_results);\n", input_var));
        output.push_str(&indent1);
        output.push_str(&format!("${} .= \"\\n\" unless ${} =~ /\\n$/;\n", input_var, input_var));
    } else {
        output.push_str(&indent1);
        output.push_str("print join(\"\\n\", @find_results) . \"\\n\";\n");
    }
    
    output.push_str(&base_indent);
    output.push_str("}\n");
    
    output
}

fn generate_system_find_fallback(generator: &mut Generator, cmd: &SimpleCommand, generate_output: bool, input_var: &str) -> String {
    let mut output = String::new();
    
    // Build the find command arguments
    let mut find_args = vec!["find".to_string()];
    for arg in &cmd.args {
        match arg {
            Word::Literal(s, _) => find_args.push(s.clone()),
            Word::StringInterpolation(interp, _) => {
                let arg_str = interp.parts.iter()
                    .map(|part| match part {
                        StringPart::Literal(s) => s.clone(),
                        StringPart::Variable(var) => format!("$ENV{{{}}}", var),
                        StringPart::MapAccess(map, key) => format!("$ENV{{{}}}[{}]", map, key),
                        StringPart::ParameterExpansion(param) => format!("$ENV{{{}}}", param),
                        StringPart::MapKeys(_) => "*".to_string(), // Fallback for unsupported
                        StringPart::MapLength(_) => "*".to_string(), // Fallback for unsupported
                        StringPart::ArraySlice(_, _, _) => "*".to_string(), // Fallback for unsupported
                        StringPart::Arithmetic(_) => "*".to_string(), // Fallback for unsupported
                        StringPart::CommandSubstitution(_) => "*".to_string(), // Fallback for unsupported
                    })
                    .collect::<String>();
                find_args.push(arg_str);
            },
            _ => {
                // For other word types, convert to Perl
                find_args.push(generator.word_to_perl(arg));
            }
        }
    }
    
    // Join arguments with spaces and escape properly
    let find_cmd = find_args.join(" ");
    
    if generate_output {
        // For pipeline context, capture output to variable
        output.push_str(&generator.indent());
        output.push_str(&format!("${} = `{}`;\n", input_var, find_cmd));
        output.push_str(&generator.indent());
        output.push_str(&format!("chomp(${});\n", input_var));
    } else {
        // For standalone commands, execute directly
        output.push_str(&generator.indent());
        output.push_str(&format!("system(\"{}\");\n", find_cmd));
    }
    
    output
}
