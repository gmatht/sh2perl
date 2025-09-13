use crate::ast::*;
use crate::generator::Generator;

pub fn generate_seq_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // seq command syntax: seq [first] [increment] last
    if cmd.args.is_empty() {
        output.push_str("print \"1\\n\";\n");
    } else if cmd.args.len() == 1 {
        // seq last
        let last_str = generator.word_to_perl(&cmd.args[0]);
        output.push_str(&format!("my $last = {};\n", last_str));
        output.push_str("for my $i (1..$last) {\n");
        output.push_str("    print \"$i\\n\";\n");
        output.push_str("}\n");
    } else if cmd.args.len() == 2 {
        // seq first last
        let first_str = generator.word_to_perl(&cmd.args[0]);
        let last_str = generator.word_to_perl(&cmd.args[1]);
        output.push_str(&format!("my $first = {};\n", first_str));
        output.push_str(&format!("my $last = {};\n", last_str));
        output.push_str("for my $i ($first..$last) {\n");
        output.push_str("    print \"$i\\n\";\n");
        output.push_str("}\n");
    } else if cmd.args.len() == 3 {
        // seq first increment last
        let first_str = generator.word_to_perl(&cmd.args[0]);
        let increment_str = generator.word_to_perl(&cmd.args[1]);
        let last_str = generator.word_to_perl(&cmd.args[2]);
        output.push_str(&format!("my $first = {};\n", first_str));
        output.push_str(&format!("my $increment = {};\n", increment_str));
        output.push_str(&format!("my $last = {};\n", last_str));
        output.push_str("for (my $i = $first; $i <= $last; $i += $increment) {\n");
        output.push_str("    print \"$i\\n\";\n");
        output.push_str("}\n");
    } else {
        output.push_str("carp \"seq: too many arguments\";\n");
        output.push_str("exit 1;\n");
    }
    
    output
}
