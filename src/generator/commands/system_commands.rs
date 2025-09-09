use crate::ast::*;
use crate::generator::Generator;

// Helper function to convert Word to bash string representation for system commands
pub fn word_to_bash_string_for_system(word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // If the literal is already properly quoted (starts and ends with same quote), use it as-is
            if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
                s.clone()
            }
            // Always quote literals that contain spaces, quotes, or special characters to ensure proper shell parsing
            else if s.contains(' ') || s.contains('"') || s.contains('\'') || s.contains(';') || s.contains('|') || s.contains('&') || s.contains('<') || s.contains('>') || s.contains('\\') || s.contains('$') {
                format!("'{}'", s.replace("'", "'\"'\"'"))
            } else {
                s.clone()
            }
        },
        Word::StringInterpolation(interp, _) => {
            // For string interpolation, we need to convert to a bash-compatible format
            // This is a simplified version - for complex cases we might need more work
            let mut result = String::new();
            for part in &interp.parts {
                match part {
                    StringPart::Literal(s) => result.push_str(s),
                    StringPart::Variable(var) => result.push_str(&format!("${}", var)),
                    _ => result.push_str("UNSUPPORTED_INTERPOLATION"),
                }
            }
            if result.contains(' ') || result.contains(';') {
                format!("'{}'", result.replace("'", "'\"'\"'"))
            } else {
                result
            }
        },
        _ => {
            // For other word types, convert to string and quote if needed
            let s = word.to_string();
            if s.contains(' ') || s.contains(';') {
                format!("'{}'", s.replace("'", "'\"'\"'"))
            } else {
                s
            }
        }
    }
}

pub fn generate_command_string_for_system_impl(generator: &mut Generator, cmd: &Command) -> String {
    match cmd {
        Command::Simple(simple_cmd) => {
            let args: Vec<String> = simple_cmd.args.iter()
                .map(|arg| word_to_bash_string_for_system(arg))
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
                            .map(|arg| word_to_bash_string_for_system(arg))
                            .collect();
                        if args.is_empty() {
                            Some(simple_cmd.name.to_string())
                        } else {
                            Some(format!("{} {}", simple_cmd.name, args.join(" ")))
                        }
                    } else {
                        None
                    }
                })
                .collect();
            commands.join(" | ")
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
        Command::Redirect(_redirect_cmd) => {
            // For RedirectCommand with process substitution, we can't generate a simple shell command
            // Instead, we should not be called for these commands
            eprintln!("WARNING: generate_command_string_for_system called with RedirectCommand");
            "echo 'RedirectCommand cannot be converted to shell command'".to_string()
        }
        _ => {
            // For complex commands that can't be converted to simple shell commands,
            // we should not be called. This indicates a design issue.
            eprintln!("WARNING: generate_command_string_for_system called with complex command: {:?}", cmd);
            "echo \"Complex command cannot be converted to shell command\"".to_string()
        }
    }
}
