use crate::ast::*;
use crate::generator::Generator;

pub fn generate_cp_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let command = Command::Simple(cmd.clone());
    let command_str = generator.generate_command_string_for_system(&command);
    let command_lit = generator.perl_string_literal(&Word::literal(command_str));

    format!(
        "do {{\n    my $cp_cmd = {};\n    my $cp_output = qx{{$cp_cmd}};\n    print $cp_output;\n    $cp_output;\n}};\n",
        command_lit
    )
}
