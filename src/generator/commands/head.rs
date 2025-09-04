use crate::ast::*;
use crate::generator::Generator;

pub fn generate_head_command(_generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: usize) -> String {
    let mut output = String::new();
    
    // head command syntax: head [options] [file...]
    let mut num_lines = 10; // Default to first 10 lines
    
    // Parse head options
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(arg_str, _) = &cmd.args[i] {
            if arg_str == "-n" {
                // Handle -n followed by number as separate argument
                if i + 1 < cmd.args.len() {
                    if let Word::Literal(num_str, _) = &cmd.args[i + 1] {
                        if let Ok(num) = num_str.parse::<usize>() {
                            num_lines = num;
                            i += 2; // Skip both -n and the number
                            continue;
                        }
                    }
                }
            } else if arg_str.starts_with("-n") {
                // Handle -n100 style (number attached to -n)
                if let Some(num_str) = arg_str.strip_prefix("-n") {
                    if let Ok(num) = num_str.parse::<usize>() {
                        num_lines = num;
                    }
                }
            } else if arg_str.starts_with("-") && arg_str.len() > 1 {
                // Handle -10, -20 style line counts
                if let Ok(num) = arg_str[1..].parse::<usize>() {
                    num_lines = num;
                }
            }
        }
        i += 1;
    }
    
    // Use line-by-line processing instead of arrays
    output.push_str(&format!("my $num_lines = {};\n", num_lines));
    output.push_str(&format!("my $line_count = 0;\n"));
    output.push_str(&format!("my $result = '';\n"));
    output.push_str(&format!("my $input = ${};\n", input_var));
    output.push_str(&format!("my $pos = 0;\n"));
    output.push_str(&format!("while ($pos < length($input) && $line_count < $num_lines) {{\n"));
    output.push_str(&format!("    my $line_end = index($input, \"\\n\", $pos);\n"));
    output.push_str(&format!("    if ($line_end == -1) {{\n"));
    output.push_str(&format!("        $line_end = length($input);\n"));
    output.push_str(&format!("    }}\n"));
    output.push_str(&format!("    my $line = substr($input, $pos, $line_end - $pos);\n"));
    output.push_str(&format!("    $result .= $line . \"\\n\";\n"));
    output.push_str(&format!("    $pos = $line_end + 1;\n"));
    output.push_str(&format!("    $line_count++;\n"));
    output.push_str(&format!("}}\n"));
    output.push_str(&format!("${} = $result;\n", input_var));
    output.push_str("\n");
    
    output
}
