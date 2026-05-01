use crate::ast::*;
use crate::generator::Generator;

pub fn generate_xargs_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
) -> String {
    generate_xargs_command_with_output(
        generator,
        cmd,
        input_var,
        command_index,
        &format!("xargs_result_{}", command_index),
    )
}

pub fn generate_xargs_command_with_output(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
    output_var: &str,
) -> String {
    let mut output = String::new();

    let mut command = "echo";
    let mut args = Vec::new();
    let mut max_args = 1; // Default to 1 argument per command
                          // Optional placeholder value for -I
    let mut replace_placeholder: Option<String> = None;

    // Parse xargs arguments
    // We use a two-phase approach:
    //   Phase 1: parse xargs-level flags (-I, -n1, …) until the sub-command name is found.
    //   Phase 2: once the sub-command has been identified, every remaining argument
    //            (including flags starting with '-') belongs to that sub-command and is
    //            pushed into `args`. This prevents flags like `wc -l` from being silently
    //            dropped when xargs passes them through to its sub-command.
    let mut command_found = false;
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(arg_str, _) = &cmd.args[i] {
            if command_found {
                // We already know the sub-command; all remaining literals are its args.
                args.push(arg_str.clone());
                i += 1;
                continue;
            }
            // Detect -I and -I<placeholder> forms
            if arg_str == "-I" {
                if i + 1 < cmd.args.len() {
                    // The placeholder may be a Literal ("{}", REPLACEME, etc.) or a
                    // BraceExpansion that the shell parser produced for bare "{}".
                    let ph: Option<String> = match &cmd.args[i + 1] {
                        Word::Literal(ph, _) => Some(ph.clone()),
                        Word::BraceExpansion(be, _) => {
                            // Empty brace expansion {} → use the literal string "{}"
                            if be.items.is_empty() {
                                Some("{}".to_string())
                            } else {
                                // Non-empty expansion – stringify as {item1,item2}
                                Some(format!("{{{}}}", be.items.iter().map(|item| {
                                    match item {
                                        crate::ast_words::BraceItem::Literal(s) => s.clone(),
                                        _ => String::new(),
                                    }
                                }).collect::<Vec<_>>().join(",")))
                            }
                        }
                        _ => None,
                    };
                    if let Some(ph) = ph {
                        replace_placeholder = Some(ph);
                        i += 1; // consume placeholder
                    }
                }
            } else if arg_str.starts_with("-I") && arg_str.len() > 2 {
                replace_placeholder = Some(arg_str[2..].to_string());
            } else if arg_str == "grep" {
                command = "grep";
                command_found = true;
            } else if arg_str == "-n1" {
                max_args = 1;
            } else if arg_str == "function" {
                args.push("function".to_string());
            } else if !arg_str.starts_with('-') {
                // This is likely the command to execute
                command = arg_str;
                command_found = true;

                // Check if the next argument is a string interpolation (like "Number:")
                if i + 1 < cmd.args.len() {
                    if let Word::StringInterpolation(interp, _) = &cmd.args[i + 1] {
                        let pattern = interp
                            .parts
                            .iter()
                            .map(|part| match part {
                                StringPart::Literal(s) => s,
                                _ => ".*",
                            })
                            .collect::<Vec<_>>()
                            .into_iter()
                            .map(|s| s)
                            .collect::<String>();
                        args.push(pattern);
                        i += 1; // Skip the next argument since we processed it
                    }
                }
            }
        } else if let Word::StringInterpolation(interp, _) = &cmd.args[i] {
            let pattern = interp
                .parts
                .iter()
                .map(|part| match part {
                    StringPart::Literal(s) => s,
                    _ => ".*",
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|s| s)
                .collect::<String>();
            args.push(pattern);
        }
        i += 1;
    }

    if command == "grep" && args.contains(&"function".to_string()) {
        // Handle grep -l "function" on the input files
        output.push_str(&format!(
            "my @xargs_files_{} = split /\\n/msx, ${};\n",
            command_index, input_var
        ));
        output.push_str(&format!("my @xargs_matching_files_{};\n", command_index));
        output.push_str(&format!(
            "foreach my $file (@xargs_files_{}) {{\n",
            command_index
        ));
        output.push_str("next if !($file && -f $file);\n");
        output.push_str("if (open my $fh, '<', $file) {\n");
        output.push_str(&format!("my $xargs_found_{} = 0;\n", command_index));
        output.push_str("while (my $line = <$fh>) {\n");
        output.push_str(&format!(
            "if ($line =~ {}) {{\n",
            generator.format_regex_pattern("function")
        ));
        output.push_str(&format!("$xargs_found_{} = 1;\n", command_index));
        output.push_str("last;\n");
        output.push_str("}\n");
        output.push_str("}\n");
        output.push_str("close $fh or carp \"Close failed: $OS_ERROR\";\n");
        output.push_str(&format!(
            "if ($xargs_found_{}) {{ push @xargs_matching_files_{}, $file; }}\n",
            command_index, command_index
        ));
        output.push_str("}\n");
        output.push_str("}\n");
        // Write into a result variable expected by the pipeline
        output.push_str(&format!(
            "my ${} = join \"\\n\", @xargs_matching_files_{};\n",
            output_var, command_index
        ));
        // Ensure output ends with newline to match shell behavior
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "if (!(${} =~ {})) {{\n",
            output_var,
            generator.newline_end_regex()
        ));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("${} .= \"\\n\";\n", output_var));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else {
        // If -I was provided, build a reusable template of the command's
        // arguments (everything after the command name). We'll emit a
        // non-interpolating Perl literal for each template element so we can
        // safely substitute the placeholder at runtime.
        let mut xargs_template_literals: Vec<String> = Vec::new();
        if let Some(_) = &replace_placeholder {
            // Find the position of the command name (first non-flag word)
            let mut cmd_pos: Option<usize> = None;
            for (idx, arg) in cmd.args.iter().enumerate() {
                match arg {
                    Word::Literal(s, _) => {
                        if !s.starts_with('-') {
                            cmd_pos = Some(idx);
                            break;
                        }
                    }
                    Word::StringInterpolation(_, _) => {
                        // Treat an interpolation as a potential command name
                        cmd_pos = Some(idx);
                        break;
                    }
                    _ => {}
                }
            }

            if let Some(pos) = cmd_pos {
                for arg in cmd.args.iter().skip(pos + 1) {
                    // Emit each template element as a non-interpolating Perl literal
                    xargs_template_literals.push(generator.perl_string_literal_no_interp(arg));
                }
            }
        }
        // Handle xargs with command execution
        // Split input on newlines to preserve filenames that may contain spaces.
        output.push_str(&format!(
            "my @xargs_input_{} = split /\\n/msx, ${};\n",
            command_index, input_var
        ));
        output.push_str(&format!("my @xargs_output_{};\n", command_index));
        output.push_str(&format!(
            "for my $i (0..scalar @xargs_input_{}-1) {{\n",
            command_index
        ));
        output.push_str(&format!("    my @xargs_args_{};\n", command_index));
        output.push_str(&format!("    for my $j (0..{}-1) {{\n", max_args));
        output.push_str(&format!(
            "        push @xargs_args_{}, $xargs_input_{}[$i + $j];\n",
            command_index, command_index
        ));
        output.push_str("    }\n");

        if command == "echo" {
            // Handle echo command
            if let Some(ph) = &replace_placeholder {
                // Build a simple runtime template from provided args (joined with spaces)
                let joined = if args.is_empty() {
                    String::new()
                } else {
                    args.join(" ")
                };

                let template_literal = if joined.is_empty() {
                    "q{}".to_string()
                } else {
                    format!("q{{{}}}", joined)
                };

                output.push_str(&format!(
                    "    my $xargs_template_{} = {};\n",
                    command_index, template_literal
                ));
                output.push_str(&format!(
                    "    foreach my $arg (@xargs_args_{}) {{\n",
                    command_index
                ));
                output.push_str(&format!(
                    "        my $xargs_line_{} = $xargs_template_{};\n",
                    command_index, command_index
                ));
                output.push_str(&format!(
                    "        my $ph_{} = q{{{}}};\n",
                    command_index, ph
                ));
                output.push_str(&format!(
                    "        $xargs_line_{} =~ s/\\Q$ph_{}\\E/$arg/g;\n",
                    command_index, command_index
                ));
                output.push_str(&format!(
                    "        push @xargs_output_{}, $xargs_line_{};\n",
                    command_index, command_index
                ));
                output.push_str("    }\n");
            } else {
                output.push_str(&format!("    my $xargs_line_{} = q{{}};\n", command_index));

                // Add the echo prefix if we have args
                if !args.is_empty() {
                    output.push_str(&format!(
                        "    $xargs_line_{} .= \"{}\";\n",
                        command_index, args[0]
                    ));
                }

                // Add the input arguments
                output.push_str(&format!(
                    "    foreach my $arg (@xargs_args_{}) {{\n",
                    command_index
                ));
                output.push_str(&format!(
                    "        $xargs_line_{} .= q{{ }} . $arg;\n",
                    command_index
                ));
                output.push_str("    }\n");

                output.push_str(&format!(
                    "    push @xargs_output_{}, $xargs_line_{};\n",
                    command_index, command_index
                ));
            }
        } else {
            // Handle other commands
            output.push_str(&format!(
                "    my ($in_{}, $out_{}, $err_{});\n",
                command_index, command_index, command_index
            ));

            if replace_placeholder.is_some() && !xargs_template_literals.is_empty() {
                // Emit a template array and build per-invocation args with placeholder substitution
                output.push_str(&format!(
                    "    my @xargs_template_{} = ({});\n",
                    command_index,
                    xargs_template_literals.join(", ")
                ));

                output.push_str(&format!(
                    "    foreach my $item (@xargs_args_{}) {{\n",
                    command_index
                ));
                output.push_str(&format!(
                    "        my @args_for_invocation_{} = @xargs_template_{};\n",
                    command_index, command_index
                ));
                output.push_str(&format!(
                    "        my $ph_{} = q{{{}}};\n",
                    command_index,
                    replace_placeholder.as_ref().unwrap()
                ));
                output.push_str(&format!(
                    "        for my $k (0..$#args_for_invocation_{}) {{ $args_for_invocation_{}[$k] =~ s/\\Q$ph_{}\\E/$item/g; }}\n",
                    command_index, command_index, command_index
                ));
                // Call open3 with the substituted args
                output.push_str(&format!(
                    "        my $pid_{} = open3($in_{}, $out_{}, $err_{}, '{}', @args_for_invocation_{});\n",
                    command_index, command_index, command_index, command_index, command, command_index
                ));
                output.push_str(&format!(
                    "        close $in_{} or croak 'Close failed: $OS_ERROR';\n",
                    command_index
                ));
                output.push_str(&format!(
                    "        my $xargs_result_{} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <$out_{}> }};\n",
                    command_index, command_index
                ));
                output.push_str(&format!(
                    "        close $out_{} or croak 'Close failed: $OS_ERROR';\n",
                    command_index
                ));
                output.push_str(&format!("        waitpid $pid_{}, 0;\n", command_index));
                output.push_str(&format!("        chomp $xargs_result_{};\n", command_index));
                output.push_str(&format!(
                    "        push @xargs_output_{}, $xargs_result_{};\n",
                    command_index, command_index
                ));
                output.push_str("    }\n");
            } else {
                // No placeholder templates - fall back to previous behaviour.
                // Any extra args collected for the sub-command (e.g. flags like
                // `-l` in `xargs wc -l`) are prepended before the xargs input
                // items so the sub-command receives them correctly.
                let extra_args = if args.is_empty() {
                    String::new()
                } else {
                    args.iter()
                        .map(|a| format!("'{}'", a))
                        .collect::<Vec<_>>()
                        .join(", ")
                        + ", "
                };
                output.push_str(&format!(
                    "    my $pid_{} = open3($in_{}, $out_{}, $err_{}, '{}', {}@xargs_args_{});\n",
                    command_index,
                    command_index,
                    command_index,
                    command_index,
                    command,
                    extra_args,
                    command_index
                ));
                output.push_str(&format!(
                    "    close $in_{} or croak 'Close failed: $OS_ERROR';\n",
                    command_index
                ));
                output.push_str(&format!("    my $xargs_result_{} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <$out_{}> }};\n", command_index, command_index));
                output.push_str(&format!(
                    "    close $out_{} or croak 'Close failed: $OS_ERROR';\n",
                    command_index
                ));
                output.push_str(&format!("    waitpid $pid_{}, 0;\n", command_index));
                output.push_str(&format!("    chomp $xargs_result_{};\n", command_index));
                output.push_str(&format!(
                    "    push @xargs_output_{}, $xargs_result_{};\n",
                    command_index, command_index
                ));
            }
        }

        output.push_str("}\n");
        output.push_str(&format!(
            "my ${} = join \"\\n\", @xargs_output_{};\n",
            output_var, command_index
        ));
        // Ensure the output ends with a newline when non-empty.  In the real
        // shell, each xargs invocation of 'echo' appends a newline to its
        // output; downstream commands such as 'wc -l' rely on this.  Using
        // join("\n", …) above produces inter-element separators but no
        // trailing newline, so we add one here when necessary.
        output.push_str(&format!(
            "if (${} ne q{{}} && !( ${} =~ m{{\\n\\z}}msx )) {{ ${} .= \"\\n\"; }}\n",
            output_var, output_var, output_var
        ));

        // For pipeline context, also assign to the expected pipeline output variable
        if output_var.starts_with("xargs_result_") {
            // Extract the unique_id from the command_index (format: unique_id_i)
            let parts: Vec<&str> = command_index.split('_').collect();
            if parts.len() >= 2 {
                let unique_id = parts[0];
                output.push_str(&format!("$output_{} = ${};\n", unique_id, output_var));
            }
        }
    }
    output.push_str("\n");

    output
}
