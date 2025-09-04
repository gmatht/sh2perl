use crate::ast::*;
use crate::generator::Generator;

pub fn generate_subshell_impl(generator: &mut Generator, command: &Command) -> String {
    let mut output = String::new();
    
    // Generate subshell command with proper variable scoping
    // Save current variable state and create local copies for the subshell
    output.push_str(&generator.indent());
    output.push_str("do {\n");
    generator.indent_level += 1;
    
    // Create local copies of all declared variables to isolate subshell scope
    for var_name in &generator.declared_locals {
        output.push_str(&generator.indent());
        output.push_str(&format!("my ${} = ${} if defined ${};\n", var_name, var_name, var_name));
    }
    
    output.push_str(&generator.generate_command(command));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("};\n");
    
    output
}

pub fn generate_background_impl(generator: &mut Generator, command: &Command) -> String {
    let mut output = String::new();
    
    // Generate background command using Perl's fork() for true background execution
    output.push_str(&generator.indent());
    output.push_str("if (my $pid = fork()) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("# Parent process continues\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("} elsif (defined $pid) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("# Child process executes the background command\n");
    output.push_str(&generator.generate_command(command));
    output.push_str(&generator.indent());
    output.push_str("exit(0);\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("} else {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("die \"Cannot fork: $!\\n\";\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}
