use crate::ast::*;
use crate::generator::Generator;

pub fn generate_printf_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    _input_var: &str,
    _command_index: usize,
    output_var: Option<&str>,
    as_expression: bool,
) -> String {
    let mut output = String::new();
    let is_expression = output_var.is_none() && as_expression;

    // Parse printf format string and arguments
    let mut format_string = String::new();
    // decoded_format stores the raw decoded format string (without surrounding
    // shell quotes) so we can count % specifiers correctly and decide when
    // to repeat the format across arguments to emulate shell printf semantics.
    let mut decoded_format = String::new();
    let mut args = Vec::new();

    for (i, arg) in cmd.args.iter().enumerate() {
        if i == 0 {
            // First argument is the format string. Keep the Perl-quoted literal
            // returned by word_to_perl then normalize doubled backslashes
            // (which were produced by perl_string_literal_impl) into single
            // backslashes so sequences like "\\n" become "\n" in the
            // generated Perl source and are interpreted as newlines at
            // runtime.
            // Obtain the raw Perl literal for the format string. If this is
            // a quoted literal like '"..."' or '\'...\'', strip the outer
            // quotes and decode common shell-style escape sequences so that
            // perl_string_literal_impl will emit a single-escaped \n (not a
            // double-escaped \\n) in the generated source.
            // Decode common escapes (
            // "\\n", "\\t", etc.) into actual characters so that
            // perl_string_literal_impl will emit the correct single-escaped
            // sequences in the generated Perl source. Prefer handling
            // literal AST nodes directly so we decode only user-provided
            // content.
            // Use shared helper from utils to decode common shell-style escapes
            // (\n, \t, \r, \\\) so the resulting string contains actual
            // control characters before we re-quote it for Perl source.

            match arg {
                Word::Literal(s, _) => {
                    // Strip outer shell quotes if present
                    let mut raw = s.clone();
                    // Debug prints removed: avoid polluting debashc stderr/stdout which
                    // can end up embedded into the generated Perl output.
                    if (raw.starts_with('"') && raw.ends_with('"'))
                        || (raw.starts_with('\'') && raw.ends_with('\''))
                    {
                        raw = raw[1..raw.len() - 1].to_string();
                    }
                    let decoded = crate::generator::utils::decode_shell_escapes_impl(&raw);
                    // Remember the decoded raw format for specifier counting
                    decoded_format = decoded.clone();
                    format_string = generator.perl_string_literal(&Word::literal(decoded));
                    // final perl literal stored in format_string
                }
                Word::StringInterpolation(interp, _) => {
                    let reconstructed = interp
                        .parts
                        .iter()
                        .map(|part| match part {
                            crate::ast::StringPart::Literal(s) => s.clone(),
                            _ => "".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    // Decode common shell-style escapes in the reconstructed literal
                    let decoded =
                        crate::generator::utils::decode_shell_escapes_impl(&reconstructed);
                    // Remember the decoded raw format for specifier counting
                    decoded_format = decoded.clone();
                    format_string = generator.perl_string_literal(&Word::literal(decoded));
                    // final perl literal for interpolation stored in format_string
                }
                _ => {
                    // Fallback: use the generic word_to_perl path and attempt
                    // to decode any surrounding quoting.
                    let mut tmp = generator.word_to_perl(arg);
                    if (tmp.starts_with('"') && tmp.ends_with('"'))
                        || (tmp.starts_with('\'') && tmp.ends_with('\''))
                    {
                        tmp = tmp[1..tmp.len() - 1].to_string();
                    }
                    let decoded = crate::generator::utils::decode_shell_escapes_impl(&tmp);
                    // Remember the decoded raw format for specifier counting
                    decoded_format = decoded.clone();
                    format_string = generator.perl_string_literal(&Word::literal(decoded));
                }
            }
        } else {
            // Subsequent arguments are the values to format
            args.push(generator.word_to_perl(arg));
        }
    }

    if format_string.is_empty() {
        // No format string provided, return error
        output.push_str("carp \"printf: no format string specified\";\n");
        output.push_str("exit 1;\n");
    } else {
        // Handle special case for array expansion like "${lines[@]}" or @"lines"
        if args.len() == 1
            && (args[0].contains("${") && args[0].contains("[@]")
                || args[0].contains("@") && !args[0].contains(" "))
        {
            // This is likely an array expansion like "${arr[@]}" or @"lines"
            let mut array_var = args[0].clone();

            // Extract the array name from "${arr[@]}" or @"lines"
            if array_var.starts_with('"') && array_var.ends_with('"') {
                array_var = array_var[1..array_var.len() - 1].to_string();
            }
            if array_var.starts_with('\'') && array_var.ends_with('\'') {
                array_var = array_var[1..array_var.len() - 1].to_string();
            }
            if array_var.starts_with('@') {
                array_var = array_var[1..].to_string();
            }
            if let Some(start) = array_var.find("${") {
                if let Some(end) = array_var.find("[@]") {
                    array_var = array_var[start + 2..end].to_string();
                }
            }

            // Generate Perl code to print array elements with the format
            if is_expression {
                output.push_str(&format!(
                    "join('', map {{ sprintf({}, $_) }} @{})",
                    format_string, array_var
                ));
            } else {
                output.push_str(&format!("foreach my $item (@{}) {{\n", array_var));
                // format_string already includes Perl quoting; emit it directly.
                output.push_str(&format!("    printf({}, $item);\n", format_string));
                output.push_str("}\n");
            }
        } else {
            // Regular printf with individual arguments
            // For printf, format string and arguments should be separate
            if args.is_empty() {
                if let Some(var) = output_var {
                    // Capture printf output to variable
                    output.push_str(&format!("my ${};\n", var));
                    output.push_str(&format!("{{\n"));
                    output.push_str(&format!("    local *STDOUT;\n"));
                    output.push_str(&format!(
                        "    open STDOUT, '>', \\${} or die \"Cannot redirect STDOUT\";\n",
                        var
                    ));
                    // format_string includes quoting; use it directly
                    output.push_str(&format!("    printf({});\n", format_string));
                    output.push_str(&format!("}}\n"));
                } else {
                    // In command-substitution contexts, return the formatted text.
                    output.push_str(&format!("sprintf({});\n", format_string));
                }
            } else {
                // When args are present, decide whether to emulate shell printf's
                // repeating-format behaviour. If the decoded format string
                // contains fewer non-%% specifiers than the number of
                // arguments, generate Perl code that repeats the format across
                // args. Otherwise emit a normal printf call.
                if args.is_empty() {
                    // (handled above) - keep for clarity
                }

                let formatted_args = args.join(", ");

                // Count non-%% specifiers in decoded_format. If decoded_format
                // wasn't set for some reason, fallback to inspecting
                // format_string (less ideal but safe).
                let fmt_for_count = if !decoded_format.is_empty() {
                    decoded_format.clone()
                } else {
                    // Strip surrounding quotes from format_string so we count raw %s
                    let mut s = format_string.clone();
                    if (s.starts_with('"') && s.ends_with('"'))
                        || (s.starts_with('\'') && s.ends_with('\''))
                    {
                        s = s[1..s.len() - 1].to_string();
                    }
                    s
                };

                let specifier_count = {
                    let mut chars = fmt_for_count.chars().peekable();
                    let mut count = 0usize;
                    while let Some(ch) = chars.next() {
                        if ch == '%' {
                            if chars.peek().map_or(false, |&n| n != '%') {
                                count += 1;
                            } else {
                                // consume the second '%' of '%%'
                                chars.next();
                            }
                        }
                    }
                    count
                };

                if specifier_count > 0 && args.len() > specifier_count {
                    if specifier_count == 1 {
                        // Map sprintf over each arg and join
                        output.push_str(&format!("do {{\n    my $result = join('', map {{ sprintf {}, $_ }} ({}));\n    $result;\n}}\n", format_string, formatted_args));
                    } else {
                        // Batch args into groups of specifier_count and pad final batch
                        output.push_str(&format!(
                            "do {{\n    my @__args = ({});\n    my $result = '';\n    while (@__args) {{\n        my @__batch = splice(@__args, 0, {});\n        push @__batch, ('') x ({} - scalar @__batch) if @__batch < {};\n        $result .= sprintf {}, @__batch;\n    }}\n    $result;\n}}\n",
                            formatted_args,
                            specifier_count,
                            specifier_count,
                            specifier_count,
                            format_string
                        ));
                    }
                } else {
                    // Emit a simple printf(format, args...)
                    let mut printf_call = format!(
                        "{}({}",
                        if is_expression { "sprintf" } else { "printf" },
                        format_string
                    );
                    for (_i, arg) in args.iter().enumerate() {
                        let raw_arg = arg.trim_matches(|c| c == '"' || c == '\'');
                        if raw_arg.chars().all(|c| c.is_ascii_digit() || c == '.')
                            && format_string.contains("%c")
                        {
                            printf_call.push_str(&format!(", ord(substr(\"{}\", 0, 1))", raw_arg));
                        } else {
                            printf_call.push_str(&format!(", {}", arg));
                        }
                    }
                    printf_call.push_str(");\n");

                    if let Some(var) = output_var {
                        output.push_str(&format!("my ${};\n", var));
                        output.push_str(&format!("{{\n"));
                        output.push_str(&format!("    local *STDOUT;\n"));
                        output.push_str(&format!(
                            "    open STDOUT, '>', \\${} or die \"Cannot redirect STDOUT\";\n",
                            var
                        ));
                        output.push_str(&format!("    {}\n", printf_call.trim()));
                        output.push_str(&format!("}}\n"));
                    } else {
                        output.push_str(&printf_call);
                    }
                }
            }
        }
    }

    output
}
