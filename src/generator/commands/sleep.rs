use crate::ast::*;
use crate::generator::Generator;

pub fn generate_sleep_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();
    
    // sleep command syntax: sleep [number][suffix] [number][suffix] ...
    // Supports seconds (default), minutes (m), hours (h), days (d)
    // Multiple arguments are summed together
    if !cmd.args.is_empty() {
        output.push_str("use Time::HiRes qw(sleep);\n");
        
        if cmd.args.len() == 1 {
            // Single argument - use directly
            let duration_str = generator.word_to_perl(&cmd.args[0]);
            output.push_str(&format!("sleep({});\n", duration_str));
        } else {
            // Multiple arguments - sum them together
            output.push_str("my $total_sleep = 0;\n");
            for arg in &cmd.args {
                let duration_str = generator.word_to_perl(arg);
                output.push_str(&format!("$total_sleep += {};\n", duration_str));
            }
            output.push_str("sleep($total_sleep);\n");
        }
    } else {
        // Default to 1 second if no argument provided
        output.push_str("use Time::HiRes qw(sleep);\n");
        output.push_str("sleep(1);\n");
    }
    
    output
}
