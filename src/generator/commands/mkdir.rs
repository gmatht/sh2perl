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
        output.push_str("use File::Path qw(make_path);\n");
        if !generator.declared_locals.contains("err") {
            output.push_str("my $err;\n");
            generator.declared_locals.insert("err".to_string());
        }
        
        for dir in &directories {
            if create_parents {
                output.push_str(&format!("if (!-d {}) {{\n", dir));
                output.push_str(&format!("make_path({}, {{error => \\$err}});\n", dir));
                output.push_str("if (@{$err}) {\n");
                output.push_str(&format!("croak \"mkdir: cannot create directory {}: $err->[0]\\n\";\n", dir));
                output.push_str("} else {\n");
                output.push_str(&format!("print \"mkdir: created directory {}\\n\";\n", dir));
                output.push_str("}\n");
                output.push_str("} else {\n");
                // mkdir -p is silent when directory already exists (matches shell behavior)
                output.push_str("}\n");
            } else {
                output.push_str(&format!("if (!-d {}) {{\n", dir));
                output.push_str(&format!("if (mkdir {}) {{\n", dir));
                output.push_str(&format!("print \"mkdir: created directory {}\\n\";\n", dir));
                output.push_str("} else {\n");
                output.push_str(&format!("croak \"mkdir: cannot create directory {}: $ERRNO\\n\";\n", dir));
                output.push_str("}\n");
                output.push_str("} else {\n");
                // When directory exists, mkdir should output error to stderr and fail
                // This matches shell behavior
                output.push_str(&format!("print {{\\*STDERR}} \"mkdir: cannot create directory {}: File exists\\n\";\n", dir));
                output.push_str("local $CHILD_ERROR = 256;\n");
                output.push_str("}\n");
            }
        }
    }
    
    output
}
