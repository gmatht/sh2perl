use crate::generator::Generator;
use crate::ast::*;
use crate::mir::*;

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
    let mut files_without_match = false;
    let mut after_context = 0;
    let mut before_context = 0;
    let mut context_lines = 0;
    let mut color_always = false;
    let mut recursive = false;
    let mut include_pattern = None;
    let mut exclude_pattern = None;
    let mut pattern_file = None;
    let mut missing_pattern = None;
    
    // First pass: identify options and find the pattern
    let mut args_iter = cmd.args.iter();
    while let Some(arg) = args_iter.next() {
        if let Word::Literal(s, _) = arg {
            if s.starts_with('-') {
                // Handle --color=always first
                if s.starts_with("--color") {
                    if s == "--color=always" {
                        color_always = true;
                    }
                    // Don't treat this as a pattern
                    continue;
                }
                
                // Handle --include flag
                if s.starts_with("--include=") {
                    let pattern = s[10..].to_string(); // Remove "--include=" prefix
                    // Remove quotes if present
                    let clean_pattern = if pattern.starts_with('"') && pattern.ends_with('"') {
                        pattern[1..pattern.len()-1].to_string()
                    } else if pattern.starts_with("'") && pattern.ends_with("'") {
                        pattern[1..pattern.len()-1].to_string()
                    } else {
                        pattern
                    };
                    include_pattern = Some(clean_pattern);
                    // Don't treat this as a pattern

                    continue;
                }
                
                // Handle --exclude flag
                if s.starts_with("--exclude=") {
                    let pattern = s[10..].to_string(); // Remove "--exclude=" prefix
                    // Remove quotes if present
                    let clean_pattern = if pattern.starts_with('"') && pattern.ends_with('"') {
                        pattern[1..pattern.len()-1].to_string()
                    } else if pattern.starts_with("'") && pattern.ends_with("'") {
                        pattern[1..pattern.len()-1].to_string()
                    } else {
                        pattern
                    };
                    exclude_pattern = Some(clean_pattern);
                    // Don't treat this as a pattern
                    continue;
                }
                
                // Handle --missing flag
                if s.starts_with("--missing=") {
                    let pattern = s[10..].to_string(); // Remove "--missing=" prefix
                    // Remove quotes if present
                    let clean_pattern = if pattern.starts_with('"') && pattern.ends_with('"') {
                        pattern[1..pattern.len()-1].to_string()
                    } else if pattern.starts_with("'") && pattern.ends_with("'") {
                        pattern[1..pattern.len()-1].to_string()
                    } else {
                        pattern
                    };
                    missing_pattern = Some(clean_pattern);
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
                if s.contains('L') { files_without_match = true; }
                if s.contains('r') { recursive = true; }
                
                // Handle numeric options
                if s == "-m" {
                    if let Some(Word::Literal(next_arg, _)) = args_iter.next() {
                        max_count = Some(next_arg.parse().unwrap_or(0));
                    }
                } else if s == "-f" {
                    if let Some(Word::Literal(next_arg, _)) = args_iter.next() {
                        pattern_file = Some(next_arg.clone());
                    } else if let Some(ref temp_file) = generator.current_process_sub_file {
                        // Use the process substitution file if no explicit file is provided
                        pattern_file = Some(format!("${}", temp_file));
                    }
                } else if s == "-A" {
                    if let Some(Word::Literal(next_arg, _)) = args_iter.next() {
                        after_context = next_arg.parse().unwrap_or(0);
                    }
                } else if s == "-B" {
                    if let Some(Word::Literal(next_arg, _)) = args_iter.next() {
                        before_context = next_arg.parse().unwrap_or(0);
                    }
                } else if s == "-C" {
                    if let Some(Word::Literal(next_arg, _)) = args_iter.next() {
                        context_lines = next_arg.parse().unwrap_or(0);
                    }
                }
            } else if pattern.is_empty() {
                // First non-option argument is the pattern
                pattern = s.clone();
            }
        } else if pattern.is_empty() {
            // First non-literal argument is the pattern
            // For StringInterpolation, extract the raw string content for regex patterns
            if let Word::StringInterpolation(interp, _) = arg {
                if interp.parts.len() == 1 {
                    if let StringPart::Literal(s) = &interp.parts[0] {
                        // Use the raw string content for regex patterns
                        pattern = s.clone();
                    } else {
                        // Fall back to normal string interpolation handling
                        pattern = generator.word_to_perl(arg);
                    }
                } else {
                    // Fall back to normal string interpolation handling
                    pattern = generator.word_to_perl(arg);
                }
            } else {
                // For other word types, use normal processing
                pattern = generator.word_to_perl(arg);
            }
        }
    }
    
    if pattern.is_empty() && pattern_file.is_none() {
        // No pattern provided, return error
        output.push_str("warn \"grep: no pattern specified\";\n");
        output.push_str("exit(1);\n");
        return output;
    }
    
    // Second pass: collect file arguments (arguments that are not options and not the pattern)
    let mut file_args = Vec::new();
    let mut i = 0;
    while i < cmd.args.len() {
            if let Word::Literal(s, _) = &cmd.args[i] {
                if !s.starts_with('-') && s != &pattern {
                    // Check if this is a pattern file (skip it from file_args)
                    if let Some(ref pf) = pattern_file {
                        if s == pf {
                            i += 1; // Skip the pattern file argument
                            continue;
                        }
                    }
                    
                    // Check if this is a numeric value that follows a context flag
                    if i > 0 {
                        if let Word::Literal(prev, _) = &cmd.args[i - 1] {
                            if prev == "-m" || prev == "-A" || prev == "-B" || prev == "-C" {
                                i += 1; // Skip the numeric value after context flags
                                continue;
                            }
                        }
                    }
                    
                    // Check if this is part of a glob pattern (like *.txt)
                    if s == "*" && i + 1 < cmd.args.len() {
                        if let Word::Literal(next, _) = &cmd.args[i + 1] {
                            if next.starts_with('.') {
                                // Combine * and .txt into *.txt
                                file_args.push(format!("{}{}", s, next));
                                i += 1; // Skip the next argument
                            } else {
                                file_args.push(s.clone());
                            }
                        } else {
                            file_args.push(s.clone());
                        }
                    } else {
                        file_args.push(s.clone());
                    }
                }
            }
            i += 1;
        }
        
        let has_file_args = !file_args.is_empty();
        
        // Always declare the result variable when called from generate_generic_builtin
        // The function is responsible for declaring the variables it uses
        // But don't declare if it's already declared (to avoid duplicate declarations in logical OR)
        output.push_str(&format!("my $grep_result_{};\n", command_index));
        
        if has_file_args {
            // File-based grep - read from files
            output.push_str(&format!("my @grep_lines_{} = ();\n", command_index));
            output.push_str(&format!("my @grep_filenames_{} = ();\n", command_index));
            
            if recursive {
                // Recursive search
                output.push_str(&format!("sub find_files_recursive_{} {{\n", command_index));
                output.push_str(&format!("    my ($dir, $pattern) = @_;\n"));
                output.push_str(&format!("    my @files;\n"));
                output.push_str(&format!("    if (opendir(my $dh, $dir)) {{\n"));
                output.push_str(&format!("        while (my $file = readdir($dh)) {{\n"));
                output.push_str(&format!("            next if $file eq '.' || $file eq '..';\n"));
                output.push_str(&format!("            my $path = \"$dir/$file\";\n"));
                output.push_str(&format!("            if (-d $path) {{\n"));
                output.push_str(&format!("                @files = (@files, find_files_recursive_{}($path, $pattern));\n", command_index));
                output.push_str(&format!("            }} elsif (-f $path) {{\n"));
                // Handle file filtering with include/exclude patterns
                let mut conditions = Vec::new();
                
                if let Some(ref include_pat) = include_pattern {
                    // Convert shell glob pattern to Perl regex
                    // *.txt -> .*\.txt$
                    let mut regex_pattern = include_pat.clone();
                    // Replace * with .* first
                    regex_pattern = regex_pattern.replace("*", ".*");
                    // Escape literal dots (but not the ones from .*)
                    regex_pattern = regex_pattern.replace(".", "\\.");
                    // Fix the .* that got escaped to \.*
                    regex_pattern = regex_pattern.replace("\\.*", ".*");
                    // Add end anchor for proper matching
                    if !regex_pattern.ends_with("$") {
                        regex_pattern.push_str("$");
                    }
                    conditions.push(format!("$file =~ /{}/", regex_pattern));
                } else {
                    // Only include .txt files by default for recursive search
                    conditions.push("$file =~ /\\.txt$/".to_string());
                }
                
                if let Some(ref exclude_pat) = exclude_pattern {
                    // Convert shell glob pattern to Perl regex for exclusion
                    let mut regex_pattern = exclude_pat.clone();
                    regex_pattern = regex_pattern.replace("*", ".*");
                    regex_pattern = regex_pattern.replace(".", "\\.");
                    regex_pattern = regex_pattern.replace("\\.*", ".*");
                    if !regex_pattern.ends_with("$") {
                        regex_pattern.push_str("$");
                    }
                    conditions.push(format!("$file !~ /{}/", regex_pattern));
                }
                
                if conditions.is_empty() {
                    output.push_str(&format!("                if ($file =~ /\\.txt$/) {{\n"));
                } else {
                    output.push_str(&format!("                if ({}) {{\n", conditions.join(" && ")));
                }
                output.push_str(&format!("                    push @files, $path;\n"));
                output.push_str(&format!("                }}\n"));
                output.push_str(&format!("            }}\n"));
                output.push_str(&format!("        }}\n"));
                output.push_str(&format!("        closedir($dh);\n"));
                output.push_str(&format!("    }}\n"));
                output.push_str(&format!("    return @files;\n"));
                output.push_str(&format!("}}\n"));
                
                for file in &file_args {
                    output.push_str(&format!("my @files_{} = find_files_recursive_{}('{}', '{}');\n", command_index, command_index, file, include_pattern.as_ref().unwrap_or(&"*".to_string())));
                    output.push_str(&format!("for my $file (@files_{}) {{\n", command_index));
                    output.push_str(&format!("    if (-f $file) {{\n"));
                    output.push_str(&format!("        open(my $fh, '<', $file) or die \"Cannot open $file: $!\";\n"));
                    output.push_str(&format!("        while (my $line = <$fh>) {{\n"));
                    output.push_str(&format!("            chomp($line);\n"));
                    output.push_str(&format!("            push @grep_lines_{}, $line;\n", command_index));
                    output.push_str(&format!("            push @grep_filenames_{}, $file;\n", command_index));
                    output.push_str(&format!("        }}\n"));
                    output.push_str(&format!("        close($fh);\n"));
                    output.push_str(&format!("    }}\n"));
                    output.push_str(&format!("}}\n"));
                }
            } else {
                // Non-recursive search
                for file in &file_args {
                    if file.contains('*') {
                        // Handle glob patterns
                        output.push_str(&format!("my @glob_files_{} = glob('{}');\n", command_index, file));
                        output.push_str(&format!("for my $glob_file (@glob_files_{}) {{\n", command_index));
                        output.push_str(&format!("    if (-f $glob_file) {{\n"));
                        output.push_str(&format!("        open(my $fh, '<', $glob_file) or die \"Cannot open $glob_file: $!\";\n"));
                        output.push_str("        while (my $line = <$fh>) {\n");
                        output.push_str("            chomp($line);\n");
                        output.push_str(&format!("            push @grep_lines_{}, $line;\n", command_index));
                        output.push_str(&format!("            push @grep_filenames_{}, $glob_file;\n", command_index));
                        output.push_str("        }\n");
                        output.push_str("        close($fh);\n");
                        output.push_str("    }\n");
                        output.push_str("}\n");
                    } else {
                        output.push_str(&format!("if (-f '{}') {{\n", file));
                        output.push_str(&format!("    open(my $fh, '<', '{}') or die \"Cannot open {}: $!\";\n", file, file));
                        output.push_str("    while (my $line = <$fh>) {\n");
                        output.push_str("        chomp($line);\n");
                        output.push_str(&format!("        push @grep_lines_{}, $line;\n", command_index));
                        output.push_str(&format!("        push @grep_filenames_{}, '{}';\n", command_index, file));
                        output.push_str("    }\n");
                        output.push_str("    close($fh);\n");
                        output.push_str("}\n");
                    }
                }
            }
        } else if input_var != "input_data" && !input_var.is_empty() {
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
        
        // Handle pattern source - either from command line or from file
        let regex_flags = if ignore_case { "i" } else { "" };
        let mut regex_pattern = String::new();
        
        if let Some(pattern_file_name) = &pattern_file {
            // Read patterns from file (-f option)
            output.push_str(&format!("my @patterns_{} = ();\n", command_index));
            
            // Check if this is a process substitution variable (starts with $)
            if pattern_file_name.starts_with('$') {
                output.push_str(&format!("if (-f {}) {{\n", pattern_file_name));
                output.push_str(&format!("    open(my $fh_{}, '<', {}) or die \"Cannot open pattern file: $!\";\n", command_index, pattern_file_name));
            } else {
                output.push_str(&format!("if (-f '{}') {{\n", pattern_file_name));
                output.push_str(&format!("    open(my $fh_{}, '<', '{}') or die \"Cannot open pattern file {}: $!\";\n", command_index, pattern_file_name, pattern_file_name));
            }
            
            output.push_str(&format!("    while (my $line = <$fh_{}>) {{\n", command_index));
            output.push_str(&format!("        chomp($line);\n"));
            output.push_str(&format!("        push @patterns_{}, $line if $line ne '';\n", command_index));
            output.push_str(&format!("    }}\n"));
            output.push_str(&format!("    close($fh_{});\n", command_index));
            output.push_str(&format!("}}\n"));
            
            // Apply grep filtering with multiple patterns
            output.push_str(&format!("my @grep_filtered_{} = ();\n", command_index));
            output.push_str(&format!("for my $line (@grep_lines_{}) {{\n", command_index));
            output.push_str(&format!("    my $match = 0;\n"));
            output.push_str(&format!("    for my $pattern (@patterns_{}) {{\n", command_index));
            if invert_match {
                output.push_str(&format!("        if ($line =~ /$pattern/{}) {{\n", regex_flags));
                output.push_str(&format!("            $match = 1;\n"));
                output.push_str(&format!("            last;\n"));
                output.push_str(&format!("        }}\n"));
                output.push_str(&format!("    }}\n"));
                output.push_str(&format!("    push @grep_filtered_{}, $line unless $match;\n", command_index));
            } else {
                output.push_str(&format!("        if ($line =~ /$pattern/{}) {{\n", regex_flags));
                output.push_str(&format!("            $match = 1;\n"));
                output.push_str(&format!("            last;\n"));
                output.push_str(&format!("        }}\n"));
                output.push_str(&format!("    }}\n"));
                output.push_str(&format!("    push @grep_filtered_{}, $line if $match;\n", command_index));
            }
            output.push_str(&format!("}}\n"));
        } else {
            // Use pattern from command line
            let escaped_pattern = pattern.to_string();
            // Remove quotes if they exist around the pattern
            regex_pattern = if escaped_pattern.starts_with('"') && escaped_pattern.ends_with('"') {
                escaped_pattern[1..escaped_pattern.len()-1].to_string()
            } else if escaped_pattern.starts_with("'") && escaped_pattern.ends_with("'") {
                escaped_pattern[1..escaped_pattern.len()-1].to_string()
            } else {
                escaped_pattern
            };
            
            // Convert shell regex patterns to Perl regex patterns
            // Convert \+ to + (shell extended regex to Perl)
            regex_pattern = regex_pattern.replace("\\+", "+");
            // Convert \? to ? (shell extended regex to Perl)
            regex_pattern = regex_pattern.replace("\\?", "?");
            // Convert \( and \) to ( and ) (shell extended regex to Perl)
            regex_pattern = regex_pattern.replace("\\(", "(");
            regex_pattern = regex_pattern.replace("\\)", ")");
            // Convert \{ and \} to { and } (shell extended regex to Perl)
            regex_pattern = regex_pattern.replace("\\{", "{");
            regex_pattern = regex_pattern.replace("\\}", "}");
            // Convert \| to | (shell extended regex to Perl)
            regex_pattern = regex_pattern.replace("\\|", "|");
            // Convert \. to . (shell extended regex to Perl) - but keep \. for literal dot
            // Actually, \. in shell regex means literal dot, so we should keep it as \. in Perl
            // No conversion needed for \.
            
            // Apply grep filtering
            if invert_match {
                // Negative grep: exclude lines that match the pattern
                output.push_str(&format!("my @grep_filtered_{} = grep !/{}/{}, @grep_lines_{};\n", command_index, regex_pattern, regex_flags, command_index));
            } else {
                // Positive grep: include lines that match the pattern
                output.push_str(&format!("my @grep_filtered_{} = grep /{}/{}, @grep_lines_{};\n", command_index, regex_pattern, regex_flags, command_index));
            }
        }
        
        // Apply max count if specified
        if let Some(max) = max_count {
            if max > 0 {
                output.push_str(&format!("@grep_filtered_{} = @grep_filtered_{}[0..{}];\n", command_index, command_index, max - 1));
            }
        }
        
        // Handle --missing option: find lines matching regex that are not in chunks
        if let Some(ref missing_regex) = missing_pattern {
            output.push_str(&format!("my @missing_lines_{} = ();\n", command_index));
            output.push_str(&format!("my @chunk_ranges_{} = ();\n", command_index));
            
            // First, identify chunks (consecutive lines with output.push_str or result.push_str calls)
            output.push_str(&format!("my $in_chunk_{} = 0;\n", command_index));
            output.push_str(&format!("my $chunk_start_{} = -1;\n", command_index));
            output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
            output.push_str(&format!("    if ($grep_lines_{}[$i] =~ /(output|result)\\.push_str\\(/) {{\n", command_index));
            output.push_str(&format!("        if (!$in_chunk_{}) {{\n", command_index));
            output.push_str(&format!("            $chunk_start_{} = $i;\n", command_index));
            output.push_str(&format!("            $in_chunk_{} = 1;\n", command_index));
            output.push_str("        }\n");
            output.push_str("    } else {\n");
            output.push_str(&format!("        if ($in_chunk_{}) {{\n", command_index));
            output.push_str(&format!("            push @chunk_ranges_{}, [$chunk_start_{}, $i - 1];\n", command_index, command_index));
            output.push_str(&format!("            $in_chunk_{} = 0;\n", command_index));
            output.push_str("        }\n");
            output.push_str("    }\n");
            output.push_str("}\n");
            output.push_str(&format!("if ($in_chunk_{}) {{\n", command_index));
            output.push_str(&format!("    push @chunk_ranges_{}, [$chunk_start_{}, $#grep_lines_{}];\n", command_index, command_index, command_index));
            output.push_str("}\n");
            
            // Now find lines that match the missing regex but are not in chunks
            output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
            output.push_str(&format!("    my $in_chunk_{} = 0;\n", command_index));
            output.push_str(&format!("    for my $range (@chunk_ranges_{}) {{\n", command_index));
            output.push_str(&format!("        if ($i >= $range->[0] && $i <= $range->[1]) {{\n"));
            output.push_str(&format!("            $in_chunk_{} = 1;\n", command_index));
            output.push_str("            last;\n");
            output.push_str("        }\n");
            output.push_str("    }\n");
            output.push_str(&format!("    if (!$in_chunk_{} && $grep_lines_{}[$i] =~ /{}/) {{\n", command_index, command_index, missing_regex));
            output.push_str(&format!("        push @missing_lines_{}, $grep_lines_{}[$i];\n", command_index, command_index));
            output.push_str("    }\n");
            output.push_str("}\n");
            
            // Replace the filtered results with missing lines
            output.push_str(&format!("@grep_filtered_{} = @missing_lines_{};\n", command_index, command_index));
        }
        
        // Generate output based on options
        if count_only {
            if has_file_args && recursive {
                // For recursive grep with -c, generate per-file counts
                output.push_str(&format!("my %file_counts_{};\n", command_index));
                output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
                output.push_str(&format!("    if (grep {{ $_ eq $grep_lines_{}[$i] }} @grep_filtered_{}) {{\n", command_index, command_index));
                output.push_str(&format!("        $file_counts_{}{{$grep_filenames_{}[$i]}}++;\n", command_index, command_index));
                output.push_str("    }\n");
                output.push_str("}\n");
                output.push_str(&format!("$grep_result_{} = '';\n", command_index));
                output.push_str(&format!("for my $file (sort keys %file_counts_{}) {{\n", command_index));
                output.push_str(&format!("    $grep_result_{} .= \"$file:$file_counts_{}{{$file}}\\n\";\n", command_index, command_index));
                output.push_str("}\n");
                output.push_str(&format!("$grep_result_{} =~ s/\\n$//; # Remove trailing newline\n", command_index));
            } else {
                output.push_str(&format!("$grep_result_{} = scalar(@grep_filtered_{});\n", command_index, command_index));
            }
            if should_print && !quiet_mode {
                output.push_str(&format!("print $grep_result_{};\n", command_index));
                output.push_str("print \"\\n\";\n");
            }
        } else if after_context > 0 || before_context > 0 || context_lines > 0 {
            // Handle context flags: -A, -B, -C
            let after = if context_lines > 0 { context_lines } else { after_context };
            let before = if context_lines > 0 { context_lines } else { before_context };
            
            output.push_str(&format!("my @grep_with_context_{};\n", command_index));
            output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
            output.push_str(&format!("    if (grep {{ $_ eq $grep_lines_{}[$i] }} @grep_filtered_{}) {{\n", command_index, command_index));
            // Add before context
            if before > 0 {
                output.push_str(&format!("        for (my $j = $i - {}; $j < $i; $j++) {{\n", before));
                output.push_str(&format!("            if ($j >= 0) {{\n"));
                output.push_str(&format!("                push @grep_with_context_{}, $grep_lines_{}[$j];\n", command_index, command_index));
                output.push_str("            }\n");
                output.push_str("        }\n");
            }
            // Add matching line
            output.push_str(&format!("        push @grep_with_context_{}, $grep_lines_{}[$i];\n", command_index, command_index));
            // Add after context
            if after > 0 {
                output.push_str(&format!("        for (my $j = $i + 1; $j <= $i + {} && $j < @grep_lines_{}; $j++) {{\n", after, command_index));
                output.push_str(&format!("            push @grep_with_context_{}, $grep_lines_{}[$j];\n", command_index, command_index));
                output.push_str("        }\n");
            }
            output.push_str("    }\n");
            output.push_str("}\n");
            output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_with_context_{});\n", command_index, command_index));
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
            // Handle -l flag: only show filenames that contain matches
            if has_file_args {
                if file_args[0].contains('*') {
                    // For glob patterns, output the actual filenames that contain matches
                    output.push_str(&format!("my @matching_files_{};\n", command_index));
                    output.push_str(&format!("my %file_has_match_{};\n", command_index));
                    output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
                    output.push_str(&format!("    if (grep {{ $_ eq $grep_lines_{}[$i] }} @grep_filtered_{}) {{\n", command_index, command_index));
                    output.push_str(&format!("        $file_has_match_{}{{$grep_filenames_{}[$i]}} = 1;\n", command_index, command_index));
                    output.push_str("    }\n");
                    output.push_str("}\n");
                    output.push_str(&format!("for my $file (sort keys %file_has_match_{}) {{\n", command_index));
                    output.push_str(&format!("    push @matching_files_{}, $file;\n", command_index));
                    output.push_str("}\n");
                    output.push_str(&format!("$grep_result_{} = join(\"\\n\", @matching_files_{});\n", command_index, command_index));
                } else {
                    output.push_str(&format!("$grep_result_{} = @grep_filtered_{} > 0 ? \"{}\" : \"\";\n", 
                        command_index, command_index, file_args[0]));
                }
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
        } else if files_without_match {
            // Handle -L flag: only show filenames that do NOT contain matches
            if has_file_args {
                if file_args[0].contains('*') {
                    // For glob patterns, output the actual filenames that do NOT contain matches
                    output.push_str(&format!("my @non_matching_files_{};\n", command_index));
                    output.push_str(&format!("my %file_has_match_{};\n", command_index));
                    output.push_str(&format!("my %all_files_{};\n", command_index));
                    
                    // First, collect all files that match the glob pattern
                    output.push_str(&format!("my @all_glob_files_{} = glob('{}');\n", command_index, file_args[0]));
                    output.push_str(&format!("for my $file (@all_glob_files_{}) {{\n", command_index));
                    output.push_str(&format!("    if (-f $file) {{\n"));
                    output.push_str(&format!("        $all_files_{}{{$file}} = 1;\n", command_index));
                    output.push_str("    }\n");
                    output.push_str("}\n");
                    
                    // Then, mark which ones have matches
                    output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
                    output.push_str(&format!("    if (grep {{ $_ eq $grep_lines_{}[$i] }} @grep_filtered_{}) {{\n", command_index, command_index));
                    output.push_str(&format!("        $file_has_match_{}{{$grep_filenames_{}[$i]}} = 1;\n", command_index, command_index));
                    output.push_str("    }\n");
                    output.push_str("}\n");
                    
                    // Finally, find files that don't have matches (sorted alphabetically like Bash)
                    output.push_str(&format!("for my $file (sort keys %all_files_{}) {{\n", command_index));
                    output.push_str(&format!("    if (!exists $file_has_match_{}{{$file}}) {{\n", command_index));
                    output.push_str(&format!("        push @non_matching_files_{}, $file;\n", command_index));
                    output.push_str("    }\n");
                    output.push_str("}\n");
                    output.push_str(&format!("$grep_result_{} = join(\"\\n\", @non_matching_files_{});\n", command_index, command_index));
                } else {
                    output.push_str(&format!("$grep_result_{} = @grep_filtered_{} == 0 ? \"{}\" : \"\";\n", 
                        command_index, command_index, file_args[0]));
                }
            } else {
                output.push_str(&format!("$grep_result_{} = @grep_filtered_{} == 0 ? \"(standard input)\" : \"\";\n", 
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
                if recursive {
                    output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
                    output.push_str(&format!("    if (grep {{ $_ eq $grep_lines_{}[$i] }} @grep_filtered_{}) {{\n", command_index, command_index));
                    output.push_str(&format!("        push @grep_with_filename_{}, \"$grep_filenames_{}[$i]:$grep_lines_{}[$i]\";\n", command_index, command_index, command_index));
                    output.push_str("    }\n");
                    output.push_str("}\n");
                } else {
                    output.push_str(&format!("for my $line (@grep_filtered_{}) {{\n", command_index));
                    output.push_str(&format!("    push @grep_with_filename_{}, \"{}:$line\";\n", command_index, file_args[0]));
                    output.push_str("}\n");
                }
                output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_with_filename_{});\n", command_index, command_index));
                    } else {
            // Default: just output matching lines (handles -h flag implicitly by not adding filename)
            if recursive && has_file_args {
                // For recursive search, show filename:content format
                output.push_str(&format!("my @grep_with_filename_{};\n", command_index));
                output.push_str(&format!("for (my $i = 0; $i < @grep_lines_{}; $i++) {{\n", command_index));
                output.push_str(&format!("    if (grep {{ $_ eq $grep_lines_{}[$i] }} @grep_filtered_{}) {{\n", command_index, command_index));
                output.push_str(&format!("        push @grep_with_filename_{}, \"$grep_filenames_{}[$i]:$grep_lines_{}[$i]\";\n", command_index, command_index, command_index));
                output.push_str("    }\n");
                output.push_str("}\n");
                output.push_str(&format!("$grep_result_{} = join(\"\\n\", @grep_with_filename_{});\n", command_index, command_index));
            } else if color_always {
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
        
        // Set exit status for all grep commands
        // For quiet mode, set exit code based on whether matches were found
        output.push_str(&format!("$? = scalar(@grep_filtered_{}) > 0 ? 0 : 1;\n", command_index));
    
    output
}
