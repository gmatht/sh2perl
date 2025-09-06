use crate::ast::*;
use crate::generator::Generator;

pub fn generate_sort_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str) -> String {
    generate_sort_command_with_output(generator, cmd, input_var, command_index, input_var)
}

pub fn generate_sort_command_with_output(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str, output_var: &str) -> String {
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
    
    output.push_str(&format!("my @sort_lines_{} = split /\\n/msx, ${};\n", command_index, input_var));
    if numeric {
        // For numeric sort, use a separate function to avoid complex sort blocks
        output.push_str(&format!("sub sort_numeric_{} {{\n", command_index));
        output.push_str("    my @a_fields = split /\\s+/msx, $a;\n");
        output.push_str("    my @b_fields = split /\\s+/msx, $b;\n");
        output.push_str("    my $a_num = 0;\n");
        output.push_str("    my $b_num = 0;\n");
        output.push_str(&format!("    if (scalar(@a_fields) > 0 && $a_fields[0] =~ {}) {{ $a_num = $a_fields[0]; }}\n", generator.format_regex_pattern(r"^\\d+$")));
        output.push_str(&format!("    if (scalar(@b_fields) > 0 && $b_fields[0] =~ {}) {{ $b_num = $b_fields[0]; }}\n", generator.format_regex_pattern(r"^\\d+$")));
        output.push_str("    return $a_num <=> $b_num || $a cmp $b;\n");
        output.push_str("}\n");
        output.push_str(&format!("my @sort_sorted_{} = sort sort_numeric_{} @sort_lines_{};\n", command_index, command_index, command_index));
    } else {
        output.push_str(&format!("my @sort_sorted_{} = sort @sort_lines_{};\n", command_index, command_index));
    }
    if reverse {
        output.push_str(&format!("@sort_sorted_{} = reverse @sort_sorted_{};\n", command_index, command_index));
    }
    output.push_str(&format!("${} = join \"\\n\", @sort_sorted_{};\n", output_var, command_index));
    // Ensure output ends with newline to match shell behavior
    output.push_str(&format!("{}\n", generator.convert_postfix_unless_to_block(&format!("${} =~ {}", output_var, generator.newline_end_regex()), &format!("${} .= \"\\n\"", output_var))));
    
    output
}
