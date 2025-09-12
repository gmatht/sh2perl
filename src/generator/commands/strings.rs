use crate::ast::*;
use crate::generator::Generator;

pub fn generate_strings_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str) -> String {
    let mut output = String::new();
    
    // strings command syntax: strings [options] file
    // Extracts printable strings from binary files
    let mut min_length = 4; // Default minimum string length
    
    // Parse strings options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            if arg_str.starts_with("-n") {
                // Parse minimum length option
                if let Some(length_str) = arg_str.strip_prefix("-n") {
                    if let Ok(length) = length_str.parse::<usize>() {
                        min_length = length;
                    }
                }
            }
        }
    }
    
    // For strings command, we need to process the input as binary data
    output.push_str(&format!("my $input_data = {};\n", input_var));
    output.push_str("my @result;\n");
    output.push_str("my $current_string = q{};\n");
    output.push_str("for my $char (split //msx, $input_data) {\n");
    output.push_str("if ($char =~ /[\\x20-\\x7E]/msx) {\n"); // Printable ASCII
    output.push_str("$current_string .= $char;\n");
    output.push_str("} else {\n");
    output.push_str(&format!("if (length $current_string >= {}) {{\n", min_length));
    output.push_str("push @result, $current_string;\n");
    output.push_str("}\n");
    output.push_str("$current_string = q{};\n");
    output.push_str("}\n");
    output.push_str("}\n");
    output.push_str(&format!("if (length $current_string >= {}) {{\n", min_length));
    output.push_str("push @result, $current_string;\n");
    output.push_str("}\n");
    output.push_str(&format!("{} = join \"\\n\", @result;\n", input_var));
    output.push_str("\n");
    
    output
}
