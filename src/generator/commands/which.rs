use crate::ast::*;
use crate::generator::Generator;

pub fn generate_which_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let command = Command::Simple(cmd.clone());
    let command_str = generator.generate_command_string_for_system(&command);
    let command_lit = generator.perl_string_literal_no_interp(&Word::literal(command_str));

    format!(
        "my $which_cmd = {};\nmy $which_output = qx{{$which_cmd}};\nprint $which_output;\n$CHILD_ERROR = $? >> 8;\n",
        command_lit
    )
}
