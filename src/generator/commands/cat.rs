use crate::ast::*;
use crate::generator::Generator;

fn cat_requires_shell(cmd: &SimpleCommand) -> bool {
    // If any argument looks like a shell operator (pipe) or an option (-...),
    // we must run via the shell instead of treating args as filenames.
    cmd.args.iter().any(|arg| match arg {
        Word::Literal(text, _) => {
            // If the literal is exactly a pipe token or starts with an option dash,
            // require shell execution.
            if text == "|" {
                true
            } else {
                text.starts_with('-')
            }
        }
        Word::StringInterpolation(interp, _) => {
            // If it's a simple literal interpolation, inspect the literal value.
            if interp.parts.len() == 1 {
                if let StringPart::Literal(text) = &interp.parts[0] {
                    if text == "|" {
                        true
                    } else {
                        text.starts_with('-')
                    }
                } else {
                    true
                }
            } else {
                true
            }
        }
        _ => true,
    })
}

pub fn generate_cat_command_for_substitution(
    generator: &mut Generator,
    cmd: &SimpleCommand,
) -> String {
    if cmd.args.is_empty() {
        return "do { local $INPUT_RECORD_SEPARATOR = undef; <STDIN> };".to_string();
    }

    if cat_requires_shell(cmd) {
        let cmd_str = generator.generate_command_string_for_system(&Command::Simple(cmd.clone()));
        let cmd_lit = generator.perl_string_literal_no_interp(&Word::literal(cmd_str));
        return format!("do {{ my $cat_cmd = {}; qx{{$cat_cmd}}; }}", cmd_lit);
    }

    let mut parts = Vec::new();
    for arg in &cmd.args {
        let path = generator.perl_string_literal(arg);
        // Use a conditional open so a missing file produces empty output
        // (and a warning to stderr) rather than die-ing, matching bash
        // command-substitution behaviour: `$(cat missing_file)` → q{}.
        parts.push(format!(
            "do {{ my $cat_chunk = q{{}}; if ( open my $fh, '<', {} ) {{ local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; }} else {{ carp 'cat: ' . {} . ': ' . $OS_ERROR . \"\\n\"; }} $cat_chunk; }}",
            path, path
        ));
    }

    if parts.len() == 1 {
        parts.pop().unwrap()
    } else {
        format!("({})", parts.join(" . "))
    }
}

pub fn generate_cat_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    redirects: &[Redirect],
    input_var: &str,
) -> String {
    let mut output = String::new();
    let target_var = input_var.trim_start_matches('$');

    // Check if this cat command has heredoc redirects
    let mut has_heredoc = false;
    let mut output_file: Option<String> = None;
    for redir in redirects {
        if matches!(
            redir.operator,
            RedirectOperator::Heredoc | RedirectOperator::HeredocTabs
        ) {
            has_heredoc = true;
        }
        // Check for output redirect: > file or >> file
        if matches!(redir.operator, RedirectOperator::Output | RedirectOperator::Append) {
            if let Word::Literal(filename, _) = &redir.target {
                output_file = Some(filename.clone());
            } else {
                output_file = Some(generator.perl_string_literal(&redir.target));
            }
        }
    }

    if has_heredoc {
        // Collect heredoc content
        let mut body = String::new();
        for redir in redirects {
            if matches!(
                redir.operator,
                RedirectOperator::Heredoc | RedirectOperator::HeredocTabs
            ) {
                if let Some(content) = &redir.heredoc_body {
                    body.push_str(content);
                }
            }
        }

        if let Some(filename) = output_file {
            // Write heredoc content to the output file
            let filename_pl = generator.perl_string_literal(&Word::literal(filename));
            output.push_str(&format!(
                "open my $fh_cat, '>', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                filename_pl
            ));
            output.push_str(&format!(
                "print {{$fh_cat}} {};\n",
                generator.perl_string_literal_no_interp(&Word::literal(body))
            ));
            output.push_str(&format!(
                "close $fh_cat or croak \"Close failed: $OS_ERROR\\n\";\n"
            ));
        } else {
            // No output redirect.
            // In a pipeline or command-substitution context, assign the heredoc
            // content to the target variable instead of printing directly.
            let body_lit = generator.perl_string_literal_no_interp(&Word::literal(body));
            if target_var.is_empty() {
                // Standalone command: print to stdout.
                output.push_str(&format!("print {};\n", body_lit));
            } else {
                // Pipeline/command-substitution context: set the variable.
                output.push_str(&format!("${} = {};\n", target_var, body_lit));
            }
        }
    } else {
        // If no heredocs, handle file reading as before
        let substitution = generate_cat_command_for_substitution(generator, cmd);
        if target_var.is_empty() {
            output.push_str(&format!("print {};\n", substitution));
        } else {
            output.push_str(&format!("${} = {};\n", target_var, substitution));
        }
    }

    output
}
