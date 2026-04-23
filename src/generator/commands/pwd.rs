use crate::ast::*;
use crate::generator::Generator;

pub fn generate_pwd_command(_generator: &mut Generator, _cmd: &SimpleCommand) -> String {
    let mut output = String::new();

    // pwd command - get current working directory
    output.push_str("use Cwd;\n");
    output.push_str("my $pwd = getcwd();\n");
    // Return the path string (including a trailing newline) as the
    // expression value rather than printing it directly. The purify
    // wrapper captures the inner block's return value and prints it if
    // non-empty; emitting the path as a string preserves semantics and
    // avoids printing the numeric return value of print (which is 1).
    output.push_str("$pwd . \"\\n\";\n");

    output
}
