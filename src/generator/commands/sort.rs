use crate::ast::*;
use crate::generator::Generator;

pub fn generate_sort_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
) -> String {
    let output_var = format!("sort_output_{}", command_index);
    generate_sort_command_with_output(generator, cmd, input_var, command_index, &output_var)
}

pub fn generate_sort_command_with_output(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
    output_var: &str,
) -> String {
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

    let input_ref = if input_var.starts_with('$') {
        input_var.to_string()
    } else {
        format!("${}", input_var)
    };
    output.push_str(&format!(
        "my @sort_lines_{} = split /\\n/msx, {};\n",
        command_index, input_ref
    ));
    if numeric {
        // For numeric sort, use a separate function to avoid complex sort blocks
        output.push_str(&format!("sub sort_numeric_{} {{\n", command_index));
        output.push_str("    my @a_fields = split /\\s+/msx, $a;\n");
        output.push_str("    my @b_fields = split /\\s+/msx, $b;\n");
        output.push_str("    my $a_num = 0;\n");
        output.push_str("    my $b_num = 0;\n");
        output.push_str(&format!(
            "    if ( scalar @a_fields > 0 && $a_fields[0] =~ {} ) {{ $a_num = $a_fields[0]; }}\n",
            generator.format_regex_pattern(r"^\d+$")
        ));
        output.push_str(&format!(
            "    if ( scalar @b_fields > 0 && $b_fields[0] =~ {} ) {{ $b_num = $b_fields[0]; }}\n",
            generator.format_regex_pattern(r"^\d+$")
        ));
        output.push_str("    return $a_num <=> $b_num || $a cmp $b;\n");
        output.push_str("}\n");
        output.push_str(&format!(
            "my @sort_sorted_{} = sort sort_numeric_{} @sort_lines_{};\n",
            command_index, command_index, command_index
        ));
    } else {
        output.push_str(&format!(
            "my @sort_sorted_{} = sort @sort_lines_{};\n",
            command_index, command_index
        ));
    }
    if reverse {
        output.push_str(&format!(
            "@sort_sorted_{} = reverse @sort_sorted_{};\n",
            command_index, command_index
        ));
    }
    let output_name = output_var.trim_start_matches('$');
    let output_ref = if output_var.starts_with('$') {
        output_var.to_string()
    } else {
        format!("${}", output_name)
    };

    // Reuse an existing declaration when the caller already introduced this variable.
    if generator.declared_locals.contains(output_name) {
        output.push_str(&format!(
            "{} = join \"\\n\", @sort_sorted_{};\n",
            output_ref, command_index
        ));
    } else {
        output.push_str(&format!(
            "my {} = join \"\\n\", @sort_sorted_{};\n",
            output_ref, command_index
        ));
        generator.declared_locals.insert(output_name.to_string());
    }
    // Ensure output ends with newline to match shell behavior
    output.push_str(&generator.indent());
    output.push_str(&format!(
        "if ({} ne q{{}} && !({} =~ {})) {{\n",
        output_ref,
        output_ref,
        generator.newline_end_regex()
    ));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("{} .= \"\\n\";\n", output_ref));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");

    // If this is being used in a pipeline context, assign the result back to the input variable
    if input_var != output_var {
        let output_input_ref = if input_var.starts_with('$') {
            input_var.to_string()
        } else {
            format!("${}", input_var)
        };
        output.push_str(&generator.indent());
        output.push_str(&format!("{} = {};\n", output_input_ref, output_ref));
    }

    output
}
