use crate::ast::*;
use crate::generator::Generator;

pub fn generate_mv_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // mv command syntax: mv [options] source... destination
    let mut _force = false;
    let mut sources = Vec::new();
    
    // Parse mv options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            match arg_str.as_str() {
                "-f" | "--force" => _force = true,
                _ => {
                    if !arg_str.starts_with('-') {
                        sources.push(generator.word_to_perl(arg));
                    }
                }
            }
        } else {
            sources.push(generator.word_to_perl(arg));
        }
    }
    
    if sources.len() < 2 {
        output.push_str("die \"mv: missing file operand\\n\";\n");
    } else {
        // Last argument is the destination
        let destination = sources.pop().unwrap();
        let quoted_destination = if destination.starts_with('"') || destination.starts_with("'") {
            destination.clone()
        } else {
            format!("\"{}\"", destination)
        };
        
        if !generator.declared_locals.contains("err") {
            output.push_str(&generator.indent());
            output.push_str("my $err;\n");
            generator.declared_locals.insert("err".to_string());
        }
        if !generator.declared_locals.contains("force") {
            output.push_str(&generator.indent());
            output.push_str(&format!("my $force = {};\n", if _force { "1" } else { "0" }));
            generator.declared_locals.insert("force".to_string());
        }
        
        for source in &sources {
            let quoted_source = if source.starts_with('"') || source.starts_with("'") {
                source.clone()
            } else {
                format!("\"{}\"", source)
            };
            output.push_str(&generator.indent());
            output.push_str(&format!("if (-e {}) {{\n", quoted_source));
            generator.indent_level += 1;
            
            // Check if destination exists and is a directory
            output.push_str(&generator.indent());
            output.push_str(&format!("my $dest = {};\n", quoted_destination));
            output.push_str(&generator.indent());
            output.push_str("if (-e $dest && -d $dest) {\n");
            generator.indent_level += 1;
            // Destination is a directory, append source name
            output.push_str(&generator.indent());
            output.push_str(&format!("$dest = \"$dest/{}\";\n", source));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            
            // Check if destination already exists
            output.push_str(&generator.indent());
            output.push_str("if (-e $dest && !$force) {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("die \"mv: $dest: File exists (use -f to force overwrite)\\n\";\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            
            // Create destination directory if it doesn't exist
            output.push_str(&generator.indent());
            output.push_str("my $dest_dir = $dest;\n");
            output.push_str(&generator.indent());
            output.push_str("$dest_dir =~ s/\\/[^\\/]*$//msx;\n");
            output.push_str(&generator.indent());
            output.push_str("if ($dest_dir eq $dest) {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("$dest_dir = q{};\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output.push_str(&generator.indent());
            output.push_str("if ($dest_dir ne q{} && !-d $dest_dir) {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("my $err;\n");
            output.push_str(&generator.indent());
            output.push_str("make_path($dest_dir, {error => \\$err});\n");
            output.push_str(&generator.indent());
            output.push_str("if (@{$err}) {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("die \"mv: cannot create directory $dest_dir: $err->[0]\\n\";\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            
            // Perform the move
            output.push_str(&generator.indent());
            output.push_str(&format!("if (move({}, $dest)) {{\n", quoted_source));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("print \"mv: moved {} to $dest\\n\";\n", source));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("} else {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("die \"mv: cannot move {} to $dest: $ERRNO\\n\";\n", source));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("} else {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("die \"mv: {}: No such file or directory\\n\";\n", source));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
    }
    
    output
}
