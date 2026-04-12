use crate::ast::*;
use crate::generator::Generator;

pub fn generate_dirname_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
) -> String {
    let mut output = String::new();

    if !cmd.args.is_empty() {
        let command = Command::Simple(cmd.clone());
        let command_str = generator.generate_command_string_for_system(&command);
        let command_lit = generator.perl_string_literal(&Word::literal(command_str));

        output.push_str(&format!("my $dirname_cmd = {};\n", command_lit));
        output.push_str("my $dirname_output = qx{$dirname_cmd};\n");
        output.push_str("$CHILD_ERROR = $? >> 8;\n");
        if input_var.is_empty() {
            output.push_str("print $dirname_output;\n");
        } else {
            output.push_str(&format!("${} = $dirname_output;\n", input_var));
        }
    } else {
        // Default to current directory
        if input_var.is_empty() {
            output.push_str("print q{.}, \"\\n\";\n");
        } else {
            output.push_str(&format!("${} = q{{.}};\n", input_var));
        }
    }
    output.push_str("\n");

    output
}
