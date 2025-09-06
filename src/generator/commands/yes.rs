use crate::ast::*;
use crate::generator::Generator;

pub fn generate_yes_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // yes command syntax: yes [string]
    let string_to_repeat = if let Some(arg) = cmd.args.first() {
        generator.perl_string_literal(arg)
    } else {
        "\"y\"".to_string()
    };
    
    output.push_str(&format!("my $string = {};\n", string_to_repeat));
    output.push_str("while (1) {\n");
    output.push_str("print \"$string\\n\";\n");
    output.push_str("}\n");
    
    output
}

pub fn generate_yes_command_with_context(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, output_var: &str, command_index: &str) -> String {
    let mut output = String::new();
    
    // yes command syntax: yes [string]
    let string_to_repeat = if let Some(arg) = cmd.args.first() {
        generator.perl_string_literal(arg)
    } else {
        "\"y\"".to_string()
    };
    
    if !output_var.is_empty() {
        // In pipeline context - generate a limited number of lines and assign to output_var
        // For yes command in pipeline, we need to generate a reasonable number of lines
        // that can be consumed by the next command (like head)
        output.push_str(&format!("my $string = {};\n", string_to_repeat));
        output.push_str(&format!("${} = q{{}};\n", output_var));
        output.push_str("for (my $i = 0; $i < 1000; $i++) {\n");
        output.push_str(&format!("    ${} .= \"$string\\n\";\n", output_var));
        output.push_str("}\n");
    } else {
        // Standalone yes command - infinite loop
        output.push_str(&format!("my $string = {};\n", string_to_repeat));
        output.push_str("while (1) {\n");
        output.push_str("print \"$string\\n\";\n");
        output.push_str("}\n");
    }
    
    output
}
