use crate::ast::*;
use crate::generator::Generator;

pub fn generate_tee_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str) -> String {
    let mut output = String::new();
    
    // tee command syntax: tee [options] file
    let mut append_mode = false;
    let mut files = Vec::new();
    
    // Parse tee options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            if arg_str == "-a" {
                append_mode = true;
            } else if !arg_str.starts_with('-') {
                files.push(generator.word_to_perl(arg));
            }
        } else {
            files.push(generator.word_to_perl(arg));
        }
    }
    
    if files.is_empty() {
        // No files specified, just pass through
        output.push_str(&format!("{} = {};\n", input_var, input_var));
    } else {
        // Write to specified files
        let input_ref = if input_var.starts_with('$') { input_var } else { &format!("${}", input_var) };
        output.push_str(&format!("my @lines = split /\\n/msx, {};\n", input_ref));
        
        for file in &files {
            let mode = if append_mode { ">>" } else { ">" };
            output.push_str(&format!("if (open my $fh, '{}', \"{}\") {{\n", mode, file));
            output.push_str("foreach my $line (@lines) {\n");
            output.push_str("print {$fh} \"$line\\n\";\n");
            output.push_str("}\n");
            output.push_str("close $fh or croak \"Close failed: $ERRNO\";\n");
            output.push_str("} else {\n");
            output.push_str(&format!("carp \"tee: Cannot open {}: $ERRNO\";\n", file));
            output.push_str("}\n");
        }
        
        // Keep the output for further processing - the input is already preserved in the variable
    }
    output.push_str("\n");
    
    output
}
