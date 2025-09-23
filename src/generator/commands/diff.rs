use crate::ast::*;
use crate::generator::Generator;

pub fn generate_diff_command(generator: &mut Generator, cmd: &SimpleCommand, _input_var: &str, _command_index: usize, _is_final_command: bool) -> String {
    let mut output = String::new();
    
    // Always use diff.exe instead of built-in implementation
    output.push_str(&generator.indent());
    output.push_str("my $diff_exit_code = 0;\n");
    output.push_str(&generator.indent());
    output.push_str("my $diff_output = q{};\n");
    
    // Build the diff command arguments
    let mut args = Vec::new();
    for arg in &cmd.args {
        let arg_str = generator.word_to_perl(arg);
        args.push(arg_str);
    }
    
    if !args.is_empty() {
        output.push_str(&generator.indent());
        output.push_str("{\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("local $INPUT_RECORD_SEPARATOR = undef;  # Read entire input at once\n");
        output.push_str(&generator.indent());
        output.push_str(&format!("open my $pipe, '-|', 'diff.exe', {} or croak \"Cannot open diff pipe: $OS_ERROR\";\n", 
            args.iter().map(|arg| format!("\"{}\"", arg)).collect::<Vec<_>>().join(", ")));
        output.push_str(&generator.indent());
        output.push_str("$diff_output = <$pipe>;\n");
        output.push_str(&generator.indent());
        output.push_str("close $pipe or croak \"Close failed: $OS_ERROR\";\n");
        output.push_str(&generator.indent());
        output.push_str("$diff_exit_code = $? >> 8;\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else {
        output.push_str(&generator.indent());
        output.push_str("$diff_output = q{};\n");
    }
    
    // For command substitution, we need to return the output
    output.push_str(&generator.indent());
    output.push_str("$diff_output");
    
    output
}
