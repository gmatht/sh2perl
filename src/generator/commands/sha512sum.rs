use crate::ast::*;
use crate::generator::Generator;

pub fn generate_sha512sum_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str) -> String {
    let mut output = String::new();
    
    
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
        // Check mode: verify checksums from input
        output.push_str(&format!("my @lines = split /\\n/msx, {};\n", input_var));
        output.push_str("my @results;\n");
        output.push_str("foreach my $line (@lines) {\n");
        output.push_str("chomp $line;\n");
        output.push_str(&format!("if ($line =~ {}) {{\n", generator.format_regex_pattern(r"^([a-f0-9]{128})\\s+(.+)$")));
        output.push_str("my ($expected_hash, $filename) = ($1, $2);\n");
        output.push_str("if (-f \"$filename\") {\n");
        output.push_str("my $actual_hash = sha512_hex(do { local $/; open my $fh, '<', $filename or die \"Cannot open $filename: $OS_ERROR\"; <$fh> });\n");
        output.push_str("if ($expected_hash eq $actual_hash) {\n");
        output.push_str("push @results, \"$filename: OK\";\n");
        output.push_str("} else {\n");
        output.push_str("push @results, \"$filename: FAILED\";\n");
        output.push_str("}\n");
        output.push_str("} else {\n");
        output.push_str("push @results, \"$filename: No such file\";\n");
        output.push_str("}\n");
        output.push_str("}\n");
        output.push_str("}\n");
        if input_var.is_empty() {
            output.push_str("join \"\\n\", @results");
        } else {
            output.push_str(&format!("{} = join \"\\n\", @results;\n", input_var));
        }
    } else if files.is_empty() {
        // No files specified, calculate hash of input
        output.push_str(&format!("{} = sha512_hex({});\n", input_var, input_var));
    } else {
        // Calculate hashes of specified files
        if input_var.is_empty() {
            // For command substitution, return the joined result directly
            output.push_str("do {\n");
            output.push_str("my @results;\n");
            for file in &files {
                output.push_str(&format!("if (-f \"{}\") {{\n", file));
                output.push_str(&format!("my $hash = sha512_hex(do {{ local $INPUT_RECORD_SEPARATOR = undef; open my $fh, '<', \"{}\" or croak \"Cannot open {}: $ERRNO\"; my $content = <$fh>; close $fh or croak \"Close failed: $ERRNO\"; $content }});\n", file, file));
                output.push_str(&format!("push @results, \"$hash  {}\";\n", file));
                output.push_str("} else {\n");
                output.push_str(&format!("push @results, \"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000  {}  FAILED open or read\";\n", file));
                output.push_str("}\n");
            }
            output.push_str("join \"\\n\", @results;\n");
            output.push_str("}");
        } else {
            output.push_str("my @results;\n");
            for file in &files {
                output.push_str(&format!("if (-f \"{}\") {{\n", file));
                output.push_str(&format!("my $hash = sha512_hex(do {{ local $INPUT_RECORD_SEPARATOR = undef; open my $fh, '<', \"{}\" or croak \"Cannot open {}: $ERRNO\"; my $content = <$fh>; close $fh or croak \"Close failed: $ERRNO\"; $content }});\n", file, file));
                output.push_str(&format!("push @results, \"$hash  {}\";\n", file));
                output.push_str("} else {\n");
                output.push_str(&format!("push @results, \"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000  {}  FAILED open or read\";\n", file));
                output.push_str("}\n");
            }
            output.push_str(&format!("{} = join \"\\n\", @results;\n", input_var));
        }
    }
    output.push_str("\n");
    
    output
}
