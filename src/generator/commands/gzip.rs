use crate::ast::*;
use crate::generator::Generator;

pub fn generate_gzip_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str) -> String {
    let mut output = String::new();
    
    // gzip command syntax: gzip [options] [file]
    let mut _compress_mode = true; // Default to compression
    let mut decompress_mode = false;
    let mut keep_original = false;
    let mut files = Vec::new();
    
    // Parse gzip options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            match arg_str.as_str() {
                "-d" | "--decompress" => {
                    decompress_mode = true;
                    _compress_mode = false;
                }
                "-k" | "--keep" => keep_original = true,
                "-f" | "--force" => {}, // Force overwrite (handled by gzip)
                "-v" | "--verbose" => {}, // Verbose output (handled by gzip)
                _ => {
                    if !arg_str.starts_with('-') {
                        files.push(generator.word_to_perl(arg));
                    }
                }
            }
        } else {
            files.push(generator.word_to_perl(arg));
        }
    }
    
    if files.is_empty() {
        // No files specified, compress/decompress input
        if decompress_mode {
            let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
            output.push_str(&format!("my ({});
my {} = open3({}, {}, {}, 'bash', '-c', 'echo \"${}\" | gunzip 2>/dev/null');
close {} or croak 'Close failed: $!';
my $decompressed = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $!';
waitpid {}, 0;\n", in_var, pid_var, in_var, out_var, err_var, input_var, in_var, out_var, out_var, pid_var));
            output.push_str("if (defined $decompressed) {\n");
            output.push_str(&format!("{} = $decompressed;\n", input_var));
            output.push_str("} else {\n");
            output.push_str(&format!("{} = \"gunzip: input not in gzip format\\n\";\n", input_var));
            output.push_str("}\n");
        } else {
            let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
            output.push_str(&format!("my ({});
my {} = open3({}, {}, {}, 'bash', '-c', 'echo \"${}\" | gzip | base64');
close {} or croak 'Close failed: $!';
my $compressed = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $!';
waitpid {}, 0;\n", in_var, pid_var, in_var, out_var, err_var, input_var, in_var, out_var, out_var, pid_var));
            output.push_str("chomp $compressed;\n");
            output.push_str(&format!("{} = $compressed;\n", input_var));
        }
    } else {
        // Process specified files
        output.push_str("my @results;\n");
        for file in &files {
            if decompress_mode {
                // Decompress file
                output.push_str(&format!("if (-f {}) {{\n", file));
                output.push_str(&format!("if ({}.gz =~ {}) {{\n", file, generator.format_regex_pattern(r"\\.gz$")));
                let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
                output.push_str(&format!("my ({});
my {} = open3({}, {}, {}, 'gunzip', '-c', '{}.gz');
close {} or croak 'Close failed: $!';
my $decompressed = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $!';
waitpid {}, 0;\n", in_var, pid_var, in_var, out_var, err_var, file, in_var, out_var, out_var, pid_var));
                output.push_str("if (defined $decompressed) {\n");
                output.push_str(&format!("push @results, \"Decompressed: {}\";\n", file));
                output.push_str("} else {\n");
                output.push_str(&format!("push @results, \"Failed to decompress: {}\";\n", file));
                output.push_str("}\n");
                output.push_str("} else {\n");
                output.push_str(&format!("push @results, \"File not compressed: {}\";\n", file));
                output.push_str("}\n");
                output.push_str("} else {\n");
                output.push_str(&format!("push @results, \"File not found: {}\";\n", file));
                output.push_str("}\n");
            } else {
                // Compress file
                output.push_str(&format!("if (-f {}) {{\n", file));
                let gzip_cmd = if keep_original {
                    format!("gzip -k {}", file)
                } else {
                    format!("gzip {}", file)
                };
                let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
                output.push_str(&format!("my ({});
my {} = open3({}, {}, {}, 'bash', '-c', '{}');
close {} or croak 'Close failed: $!';
my $result = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $!';
waitpid {}, 0;\n", in_var, pid_var, in_var, out_var, err_var, gzip_cmd, in_var, out_var, out_var, pid_var));
                output.push_str("if ($CHILD_ERROR == 0) {\n");
                output.push_str(&format!("push @results, \"Compressed: {}\";\n", file));
                output.push_str("} else {\n");
                output.push_str(&format!("push @results, \"Failed to compress: {}\";\n", file));
                output.push_str("}\n");
                output.push_str("} else {\n");
                output.push_str(&format!("push @results, \"File not found: {}\";\n", file));
                output.push_str("}\n");
            }
        }
        output.push_str(&format!("{} = join \"\\n\", @results;\n", input_var));
    }
    output.push_str("\n");
    
    output
}
