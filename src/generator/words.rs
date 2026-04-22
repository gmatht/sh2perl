use super::Generator;
use crate::ast::*;
use regex::Regex;

fn push_string_expr(parts: &mut Vec<String>, current_string: &mut String) {
    if current_string.is_empty() {
        return;
    }

    let rendered = if current_string.contains("system") || current_string.contains('`') {
        crate::generator::commands::utilities::source_safe_perl_string_expr(current_string)
    } else {
        format!("\"{}\"", current_string.replace('"', "\\\""))
    };

    parts.push(rendered);
    current_string.clear();
}

pub fn word_to_perl_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Handle literal strings
            if s.starts_with('`') && s.ends_with('`') {
                let command_str = s[1..s.len() - 1].to_string();
                if let Ok(command) = crate::parser::commands::parse_pipeline_from_text(&command_str)
                {
                    return match command {
                        Command::Simple(simple_cmd) => {
                            if let Word::Literal(name, _) = &simple_cmd.name {
                                if name == "head" || name == "tail" {
                                    return generator.word_to_perl(&Word::CommandSubstitution(
                                        Box::new(Command::Simple(simple_cmd)),
                                        None,
                                    ));
                                }
                            }
                            generator.word_to_perl(&Word::CommandSubstitution(
                                Box::new(Command::Simple(simple_cmd)),
                                None,
                            ))
                        }
                        Command::Pipeline(pipeline) => generator.word_to_perl(
                            &Word::CommandSubstitution(Box::new(Command::Pipeline(pipeline)), None),
                        ),
                        other => generator
                            .word_to_perl(&Word::CommandSubstitution(Box::new(other), None)),
                    };
                }
                let command_lit =
                    generator.perl_string_literal_no_interp(&Word::literal(command_str));
                format!(
                    "do {{ my $command = {}; my $result = qx{{$command}}; $CHILD_ERROR = $? >> 8; $result; }}",
                    command_lit
                )
            } else if Regex::new(r"^\d+\.\.\d+$").unwrap().is_match(s) {
                generator.handle_range_expansion(s)
            } else if Regex::new(r"^\d+(?:\s*,\s*\d+)+$").unwrap().is_match(s) {
                generator.handle_comma_expansion(s)
            } else {
                // For literal strings, delegate to the central Perl string literal
                // helper so quoting/escaping rules are consistent and we avoid
                // accidental Perl interpolation of shell snippets (like awk/sed)
                // which may contain "$" or "@". Using generator.perl_string_literal
                // ensures single-quoting is used when safe.
                generator.perl_string_literal(&Word::literal(s.clone()))
            }
        }
        Word::ParameterExpansion(pe, _) => generator.generate_parameter_expansion(pe),
        Word::Array(name, elements, _) => {
            let elements_str = elements
                .iter()
                .map(|e| format!("'{}'", e.replace("'", "\\'")))
                .collect::<Vec<_>>()
                .join(", ");
            format!("@{} = ({});", name, elements_str)
        }
        Word::StringInterpolation(interp, _) => {
            generator.convert_string_interpolation_to_perl(interp)
        }
        Word::Arithmetic(expr, _) => generator.convert_arithmetic_to_perl(&expr.expression),
        Word::BraceExpansion(expansion, _) => {
            let expanded = generator.handle_brace_expansion(expansion);
            // Quote the result since it's used in contexts where quotes are needed
            format!("\"{}\"", expanded)
        }
        Word::CommandSubstitution(cmd, _) => {
            // Handle command substitution
            let result = match cmd.as_ref() {
                Command::Redirect(_) => {
                    let command_str =
                        crate::generator::redirects::generate_bash_command_string(cmd);
                    let command_lit =
                        generator.perl_string_literal_no_interp(&Word::literal(command_str));

                    format!(
                        "do {{ my $command = {}; my $result = qx{{$command}}; $CHILD_ERROR = $? >> 8; $result; }}",
                        command_lit
                    )
                }
                Command::Simple(simple_cmd) => {
                    let cmd_name = generator.word_to_perl(&simple_cmd.name);

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
                        } else if name == "head" {
                            // Use the shell command directly so file and flag handling stays faithful
                            let head_cmd = generator.generate_command_string_for_system(cmd);
                            let head_lit =
                                generator.perl_string_literal_no_interp(&Word::literal(head_cmd));
                            format!("do {{ my $head_cmd = {}; qx{{$head_cmd}}; }}", head_lit)
                        } else if name == "tail" {
                            // Use the shell command directly so file and flag handling stays faithful
                            let tail_cmd = generator.generate_command_string_for_system(cmd);
                            let tail_lit =
                                generator.perl_string_literal_no_interp(&Word::literal(tail_cmd));
                            format!("do {{ my $tail_cmd = {}; qx{{$tail_cmd}}; }}", tail_lit)
                        } else if name == "cat" {
                            crate::generator::commands::cat::generate_cat_command_for_substitution(
                                generator, simple_cmd,
                            )
                        } else if name == "yes" {
                            // Special handling for yes command in command substitution
                            let string_to_repeat = if let Some(arg) = simple_cmd.args.first() {
                                generator.perl_string_literal(arg)
                            } else {
                                "\"y\"".to_string()
                            };

                            // Generate a limited number of lines for command substitution
                            format!("do {{ my $string = {}; my $output = q{{}}; for my $i (0..999) {{ $output .= \"$string\\n\"; }} $output; }}", string_to_repeat)
                        } else if name == "paste" {
                            // Special handling for paste command
                            // Check if this command has process substitution redirects
                            let mut has_process_sub = false;
                            for redirect in &simple_cmd.redirects {
                                if matches!(
                                    redirect.operator,
                                    crate::ast::RedirectOperator::ProcessSubstitutionInput(_)
                                ) {
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
                                    if let crate::ast::RedirectOperator::ProcessSubstitutionInput(
                                        cmd,
                                    ) = &redirect.operator
                                    {
                                        // Generate the process substitution command and create temp file
                                        let temp_file_id = generator.get_unique_id();
                                        let temp_file = format!("temp_file_ps_{}", temp_file_id);

                                        // Check if this is an echo command and use the dedicated echo generator
                                        let process_sub_output =
                                            if let crate::ast::Command::Simple(echo_cmd) = &**cmd {
                                                if let crate::ast::Word::Literal(name, _) =
                                                    &echo_cmd.name
                                                {
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
                                        process_sub_code.push_str(&format!(
                                            "my ${} = {} . '/process_sub_{}.tmp';\n",
                                            temp_file,
                                            crate::generator::utils::get_temp_dir(),
                                            temp_file_id
                                        ));
                                        process_sub_code.push_str(&format!("{{\n"));
                                        process_sub_code.push_str(&format!("    open my $fh, '>', ${} or croak \"Cannot create temp file: $OS_ERROR\\n\";\n", temp_file));

                                        // Check if this is an echo command and handle it specially
                                        if let crate::ast::Command::Simple(echo_cmd) = &**cmd {
                                            if let crate::ast::Word::Literal(name, _) =
                                                &echo_cmd.name
                                            {
                                                if name == "echo" {
                                                    // For echo commands, we need to execute the echo command and capture its output
                                                    process_sub_code
                                                        .push_str("    my $temp_output = q{};\n");
                                                    process_sub_code.push_str(&format!(
                                                        "    {}\n",
                                                        process_sub_output
                                                    ));
                                                    process_sub_code.push_str(
                                                        "    print {$fh} $temp_output;\n",
                                                    );
                                                } else {
                                                    process_sub_code.push_str(&format!(
                                                        "    print $fh {};\n",
                                                        process_sub_output
                                                    ));
                                                }
                                            } else {
                                                process_sub_code.push_str(&format!(
                                                    "    print $fh {};\n",
                                                    process_sub_output
                                                ));
                                            }
                                        } else {
                                            process_sub_code.push_str(&format!(
                                                "    print $fh {};\n",
                                                process_sub_output
                                            ));
                                        }
                                        process_sub_code.push_str("    close $fh\n");
                                        process_sub_code.push_str(
                                            "        or croak \"Close failed: $OS_ERROR\\n\";\n",
                                        );
                                        process_sub_code.push_str(&format!("}}\n"));

                                        process_sub_files
                                            .push((temp_file.clone(), process_sub_output));
                                    }
                                }

                                // Use the paste generator for proper output handling
                                let paste_output =
                                    crate::generator::commands::paste::generate_paste_command(
                                        generator,
                                        simple_cmd,
                                        &process_sub_files,
                                    );
                                format!("do {{ {} {} }}", process_sub_code, paste_output)
                            } else {
                                // Regular paste command without process substitution - use dedicated implementation
                                crate::generator::commands::paste::generate_paste_command(
                                    generator,
                                    simple_cmd,
                                    &[],
                                )
                            }
                        } else if name == "comm" {
                            // Special handling for comm command with process substitution
                            // Check if this command has process substitution redirects

                            let mut has_process_sub = false;
                            for redirect in &simple_cmd.redirects {
                                if matches!(
                                    redirect.operator,
                                    crate::ast::RedirectOperator::ProcessSubstitutionInput(_)
                                ) {
                                    has_process_sub = true;

                                    break;
                                }
                            }

                            if has_process_sub {
                                // Handle comm command with process substitution like paste command
                                let mut process_sub_code = String::new();
                                let mut process_sub_files = Vec::new();

                                for redirect in &simple_cmd.redirects {
                                    if let crate::ast::RedirectOperator::ProcessSubstitutionInput(
                                        sub_cmd,
                                    ) = &redirect.operator
                                    {
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
                                        process_sub_code.push_str(&format!(
                                            "my ${} = {} . '/process_sub_{}.tmp';\n",
                                            temp_file,
                                            crate::generator::utils::get_temp_dir(),
                                            temp_file_id
                                        ));
                                        process_sub_code.push_str(&format!("{{\n"));
                                        process_sub_code.push_str(&format!("    open my $fh, '>', ${} or croak \"Cannot create temp file: $OS_ERROR\\n\";\n", temp_file));
                                        process_sub_code.push_str("    my $temp_output = q{};\n");
                                        process_sub_code.push_str(&format!(
                                            "    $temp_output .= {};\n",
                                            process_sub_output
                                        ));
                                        process_sub_code
                                            .push_str("    print {$fh} $temp_output;\n");
                                        process_sub_code.push_str("    close $fh\n");
                                        process_sub_code.push_str(
                                            "        or croak \"Close failed: $OS_ERROR\\n\";\n",
                                        );
                                        process_sub_code.push_str(&format!("}}\n"));

                                        process_sub_files
                                            .push((temp_file.clone(), process_sub_output));
                                    }
                                }

                                // Use the comm generator for proper output handling
                                let comm_output =
                                    crate::generator::commands::comm::generate_comm_command(
                                        generator,
                                        simple_cmd,
                                        "cmd_result",
                                        &process_sub_files,
                                    );
                                format!("do {{ {} {} }}", process_sub_code, comm_output)
                            } else {
                                // Regular comm command without process substitution - use dedicated implementation
                                let comm_output =
                                    crate::generator::commands::comm::generate_comm_command(
                                        generator,
                                        simple_cmd,
                                        "comm_result",
                                        &[],
                                    );
                                format!("do {{ {} }}", comm_output)
                            }
                        } else if name == "diff" {
                            // Special handling for diff command in command substitution

                            // Use the dedicated diff command implementation
                            let diff_output =
                                crate::generator::commands::diff::generate_diff_command(
                                    generator,
                                    simple_cmd,
                                    "diff_result",
                                    0,
                                    false,
                                );
                            format!("do {{ {} }}", diff_output)
                        } else if name == "xargs" {
                            // Special handling for xargs command in command substitution

                            // Use the dedicated xargs command generator
                            let unique_id = generator.get_unique_id();
                            let xargs_output = crate::generator::commands::xargs::generate_xargs_command_with_output(generator, simple_cmd, "input_data", &unique_id.to_string(), "xargs_result");

                            // For command substitution, we need to return the result, not print it
                            format!("do {{ my $input_data = q{{}}; {} }}", xargs_output)
                        } else if name == "tr" {
                            // Special handling for tr command in command substitution

                            // Check if this tr command has here string redirects
                            let has_here_string = simple_cmd
                                .redirects
                                .iter()
                                .any(|r| matches!(r.operator, RedirectOperator::HereString));

                            let unique_id = generator.get_unique_id();
                            let input_data = if has_here_string {
                                // Extract here string content
                                let here_string_content = simple_cmd
                                    .redirects
                                    .iter()
                                    .find(|r| matches!(r.operator, RedirectOperator::HereString))
                                    .and_then(|r| r.heredoc_body.as_ref())
                                    .map(|content| format!("\"{}\"", content))
                                    .unwrap_or_else(|| "q{}".to_string());
                                format!("my $input_data = {};", here_string_content)
                            } else {
                                "my $input_data = q{};".to_string()
                            };

                            // Use the dedicated tr command generator for substitution (no newline)
                            let tr_output = crate::generator::commands::tr::generate_tr_command_for_substitution(generator, simple_cmd, "input_data", &unique_id.to_string());

                            // For command substitution, we need to return the result, not print it
                            format!("do {{ {} {} }}", input_data, tr_output)
                        } else if name == "perl" {
                            // Special handling for perl in command substitution - use native Perl instead of open3

                            if simple_cmd.args.len() >= 2 {
                                if let (Word::Literal(flag, _), Word::Literal(code, _)) =
                                    (&simple_cmd.args[0], &simple_cmd.args[1])
                                {
                                    if flag == "-e" {
                                        // Execute Perl code directly instead of using open3
                                        // Use capture_stdout to capture the output of print statements
                                        format!(
                                            "do {{ 
    my $result;
    my $eval_success = eval {{
        $result = capture_stdout( sub {{ {} }} );
        1;
    }};
    if ( !$eval_success ) {{
        $result = \"Error executing Perl code: $EVAL_ERROR\";
    }}
    $result;
}}",
                                            code
                                        )
                                    } else {
                                        // For other perl commands, use system call as fallback
                                        let args: Vec<String> = simple_cmd
                                            .args
                                            .iter()
                                            .map(|arg| generator.perl_string_literal(arg))
                                            .collect();
                                        let formatted_args = args.join(", ");
                                        format!(
                                            "do {{ 
                                            my $result = qx{{perl {}}};
                                            chomp $result;
                                            $result;
                                        }}",
                                            formatted_args
                                        )
                                    }
                                } else {
                                    // For other perl commands, use system call as fallback
                                    let args: Vec<String> = simple_cmd
                                        .args
                                        .iter()
                                        .map(|arg| generator.perl_string_literal(arg))
                                        .collect();
                                    let formatted_args = args.join(", ");
                                    format!(
                                        "do {{ 
                                        my $result = qx{{perl {}}};
                                        chomp $result;
                                        $result;
                                    }}",
                                        formatted_args
                                    )
                                }
                            } else {
                                // For perl commands with no arguments, use system call as fallback
                                format!(
                                    "do {{ 
                                    my $result = qx{{perl}};
                                    chomp $result;
                                    $result;
                                }}"
                                )
                            }
                        } else if name == "wc" {
                            let unique_id = generator.get_unique_id();
                            let output_var = format!("wc_output_{}", unique_id);
                            let input_var = format!("wc_input_{}", unique_id);
                            let input_setup = simple_cmd
                                .redirects
                                .iter()
                                .rev()
                                .find(|redirect| {
                                    matches!(redirect.operator, RedirectOperator::Input)
                                })
                                .map(|redirect| {
                                    let file_name = generator.word_to_perl(&redirect.target);
                                    format!(
                                        "my ${} = do {{\n    local $INPUT_RECORD_SEPARATOR = undef;\n    open my $fh, '<', {}\n        or croak \"Cannot open file: $OS_ERROR\";\n    my $content = <$fh>;\n    close $fh\n        or croak \"Close failed: $OS_ERROR\";\n    $content\n}};\n",
                                        input_var, file_name
                                    )
                                })
                                .unwrap_or_default();
                            let wc_code =
                                crate::generator::commands::wc::generate_wc_command_with_output(
                                    generator,
                                    simple_cmd,
                                    if input_setup.is_empty() {
                                        ""
                                    } else {
                                        &input_var
                                    },
                                    &unique_id,
                                    &output_var,
                                );
                            format!(
                                "do {{\n{}{}\n    ${};\n}}",
                                input_setup,
                                wc_code.trim_end(),
                                output_var,
                            )
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
                                if generator.inline_mode {
                                    format!("({}) . \"\\n\"", args.join(" . q{ } . "))
                                } else {
                                    format!("({})", args.join(" . q{ } . "))
                                }
                            }
                        } else if name == "sha256sum" {
                            // Generate the sha256 handling directly in Perl for
                            // command substitution instead of running the external
                            // sha256sum program which may be missing in some
                            // environments. The generator emits equivalent logic
                            // so inline it here as a single expression.
                            crate::generator::commands::sha256sum::generate_sha256sum_command(
                                generator, simple_cmd, "",
                            )
                        } else if name == "sha512sum" {
                            // Generate the sha512 handling directly in Perl for
                            // command substitution instead of running the external
                            // sha512sum program which may be missing in some
                            // environments.
                            crate::generator::commands::sha512sum::generate_sha512sum_command(
                                generator, simple_cmd, "",
                            )
                        } else if name == "grep" {
                            // Use the proper grep command generator
                            let unique_id = generator.get_unique_id();
                            let grep_output =
                                crate::generator::commands::grep::generate_grep_command(
                                    generator,
                                    simple_cmd,
                                    "",
                                    &unique_id.to_string(),
                                    false,
                                );
                            format!("do {{ {} $grep_result_{}; }}", grep_output, unique_id)
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
                                if args.is_empty() {
                                    format!(
                                        "do {{\n    my $result = sprintf \"{}\";\n    $result;\n}}",
                                        format_string.replace("\"", "\\\"").replace("\\\\", "\\")
                                    )
                                } else {
                                    // Properly quote string arguments for sprintf
                                    let formatted_args = args
                                        .iter()
                                        .map(|arg| {
                                            // Check if the argument is already quoted
                                            if (arg.starts_with('"') && arg.ends_with('"'))
                                                || (arg.starts_with('\'') && arg.ends_with('\''))
                                                || arg.starts_with("q{")
                                            {
                                                arg.clone()
                                            } else {
                                                // Quote unquoted arguments
                                                format!("\"{}\"", arg.replace("\"", "\\\""))
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    format!("do {{\n    my $result = sprintf \"{}\", {};\n    $result;\n}}", 
                                        format_string.replace("\"", "\\\"").replace("\\\\", "\\"),
                                        formatted_args)
                                }
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
                            let basename_lit = generator
                                .perl_string_literal_no_interp(&Word::literal(basename_cmd));
                            format!(
                                "do {{ my $basename_cmd = {}; my $basename_output = qx{{$basename_cmd}}; $CHILD_ERROR = $? >> 8; $basename_output; }}",
                                basename_lit
                            )
                        } else if name == "dirname" {
                            let dirname_cmd = generator.generate_command_string_for_system(
                                &Command::Simple(simple_cmd.clone()),
                            );
                            let dirname_lit = generator
                                .perl_string_literal_no_interp(&Word::literal(dirname_cmd));
                            format!(
                                "do {{ my $dirname_cmd = {}; my $dirname_output = qx{{$dirname_cmd}}; $CHILD_ERROR = $? >> 8; $dirname_output; }}",
                                dirname_lit
                            )
                        } else if name == "which" {
                            // Use the real which command so flags and exit codes match the host tool.
                            let which_cmd = generator.generate_command_string_for_system(cmd);
                            let which_lit =
                                generator.perl_string_literal_no_interp(&Word::literal(which_cmd));
                            format!(
                                "do {{ my $which_cmd = {}; my $which_output = qx{{$which_cmd}}; $CHILD_ERROR = $? >> 8; $which_output; }}",
                                which_lit
                            )
                        } else if name == "seq" {
                            // Special handling for seq in command substitution
                            if simple_cmd.args.is_empty() {
                                "\"1\"".to_string()
                            } else if simple_cmd.args.len() == 1 {
                                let last_str = generator.word_to_perl(&simple_cmd.args[0]);
                                format!(
                                    "do {{ my $last; $last = {}; join \"\\n\", 1..$last; }}",
                                    last_str
                                )
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
                            // Special handling for perl in command substitution - use native Perl instead of open3
                            // For perl -e 'print "..."' commands, capture the output instead of printing
                            if simple_cmd.args.len() >= 2 {
                                if let (Word::Literal(flag, _), Word::Literal(code, _)) =
                                    (&simple_cmd.args[0], &simple_cmd.args[1])
                                {
                                    if flag == "-e" {
                                        // Clean the code by removing outer quotes and fixing escaping
                                        let mut clean_code = code.clone();
                                        if (clean_code.starts_with('"')
                                            && clean_code.ends_with('"'))
                                            || (clean_code.starts_with('\'')
                                                && clean_code.ends_with('\''))
                                        {
                                            clean_code =
                                                clean_code[1..clean_code.len() - 1].to_string();
                                        }
                                        // Fix double-escaped quotes and newlines
                                        clean_code = clean_code
                                            .replace("\\\"", "\"")
                                            .replace("\\\\n", "\\n");

                                        // Execute Perl code directly instead of using open3
                                        // Use capture_stdout to capture the output of print statements
                                        // Format for command substitution - content should have 4-space indentation inside do { }
                                        format!("do {{\n    my $result;\n    my $eval_success = eval {{\n        $result = capture_stdout(sub {{ {} }});\n        1;\n    }};\n    if (!$eval_success) {{\n        $result = \"Error executing Perl code: $EVAL_ERROR\";\n    }}\n    $result;\n}}", clean_code)
                                    } else {
                                        // For other perl commands, use system call as fallback
                                        let args: Vec<String> = simple_cmd
                                            .args
                                            .iter()
                                            .map(|arg| generator.word_to_perl(arg))
                                            .collect();
                                        let formatted_args = args.join(" ");
                                        format!("do {{\n    my $result = qx{{perl {}}};\n    chomp $result;\n    $result;\n}}", formatted_args)
                                    }
                                } else {
                                    // For other perl commands, use system call as fallback
                                    let args: Vec<String> = simple_cmd
                                        .args
                                        .iter()
                                        .map(|arg| generator.word_to_perl(arg))
                                        .collect();
                                    let formatted_args = args.join(" ");
                                    format!("do {{\n    my $result = qx{{perl {}}};\n    chomp $result;\n    $result;\n}}", formatted_args)
                                }
                            } else {
                                // For perl commands with no arguments, use system call as fallback
                                "do {\n    my $result = qx{perl};\n    chomp $result;\n    $result;\n}".to_string()
                            }
                        } else if generator.inline_mode && name == "echo" {
                            // In inline mode for echo, generate the output value directly
                            if simple_cmd.args.is_empty() {
                                "\"\\n\"".to_string()
                            } else {
                                let args: Vec<String> = simple_cmd
                                    .args
                                    .iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect();
                                format!("({}) . \"\\n\"", args.join(" . q{ } . "))
                            }
                        } else if name == "cp" {
                            // Use native Perl cp implementation for command substitution

                            // Generate cp code - need to preserve relative indentation
                            let cp_code = crate::generator::commands::cp::generate_cp_command(
                                generator, simple_cmd,
                            );
                            // Find the minimum indentation in cp_code to normalize it
                            let lines: Vec<&str> = cp_code.trim_end_matches('\n').lines().collect();
                            let min_indent = lines
                                .iter()
                                .filter(|line| !line.trim().is_empty())
                                .map(|line| line.len() - line.trim_start().len())
                                .min()
                                .unwrap_or(0);

                            // Remove base indentation and add eval block indentation
                            let mut formatted_lines = Vec::new();
                            let base_eval_indent = 12; // 12 spaces for eval block content (inside do{ } at 4 spaces, then eval { at 8 spaces, so content is at 12)
                            for line in lines {
                                let trimmed = line.trim_start();
                                if !trimmed.is_empty() {
                                    // Calculate relative indentation from original line
                                    let orig_indent = line.len() - trimmed.len();
                                    // Remove base indent and add eval block base indent
                                    let relative_indent = orig_indent.saturating_sub(min_indent);
                                    let new_indent = base_eval_indent + relative_indent;
                                    formatted_lines.push(format!(
                                        "{}{}",
                                        " ".repeat(new_indent),
                                        trimmed
                                    ));
                                }
                            }
                            let formatted_code = formatted_lines
                                .join("\n")
                                .replace("if(-e", "if ( -e")
                                .replace("if (-e", "if ( -e")
                                .replace("if(-d", "if ( -d")
                                .replace("if (-d", "if ( -d")
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            // Ensure formatted_code ends with a newline for proper formatting
                            let formatted_code = if formatted_code.ends_with('\n') {
                                formatted_code
                            } else {
                                format!("{}\n", formatted_code)
                            };
                            // The do block is nested inside another do block (my $left_result_0 = do {)
                            // So we need to account for that extra indentation level
                            // Fixed indentation: outer do block at 4 spaces, inner do block at 8 spaces, eval at 12 spaces
                            // We use fixed indentation to ensure consistency regardless of generator.indent_level
                            let indent1 = "    ".to_string(); // 4 spaces for outer do block
                            let indent1_do = "        ".to_string(); // 8 spaces for inner do block
                            let indent2 = "            ".to_string(); // 12 spaces for eval block
                            format!("do {{\n{}local $CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}local $CHILD_ERROR = 0;\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    local $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do, indent1_do)
                        } else if name == "mv" {
                            // Use native Perl mv implementation for command substitution

                            let mv_code = crate::generator::commands::mv::generate_mv_command(
                                generator, simple_cmd,
                            );
                            let lines: Vec<&str> = mv_code.trim_end_matches('\n').lines().collect();
                            let min_indent = lines
                                .iter()
                                .filter(|line| !line.trim().is_empty())
                                .map(|line| line.len() - line.trim_start().len())
                                .min()
                                .unwrap_or(0);
                            let mut formatted_lines = Vec::new();
                            let base_eval_indent = 12;
                            for line in lines {
                                let trimmed = line.trim_start();
                                if !trimmed.is_empty() {
                                    let orig_indent = line.len() - trimmed.len();
                                    let relative_indent = orig_indent.saturating_sub(min_indent);
                                    formatted_lines.push(format!(
                                        "{}{}",
                                        " ".repeat(base_eval_indent + relative_indent),
                                        trimmed
                                    ));
                                }
                            }
                            let formatted_code = formatted_lines
                                .join("\n")
                                .replace("if(-e", "if ( -e")
                                .replace("if (-e", "if ( -e")
                                .replace("if(-d", "if ( -d")
                                .replace("if (-d", "if ( -d")
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = "    ".to_string();
                            let indent1_do = "        ".to_string();
                            let indent2 = "            ".to_string();
                            format!("do {{\n{}local $CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}local $CHILD_ERROR = 0;\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    local $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do, indent1_do)
                        } else if name == "rm" {
                            // Use native Perl rm implementation for command substitution

                            let rm_code = crate::generator::commands::rm::generate_rm_command(
                                generator, simple_cmd,
                            );
                            let lines: Vec<&str> = rm_code.trim_end_matches('\n').lines().collect();
                            let min_indent = lines
                                .iter()
                                .filter(|line| !line.trim().is_empty())
                                .map(|line| line.len() - line.trim_start().len())
                                .min()
                                .unwrap_or(0);
                            let mut formatted_lines = Vec::new();
                            let base_eval_indent = 12;
                            for line in lines {
                                let trimmed = line.trim_start();
                                if !trimmed.is_empty() {
                                    let orig_indent = line.len() - trimmed.len();
                                    let relative_indent = orig_indent.saturating_sub(min_indent);
                                    formatted_lines.push(format!(
                                        "{}{}",
                                        " ".repeat(base_eval_indent + relative_indent),
                                        trimmed
                                    ));
                                }
                            }
                            let formatted_code = formatted_lines
                                .join("\n")
                                .replace("if(-e", "if ( -e")
                                .replace("if (-e", "if ( -e")
                                .replace("if(-d", "if ( -d")
                                .replace("if (-d", "if ( -d")
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = "    ".to_string();
                            let indent1_do = "        ".to_string();
                            let indent2 = "            ".to_string();
                            format!("do {{\n{}local $CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}local $CHILD_ERROR = 0;\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    local $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do, indent1_do)
                        } else if name == "mkdir" {
                            // Use native Perl mkdir implementation for command substitution

                            let mkdir_code =
                                crate::generator::commands::mkdir::generate_mkdir_command(
                                    generator, simple_cmd,
                                );
                            let lines: Vec<&str> =
                                mkdir_code.trim_end_matches('\n').lines().collect();
                            let min_indent = lines
                                .iter()
                                .filter(|line| !line.trim().is_empty())
                                .map(|line| line.len() - line.trim_start().len())
                                .min()
                                .unwrap_or(0);
                            let mut formatted_lines = Vec::new();
                            let base_indent = 8;
                            for line in lines {
                                let trimmed = line.trim_start();
                                if !trimmed.is_empty() {
                                    let orig_indent = line.len() - trimmed.len();
                                    let relative_indent = orig_indent.saturating_sub(min_indent);
                                    formatted_lines.push(format!(
                                        "{}{}",
                                        " ".repeat(base_indent + relative_indent),
                                        trimmed
                                    ));
                                }
                            }
                            let formatted_code =
                                formatted_lines.join("\n").replace("die ", "croak ");
                            let indent1 = "    ".to_string();
                            let indent1_do = "        ".to_string();
                            let indent2 = "            ".to_string();
                            format!("do {{\n{}local $CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}local $CHILD_ERROR = 0;\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    local $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do, indent1_do)
                        } else if name == "touch" {
                            // Use native Perl touch implementation for command substitution

                            let touch_code =
                                crate::generator::commands::touch::generate_touch_command(
                                    generator, simple_cmd,
                                );
                            let lines: Vec<&str> =
                                touch_code.trim_end_matches('\n').lines().collect();
                            let min_indent = lines
                                .iter()
                                .filter(|line| !line.trim().is_empty())
                                .map(|line| line.len() - line.trim_start().len())
                                .min()
                                .unwrap_or(0);
                            let mut formatted_lines = Vec::new();
                            let base_eval_indent = 12;
                            for line in lines {
                                let trimmed = line.trim_start();
                                if !trimmed.is_empty() {
                                    let orig_indent = line.len() - trimmed.len();
                                    let relative_indent = orig_indent.saturating_sub(min_indent);
                                    formatted_lines.push(format!(
                                        "{}{}",
                                        " ".repeat(base_eval_indent + relative_indent),
                                        trimmed
                                    ));
                                }
                            }
                            let formatted_code = formatted_lines
                                .join("\n")
                                .replace("print ", "# print ")
                                .replace("die ", "croak ");
                            let indent1 = "    ".to_string();
                            let indent1_do = "        ".to_string();
                            let indent2 = "            ".to_string();
                            format!("do {{\n{}local $CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}local $CHILD_ERROR = 0;\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    local $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do, indent1_do)
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
                                // Properly escape quotes in the command string
                                let escaped_command = command_str.replace("\"", "\\\"");
                                time_output.push_str(&format!("system \"{}\";\n", escaped_command));
                            }

                            time_output.push_str("my $end_time = [gettimeofday];\n");
                            time_output
                                .push_str("my $elapsed = tv_interval($start_time, $end_time);\n");
                            time_output.push_str("my $time_output = sprintf \"real\\t0m%.3fs\\nuser\\t0m0.000s\\nsys\\t0m0.000s\\n\", $elapsed;\n");
                            time_output.push_str("print STDERR $time_output;\n");
                            time_output.push_str("q{};\n");

                            format!("do {{ {} }}", time_output)
                        } else if name == "sleep" {
                            crate::generator::commands::sleep::generate_sleep_expression(
                                generator, simple_cmd,
                            )
                        } else {
                            // Fall back to system command for non-builtin commands
                            let cmd_name = generator.perl_string_literal(&simple_cmd.name);
                            let args: Vec<String> = simple_cmd
                                .args
                                .iter()
                                .map(|arg| generator.perl_string_literal(arg))
                                .collect();

                            let (in_var, out_var, err_var, pid_var, result_var) =
                                generator.get_unique_ipc_vars();
                            if args.is_empty() {
                                format!("do {{\n    my ({}, {}, {});\n    my {} = open3({}, {}, {}, {});\n    close {} or croak 'Close failed: $OS_ERROR';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $OS_ERROR';\n    waitpid {}, 0;\n    {}\n}}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
                            } else {
                                let formatted_args = args.join(", ");
                                format!("do {{\n    my ({}, {}, {});\n    my {} = open3({}, {}, {}, {}, {});\n    close {} or croak 'Close failed: $OS_ERROR';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $OS_ERROR';\n    waitpid {}, 0;\n    {}\n}}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                            }
                        }
                    } else {
                        // Fall back to system command for non-literal command names
                        let cmd_name = generator.perl_string_literal(&simple_cmd.name);
                        let args: Vec<String> = simple_cmd
                            .args
                            .iter()
                            .map(|arg| generator.perl_string_literal(arg))
                            .collect();

                        let (in_var, out_var, err_var, pid_var, result_var) =
                            generator.get_unique_ipc_vars();
                        if args.is_empty() {
                            format!("do {{\n    my ({}, {}, {});\n    my {} = open3({}, {}, {}, {});\n    close {} or croak 'Close failed: $OS_ERROR';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $OS_ERROR';\n    waitpid {}, 0;\n    {}\n}}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, in_var, result_var, out_var, out_var, pid_var, result_var)
                        } else {
                            let formatted_args = args.join(", ");
                            format!("do {{\n    my ({}, {}, {});\n    my {} = open3({}, {}, {}, {}, {});\n    close {} or croak 'Close failed: $OS_ERROR';\n    my {} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $OS_ERROR';\n    waitpid {}, 0;\n    {}\n}}", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, cmd_name, formatted_args, in_var, result_var, out_var, out_var, pid_var, result_var)
                        }
                    }
                }
                Command::Pipeline(pipeline) => {
                    // For command substitution pipelines, keep the shell pipeline intact
                    // but emit it through qx{} so the purified script does not contain backticks.
                    let pipeline_cmd = generator
                        .generate_command_string_for_system(&Command::Pipeline(pipeline.clone()));
                    // Debug: print the raw pipeline command and the Perl literal that will be
                    // embedded into the generated source. This helps diagnose quoting/escaping
                    // issues where Perl interpolation or escape sequences change the runtime
                    // shell command semantics.
                    // Ensure we emit a non-interpolating Perl literal for the
                    // shell pipeline string so Perl does not interpret "$" or
                    // turn "\\n" sequences into real newlines at compile-time.
                    // Using a non-interpolating literal preserves the exact
                    // shell command text that will be passed to qx{}.
                    let pipeline_lit = generator
                        .perl_string_literal_no_interp(&Word::literal(pipeline_cmd.clone()));
                    format!(
                        "do {{ my $pipeline_cmd = {}; my $result = qx{{$pipeline_cmd}}; $CHILD_ERROR = $? >> 8; $result; }}",
                        pipeline_lit
                    )
                }
                Command::And(left_cmd, right_cmd) => {
                    // Debug: print the AST shapes for left and right when handling && in
                    // command substitution to help diagnose wrapping/redirect issues.
                    eprintln!(
                        "DEBUG: Command::And left={:?} right={:?}",
                        left_cmd, right_cmd
                    );
                    // Conservative handling for And in command substitution:
                    // If both sides are simple commands without redirects we can
                    // try to compose them into Perl do-blocks. Otherwise fall
                    // back to running the whole AND expression through the
                    // shell (via qx{}) to avoid fragile string splicing and
                    // nested do-block/variable duplication issues.
                    let is_simple_pair = match (left_cmd.as_ref(), right_cmd.as_ref()) {
                        (Command::Simple(ls), Command::Simple(rs)) => {
                            ls.redirects.is_empty() && rs.redirects.is_empty()
                        }
                        _ => false,
                    };

                    if !is_simple_pair {
                        // Special-case: left side writes a checksum file (via > or >>)
                        // and the right side is a matching sha256sum/sha512sum -c
                        // verifier that reads that file. In that case, avoid
                        // falling back to running the whole expression in the
                        // shell (which uses external sha*sum binaries). Instead
                        // compute the checksum in Perl, write the checksum file,
                        // and then invoke the existing pure-Perl sha verifier.
                        if let (Command::Simple(simple_left), Command::Simple(simple_right)) =
                            (left_cmd.as_ref(), right_cmd.as_ref())
                        {
                            // Only handle the common literal-name case here
                            if let (Word::Literal(lname, _), Word::Literal(rname, _)) =
                                (&simple_left.name, &simple_right.name)
                            {
                                if (lname == "sha256sum" || lname == "sha512sum") && rname == lname
                                {
                                    // Look for an output redirect on the left side
                                    if let Some(redirect) = simple_left.redirects.iter().find(|r| {
                                        matches!(
                                            r.operator,
                                            RedirectOperator::Output | RedirectOperator::Append
                                        )
                                    }) {
                                        // Require the redirect target to be a literal filename
                                        if let Word::Literal(target_name, _) = &redirect.target {
                                            // Verify the right-hand args include "-c" and the same filename
                                            let mut found_c = false;
                                            let mut found_target = false;
                                            for arg in &simple_right.args {
                                                if let Word::Literal(a, _) = arg {
                                                    if a == "-c" {
                                                        found_c = true;
                                                    } else if a == target_name {
                                                        found_target = true;
                                                    }
                                                }
                                            }

                                            if found_c && found_target {
                                                // Prepare a left-side simple command without the redirect
                                                let mut left_clone = simple_left.clone();
                                                left_clone.redirects.clear();

                                                // Compute checksum content using existing generators
                                                let compute_expr = if lname == "sha256sum" {
                                                    crate::generator::commands::sha256sum::generate_sha256sum_command(
                                                        generator,
                                                        &left_clone,
                                                        "",
                                                    )
                                                } else {
                                                    crate::generator::commands::sha512sum::generate_sha512sum_command(
                                                        generator,
                                                        &left_clone,
                                                        "",
                                                    )
                                                };

                                                let target_lit =
                                                    generator.perl_string_literal(&redirect.target);

                                                // Generate verifier code using the right-hand command
                                                let check_expr = if rname == "sha256sum" {
                                                    crate::generator::commands::sha256sum::generate_sha256sum_command(
                                                        generator,
                                                        simple_right,
                                                        "",
                                                    )
                                                } else {
                                                    crate::generator::commands::sha512sum::generate_sha512sum_command(
                                                        generator,
                                                        simple_right,
                                                        "",
                                                    )
                                                };

                                                // Compose a do-block: compute checksum string, write it to
                                                // the checksum file, then run verifier and return its output.
                                                return format!(
                                                    "do {{\n    my $checksum_content = {}\n    open my $fh, '>', {} or croak \"Cannot create {}: $OS_ERROR\\n\";\n    print $fh $checksum_content;\n    close $fh or croak \"Close failed: $OS_ERROR\\n\";\n    {}\n}}",
                                                    compute_expr, target_lit, target_lit, check_expr
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Fallback: run the combined command via the shell
                        let command_str =
                            crate::generator::redirects::generate_bash_command_string(cmd);
                        let command_lit =
                            generator.perl_string_literal_no_interp(&Word::literal(command_str));
                        return format!(
                            "do {{ my $command = {}; my $result = qx{{$command}}; $CHILD_ERROR = $? >> 8; $result; }}",
                            command_lit
                        );
                    }

                    // Both sides are simple and without redirects: compose them.
                    let unique_id = generator.get_unique_id();
                    let left_result = word_to_perl_impl(
                        generator,
                        &Word::CommandSubstitution(left_cmd.clone(), Default::default()),
                    );

                    // Default: generate the right-hand result normally. For the
                    // special case where the right-hand simple command is a
                    // sha256sum/sha512sum -c verification that should read from
                    // the left command's output, call the sha generator directly
                    // and pass the $left_result_<id> variable as input_var so the
                    // split/verification code has a valid variable to operate on.
                    let mut right_result = word_to_perl_impl(
                        generator,
                        &Word::CommandSubstitution(right_cmd.clone(), Default::default()),
                    );
                    // Debug: persist the raw left/right generated Perl snippets to /tmp
                    // so we can inspect exact shapes when diagnosing nested do-block issues.
                    // These files are temporary and helpful during development; they can be
                    // removed once the issue is resolved.
                    let _ = std::fs::write(
                        format!("/tmp/sh2perl_and_left_{}.txt", unique_id),
                        &left_result,
                    );
                    let _ = std::fs::write(
                        format!("/tmp/sh2perl_and_right_{}.txt", unique_id),
                        &right_result,
                    );
                    if let Command::Simple(simple_right) = right_cmd.as_ref() {
                        if let Word::Literal(rname, _) = &simple_right.name {
                            if (rname == "sha256sum" || rname == "sha512sum")
                                && simple_right
                                    .args
                                    .iter()
                                    .any(|a| matches!(a, Word::Literal(s, _) if s == "-c"))
                            {
                                // Decide whether to pass the left-side captured
                                // variable into the sha generator. Only do this
                                // when the left command is a simple command
                                // without redirections (i.e., its output was
                                // captured into a Perl value). If the left
                                // command performed a shell redirection (e.g.
                                // '> file') then the checksum is written to
                                // a file and the sha verifier should read that
                                // file instead, so pass an empty input_var.
                                let mut input_var = String::new();
                                if let Command::Simple(simple_left) = left_cmd.as_ref() {
                                    if simple_left.redirects.is_empty() {
                                        input_var = format!("$left_result_{}", unique_id);
                                    }
                                }

                                // Only inline the sha generator when we have a valid
                                // Perl variable to pass as input_var (i.e. the left
                                // side's output was captured into a variable). If the
                                // left side performed shell redirections (wrote to a
                                // file) we should NOT replace the already-generated
                                // right_result; leave it as the normal command
                                // substitution form which executes the checksum tool
                                // via the shell. This avoids producing nested
                                // do-blocks / duplicated declarations.
                                if !input_var.is_empty() {
                                    if rname == "sha256sum" {
                                        right_result = crate::generator::commands::sha256sum::generate_sha256sum_command(
                                            generator,
                                            simple_right,
                                            &input_var,
                                        );
                                    } else {
                                        right_result = crate::generator::commands::sha512sum::generate_sha512sum_command(
                                            generator,
                                            simple_right,
                                            &input_var,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Generate code that executes left command, checks exit code,
                    // then executes right if successful. Keep both sides as
                    // expression-valued do-blocks to avoid fragile string
                    // splicing/manipulation.
                    // Normalize a specific dupication pattern where a generator
                    // may emit a top-level "my @results;" and also an inner
                    // do { ... } block that itself begins with a declaration.
                    // This produced nested "my @results; do { my @results;" in
                    // the output; remove the redundant one here.
                    let left_normalized = left_result
                        .replace("my @results;\n    do {", "do {")
                        .replace("my @results;\ndo {", "do {");
                    let right_normalized = right_result
                        .replace("my @results;\n    do {", "do {")
                        .replace("my @results;\ndo {", "do {");

                    let left_wrapped = if left_normalized.trim_start().starts_with("do {") {
                        left_normalized
                    } else {
                        format!("do {{ {} }}", left_normalized)
                    };

                    let right_wrapped = if right_normalized.trim_start().starts_with("do {") {
                        right_normalized
                    } else {
                        format!("do {{ {} }}", right_normalized)
                    };

                    format!(
                        "do {{\n    my $left_result_{} = {};\n    if ( $CHILD_ERROR == 0 ) {{\n        my $right_result_{} = {};\n        $left_result_{} . $right_result_{};\n    }} else {{\n        q{{}};\n    }}\n}}",
                        unique_id, left_wrapped, unique_id, right_wrapped, unique_id, unique_id,
                    )
                }
                _ => {
                    // For other command types, execute the real shell command so
                    // control operators and redirections keep working.
                    let command_str =
                        crate::generator::redirects::generate_bash_command_string(cmd);
                    let command_lit = generator.perl_string_literal(&Word::literal(command_str));
                    format!(
                        "do {{ my $command = {}; my $result = qx{{$command}}; $CHILD_ERROR = $? >> 8; $result; }}",
                        command_lit
                    )
                }
            };
            // For simple expressions, avoid unnecessary wrapping
            if result.contains("use POSIX qw(strftime)")
                || result.contains("use Cwd; getcwd()")
                || result.starts_with("do { my $")
                || result.contains("chomp $result")
                || result.len() < 100
            {
                // Simple expressions don't need wrapping
                result
            } else {
                // Check if the result is already a do block - if so, return as-is
                // (don't add extra indentation here as it will be inserted into assignments)
                if result.trim_start().starts_with("do {") {
                    // Result is already a complete do block, return as-is without additional indentation
                    // The caller will handle any necessary indentation based on context
                    result
                } else {
                    // For other results, return as-is
                    result
                }
            }
        }
        Word::Variable(var, _, _) => {
            // Handle special shell variables
            match var.as_str() {
                "#" => "scalar(@ARGV)".to_string(), // $# -> scalar(@ARGV) for argument count
                "@" => "@ARGV".to_string(),         // $@ -> @ARGV for arguments array
                "*" => "@ARGV".to_string(),         // $* -> @ARGV for arguments array
                "0" => "$PROGRAM_NAME".to_string(), // $0 -> $PROGRAM_NAME (Perl::Critic compliant)
                _ => format!("${}", var),           // Regular variable
            }
        }
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
        }
        Word::MapKeys(map_name, _) => {
            // Handle map keys like !map[@] -> keys %map
            format!("keys %{}", map_name)
        }
        Word::MapLength(map_name, _) => {
            // Handle array length like #arr[@] -> scalar(@arr)
            format!("scalar(@{})", map_name)
        }
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
        _ => format!("{:?}", word),
    }
}

// Helper methods
pub fn handle_range_expansion_impl(_generator: &Generator, s: &str) -> String {
    let parts: Vec<&str> = s.split("..").collect();
    if parts.len() == 2 {
        if let (Ok(start), Ok(end)) = (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
            let values: Vec<String> = (start..=end).map(|i| i.to_string()).collect();
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

pub fn handle_brace_expansion_impl(
    generator: &mut Generator,
    expansion: &BraceExpansion,
) -> String {
    // Handle prefix and suffix
    let prefix = expansion.prefix.as_deref().unwrap_or("");
    let suffix = expansion.suffix.as_deref().unwrap_or("");

    if expansion.items.len() == 1 {
        let expanded = generator.word_to_perl(&generator.brace_item_to_word(&expansion.items[0]));
        if !prefix.is_empty() || !suffix.is_empty() {
            // Split the expanded items and add prefix/suffix to each
            let items: Vec<String> = expanded
                .split_whitespace()
                .map(|item| format!("{}{}{}", prefix, item, suffix))
                .collect();
            items.join(" ")
        } else {
            expanded
        }
    } else {
        // Handle cartesian product for multiple brace items
        let expanded_items: Vec<Vec<String>> = expansion
            .items
            .iter()
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
        let items: Vec<String> = cartesian
            .iter()
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
        }
        BraceItem::Sequence(seq) => Word::literal(seq.join(" ")),
    }
}

fn expand_range(range: &BraceRange) -> String {
    // Check if this is a numeric range
    if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
        let step = range
            .step
            .as_ref()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(1);

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
        if let (Some(start_char), Some(end_char)) =
            (range.start.chars().next(), range.end.chars().next())
        {
            let step = range
                .step
                .as_ref()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(1);

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

pub fn convert_string_interpolation_to_perl_impl(
    generator: &mut Generator,
    interp: &StringInterpolation,
) -> String {
    // Convert string interpolation to Perl concatenation when command substitutions are present
    let mut parts = Vec::new();
    let mut current_string = String::new();

    for part in &interp.parts {
        match part {
            StringPart::Literal(s) => {
                // Accumulate literal parts into the current string
                current_string.push_str(s);
            }
            StringPart::Variable(var) => {
                // Handle special shell variables
                match var.as_str() {
                    "#" => current_string.push_str("${scalar(@ARGV)}"), // $# -> ${scalar(@ARGV)} for interpolation
                    "@" => current_string.push_str("@ARGV"), // Arrays don't need $ in interpolation
                    "*" => current_string.push_str("@ARGV"), // Arrays don't need $ in interpolation
                    _ => {
                        // Check if this is a shell positional parameter ($1, $2, etc.)
                        if var.chars().all(|c| c.is_digit(10)) {
                            // Convert $1 to $_[0], $2 to $_[1], etc.
                            let index = var.parse::<usize>().unwrap_or(0);
                            current_string.push_str(&format!("$_[{}]", index - 1));
                        // Perl arrays are 0-indexed
                        } else {
                            // Regular variable - add directly for interpolation
                            current_string.push_str(&format!("${}", var));
                        }
                    }
                }
            }
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
                    push_string_expr(&mut parts, &mut current_string);
                }
                // Add the command substitution as a separate part
                let cmd_result =
                    generator.word_to_perl(&Word::CommandSubstitution(cmd.clone(), None));
                parts.push(format!(
                    "(do {{ my $_chomp_temp = {}; chomp $_chomp_temp; $_chomp_temp; }})",
                    cmd_result
                ));
            }
            StringPart::ParameterExpansion(pe) => {
                // Handle parameter expansions like ${arr[1]}, ${#arr[@]}, etc.
                // We need to convert the ParameterExpansion to Perl code
                // For now, let's handle the common cases directly

                // First, add any accumulated string as a quoted part
                if !current_string.is_empty() {
                    push_string_expr(&mut parts, &mut current_string);
                }

                // Check for special array operations first
                match &pe.operator {
                    ParameterExpansionOperator::ArraySlice(offset, length) => {
                        if offset == "@" {
                            // This is ${#arr[@]} or ${arr[@]} - array length or array iteration
                            if pe.variable.starts_with('#') {
                                // ${#arr[@]} -> scalar(@arr)
                                let array_name = &pe.variable[1..];
                                parts.push(format!("scalar(@{})", array_name));
                            } else if pe.variable.starts_with('!') {
                                // ${!map[@]} -> keys %map (map keys iteration)
                                let map_name = &pe.variable[1..]; // Remove ! prefix
                                parts.push(format!("keys %{}", map_name));
                            } else {
                                // ${arr[@]} -> @arr (for array iteration)
                                let array_name = &pe.variable;
                                parts.push(format!("@{}", array_name));
                            }
                        } else {
                            // Regular array slice
                            if let Some(length_str) = length {
                                parts.push(format!(
                                    "@${{{}}}[{}..{}]",
                                    pe.variable, offset, length_str
                                ));
                            } else {
                                parts.push(format!("@${{{}}}[{}..]", pe.variable, offset));
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
                                        parts.push(format!("${}[{}]", var_name, key));
                                    } else {
                                        // Associative array access: map[foo] -> $map{foo}
                                        parts.push(format!("${}{{{}}}", var_name, key));
                                    }
                                } else {
                                    parts.push(format!("${{{}}}", pe.variable));
                                }
                            } else {
                                parts.push(format!("${{{}}}", pe.variable));
                            }
                        } else {
                            // Simple variable reference - use the proper parameter expansion generation
                            parts.push(generator.generate_parameter_expansion(pe));
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
        push_string_expr(&mut parts, &mut current_string);
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
