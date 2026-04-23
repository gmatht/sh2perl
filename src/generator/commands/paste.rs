use crate::ast::*;
use crate::generator::Generator;

pub fn generate_paste_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    process_sub_files: &[(String, String)],
) -> String {
    let mut result = String::new();

    if !process_sub_files.is_empty() {
        // Handle process substitution case - always return an expression-valued do-block
        if process_sub_files.len() >= 2 {
            let file1 = &process_sub_files[0];
            let file2 = &process_sub_files[1];

            // Read both files and paste them together
            let paste_id = generator.get_unique_file_handle();
            // Start expression block
            result.push_str(&format!("do {{\n"));
            result.push_str(&generator.indent());
            result.push_str(&format!("my @paste_file1_lines_{};\n", paste_id));
            result.push_str(&generator.indent());
            result.push_str(&format!("my @paste_file2_lines_{};\n", paste_id));

            // Read first file
            result.push_str(&generator.indent());
            result.push_str(&format!("if (open my $fh1, '<', ${}) {{\n", file1.0));
            result.push_str(&generator.indent());
            result.push_str("    while (my $line = <$fh1>) {\n");
            result.push_str(&generator.indent());
            result.push_str("        chomp $line;\n");
            result.push_str(&generator.indent());
            result.push_str(&format!(
                "        push @paste_file1_lines_{}, $line;\n",
                paste_id
            ));
            result.push_str(&generator.indent());
            result.push_str("    }\n");
            result.push_str(&generator.indent());
            result.push_str("    close $fh1 or croak \"Close failed: $OS_ERROR\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");

            // Read second file
            result.push_str(&generator.indent());
            result.push_str(&format!("if (open my $fh2, '<', ${}) {{\n", file2.0));
            result.push_str(&generator.indent());
            result.push_str("    while (my $line = <$fh2>) {\n");
            result.push_str(&generator.indent());
            result.push_str("        chomp $line;\n");
            result.push_str(&generator.indent());
            result.push_str(&format!(
                "        push @paste_file2_lines_{}, $line;\n",
                paste_id
            ));
            result.push_str(&generator.indent());
            result.push_str("    }\n");
            result.push_str(&generator.indent());
            result.push_str("    close $fh2 or croak \"Close failed: $OS_ERROR\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");

            // Paste the lines together
            result.push_str(&generator.indent());
            result.push_str(&format!("my $max_lines = scalar @paste_file1_lines_{} > scalar @paste_file2_lines_{} ? scalar @paste_file1_lines_{} : scalar @paste_file2_lines_{};\n", paste_id, paste_id, paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str("my $paste_output = q{};\n");
            result.push_str(&generator.indent());
            result.push_str("for my $i (0..$max_lines-1) {\n");
            result.push_str(&generator.indent());
            result.push_str(&format!("    my $line1 = $i < scalar @paste_file1_lines_{} ? $paste_file1_lines_{}[$i] : q{{}};\n", paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str(&format!("    my $line2 = $i < scalar @paste_file2_lines_{} ? $paste_file2_lines_{}[$i] : q{{}};\n", paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str("    $paste_output .= \"$line1\\t$line2\\n\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");
            // Return the computed string as the last expression of the do block
            result.push_str("$paste_output");
            result.push_str(&format!("\n{}}}", generator.indent()));
        }
    } else {
        // Handle regular paste command with file arguments
        // Detect '-' arguments and, when a pipeline buffer is active, read
        // the in-memory pipeline variable instead of opening the literal
        // filename '-' which would incorrectly try to open a file named '-'.
        let has_dash = cmd.args.iter().any(|arg| match arg {
            Word::Literal(s, _) => s == "-",
            _ => false,
        });

        // Prepare perl-literal versions for non-'-' args
        let perl_args: Vec<String> = cmd
            .args
            .iter()
            .map(|arg| generator.perl_string_literal(arg))
            .collect();

        // If there are '-' args and a pipeline output id is active, consume
        // the pipeline buffer in-memory. This reproduces the behavior of
        // "paste - -" when used in pipelines (group successive stdin lines
        // into columns).
        // Defensive fallback: if the generator doesn't currently have a
        // pipeline id active (due to generation-order quirks), try to find the
        // most-recently-declared output_<id> variable and use that. This is a
        // small, localized heuristic to make paste consume in-memory buffers
        // when they exist instead of attempting to open a literal '-' file.
        let mut effective_pipeline_id: Option<String> = None;
        if has_dash {
            if let Some(id) = generator.current_pipeline_output_id() {
                effective_pipeline_id = Some(id.clone());
            } else {
                // Look for the most recently declared local that matches output_<n>
                // by scanning declared_locals for the largest numeric suffix.
                let mut best_id: Option<(usize, String)> = None;
                for name in &generator.declared_locals {
                    if let Some(rest) = name.strip_prefix("output_") {
                        if let Ok(n) = rest.parse::<usize>() {
                            if best_id.as_ref().map(|(bn, _)| *bn).unwrap_or(0) < n {
                                best_id = Some((n, name.clone()));
                            }
                        }
                    }
                }
                if let Some((_n, name)) = best_id {
                    // name is like "output_123" - extract the numeric portion
                    if let Some(num) = name.strip_prefix("output_") {
                        effective_pipeline_id = Some(num.to_string());
                    }
                }
            }
        }

        if has_dash && effective_pipeline_id.is_some() {
            let paste_id = generator.get_unique_file_handle();

            // Start expression block
            result.push_str(&format!("do {{\n"));

            // Split the current pipeline buffer into lines
            let current_id = effective_pipeline_id.unwrap();
            let input_var = format!("output_{}", current_id);
            result.push_str(&generator.indent());
            result.push_str(&format!(
                "my @paste_stdin_lines_{} = split /\\n/msx, ${};\n",
                paste_id, input_var
            ));

            // Read any explicit filename arguments into arrays so they behave like files
            let mut file_index = 0usize;
            for (i, arg) in cmd.args.iter().enumerate() {
                match arg {
                    Word::Literal(s, _) if s == "-" => {
                        // stdin placeholder - no file to read
                    }
                    _ => {
                        file_index += 1;
                        let lit = &perl_args[i];
                        result.push_str(&generator.indent());
                        result.push_str(&format!(
                            "my @paste_file{}_lines_{};\n",
                            file_index, paste_id
                        ));
                        result.push_str(&generator.indent());
                        result.push_str(&format!(
                            "if (open my $fh{}_{} , '<', {}) {{\n",
                            file_index, paste_id, lit
                        ));
                        result.push_str(&generator.indent());
                        result.push_str("    while (my $line = <$fh");
                        // continue the while line (we need to inject file handle name)
                        result.push_str(&format!("{}_{}>) {{\n", file_index, paste_id));
                        result.push_str(&generator.indent());
                        result.push_str("        chomp $line;\n");
                        result.push_str(&generator.indent());
                        result.push_str(&format!(
                            "        push @paste_file{}_lines_{}, $line;\n",
                            file_index, paste_id
                        ));
                        result.push_str(&generator.indent());
                        result.push_str("    }\n");
                        result.push_str(&generator.indent());
                        result.push_str(&format!(
                            "    close $fh{}_{} or croak \"Close failed: $OS_ERROR\";\n",
                            file_index, paste_id
                        ));
                        result.push_str(&generator.indent());
                        result.push_str("}\n");
                    }
                }
            }

            // Build the pasted output by iterating rows. File args are indexed by
            // row number; '-' args consume the @paste_stdin_lines sequentially.
            result.push_str(&generator.indent());
            result.push_str("my $paste_output = q{};\n");
            result.push_str(&generator.indent());
            result.push_str(&format!("my $stdin_pos_{} = 0;\n", paste_id));
            result.push_str(&generator.indent());
            result.push_str(&format!("for (my $i = 0; ; $i++) {{\n"));

            // Per-argument handling inside the loop
            // We'll create per-arg temporary variables and push them into @parts
            result.push_str(&generator.indent());
            result.push_str("    my @parts = ();\n");
            result.push_str(&generator.indent());
            result.push_str("    my $row_has_data = 0;\n");

            // Track which file array index corresponds to each non-'-' arg
            let mut file_idx_for_arg = 0usize;
            for (i, arg) in cmd.args.iter().enumerate() {
                match arg {
                    Word::Literal(s, _) if s == "-" => {
                        // Consume next stdin item (if any) for this column
                        result.push_str(&generator.indent());
                        result.push_str(&format!("    my $val_{}_{} = $stdin_pos_{} < scalar @paste_stdin_lines_{} ? $paste_stdin_lines_{}[$stdin_pos_{}] : q{{}};\n", i, paste_id, paste_id, paste_id, paste_id, paste_id));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("    if ($val_{}_{} ne q{{}}) {{ $row_has_data = 1; $stdin_pos_{}++; }}\n", i, paste_id, paste_id));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("    push @parts, $val_{}_{};\n", i, paste_id));
                    }
                    _ => {
                        file_idx_for_arg += 1;
                        // Read from corresponding file array at index $i
                        result.push_str(&generator.indent());
                        result.push_str(&format!("    my $val_{}_{} = $i < scalar @paste_file{}_lines_{} ? $paste_file{}_lines_{}[$i] : q{{}};\n", i, paste_id, file_idx_for_arg, paste_id, file_idx_for_arg, paste_id));
                        result.push_str(&generator.indent());
                        result.push_str(&format!(
                            "    if ($val_{}_{} ne q{{}}) {{ $row_has_data = 1; }}\n",
                            i, paste_id
                        ));
                        result.push_str(&generator.indent());
                        result.push_str(&format!("    push @parts, $val_{}_{};\n", i, paste_id));
                    }
                }
            }

            result.push_str(&generator.indent());
            result.push_str("    last unless $row_has_data;\n");
            result.push_str(&generator.indent());
            result.push_str("    $paste_output .= join(\"\\t\", @parts) . \"\\n\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");
            // Return the computed string as the last expression of the do block
            result.push_str("$paste_output");
            result.push_str(&format!("\n{}}}", generator.indent()));
        } else if perl_args.len() >= 2 {
            // No '-' handling needed - fall back to file-based paste for two args
            let paste_id = generator.get_unique_file_handle();
            result.push_str(&format!("do {{\n"));
            result.push_str(&generator.indent());
            result.push_str(&format!("my @paste_file1_lines_{};\n", paste_id));
            result.push_str(&generator.indent());
            result.push_str(&format!("my @paste_file2_lines_{};\n", paste_id));

            // Read first file
            result.push_str(&generator.indent());
            result.push_str(&format!("if (open my $fh1, '<', {}) {{\n", perl_args[0]));
            result.push_str(&generator.indent());
            result.push_str("    while (my $line = <$fh1>) {\n");
            result.push_str(&generator.indent());
            result.push_str("        chomp $line;\n");
            result.push_str(&generator.indent());
            result.push_str(&format!(
                "        push @paste_file1_lines_{}, $line;\n",
                paste_id
            ));
            result.push_str(&generator.indent());
            result.push_str("    }\n");
            result.push_str(&generator.indent());
            result.push_str("    close $fh1 or croak \"Close failed: $OS_ERROR\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");

            // Read second file
            result.push_str(&generator.indent());
            result.push_str(&format!("if (open my $fh2, '<', {}) {{\n", perl_args[1]));
            result.push_str(&generator.indent());
            result.push_str("    while (my $line = <$fh2>) {\n");
            result.push_str(&generator.indent());
            result.push_str("        chomp $line;\n");
            result.push_str(&generator.indent());
            result.push_str(&format!(
                "        push @paste_file2_lines_{}, $line;\n",
                paste_id
            ));
            result.push_str(&generator.indent());
            result.push_str("    }\n");
            result.push_str(&generator.indent());
            result.push_str("    close $fh2 or croak \"Close failed: $OS_ERROR\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");

            // Paste the lines together
            result.push_str(&generator.indent());
            result.push_str(&format!("my $max_lines = scalar @paste_file1_lines_{} > scalar @paste_file2_lines_{} ? scalar @paste_file1_lines_{} : scalar @paste_file2_lines_{};\n", paste_id, paste_id, paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str("my $paste_output = q{};\n");
            result.push_str(&generator.indent());
            result.push_str("for my $i (0..$max_lines-1) {\n");
            result.push_str(&generator.indent());
            result.push_str(&format!("    my $line1 = $i < scalar @paste_file1_lines_{} ? $paste_file1_lines_{}[$i] : q{{}};\n", paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str(&format!("    my $line2 = $i < scalar @paste_file2_lines_{} ? $paste_file2_lines_{}[$i] : q{{}};\n", paste_id, paste_id));
            result.push_str(&generator.indent());
            result.push_str("    $paste_output .= \"$line1\\t$line2\\n\";\n");
            result.push_str(&generator.indent());
            result.push_str("}\n");
            result.push_str(&generator.indent());
            result.push_str("$paste_output");
            result.push_str(&format!("\n{}}}", generator.indent()));
        } else if !perl_args.is_empty() {
            // Fall back to running paste via the shell but capture its output
            // and return it as the value of the expression. Use the generator's
            // system-command serializer so arguments are quoted consistently and
            // embed it as a non-interpolating Perl literal to preserve shell
            // fragments (awk/sed/etc.). Set $CHILD_ERROR from the command exit
            // status so callers can inspect it like the shell would.
            let cmd_string = generator
                .generate_command_string_for_system(&crate::ast::Command::Simple(cmd.clone()));
            let cmd_lit =
                generator.perl_string_literal_no_interp(&crate::ast::Word::literal(cmd_string));
            result.push_str(&format!(
                "do {{ my $paste_cmd = {}; my $paste_output = qx{{$paste_cmd}}; $CHILD_ERROR = $? >> 8; $paste_output }}",
                cmd_lit
            ));
        } else {
            // No args: just run `paste` and return its captured output
            let cmd_lit = generator
                .perl_string_literal_no_interp(&crate::ast::Word::literal("paste".to_string()));
            result.push_str(&format!(
                "do {{ my $paste_cmd = {}; my $paste_output = qx{{$paste_cmd}}; $CHILD_ERROR = $? >> 8; $paste_output }}",
                cmd_lit
            ));
        }
    }

    result
}
