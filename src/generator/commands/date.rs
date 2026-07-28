use crate::ast::*;
use crate::generator::Generator;
use crate::ir::{expr_to_perl, IrExpr, StrStyle};

fn perl_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn simple_word_text(word: &Word) -> Option<String> {
    match word {
        Word::Literal(text, _) => Some(text.clone()),
        Word::StringInterpolation(interp, _) => {
            let mut text = String::new();
            for part in &interp.parts {
                match part {
                    StringPart::Literal(s) => text.push_str(s),
                    _ => return None,
                }
            }
            Some(text)
        }
        _ => None,
    }
}

/// Build an IR expression for POSIX::strftime with the given format.
fn strftime_ir(format: &str, gmtime: bool) -> IrExpr {
    let time_func = if gmtime { "gmtime" } else { "localtime" };
    IrExpr::Call {
        func: "POSIX::strftime".to_string(),
        args: vec![
            IrExpr::Str(format.to_string(), StrStyle::SingleQuoted),
            IrExpr::Call {
                func: time_func.to_string(),
                args: vec![],
            },
        ],
    }
}

/// Build an IR expression for POSIX::strftime with a dynamic format variable.
fn strftime_var_ir(format_expr: IrExpr, gmtime: bool) -> IrExpr {
    let time_func = if gmtime { "gmtime" } else { "localtime" };
    IrExpr::Call {
        func: "POSIX::strftime".to_string(),
        args: vec![
            format_expr,
            IrExpr::Call {
                func: time_func.to_string(),
                args: vec![],
            },
        ],
    }
}

/// Return just the strftime call expression (without `require POSIX;`).
/// Used by generators that want to emit `require POSIX;` separately.
pub fn date_strftime_expr(format: &str, gmtime: bool) -> String {
    let ir = strftime_ir(format, gmtime);
    expr_to_perl(&ir)
}

fn default_date_expr() -> String {
    // Use the IR backend for clean formatting
    let ir = strftime_ir("%a %b %e %H:%M:%S %Z %Y", false);
    format!("require POSIX; {}", expr_to_perl(&ir))
}

fn format_date_expr(format: &str) -> String {
    let cleaned = format.strip_prefix('+').unwrap_or(format);
    let ir = strftime_ir(cleaned, false);
    format!("require POSIX; {}", expr_to_perl(&ir))
}

pub fn generate_date_expression(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut prefix = String::new();
    for (name, value) in &cmd.env_vars {
        prefix.push_str(&format!(
            "local $ENV{{{}}} = {};\n",
            name,
            generator.word_to_perl(value)
        ));
    }

    let body = match cmd.args.as_slice() {
        [] => default_date_expr(),
        // -u: print date in UTC
        [flag_word] if simple_word_text(flag_word).as_deref() == Some("-u") => {
            let ir = strftime_ir("%a %b %e %H:%M:%S UTC %Y", true);
            format!("require POSIX; {}", expr_to_perl(&ir))
        }
        // -u -d 'date string': parse and print in UTC
        [uflag, dflag, arg, ..]
            if simple_word_text(uflag).as_deref() == Some("-u")
                && simple_word_text(dflag).as_deref() == Some("-d") =>
        {
            let source_expr = generator.word_to_perl(arg);
            format!(
                "my $date_source = {};\nrequire POSIX;\nrequire Time::Local;\nif ($date_source =~ /^(\\d{{4}})-(\\d{{2}})-(\\d{{2}})\\s+(\\d{{2}}):(\\d{{2}}):(\\d{{2}})(?:\\s+UTC)?$/) {{\n    my $date_epoch = Time::Local::timegm($6,$5,$4,$3,$2-1,$1-1900);\n    POSIX::strftime('%a %b %e %H:%M:%S UTC %Y', gmtime($date_epoch))\n}}\nelsif ($date_source =~ /^@([0-9]+)$/) {{\n    my $date_epoch = $1;\n    POSIX::strftime('%a %b %e %H:%M:%S UTC %Y', gmtime($date_epoch))\n}}\nelse {{\n    select((select(STDOUT), $| = 1)[0]);\n    print {{*STDERR}} \"date: option requires an argument -- 'd'\\nTry 'date --help' for more information.\\n\";\n    q{{}};\n}}",
                source_expr
            )
        }
        // -u +format: print formatted date in UTC
        [uflag, format_word, ..] if simple_word_text(uflag).as_deref() == Some("-u") => {
            if let Some(format) = simple_word_text(format_word) {
                let cleaned = format.strip_prefix('+').unwrap_or(&format);
                let ir = strftime_ir(cleaned, true);
                format!("require POSIX; {}", expr_to_perl(&ir))
            } else {
                let format_expr = generator.word_to_perl(format_word);
                format!(
                    "my $date_now = time(); my $date_format = {}; $date_format =~ s/^\\+//; require POSIX; {}",
                    format_expr,
                    expr_to_perl(&strftime_var_ir(
                        IrExpr::Var("date_format".to_string(), crate::ir::Sigil::Scalar),
                        true
                    ))
                )
            }
        }
        [flag_word, arg, ..] if simple_word_text(flag_word).as_deref() == Some("-r") => {
            let path_expr = generator.word_to_perl(arg);
            let ir = strftime_ir("%a %b %e %H:%M:%S %Z %Y", false);
            // Replace localtime() with localtime((stat(path))[9])
            let strftime_call = expr_to_perl(&ir);
            // The strftime IR uses localtime() — for -r we need localtime((stat(path))[9])
            // Since expr_to_perl gives us the clean call, we swap the time argument.
            let time_arg = format!("localtime((stat({}))[9])", path_expr);
            let modified_call = strftime_call.replace("localtime()", &time_arg);
            format!("my $date_path = {};\nrequire POSIX; {}", path_expr, modified_call)
        }
        [flag_word, arg, ..] if simple_word_text(flag_word).as_deref() == Some("-d") => {
            let source_expr = generator.word_to_perl(arg);
            let ir_str = expr_to_perl(&strftime_ir("%a %b %e %H:%M:%S %Z %Y", false));
            format!(
                "my $date_source = {};\nrequire POSIX;\nif ($date_source =~ /^@([0-9]+)$/) {{\n    my $date_epoch = $1;\n    {}\n}}\nelse {{\n    select((select(STDOUT), $| = 1)[0]);\n    print {{*STDERR}} \"date: option requires an argument -- 'd'\\nTry 'date --help' for more information.\\n\";\n    q{{}};\n}}",
                source_expr,
                ir_str.replace("localtime()", "localtime($date_epoch)")
            )
        }
        [format_word, ..] => {
            if let Some(format) = simple_word_text(format_word) {
                format_date_expr(&format)
            } else {
                let format_expr = generator.word_to_perl(format_word);
                format!(
                    "my $date_now = time(); my $date_format = {}; $date_format =~ s/^\\+//; require POSIX; {}",
                    format_expr,
                    expr_to_perl(&strftime_var_ir(
                        IrExpr::Var("date_format".to_string(), crate::ir::Sigil::Scalar),
                        false
                    ))
                )
            }
        }
    };

    format!("{}{}", prefix, body)
}

/// Helper: if `body` starts with `require POSIX; ` or `require POSIX;\n` return
/// the part after that prefix.  This lets callers split the require from the
/// expression for cleaner output.
pub fn split_posix_require(body: &str) -> Option<&str> {
    if let Some(rest) = body.strip_prefix("require POSIX; ") {
        Some(rest)
    } else if let Some(rest) = body.strip_prefix("require POSIX;\n") {
        Some(rest)
    } else {
        None
    }
}

pub fn generate_date_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let body = generate_date_expression(generator, cmd);
    // If the body is just `require POSIX; expr`, split into separate statements
    // to avoid the `do { require POSIX; expr }` wrapper (Pattern B).
    if let Some(expr) = split_posix_require(&body) {
        format!(
            "require POSIX;\nmy $date = {} . \"\\n\";\nprint $date;\n",
            expr
        )
    } else {
        // Complex body (e.g., -d with multiple requires) — keep the do-block.
        format!("my $date = do {{\n{}\n}} . \"\\n\";\nprint $date;\n", body)
    }
}

