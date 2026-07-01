use crate::ast::*;
use crate::generator::Generator;

pub fn generate_rmdir_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();

    // rmdir command syntax: rmdir [options] directory...
    let mut directories = Vec::new();

    // Parse rmdir arguments
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            if !arg_str.starts_with('-') {
                directories.push(generator.perl_string_literal(arg));
            }
            // TODO: Handle rmdir options like -p (parents) if needed
        } else {
            directories.push(generator.perl_string_literal(arg));
        }
    }

    if directories.is_empty() {
        output.push_str("carp \"rmdir: missing operand\\n\";\n");
        output.push_str("$CHILD_ERROR = 1;\n");
    } else {
        for dir in &directories {
            output.push_str(&format!("if ( -d {} ) {{\n", dir));
            output.push_str(&format!("    if ( rmdir {} ) {{\n", dir));
            output.push_str("    }\n");
            output.push_str("                else {\n");
            output.push_str(&format!(
                "        carp \"rmdir: failed to remove {}: $ERRNO\\n\";\n",
                dir
            ));
            output.push_str("        $CHILD_ERROR = 1;\n");
            output.push_str("    }\n");
            output.push_str("}\n");
            output.push_str("else {\n");
            output.push_str(&format!(
                "        carp \"rmdir: failed to remove {}: $ERRNO\\n\";\n",
                dir
            ));
            output.push_str("        $CHILD_ERROR = 1;\n");
            output.push_str("}\n");
        }
    }

    output
}
