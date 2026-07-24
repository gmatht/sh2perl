use crate::ast::*;
use crate::generator::Generator;

pub fn generate_hostname_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();

    if cmd.args.is_empty() {
        // Just print hostname
        output.push_str(&generator.indent());
        output.push_str("do {\n");
        output.push_str(&generator.indent());
        output.push_str("    use Sys::Hostname;\n");
        output.push_str(&generator.indent());
        output.push_str("    print hostname() . \"\\n\";\n");
        output.push_str(&generator.indent());
        output.push_str("    $CHILD_ERROR = 0;\n");
        output.push_str(&generator.indent());
        output.push_str("};\n");
    } else {
        // Set hostname: hostname newname
        // In Perl, use sethostname from Sys::Hostname::Long
        let newname = generator.perl_string_literal(&cmd.args[0]);
        output.push_str(&generator.indent());
        output.push_str("do {\n");
        output.push_str(&generator.indent());
        output.push_str("    use Sys::Hostname::Long;\n");
        output.push_str(&generator.indent());
        output.push_str(&format!("    sethostname({});\n", newname));
        output.push_str(&generator.indent());
        output.push_str("    $CHILD_ERROR = 0;\n");
        output.push_str(&generator.indent());
        output.push_str("};\n");
    }

    output
}
