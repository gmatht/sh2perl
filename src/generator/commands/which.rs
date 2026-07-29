use crate::ast::*;
use crate::generator::Generator;

pub fn generate_which_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    // Build a qx{} call with arguments stored in separate variables
    // for clean interpolation.
    let mut arg_vars = String::new();
    let mut qx_body = String::from("$which_prog");
    for (i, arg) in cmd.args.iter().enumerate() {
        let arg_perl = generator.word_to_perl(arg);
        let var_name = format!("_wa{}", i);
        arg_vars.push_str(&format!("my ${} = {};\n", var_name, arg_perl));
        qx_body.push_str(&format!(" ${}", var_name));
    }

    // Native Perl which: search PATH for the executable
    format!(
        "{}my $_which_out = do {{ my $__r = q{{}}; for my $__d (split /:/, $ENV{{PATH}} // q{{}}) {{ my $__f = \"$__d/which\"; if (-x $__f) {{ $__r = $__f; last }} }} $__r; }};\nprint $_which_out;\n$CHILD_ERROR = 0;\n",
        arg_vars
    )
}
