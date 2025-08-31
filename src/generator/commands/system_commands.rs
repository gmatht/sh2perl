use crate::ast::*;
use crate::generator::Generator;

pub fn generate_command_string_for_system_impl(generator: &mut Generator, cmd: &Command) -> String {
    match cmd {
        Command::Simple(simple_cmd) => {
            let args: Vec<String> = simple_cmd.args.iter()
                .map(|arg| generator.word_to_perl(arg))
                .collect();
            if args.is_empty() {
                simple_cmd.name.to_string()
            } else {
                format!("{} {}", simple_cmd.name, args.join(" "))
            }
        }
        Command::Subshell(subshell_cmd) => {
            match &**subshell_cmd {
                Command::Simple(simple_cmd) => {
                    let args: Vec<String> = simple_cmd.args.iter()
                        .map(|arg| generator.word_to_perl(arg))
                        .collect();
                    if args.is_empty() {
                        simple_cmd.name.to_string()
                    } else {
                        format!("{} {}", simple_cmd.name, args.join(" "))
                    }
                }
                Command::Pipeline(pipeline) => {
                    let commands: Vec<String> = pipeline.commands.iter()
                        .filter_map(|cmd| {
                            if let Command::Simple(simple_cmd) = cmd {
                                let args: Vec<String> = simple_cmd.args.iter()
                                    .map(|arg| generator.word_to_perl(arg))
                                    .collect();
                                Some(format!("{} {}", simple_cmd.name, args.join(" ")))
                            } else {
                                None
                            }
                        })
                        .collect();
                    commands.join(" | ")
                }
                Command::For(for_loop) => {
                    // For loops need to be handled as Perl code, not system commands
                    generator.generate_for_loop(for_loop)
                }
                _ => {
                    // For complex commands, generate proper Perl code instead of debug representation
                    generator.generate_command(cmd)
                }
            }
        }
        Command::For(for_loop) => {
            // For loops need to be handled as Perl code, not system commands
            generator.generate_for_loop(for_loop)
        }
        Command::Redirect(redirect_cmd) => {
            // For RedirectCommand with process substitution, we can't generate a simple shell command
            // Instead, we should not be called for these commands
            eprintln!("WARNING: generate_command_string_for_system called with RedirectCommand");
            "echo 'RedirectCommand cannot be converted to shell command'".to_string()
        }
        _ => {
            // For other complex commands, generate proper Perl code instead of debug representation
            generator.generate_command(cmd)
        }
    }
}
