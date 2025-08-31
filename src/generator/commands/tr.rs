use crate::ast::*;
use crate::generator::Generator;

pub fn generate_tr_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str, linebyline: bool) -> String {
    if linebyline {
        generate_tr_linebyline_impl(generator, cmd, input_var, command_index)
    } else {
        generate_tr_buffered_impl(generator, cmd, input_var, command_index)
    }
}

fn generate_tr_linebyline_impl(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str) -> String {
    let mut output = String::new();
    
    // tr command syntax: tr [OPTION]... SET1 [SET2]
    // Check for -d flag (delete characters)
    let mut delete_mode = false;
    let mut args = Vec::new();
    
    for arg in &cmd.args {
        if let Word::Literal(s) = arg {
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
        
        // For line-by-line, process the line directly
        output.push_str(&format!("${} =~ tr/{}/ /d;\n", input_var, set1));
    } else if args.len() >= 2 {
        // tr SET1 SET2: translate characters
        let set1 = generator.strip_shell_quotes_and_convert_to_perl(&args[0]);
        let set2 = generator.strip_shell_quotes_and_convert_to_perl(&args[1]);
        
        // For line-by-line, process the line directly
        output.push_str(&format!("${} =~ tr/{}/{}/;\n", input_var, set1, set2));
    }
    // No valid arguments - line passes through unchanged
    
    output
}

fn generate_tr_buffered_impl(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: &str) -> String {
    let mut output = String::new();
    
    // tr command syntax: tr [OPTION]... SET1 [SET2]
    // Check for -d flag (delete characters)
    let mut delete_mode = false;
    let mut args = Vec::new();
    
    for arg in &cmd.args {
        if let Word::Literal(s) = arg {
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
        
        output.push_str(&format!("my $set1 = {};\n", set1));
        output.push_str(&format!("my $input = ${};\n", input_var));
        
        // Delete characters in SET1 from input
        output.push_str(&format!("$tr_result_{} = '';\n", command_index));
        output.push_str("for my $char (split //, $input) {\n");
        output.push_str("    if (index($set1, $char) == -1) {\n");
        output.push_str(&format!("        $tr_result_{} .= $char;\n", command_index));
        output.push_str("    }\n");
        output.push_str("}\n");
        // Ensure output ends with newline to match shell behavior
        output.push_str(&format!("$tr_result_{} .= \"\\n\" unless $tr_result_{} =~ /\\n$/;\n", command_index, command_index));
    } else if args.len() >= 2 {
        // tr SET1 SET2: translate characters
        let set1 = generator.strip_shell_quotes_and_convert_to_perl(&args[0]);
        let set2 = generator.strip_shell_quotes_and_convert_to_perl(&args[1]);
        
        output.push_str(&format!("my $set1 = {};\n", set1));
        output.push_str(&format!("my $input = ${};\n", input_var));
        
        // Character-by-character translation
        output.push_str(&format!("$tr_result_{} = '';\n", command_index));
        output.push_str("for my $char (split //, $input) {\n");
        output.push_str("    my $pos = index($set1, $char);\n");
        output.push_str("    if ($pos >= 0 && $pos < length($set2)) {\n");
        output.push_str(&format!("        $tr_result_{} .= substr($set2, $pos, 1);\n", command_index));
        output.push_str("    } else {\n");
        output.push_str(&format!("        $tr_result_{} .= $char;\n", command_index));
        output.push_str("    }\n");
        output.push_str("}\n");
        // Ensure output ends with newline to match shell behavior
        output.push_str(&format!("$tr_result_{} .= \"\\n\" unless $tr_result_{} =~ /\\n$/;\n", command_index, command_index));
    } else {
        // No valid arguments, just pass through input
        output.push_str(&format!("${} = ${};\n", input_var, input_var));
    }
    
    output
}
