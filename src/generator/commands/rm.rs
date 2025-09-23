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
                "f" => {
                    // Handle case where -rf is parsed as -r and f separately
                    if recursive {
                        force = true;
                    } else {
                        files.push(format!("\"{}\"", arg_str));
                    }
                },
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
                    output.push_str("my $err;\n");
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
                    output.push_str(&generator.indent());
                    output.push_str("local $CHILD_ERROR = 0;\n");
                    // Silent operation - no output unless error
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    generator.indent_level += 1;
                    if force {
                        output.push_str(&generator.indent());
                        output.push_str("local $CHILD_ERROR = 1;\n");
                        output.push_str(&generator.indent());
                        output.push_str("carp \"rm: carping: could not remove \", $file_to_remove,\n");
                        output.push_str("    \": $OS_ERROR\\n\";\n");
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str("local $CHILD_ERROR = 1;\n");
                        output.push_str(&generator.indent());
                        output.push_str("croak \"rm: cannot remove \", $file_to_remove,\n");
                        output.push_str("    \": $OS_ERROR\\n\";\n");
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
                        output.push_str("carp \"rm: carping: \", $file_to_remove,\n");
                        output.push_str("    \" is a directory (use -r to remove recursively)\\n\";\n");
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str("croak \"rm: \", $file_to_remove,\n");
                        output.push_str("    \" is a directory (use -r to remove recursively)\\n\";\n");
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
                        output.push_str("local $CHILD_ERROR = 1;\n");
                        output.push_str(&generator.indent());
                        output.push_str("carp \"rm: carping: could not remove \", $file_to_remove,\n");
                        output.push_str("    \": $OS_ERROR\\n\";\n");
                    } else {
                        output.push_str(&generator.indent());
                        output.push_str("local $CHILD_ERROR = 1;\n");
                        output.push_str(&generator.indent());
                        output.push_str("croak \"rm: cannot remove \", $file_to_remove,\n");
                        output.push_str("    \": $OS_ERROR\\n\";\n");
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
                    output.push_str("local $CHILD_ERROR = 0;\n");
                    output.push_str(&generator.indent());
                        output.push_str("carp \"rm: carping: \", $file_to_remove,\n");
                        output.push_str("    \": No such file or directory\\n\";\n");
                } else {
                    output.push_str(&generator.indent());
                    output.push_str("local $CHILD_ERROR = 1;\n");
                    output.push_str(&generator.indent());
                        output.push_str("croak \"rm: \", $file_to_remove,\n");
                        output.push_str("    \": No such file or directory\\n\";\n");
                }
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            } else {
                // For non-glob patterns, use the original logic
                let quoted_file = if file.starts_with('"') || file.starts_with("'") {
                    file.clone()
                } else {
                    format!("\"{}\"", file)
                };
                output.push_str(&format!("if (-e {}) {{\n", quoted_file));
                
                if recursive {
                    // Recursive removal
                    output.push_str(&format!("if (-d {}) {{\n", quoted_file));
                    output.push_str("my $err;\n");
                    output.push_str(&format!("remove_tree({}, {{error => \\$err}});\n", quoted_file));
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
                    output.push_str(&format!("if (unlink {}) {{\n", quoted_file));
                    // Silent operation - no output unless error
                    output.push_str("$main_exit_code = 0;\n");
                    output.push_str("} else {\n");
                    if force {
                        output.push_str(&format!("carp \"rm: carping: could not remove \", {},\n", file));
                        output.push_str("    \": $OS_ERROR\\n\";\n");
                    } else {
                        output.push_str(&format!("croak \"rm: cannot remove \", {},\n", file));
                        output.push_str("    \": $OS_ERROR\\n\";\n");
                    }
                    output.push_str("}\n");
                    output.push_str("}\n");
                } else {
                    // Non-recursive removal
                    output.push_str(&format!("if (-d {}) {{\n", quoted_file));
                    if force {
                        output.push_str(&format!("carp \"rm: carping: \", {},\n", file));
                        output.push_str("    \" is a directory (use -r to remove recursively)\\n\";\n");
                    } else {
                        output.push_str(&format!("croak \"rm: \", {},\n", file));
                        output.push_str("    \" is a directory (use -r to remove recursively)\\n\";\n");
                    }
                    output.push_str("} else {\n");
                    output.push_str(&format!("if (unlink {}) {{\n", quoted_file));
                    // Silent operation - no output unless error
                    output.push_str("$main_exit_code = 0;\n");
                    output.push_str("} else {\n");
                    if force {
                        output.push_str(&format!("carp \"rm: carping: could not remove \", {},\n", file));
                        output.push_str("    \": $OS_ERROR\\n\";\n");
                    } else {
                        output.push_str(&format!("croak \"rm: cannot remove \", {},\n", file));
                        output.push_str("    \": $OS_ERROR\\n\";\n");
                    }
                    output.push_str("}\n");
                    output.push_str("}\n");
                }
                
                output.push_str("} else {\n");
                if force {
                    output.push_str("local $CHILD_ERROR = 0;\n");
                    output.push_str(&format!("carp \"rm: carping: \", {},\n", file));
                    output.push_str("    \": No such file or directory\\n\";\n");
                } else {
                    output.push_str("local $CHILD_ERROR = 1;\n");
                    output.push_str(&format!("croak \"rm: \", {},\n", file));
                    output.push_str("    \": No such file or directory\\n\";\n");
                }
                output.push_str("}\n");
            }
        }
    }
    
    output
}
