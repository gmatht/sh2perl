use crate::ast::*;
use crate::generator::Generator;

/// Generate logical AND operation (left && right)
pub fn generate_logical_and(generator: &mut Generator, left: &Command, right: &Command) -> String {
    let mut output = String::new();
    
    // Generate: left && right
    output.push_str(&generator.indent());
    output.push_str("if (");
    
    // For RedirectCommand, we need to check exit code
    if let Command::Redirect(_) = left {
        // Generate the redirect command first, then check exit code
        output.push_str("do {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(left));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("} == 0");
    } else {
        output.push_str(&generator.generate_command(left));
    }
    
    output.push_str(") {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&generator.generate_command(right));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}

/// Generate logical OR operation (left || right)
pub fn generate_logical_or(generator: &mut Generator, left: &Command, right: &Command) -> String {
    let mut output = String::new();
    
    // Generate: left || right
    // OR operations should NEVER capture STDOUT - they're about conditional execution
    output.push_str(&generator.indent());
    
    // Check if left is a test expression
    if let Command::TestExpression(_) = left {
        // For test expressions, generate: if (!left) { right }
        output.push_str("if (!(");
        output.push_str(&generator.generate_command(left));
        output.push_str(")) {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else {
        // Execute left command and check exit code
        output.push_str(&generator.generate_command(left));
        
        // Execute right command if left command fails
        // For diff commands, check $diff_exit_code; for others, check $?
        let exit_code_var = if contains_diff_command(left) {
            "$diff_exit_code"
        } else {
            "$?"
        };
        
        output.push_str(&generator.indent());
        output.push_str(&format!("if ({} != 0) {{\n", exit_code_var));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&generator.generate_command(right));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }
    
    output
}

/// Check if a command is a diff command (for exit code handling)
fn contains_diff_command(cmd: &Command) -> bool {
    match cmd {
        Command::Simple(simple_cmd) => {
            if let Word::Literal(name) = &simple_cmd.name {
                name == "diff"
            } else {
                false
            }
        }
        Command::Redirect(redirect_cmd) => {
            contains_diff_command(&redirect_cmd.command)
        }
        _ => false,
    }
}
