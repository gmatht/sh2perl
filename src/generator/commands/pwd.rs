use crate::ast::*;
use crate::generator::Generator;

pub fn generate_pwd_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // pwd command - get current working directory
    output.push_str("use Cwd;\n");
    output.push_str("my $pwd = getcwd();\n");
    output.push_str("print $pwd;\n");
    
    output
}
