use crate::ast::*;
use crate::generator::Generator;

pub fn generate_gzip_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
) -> String {
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
                "-f" | "--force" => {}   // Force overwrite (handled by gzip)
                "-v" | "--verbose" => {} // Verbose output (handled by gzip)
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
            let bash_cmd = format!("echo \"${}\" | gunzip 2>/dev/null", input_var);
            output.push_str(&format!(
                "my $decompressed = do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or croak \"cmd failed: $!\"; local $/; my $_r = <$__fh>; close $__fh; $_r; }};\n",
                generator.perl_string_literal_no_interp(&crate::ast::Word::literal(bash_cmd.to_string())),
            ));
            output.push_str("if (defined $decompressed) {\n");
            output.push_str(&format!("{} = $decompressed;\n", input_var));
            output.push_str("} else {\n");
            output.push_str(&format!(
                "{} = \"gunzip: input not in gzip format\\n\";\n",
                input_var
            ));
            output.push_str("}\n");
        } else {
            output.push_str(&format!(
                "my $compressed = do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', \'echo \"${}\" | gzip | base64\') or croak \"cmd failed: $!\"; local $/; my $_r = <$__fh>; close $__fh; $_r; }};\n",
                input_var
            ));
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
                output.push_str(&format!(
                    "if ({}.gz =~ {}) {{\n",
                    file,
                    generator.format_regex_pattern(r"\\.gz$")
                ));
                let bash_cmd = format!("gunzip -c {}.gz", file);
                let bash_lit = generator.perl_string_literal_no_interp(
                    &crate::ast::Word::literal(bash_cmd.to_string()),
                );
                output.push_str(&format!(
                    "my $decompressed = do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or croak \"cmd failed: $!\"; local $/; my $_r = <$__fh>; close $__fh; $_r; }};\n",
                    bash_lit
                ));
                output.push_str("if (defined $decompressed) {\n");
                output.push_str(&format!("push @results, \"Decompressed: {}\";\n", file));
                output.push_str("} else {\n");
                output.push_str(&format!(
                    "push @results, \"Failed to decompress: {}\";\n",
                    file
                ));
                output.push_str("}\n");
                output.push_str("} else {\n");
                output.push_str(&format!(
                    "push @results, \"File not compressed: {}\";\n",
                    file
                ));
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
                output.push_str(&format!(
                    "my $result = do {{ open(my $__fh, \'-|\', \'bash\', \'-c\', {}) or croak \"cmd failed: $!\"; local $/; my $_r = <$__fh>; close $__fh; $_r; }};\n",
                    generator.perl_string_literal_no_interp(&crate::ast::Word::literal(gzip_cmd.to_string()))
                ));
                output.push_str("if ( $CHILD_ERROR == 0 ) {\n");
                output.push_str(&format!("push @results, \"Compressed: {}\";\n", file));
                output.push_str("} else {\n");
                output.push_str(&format!(
                    "push @results, \"Failed to compress: {}\";\n",
                    file
                ));
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
