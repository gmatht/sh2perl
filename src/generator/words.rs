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
            match cmd.as_ref() {
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
                        } else if name == "echo" {
                            // Special handling for echo in command substitution
                            if simple_cmd.args.is_empty() {
                                "\"\\n\"".to_string()
                            } else {
                                let args: Vec<String> = simple_cmd.args.iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect();
                                format!("{} . \"\\n\"", args.join(" . q{ } . "))
                            }
                        } else if name == "printf" {
                            // Special handling for printf in command substitution
                            let mut format_string = String::new();
                            let mut args = Vec::new();
                            
                            for (i, arg) in simple_cmd.args.iter().enumerate() {
                                if i == 0 {
                                    format_string = generator.word_to_perl(arg);
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
                                format!("sprintf({}, {})", 
                                    generator.perl_string_literal(&Word::Literal(format_string, Default::default())),
                                    args.join(", "))
                            }
                        } else if name == "date" {
                            // Special handling for date in command substitution
                            if let Some(format) = simple_cmd.args.first() {
                                let format_str = generator.word_to_perl(format);
                                format!("do {{ use POSIX qw(strftime); strftime({}, localtime); }}", format_str)
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
                                    "\"\"".to_string()
                                };
                                format!("do {{ my $path = {}; my $suffix = {}; $path =~ s/\\Q$suffix\\E$// if $suffix ne q{{}}; $path =~ s/.*\\///; $path; }}", path_str, suffix)
                            } else {
                                "\".\"".to_string()
                            }
                        } else if name == "dirname" {
                            // Special handling for dirname in command substitution
                            if let Some(path) = simple_cmd.args.first() {
                                let path_str = generator.word_to_perl(path);
                                format!("do {{ my $path = {}; if ($path =~ /\\//) {{ $path =~ s/\\/[^\\/]*$//; $path = q{{.}} if $path eq q{{}}; }} else {{ $path = q{{.}}; }} $path; }}", path_str)
                            } else {
                                "\".\"".to_string()
                            }
                        } else if name == "which" {
                            // Special handling for which in command substitution
                            if let Some(command) = simple_cmd.args.first() {
                                let command_str = generator.word_to_perl(command);
                                format!("do {{ my $command = {}; my $found = 0; my $result = q{{}}; foreach my $dir (split /:/, $ENV{{PATH}}) {{ my $full_path = \"$dir/$command\"; if (-x $full_path) {{ $result = $full_path; $found = 1; last; }} }} $result; }}", command_str)
                            } else {
                                "\"\"".to_string()
                            }
                        } else if name == "seq" {
                            // Special handling for seq in command substitution
                            if simple_cmd.args.is_empty() {
                                "\"1\"".to_string()
                            } else if simple_cmd.args.len() == 1 {
                                let last_str = generator.word_to_perl(&simple_cmd.args[0]);
                                format!("do {{ my $last = {}; join \"\\n\", 1..$last; }}", last_str)
                            } else if simple_cmd.args.len() == 2 {
                                let first_str = generator.word_to_perl(&simple_cmd.args[0]);
                                let last_str = generator.word_to_perl(&simple_cmd.args[1]);
                                format!("do {{ my $first = {}; my $last = {}; join \"\\n\", $first..$last; }}", first_str, last_str)
                            } else if simple_cmd.args.len() == 3 {
                                let first_str = generator.word_to_perl(&simple_cmd.args[0]);
                                let increment_str = generator.word_to_perl(&simple_cmd.args[1]);
                                let last_str = generator.word_to_perl(&simple_cmd.args[2]);
                                format!("do {{ my $first = {}; my $increment = {}; my $last = {}; my @result; for (my $i = $first; $i <= $last; $i += $increment) {{ push @result, $i; }} join \"\\n\", @result; }}", first_str, increment_str, last_str)
                            } else {
                                "\"\"".to_string()
                            }
                        } else if name == "grep" {
                            // Special handling for grep in command substitution
                            // For now, fall back to system grep until we implement a full native version
                            let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                            format!(" my ({}, {}, {}); my {} = open3({}, {}, {}, 'grep', {}); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {}", 
                                in_var, out_var, err_var, pid_var, in_var, out_var, err_var, 
                                simple_cmd.args.iter().map(|arg| generator.word_to_perl(arg)).collect::<Vec<_>>().join(", "),
                                in_var, result_var, out_var, out_var, pid_var, result_var)
                        } else if generator.inline_mode && name == "echo" {
                            // In inline mode for echo, generate the output value directly
                            if simple_cmd.args.is_empty() {
                                "\"\\n\"".to_string()
                            } else {
                                let args: Vec<String> = simple_cmd.args.iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect();
                                format!("{} . \"\\n\"", args.join(" . q{ } . "))
                            }
                        } else {
                            // Fall back to system command for non-builtin commands
                            let args: Vec<String> = simple_cmd.args.iter()
                                .map(|arg| generator.word_to_perl(arg))
                                .collect();
                            
                            let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                            if args.is_empty() {
                                format!(" my ({}, {}, {}); my {} = open3({}, {}, {}, '{}'); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
                            } else {
                                let formatted_args = args.iter().map(|arg| {
                                    let word = Word::Literal(arg.clone(), Default::default());
                                    generator.perl_string_literal(&word)
                                }).collect::<Vec<_>>().join(", ");
                                format!(" my ({}, {}, {}); my {} = open3({}, {}, {}, '{}', {}); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        }
                    } else {
                        // Fall back to system command for non-literal command names
                        let args: Vec<String> = simple_cmd.args.iter()
                            .map(|arg| generator.word_to_perl(arg))
                            .collect();
                        
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
                    }
                },
                Command::Pipeline(pipeline) => {
                    // For command substitution pipelines, we need to execute the pipeline
                    // and capture its output instead of printing it
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
                        "if ({} ne q{} && !defined $output_printed_{}) {{\n\n        print {};\n        if (!({} =~ {})) {{ print \"\\n\"; }}\n    }}", 
                        output_var, "", output_var_num, output_var, output_var, generator.newline_end_regex()
                    );
                    captured_pipeline = captured_pipeline.replace(&print_block_to_remove, "");
                    
                    // Also try without the extra newlines in case formatting is different
                    let print_block_compact = format!(
                        "if ({} ne q{} && !defined $output_printed_{}) {{ print {}; if (!({} =~ {})) {{ print \"\\n\"; }} }}", 
                        output_var, "", output_var_num, output_var, output_var, generator.newline_end_regex()
                    );
                    captured_pipeline = captured_pipeline.replace(&print_block_compact, "");
                    
                    // Remove the outer braces if they exist, as we'll wrap in our own do block
                    captured_pipeline = captured_pipeline.trim().to_string();
                    if captured_pipeline.starts_with('{') && captured_pipeline.ends_with('}') {
                        captured_pipeline = captured_pipeline[1..captured_pipeline.len()-1].to_string();
                    }
                    
                    // Return the code that executes the pipeline and captures output
                    // Command substitution should convert newlines to spaces (bash behavior)
                    if captured_pipeline.contains(&output_var) {
                        format!("do {{ {} chomp {}; {} =~ s/\\n/ /gsxm; {} }}", captured_pipeline.trim(), output_var, output_var, output_var)
                    } else {
                        // If the pipeline doesn't contain the output variable, declare it and assign the result
                        format!("do {{ my {} = q{{}}; {} chomp {}; {} =~ s/\\n/ /gsxm; {} }}", output_var, captured_pipeline.trim(), output_var, output_var, output_var)
                    }
                },
                _ => {
                    // For other command types, use system command fallback
                    let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                    format!(" my ({}); my {} = open3({}, {}, {}, 'bash', '-c', '{}'); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {}", in_var, pid_var, in_var, out_var, err_var, generator.generate_command_string_for_system(cmd), in_var, result_var, out_var, out_var, pid_var, result_var)
                }
            }
        },
        Word::Variable(var, _, _) => {
            // Handle special shell variables
            match var.as_str() {
                "#" => "scalar(@ARGV)".to_string(),  // $# -> scalar(@ARGV) for argument count
                "@" => "@ARGV".to_string(),          // $@ -> @ARGV for arguments array
                "*" => "@ARGV".to_string(),          // $* -> @ARGV for arguments array
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
        Word::Literal(s, _) => s.clone(),
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

pub fn convert_string_interpolation_to_perl_impl(_generator: &Generator, interp: &StringInterpolation) -> String {
    // Convert string interpolation to a single Perl interpolated string
    let mut combined_string = String::new();
    
    for part in &interp.parts {
        match part {
            StringPart::Literal(s) => {
                // Keep escape sequences as literal characters for Perl
                // Don't process them here - let Perl handle them
                combined_string.push_str(s);
            },
            StringPart::Variable(var) => {
                // Handle special shell variables
                match var.as_str() {
                    "#" => combined_string.push_str("scalar(@ARGV)"),  // $# -> scalar(@ARGV) for argument count
                    "@" => combined_string.push_str("@ARGV"),          // Arrays don't need $ in interpolation
                    "*" => combined_string.push_str("@ARGV"),          // Arrays don't need $ in interpolation
                    _ => {
                        // Check if this is a shell positional parameter ($1, $2, etc.)
                        if var.chars().all(|c| c.is_digit(10)) {
                            // Convert $1 to $_[0], $2 to $_[1], etc.
                            let index = var.parse::<usize>().unwrap_or(0);
                            combined_string.push_str(&format!("$_[{}]", index - 1)); // Perl arrays are 0-indexed
                        } else {
                            // Regular variable - add directly for interpolation
                            // In bash, $ENV{SHELL_VAR} is treated as variable $ENV followed by literal {SHELL_VAR}
                            combined_string.push_str(&format!("${}", var));
                        }
                    }
                }
            },
            StringPart::MapAccess(map_name, key) => {
                if map_name == "map" {
                    combined_string.push_str(&format!("$map{{{}}}", key));
                } else {
                    combined_string.push_str(&format!("${}{{{}}}", map_name, key));
                }
            }
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
                                combined_string.push_str(&format!("scalar(@{})", array_name));
                            } else if pe.variable.starts_with('!') {
                                // ${!map[@]} -> keys %map (map keys iteration)
                                let map_name = &pe.variable[1..]; // Remove ! prefix
                                combined_string.push_str(&format!("keys %{}", map_name));
                            } else {
                                // ${arr[@]} -> @arr (for array iteration)
                                let array_name = &pe.variable;
                                combined_string.push_str(&format!("@{}", array_name));
                            }
                        } else {
                            // Regular array slice
                            if let Some(length_str) = length {
                                combined_string.push_str(&format!("@${{{}}}[{}..{}]", pe.variable, offset, length_str));
                            } else {
                                combined_string.push_str(&format!("@${{{}}}[{}..]", pe.variable, offset));
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
                                        combined_string.push_str(&format!("${}[{}]", var_name, key));
                                    } else {
                                        // Associative array access: map[foo] -> $map{foo}
                                        combined_string.push_str(&format!("${}{{{}}}", var_name, key));
                                    }
                                } else {
                                    combined_string.push_str(&format!("${{{}}}", pe.variable));
                                }
                            } else {
                                combined_string.push_str(&format!("${{{}}}", pe.variable));
                            }
                        } else {
                            // Simple variable reference
                            combined_string.push_str(&format!("${{{}}}", pe.variable));
                        }
                    }
                }
            }
            _ => {
                // Handle other StringPart variants by converting them to debug format for now
                combined_string.push_str(&format!("{:?}", part));
            }
        }
    }
    
    // Check if the string actually needs interpolation
    let needs_interpolation = combined_string.contains('$') || combined_string.contains('@') || combined_string.contains('\\');
    
    if needs_interpolation {
        // Return as a double-quoted interpolated string
        // Escape newlines, tabs, and other special characters for proper Perl formatting
        let escaped_string = combined_string
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\t", "\\t")
            .replace("\r", "\\r");
        format!("\"{}\"", escaped_string)
    } else {
        // Return as a single-quoted string since no interpolation is needed
        // Use q{} for single characters to avoid "noisy quotes" violations
        if combined_string.len() == 1 && !combined_string.contains('\'') && !combined_string.contains('{') && !combined_string.contains('}') {
            format!("q{{{}}}", combined_string)
        } else if combined_string.len() == 1 && combined_string.contains('\'') {
            // Handle single quotes in single character strings
            format!("q{{{}}}", combined_string)
        } else {
            let escaped = combined_string.replace("\\", "\\\\").replace("'", "\\'");
            format!("'{}'", escaped)
        }
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
