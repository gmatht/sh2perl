use crate::ast::*;
use crate::generator::Generator;

/// Convert a heredoc body to a Perl interpolating string literal (`"..."`).
/// Unlike `perl_string_literal_no_interp`, this preserves `$` and `@` so that
/// Perl will interpolate variable references (matching bash behaviour for
/// unquoted heredocs like `<< EOF`).  Backslashes, double-quotes and control
/// characters are still escaped so the Perl source is valid.
fn heredoc_body_to_perl_interp(body: &str) -> String {
    let escaped = body
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\t", "\\t")
        .replace("\r", "\\r");
    format!("\"{}\"", escaped)
}

fn cat_requires_shell(cmd: &SimpleCommand) -> bool {
    // If any argument looks like a shell operator (pipe) or an option (-...),
    // we must run via the shell instead of treating args as filenames.
    // Variables and string interpolations are handled natively by open().
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
                    false // Non-literal interpolation (e.g. variables) - handle natively
                }
            } else {
                false // Multi-part interpolation - handle natively
            }
        }
        _ => false, // Variables and other word types - handle natively via open()
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
        // Collect heredoc content and determine if any delimiter was quoted
        let mut body = String::new();
        let mut any_quoted = false;
        let mut any_unquoted = false;
        for redir in redirects {
            if matches!(
                redir.operator,
                RedirectOperator::Heredoc | RedirectOperator::HeredocTabs
            ) {
                if let Some(content) = &redir.heredoc_body {
                    body.push_str(content);
                    if redir.heredoc_quoted {
                        any_quoted = true;
                    } else {
                        any_unquoted = true;
                    }
                }
            }
        }

        // Use interpolating strings for unquoted heredocs, non-interpolating for quoted
        // For mixed heredocs (shouldn't normally happen), use a reasonable default
        let body_lit = if any_unquoted && !any_quoted {
            // Only unquoted heredocs: use interpolating Perl string
            heredoc_body_to_perl_interp(&body)
        } else {
            // Quoted (or mixed) heredocs: use non-interpolating Perl string
            generator.perl_string_literal_no_interp(&Word::literal(body))
        };

        if let Some(filename) = output_file {
            // Write heredoc content to the output file
            let filename_pl = generator.perl_string_literal(&Word::literal(filename));
            output.push_str(&format!(
                "open my $fh_cat, '>', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                filename_pl
            ));
            output.push_str(&format!(
                "print {{$fh_cat}} {};\n",
                body_lit
            ));
            output.push_str(&format!(
                "close $fh_cat or croak \"Close failed: $OS_ERROR\\n\";\n"
            ));
        } else {
            // No output redirect.
            // In a pipeline or command-substitution context, assign the heredoc
            // content to the target variable instead of printing directly.
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
