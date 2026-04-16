use super::Generator;
use crate::ast::*;

/// Get the appropriate temporary directory for the current platform
pub fn get_temp_dir() -> &'static str {
    // On Windows, use $TEMP, otherwise use /tmp
    if cfg!(target_os = "windows") {
        "($ENV{TEMP} || $ENV{TMP} || \"C:\\\\temp\")"
    } else {
        "q{/tmp}"
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
            let elements: Vec<String> = content.split_whitespace().map(|s| s.to_string()).collect();
            return Some(elements);
        }
    }
    None
}

pub fn perl_string_literal_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            if s.contains("system") || s.contains('`') {
                return crate::generator::commands::utilities::source_safe_perl_string_expr(s);
            }

            // Handle empty strings with q{}
            if s.is_empty() {
                return "q{}".to_string();
            }

            // Use double quotes when we need escape sequences (newlines,
            // tabs, carriage returns) or when the string contains backslashes
            // or embedded double quotes that must be escaped. Avoid forcing
            // double-quoted strings simply because the content contains
            // dollar or at-sign characters; those are often shell code or
            // awk programs and should not be interpolated by Perl.
            if s.contains('\n')
                || s.contains('\t')
                || s.contains('\r')
                || s.contains('\\')
                || s.contains('"')
            {
                let escaped = s
                    .replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\n")
                    .replace("\t", "\\t")
                    .replace("\r", "\\r");
                format!("\"{}\"", escaped)
            } else {
                // Use q{} for single characters to avoid "noisy quotes" violations
                if s.len() == 1 {
                    // Always use q{} for single characters to avoid Perl::Critic violations
                    format!("q{{{}}}", s)
                } else {
                    let escaped = s.replace("\\", "\\\\").replace("'", "\\'");
                    format!("'{}'", escaped)
                }
            }
        }
        Word::Variable(var, _, _) => {
            // Handle special shell variables
            match var.as_str() {
                "#" => "scalar(@ARGV)".to_string(), // $# -> scalar(@ARGV) for argument count
                "@" => "@ARGV".to_string(),         // $@ -> @ARGV for arguments array
                "0" => "$PROGRAM_NAME".to_string(), // $0 -> $PROGRAM_NAME (Perl::Critic compliant)
                _ => format!("${}", var),           // Regular variables
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
                            let perl_code =
                                crate::generator::commands::ls::generate_ls_for_substitution(
                                    generator, simple_cmd,
                                );

                            // For backtick commands, we need to return the value, not print it
                            // The generate_ls_for_substitution already returns the joined string
                            perl_code
                        } else if name == "find" {
                            // Use the find command handler for proper conversion
                            let perl_code = crate::generator::commands::find::generate_find_command(
                                generator,
                                simple_cmd,
                                true,
                                "found_files",
                            );

                            // For backtick commands, we need to return the value, not print it
                            // The generate_find_command already returns the joined string
                            perl_code
                        } else if name == "yes" {
                            // Special handling for yes command in command substitution
                            let string_to_repeat = if let Some(arg) = simple_cmd.args.first() {
                                generator.perl_string_literal(arg)
                            } else {
                                "\"y\"".to_string()
                            };

                            // Generate a limited number of lines for command substitution
                            format!("do {{ my $string = {}; my $output = q{{}}; for my $i (0..999) {{ $output .= \"$string\\n\"; }} $output; }}", string_to_repeat)
                        } else if name == "echo" {
                            // Special handling for echo in command substitution
                            if simple_cmd.args.is_empty() {
                                "\"\\n\"".to_string()
                            } else {
                                // Process arguments with proper string interpolation handling
                                let args: Vec<String> = simple_cmd
                                    .args
                                    .iter()
                                    .map(|arg| {
                                        match arg {
                                            Word::StringInterpolation(interp, _) => generator
                                                .convert_string_interpolation_to_perl(interp),
                                            Word::Literal(literal, _) => {
                                                // Escaped backticks should be treated as literal backticks, not command substitution
                                                generator.perl_string_literal(arg)
                                            }
                                            _ => generator.word_to_perl(arg),
                                        }
                                    })
                                    .collect();
                                format!("({}) . \"\\n\"", args.join(" . q{ } . "))
                            }
                        } else if name == "sha256sum" {
                            // Use the sha256sum command handler for proper conversion
                            crate::generator::commands::sha256sum::generate_sha256sum_command(
                                generator, simple_cmd, "",
                            )
                        } else if name == "sha512sum" {
                            // Use the sha512sum command handler for proper conversion
                            crate::generator::commands::sha512sum::generate_sha512sum_command(
                                generator, simple_cmd, "",
                            )
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
                                            format_string = interp
                                                .parts
                                                .iter()
                                                .map(|part| match part {
                                                    StringPart::Literal(s) => s.clone(),
                                                    _ => "".to_string(), // Skip variables in format strings for now
                                                })
                                                .collect::<Vec<_>>()
                                                .join("");
                                        }
                                        Word::Literal(s, _) => {
                                            format_string = s.clone();
                                        }
                                        _ => {
                                            format_string = generator.word_to_perl(arg);
                                        }
                                    }
                                    // Remove quotes if they exist around the format string
                                    if format_string.starts_with('\'')
                                        && format_string.ends_with('\'')
                                    {
                                        format_string =
                                            format_string[1..format_string.len() - 1].to_string();
                                    } else if format_string.starts_with('"')
                                        && format_string.ends_with('"')
                                    {
                                        format_string =
                                            format_string[1..format_string.len() - 1].to_string();
                                    }
                                } else {
                                    args.push(generator.word_to_perl(arg));
                                }
                            }

                            if format_string.is_empty() {
                                "\"\"".to_string()
                            } else {
                                let formatted_args = args
                                    .iter()
                                    .map(|arg| {
                                        generator.perl_string_literal(&Word::Literal(
                                            arg.clone(),
                                            Default::default(),
                                        ))
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!(
                                    "sprintf \"{}\", {}",
                                    format_string.replace("\"", "\\\"").replace("\\\\", "\\"),
                                    formatted_args
                                )
                            }
                        } else if name == "date" {
                            format!(
                                "do {{\n{}\n}}",
                                crate::generator::commands::date::generate_date_expression(
                                    generator, simple_cmd,
                                )
                            )
                        } else if name == "pwd" {
                            // Special handling for pwd in command substitution
                            "do { use Cwd; getcwd(); }".to_string()
                        } else if name == "basename" {
                            // Run basename via the host command so output and edge cases match.
                            let basename_cmd = generator.generate_command_string_for_system(
                                &Command::Simple(simple_cmd.clone()),
                            );
                            let basename_lit =
                                generator.perl_string_literal(&Word::literal(basename_cmd));
                            format!("do {{ my $basename_cmd = {}; my $basename_output = qx{{$basename_cmd}}; $CHILD_ERROR = $? >> 8; $basename_output; }}", basename_lit)
                        } else if name == "dirname" {
                            let dirname_cmd = generator.generate_command_string_for_system(
                                &Command::Simple(simple_cmd.clone()),
                            );
                            let dirname_lit =
                                generator.perl_string_literal(&Word::literal(dirname_cmd));
                            format!("do {{ my $dirname_cmd = {}; my $dirname_output = qx{{$dirname_cmd}}; $CHILD_ERROR = $? >> 8; $dirname_output; }}", dirname_lit)
                        } else if name == "which" {
                            // Use the real which command so flags and exit codes match the host tool.
                            let which_cmd = generator.generate_command_string_for_system(cmd);
                            let which_lit =
                                generator.perl_string_literal(&Word::literal(which_cmd));
                            format!("do {{ my $which_cmd = {}; my $which_output = qx{{$which_cmd}}; $CHILD_ERROR = $? >> 8; $which_output; }}", which_lit)
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
                        } else if name == "time" {
                            // Special handling for time in command substitution
                            // Use custom time implementation instead of open3
                            let mut time_output = String::new();
                            time_output.push_str("use Time::HiRes qw(gettimeofday tv_interval);\n");
                            time_output.push_str("my $start_time = [gettimeofday];\n");

                            // Execute the command (if any arguments provided)
                            if !simple_cmd.args.is_empty() {
                                let args: Vec<String> = simple_cmd
                                    .args
                                    .iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect();
                                let command_str = args.join(" ");
                                time_output.push_str(&format!("system {};\n", command_str));
                            }

                            time_output.push_str("my $end_time = [gettimeofday];\n");
                            time_output
                                .push_str("my $elapsed = tv_interval($start_time, $end_time);\n");
                            time_output.push_str(
                                "sprintf \"real %.3fs\\nuser 0.000s\\nsys 0.000s\\n\", $elapsed;\n",
                            );

                            format!("do {{ {} }}", time_output)
                        } else {
                            // For non-builtin commands, use open3 to capture output without backticks
                            let args: Vec<String> = simple_cmd
                                .args
                                .iter()
                                .map(|arg| generator.word_to_perl(arg))
                                .collect();

                            let (in_var, out_var, err_var, pid_var, result_var) =
                                generator.get_unique_ipc_vars();
                            if args.is_empty() {
                                format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, '{}'); close {} or croak 'Close failed: $OS_ERROR'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $OS_ERROR'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, name, in_var, result_var, out_var, out_var, pid_var, result_var)
                            } else {
                                format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, '{}', {}); close {} or croak 'Close failed: $OS_ERROR'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $OS_ERROR'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, name, args.iter().map(|arg| format!("'{}'", arg)).collect::<Vec<_>>().join(", "), in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        }
                    } else {
                        // For non-literal command names, use open3 to capture output without backticks
                        let cmd_name = generator.word_to_perl(&simple_cmd.name);
                        let args: Vec<String> = simple_cmd
                            .args
                            .iter()
                            .map(|arg| generator.word_to_perl(arg))
                            .collect();

                        let (in_var, out_var, err_var, pid_var, result_var) =
                            generator.get_unique_ipc_vars();
                        if args.is_empty() {
                            format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, {}); close {} or croak 'Close failed: $OS_ERROR'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $OS_ERROR'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
                        } else {
                            format!("do {{ my ({}, {}, {}); my {} = open3({}, {}, {}, {}, {}); close {} or croak 'Close failed: $OS_ERROR'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $OS_ERROR'; waitpid {}, 0; {} }}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, args.iter().map(|arg| format!("'{}'", arg)).collect::<Vec<_>>().join(", "), in_var, result_var, out_var, out_var, pid_var, result_var)
                        }
                    }
                }
                Command::Pipeline(pipeline) => {
                    // For command substitution pipelines, use the specialized function
                    // Wrap in do block for utils context
                    format!("do {{ {} }}", crate::generator::commands::pipeline_commands::generate_pipeline_for_substitution(generator, pipeline))
                }
                _ => {
                    // For other command types, use system command fallback
                    let (in_var, out_var, err_var, pid_var, result_var) =
                        generator.get_unique_ipc_vars();
                    // Ensure the command string is embedded as a non-interpolating
                    // Perl literal so embedded single quotes or "$" sequences
                    // (e.g. awk programs containing $0) are preserved verbatim and
                    // not interpreted by the generated Perl code.
                    let cmd_str = generator.generate_command_string_for_system(cmd);
                    let cmd_lit = generator.perl_string_literal_no_interp(&Word::literal(cmd_str));
                    format!(" my ({}, {}, {}); my {} = open3({}, {}, {}, 'bash', '-c', {}); close {} or croak 'Close failed: $OS_ERROR'; my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }}; close {} or croak 'Close failed: $OS_ERROR'; waitpid {}, 0; {}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_lit, in_var, result_var, out_var, out_var, pid_var, result_var)
                }
            }
        }
        _ => format!("{:?}", word),
    }
}

/// Emit a Perl string literal that never interpolates (no "$" or "\\n" processing).
/// This is used for shell snippets that will later be passed to qx{} so the
/// exact byte-for-byte contents must be preserved.
pub fn perl_string_literal_no_interp_impl(_generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Empty string -> q{} is compact and safe
            if s.is_empty() {
                return "q{}".to_string();
            }

            // Prefer a single-quoted literal when the content has no single
            // quotes and is a simple one-line value. This keeps generated
            // output readable. For strings that contain single quotes or
            // embedded newlines prefer Perl's q{}-style non-interpolating
            // operator which can contain single quotes and newlines safely.
            let contains_single_quote = s.contains('\'');
            let contains_newline = s.contains('\n');

            if !contains_single_quote && !contains_newline {
                // Escape backslashes and single quotes conservatively
                let escaped = s.replace("\\", "\\\\").replace("'", "\\'");
                return format!("'{}'", escaped);
            }

            // Otherwise try a variety of delimiter pairs for q<delim>...<delim>
            // Choose a pair where neither the open nor close delimiter appears
            // in the content. This preserves the literal bytes (including newlines)
            // without requiring interpolation or escape processing.
            let delimiters = vec![
                ('{', '}'),
                ('(', ')'),
                ('[', ']'),
                ('<', '>'),
                ('|', '|'),
                ('/', '/'),
                ('#', '#'),
                ('%', '%'),
                ('@', '@'),
                ('!', '!'),
                ('~', '~'),
                ('^', '^'),
                (':', ':'),
                (';', ';'),
            ];

            for (open, close) in delimiters {
                let open_s = open.to_string();
                let close_s = close.to_string();
                if !s.contains(&open_s) && !s.contains(&close_s) {
                    return format!("q{}{}{}", open, s, close);
                }
            }

            // If every candidate delimiter appears in the string (rare),
            // fall back to a double-quoted literal with explicit escaping.
            // Double-quoting is safe because we properly escape backslashes,
            // quotes and control characters.
            let escaped = s
                .replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\n")
                .replace("\t", "\\t")
                .replace("\r", "\\r");
            format!("\"{}\"", escaped)
        }
        _ => perl_string_literal_impl(_generator, word),
    }
}

pub fn strip_shell_quotes_and_convert_to_perl_impl(
    generator: &mut Generator,
    word: &Word,
) -> String {
    match word {
        Word::Literal(s, _) => {
            // Strip shell quotes if present and convert to Perl string literal
            let stripped = if (s.starts_with("'") && s.ends_with("'"))
                || (s.starts_with("\"") && s.ends_with("\""))
            {
                // Remove the outer quotes
                &s[1..s.len() - 1]
            } else {
                s
            };

            // Handle empty strings with q{}
            if stripped.is_empty() {
                return "q{}".to_string();
            }

            // Check if string needs escape processing for use in double-quoted
            // Perl literals. We avoid treating '$' and '@' as a reason to force
            // double-quoting because those characters commonly appear in shell
            // fragments (awk/sed programs, etc.) and should not trigger Perl
            // interpolation.
            let needs_double_quoted = stripped.contains('\\')
                || stripped.contains('\n')
                || stripped.contains('\t')
                || stripped.contains('\r')
                || stripped.contains('"');

            if needs_double_quoted {
                // Escape quotes and backslashes for Perl string literals
                let escaped = stripped
                    .replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\n")
                    .replace("\t", "\\t")
                    .replace("\r", "\\r");
                format!("\"{}\"", escaped)
            } else {
                // Use q{} for single characters to avoid "noisy quotes" violations
                if stripped.len() == 1
                    && !stripped.contains('\'')
                    && !stripped.contains('{')
                    && !stripped.contains('}')
                {
                    format!("q{{{}}}", stripped)
                } else if stripped.len() == 1 && stripped.contains('\'') {
                    // Handle single quotes in single character strings
                    format!("q{{{}}}", stripped)
                } else {
                    // Use single quotes for strings that don't need interpolation
                    let escaped = stripped.replace("\\", "\\\\").replace("'", "\\'");
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
        _ => format!("{:?}", word),
    }
}

pub fn strip_shell_quotes_for_regex_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Strip shell quotes if present and return the raw string for regex
            if (s.starts_with("'") && s.ends_with("'"))
                || (s.starts_with("\"") && s.ends_with("\""))
            {
                // Remove the outer quotes
                s[1..s.len() - 1].to_string()
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
        _ => format!("{:?}", word),
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
    pattern
        .replace("\\.", "[.]")
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
        .replace("\\t", "\t")
        .replace("\\r", "\r")
        .replace("{", "\\{")
        .replace("}", "\\}")
}

/// Decode common shell-style escape sequences in a string literal.
/// Converts sequences like "\\n", "\\t", "\\r", "\\\\",
/// "\\\"" and "\\'" into their actual characters. Unknown escape
/// sequences are replaced by the character following the backslash.
pub fn decode_shell_escapes_impl(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                match n {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    other => out.push(other),
                }
            } else {
                // Trailing backslash - preserve it
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Generate a regex pattern for checking if string ends with newline
pub fn newline_end_regex() -> String {
    // Use a regex pattern that matches actual newline characters
    // Use \z so we only match a true trailing newline, not any newline in a multiline string.
    "m{\\n\\z}msx".to_string()
}

/// Convert postfix unless statement to block form
pub fn convert_postfix_unless_to_block(condition: &str, statement: &str) -> String {
    format!("if (!({})) {{\n    {};\n}}", condition, statement)
}

/// Convert postfix unless statement to block form with proper indentation
pub fn convert_postfix_unless_to_block_with_indent(
    condition: &str,
    statement: &str,
    indent: &str,
) -> String {
    format!(
        "{}if (!({})) {{\n{}    {};\n{}}}",
        indent, condition, indent, statement, indent
    )
}

/// Convert postfix unless statement to block form without adding indentation (for use within already indented blocks)
pub fn convert_postfix_unless_to_block_no_indent(condition: &str, statement: &str) -> String {
    format!("if (!({})) {{\n    {};\n}}", condition, statement)
}
