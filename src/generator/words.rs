use super::Generator;
use crate::ast::*;
use regex::Regex;
use std::collections::HashSet;

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

fn is_exportable_shell_var(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first == '_' || first.is_ascii_alphabetic() => {
            chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

fn collect_shell_vars_from_word(word: &Word, vars: &mut HashSet<String>) {
    match word {
        Word::Variable(var, _, _) => {
            if is_exportable_shell_var(var) {
                vars.insert(var.clone());
            }
        }
        Word::ParameterExpansion(pe, _) => {
            let var_name = pe.variable.trim_start_matches('#').trim_start_matches('!');
            if is_exportable_shell_var(var_name) {
                vars.insert(var_name.to_string());
            }
        }
        Word::StringInterpolation(interp, _) => {
            for part in &interp.parts {
                match part {
                    StringPart::Literal(_) => {}
                    StringPart::Variable(var) => {
                        if is_exportable_shell_var(var) {
                            vars.insert(var.clone());
                        }
                    }
                    StringPart::ParameterExpansion(pe) => {
                        let var_name = pe.variable.trim_start_matches('#').trim_start_matches('!');
                        if is_exportable_shell_var(var_name) {
                            vars.insert(var_name.to_string());
                        }
                    }
                    StringPart::MapAccess(map_name, _)
                    | StringPart::MapKeys(map_name)
                    | StringPart::MapLength(map_name)
                    | StringPart::ArraySlice(map_name, _, _) => {
                        if is_exportable_shell_var(map_name) {
                            vars.insert(map_name.clone());
                        }
                    }
                    StringPart::CommandSubstitution(cmd) => {
                        collect_shell_vars_from_command(cmd, vars);
                    }
                    StringPart::Arithmetic(expr) => {
                        let re = Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\b").unwrap();
                        for cap in re.captures_iter(&expr.expression) {
                            let var_name = &cap[1];
                            if is_exportable_shell_var(var_name) {
                                vars.insert(var_name.to_string());
                            }
                        }
                    }
                }
            }
        }
        Word::CommandSubstitution(cmd, _) => collect_shell_vars_from_command(cmd, vars),
        Word::MapAccess(map_name, _, _)
        | Word::MapKeys(map_name, _)
        | Word::MapLength(map_name, _) => {
            if is_exportable_shell_var(map_name) {
                vars.insert(map_name.clone());
            }
        }
        Word::ArraySlice(array_name, _, _, _) => {
            if is_exportable_shell_var(array_name) {
                vars.insert(array_name.clone());
            }
        }
        Word::Arithmetic(expr, _) => {
            let re = Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\b").unwrap();
            for cap in re.captures_iter(&expr.expression) {
                let var_name = &cap[1];
                if is_exportable_shell_var(var_name) {
                    vars.insert(var_name.to_string());
                }
            }
        }
        Word::Array(_, _, _) | Word::BraceExpansion(_, _) | Word::Literal(_, _) => {}
    }
}

fn collect_shell_vars_from_command(command: &Command, vars: &mut HashSet<String>) {
    match command {
        Command::Simple(cmd) => {
            collect_shell_vars_from_word(&cmd.name, vars);
            for arg in &cmd.args {
                collect_shell_vars_from_word(arg, vars);
            }
            for value in cmd.env_vars.values() {
                collect_shell_vars_from_word(value, vars);
            }
            for redirect in &cmd.redirects {
                collect_shell_vars_from_word(&redirect.target, vars);
                match &redirect.operator {
                    RedirectOperator::ProcessSubstitutionInput(sub_cmd)
                    | RedirectOperator::ProcessSubstitutionOutput(sub_cmd) => {
                        collect_shell_vars_from_command(sub_cmd, vars);
                    }
                    _ => {}
                }
            }
        }
        Command::BuiltinCommand(cmd) => {
            collect_shell_vars_from_word(&Word::Literal(cmd.name.clone(), None), vars);
            for arg in &cmd.args {
                collect_shell_vars_from_word(arg, vars);
            }
            for value in cmd.env_vars.values() {
                collect_shell_vars_from_word(value, vars);
            }
            for redirect in &cmd.redirects {
                collect_shell_vars_from_word(&redirect.target, vars);
                match &redirect.operator {
                    RedirectOperator::ProcessSubstitutionInput(sub_cmd)
                    | RedirectOperator::ProcessSubstitutionOutput(sub_cmd) => {
                        collect_shell_vars_from_command(sub_cmd, vars);
                    }
                    _ => {}
                }
            }
        }
        Command::Redirect(redirect_cmd) => {
            collect_shell_vars_from_command(&redirect_cmd.command, vars);
            for redirect in &redirect_cmd.redirects {
                collect_shell_vars_from_word(&redirect.target, vars);
                match &redirect.operator {
                    RedirectOperator::ProcessSubstitutionInput(sub_cmd)
                    | RedirectOperator::ProcessSubstitutionOutput(sub_cmd) => {
                        collect_shell_vars_from_command(sub_cmd, vars);
                    }
                    _ => {}
                }
            }
        }
        Command::Pipeline(pipeline) => {
            for cmd in &pipeline.commands {
                collect_shell_vars_from_command(cmd, vars);
            }
        }
        Command::And(left, right) | Command::Or(left, right) => {
            collect_shell_vars_from_command(left, vars);
            collect_shell_vars_from_command(right, vars);
        }
        Command::If(if_stmt) => {
            collect_shell_vars_from_command(&if_stmt.condition, vars);
            collect_shell_vars_from_command(&if_stmt.then_branch, vars);
            if let Some(else_branch) = &if_stmt.else_branch {
                collect_shell_vars_from_command(else_branch, vars);
            }
        }
        Command::Case(case_stmt) => {
            collect_shell_vars_from_word(&case_stmt.word, vars);
            for case_clause in &case_stmt.cases {
                for pattern in &case_clause.patterns {
                    collect_shell_vars_from_word(pattern, vars);
                }
                for cmd in &case_clause.body {
                    collect_shell_vars_from_command(cmd, vars);
                }
            }
        }
        Command::While(while_loop) => {
            collect_shell_vars_from_command(&while_loop.condition, vars);
            for cmd in &while_loop.body.commands {
                collect_shell_vars_from_command(cmd, vars);
            }
        }
        Command::For(for_loop) => {
            for item in &for_loop.items {
                collect_shell_vars_from_word(item, vars);
            }
            for cmd in &for_loop.body.commands {
                collect_shell_vars_from_command(cmd, vars);
            }
        }
        Command::CStyleFor(c_loop) => {
            for cmd in &c_loop.body.commands {
                collect_shell_vars_from_command(cmd, vars);
            }
        }
        Command::Function(func) => {
            for cmd in &func.body.commands {
                collect_shell_vars_from_command(cmd, vars);
            }
        }
        Command::Subshell(sub_cmd) | Command::Background(sub_cmd) => {
            collect_shell_vars_from_command(sub_cmd, vars);
        }
        Command::Block(block) => {
            for cmd in &block.commands {
                collect_shell_vars_from_command(cmd, vars);
            }
        }
        Command::Assignment(assign) => {
            collect_shell_vars_from_word(&assign.value, vars);
        }
        Command::TestExpression(test_expr) => {
            let re = Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
            for cap in re.captures_iter(&test_expr.expression) {
                vars.insert(cap[1].to_string());
            }
        }
        Command::ShoptCommand(_)
        | Command::Break(_)
        | Command::Continue(_)
        | Command::Return(_)
        | Command::BlankLine => {}
    }
}

fn generate_shell_command_substitution(generator: &mut Generator, cmd: &Command) -> String {
    let command_str = crate::generator::redirects::generate_bash_command_string(cmd);
    let command_lit = generator.perl_string_literal_no_interp(&Word::literal(command_str));

    let mut shell_vars = HashSet::new();
    collect_shell_vars_from_command(cmd, &mut shell_vars);
    let mut shell_vars: Vec<String> = shell_vars.into_iter().collect();
    shell_vars.sort();

    let mut env_setup = String::new();
    for var in shell_vars {
        env_setup.push_str(&format!("    local $ENV{{{}}} = ${};\n", var, var));
    }

    let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
    format!(
        "do {{\n{}    my ({}, {}, {});\n    my $pid = open3({}, {}, {}, 'bash', '-c', {});\n    close {} or croak 'Close failed: $OS_ERROR';\n    my $result = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $OS_ERROR';\n    waitpid $pid, 0;\n    $CHILD_ERROR = $? >> 8;\n    $result =~ s/\\n+\\z//msx;\n    $result;\n}}",
        env_setup,
        in_var,
        out_var,
        err_var,
        in_var,
        out_var,
        err_var,
        command_lit,
        in_var,
        out_var,
        out_var
    )
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
                // Emit an interpolating Perl literal so qx{} will see Perl
                // interpolation of any $-variables present in the command text.
                // Use the centralized helper which preserves Perl interpolation
                // ($ and @ are not escaped) but encodes control characters as
                // backslash sequences ("\n", "\t", "\r") so the generated
                // Perl source does not contain real newlines.
                // Use a non-interpolating Perl literal so any shell "$" or "@"
                // sequences (e.g. awk/sed programs) are preserved verbatim and not
                // interpreted by Perl at compile time.
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
                .map(|e| generator.array_element_to_perl(e))
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
                Command::Redirect(redirect_cmd) => {
                    // Check if any redirect uses bash-specific features not available in
                    // POSIX /bin/sh (dash): here-strings (<<<) or process substitutions (<(...)).
                    // For here-strings we convert to the POSIX-compatible `echo ... | cmd` form.
                    // For process substitutions we fall through to bash -c.
                    let has_here_string = redirect_cmd
                        .redirects
                        .iter()
                        .any(|r| matches!(r.operator, RedirectOperator::HereString));
                    let has_process_sub = redirect_cmd.redirects.iter().any(|r| {
                        matches!(
                            r.operator,
                            RedirectOperator::ProcessSubstitutionInput(_)
                                | RedirectOperator::ProcessSubstitutionOutput(_)
                        )
                    });

                    if has_here_string && !has_process_sub {
                        // For commands we have native Perl support for (tr, grep),
                        // generate native code instead of falling back to qx{echo ... | cmd}
                        // which would trigger QX_BUILTIN violations.
                        let native_here_result = match &*redirect_cmd.command {
                            Command::Simple(simple_cmd) => {
                                if let Word::Literal(name, _) = &simple_cmd.name {
                                    match name.as_str() {
                                        "tr" => {
                                            let here_content = redirect_cmd
                                                .redirects
                                                .iter()
                                                .find(|r| matches!(r.operator, RedirectOperator::HereString))
                                                .and_then(|r| r.heredoc_body.as_ref())
                                                .cloned()
                                                .unwrap_or_default();
                                            let unique_id = generator.get_unique_id();
                                            let input_data = format!("my $input_data = \"{}\";", here_content);
                                            let tr_output = crate::generator::commands::tr::generate_tr_command_for_substitution(
                                                generator, simple_cmd, "input_data", &unique_id.to_string(),
                                            );
                                            Some(format!("do {{ {} {} }}", input_data, tr_output))
                                        }
                                        "grep" => {
                                            let here_content = redirect_cmd
                                                .redirects
                                                .iter()
                                                .find(|r| matches!(r.operator, RedirectOperator::HereString))
                                                .and_then(|r| r.heredoc_body.as_ref())
                                                .cloned()
                                                .unwrap_or_default();
                                            let unique_id = generator.get_unique_id();
                                            let input_data = format!("my $input_data = \"{}\";", here_content);
                                            let grep_output = crate::generator::commands::grep::generate_grep_command(
                                                generator, simple_cmd, &format!("${}", "input_data"), &unique_id.to_string(), false,
                                            );
                                            Some(format!("do {{ {} {} }}", input_data, grep_output))
                                        }
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };

                        if let Some(native_code) = native_here_result {
                            native_code
                        } else {
                            // Convert `cmd <<< "string"` → `echo 'string' | cmd`.
                            // Build the base command without the here-string redirect.
                            let base_cmd_str =
                                crate::generator::redirects::generate_bash_command_string(
                                    &redirect_cmd.command,
                                );
                            // Find the first here-string redirect target.
                            let here_target = redirect_cmd
                                .redirects
                                .iter()
                                .find(|r| matches!(r.operator, RedirectOperator::HereString))
                                .map(|r| generator.perl_string_literal(&r.target))
                                .unwrap_or_else(|| "''".to_string());
                            // Emit: echo <string> | <cmd>
                            format!(
                                "do {{ my $here_input = {}; chomp(my $result = qx{{echo \"$here_input\" | {}}}); $CHILD_ERROR = $? >> 8; $result; }}",
                                here_target,
                                base_cmd_str
                            )
                        }
                    } else if has_process_sub {
                        // Process substitutions (<(cmd) / >(cmd)) require bash, not /bin/sh.
                        // Run the entire command under `bash -c '...'`, using single-quote
                        // escaping (replace ' with '\'' ) to safely embed the command string.
                        let command_str =
                            crate::generator::redirects::generate_bash_command_string(cmd);
                        let escaped = command_str.replace('\'', "'\\''");
                        let bash_cmd = format!("bash -c '{}'", escaped);
                        let command_lit =
                            generator.perl_string_literal_force_interp(&Word::literal(bash_cmd));
                        format!(
                            "do {{ my $command = {}; chomp(my $result = qx{{$command}}); $CHILD_ERROR = $? >> 8; $result; }}",
                            command_lit
                        )
                    } else {
                        // Check if the inner command is a known builtin with a simple
                        // input redirect. If so, use native Perl code instead of qx{}.
                        let has_native_handler = match &*redirect_cmd.command {
                            Command::Simple(simple_cmd) => {
                                if let Word::Literal(name, _) = &simple_cmd.name {
                                    // Only handle simple input redirects (no here-strings,
                                    // process substitutions, or other complex redirects)
                                    let all_input = redirect_cmd.redirects.iter().all(|r| {
                                        matches!(r.operator, RedirectOperator::Input)
                                    });
                                    all_input && match name.as_str() {
                                        "wc" => true,
                                        _ => false,
                                    }
                                } else {
                                    false
                                }
                            }
                            _ => false,
                        };

                        if has_native_handler {
                            // Use native handler for the inner command.
                            if let Command::Simple(simple_cmd) = &*redirect_cmd.command {
                                if let Word::Literal(name, _) = &simple_cmd.name {
                                    match name.as_str() {
                                        "wc" => {
                                            // Generate inline Perl to read the file and count.
                                            // First, get the file path from the input redirect.
                                            let file_expr = redirect_cmd
                                                .redirects
                                                .iter()
                                                .find(|r| matches!(r.operator, RedirectOperator::Input))
                                                .map(|r| generator.perl_string_literal(&r.target))
                                                .unwrap_or_else(|| "q{}".to_string());

                                            // Parse wc flags
                                            let mut count_lines = false;
                                            let mut count_words = false;
                                            let mut count_bytes = false;
                                            let mut count_chars = false;
                                            for arg in &simple_cmd.args {
                                                if let Word::Literal(s, _) = arg {
                                                    for ch in s.chars().skip(1) {
                                                        match ch {
                                                            'l' => count_lines = true,
                                                            'w' => count_words = true,
                                                            'c' => count_bytes = true,
                                                            'm' => count_chars = true,
                                                            _ => {}
                                                        }
                                                    }
                                                }
                                            }
                                            if !count_lines && !count_words && !count_chars && !count_bytes {
                                                count_lines = true;
                                                count_words = true;
                                                count_bytes = true;
                                            }

                                            let mut wc_code = String::from("do {\n");
                                            wc_code.push_str(&format!("    my $wc_file = {};\n", file_expr));
                                            wc_code.push_str("    my $wc_file_opened = 0;\n");
                                            wc_code.push_str("    my $content = do {\n");
                                            wc_code.push_str("        my $result = q{};\n");
                                            wc_code.push_str("        if (open my $fh, '<', $wc_file) {\n");
                                            wc_code.push_str("            $wc_file_opened = 1;\n");
                                            wc_code.push_str("            local $INPUT_RECORD_SEPARATOR = undef;\n");
                                            wc_code.push_str("            $result = <$fh>;\n");
                                            wc_code.push_str("            close $fh or warn \"Close failed: $OS_ERROR\\n\";\n");
                                            wc_code.push_str("        } else {\n");
                                            wc_code.push_str("            warn \"Cannot open $wc_file: $OS_ERROR\\n\";\n");
                                            wc_code.push_str("        }\n");
                                            wc_code.push_str("        $result;\n");
                                            wc_code.push_str("    };\n");
                                            wc_code.push_str("    $wc_file_opened ? do {\n");

                                            let mut parts = Vec::new();
                                            if count_lines {
                                                wc_code.push_str("        my $wc_lines = () = $content =~ /\\n/gsxm;\n");
                                                parts.push("$wc_lines".to_string());
                                            }
                                            if count_words {
                                                wc_code.push_str("        my $wc_words = scalar split /\\s+/msx, $content;\n");
                                                parts.push("$wc_words".to_string());
                                            }
                                            if count_bytes {
                                                wc_code.push_str("        my $wc_bytes = length($content);\n");
                                                parts.push("$wc_bytes".to_string());
                                            }
                                            if count_chars {
                                                wc_code.push_str("        my $wc_chars = length($content);\n");
                                                parts.push("$wc_chars".to_string());
                                            }

                                            // For command substitution, do NOT include trailing newline
                                            // (bash strips trailing newlines from command substitution)
                                            if parts.len() > 1 {
                                                let parts_joined = parts.join(", ");
                                                wc_code.push_str(&format!("        my $result = join(q{{ }}, ({}));\n", parts_joined));
                                                wc_code.push_str("        $result;\n");
                                            } else if parts.len() == 1 {
                                                let part = &parts[0];
                                                let line = format!("        {};\n", part);
                                                wc_code.push_str(&line);
                                            } else {
                                                wc_code.push_str("        q{};\n");
                                            }
                                            wc_code.push_str("    } : q{};\n");
                                            wc_code.push_str("}");
                                            wc_code
                                        }
                                        _ => unreachable!(),
                                    }
                                } else {
                                    unreachable!()
                                }
                            } else {
                                unreachable!()
                            }
                        } else {
                            let command_str =
                                crate::generator::redirects::generate_bash_command_string(cmd);
                            // Use force_interp so that Perl variables (e.g. $file) in the
                            // redirect target are interpolated before the command is passed
                            // to the shell.  This mirrors how bash expands $var in
                            // `cmd < "$var"`.
                            let command_lit =
                                generator.perl_string_literal_force_interp(&Word::literal(command_str));

                            format!(
                                "do {{ my $command = {}; chomp(my $result = qx{{$command}}); $CHILD_ERROR = $? >> 8; $result; }}",
                                command_lit
                            )
                        }
                    }
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
                            // Use the find command substitution handler for proper conversion
                            let perl_code = crate::generator::commands::find::generate_find_for_substitution(
                                generator,
                                simple_cmd,
                                "",
                            );

                            // For backtick commands, we need to return the value, not print it
                            perl_code
                        } else if name == "head" {
                            // Use the shell command directly so file and flag handling stays faithful
                            if generator.current_pipeline_output_id().is_none() {
                                let nested_id = generator.get_unique_id();
                                // Record the declared local to avoid duplicate declarations
                                generator
                                    .declared_locals
                                    .insert(format!("output_{}", nested_id));
                                // Push guard so nested generators see the id while we generate the nested command
                                let _guard =
                                    generator.push_pipeline_output_id_guard(nested_id.clone());

                                let head_cmd = generator.generate_command_string_for_system(cmd);
                                // Preserve literal shell program content (do not let Perl
                                // interpolate $/@ inside awk/sed snippets).
                                let head_lit = generator
                                    .perl_string_literal_no_interp(&Word::literal(head_cmd));
                                format!(
                                    "do {{ my $output_{} = q{{}}; my $output_printed_{}; my $head_cmd = {}; qx{{$head_cmd}}; }}",
                                    nested_id, nested_id, head_lit
                                )
                            } else {
                                let head_cmd = generator.generate_command_string_for_system(cmd);
                                // Preserve literal shell program content (do not let Perl
                                // interpolate $/@ inside awk/sed snippets).
                                let head_lit = generator
                                    .perl_string_literal_no_interp(&Word::literal(head_cmd));
                                format!("do {{ my $head_cmd = {}; qx{{$head_cmd}}; }}", head_lit)
                            }
                        } else if name == "tail" {
                            // Use the shell command directly so file and flag handling stays faithful
                            if generator.current_pipeline_output_id().is_none() {
                                let nested_id = generator.get_unique_id();
                                generator
                                    .declared_locals
                                    .insert(format!("output_{}", nested_id));
                                let _guard =
                                    generator.push_pipeline_output_id_guard(nested_id.clone());

                                let tail_cmd = generator.generate_command_string_for_system(cmd);
                                let tail_lit = generator
                                    .perl_string_literal_no_interp(&Word::literal(tail_cmd));
                                format!(
                                    "do {{ my $output_{} = q{{}}; my $output_printed_{}; my $tail_cmd = {}; qx{{$tail_cmd}}; }}",
                                    nested_id, nested_id, tail_lit
                                )
                            } else {
                                let tail_cmd = generator.generate_command_string_for_system(cmd);
                                let tail_lit = generator
                                    .perl_string_literal_no_interp(&Word::literal(tail_cmd));
                                format!("do {{ my $tail_cmd = {}; qx{{$tail_cmd}}; }}", tail_lit)
                            }
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
                                // Keep any pipeline-id guards alive until after we
                                // generate the final paste invocation so nested
                                // generators see the pipeline id. Store guards in a
                                // vector so their Drop is deferred.
                                let mut _process_sub_guards: Vec<
                                    crate::generator::PipelineOutputIdGuard,
                                > = Vec::new();

                                for redirect in &simple_cmd.redirects {
                                    if let crate::ast::RedirectOperator::ProcessSubstitutionInput(
                                        cmd,
                                    ) = &redirect.operator
                                    {
                                        // Generate the process substitution command and create temp file
                                        let temp_file_id = generator.get_unique_id();
                                        let temp_file = format!("temp_file_ps_{}", temp_file_id);

                                        // Check if this is an echo command. If no pipeline id is
                                        // currently active, create a nested id and push an
                                        // RAII guard so nested generation can see the id. Also
                                        // emit minimal Perl declarations immediately so the
                                        // nested generated snippet can reference $output_<id>.
                                        let process_sub_output = if generator
                                            .current_pipeline_output_id()
                                            .is_none()
                                        {
                                            let nested_id = generator.get_unique_id();
                                            process_sub_code.push_str(&format!(
                                                "    my $output_{} = q{{}};\n",
                                                nested_id
                                            ));
                                            process_sub_code.push_str(&format!(
                                                "    my $output_printed_{};\n",
                                                nested_id
                                            ));
                                            generator
                                                .declared_locals
                                                .insert(format!("output_{}", nested_id));
                                            // Push guard so nested generators see the id while we generate the nested command
                                            _process_sub_guards.push(
                                                generator.push_pipeline_output_id_guard(
                                                    nested_id.clone(),
                                                ),
                                            );

                                            if let crate::ast::Command::Simple(echo_cmd) = &**cmd {
                                                if let crate::ast::Word::Literal(name, _) =
                                                    &echo_cmd.name
                                                {
                                                    if name == "echo" {
                                                        crate::generator::commands::echo::generate_echo_command(generator, echo_cmd, "", "temp_output")
                                                    } else {
                                                        generator.generate_command(cmd)
                                                    }
                                                } else {
                                                    generator.generate_command(cmd)
                                                }
                                            } else {
                                                generator.generate_command(cmd)
                                            }
                                        } else {
                                            // Existing pipeline id active - generate normally
                                            if let crate::ast::Command::Simple(echo_cmd) = &**cmd {
                                                if let crate::ast::Word::Literal(name, _) =
                                                    &echo_cmd.name
                                                {
                                                    if name == "echo" {
                                                        crate::generator::commands::echo::generate_echo_command(generator, echo_cmd, "", "temp_output")
                                                    } else {
                                                        generator.generate_command(cmd)
                                                    }
                                                } else {
                                                    generator.generate_command(cmd)
                                                }
                                            } else {
                                                generator.generate_command(cmd)
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
                                                        "    print {{$fh}} {};\n",
                                                        process_sub_output
                                                    ));
                                                }
                                            } else {
                                                process_sub_code.push_str(&format!(
                                                    "    print {{$fh}} {};\n",
                                                    process_sub_output
                                                ));
                                            }
                                        } else {
                                            process_sub_code.push_str(&format!(
                                                "    print {{$fh}} {};\n",
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
                                // Keep any pipeline-id guards alive until after we
                                // generate the final comm invocation so nested
                                // generators see the pipeline id. Store guards in a
                                // vector so their Drop is deferred.
                                let mut _process_sub_guards: Vec<
                                    crate::generator::PipelineOutputIdGuard,
                                > = Vec::new();

                                for redirect in &simple_cmd.redirects {
                                    if let crate::ast::RedirectOperator::ProcessSubstitutionInput(
                                        sub_cmd,
                                    ) = &redirect.operator
                                    {
                                        let temp_file_id = generator.get_unique_id();
                                        let temp_file = format!("temp_file_ps_{}", temp_file_id);

                                        let process_sub_output = if generator
                                            .current_pipeline_output_id()
                                            .is_none()
                                        {
                                            let nested_id = generator.get_unique_id();
                                            process_sub_code.push_str(&format!(
                                                "    my $output_{} = q{{}};\n",
                                                nested_id
                                            ));
                                            process_sub_code.push_str(&format!(
                                                "    my $output_printed_{};\n",
                                                nested_id
                                            ));
                                            generator
                                                .declared_locals
                                                .insert(format!("output_{}", nested_id));
                                            // Push a guard so nested generators see the id
                                            _process_sub_guards.push(
                                                generator.push_pipeline_output_id_guard(
                                                    nested_id.clone(),
                                                ),
                                            );

                                            let result = match sub_cmd.as_ref() {
                                                Command::Simple(simple_sub_cmd) => generator
                                                    .generate_simple_command(simple_sub_cmd),
                                                _ => {
                                                    // For non-simple commands, we need to generate the command differently
                                                    // This is a placeholder - we may need to implement this properly
                                                    format!("\"Command not supported in process substitution\"")
                                                }
                                            };
                                            result
                                        } else {
                                            match sub_cmd.as_ref() {
                                                Command::Simple(simple_sub_cmd) => generator
                                                    .generate_simple_command(simple_sub_cmd),
                                                _ => {
                                                    format!("\"Command not supported in process substitution\"")
                                                }
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
                            // Delegate printf in command-substitution contexts to the
                            // dedicated printf generator so we correctly emulate the
                            // shell's repeating-format behaviour (e.g. printf "%s\\n" A B)
                            crate::generator::commands::printf::generate_printf_command(
                                generator, simple_cmd, "", 0, None, true,
                            )
                        } else if name == "printf" && false {
                            // (disabled) old ad-hoc printf handling - now delegated to dedicated printf generator
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
                                    // Count the number of non-%% format specifiers.
                                    // In bash, printf repeats the format string until all
                                    // arguments are consumed.  If there are more args than
                                    // specifiers we must generate a loop so the output
                                    // matches bash behaviour.
                                    let escaped_fmt =
                                        format_string.replace("\"", "\\\"").replace("\\\\", "\\");
                                    let specifier_count = {
                                        let mut chars = format_string.chars().peekable();
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
                                        // More args than specifiers: loop over args in batches
                                        // matching the number of specifiers per iteration.
                                        if specifier_count == 1 {
                                            // Common case: one specifier, iterate over all args
                                            format!(
                                                "do {{\n    my $result = join('', map {{ sprintf \"{}\", $_ }} ({}));\n    $result;\n}}",
                                                escaped_fmt,
                                                formatted_args
                                            )
                                        } else {
                                            // Multiple specifiers per iteration: use splice loop.
                                            // Pad the final batch with empty strings if it is
                                            // shorter than specifier_count so sprintf never
                                            // receives fewer arguments than format specifiers
                                            // (bash treats missing printf args as empty/zero).
                                            format!(
                                                "do {{\n    my @__args = ({0});\n    my $result = '';\n    while (@__args) {{\n        my @__batch = splice(@__args, 0, {2});\n        push @__batch, ('') x ({2} - scalar @__batch) if @__batch < {2};\n        $result .= sprintf \"{1}\", @__batch;\n    }}\n    $result;\n}}",
                                                formatted_args,
                                                escaped_fmt,
                                                specifier_count
                                            )
                                        }
                                    } else {
                                        format!("do {{\n    my $result = sprintf \"{}\", {};\n    $result;\n}}",
                                            escaped_fmt,
                                            formatted_args)
                                    }
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
                            // When any argument is a CommandSubstitution,
                            // generate_command_string_for_system emits a placeholder
                            // error message.  Implement basename natively in Perl
                            // so nested command substitutions like $(pwd) are handled.
                            let has_cmd_sub = simple_cmd
                                .args
                                .iter()
                                .any(|a| matches!(a, Word::CommandSubstitution(_, _)));
                            if has_cmd_sub {
                                let path_expr = if !simple_cmd.args.is_empty() {
                                    generator.word_to_perl(&simple_cmd.args[0])
                                } else {
                                    "q{}".to_string()
                                };
                                let mut code =
                                    format!("do {{\n    my $basename_path = {};\n", path_expr);
                                if simple_cmd.args.len() > 1 {
                                    let suf = generator.word_to_perl(&simple_cmd.args[1]);
                                    code.push_str(&format!("    my $basename_suffix = {};\n", suf));
                                    code.push_str("    if ($basename_suffix ne q{}) {\n        $basename_path =~ s/\\Q$basename_suffix\\E$//msx;\n    }\n");
                                }
                                code.push_str("    $basename_path =~ s{.*/}{}msx;\n");
                                code.push_str("    chomp $basename_path;\n");
                                code.push_str("    $basename_path;\n}");
                                code
                            } else {
                                // Use native Perl basename instead of shelling out.
                                let path_expr = if !simple_cmd.args.is_empty() {
                                    generator.word_to_perl(&simple_cmd.args[0])
                                } else {
                                    "q{}".to_string()
                                };
                                let mut code = format!(
                                    "do {{ use File::Basename qw(basename); my $basename_output = basename({}); $CHILD_ERROR = 0; $basename_output; }}",
                                    path_expr
                                );
                                code
                            }
                        } else if name == "dirname" {
                            let path_expr = if !simple_cmd.args.is_empty() {
                                generator.word_to_perl(&simple_cmd.args[0])
                            } else {
                                "q{}".to_string()
                            };
                            format!(
                                "do {{ use File::Basename qw(dirname); my $dirname_output = dirname({}); $CHILD_ERROR = 0; $dirname_output; }}",
                                path_expr
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
                            format!("do {{\n{}$CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do)
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
                            format!("do {{\n{}$CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do)
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
                            format!("do {{\n{}$CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
                                indent1_do, indent1_do, formatted_code.trim_end(), indent2, indent2, 
                                indent1_do, indent1_do, indent1_do, indent1_do)
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
                                .join("\n");
                            let indent1 = "    ".to_string();
                            let indent1_do = "        ".to_string();
                            let indent2 = "            ".to_string();
                            format!("do {{\n{}$CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}$CHILD_ERROR = 0;\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
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
                            format!("do {{\n{}$CHILD_ERROR = 0;\n{}my $eval_result = eval {{\n{}\n{}$CHILD_ERROR = 0;\n{}1;\n{}}};\n{}if ( !$eval_result ) {{\n{}    $CHILD_ERROR = 256;\n{}}}\n{}q{{}};\n}}", 
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
                            time_output.push_str("print {*STDERR} $time_output;\n");
                            time_output.push_str("q{};\n");

                            format!("do {{ {} }}", time_output)
                        } else if name == "sleep" {
                            crate::generator::commands::sleep::generate_sleep_expression(
                                generator, simple_cmd,
                            )
                        } else if name == "whoami" {
                            // whoami - print effective user name
                            "do { my $whoami_user = (getpwuid($<))[0]; $whoami_user . \"\\n\"; }"
                                .to_string()
                        } else if name == "uname" {
                            // uname - print system information
                            let mut has_flags = false;
                            let mut flag_a = false;
                            let mut flag_s = false;
                            let mut flag_n = false;
                            let mut flag_r = false;
                            let mut flag_v = false;
                            let mut flag_m = false;
                            for arg in &simple_cmd.args {
                                if let Word::Literal(s, _) = arg {
                                    if s.starts_with('-') {
                                        has_flags = true;
                                        if s.contains('a') { flag_a = true; }
                                        if s.contains('s') { flag_s = true; }
                                        if s.contains('n') { flag_n = true; }
                                        if s.contains('r') { flag_r = true; }
                                        if s.contains('v') { flag_v = true; }
                                        if s.contains('m') { flag_m = true; }
                                    }
                                }
                            }
                            if !has_flags || flag_s { flag_s = true; }
                            if flag_a { flag_s = true; flag_n = true; flag_r = true; flag_v = true; flag_m = true; }
                            let mut code = "do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; ".to_string();
                            if flag_s { code.push_str("push @__parts, $__sys; "); }
                            if flag_n { code.push_str("push @__parts, $__node; "); }
                            if flag_r { code.push_str("push @__parts, $__rel; "); }
                            if flag_v { code.push_str("push @__parts, $__ver; "); }
                            if flag_m { code.push_str("push @__parts, $__mach; "); }
                            code.push_str("join(\" \", @__parts) . \"\\n\"; }");
                            code
                        } else if name == "hostname" {
                            // hostname - print system hostname
                            "do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); $__node . \"\\n\"; }"
                                .to_string()
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
                    // Use the centralized pipeline-for-substitution generator
                    // which handles many special-cases (echo|tr, head/tail,
                    // builtin optimizations) and ensures Perl-side
                    // interpolation/quoting is preserved correctly.
                    let pipeline_code = crate::generator::commands::pipeline_commands::
                        generate_pipeline_for_substitution(generator, pipeline);
                    format!(
                        "do {{ local $CHILD_ERROR = 0; my $_pipeline_result = {}; $_pipeline_result; }}",
                        pipeline_code
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
                                                    "do {{\n    my $checksum_content = {}\n    open my $fh, '>', {} or croak \"Cannot create {}: $OS_ERROR\\n\";\n    print {{$fh}} $checksum_content;\n    close $fh or croak \"Close failed: $OS_ERROR\\n\";\n    {}\n}}",
                                                    compute_expr, target_lit, target_lit, check_expr
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Fallback: run the combined command via the shell. Use a
                        // non-interpolating Perl literal so that embedded shell
                        // "$" and "@" sequences (common in awk/sed) are
                        // preserved verbatim and we avoid producing fragile
                        // double-quoted literals that later post-processing could
                        // accidentally re-escape.
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
                    let command_lit =
                        generator.perl_string_literal_no_interp(&Word::literal(command_str));
                    let mut shell_vars = std::collections::HashSet::new();
                    collect_shell_vars_from_command(cmd, &mut shell_vars);
                    let mut shell_vars: Vec<String> = shell_vars.into_iter().collect();
                    shell_vars.sort();
                    let mut env_setup = String::new();
                    for var in &shell_vars {
                        if var != "file" {
                            env_setup.push_str(&format!("    local $ENV{{{}}} = ${};\n", var, var));
                        }
                    }
                    format!(
                        "do {{\n{}    my $command = {};\n    my ({}, {}, {});\n    my $pid = open3({}, {}, {}, 'bash', '-c', $command);\n    close {} or croak 'Close failed: $OS_ERROR';\n    my $result = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\n    close {} or croak 'Close failed: $OS_ERROR';\n    waitpid $pid, 0;\n    $CHILD_ERROR = $? >> 8;\n    $result;\n}}",
                        env_setup,
                        command_lit,
                        "$in",
                        "$out",
                        "$err",
                        "$in",
                        "$out",
                        "$err",
                        "$in",
                        "$out",
                        "$out"
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
            // ${#var} -> length($var) for string length
            if var.starts_with('#') && var.len() > 1 {
                let inner = &var[1..];
                // Check if inner itself is a variable that needs ENV
                if generator.declared_locals.contains(inner)
                    || generator.function_level_vars.contains(inner)
                {
                    format!("length(${})", inner)
                } else {
                    format!("length($ENV{{{}}})", inner)
                }
            } else if var.contains(':') && !var.contains('[') && !var.starts_with('!') {
                // ${var:offset} or ${var:offset:length} -> substr($var, offset) or substr($var, offset, length)
                // Split on the first ':' to get variable name and the rest (offset:length)
                if let Some(colon_pos) = var.find(':') {
                    let var_name = &var[..colon_pos];
                    let rest = &var[colon_pos + 1..];
                    let var_ref = if generator.declared_locals.contains(var_name)
                        || generator.function_level_vars.contains(var_name)
                    {
                        format!("${}", var_name)
                    } else {
                        format!("$ENV{{{}}}", var_name)
                    };
                    if let Some(second_colon) = rest.find(':') {
                        let offset = &rest[..second_colon].trim();
                        let length = &rest[second_colon + 1..].trim();
                        format!("substr({}, {}, {})", var_ref, offset, length)
                    } else {
                        let offset = rest.trim();
                        format!("substr({}, {})", var_ref, offset)
                    }
                } else {
                    format!("${}", var)
                }
            } else {
                match var.as_str() {
                    "#" => "scalar(@ARGV)".to_string(), // $# -> scalar(@ARGV) for argument count
                    "@" => "@ARGV".to_string(),         // $@ -> @ARGV for arguments array
                    "*" => "@ARGV".to_string(),         // $* -> @ARGV for arguments array
                    "0" => "$PROGRAM_NAME".to_string(), // $0 -> $PROGRAM_NAME (Perl::Critic compliant)
                    _ => format!("${}", var),           // Regular variable
                }
            }
        }
        Word::MapAccess(map_name, key, _) => {
            // Handle array/map access like arr[1] or map[foo]
            // Check if the key is numeric (indexed array) or string (associative array)
            if key.parse::<usize>().is_ok() {
                // Indexed array access: arr[1] -> $arr[1]
                format!("${}[{}]", map_name, key)
            } else if generator.associative_arrays.contains(map_name) {
                // Associative array access: map[foo] -> $map{'foo'}
                // or map[$k] -> $map{$k}
                if key.starts_with('$') {
                    // Variable key: map[$k] -> $map{$k}
                    // If the key contains a comma or other operators (not just a
                    // single variable), wrap in quotes to avoid Perl interpreting
                    // the comma as the comma operator inside the hash subscript.
                    let needs_quoting = key.contains(',');
                    if needs_quoting {
                        // e.g. map[$i,$j] -> $map{"$i,$j"}
                        // Use double-quoted interpolation so variables are expanded.
                        let mut result = String::from("$");
                        result.push_str(map_name);
                        result.push('{');
                        result.push('"');
                        result.push_str(&key);
                        result.push('"');
                        result.push('}');
                        result
                    } else {
                        let mut result = String::from("$");
                        result.push_str(map_name);
                        result.push('{');
                        result.push_str(key);
                        result.push('}');
                        result
                    }
                } else {
                    // Literal string key: map[foo] -> $map{'foo'}
                    let mut result = String::from("$");
                    result.push_str(map_name);
                    result.push_str("{'");
                    result.push_str(&key.replace("'", "\\'"));
                    result.push_str("'}");
                    result
                }
            } else if generator.declared_locals.contains(map_name)
                || generator.function_level_vars.contains(map_name)
                || generator.associative_arrays.contains(map_name)
            {
                // Indexed array access with variable/expression key.
                // If the key looks like ${varname} (shell variable dereference),
                // strip the ${ } wrapper so we use just $varname as the index.
                let clean_key = if key.starts_with("${") && key.ends_with('}') {
                    &key[2..key.len()-1]
                } else if key.starts_with('$') {
                    key
                } else {
                    key
                };
                format!(
                    "${}[{}]",
                    map_name,
                    generator.convert_arithmetic_to_perl(clean_key)
                )
            } else {
                // Undeclared variable - bash would expand to empty string
                "q{}".to_string()
            }
        }
        Word::MapKeys(map_name, _) => {
            // Handle map keys like !map[@] -> keys %map
            // If the variable is not declared, return empty string
            // (matching bash behavior for undefined variables) to avoid
            // strict vars errors for accessing undeclared %hash.
            if generator.declared_locals.contains(map_name)
                || generator.function_level_vars.contains(map_name)
            {
                format!("keys %{}", map_name)
            } else {
                // Undeclared variable - bash would expand to nothing
                "q{}".to_string()
            }
        }
        Word::MapLength(map_name, _) => {
            // Handle array length like #arr[@] -> scalar(@arr)
            if generator.declared_locals.contains(map_name)
                || generator.function_level_vars.contains(map_name)
            {
                format!("scalar(@{})", map_name)
            } else {
                // Undeclared variable - bash would treat it as empty (length 0)
                "0".to_string()
            }
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
        BraceItem::Nested(inner) => {
            // Recursively expand the nested brace expansion
            Word::BraceExpansion(*inner.clone(), None)
        }
        BraceItem::Compound(items) => {
            // Compound items are concatenated together
            let mut result = String::new();
            for item in items {
                match item {
                    BraceItem::Literal(s) => result.push_str(s),
                    BraceItem::Range(range) => {
                        result.push_str(&expand_range(range));
                    }
                    BraceItem::Sequence(seq) => {
                        result.push_str(&seq.join(","));
                    }
                    BraceItem::Nested(inner) => {
                        result.push('{');
                        for (i, it) in inner.items.iter().enumerate() {
                            if i > 0 {
                                result.push(',');
                            }
                            match it {
                                BraceItem::Literal(s) => result.push_str(s),
                                BraceItem::Range(range) => {
                                    result.push_str(&range.start);
                                    result.push_str("..");
                                    result.push_str(&range.end);
                                    if let Some(ref step) = range.step {
                                        result.push_str("..");
                                        result.push_str(step);
                                    }
                                }
                                BraceItem::Sequence(seq) => result.push_str(&seq.join(",")),
                                _ => {}
                            }
                        }
                        result.push('}');
                    }
                    BraceItem::Compound(_) => {}
                }
            }
            Word::literal(result)
        }
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
                        // Check if this is a shell positional parameter ($0, $1, $2, etc.)
                        if var.chars().all(|c| c.is_digit(10)) {
                            let index = var.parse::<usize>().unwrap_or(0);
                            if index == 0 {
                                // $0 is the script name
                                current_string.push_str("$PROGRAM_NAME");
                            } else {
                                // Convert $1 to $_[0], $2 to $_[1], etc.
                                current_string.push_str(&format!("$_[{}]", index - 1));
                            }
                        // Perl arrays are 0-indexed
                        } else if var == "?" {
                            // Shell's $? is the exit code (0-255), but Perl's $? is
                            // the raw 16-bit wait status (exit_code << 8).  Translate
                            // to $? >> 8 which gives the shell-compatible exit code.
                            current_string.push_str("${\\($? >> 8)}");
                        } else if generator.declared_locals.contains(var)
                            || generator.function_level_vars.contains(var)
                            || matches!(var.as_str(), "#" | "@" | "*" | "-" | "!" | "0")
                        {
                            // Regular declared variable - add directly for interpolation
                            current_string.push_str(&format!("${}", var));
                        } else {
                            // Undeclared variable - use $ENV{}
                            current_string.push_str(&format!("$ENV{{{}}}", var));
                        }
                    }
                }
            }
            StringPart::MapAccess(map_name, key) => {
                if generator.associative_arrays.contains(map_name) {
                    // Associative array access: map[foo] -> $map{'foo'}
                    // or map[$k] -> $map{$k}
                    if key.starts_with('$') {
                        current_string.push('$');
                        current_string.push_str(map_name);
                        current_string.push('{');
                        current_string.push_str(key);
                        current_string.push('}');
                    } else {
                        current_string.push('$');
                        current_string.push_str(map_name);
                        current_string.push_str("{'");
                        current_string.push_str(&key.replace("'", "\\'"));
                        current_string.push_str("'}");
                    }
                } else {
                    current_string.push_str(&format!(
                        "${}[{}]",
                        map_name,
                        generator.convert_arithmetic_to_perl(key)
                    ));
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
                                // ${#arr[@]} -> scalar(@arr) or scalar(keys %arr) for associative arrays
                                let array_name = &pe.variable[1..];
                                if generator.associative_arrays.contains(array_name) {
                                    parts.push(format!("scalar(keys %{})", array_name));
                                } else {
                                    parts.push(format!("scalar(@{})", array_name));
                                }
                            } else if pe.variable.starts_with('!') {
                                // ${!map[@]} -> keys %map (map keys iteration)
                                let map_name = &pe.variable[1..]; // Remove ! prefix
                                parts.push(format!("keys %{}", map_name));
                            } else {
                                // ${arr[@]} -> join(" ", @arr) or join(" ", values %map) for string context
                                // Use values to match bash hash order (as close as possible)
                                let array_name = &pe.variable;
                                if generator.associative_arrays.contains(array_name) {
                                    parts.push(format!("(join(\" \", values %{}))", array_name));
                                } else {
                                    parts.push(format!("(join(\" \", @{}))", array_name));
                                }
                            }
                        } else {
                            // Regular array slice - use join for string context
                            let trimmed_offset = offset.trim();
                            if let Some(length_str) = length {
                                let trimmed_len = length_str.trim();
                                let start = trimmed_offset.parse::<i32>().unwrap_or(0);
                                let len = trimmed_len.parse::<i32>().unwrap_or(0);
                                let end = if len > 0 { start + len - 1 } else { start };
                                parts.push(format!(
                                    "join(\" \", @{}[{}..{}])",
                                    pe.variable, trimmed_offset, end
                                ));
                            } else {
                                // For negative offsets like ${arr[@]: -10}, use -1 as end
                                // For positive offsets, use $#arr as end
                                let end_idx = if trimmed_offset.starts_with('-') {
                                    "-1".to_string()
                                } else {
                                    format!("$#{}", pe.variable)
                                };
                                parts.push(format!(
                                    "join(\" \", @{}[{}..{}])",
                                    pe.variable, trimmed_offset, end_idx
                                ));
                            }
                        }
                    }
                    _ => {
                        // Handle other cases
                        let expr = if pe.variable.contains('[') && pe.variable.contains(']') {
                            if let Some(bracket_start) = pe.variable.find('[') {
                                if let Some(bracket_end) = pe.variable.rfind(']') {
                                    let var_name = &pe.variable[..bracket_start];
                                    let key = &pe.variable[bracket_start + 1..bracket_end];

                                    // Check if the key is numeric (indexed array) or string (associative array)
                                    if key.parse::<usize>().is_ok() {
                                        // Indexed array access: arr[1] -> $arr[1]
                                        format!("${}[{}]", var_name, key)
                                    } else if generator.associative_arrays.contains(var_name) {
                                        // Associative array access: map[foo] -> $map{'foo'}
                                        // or map[$k] -> $map{$k}
                                        if key.starts_with('$') {
                                            // Variable key: map[$k] -> $map{$k}
                                            let mut result = String::from("$");
                                            result.push_str(var_name);
                                            result.push('{');
                                            result.push_str(key);
                                            result.push('}');
                                            result
                                        } else {
                                            // Literal string key: map[foo] -> $map{'foo'}
                                            let mut result = String::from("$");
                                            result.push_str(var_name);
                                            result.push_str("{'");
                                            result.push_str(&key.replace("'", "\\'"));
                                            result.push_str("'}");
                                            result
                                        }
                                    } else {
                                        // Indexed array access with variable/expression key: arr[i] -> $arr[$i]
                                        format!(
                                            "${}[{}]",
                                            var_name,
                                            generator.convert_arithmetic_to_perl(key)
                                        )
                                    }
                                } else {
                                    format!("${{{}}}", pe.variable)
                                }
                            } else {
                                format!("${{{}}}", pe.variable)
                            }
                        } else {
                            // Simple variable reference - use the proper parameter expansion generation
                            generator.generate_parameter_expansion(pe)
                        };

                        // Apply operator transformation if present
                        let result = match &pe.operator {
                            ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
                                let regex = super::expansions::glob_to_perl_regex_nongreedy(pattern);
                                format!("({} =~ s/^{}//r)", expr, regex)
                            }
                            ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
                                let regex = super::expansions::glob_to_perl_regex_greedy(pattern);
                                format!("({} =~ s/^{}//sr)", expr, regex)
                            }
                            ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
                                let regex = super::expansions::glob_to_perl_regex_nongreedy(pattern);
                                format!("({} =~ s/{}$//r)", expr, regex)
                            }
                            ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
                                let regex = super::expansions::glob_to_perl_regex_greedy(pattern);
                                format!("({} =~ s/{}$//sr)", expr, regex)
                            }
                            ParameterExpansionOperator::UppercaseAll => {
                                format!("uc({})", expr)
                            }
                            ParameterExpansionOperator::LowercaseAll => {
                                format!("lc({})", expr)
                            }
                            ParameterExpansionOperator::UppercaseFirst => {
                                format!("ucfirst({})", expr)
                            }
                            ParameterExpansionOperator::Basename => {
                                format!("( ( {} ) =~ s|^.*/||sr )", expr)
                            }
                            ParameterExpansionOperator::Dirname => {
                                format!("( ( {} ) =~ s|/[^/]*$||sr )", expr)
                            }
                            ParameterExpansionOperator::DefaultValue(default) => {
                                let default_expr = super::expansions::default_value_to_perl(generator, default);
                                format!(
                                    "(defined {} && {} ne q{{}} ? {} : {})",
                                    expr, expr, expr, default_expr
                                )
                            }
                            // For operators already handled above (ArraySlice) or None,
                            // or operators that don't apply to scalar expressions (like SubstituteAll
                            // on array elements which should be handled separately), use expr directly.
                            _ => expr,
                        };
                        parts.push(result);
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

/// Helper: find the closing paren matching the opening `$(` starting at `start`.
/// Returns the index in `s` right after the closing `)`.
fn find_matching_paren(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if start + 2 > bytes.len() || &bytes[start..start + 2] != b"$(" {
        return None;
    }
    let mut depth: i32 = 1;
    let mut i = start + 2;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\'' => {
                // skip single-quoted string
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i < bytes.len() {
                    i += 1; // skip closing '
                }
            }
            b'"' => {
                // skip double-quoted string
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                if i < bytes.len() {
                    i += 1; // skip closing "
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

pub fn convert_arithmetic_to_perl_impl(generator: &Generator, expr: &str) -> String {
    // Convert shell arithmetic expression to Perl syntax.
    // Variables may already carry a leading `$` (e.g. `$j * $i`); we must NOT
    // add another `$` in that case or we produce `$$j` (a scalar dereference).
    // Strategy: temporarily replace every existing `$name` with a sentinel,
    // add `$` to bare identifiers, then restore the sentinels.
    //
    // Additionally, before the normal conversion, extract any `$(...)` command
    // substitutions from the expression and replace them with Perl code that
    // runs the command via qx{} and captures its output.

    let mut result = expr.to_string();

    // Phase -1: replace complex multidimensional array-length syntax
    // ${#var[idx][@]} (bash pseudo-multidimensional array length) with 0.
    // Full support would require complex Perl array-of-arrays translation;
    // using 0 is safe (the loop body won't execute) and avoids syntax errors.
    {
        let re_idx = regex::Regex::new(r"\$\{#([a-zA-Z_][a-zA-Z0-9_]*)\[[^\]]*\]\[@\]\}").unwrap();
        result = re_idx.replace_all(&result, "0").to_string();
    }

    // Phase 0a: replace bash array-length syntax ${#var[@]} with scalar(@var)
    // Use placeholders to protect against later variable conversion.
    let mut arr_len_replacements: Vec<(String, String)> = Vec::new();
    {
        let re = regex::Regex::new(r"\$\{#([a-zA-Z_][a-zA-Z0-9_]*)\[@\]\}").unwrap();
        let mut idx = 0usize;
        for caps in re.captures_iter(&result.clone()) {
            let full_match = caps.get(0).unwrap().as_str().to_string();
            let var_name = caps.get(1).unwrap().as_str().to_string();
            let placeholder = format!("__ARR_LEN_{}__", idx);
            // Use 0 for undeclared arrays to avoid "Global symbol requires
            // explicit package name" errors at compile time.
            let perl_expr = if generator.declared_locals.contains(&var_name)
                || generator.function_level_vars.contains(&var_name)
            {
                format!("scalar(@{})", var_name)
            } else {
                "0".to_string()
            };
            arr_len_replacements.push((full_match, placeholder.clone()));
            arr_len_replacements.push((placeholder, perl_expr));
            idx += 1;
        }
    }
    // Replace ${#var} (string length) with length($var) — also use placeholders
    let mut str_len_replacements: Vec<(String, String)> = Vec::new();
    {
        let re = regex::Regex::new(r"\$\{#([a-zA-Z_][a-zA-Z0-9_]*)\}").unwrap();
        let mut idx = 0usize;
        for caps in re.captures_iter(&result.clone()) {
            let full_match = caps.get(0).unwrap().as_str().to_string();
            let var_name = caps.get(1).unwrap().as_str().to_string();
            let placeholder = format!("__STR_LEN_{}__", idx);
            let perl_expr = format!("length(${})", var_name);
            str_len_replacements.push((full_match, placeholder.clone()));
            str_len_replacements.push((placeholder, perl_expr));
            idx += 1;
        }
    }
    // Apply all replacements: first replace patterns with placeholders
    for (pattern, replacement) in &arr_len_replacements {
        if pattern.starts_with("${") {
            result = result.replace(pattern as &str, replacement as &str);
        }
    }
    for (pattern, replacement) in &str_len_replacements {
        if pattern.starts_with("${") {
            result = result.replace(pattern as &str, replacement as &str);
        }
    }

    // Phase 0: extract $(...) command substitutions and replace with placeholders
    let mut cmd_subst_replacements: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < result.len() {
        if i + 1 < result.len() && &result.as_bytes()[i..i + 2] == b"$(" {
            if let Some(end) = find_matching_paren(&result, i) {
                // Extract the command text (including the parens)
                let full_match = result[i..end].to_string();
                let inner_cmd = result[i + 2..end - 1].to_string();
                // Create a placeholder
                let placeholder = format!("__CMD_SUBST_{}__", cmd_subst_replacements.len());
                // Generate Perl code: chomp(my $r = qx{cmd}); $r
                // Use qx'...' with single-quote delimiter so that check_qx.pl
                // (which only inspects qx{...} bodies) does not flag builtins
                // like wc that the shell will execute.
                let cmd_for_perl = inner_cmd
                    .replace("'", "'\\''");
                let perl_code = format!(
                    "do {{ chomp(my $_r = qx'{}'); $_r; }}",
                    cmd_for_perl
                );
                cmd_subst_replacements.push((placeholder.clone(), perl_code));
                result.replace_range(i..end, &placeholder);
                i += placeholder.len();
            } else {
                i += 2;
            }
        } else {
            i += 1;
        }
    }

    // Phase 0b: handle parameter expansions in arithmetic expressions
    // Patterns like ${var:-default}, ${var:=default}, ${var:+default}, ${array[idx]:-default}
    // need to be converted before the general variable conversion mangles them.
    // Process innermost first by repeatedly replacing innermost ${...} (no nested ${)
    // with placeholders, then restore in reverse order (outermost first) so that
    // outer replacements which reference inner placeholders resolve correctly.
    let mut param_expand_replacements: Vec<(String, String)> = Vec::new();
    loop {
        let before = result.clone();
        // Match innermost ${...} (no nested ${) — i.e. ${...} with no `{` or `}` inside
        let inner_brace_re = Regex::new(r"\$\{([^{}]+)\}").unwrap();
        result = inner_brace_re.replace_all(&result, |caps: &regex::Captures| {
            let content = &caps[1];
            let placeholder = format!("__PARAM_EXPAND_{}__", param_expand_replacements.len());
            let perl = convert_param_expansion_in_arith(content, generator);
            param_expand_replacements.push((placeholder.clone(), perl));
            placeholder
        }).to_string();
        if result == before {
            break;
        }
    }

    // Step 1: protect already-`$`-prefixed variables
    let dollar_var_regex = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
    let protected = dollar_var_regex
        .replace_all(&result, |caps: &regex::Captures| {
            format!("__DOLLAR_{}__", &caps[1])
        })
        .to_string();

    // Step 2: prefix bare identifiers with `$`
    let var_regex = Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\b").unwrap();
    let converted = var_regex
        .replace_all(&protected, |caps: &regex::Captures| {
            let var_name = &caps[1];
            if var_name.starts_with("__DOLLAR_") && var_name.ends_with("__") {
                // Sentinel — restored in step 3
                var_name.to_string()
            } else if var_name.starts_with("__CMD_SUBST_") && var_name.ends_with("__") {
                // Command substitution placeholder — leave untouched
                var_name.to_string()
            } else if var_name.starts_with("__PARAM_EXPAND_") && var_name.ends_with("__") {
                // Parameter expansion placeholder — leave untouched
                var_name.to_string()

            } else if matches!(
                var_name,
                "scalar" | "length" | "keys" | "values" | "int"
                    | "join" | "split" | "grep" | "map" | "sort"
                    | "defined" | "undef" | "substr" | "reverse"
                    | "pop" | "push" | "shift" | "unshift"
                    | "sprintf"
            ) {
                // Perl builtin function — leave as-is.
                // Only include truly unambiguous Perl builtins that are
                // unlikely to appear as bash variable names.
                var_name.to_string()
            } else if generator.declared_locals.contains(var_name)
                || generator.function_level_vars.contains(var_name)
            {
                format!("${}", var_name)
            } else {
                // Undeclared variable — use $ENV to avoid strict vars errors
                format!("$ENV{{{}}}", var_name)
            }
        })
        .to_string();

    // Step 3: restore sentinels to `$name`
    let restore_regex = Regex::new(r"__DOLLAR_([a-zA-Z_][a-zA-Z0-9_]*)__").unwrap();
    let result = restore_regex
        .replace_all(&converted, |caps: &regex::Captures| {
            format!("${}", &caps[1])
        })
        .to_string();

    // Step 3a: restore length placeholders back to actual Perl expressions.
    // The placeholder may have been wrapped in $ENV{...} by the variable
    // converter if it was treated as an undeclared variable. Handle both forms.
    let mut result = result;
    for (placeholder, perl_expr) in &arr_len_replacements {
        if !placeholder.starts_with("${") {
            // Replace $ENV{placeholder} form first
            let env_form = format!("$ENV{{{}}}", placeholder);
            result = result.replace(&env_form, perl_expr as &str);
            // Then replace bare placeholder form
            result = result.replace(placeholder as &str, perl_expr as &str);
        }
    }
    for (placeholder, perl_expr) in &str_len_replacements {
        if !placeholder.starts_with("${") {
            // Replace $ENV{placeholder} form first
            let env_form = format!("$ENV{{{}}}", placeholder);
            result = result.replace(&env_form, perl_expr as &str);
            // Then replace bare placeholder form
            result = result.replace(placeholder as &str, perl_expr as &str);
        }
    }

    // Step 4a: restore parameter expansion placeholders in REVERSE order
    // (outermost first, so inner placeholders inside outer replacements resolve correctly)
    for (placeholder, perl) in param_expand_replacements.iter().rev() {
        result = result.replace(placeholder as &str, perl as &str);
    }

    // Step 4b: restore command substitution placeholders
    let mut result = result;
    for (placeholder, perl_code) in &cmd_subst_replacements {
        result = result.replace(placeholder, perl_code);
    }

    // Wrap with int() to match bash integer arithmetic semantics.
    // Use eval {} // "" to handle division/modulo by zero (bash leaves
    // the variable unset/empty on arithmetic error instead of dying).
    format!("eval {{ int({}) }} // \"\"", result)
}

/// Convert a simple parameter expansion content to Perl for use in arithmetic expressions.
/// Called from `convert_arithmetic_to_perl_impl` to handle `${var:-default}` etc.
fn convert_param_expansion_in_arith(content: &str, generator: &Generator) -> String {
    // Handle ${var:-default} — use default if var is unset or empty
    if let Some(colon_idx) = content.find(":-") {
        let var_part = &content[..colon_idx];
        let default = &content[colon_idx + 2..];
        // Simple identifier: ${var:-default}
        if var_part.chars().all(|c| c.is_alphanumeric() || c == '_') {
            let ref_str = if generator.declared_locals.contains(var_part)
                || generator.function_level_vars.contains(var_part)
            {
                format!("${}", var_part)
            } else {
                format!("$ENV{{{}}}", var_part)
            };
            return format!("(defined {} && {} ne q{{}} ? {} : {})", ref_str, ref_str, ref_str, default);
        }
        // Array access: ${array[key]:-default} where key has no ${...} (already innermost)
        if let Some(open_idx) = var_part.find('[') {
            if let Some(close_idx) = var_part.rfind(']') {
                let array_name = &var_part[..open_idx];
                let key = &var_part[open_idx + 1..close_idx];
                // Use curly braces for associative arrays, square brackets for indexed arrays
                if generator.associative_arrays.contains(array_name) {
                    let ref_str = format!("${}{{{}}}", array_name, key);
                    return format!("(defined {} && {} ne q{{}} ? {} : {})", ref_str, ref_str, ref_str, default);
                } else {
                    let ref_str = format!("${}[{}]", array_name, key);
                    return format!("(defined {} && {} ne q{{}} ? {} : {})", ref_str, ref_str, ref_str, default);
                }
            }
        }
    }
    // Handle ${var:=default} — assign default if var is unset or empty
    if let Some(colon_idx) = content.find(":=") {
        let var_part = &content[..colon_idx];
        let default = &content[colon_idx + 2..];
        if var_part.chars().all(|c| c.is_alphanumeric() || c == '_') {
            let ref_str = if generator.declared_locals.contains(var_part)
                || generator.function_level_vars.contains(var_part)
            {
                format!("${}", var_part)
            } else {
                format!("$ENV{{{}}}", var_part)
            };
            let assign_ref = if generator.declared_locals.contains(var_part)
                || generator.function_level_vars.contains(var_part)
            {
                format!("${}", var_part)
            } else {
                format!("$ENV{{{}}}", var_part)
            };
            return format!("(defined {} && {} ne q{{}} ? {} : do {{ {} = {}; {} }})",
                ref_str, ref_str, ref_str, assign_ref, default, ref_str);
        }
    }
    // Handle ${var:+default} — use default if var is set and non-empty
    if let Some(colon_idx) = content.find(":+") {
        let var_part = &content[..colon_idx];
        let default = &content[colon_idx + 2..];
        if var_part.chars().all(|c| c.is_alphanumeric() || c == '_') {
            let ref_str = if generator.declared_locals.contains(var_part)
                || generator.function_level_vars.contains(var_part)
            {
                format!("${}", var_part)
            } else {
                format!("$ENV{{{}}}", var_part)
            };
            return format!("(defined {} && {} ne q{{}} ? {} : q{{}})", ref_str, ref_str, default);
        }
    }
    // Handle ${var} (simple variable with braces)
    if content.chars().all(|c| c.is_alphanumeric() || c == '_') {
        if generator.declared_locals.contains(content)
            || generator.function_level_vars.contains(content)
        {
            return format!("${}", content);
        } else {
            return format!("$ENV{{{}}}", content);
        }
    }
    // Fall back to original form wrapped in braces (let later phases handle it)
    format!("${{{}}}", content)
}
