use crate::ast::*;
use crate::generator::Generator;

pub fn generate_tr_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str, linebyline: bool) -> String {
    generate_tr_command_with_output(generator, cmd, input_var, command_index, linebyline, &format!("tr_result_{}", command_index))
}

pub fn generate_tr_command_for_substitution(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str) -> String {
    generate_tr_command_with_output_for_substitution(generator, cmd, input_var, command_index, &format!("tr_result_{}", command_index))
}

pub fn generate_tr_command_with_output(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str, linebyline: bool, output_var: &str) -> String {
    if linebyline {
        generate_tr_linebyline_impl_with_output(generator, cmd, input_var, command_index, output_var)
    } else {
        generate_tr_buffered_impl_with_output(generator, cmd, input_var, command_index, output_var)
    }
}

pub fn generate_tr_command_with_output_for_substitution(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str, output_var: &str) -> String {
    generate_tr_buffered_impl_with_output_for_substitution(generator, cmd, input_var, command_index, output_var)
}

fn generate_tr_linebyline_impl(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, _command_index: &str) -> String {
    generate_tr_linebyline_impl_with_output(generator, cmd, input_var, _command_index, input_var)
}

fn generate_tr_linebyline_impl_with_output(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, _command_index: &str, output_var: &str) -> String {
    let mut output = String::new();
    
    // tr command syntax: tr [OPTION]... SET1 [SET2]
    // Check for -d flag (delete characters)
    let mut delete_mode = false;
    let mut args = Vec::new();
    
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s == "-d" {
                delete_mode = true;
            } else {
                args.push(arg);
            }
        } else {
            args.push(arg);
        }
    }
    
    if delete_mode && args.len() >= 1 {
        // tr -d SET1: delete characters in SET1
        let set1 = generator.strip_shell_quotes_for_regex(&args[0]);
        
        // For line-by-line, process the line directly
        if input_var != output_var {
            output.push_str(&format!("${} = ${};\n", output_var, input_var));
        }
        output.push_str(&format!("${} =~ tr/{}/ /d;\n", output_var, set1));
    } else if args.len() >= 2 {
        // tr SET1 SET2: translate characters
        let set1 = generator.strip_shell_quotes_for_regex(&args[0]);
        let set2 = generator.strip_shell_quotes_for_regex(&args[1]);
        
        // For line-by-line, process the line directly
        if input_var != output_var {
            output.push_str(&format!("${} = ${};\n", output_var, input_var));
        }
        output.push_str(&format!("${} =~ tr/{}/{}/;\n", output_var, set1, set2));
    }
    // No valid arguments - line passes through unchanged
    
    output
}

fn generate_tr_buffered_impl(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str) -> String {
    generate_tr_buffered_impl_with_output(generator, cmd, input_var, command_index, &format!("tr_result_{}", command_index))
}

fn generate_tr_buffered_impl_with_output(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, _command_index: &str, output_var: &str) -> String {
    let mut output = String::new();
    
    // tr command syntax: tr [OPTION]... SET1 [SET2]
    // Check for -d flag (delete characters)
    let mut delete_mode = false;
    let mut args = Vec::new();
    
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s == "-d" {
                delete_mode = true;
            } else {
                args.push(arg);
            }
        } else {
            args.push(arg);
        }
    }
    
    if delete_mode && args.len() >= 1 {
        // tr -d SET1: delete characters in SET1
        let set1 = generator.strip_shell_quotes_and_convert_to_perl(&args[0]);
        
        let unique_id = generator.get_unique_id();
        output.push_str(&format!("my $set1_{} = {};\n", unique_id, set1));
        output.push_str(&format!("my $input_{} = ${};\n", unique_id, input_var));
        
        // Delete characters in SET1 from input
        output.push_str(&format!("my ${} = q{{}};\n", output_var));
        output.push_str(&format!("for my $char ( split //msx, $input_{} ) {{\n", unique_id));
        output.push_str(&format!("    if ( (index $set1_{}, $char) == -1 ) {{\n", unique_id));
        output.push_str(&format!("        ${} .= $char;\n", output_var));
        output.push_str("    }\n");
        output.push_str("}\n");
        // Ensure output ends with newline to match shell behavior
        output.push_str(&generator.indent());
        output.push_str(&format!("if ( !( ${} =~ {} || ${} eq q{{}} ) ) {{\n", output_var, generator.newline_end_regex(), output_var));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("${} .= \"\\n\";\n", output_var));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else if args.len() >= 2 {
        // tr SET1 SET2: translate characters
        let set1 = generator.strip_shell_quotes_and_convert_to_perl(&args[0]);
        let set2 = generator.strip_shell_quotes_and_convert_to_perl(&args[1]);
        
        let unique_id = generator.get_unique_id();
        output.push_str(&format!("my $set1_{} = {};\n", unique_id, set1));
        output.push_str(&format!("my $set2_{} = {};\n", unique_id, set2));
        output.push_str(&format!("my $input_{} = ${};\n", unique_id, input_var));
        
        // Expand character ranges for tr command
        output.push_str(&format!("# Expand character ranges for tr command\n"));
        output.push_str(&format!("my $expanded_set1_{} = $set1_{};\n", unique_id, unique_id));
        output.push_str(&format!("my $expanded_set2_{} = $set2_{};\n", unique_id, unique_id));
        output.push_str(&format!("# Handle a-z range in set1\n"));
        output.push_str(&format!("if ($expanded_set1_{} =~ /a-z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set1_{} =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        output.push_str(&format!("# Handle A-Z range in set1\n"));
        output.push_str(&format!("if ($expanded_set1_{} =~ /A-Z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set1_{} =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        output.push_str(&format!("# Handle a-z range in set2\n"));
        output.push_str(&format!("if ($expanded_set2_{} =~ /a-z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set2_{} =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        output.push_str(&format!("# Handle A-Z range in set2\n"));
        output.push_str(&format!("if ($expanded_set2_{} =~ /A-Z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set2_{} =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        
        // Character-by-character translation
        output.push_str(&format!("my ${} = q{{}};\n", output_var));
        output.push_str(&format!("for my $char ( split //msx, $input_{} ) {{\n", unique_id));
        output.push_str(&format!("    my $pos_{} = index $expanded_set1_{}, $char;\n", unique_id, unique_id));
        output.push_str(&format!("    if ( $pos_{} >= 0 && $pos_{} < length $expanded_set2_{} ) {{\n", unique_id, unique_id, unique_id));
        output.push_str(&format!("        ${} .= substr $expanded_set2_{}, $pos_{}, 1;\n", output_var, unique_id, unique_id));
        output.push_str("    } else {\n");
        output.push_str(&format!("        ${} .= $char;\n", output_var));
        output.push_str("    }\n");
        output.push_str("}\n");
        // Ensure output ends with newline to match shell behavior (but not for empty input)
        output.push_str(&generator.indent());
        output.push_str(&format!("if ( !( ${} =~ {} || ${} eq q{{}} ) ) {{\n", output_var, generator.newline_end_regex(), output_var));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("${} .= \"\\n\";\n", output_var));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else {
        // No valid arguments, just pass through input
        output.push_str(&format!("${} = ${};\n", output_var, input_var));
    }
    
    output
}

fn generate_tr_buffered_impl_with_output_for_substitution(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, _command_index: &str, output_var: &str) -> String {
    let mut output = String::new();
    
    // tr command syntax: tr [OPTION]... SET1 [SET2]
    // Check for -d flag (delete characters)
    let mut delete_mode = false;
    let mut args = Vec::new();
    
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s == "-d" {
                delete_mode = true;
            } else {
                args.push(arg);
            }
        } else {
            args.push(arg);
        }
    }
    
    if delete_mode && args.len() >= 1 {
        // tr -d SET1: delete characters in SET1
        let set1 = generator.strip_shell_quotes_and_convert_to_perl(&args[0]);
        
        let unique_id = generator.get_unique_id();
        output.push_str(&format!("my $set1_{} = {};\n", unique_id, set1));
        output.push_str(&format!("my $input_{} = ${};\n", unique_id, input_var));
        
        // Delete characters in SET1 from input
        output.push_str(&format!("my ${} = q{{}};\n", output_var));
        output.push_str(&format!("for my $char ( split //msx, $input_{} ) {{\n", unique_id));
        output.push_str(&format!("    if ( (index $set1_{}, $char) == -1 ) {{\n", unique_id));
        output.push_str(&format!("        ${} .= $char;\n", output_var));
        output.push_str("    }\n");
        output.push_str("}\n");
        // For command substitution, don't add newline - let the calling command handle it
    } else if args.len() >= 2 {
        // tr SET1 SET2: translate characters
        let set1 = generator.strip_shell_quotes_and_convert_to_perl(&args[0]);
        let set2 = generator.strip_shell_quotes_and_convert_to_perl(&args[1]);
        
        let unique_id = generator.get_unique_id();
        output.push_str(&format!("my $set1_{} = {};\n", unique_id, set1));
        output.push_str(&format!("my $set2_{} = {};\n", unique_id, set2));
        output.push_str(&format!("my $input_{} = ${};\n", unique_id, input_var));
        
        // Expand character ranges for tr command
        output.push_str(&format!("# Expand character ranges for tr command\n"));
        output.push_str(&format!("my $expanded_set1_{} = $set1_{};\n", unique_id, unique_id));
        output.push_str(&format!("my $expanded_set2_{} = $set2_{};\n", unique_id, unique_id));
        output.push_str(&format!("# Handle a-z range in set1\n"));
        output.push_str(&format!("if ($expanded_set1_{} =~ /a-z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set1_{} =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        output.push_str(&format!("# Handle A-Z range in set1\n"));
        output.push_str(&format!("if ($expanded_set1_{} =~ /A-Z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set1_{} =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        output.push_str(&format!("# Handle a-z range in set2\n"));
        output.push_str(&format!("if ($expanded_set2_{} =~ /a-z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set2_{} =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        output.push_str(&format!("# Handle A-Z range in set2\n"));
        output.push_str(&format!("if ($expanded_set2_{} =~ /A-Z/msx) {{\n", unique_id));
        output.push_str(&format!("    $expanded_set2_{} =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;\n", unique_id));
        output.push_str(&format!("}}\n"));
        
        // Character-by-character translation
        output.push_str(&format!("my ${} = q{{}};\n", output_var));
        output.push_str(&format!("for my $char ( split //msx, $input_{} ) {{\n", unique_id));
        output.push_str(&format!("    my $pos_{} = index $expanded_set1_{}, $char;\n", unique_id, unique_id));
        output.push_str(&format!("    if ( $pos_{} >= 0 && $pos_{} < length $expanded_set2_{} ) {{\n", unique_id, unique_id, unique_id));
        output.push_str(&format!("        ${} .= substr $expanded_set2_{}, $pos_{}, 1;\n", output_var, unique_id, unique_id));
        output.push_str("    } else {\n");
        output.push_str(&format!("        ${} .= $char;\n", output_var));
        output.push_str("    }\n");
        output.push_str("}\n");
        // For command substitution, don't add newline - let the calling command handle it
    } else {
        // No valid arguments, just pass through input
        output.push_str(&format!("${} = ${};\n", output_var, input_var));
    }
    
    // Return the result for command substitution
    output.push_str(&format!("${}", output_var));
    
    output
}

