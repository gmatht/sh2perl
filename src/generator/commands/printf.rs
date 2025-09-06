use crate::generator::Generator;
use crate::ast::*;

pub fn generate_printf_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: usize) -> String {
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
                format_string = format_string[1..format_string.len()-1].to_string();
            } else if format_string.starts_with('"') && format_string.ends_with('"') {
                format_string = format_string[1..format_string.len()-1].to_string();
            }
        } else {
            // Subsequent arguments are the values to format
            args.push(generator.word_to_perl(arg));
        }
    }
    
    if format_string.is_empty() {
        // No format string provided, return error
        output.push_str("carp \"printf: no format string specified\";\n");
        output.push_str("exit(1);\n");
    } else {
        // Handle special case for array expansion like "${lines[@]}" or @"lines"
        if args.len() == 1 && (args[0].contains("${") && args[0].contains("[@]") || args[0].contains("@") && !args[0].contains(" ")) {
            // This is likely an array expansion like "${arr[@]}" or @"lines"
            let mut array_var = args[0].clone();
            
            // Extract the array name from "${arr[@]}" or @"lines"
            if array_var.starts_with('"') && array_var.ends_with('"') {
                array_var = array_var[1..array_var.len()-1].to_string();
            }
            if array_var.starts_with('\'') && array_var.ends_with('\'') {
                array_var = array_var[1..array_var.len()-1].to_string();
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
                output.push_str(&format!("printf(\"{}\");\n", format_string));
            } else {
                // Build the printf call with format string and arguments properly separated
                let mut printf_call = format!("printf(\"{}\"", format_string);
                for arg in &args {
                    printf_call.push_str(&format!(", {}", arg));
                }
                printf_call.push_str(");\n");
                output.push_str(&printf_call);
            }
        }
    }
    
    output
}
