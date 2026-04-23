use crate::ast::*;
use crate::generator::Generator;

pub fn generate_sha512sum_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
) -> String {
    let mut output = String::new();
    // Normalize input_var into a Perl expression when used in emitted code.
    // The rest of the generator logic uses input_var.is_empty() to decide
    // behaviour, so keep that check. When inserting into Perl snippets we
    // need a valid Perl variable or expression; callers sometimes pass bare
    // identifiers like "output_0" (without a leading '$'). To be defensive
    // add a leading '$' only for simple identifier-like names.
    let input_expr = if input_var.is_empty() {
        String::new()
    } else if input_var.starts_with('$') {
        input_var.to_string()
    } else {
        // If the input_var looks like a simple identifier (alnum or underscore)
        // then prefix with '$'. Otherwise assume it's already an expression
        // (e.g. a do-block) and emit as-is.
        if input_var
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            format!("${}", input_var)
        } else {
            input_var.to_string()
        }
    };

    // sha512sum command syntax: sha512sum [options] [file]
    let mut check_mode = false;
    let mut files = Vec::new();

    // Parse sha512sum options
    for arg in &cmd.args {
        if let Word::Literal(arg_str, _) = arg {
            if arg_str == "-c" {
                check_mode = true;
            } else if !arg_str.starts_with('-') {
                files.push(generator.perl_string_literal(arg));
            }
        } else {
            files.push(generator.perl_string_literal(arg));
        }
    }

    if check_mode {
        // Check mode: verify checksums from either provided checksum files
        // (sha512sum -c file) or from the supplied input variable.

        if !files.is_empty() {
            // One or more checksum files were specified; read and verify each
            // If this is used in a command-substitution context (input_var is empty)
            // wrap the multi-statement verifier in a do { ... } block so it can
            // be used as a single expression.
            // Always emit an expression-valued do-block for the verifier so
            // callers can inline it safely. This avoids emitting top-level
            // assignments that can unbalance surrounding blocks when the
            // generator result is spliced into larger do { ... } expressions.
            output.push_str("do {\n    my @results;\n");

            for file in &files {
                // Unquote the filename for user-facing messages when possible
                let unquoted_file =
                    if file.starts_with('\'') && file.ends_with('\'') && file.len() > 2 {
                        &file[1..file.len() - 1]
                    } else {
                        file
                    };

                output.push_str(&format!("if ( -f {} ) {{\n", file));
                output.push_str(&format!(
                    "    open my $fh, '<', {} or croak \"Cannot open {}: $ERRNO\";\n",
                    file, file
                ));
                output.push_str("    my $file_content = do { local $/ = undef; <$fh> };\n");
                output.push_str("    close $fh or croak \"Close failed: $ERRNO\";\n");
                output.push_str("    my @lines = split /\\n/msx, $file_content;\n");
                output.push_str("    foreach my $line (@lines) {\n");
                output.push_str("        chomp $line;\n");
                output.push_str(&format!(
                    "        if ($line =~ {}) {{\n",
                    generator.format_regex_pattern(r"^([a-f0-9]{128})\s+(.+)$")
                ));
                output.push_str("        my ($expected_hash, $filename) = ($1, $2);\n");
                output.push_str("        if (-f \"$filename\") {\n");
                output.push_str("            my $actual_hash = sha512_hex(do { local $/; open my $fh, '<', $filename or die \"Cannot open $filename: $OS_ERROR\"; my $content = <$fh>; close $fh or die \"Close failed: $OS_ERROR\"; $content });\n");
                output.push_str("            if ($expected_hash eq $actual_hash) {\n");
                output.push_str("                push @results, \"$filename: OK\";\n");
                output.push_str("            } else {\n");
                output.push_str("                push @results, \"$filename: FAILED\";\n");
                output.push_str("            }\n");
                output.push_str("        } else {\n");
                output.push_str("            push @results, \"$filename: No such file\";\n");
                output.push_str("        }\n");
                // Close the outer regex match block (if ($line =~ ...))
                output.push_str("        }\n");
                output.push_str("    }\n");
                output.push_str("} else {\n");
                output.push_str(&format!(
                    "    push @results, \"{}: No such file\";\n",
                    unquoted_file
                ));
                output.push_str("}\n");
            }

            // Return the joined results as the expression value so callers can
            // decide whether to print or assign the result. Callers that
            // redirect STDOUT should explicitly print this value into the
            // redirected handle. Keeping this expression-valued contract
            // avoids breaking call-sites that expect a string result.
            output.push_str("    join(\"\\n\", @results) . \"\\n\";\n};");
        } else {
            // No checksum files specified; treat the input_var as the checksum content
            // When an input_var is provided we still emit an expression-valued
            // do-block that reads from that variable; callers will assign the
            // result if they want. This simplifies composition and avoids
            // leaking assignments into surrounding scopes.
            if input_var.is_empty() {
                // No input var: operate on implicit input (e.g., STDIN) when used
                // as a standalone command substitution.
                output.push_str("do {\n    my @lines = split /\\n/msx, do { local $/ = undef; <STDIN> };\n    my @results;\n");
            } else {
                // Read lines from the provided input variable and run verifier
                output.push_str(&format!(
                    "do {{\n    my @lines = split /\\n/msx, {};\n    my @results;\n",
                    input_expr
                ));
            }
            output.push_str("    foreach my $line (@lines) {\n        chomp $line;\n");
            output.push_str(&format!(
                "if ($line =~ {}) {{\n",
                generator.format_regex_pattern(r"^([a-f0-9]{128})\\s+(.+)$")
            ));
            output.push_str("my ($expected_hash, $filename) = ($1, $2);\n");
            output.push_str("if (-f \"$filename\") {\n");
            output.push_str("my $actual_hash = sha512_hex(do { local $/; open my $fh, '<', $filename or die \"Cannot open $filename: $OS_ERROR\"; my $content = <$fh>; close $fh or die \"Close failed: $OS_ERROR\"; $content });\n");
            output.push_str("if ($expected_hash eq $actual_hash) {\n");
            output.push_str("push @results, \"$filename: OK\";\n");
            output.push_str("} else {\n");
            output.push_str("push @results, \"$filename: FAILED\";\n");
            output.push_str("}\n");
            output.push_str("} else {\n");
            output.push_str("push @results, \"$filename: No such file\";\n");
            output.push_str("}\n");
            output.push_str("}\n");
            // Return the joined results as the expression value
            // When used as a command-substitution the result should include
            // a trailing newline. Keep the trailing-newline behaviour only
            // for expression-valued do-blocks which are used in qx/backtick
            // contexts (input_var == "").
            output.push_str("    join(\"\\n\", @results) . \"\\n\";\n};");
        }
    } else if files.is_empty() {
        // No files specified, calculate hash of input.
        // Emit an expression-valued snippet instead of assigning into the
        // caller's variable. This makes the generator safe to inline into
        // surrounding do { ... } expressions.
        if input_var.is_empty() {
            // No input var: read from STDIN and return the hash as an expression.
            // When sha512sum reads from stdin the external tool prints "<hash>  -".
            // Preserve that behavior by appending the literal "  -" so purified
            // scripts that print the result match the host tool's output.
            output.push_str("sha512_hex(do { local $/ = undef; <STDIN> }) . '  -'");
        } else {
            // Compute hash of the provided Perl variable and return it as an expression
            // When hashing a pipeline/stdin input, emulate the external tool's
            // printed form by appending the literal "  -".
            output.push_str(&format!("sha512_hex({}) . '  -'", input_expr));
        }
    } else {
        // Calculate hashes of specified files
        if input_var.is_empty() {
            // For command substitution, return the joined result directly
            output.push_str("do {\n");
            output.push_str("    my @results;\n");
            for file in &files {
                // Extract the unquoted filename for output
                let unquoted_file =
                    if file.starts_with("'") && file.ends_with("'") && file.len() > 2 {
                        &file[1..file.len() - 1]
                    } else {
                        file
                    };
                output.push_str(&format!("    if ( -f {} ) {{\n", file));
                output.push_str(&format!("        my $hash = sha512_hex(\n            do {{\n                local $INPUT_RECORD_SEPARATOR = undef;\n                open my $fh, '<', {}\n                  or croak \"Cannot open {}: $ERRNO\";\n                my $content = <$fh>;\n                close $fh\n                  or croak \"Close failed: $ERRNO\";\n                $content;\n            }}\n        );\n", file, file));
                output.push_str(&format!(
                    "        push @results, \"$hash  {}\";\n",
                    unquoted_file
                ));
                output.push_str("    }\n");
                output.push_str("    else {\n");
                output.push_str("        push @results,\n");
                output.push_str(&format!("\"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000  {}  FAILED open or read\";\n", unquoted_file));
                output.push_str("    }\n");
            }
            // For command-substitution contexts append a trailing newline so
            // the returned string matches qx/backticks which include final newlines.
            output.push_str("    join(\"\\n\", @results) . \"\\n\";\n");
            output.push_str("};");
        } else {
            // When an input_var is supplied, emit an expression-valued do-block
            // that computes and returns the joined results instead of
            // assigning into the caller's variable.
            output.push_str("do {\n    my @results;\n");
            for file in &files {
                // Extract the unquoted filename for output
                let unquoted_file =
                    if file.starts_with("'") && file.ends_with("'") && file.len() > 2 {
                        &file[1..file.len() - 1]
                    } else {
                        file
                    };
                output.push_str(&format!("    if ( -f {} ) {{\n", file));
                output.push_str(&format!("        my $hash = sha512_hex(\n            do {{\n                local $INPUT_RECORD_SEPARATOR = undef;\n                open my $fh, '<', {}\n                  or croak \"Cannot open {}: $ERRNO\";\n                my $content = <$fh>;\n                close $fh\n                  or croak \"Close failed: $ERRNO\";\n                $content;\n            }}\n        );\n", file, file));
                output.push_str(&format!(
                    "        push @results, \"$hash  {}\";\n",
                    unquoted_file
                ));
                output.push_str("    }\n");
                output.push_str("    else {\n");
                output.push_str("        push @results,\n");
                output.push_str(&format!("\"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000  {}  FAILED open or read\";\n", unquoted_file));
                output.push_str("    }\n");
            }
            output.push_str("    join(\"\\n\", @results) . \"\\n\";\n};");
        }
    }
    output.push_str("\n");

    output
}
