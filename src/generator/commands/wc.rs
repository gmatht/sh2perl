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
    let mut file_args: Vec<String> = Vec::new();

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
            } else if s != "-" {
                // File argument
                file_args.push(s.clone());
            }
        }
    }

    // If no flags given, default is -l -w -c
    if !count_lines && !count_words && !count_chars && !count_bytes && !longest_line {
        count_lines = true;
        count_words = true;
        count_bytes = true;
    }

    let file_arg = file_args.first().cloned();

    // Build the counting code
    let output_name = output_var.trim_start_matches('$');
    let output_var_expr = if output_var.starts_with('$') {
        output_var.to_string()
    } else {
        format!("${}", output_name)
    };

    // GNU wc with MULTIPLE files prints per-file lines plus a `total` row;
    // the native emulation reads ONE file. Emit the real `bash -c` capture
    // (per-file + totals exactly) — the legacy caller always `print`s the
    // output var, so a bare empty return would leave it undeclared.
    if file_args.len() > 1 {
        let flags = {
            let mut f = String::new();
            if count_lines {
                f.push_str(" -l");
            }
            if count_words {
                f.push_str(" -w");
            }
            if count_chars {
                f.push_str(" -m");
            }
            if count_bytes {
                f.push_str(" -c");
            }
            if longest_line {
                f.push_str(" -L");
            }
            f
        };
        let cmd_text = format!(
            "wc{}{}",
            flags,
            file_args
                .iter()
                .map(|f| format!(" '{}'", f.replace('\'', "'\\''")))
                .collect::<String>()
        );
        let q = crate::ir::safe_perl_q_string(&cmd_text);
        return format!(
            "{} = do {{ open(my $__fh, '-|', 'bash', '-c', {}) or die \"cmd failed: $!\\n\"; my $_r = do {{ local $/; <$__fh> }}; close $__fh; chomp $_r; $_r; }};\n",
            output_var_expr, q
        );
    }

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
        output.push_str(&format!("{} = do {{\n", output_var_expr));
    } else {
        output.push_str(&format!("my {} = do {{\n", output_var_expr));
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
    let num_cols = [
        count_lines,
        count_words,
        count_chars || count_bytes,
        longest_line,
    ]
    .iter()
    .filter(|&&x| x)
    .count();
    // GNU wc field width: digits of the LARGEST count across all files and
    // columns — `(5,8,48)` → width 2 → ` 5  8 48 file`; `(1,1,2)` → width 1
    // → `1 1 2 file`; six-digit counts → `100000 588895` (no visible pad).
    // The old `%7d` over-padded. The width is a RUNTIME value (the counts
    // depend on the file), so emit a width computation + `%*d` fields.
    let use_padding = num_cols > 1;
    let pad = if use_padding { "%*d" } else { "%d" };
    let count_vars: Vec<&str> = {
        let mut v = Vec::new();
        if count_lines {
            v.push("_wc_lines");
        }
        if count_words {
            v.push("_wc_words");
        }
        if count_chars || count_bytes {
            v.push(if count_chars { "_wc_chars" } else { "_wc_bytes" });
        }
        if longest_line {
            v.push("_wc_longest");
        }
        v
    };
    if use_padding {
        output.push_str("my $_wc_w = 1;\n");
        output.push_str(&format!(
            "for my $_wc_c ({}) {{\n",
            count_vars
                .iter()
                .map(|v| format!("${v}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        output.push_str("    my $_wc_d = length($_wc_c);\n");
        output.push_str("    $_wc_w = $_wc_d if $_wc_d > $_wc_w;\n");
        output.push_str("}\n");
    }

    let mut fmt_parts: Vec<String> = Vec::new();
    let mut sprintf_args: Vec<IrExpr> = Vec::new();

    if count_lines {
        fmt_parts.push(pad.to_string());
        if use_padding {
            sprintf_args.push(IrExpr::Var("_wc_w".to_string(), Some(Sigil::Scalar)));
        }
        sprintf_args.push(IrExpr::Var("_wc_lines".to_string(), Some(Sigil::Scalar)));
    }
    if count_words {
        fmt_parts.push(pad.to_string());
        if use_padding {
            sprintf_args.push(IrExpr::Var("_wc_w".to_string(), Some(Sigil::Scalar)));
        }
        sprintf_args.push(IrExpr::Var("_wc_words".to_string(), Some(Sigil::Scalar)));
    }
    if count_chars || count_bytes {
        fmt_parts.push(pad.to_string());
        if use_padding {
            sprintf_args.push(IrExpr::Var("_wc_w".to_string(), Some(Sigil::Scalar)));
        }
        let var_name = if count_chars {
            "_wc_chars"
        } else {
            "_wc_bytes"
        };
        sprintf_args.push(IrExpr::Var(var_name.to_string(), Some(Sigil::Scalar)));
    }
    if longest_line {
        fmt_parts.push(pad.to_string());
        if use_padding {
            sprintf_args.push(IrExpr::Var("_wc_w".to_string(), Some(Sigil::Scalar)));
        }
        sprintf_args.push(IrExpr::Var("_wc_longest".to_string(), Some(Sigil::Scalar)));
    }

    // Columns joined with single spaces; the filename (if any) follows with
    // ONE space and NO trailing space (GNU wc: `… file\n`, not `… file \n`).
    let mut fmt_str = fmt_parts.join(" ");
    if let Some(ref filename) = file_arg {
        fmt_str.push(' ');
        fmt_str.push_str(filename);
    }
    fmt_str.push_str("\n");

    let mut all_args = vec![IrExpr::Str(fmt_str, StrStyle::DoubleQuoted)];
    all_args.extend(sprintf_args);

    let sprintf_expr = IrExpr::Call {
        func: "sprintf".to_string(),
        args: all_args,
    };
    let decl = IrStmt::Declare {
        vars: vec![Decl {
            name: "_wc_result".to_string(),
            sigil: Some(Sigil::Scalar),
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
