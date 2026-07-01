use crate::ast::*;
use crate::generator::Generator;

fn sleep_exec_block(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();

    if !cmd.args.is_empty() {
        if cmd.args.len() == 1 {
            let duration_str = generator.word_to_perl(&cmd.args[0]);
            output.push_str(&format!(
                "require Time::HiRes; Time::HiRes::sleep({});\n",
                duration_str
            ));
        } else {
            output.push_str("my $total_sleep = 0;\n");
            for arg in &cmd.args {
                let duration_str = generator.word_to_perl(arg);
                output.push_str(&format!("$total_sleep += {};\n", duration_str));
            }
            output.push_str("require Time::HiRes; Time::HiRes::sleep($total_sleep);\n");
        }
    } else {
        output.push_str("require Time::HiRes; Time::HiRes::sleep(1);\n");
    }

    output
}

pub fn generate_sleep_expression(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    format!(
        "do {{\n    {}    q{{}};\n}}",
        sleep_exec_block(generator, cmd)
    )
}

pub fn generate_sleep_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    sleep_exec_block(generator, cmd)
}
