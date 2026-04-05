use crate::ast::*;
use crate::generator::Generator;

pub fn generate_printf_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    _input_var: &str,
    _command_index: usize,
    output_var: Option<&str>,
) -> String {
    let mut output = String::new();

    // Parse printf format string and arguments
    let mut format_string = String::new();
    let mut args = Vec::new();

    for (i, arg) in cmd.args.iter().enumerate() {
        if i == 0 {
            // First argument is the format string
            format_string = generator.word_to_perl(arg);
            // Remove quotes if they exist around the format string
            if format_string.starts_with('\'') && format_string.ends_with('\'') {
                format_string = format_string[1..format_string.len() - 1].to_string();
            } else if format_string.starts_with('"') && format_string.ends_with('"') {
                format_string = format_string[1..format_string.len() - 1].to_string();
            }
        } else {
            // Subsequent arguments are the values to format
            args.push(generator.word_to_perl(arg));
        }
    }

    if format_string.is_empty() {
        // No format string provided, return error
        output.push_str("carp \"printf: no format string specified\";\n");
        output.push_str("exit 1;\n");
    } else {
        // Handle special case for array expansion like "${lines[@]}" or @"lines"
        if args.len() == 1
            && (args[0].contains("${") && args[0].contains("[@]")
                || args[0].contains("@") && !args[0].contains(" "))
        {
            // This is likely an array expansion like "${arr[@]}" or @"lines"
            let mut array_var = args[0].clone();

            // Extract the array name from "${arr[@]}" or @"lines"
            if array_var.starts_with('"') && array_var.ends_with('"') {
                array_var = array_var[1..array_var.len() - 1].to_string();
            }
            if array_var.starts_with('\'') && array_var.ends_with('\'') {
                array_var = array_var[1..array_var.len() - 1].to_string();
            }
            if array_var.starts_with('@') {
                array_var = array_var[1..].to_string();
            }
            if let Some(start) = array_var.find("${") {
                if let Some(end) = array_var.find("[@]") {
                    array_var = array_var[start + 2..end].to_string();
                }
            }

            // Generate Perl code to print array elements with the format
            output.push_str(&format!("foreach my $item (@{}) {{\n", array_var));
            output.push_str(&format!("    printf(\"{}\", $item);\n", format_string));
            output.push_str("}\n");
        } else {
            // Regular printf with individual arguments
            // For printf, format string and arguments should be separate
            if args.is_empty() {
                if let Some(var) = output_var {
                    // Capture printf output to variable
                    output.push_str(&format!("my ${};\n", var));
                    output.push_str(&format!("{{\n"));
                    output.push_str(&format!("    local *STDOUT;\n"));
                    output.push_str(&format!(
                        "    open STDOUT, '>', \\${} or die \"Cannot redirect STDOUT\";\n",
                        var
                    ));
                    output.push_str(&format!("    printf(\"{}\");\n", format_string));
                    output.push_str(&format!("}}\n"));
                } else {
                    output.push_str(&format!("printf(\"{}\");\n", format_string));
                }
            } else {
                // Build the printf call with format string and arguments properly separated
                // For compatibility with broken printf system call behavior, convert numeric arguments to strings
                let mut printf_call = format!("printf(\"{}\"", format_string);
                for (_i, arg) in args.iter().enumerate() {
                    let raw_arg = arg.trim_matches(|c| c == '"' || c == '\'');
                    // Check if the argument is a numeric literal and if the corresponding format specifier is %c
                    if raw_arg.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && format_string.contains("%c")
                    {
                        // For numeric arguments with %c format, use ord to get ASCII value of first character to match broken printf behavior
                        printf_call.push_str(&format!(", ord(substr(\"{}\", 0, 1))", raw_arg));
                    } else {
                        printf_call.push_str(&format!(", {}", arg));
                    }
                }
                printf_call.push_str(");\n");

                if let Some(var) = output_var {
                    // Capture printf output to variable
                    output.push_str(&format!("my ${};\n", var));
                    output.push_str(&format!("{{\n"));
                    output.push_str(&format!("    local *STDOUT;\n"));
                    output.push_str(&format!(
                        "    open STDOUT, '>', \\${} or die \"Cannot redirect STDOUT\";\n",
                        var
                    ));
                    output.push_str(&format!("    {}\n", printf_call.trim()));
                    output.push_str(&format!("}}\n"));
                } else {
                    output.push_str(&printf_call);
                }
            }
        }
    }

    output
}
