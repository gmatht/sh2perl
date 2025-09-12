use crate::ast::*;
use super::Generator;
use regex::Regex;

/// Get the appropriate temporary directory for the current platform
pub fn get_temp_dir() -> &'static str {
    // On Windows, use $TEMP, otherwise use /tmp
    if cfg!(target_os = "windows") {
        "($ENV{TEMP} || $ENV{TMP} || \"C:\\\\temp\")"
    } else {
        "/tmp"
    }
}

pub fn extract_array_key_impl(var: &str) -> Option<(String, String)> {
    // Check if this is an associative array assignment like map[foo]=bar
    if let Some(bracket_start) = var.find('[') {
        if let Some(bracket_end) = var.find(']') {
            if bracket_start < bracket_end {
                let array_name = var[..bracket_start].to_string();
                let key = var[bracket_start + 1..bracket_end].to_string();
                return Some((array_name, key));
            }
        }
    }
    None
}

pub fn extract_array_elements_impl(value: &str) -> Option<Vec<String>> {
    // Check if this is an indexed array assignment like arr=(one two three)
    if value.starts_with('(') && value.ends_with(')') {
        let content = &value[1..value.len() - 1];
        if !content.is_empty() {
            let elements: Vec<String> = content
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            return Some(elements);
        }
    }
    None
}

pub fn perl_string_literal_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Handle empty strings with q{}
            if s.is_empty() {
                return "q{}".to_string();
            }
            
            // Check if string needs interpolation (contains variables or special chars)
            let needs_interpolation = s.contains('$') || s.contains('@') || s.contains('\\') || s.contains('`');
            
            if needs_interpolation {
                // Escape quotes and backslashes for Perl string literals
                let escaped = s.replace("\\", "\\\\")
                              .replace("\"", "\\\"")
                              .replace("`", "\\`")
                              .replace("\n", "\\n")
                              .replace("\t", "\\t")
                              .replace("\r", "\\r");
                format!("\"{}\"", escaped)
            } else {
                // Check if string contains newlines, tabs, or carriage returns
                // If it does, we need to use double quotes with escape sequences
                if s.contains('\n') || s.contains('\t') || s.contains('\r') {
                    // Use double quotes and escape special characters
                    let escaped = s.replace("\\", "\\\\")
                                  .replace("\"", "\\\"")
                                  .replace("\n", "\\n")
                                  .replace("\t", "\\t")
                                  .replace("\r", "\\r");
                    format!("\"{}\"", escaped)
                } else {
                    // Use q{} for single characters to avoid "noisy quotes" violations
                    // Use single quotes for longer strings that don't need interpolation
                    if s.len() == 1 {
                        // Always use q{} for single characters to avoid Perl::Critic violations
                        format!("q{{{}}}", s)
                    } else {
                        let escaped = s.replace("\\", "\\\\")
                                      .replace("'", "\\'");
                        format!("'{}'", escaped)
                    }
                }
            }
        }
        Word::Variable(var, _, _) => {
            // Handle special shell variables
            match var.as_str() {
                "#" => "scalar(@ARGV)".to_string(),  // $# -> scalar(@ARGV) for argument count
                "@" => "@ARGV".to_string(),          // $@ -> @ARGV for arguments array
                _ => format!("${}", var)             // Regular variables
            }
        }
        Word::Arithmetic(expr, _) => {
            // Handle arithmetic expressions by converting them to Perl
            generator.convert_arithmetic_to_perl(&expr.expression)
        }
        Word::ParameterExpansion(pe, _) => {
            // Handle parameter expansion
            generator.generate_parameter_expansion(pe)
        }
        Word::StringInterpolation(interp, _) => {
            // Handle string interpolation
            generator.convert_string_interpolation_to_perl(interp)
        }
        Word::CommandSubstitution(cmd, _) => {
            // Handle command substitution - always convert to native Perl, never use backticks
            match cmd.as_ref() {
                Command::Simple(simple_cmd) => {
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
                                format!("({}) . \"\\n\"", args.join(" . q{ } . "))
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
                                format!("sprintf {}, {}", 
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
                                    "q{}".to_string()
                                };
                                format!("do {{ my $path = {}; my $suffix = {}; if ($suffix ne q{{}}) {{ $path =~ s/\\Q$suffix\\E$//msx; }} $path =~ s/.*\\///msx; $path; }}", path_str.replace("$0", "$PROGRAM_NAME"), suffix)
                            } else {
                                "\".\"".to_string()
                            }
                        } else if name == "dirname" {
                            // Special handling for dirname in command substitution
                            if let Some(path) = simple_cmd.args.first() {
                                let path_str = generator.word_to_perl(path);
                                format!("do {{ my $path = {}; if ($path =~ /\\//msx) {{ $path =~ s/\\/[^\\/]*$//msx; if ($path eq q{{}}) {{ $path = q{{.}}; }} }} else {{ $path = q{{.}}; }} $path; }}", path_str.replace("$0", "$PROGRAM_NAME"))
                            } else {
                                "\".\"".to_string()
                            }
                        } else if name == "which" {
                            // Special handling for which in command substitution
                            if let Some(command) = simple_cmd.args.first() {
                                let command_str = generator.word_to_perl(command);
                                format!("do {{ my $command = {}; my $found = 0; my $result = q{{}}; foreach my $dir (split /:/msx, $ENV{{PATH}}) {{ my $full_path = \"$dir/$command\"; if (-x $full_path) {{ $result = $full_path; $found = 1; last; }} }} $result; }}", command_str)
                            } else {
                                "q{}".to_string()
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
                        } else {
                            // For non-builtin commands, use open3 to capture output without backticks
                            let args: Vec<String> = simple_cmd.args.iter()
                                .map(|arg| generator.word_to_perl(arg))
                                .collect();
                            
                            let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                            if args.is_empty() {
                                format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, '{}'); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, name, in_var, result_var, out_var, out_var, pid_var, result_var)
                            } else {
                                format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, '{}', {}); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, name, args.iter().map(|arg| format!("'{}'", arg)).collect::<Vec<_>>().join(", "), in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        }
                    } else {
                        // For non-literal command names, use open3 to capture output without backticks
                        let cmd_name = generator.word_to_perl(&simple_cmd.name);
                        let args: Vec<String> = simple_cmd.args.iter()
                            .map(|arg| generator.word_to_perl(arg))
                            .collect();
                        
                        let (in_var, out_var, err_var, pid_var, result_var) = generator.get_unique_ipc_vars();
                        if args.is_empty() {
                            format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, {}); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
                        } else {
                            format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, {}, {}); close {} or croak 'Close failed: $!'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $!'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, args.iter().map(|arg| format!("'{}'", arg)).collect::<Vec<_>>().join(", "), in_var, result_var, out_var, out_var, pid_var, result_var)
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
        }
        _ => format!("{:?}", word)
    }
}

pub fn strip_shell_quotes_and_convert_to_perl_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Strip shell quotes if present and convert to Perl string literal
            let stripped = if (s.starts_with("'") && s.ends_with("'")) || (s.starts_with("\"") && s.ends_with("\"")) {
                // Remove the outer quotes
                &s[1..s.len()-1]
            } else {
                s
            };
            
            // Handle empty strings with q{}
            if stripped.is_empty() {
                return "q{}".to_string();
            }
            
            // Check if string needs interpolation (contains variables or special chars)
            let needs_interpolation = stripped.contains('$') || stripped.contains('@') || stripped.contains('\\');
            
            if needs_interpolation {
                // Escape quotes and backslashes for Perl string literals
                let escaped = stripped.replace("\\", "\\\\")
                                    .replace("\"", "\\\"")
                                    .replace("\n", "\\n")
                                    .replace("\t", "\\t")
                                    .replace("\r", "\\r");
                format!("\"{}\"", escaped)
            } else {
                // Use q{} for single characters to avoid "noisy quotes" violations
                if stripped.len() == 1 && !stripped.contains('\'') && !stripped.contains('{') && !stripped.contains('}') {
                    format!("q{{{}}}", stripped)
                } else if stripped.len() == 1 && stripped.contains('\'') {
                    // Handle single quotes in single character strings
                    format!("q{{{}}}", stripped)
                } else {
                    // Use single quotes for strings that don't need interpolation
                    let escaped = stripped.replace("\\", "\\\\")
                                        .replace("'", "\\'");
                    format!("'{}'", escaped)
                }
            }
        }
        Word::Arithmetic(expr, _) => {
            // Handle arithmetic expressions by converting them to Perl
            generator.convert_arithmetic_to_perl(&expr.expression)
        }
        Word::ParameterExpansion(pe, _) => {
            // Handle parameter expansion
            generator.generate_parameter_expansion(pe)
        }
        Word::StringInterpolation(interp, _) => {
            // Handle string interpolation
            generator.convert_string_interpolation_to_perl(interp)
        }
        _ => format!("{:?}", word)
    }
}

pub fn strip_shell_quotes_for_regex_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Strip shell quotes if present and return the raw string for regex
            if (s.starts_with("'") && s.ends_with("'")) || (s.starts_with("\"") && s.ends_with("\"")) {
                // Remove the outer quotes
                s[1..s.len()-1].to_string()
            } else {
                s.clone()
            }
        }
        Word::Arithmetic(expr, _) => {
            // Handle arithmetic expressions by converting them to Perl
            generator.convert_arithmetic_to_perl(&expr.expression)
        }
        Word::ParameterExpansion(pe, _) => {
            // Handle parameter expansion
            generator.generate_parameter_expansion(pe)
        }
        Word::StringInterpolation(interp, _) => {
            // For regex, we need the raw content without quotes
            // For simple string interpolations with just literals, extract the raw content
            if interp.parts.len() == 1 {
                if let StringPart::Literal(s) = &interp.parts[0] {
                    // Convert shell regex patterns to Perl regex patterns
                    let mut regex_pattern = s.clone();
                    
                    // Convert shell extended regex patterns to Perl patterns
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
                    
                    // Return the converted regex pattern
                    regex_pattern
                } else {
                    // Fall back to normal string interpolation handling
                    generator.convert_string_interpolation_to_perl(interp)
                }
            } else {
                // Fall back to normal string interpolation handling
                generator.convert_string_interpolation_to_perl(interp)
            }
        }
        _ => format!("{:?}", word)
    }
}

pub fn get_unique_file_handle_impl(generator: &mut Generator) -> String {
    generator.file_handle_counter += 1;
    format!("fh_{}", generator.file_handle_counter)
}

/// Generate a properly formatted regex pattern with appropriate flags
pub fn format_regex_pattern(pattern: &str) -> String {
    // Convert escaped metacharacters to character classes for better Perl::Critic compliance
    let converted_pattern = convert_escaped_metacharacters(pattern);
    // Add common flags: /s for dot matching newlines, /x for extended formatting, /m for multiline
    format!("/{}/msx", converted_pattern)
}

/// Convert escaped metacharacters to character classes for better Perl::Critic compliance
pub fn convert_escaped_metacharacters(pattern: &str) -> String {
    pattern.replace("\\.", "[.]")
           .replace("\\+", "[+]")
           .replace("\\*", "[*]")
           .replace("\\?", "[?]")
           .replace("\\^", "[^]")
           .replace("\\$", "[$]")
           .replace("\\[", "[\\[]")
           .replace("\\]", "[\\]]")
           .replace("\\(", "[(]")
           .replace("\\)", "[)]")
           .replace("\\|", "[|]")
}

/// Generate a regex pattern for checking if string ends with newline
pub fn newline_end_regex() -> String {
    format_regex_pattern(r"\n$")
}

/// Convert postfix unless statement to block form
pub fn convert_postfix_unless_to_block(condition: &str, statement: &str) -> String {
    format!("if (!({}) ) {{\n    {};\n}}", condition, statement)
}

/// Convert postfix unless statement to block form with proper indentation
pub fn convert_postfix_unless_to_block_with_indent(condition: &str, statement: &str, indent: &str) -> String {
    format!("{}if (!({}) ) {{\n{}    {};\n{}}}", indent, condition, indent, statement, indent)
}

/// Convert postfix unless statement to block form without adding indentation (for use within already indented blocks)
pub fn convert_postfix_unless_to_block_no_indent(condition: &str, statement: &str) -> String {
    format!("if (!({}) ) {{\n    {};\n}}", condition, statement)
}
