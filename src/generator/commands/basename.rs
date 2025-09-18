use crate::ast::*;
use crate::generator::Generator;

pub fn generate_basename_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, output_var: &str) -> String {
    let mut output = String::new();
    
    // basename command syntax: basename path [suffix]
    if let Some(path) = cmd.args.first() {
        let path_str = generator.word_to_perl(path);
        let suffix = if cmd.args.len() > 1 {
            generator.word_to_perl(&cmd.args[1])
        } else {
            "".to_string()
        };
        
        output.push_str(&format!("my $path = {};\n", path_str));
        if !suffix.is_empty() {
            output.push_str(&format!("my $suffix = {};\n", suffix));
            output.push_str("$path =~ s/\\Q$suffix\\E$//msx;\n");
        }
        output.push_str("$path =~ s/.*\\///msx;\n"); // Remove directory part
        if !output_var.is_empty() {
            output.push_str(&format!("${} = $path;\n", output_var));
        } else if !input_var.is_empty() {
            output.push_str(&format!("${} = $path;\n", input_var));
        } else {
            output.push_str("print $path, \"\\n\";\n");
        }
    } else if !input_var.is_empty() {
        // Use pipeline input when no arguments provided
        let suffix = if cmd.args.len() > 1 {
            generator.word_to_perl(&cmd.args[1])
        } else {
            "".to_string()
        };
        
        output.push_str(&format!("my $path = ${};\n", input_var));
        if !suffix.is_empty() {
            output.push_str(&format!("my $suffix = {};\n", suffix));
            output.push_str("$path =~ s/\\Q$suffix\\E$//msx;\n");
        }
        output.push_str("$path =~ s/.*\\///msx;\n"); // Remove directory part
        if !output_var.is_empty() {
            output.push_str(&format!("${} = $path;\n", output_var));
        } else {
            output.push_str(&format!("${} = $path;\n", input_var));
        }
    } else {
        // Default to current directory
        if !output_var.is_empty() {
            output.push_str(&format!("${} = q{{.}};\n", output_var));
        } else if !input_var.is_empty() {
            output.push_str(&format!("${} = q{{.}};\n", input_var));
        } else {
            output.push_str("print q{.}, \"\\n\";\n");
        }
    }
    output.push_str("\n");
    
    output
}
