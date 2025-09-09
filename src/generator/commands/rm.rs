use crate::ast::*;
use crate::generator::Generator;

pub fn generate_rm_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // rm command syntax: rm [options] file...
    let mut recursive = false;
    let mut force = false;
    let mut files = Vec::new();
    
    // Parse rm options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            match arg_str.as_str() {
                "-r" | "-R" | "--recursive" => recursive = true,
                "-f" | "--force" => force = true,
                _ => {
                    if !arg_str.starts_with('-') {
                        files.push(format!("\"{}\"", arg_str));
                    }
                }
            }
        } else {
            files.push(generator.word_to_perl(arg));
        }
    }
    
    if files.is_empty() {
        output.push_str("croak \"rm: missing operand\\n\";\n");
    } else {
        output.push_str("use File::Path qw(remove_tree);\n");
        if !generator.declared_locals.contains("err") {
            output.push_str("my $err;\n");
            generator.declared_locals.insert("err".to_string());
        }
        
        // Process each file/pattern
        for file in &files {
            // Check if this is a glob pattern (contains * or ?)
            let is_glob = file.contains('*') || file.contains('?');
            
            if is_glob {
                // For glob patterns, expand them first
                output.push_str(&format!("my @files_to_remove = glob({});\n", file));
                output.push_str("foreach my $file_to_remove (@files_to_remove) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("if (-e $file_to_remove) {\n");
                generator.indent_level += 1;
                
                if recursive {
                    // Recursive removal
                    output.push_str(&generator.indent());
                    output.push_str("if (-d $file_to_remove) {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str("remove_tree($file_to_remove, {error => \\$err});\n");
                    output.push_str(&generator.indent());
                    output.push_str("if (@{$err}) {\n");
                    generator.indent_level += 1;
                    if force {
                        output.push_str(&generator.indent());
                        output.push_str("carp \"rm: carping: could not remove \", $file_to_remove, \": $err->[0]\\n\";\n");
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str("croak \"rm: cannot remove \", $file_to_remove, \": $err->[0]\\n\";\n");
                    }
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    // Silent operation - no output unless error
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    // File removal
                    output.push_str(&generator.indent());
                    output.push_str("if (unlink $file_to_remove) {\n");
                    // Silent operation - no output unless error
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    generator.indent_level += 1;
                    if force {
                        output.push_str(&generator.indent());
                        output.push_str("carp \"rm: carping: could not remove \", $file_to_remove, \": $ERRNO\\n\";\n");
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str("croak \"rm: cannot remove \", $file_to_remove, \": $ERRNO\\n\";\n");
                    }
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                } else {
                    // Non-recursive removal
                    output.push_str(&generator.indent());
                    output.push_str("if (-d $file_to_remove) {\n");
                    generator.indent_level += 1;
                    if force {
                        output.push_str(&generator.indent());
                        output.push_str("carp \"rm: carping: \", $file_to_remove, \" is a directory (use -r to remove recursively)\\n\";\n");
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str("croak \"rm: \", $file_to_remove, \" is a directory (use -r to remove recursively)\\n\";\n");
                    }
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str("if (unlink $file_to_remove) {\n");
                    // Silent operation - no output unless error
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    generator.indent_level += 1;
                    if force {
                        output.push_str(&generator.indent());
                        output.push_str("carp \"rm: carping: could not remove \", $file_to_remove, \": $ERRNO\\n\";\n");
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str("croak \"rm: cannot remove \", $file_to_remove, \": $ERRNO\\n\";\n");
                    }
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                }
                
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("} else {\n");
                generator.indent_level += 1;
                if force {
                    output.push_str(&generator.indent());
                    output.push_str("carp \"rm: carping: \", $file_to_remove, \": No such file or directory\\n\";\n");
                } else {
                    output.push_str(&generator.indent());
                    output.push_str("croak \"rm: \", $file_to_remove, \": No such file or directory\\n\";\n");
                }
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            } else {
                // For non-glob patterns, use the original logic
                output.push_str(&format!("if (-e {}) {{\n", file));
                
                if recursive {
                    // Recursive removal
                    output.push_str(&format!("if (-d {}) {{\n", file));
                    output.push_str(&format!("remove_tree({}, {{error => \\$err}});\n", file));
                    output.push_str("if (@{$err}) {\n");
                    if force {
                        output.push_str(&format!("carp \"rm: carping: could not remove \", {}, \": $err->[0]\\n\";\n", file));
                    } else {
                        output.push_str(&format!("croak \"rm: cannot remove \", {}, \": $err->[0]\\n\";\n", file));
                    }
                    output.push_str("} else {\n");
                    // Silent operation - no output unless error
                    output.push_str("$main_exit_code = 0;\n");
                    output.push_str("}\n");
                    output.push_str("} else {\n");
                    // File removal
                    output.push_str(&format!("if (unlink {}) {{\n", file));
                    // Silent operation - no output unless error
                    output.push_str("$main_exit_code = 0;\n");
                    output.push_str("} else {\n");
                    if force {
                        output.push_str(&format!("carp \"rm: carping: could not remove \", {}, \": $ERRNO\\n\";\n", file));
                    } else {
                        output.push_str(&format!("croak \"rm: cannot remove \", {}, \": $ERRNO\\n\";\n", file));
                    }
                    output.push_str("}\n");
                    output.push_str("}\n");
                } else {
                    // Non-recursive removal
                    output.push_str(&format!("if (-d {}) {{\n", file));
                    if force {
                        output.push_str(&format!("carp \"rm: carping: \", {}, \" is a directory (use -r to remove recursively)\\n\";\n", file));
                    } else {
                        output.push_str(&format!("croak \"rm: \", {}, \" is a directory (use -r to remove recursively)\\n\";\n", file));
                    }
                    output.push_str("} else {\n");
                    output.push_str(&format!("if (unlink {}) {{\n", file));
                    // Silent operation - no output unless error
                    output.push_str("$main_exit_code = 0;\n");
                    output.push_str("} else {\n");
                    if force {
                        output.push_str(&format!("carp \"rm: carping: could not remove \", {}, \": $ERRNO\\n\";\n", file));
                    } else {
                        output.push_str(&format!("croak \"rm: cannot remove \", {}, \": $ERRNO\\n\";\n", file));
                    }
                    output.push_str("}\n");
                    output.push_str("}\n");
                }
                
                output.push_str("} else {\n");
                if force {
                    output.push_str(&format!("carp \"rm: carping: \", {}, \": No such file or directory\\n\";\n", file));
                } else {
                    output.push_str(&format!("croak \"rm: \", {}, \": No such file or directory\\n\";\n", file));
                }
                output.push_str("}\n");
            }
        }
    }
    
    output
}
