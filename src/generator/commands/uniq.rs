use crate::ast::*;
use crate::generator::Generator;

pub fn generate_uniq_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
) -> String {
    generate_uniq_command_with_output(generator, cmd, input_var, command_index, input_var)
}

pub fn generate_uniq_command_with_output(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
    output_var: &str,
) -> String {
    let mut output = String::new();

    let mut count = false;

    // Check for flags
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            if arg_str == "-c" {
                count = true;
            }
        }
    }

    output.push_str(&format!(
        "my @uniq_lines_{} = split /\\n/msx, ${};\n",
        command_index, input_var
    ));
    output.push_str(&format!(
        "@uniq_lines_{} = grep {{ $_ ne q{{}} }} @uniq_lines_{}; # Filter out empty lines\n",
        command_index, command_index
    ));
    if count {
        output.push_str(&format!("my %uniq_counts_{};\n", command_index));
        output.push_str(&format!("my @uniq_order_{};\n", command_index));
        output.push_str(&format!(
            "foreach my $line (@uniq_lines_{}) {{\n",
            command_index
        ));
        output.push_str(&format!(
            "if (!exists $uniq_counts_{}{{$line}}) {{ push @uniq_order_{}, $line; }}\n",
            command_index, command_index
        ));
        output.push_str(&format!("$uniq_counts_{}{{$line}}++;\n", command_index));
        output.push_str("}\n");
        output.push_str(&format!("my @uniq_result_{};\n", command_index));
        output.push_str(&format!(
            "foreach my $line (@uniq_order_{}) {{\n",
            command_index
        ));
        output.push_str(&format!(
            "push @uniq_result_{}, sprintf \"%7d %s\", $uniq_counts_{}{{$line}}, $line;\n",
            command_index, command_index
        ));
        output.push_str("}\n");
        let output_name = output_var.trim_start_matches('$');
        let output_ref = if output_var.starts_with('$') {
            output_var.to_string()
        } else {
            format!("${}", output_name)
        };
        if generator.declared_locals.contains(output_name) {
            output.push_str(&format!(
                "{} = join \"\\n\", @uniq_result_{};\n",
                output_ref, command_index
            ));
        } else {
            output.push_str(&format!(
                "my {} = join \"\\n\", @uniq_result_{};\n",
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
    } else {
        output.push_str(&format!("my %uniq_seen_{};\n", command_index));
        output.push_str(&format!("my @uniq_result_{};\n", command_index));
        output.push_str(&format!(
            "foreach my $line (@uniq_lines_{}) {{\n",
            command_index
        ));
        output.push_str(&format!(
            "if (!$uniq_seen_{}{{$line}}++) {{ push @uniq_result_{}, $line; }}\n",
            command_index, command_index
        ));
        output.push_str("}\n");
        let output_name = output_var.trim_start_matches('$');
        let output_ref = if output_var.starts_with('$') {
            output_var.to_string()
        } else {
            format!("${}", output_name)
        };
        if generator.declared_locals.contains(output_name) {
            output.push_str(&format!(
                "{} = join \"\\n\", @uniq_result_{};\n",
                output_ref, command_index
            ));
        } else {
            output.push_str(&format!(
                "my {} = join \"\\n\", @uniq_result_{};\n",
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
    }

    output
}
