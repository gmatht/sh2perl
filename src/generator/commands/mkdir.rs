use crate::ast::*;
use crate::generator::Generator;

pub fn generate_mkdir_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();

    // mkdir command syntax: mkdir [options] directory...
    let mut create_parents = false;
    let mut directories = Vec::new();

    // Parse mkdir options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            match arg_str.as_str() {
                "-p" | "--parents" => create_parents = true,
                _ => {
                    if !arg_str.starts_with('-') {
                        directories.push(generator.perl_string_literal(arg));
                    }
                }
            }
        } else {
            directories.push(generator.perl_string_literal(arg));
        }
    }

    if directories.is_empty() {
        output.push_str("croak \"mkdir: missing operand\\n\";\n");
    } else {
        output.push_str(&generator.indent());
        output.push_str("use File::Path qw(make_path);\n");
        if !generator.declared_locals.contains("err") {
            output.push_str(&generator.indent());
            output.push_str("my $err;\n");
            generator.declared_locals.insert("err".to_string());
        }

        for dir in &directories {
            if create_parents {
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( !-d {} ) {{\n", dir));
                generator.indent_level += 1;

                output.push_str(&generator.indent());
                output.push_str(&format!("make_path( {}, {{ error => \\$err }} );\n", dir));
                output.push_str(&generator.indent());
                output.push_str("if ( @{$err} ) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!(
                    "croak \"mkdir: cannot create directory {}: $err->[0]\\n\";\n",
                    dir
                ));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            } else {
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( mkdir {} ) {{\n", dir));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("else {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!(
                    "croak \"mkdir: cannot create directory {}: File exists\\n\";\n",
                    dir
                ));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
        }
    }

    output
}
