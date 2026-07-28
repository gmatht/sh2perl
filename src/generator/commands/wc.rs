use crate::ast::*;
use crate::generator::Generator;
use crate::ir::{stmt_to_perl, Decl, IrExpr, IrStmt, Sigil, StrStyle};

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
            "do {{ local $INPUT_RECORD_SEPARATOR = undef; if (open my $fh, '<', '{}') {{ my $c = <$fh>; close $fh or warn \"Close failed: $OS_ERROR\"; $c }} else {{ warn \"Cannot access file: $OS_ERROR\"; q{{}} }} }}",
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

    // Build a single sprintf call through IR.  This produces clean
    // output like:  my $_wc_result = sprintf "%7d %7d %7d\n",
    //                $_wc_lines, $_wc_words, $_wc_bytes;
    // instead of the piecewise .= concatenation.
    let num_cols = [count_lines, count_words, count_chars || count_bytes, longest_line]
        .iter().filter(|&&x| x).count();
    let use_padding = num_cols > 1;
    let pad = if use_padding { "%7d" } else { "%d" };

    let mut fmt_parts: Vec<String> = Vec::new();
    let mut sprintf_args: Vec<IrExpr> = Vec::new();

    if count_lines {
        fmt_parts.push(pad.to_string());
        sprintf_args.push(IrExpr::Var("_wc_lines".to_string(), Sigil::Scalar));
    }
    if count_words {
        fmt_parts.push(pad.to_string());
        sprintf_args.push(IrExpr::Var("_wc_words".to_string(), Sigil::Scalar));
    }
    if count_chars || count_bytes {
        fmt_parts.push(pad.to_string());
        let var_name = if count_chars { "_wc_chars" } else { "_wc_bytes" };
        sprintf_args.push(IrExpr::Var(var_name.to_string(), Sigil::Scalar));
    }
    if longest_line {
        fmt_parts.push(pad.to_string());
        sprintf_args.push(IrExpr::Var("_wc_longest".to_string(), Sigil::Scalar));
    }

    // Append filename if provided
    if let Some(ref filename) = file_arg {
        fmt_parts.push(filename.clone());
    }

    // Use literal newline; DoubleQuoted handler will escape it to \\n
    fmt_parts.push("\n".to_string());
    let fmt_str = fmt_parts.join(" ");

    let mut all_args = vec![IrExpr::Str(fmt_str, StrStyle::DoubleQuoted)];
    all_args.extend(sprintf_args);

    let sprintf_expr = IrExpr::Call {
        func: "sprintf".to_string(),
        args: all_args,
    };
    let decl = IrStmt::Declare {
        vars: vec![Decl {
            name: "_wc_result".to_string(),
            sigil: Sigil::Scalar,
        }],
        init: Some(sprintf_expr),
        local: false,
    };
    output.push_str(&stmt_to_perl(&decl, generator.indent_level));
    output.push_str(&generator.indent());
    output.push_str("$_wc_result;\n");

    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("};\n");

    output
}
