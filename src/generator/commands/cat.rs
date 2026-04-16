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
        parts.push(format!(
            "do {{ open my $fh, '<', {} or die 'cat: ' . {} . ': ' . $OS_ERROR . \"\\n\"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . \"\\n\"; $chunk; }}",
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
    for redir in redirects {
        if matches!(
            redir.operator,
            RedirectOperator::Heredoc | RedirectOperator::HeredocTabs
        ) {
            has_heredoc = true;
            if let Some(body) = &redir.heredoc_body {
                // Print the heredoc content directly
                output.push_str(&format!(
                    "print {};\n",
                    generator.perl_string_literal_no_interp(&Word::literal(body.clone()))
                ));
            }
        }
    }

    // If no heredocs, handle file reading as before
    if !has_heredoc {
        let substitution = generate_cat_command_for_substitution(generator, cmd);
        if target_var.is_empty() {
            output.push_str(&format!("print {};\n", substitution));
        } else {
            output.push_str(&format!("${} = {};\n", target_var, substitution));
        }
    }

    output
}
