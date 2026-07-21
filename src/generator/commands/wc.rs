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

    // Parse wc flags
    let mut count_lines = false;
    let mut count_words = false;
    let mut count_chars = false;
    let mut count_bytes = false;
    let mut longest_line = false;
    let mut file_arg: Option<String> = None;

    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s == "-l" {
                count_lines = true;
            } else if s == "-w" {
                count_words = true;
            } else if s == "-c" {
                count_bytes = true;
            } else if s == "-m" {
                count_chars = true;
            } else if s == "-L" {
                longest_line = true;
            } else if s.starts_with('-') {
                // Combined flags like -lw
                for ch in s[1..].chars() {
                    match ch {
                        'l' => count_lines = true,
                        'w' => count_words = true,
                        'c' => count_bytes = true,
                        'm' => count_chars = true,
                        'L' => longest_line = true,
                        _ => {}
                    }
                }
            } else if !s.starts_with('-') && s != "-" {
                // File argument
                file_arg = Some(s.clone());
            }
        }
    }

    // If no flags given, default is -l -w -c
    if !count_lines && !count_words && !count_chars && !count_bytes && !longest_line {
        count_lines = true;
        count_words = true;
        count_bytes = true;
    }

    // Build the counting code
    let output_name = output_var.trim_start_matches('$');
    let output_var_expr = if output_var.starts_with('$') {
        output_var.to_string()
    } else {
        format!("${}", output_name)
    };

    // Collect lines/words/chars
    let read_input = if let Some(ref filename) = file_arg {
        format!(
            "do {{ local $INPUT_RECORD_SEPARATOR = undef; if (open my $fh, '<', '{}') {{ my $c = <$fh>; close $fh or warn \"Close failed: $OS_ERROR\"; $c }} else {{ warn \"Cannot open file: $OS_ERROR\"; q{{}} }} }}",
            filename.replace('\'', "'\\''")
        )
    } else if input_var.is_empty() {
        "do {{ local $INPUT_RECORD_SEPARATOR = undef; <STDIN> }}".to_string()
    } else {
        let input_ref = if input_var.starts_with('$') {
            input_var.to_string()
        } else {
            format!("${}", input_var)
        };
        format!("{}", input_ref)
    };

    // Declare output variable
    if generator.declared_locals.contains(output_name) {
        output.push_str(&format!(
            "{} = do {{\n",
            output_var_expr
        ));
    } else {
        output.push_str(&format!(
            "my {} = do {{\n",
            output_var_expr
        ));
        generator.declared_locals.insert(output_name.to_string());
    }
    generator.indent_level += 1;

    output.push_str(&generator.indent());
    output.push_str(&format!("my $_wc_data = {};\n", read_input));

    if count_lines {
        output.push_str(&generator.indent());
        output.push_str("my $_wc_lines = () = $_wc_data =~ /\\n/gsxm;\n");
    }
    if count_words {
        output.push_str(&generator.indent());
        output.push_str("my $_wc_words = scalar split /\\s+/msx, $_wc_data;\n");
    }
    if count_bytes {
        output.push_str(&generator.indent());
        output.push_str("my $_wc_bytes = length($_wc_data);\n");
    }
    if count_chars {
        output.push_str(&generator.indent());
        output.push_str("my $_wc_chars = length($_wc_data);\n");
    }
    if longest_line {
        output.push_str(&generator.indent());
        output.push_str("my $_wc_longest = 0;\n");
        output.push_str(&generator.indent());
        output.push_str("for my $_wc_ll (split /\\n/msx, $_wc_data) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("my $_wc_len = length($_wc_ll);\n");
        output.push_str(&generator.indent());
        output.push_str("$_wc_longest = $_wc_len if $_wc_len > $_wc_longest;\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }

    // Build output string: wc output is space-separated counts followed by filename (if given)
    // Real wc omits padding when only one count column is requested.
    output.push_str(&generator.indent());
    output.push_str("my $_wc_result = q{};\n");
    // Count how many number columns we have
    let num_cols = [count_lines, count_words, count_chars || count_bytes, longest_line]
        .iter().filter(|&&x| x).count();
    let use_padding = num_cols > 1;
    if count_lines {
        let line_fmt = if use_padding { "%7d" } else { "%d" };
        output.push_str(&generator.indent());
        output.push_str(&format!("$_wc_result .= sprintf q{{{}}}, $_wc_lines;\n", line_fmt));
    }
    if count_words {
        let word_fmt = if use_padding { "%7d" } else { "%d" };
        output.push_str(&generator.indent());
        output.push_str(&format!("$_wc_result .= sprintf q{{{}}}, $_wc_words;\n", word_fmt));
    }
    if count_chars || count_bytes {
        let val = if count_chars { "$_wc_chars" } else { "$_wc_bytes" };
        let byte_fmt = if use_padding { "%7d" } else { "%d" };
        output.push_str(&generator.indent());
        output.push_str(&format!("$_wc_result .= sprintf q{{{}}}, {};\n", byte_fmt, val));
    }
    if longest_line {
        let long_fmt = if use_padding { "%7d" } else { "%d" };
        output.push_str(&generator.indent());
        output.push_str(&format!("$_wc_result .= sprintf q{{{}}}, $_wc_longest;\n", long_fmt));
    }

    // Add filename if provided
    if let Some(ref filename) = file_arg {
        output.push_str(&generator.indent());
        output.push_str(&format!("$_wc_result .= ' ' . '{}';\n", filename));
    }

    output.push_str(&generator.indent());
    output.push_str("$_wc_result .= \"\\n\";\n");
    output.push_str(&generator.indent());
    output.push_str("$_wc_result;\n");

    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("};\n");

    output
}
