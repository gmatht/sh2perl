use crate::ast::*;
use crate::generator::Generator;

pub fn generate_dirname_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
) -> String {
    let mut output = String::new();

    if !cmd.args.is_empty() {
        let path_str = generator.word_to_perl(&cmd.args[0]);

        if !generator.declared_locals.contains("dirname_loaded_file_basename") {
            output.push_str("use File::Basename qw(dirname);\n");
            generator.declared_locals.insert("dirname_loaded_file_basename".to_string());
        }

        output.push_str(&format!("my $dirname_output = dirname({});\n", path_str));
        output.push_str("$CHILD_ERROR = 0;\n");
        if input_var.is_empty() {
            output.push_str(&format!("print $dirname_output, \"\\n\";\n"));
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
