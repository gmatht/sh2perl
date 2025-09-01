use crate::generator::Generator;
use crate::ast::*;

pub fn generate_grep_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str, should_print: bool) -> String {
    let mut output = String::new();
    
    // Parse grep options and pattern
    let mut pattern = String::new();
    let mut count_only = false;
    let mut line_numbers = false;
    let mut ignore_case = false;
    let mut invert_match = false;
    let mut word_match = false;
    let mut only_matching = false;
    let mut quiet_mode = false;
    let mut max_count = None;
    let mut byte_offset = false;
    let mut suppress_filename = false;
    let mut show_filename = false;
    let mut null_terminated = false;
    let mut list_only = false;
    let mut color_always = false;
    
    // First pass: identify options and find the pattern
    let mut args_iter = cmd.args.iter();
    while let Some(arg) = args_iter.next() {
        if let Word::Literal(s) = arg {
            if s.starts_with('-') {
                // Handle --color=always first
                if s.starts_with("--color") {
                    if s == "--color=always" {
                        color_always = true;
                    }
                    // Don't treat this as a pattern
                    continue;
                }
                
                // Parse options
                if s.contains('c') { count_only = true; }
                if s.contains('n') { line_numbers = true; }
                if s.contains('i') { ignore_case = true; }
                if s.contains('v') { invert_match = true; }
                if s.contains('w') { word_match = true; }
                if s.contains('o') { only_matching = true; }
                if s.contains('q') { quiet_mode = true; }
                if s.contains('b') { byte_offset = true; }
                if s.contains('h') { suppress_filename = true; }
                if s.contains('H') { show_filename = true; }
                if s.contains('Z') { null_terminated = true; }
                if s.contains('l') { list_only = true; }
                
                // Handle numeric options
                if s == "-m" {
                    if let Some(Word::Literal(next_arg)) = args_iter.next() {
                        max_count = Some(next_arg.parse().unwrap_or(0));
                    }
                }
            } else if pattern.is_empty() {
                // First non-option argument is the pattern
                pattern = s.clone();
            }
        } else if pattern.is_empty() {
            // First non-literal argument is the pattern
            pattern = generator.word_to_perl(arg);
        }
    }
    
    if pattern.is_empty() {
        // No pattern provided, return error
        output.push_str("warn \"grep: no pattern specified\";\n");
        output.push_str("exit(1);\n");
    } else {
        // Second pass: collect file arguments (arguments that are not options and not the pattern)
        let mut file_args = Vec::new();
        for arg in &cmd.args {
            if let Word::Literal(s) = arg {
                if !s.starts_with('-') && s != &pattern && s != "-m" {
                    // Check if this is a numeric value that follows -m
                    if let Some(prev_arg) = cmd.args.iter().position(|a| a == arg) {
                        if prev_arg > 0 {
                            if let Word::Literal(prev) = &cmd.args[prev_arg - 1] {
                                if prev == "-m" {
                                    continue; // Skip the numeric value after -m
                                }
                            }
                        }
                    }
                    file_args.push(s.clone());
                }
            }
        }
        
        let has_file_args = !file_args.is_empty();
        
        // Always declare the result variable when called from generate_generic_builtin
        // The function is responsible for declaring the variables it uses
        output.push_str(&format!("my $grep_result_{};\n", command_index));
        
        if has_file_args {
            // File-based grep - read from files
            output.push_str(&format!("my @grep_lines_{} = ();\n", command_index));
            for file in &file_args {
                output.push_str(&format!("if (-f '{}') {{\n", file));
                output.push_str(&format!("    open(my $fh, '<', '{}') or die \"Cannot open {}: $!\";\n", file, file));
                output.push_str("    while (my $line = <$fh>) {\n");
                output.push_str("        chomp($line);\n");
                output.push_str(&format!("        push @grep_lines_{}, $line;\n", command_index));
                output.push_str("    }\n");
                output.push_str("    close($fh);\n");
                output.push_str("}\n");
            }
        } else if input_var != "input_data" {
            // Pipeline-based grep with no file arguments - use the provided input variable
            let input_source = if input_var.starts_with('$') {
                input_var.to_string()
            } else {
                format!("${}", input_var)
            };
            
            // Split input into lines and apply grep logic
            output.push_str(&format!("my @grep_lines_{} = split(/\\n/, {});\n", command_index, input_source));
        } else {
            // Standalone grep with no input and no files - this shouldn't happen in practice
            output.push_str(&format!("my @grep_lines_{} = ();\n", command_index));
        }
        
        // Escape the pattern for Perl regex
        let escaped_pattern = pattern.to_string();
        // Remove quotes if they exist around the pattern
        let regex_pattern = if escaped_pattern.starts_with('"') && escaped_pattern.ends_with('"') {
            &escaped_pattern[1..escaped_pattern.len()-1]
        } else if escaped_pattern.starts_with("'") && escaped_pattern.ends_with("'") {
            &escaped_pattern[1..escaped_pattern.len()-1]
        } else {
            &escaped_pattern
        };
        
        // Apply grep filtering
        if invert_match {
            // Negative grep: exclude lines that match the pattern
            output.push_str(&format!("my @grep_filtered_{} = grep !/{}/, @grep_lines_{};\n", command_index, regex_pattern, command_index));
        } else {
            // Positive grep: include lines that match the pattern
            output.push_str(&format!("my @grep_filtered_{} = grep /{}/, @grep_lines_{};\n", command_index, regex_pattern, command_index));
        }
        
        // Apply max count if specified
        if let Some(max) = max_count {
            if max > 0 {
                output.push_str(&format!("@grep_filtered_{} = @grep_filtered_{}[0..{}];\n", command_index, command_index, max - 1));
            }
        }
        
        // Generate output based on options
        if count_only {
            output.push_str(&format!("$grep_result_{} = scalar(@grep_filtered_{});\n", command_index, command_index));
            if should_print && !quiet_mode {
                output.push_str(&format!("print $grep_result_{};\n", command_index));
                output.push_str("print \"\\n\";\n");
            }
        } else if line_numbers {
            output.push_str(&format!("my @grep_numbered_{};\n", command_index));
            output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
            output.push_str(&format!("    if (grep {{ $_ eq $grep_lines_{}[$i] }} @grep_filtered_{}) {{\n", command_index, command_index));
            output.push_str(&format!("        push @grep_numbered_{}, sprintf(\"%d:%s\", $i + 1, $grep_lines_{}[$i]);\n", command_index, command_index));
            output.push_str("    }\n");
            output.push_str("}\n");
            output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_numbered_{});\n", command_index, command_index));
            if should_print && !quiet_mode {
                output.push_str(&format!("print $grep_result_{};\n", command_index));
                output.push_str("print \"\\n\";\n");
            }
        } else if only_matching {
            // Handle -o flag: only output the matching part
            output.push_str(&format!("my @grep_matches_{};\n", command_index));
            output.push_str(&format!("foreach my $line (@grep_filtered_{}) {{\n", command_index));
            output.push_str(&format!("    if ($line =~ /({})/) {{\n", regex_pattern));
            output.push_str(&format!("        push @grep_matches_{}, $1;\n", command_index));
            output.push_str("    }\n");
            output.push_str("}\n");
            output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_matches_{});\n", command_index, command_index));
            if should_print && !quiet_mode {
                output.push_str(&format!("print $grep_result_{};\n", command_index));
                output.push_str("print \"\\n\";\n");
            }
        } else if list_only {
            // Handle -l flag: only show filenames
            if has_file_args {
                output.push_str(&format!("$grep_result_{} = @grep_filtered_{} > 0 ? \"{}\" : \"\";\n", 
                    command_index, command_index, file_args[0]));
            } else {
                output.push_str(&format!("$grep_result_{} = @grep_filtered_{} > 0 ? \"(standard input)\" : \"\";\n", 
                    command_index, command_index));
            }
            if should_print && !quiet_mode {
                output.push_str(&format!("print $grep_result_{};\n", command_index));
                if null_terminated {
                    output.push_str("print \"\\0\";\n");
                } else {
                    output.push_str("print \"\\n\";\n");
                }
            }
        } else {
            // Default case: output matching lines with various formatting options
            if byte_offset {
                // Handle -b flag: show byte offset with output lines
                output.push_str(&format!("my @grep_with_offset_{};\n", command_index));
                output.push_str(&format!("my $offset_{} = 0;\n", command_index));
                output.push_str(&format!("for my $line (@grep_lines_{}) {{\n", command_index));
                output.push_str(&format!("    if (grep {{ $_ eq $line }} @grep_filtered_{}) {{\n", command_index));
                output.push_str(&format!("        push @grep_with_offset_{}, sprintf(\"%d:%s\", $offset_{}, $line);\n", command_index, command_index));
                output.push_str("    }\n");
                output.push_str(&format!("    $offset_{} += length($line) + 1; # +1 for newline\n", command_index));
                output.push_str("}\n");
                output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_with_offset_{});\n", command_index, command_index));
            } else if show_filename && has_file_args {
                // Handle -H flag: always show filename even with single file
                output.push_str(&format!("my @grep_with_filename_{};\n", command_index));
                output.push_str(&format!("for my $line (@grep_filtered_{}) {{\n", command_index));
                output.push_str(&format!("    push @grep_with_filename_{}, \"{}:$line\";\n", command_index, file_args[0]));
                output.push_str("}\n");
                output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_with_filename_{});\n", command_index, command_index));
            } else {
                // Default: just output matching lines (handles -h flag implicitly by not adding filename)
                if color_always {
                    // Add color support for --color=always
                    output.push_str(&format!("my @grep_colored_{};\n", command_index));
                    output.push_str(&format!("for my $line (@grep_filtered_{}) {{\n", command_index));
                    output.push_str(&format!("    my $colored_line = $line;\n"));
                    output.push_str(&format!("    $colored_line =~ s/({})/\\x1b[01;31m\\x1b[K$1\\x1b[m\\x1b[K/g;\n", regex_pattern));
                    output.push_str(&format!("    push @grep_colored_{}, $colored_line;\n", command_index));
                    output.push_str("}\n");
                    output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_colored_{});\n", command_index, command_index));
                } else {
                    output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_filtered_{});\n", command_index, command_index));
                }
            }
            
            // Handle null-terminated output (-Z flag)
            if null_terminated {
                output.push_str(&format!("$grep_result_{} =~ s/\\n/\\0/g;\n", command_index));
            } else {
                // Ensure output ends with newline to match shell behavior, but only if there are matches
                output.push_str(&format!("$grep_result_{} .= \"\\n\" unless $grep_result_{} =~ /\\n$/ || $grep_result_{} eq '';\n", command_index, command_index, command_index));
            }
            
            if should_print && !quiet_mode {
                output.push_str(&format!("print $grep_result_{};\n", command_index));
            }
        }
        
        // Set exit status for quiet mode
        if quiet_mode {
            // In pipelines or logical ops, caller should inspect @grep_filtered_*
        }
    }
    
    output
}
