use crate::ast::*;
use crate::generator::Generator;

pub fn generate_wc_command(
    _generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
) -> String {
    generate_wc_command_with_output(
        _generator,
        cmd,
        input_var,
        command_index,
        &format!("wc_result_{}", command_index),
    )
}

pub fn generate_wc_command_with_output(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    command_index: &str,
    output_var: &str,
) -> String {
    let mut output = String::new();

    // Detect whether wc was called with -l so we can supply a default
    // "0\n" in cases where the child produced no output (matching real wc)
    let has_count_lines_flag = cmd.args.iter().any(|arg| match arg {
        Word::Literal(s, _) => s == "-l",
        _ => false,
    });

    let wc_args = cmd
        .args
        .iter()
        .map(|arg| generator.word_to_perl(arg))
        .collect::<Vec<_>>()
        .join(", ");

    let needs_stdin = !cmd.args.iter().any(|arg| match arg {
        Word::Literal(s, _) => s != "-" && !s.starts_with('-'),
        _ => true,
    });

    output.push_str("use IPC::Open3;\n");
    output.push_str(&format!("my @wc_args_{} = ({});\n", command_index, wc_args));
    output.push_str(&format!(
        "my ($wc_in_{}, $wc_out_{}, $wc_err_{});\n",
        command_index, command_index, command_index
    ));
    output.push_str(&format!(
        "my $wc_pid_{} = open3($wc_in_{}, $wc_out_{}, $wc_err_{}, 'wc', @wc_args_{});\n",
        command_index, command_index, command_index, command_index, command_index
    ));

    if needs_stdin {
        if input_var.is_empty() {
            output.push_str(&format!(
                "print {{$wc_in_{}}} do {{ local $/ = undef; <STDIN> }};\n",
                command_index
            ));
        } else {
            output.push_str(&format!(
                "print {{$wc_in_{}}} ${};\n",
                command_index, input_var
            ));
        }
    }

    output.push_str(&format!(
        "close $wc_in_{} or die \"Close failed: $!\\n\";\n",
        command_index
    ));
    let output_name = output_var.trim_start_matches('$');
    let output_var_expr = if output_var.starts_with('$') {
        output_var.to_string()
    } else {
        format!("${}", output_name)
    };

    if generator.declared_locals.contains(output_name) {
        output.push_str(&format!(
            "{} = do {{ local $/ = undef; <$wc_out_{}> }};\n",
            output_var_expr, command_index
        ));
    } else {
        output.push_str(&format!(
            "my {} = do {{ local $/ = undef; <$wc_out_{}> }};\n",
            output_var_expr, command_index
        ));
        generator.declared_locals.insert(output_name.to_string());
    }

    // If wc was asked to count lines (-l) but produced an empty string, the
    // real wc prints "0\n". Make the generated Perl mimic that behaviour so
    // command-substitutions end up with the expected textual result.
    if has_count_lines_flag {
        output.push_str(&format!(
            "if ({} eq q{{}}) {{ {} = \"0\\n\"; }}\n",
            output_var_expr, output_var_expr
        ));
    }
    output.push_str(&format!(
        "close $wc_out_{} or die \"Close failed: $!\\n\";\n",
        command_index
    ));
    output.push_str(&format!("waitpid $wc_pid_{}, 0;\n", command_index));

    output
}
