use crate::ast::*;
use crate::generator::Generator;

pub fn generate_tee_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    output_var: &str,
) -> String {
    let mut output = String::new();
    let input_expr = if input_var.is_empty() {
        "q{}".to_string()
    } else if input_var.starts_with('$') {
        input_var.to_string()
    } else {
        format!("${}", input_var)
    };

    // tee command syntax: tee [options] file
    let mut append_mode = false;
    let mut files = Vec::new();
    let mut stdout_copies = 1usize;

    // Parse tee options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
                if arg_str == "-a" {
                    append_mode = true;
                } else if !arg_str.starts_with('-') {
                if arg_str == "/dev/stdout" {
                    continue;
                }
                let special_stderr = arg_str == "/dev/stderr";
                files.push((generator.perl_string_literal(arg), special_stderr));
            }
        } else {
            files.push((generator.perl_string_literal(arg), false));
        }
    }

    if files.is_empty() {
        // No files specified, just pass through the input value.
    } else {
        output.push_str("use Carp qw(carp croak);\n");

        // Write to specified files
        for (file, special_stderr) in &files {
            if *special_stderr {
                output.push_str("use IO::Handle;\n");
                output.push_str("STDOUT->flush();\n");
                output.push_str("if ( open my $fh, '>', '/dev/stderr' ) {\n");
                output.push_str(&format!("    print {{$fh}} {};\n", input_expr));
                output.push_str("    close $fh or croak \"Close failed: $ERRNO\";\n");
                output.push_str("}\n");
                output.push_str("else {\n");
                output.push_str("    carp \"tee: Cannot open /dev/stderr: $ERRNO\";\n");
                output.push_str("}\n");
                continue;
            }
            let mode = if append_mode { ">>" } else { ">" };
            output.push_str(&format!("if ( open my $fh, '{}', {} ) {{\n", mode, file));
            output.push_str(&format!("    print {{$fh}} {};\n", input_expr));
            output.push_str("    close $fh or croak \"Close failed: $ERRNO\";\n");
            output.push_str("}\n");
            output.push_str("else {\n");
            output.push_str(&format!(
                "    carp \"tee: Cannot open {}: $ERRNO\";\n",
                file
            ));
            output.push_str("}\n");
        }

        // Keep the output for further processing - the input is already preserved in the variable
    }

    if output_var.is_empty() {
        let stdout_expr = std::iter::repeat(input_expr.clone())
            .take(stdout_copies)
            .collect::<Vec<_>>()
            .join(" . ");
        output.push_str(&format!("print {};\n", stdout_expr));
    } else {
        let stdout_expr = std::iter::repeat(input_expr)
            .take(stdout_copies)
            .collect::<Vec<_>>()
            .join(" . ");
        output.push_str(&format!("${} = {};\n", output_var, stdout_expr));
    }

    output
}
