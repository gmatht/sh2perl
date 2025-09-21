use crate::ast::*;
use crate::generator::Generator;

pub fn generate_cat_command(generator: &mut Generator, cmd: &SimpleCommand, redirects: &[Redirect], input_var: &str) -> String {
    let mut output = String::new();
    
    // Check if this cat command has heredoc redirects
    let mut has_heredoc = false;
    for redir in redirects {
        if matches!(redir.operator, RedirectOperator::Heredoc | RedirectOperator::HeredocTabs) {
            has_heredoc = true;
            if let Some(body) = &redir.heredoc_body {
                // Print the heredoc content directly
                output.push_str(&format!("print {};\n", generator.perl_string_literal(&Word::literal(body.clone()))));
            }
        }
    }
    
    // If no heredocs, handle file reading as before
    if !has_heredoc {
        let filename = if cmd.args.is_empty() { 
            "".to_string()
        } else { 
            // Reconstruct the filename from split arguments if needed
            if cmd.args.len() > 1 {
                cmd.args.iter()
                    .map(|arg| generator.word_to_perl(arg))
                    .collect::<Vec<_>>()
                    .join("")
            } else {
                generator.word_to_perl(&cmd.args[0])
            }
        };
        
        output.push_str(&format!("${} = q{{}};\n", input_var));
        // Adjust filename for Perl execution context (runs from examples directory)
        let adjusted_filename = generator.adjust_file_path_for_perl_execution(&filename);
        output.push_str(&format!("if (open my $fh, '<', '{}') {{\n", adjusted_filename));
        output.push_str("while (my $line = <$fh>) {\n");
        output.push_str(&format!("${} .= $line;\n", input_var));
        output.push_str("}\n");
        output.push_str("close $fh or croak \"Close failed: $OS_ERROR\";\n");
        output.push_str(&format!("# Ensure content ends with newline to prevent line concatenation\n"));
        output.push_str(&generator.indent());
        output.push_str(&format!("if (!(${} =~ {})) {{\n", input_var, generator.newline_end_regex()));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("${} .= \"\\n\";\n", input_var));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        output.push_str("} else {\n");
        output.push_str(&format!("carp \"cat: {}: No such file or directory\";\n", adjusted_filename));
        // Instead of calling exit(1), set the output to empty and let the pipeline handle the failure
        output.push_str(&format!("${} = q{{}};\n", input_var));
        output.push_str("}\n");
        output.push_str("\n");
    }
    
    output
}
