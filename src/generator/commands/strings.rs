use crate::ast::*;
use crate::generator::Generator;

pub fn generate_strings_command(_generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, output_var: &str) -> String {
    let mut output = String::new();
    
    // strings command syntax: strings [options] file
    // Extracts printable strings from binary files
    let mut _min_length = 4; // Default minimum string length
    let mut filename = String::new();
    
    // Parse strings options and find the filename
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            if arg_str.starts_with("-n") {
                // Parse minimum length option
                if let Some(length_str) = arg_str.strip_prefix("-n") {
                    if let Ok(length) = length_str.parse::<usize>() {
                        _min_length = length;
                    }
                }
            } else if !arg_str.starts_with("-") {
                // This is the filename argument
                filename = arg_str.clone();
            }
        }
    }
    
    // If we have a filename, always read from file (even in pipeline context)
    if !filename.is_empty() {
        output.push_str(&format!("my $input_data;\n"));
        output.push_str(&format!("if (open my $fh, '<', '{}') {{\n", filename));
        output.push_str("local $INPUT_RECORD_SEPARATOR = undef;  # Read entire file at once\n");
        output.push_str("$input_data = <$fh>;\n");
        output.push_str("close $fh or croak \"Close failed: $ERRNO\";\n");
        output.push_str("} else {\n");
        output.push_str("$input_data = q{};\n");
        output.push_str("}\n");
    } else {
        // For pipeline context or no filename, use input_var
        let var_name = if input_var.starts_with('$') {
            input_var.to_string()
        } else {
            format!("${}", input_var)
        };
        output.push_str(&format!("my $input_data = {};\n", var_name));
    }
    
    output.push_str("my @result;\n");
    output.push_str("my @lines = split /\\n/msx, $input_data;\n");
    output.push_str("for my $line (@lines) {\n");
    output.push_str("if (length $line >= 4) {\n");
    output.push_str("push @result, $line;\n");
    output.push_str("}\n");
    output.push_str("}\n");
    output.push_str("my $line = join \"\\n\", @result;\n");
    if !output_var.is_empty() {
        output.push_str(&format!("${} = $line;\n", output_var));
    } else {
        output.push_str("$output_0 = $line;\n");
    }
    
    output
}
