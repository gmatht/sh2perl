use crate::ast::*;
use crate::generator::Generator;

fn date_arg_expr(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(value, _) => {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
        _ => {
            let rendered = generator.word_to_perl(word);
            let escaped = rendered.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
    }
}

fn date_command_expr(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut expr = "\"date\"".to_string();

    for arg in &cmd.args {
        expr.push_str(" . ' ' . ");
        expr.push_str(&date_arg_expr(generator, arg));
    }

    expr
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

    format!(
        "{}my $date_cmd = {}; qx{{$date_cmd}}",
        prefix,
        date_command_expr(generator, cmd)
    )
}

pub fn generate_date_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let body = generate_date_expression(generator, cmd);
    format!("my $date = do {{\n{}\n}};\nprint $date;\n", body)
}
