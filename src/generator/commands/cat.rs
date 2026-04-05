use crate::ast::*;
use crate::generator::Generator;

pub fn generate_cat_command_for_substitution(
    generator: &mut Generator,
    cmd: &SimpleCommand,
) -> String {
    let command = Command::Simple(cmd.clone());
    let command_str = generator.generate_command_string_for_system(&command);
    let command_lit = generator.perl_string_literal(&Word::literal(command_str));

    format!(
        "do {{ my $cat_cmd = {}; my $result = qx{{$cat_cmd}}; $result; }};",
        command_lit
    )
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
                    generator.perl_string_literal(&Word::literal(body.clone()))
                ));
            }
        }
    }

    // If no heredocs, handle file reading as before
    if !has_heredoc {
        let command = Command::Simple(cmd.clone());
        let command_str = generator.generate_command_string_for_system(&command);
        let command_lit = generator.perl_string_literal(&Word::literal(command_str));

        if target_var.is_empty() {
            output.push_str(&format!(
                "do {{ my $cat_cmd = {}; print qx{{$cat_cmd}}; }};\n",
                command_lit
            ));
        } else {
            output.push_str(&format!(
                "do {{ my $cat_cmd = {}; ${} = qx{{$cat_cmd}}; }};\n",
                command_lit, target_var
            ));
        }
    }

    output
}
