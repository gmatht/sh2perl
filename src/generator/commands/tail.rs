use crate::ast::*;
use crate::generator::Generator;

pub fn generate_tail_command(
    _generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    _command_index: usize,
) -> String {
    let mut output = String::new();

    // tail command syntax: tail [options] [file...]
    let mut num_lines = 10; // Default to last 10 lines
    let mut follow = false;

    // Parse tail options
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(arg_str, _) = &cmd.args[i] {
            if arg_str == "-f" || arg_str == "--follow" {
                follow = true;
            } else if arg_str == "-n" {
                if i + 1 < cmd.args.len() {
                    if let Word::Literal(num_str, _) = &cmd.args[i + 1] {
                        if let Ok(num) = num_str.parse::<usize>() {
                            num_lines = num;
                            i += 2;
                            continue;
                        }
                    }
                }
            } else if arg_str.starts_with("-n") {
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

    if follow {
        // Follow mode - this would require more complex logic in a real implementation
        output.push_str("carp \"tail: -f option not fully implemented in this version\\n\";\n");
    }

    if input_var.starts_with('$') {
        output.push_str(&format!("my @lines = split /\\n/msx, {};\n", input_var));
    } else {
        output.push_str(&format!("my @lines = split /\\n/msx, ${};\n", input_var));
    }
    output.push_str(&format!("my $num_lines = {};\n", num_lines));
    output.push_str("if ($num_lines > scalar @lines) {\n");
    output.push_str("$num_lines = scalar @lines;\n");
    output.push_str("}\n");
    output.push_str("my $start_index = scalar @lines - $num_lines;\n");
    output.push_str("if ($start_index < 0) { $start_index = 0; }\n");
    output.push_str("my @result = @lines[$start_index..$#lines];\n");
    if input_var.starts_with('$') {
        output.push_str(&format!("{} = join \"\\n\", @result;\n", input_var));
    } else {
        output.push_str(&format!("${} = join \"\\n\", @result;\n", input_var));
    }
    output.push_str("\n");

    output
}
