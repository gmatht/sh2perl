use crate::ast::*;
use crate::generator::Generator;

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

fn default_date_expr() -> String {
    "require POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime($DATE_SNAPSHOT)) . \"\\n\""
        .to_string()
}

fn format_date_expr(format: &str) -> String {
    let cleaned = format.strip_prefix('+').unwrap_or(format);
    let format_expr = perl_single_quoted(cleaned);

    format!(
        "require POSIX; POSIX::strftime({}, localtime($DATE_SNAPSHOT)) . \"\\n\"",
        format_expr
    )
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
        [flag_word, arg, ..] if simple_word_text(flag_word).as_deref() == Some("-r") => {
            let path_expr = generator.word_to_perl(arg);
            format!(
                "my $date_path = {};\nrequire POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime((stat($date_path))[9])) . \"\\n\"",
                path_expr
            )
        }
        [flag_word, arg, ..] if simple_word_text(flag_word).as_deref() == Some("-d") => {
            let source_expr = generator.word_to_perl(arg);
            format!(
                "my $date_source = {};\nrequire POSIX;\nif ($date_source =~ /^@([0-9]+)$/) {{\n    my $date_epoch = $1;\n    POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime($date_epoch)) . \"\\n\"\n}}\nelse {{\n    select((select(STDOUT), $| = 1)[0]);\n    print STDERR \"date: option requires an argument -- 'd'\\nTry 'date --help' for more information.\\n\";\n    q{{}};\n}}",
                source_expr
            )
        }
        [format_word, ..] => {
            if let Some(format) = simple_word_text(format_word) {
                format_date_expr(&format)
            } else {
                let format_expr = generator.word_to_perl(format_word);
                format!(
                    "my $date_now = $DATE_SNAPSHOT; my $date_format = {}; $date_format =~ s/^\\+//; require POSIX; POSIX::strftime($date_format, localtime($date_now)) . \"\\n\"",
                    format_expr
                )
            }
        }
    };

    format!("{}{}", prefix, body)
}

pub fn generate_date_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let body = generate_date_expression(generator, cmd);
    format!("my $date = do {{\n{}\n}};\nprint $date;\n", body)
}
