use crate::ast::*;
use crate::generator::Generator;

pub fn generate_which_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    // For each argument, generate a PATH search that finds the executable.
    // `which cmd1 cmd2` prints the full path of each command found.
    let mut results = Vec::new();
    for arg in &cmd.args {
        let arg_perl = generator.word_to_perl(arg);
        results.push(format!(
            "do {{ my $__prog = {}; my $__r = q{{}}; for my $__d (split /:/, $ENV{{PATH}} // q{{}}) {{ my $__f = \"$__d/$__prog\"; if (-x $__f) {{ $__r = $__f; last }} }} $__r; }}",
            arg_perl
        ));
    }
    let joined = if results.len() == 1 {
        results[0].clone()
    } else {
        format!("join(\"\\n\", {})", results.join(", "))
    };
    format!("print {}, \"\\n\";\n$CHILD_ERROR = 0;\n", joined)
}
