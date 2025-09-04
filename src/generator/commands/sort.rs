use crate::ast::*;
use crate::generator::Generator;

pub fn generate_sort_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str) -> String {
    generate_sort_command_with_output(generator, cmd, input_var, command_index, input_var)
}

pub fn generate_sort_command_with_output(_generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str, output_var: &str) -> String {
    let mut output = String::new();
    
    let mut numeric = false;
    let mut reverse = false;
    
    // Check for flags
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            if arg_str == "-n" {
                numeric = true;
            } else if arg_str == "r" || arg_str == "-r" {
                reverse = true;
            } else if arg_str == "-nr" || arg_str == "-rn" {
                numeric = true;
                reverse = true;
            }
        }
    }
    
    output.push_str(&format!("my @sort_lines_{} = split(/\\n/, ${});\n", command_index, input_var));
    if numeric {
        // For numeric sort, extract the first field (number) and sort by that
        // Use the entire line as secondary sort key for stable sort behavior
        output.push_str(&format!("my @sort_sorted_{} = sort {{\n", command_index));
        output.push_str("    my @a_fields = split(/\\s+/, $a);\n");
        output.push_str("    my @b_fields = split(/\\s+/, $b);\n");
        output.push_str("    my $a_num = 0;\n");
        output.push_str("    my $b_num = 0;\n");
        output.push_str("    if (scalar(@a_fields) > 0 && $a_fields[0] =~ /^\\d+$/) { $a_num = $a_fields[0]; }\n");
        output.push_str("    if (scalar(@b_fields) > 0 && $b_fields[0] =~ /^\\d+$/) { $b_num = $b_fields[0]; }\n");
        output.push_str("    $a_num <=> $b_num || $a cmp $b;\n");
        output.push_str(&format!("}} @sort_lines_{};\n", command_index));
    } else {
        output.push_str(&format!("my @sort_sorted_{} = sort @sort_lines_{};\n", command_index, command_index));
    }
    if reverse {
        output.push_str(&format!("@sort_sorted_{} = reverse(@sort_sorted_{});\n", command_index, command_index));
    }
    output.push_str(&format!("${} = join(\"\\n\", @sort_sorted_{});\n", output_var, command_index));
    // Ensure output ends with newline to match shell behavior
    output.push_str(&format!("${} .= \"\\n\" unless ${} =~ /\\n$/;\n", output_var, output_var));
    
    output
}
