use crate::ast::*;
use crate::generator::Generator;
use crate::ir::{stmt_to_perl, AssignTarget, IrExpr, IrStmt, Sigil, StrStyle};

fn cat_requires_shell(cmd: &SimpleCommand) -> bool {
    // If any argument looks like a shell operator (pipe),
    // we must run via the shell instead of treating args as filenames.
    // Variables and string interpolations are handled natively by open().
    // Exception: '-' as argument means stdin, handled natively.
    cmd.args.iter().any(|arg| match arg {
        Word::Literal(text, _) => {
            if text == "|" {
                true
            } else if text == "-" {
                false // cat - is just stdin, can be handled natively
            } else {
                text.starts_with('-')
            }
        }
        Word::StringInterpolation(interp, _) => {
            if interp.parts.len() == 1 {
                if let StringPart::Literal(text) = &interp.parts[0] {
                    if text == "|" {
                        true
                    } else if text == "-" {
                        false // cat - is stdin, handled natively
                    } else {
                        text.starts_with('-')
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
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
        // Use system() with proper argument list instead of qx{}
        // for commands that require shell features.
        let cmd_str = generator.generate_command_string_for_system(&Command::Simple(cmd.clone()));
        // Split into command and args and use system() with list form
        let mut words: Vec<&str> = cmd_str.split_whitespace().collect();
        if words.is_empty() {
            words.push("cat");
        }
        let args_perl: Vec<String> = words
            .iter()
            .map(|w| format!("'{}'", w.replace("'", "'\\''")))
            .collect();
        return format!(
            "do {{ my $cat_out = q{{}}; my $cat_pid = open3(my $cat_in, my $cat_out_r, undef, {}); close $cat_in or croak 'Close failed: $OS_ERROR'; local $INPUT_RECORD_SEPARATOR = undef; $cat_out = <$cat_out_r>; close $cat_out_r or croak 'Close failed: $OS_ERROR'; waitpid $cat_pid, 0; $cat_out; }}",
            args_perl.join(", ")
        );
    }

    let mut parts = Vec::new();
    for arg in &cmd.args {
        let path = generator.perl_string_literal(arg);
        // Check if this argument is '-' which means stdin
        let is_stdin = matches!(arg, Word::Literal(text, _) if text == "-");
        if is_stdin {
            // Read from STDIN directly
            parts.push(
                "do { local $INPUT_RECORD_SEPARATOR = undef; my $cat_chunk = <STDIN>; $cat_chunk; }".to_string()
            );
        } else {
            // Use a conditional open so a missing file produces empty output
            // (and a warning to stderr) rather than die-ing, matching bash
            // command-substitution behaviour: `$(cat missing_file)` → q{}.
            parts.push(format!(
                "do {{ my $cat_chunk = q{{}}; if ( open my $fh, '<', {} ) {{ local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; }} else {{ carp 'cat: ' . {} . ': ' . $OS_ERROR . \"\\n\"; }} $cat_chunk; }}",
                path, path
            ));
        }
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
        if matches!(
            redir.operator,
            RedirectOperator::Output | RedirectOperator::Append
        ) {
            // Convert the target to a real Perl EXPRESSION: literal filenames
            // become quoted Perl strings, interpolations (`> "$f"`) become
            // their variable references.  Re-wrapping later in
            // perl_string_literal would quote `$f`/`${f}` literally, creating
            // a file literally named `"${f}"` instead of x.py.
            output_file = Some(generator.word_to_perl(&redir.target));
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

        // Choose string style based on heredoc quoting:
        // - Unquoted heredocs (<<EOF): use Heredoc style which preserves
        //   $ and @ for Perl interpolation, but escapes \n etc.
        // - Quoted heredocs (<<'EOF'): use SingleQuoted which prevents
        //   Perl interpolation, matching shell semantics.
        let style = if any_unquoted && !any_quoted {
            // For unquoted heredocs, preprocess shell variable references
            // (${var} and $var) into proper Perl references.
            body = crate::generator::words::preprocess_shell_vars_in_raw_string(generator, &body);
            StrStyle::Heredoc
        } else {
            StrStyle::SingleQuoted
        };

        if let Some(filename) = output_file {
            // Write heredoc content to the output file (raw text for now).
            // `filename` is already a Perl expression (quoted literal or var ref).
            output.push_str(&format!(
                "open my $fh_cat, '>', {} or croak \"Cannot access file: $OS_ERROR\\n\";\n",
                filename
            ));
            let body_lit = crate::ir::ir_expr_to_perl(&IrExpr::Str(body, style));
            output.push_str(&format!("print {{$fh_cat}} {};\n", body_lit));
            output.push_str(&format!(
                "close $fh_cat or croak \"Close failed: $OS_ERROR\\n\";\n"
            ));
        } else {
            // No output redirect.
            // In a pipeline or command-substitution context, assign the heredoc
            // content to the target variable instead of printing directly.
            if target_var.is_empty() {
                // Standalone command: emit as IR Output node for clean
                // print with escaped content (newline: false because the
                // heredoc body already ends with \n).
                let ir_stmt = IrStmt::Output {
                    value: IrExpr::Str(body, style),
                    newline: false,
                    target: None,
                };
                output.push_str(&stmt_to_perl(&ir_stmt, 0));
            } else {
                // Pipeline/command-substitution context: assign to variable.
                // Use IR Assign node so the backend formats it consistently.
                let ir_stmt = IrStmt::Assign {
                    targets: vec![AssignTarget {
                        var: target_var.to_string(),
                        sigil: Some(Sigil::Scalar),
                        indices: vec![],
                    }],
                    expr: IrExpr::Str(body, style),
                };
                output.push_str(&stmt_to_perl(&ir_stmt, 0));
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
