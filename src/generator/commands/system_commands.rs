use crate::ast::*;
use crate::generator::Generator;

// Helper function to convert Word to bash string representation for system commands
pub fn word_to_bash_string_for_system(word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Preserve existing shell single quotes verbatim
            if s.starts_with('\'') && s.ends_with('\'') {
                return s.clone();
            }
            // If the literal was originally double-quoted, keep the original
            // double-quoted form so that inner-shell variable expansion still
            // occurs when the reconstructed command is executed under bash -c.
            if s.starts_with('"') && s.ends_with('"') {
                return s.clone();
            }
            if s.is_empty() {
                return "''".to_string();
            }
            // Keep common shell operator tokens verbatim so generated shell command
            // strings can contain real operators (like pipes) instead of quoted
            // literal arguments. This is important for reconstructing
            // list-form `system("cat", "file", "|", "grep", ... )`
            // into a proper shell pipeline string.
            else if s == "|" {
                s.clone()
            }
            // Always quote literals that contain spaces, quotes, or special characters to ensure proper shell parsing
            else if s.contains(' ')
                || s.contains('\n')
                || s.contains('\r')
                || s.contains('\t')
                || s.contains('"')
                || s.contains('\'')
                || s.contains(';')
                || s.contains('|')
                || s.contains('&')
                || s.contains('<')
                || s.contains('>')
                || s.contains('\\')
                // Treat shell glob metacharacters as requiring quoting so that
                // patterns like "*.txt" are preserved when reconstructing
                // shell commands for system()/qx{} conversion.
                || s.contains('*')
                || s.contains('?')
                || s.contains('[')
            {
                // Escape single quotes for safe embedding in single-quoted shell literals
                format!("'{}'", s.replace("'", "'\\''"))
            } else {
                s.clone()
            }
        }
        Word::StringInterpolation(interp, _) => {
            // Reconstruct interpolation parts. If interpolation contains any
            // variable or parameter expansion parts we must preserve double-quote
            // semantics so that the inner shell expands $VAR. In that case emit
            // a double-quoted fragment (leaving $-style tokens intact). If the
            // interpolation contains only literal parts, fall back to the
            // conservative quoting used for literals.
            let mut has_var = false;
            let mut result = String::new();
            for part in &interp.parts {
                match part {
                    StringPart::Literal(s) => result.push_str(s),
                    StringPart::Variable(var) => {
                        has_var = true;
                        result.push_str(&format!("${}", var))
                    }
                    StringPart::ParameterExpansion(pe) => {
                        has_var = true;
                        result.push_str(&format!("${{{}}}", pe.variable))
                    }
                    _ => {
                        // For other complex parts, mark as variable-like to
                        // be conservative and preserve expansion semantics
                        has_var = true;
                        result.push_str("$var");
                    }
                }
            }

            if result.is_empty() {
                return "''".to_string();
            }

            if has_var {
                // We must emit a double-quoted string so the inner shell will
                // perform expansions. Escape double-quotes and backslashes but
                // do NOT escape $ or ${} sequences.
                let escaped = result.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            } else if result.contains(' ')
                || result.contains('\n')
                || result.contains('\r')
                || result.contains('\t')
                || result.contains(';')
                || result.contains('*')
                || result.contains('?')
                || result.contains('[')
            {
                // No variables, but contains characters that need quoting - use single-quote
                format!("'{}'", result.replace("'", "'\\''"))
            } else {
                result
            }
        }
        Word::CommandSubstitution(cmd, _) => {
            // For command substitutions in system commands, we need to generate the actual bash command
            // This is a complex case - for now, generate a placeholder that won't break bash
            format!("\"$(echo 'command substitution not supported in system command context')\"")
        }
        _ => {
            // For other word types, convert to string and quote if needed
            let s = word.to_string();
            if s.contains(' ')
                || s.contains(';')
                || s.contains('*')
                || s.contains('?')
                || s.contains('[')
            {
                // Escape single quotes for safe embedding in single-quoted shell literals
                format!("'{}'", s.replace("'", "'\\''"))
            } else {
                s
            }
        }
    }
}

pub fn generate_command_string_for_system_impl(generator: &mut Generator, cmd: &Command) -> String {
    match cmd {
        Command::Simple(simple_cmd) => {
            let args: Vec<String> = simple_cmd
                .args
                .iter()
                .map(|arg| word_to_bash_string_for_system(arg))
                .collect();
            // Merge short-option fragments like ["-n", "r"] -> ["-nr"] to avoid
            // producing strings like "-n r" which would be interpreted as a filename
            // by many commands (e.g. sort). Conservative check: only merge when the
            // first arg is exactly two chars starting with '-' and the next is a
            // single ASCII alphabetic character.
            let mut merged_args: Vec<String> = Vec::with_capacity(args.len());
            let mut i = 0;
            while i < args.len() {
                if i + 1 < args.len() {
                    let a = &args[i];
                    let b = &args[i + 1];
                    if a.len() == 2 && a.starts_with('-') && b.len() == 1 {
                        let ch = b.chars().next().unwrap();
                        if ch.is_ascii_alphabetic() {
                            merged_args.push(format!("{}{}", a, b));
                            i += 2;
                            continue;
                        }
                    }
                }
                merged_args.push(args[i].clone());
                i += 1;
            }

            if merged_args.is_empty() {
                simple_cmd.name.to_string()
            } else {
                format!("{} {}", simple_cmd.name, merged_args.join(" "))
            }
        }
        Command::Pipeline(pipeline) => crate::generator::redirects::generate_bash_command_string(
            &Command::Pipeline(pipeline.clone()),
        ),
        Command::Subshell(subshell_cmd) => {
            match &**subshell_cmd {
                Command::Simple(simple_cmd) => {
                    let args: Vec<String> = simple_cmd
                        .args
                        .iter()
                        .map(|arg| generator.word_to_perl(arg))
                        .collect();
                    if args.is_empty() {
                        simple_cmd.name.to_string()
                    } else {
                        format!("{} {}", simple_cmd.name, args.join(" "))
                    }
                }
                Command::Pipeline(pipeline) => {
                    let commands: Vec<String> = pipeline
                        .commands
                        .iter()
                        .filter_map(|cmd| {
                            if let Command::Simple(simple_cmd) = cmd {
                                let args: Vec<String> = simple_cmd
                                    .args
                                    .iter()
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
                Command::Block(_block) => {
                    // Serialize a subshell block (multiple commands) into a bash string
                    // Delegate to the redirects helper which knows how to join inner
                    // commands with "; " so constructs like (cmd1; cmd2) round-trip.
                    crate::generator::redirects::generate_bash_command_string(&**subshell_cmd)
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
            eprintln!(
                "WARNING: generate_command_string_for_system called with complex command: {:?}",
                cmd
            );
            "echo \"Complex command cannot be converted to shell command\"".to_string()
        }
    }
}
