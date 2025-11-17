use crate::ast::*;
use crate::generator::Generator;

pub fn generate_cp_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // cp command syntax: cp [options] source... destination
    let mut recursive = false;
    let mut preserve = false;
    let mut sources = Vec::new();
    
    // Parse cp options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            match arg_str.as_str() {
                "-r" | "-R" | "--recursive" => recursive = true,
                "-p" | "--preserve" => preserve = true,
                _ => {
                    if !arg_str.starts_with('-') {
                        sources.push(generator.perl_string_literal(arg));
                    }
                }
            }
        } else {
            sources.push(generator.perl_string_literal(arg));
        }
    }
    
    if sources.len() < 2 {
        output.push_str("die \"cp: missing file operand\\n\";\n");
    } else {
        // Last argument is the destination
        let destination = sources.pop().unwrap();
        
        if !generator.declared_locals.contains("err") {
            output.push_str(&generator.indent());
            output.push_str("my $err;\n");
            generator.declared_locals.insert("err".to_string());
        }
        
        for source in &sources {
            output.push_str(&generator.indent());
            output.push_str(&format!("if ( -e {} ) {{\n", source));
            generator.indent_level += 1;
            
            if recursive && format!("-d {}", source).contains("-d") {
                // Recursive copy for directories
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( -d {} ) {{\n", source));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("my $dest_dir = {};\n", destination));
                output.push_str(&generator.indent());
                output.push_str("if ( -e $dest_dir && !-d $dest_dir ) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("die \"cp: $dest_dir: not a directory\\n\";\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str("if ( !-d $dest_dir ) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("my $err;\n");
                output.push_str(&generator.indent());
                output.push_str("make_path($dest_dir, {error => \\$err});\n");
                output.push_str(&generator.indent());
                output.push_str("if (@{$err}) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("die \"cp: cannot create directory $dest_dir: $err->[0]\\n\";\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str(&format!("my $cmd = \"cp -r {} $dest_dir\";\n", source));
                output.push_str(&generator.indent());
                output.push_str("my $result = system $cmd;\n");
                output.push_str(&generator.indent());
                output.push_str("if ($result == 0) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("print \"cp: copied directory {} to $dest_dir\\n\";\n", source));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("} else {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("die \"cp: failed to copy directory {}\\n\";\n", source));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("} else {\n");
                generator.indent_level += 1;
                // Copy single file
                output.push_str(&generator.indent());
                output.push_str(&format!("my $dest = {};\n", destination));
                output.push_str(&generator.indent());
                output.push_str("if ( -d $dest ) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("$dest = \"$dest/{}\";\n", source));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( copy( {}, $dest ) ) {{\n", source));
                generator.indent_level += 1;
                if preserve {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("my ($atime, $mtime) = (stat({}))[8,9];\n", source));
                    output.push_str(&generator.indent());
                    output.push_str("utime $atime, $mtime, $dest;\n");
                }
                // Empty if block - perltidy wants blank line
                output.push_str("\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str("else {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("die \"cp: cannot copy {} to $dest: $ERRNO\\n\";\n", source));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            } else {
                // Copy single file
                output.push_str(&generator.indent());
                output.push_str(&format!("my $dest = {};\n", destination));
                output.push_str(&generator.indent());
                output.push_str("if ( -d $dest ) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("$dest = \"$dest/{}\";\n", source));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( copy( {}, $dest ) ) {{\n", source));
                generator.indent_level += 1;
                if preserve {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("my ($atime, $mtime) = (stat({}))[8,9];\n", source));
                    output.push_str(&generator.indent());
                    output.push_str("utime $atime, $mtime, $dest;\n");
                }
                // Empty if block - perltidy wants blank line
                output.push_str("\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str("else {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("die \"cp: cannot copy {} to $dest: $ERRNO\\n\";\n", source));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
            
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output.push_str(&generator.indent());
            output.push_str("else {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("die \"cp: {}: No such file or directory\\n\";\n", source));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
    }
    
    output
}
