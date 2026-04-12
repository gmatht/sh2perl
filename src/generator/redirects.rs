use super::Generator;
use crate::ast::*;
use crate::generator::utils::get_temp_dir;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEMP_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn generate_redirect_impl(generator: &mut Generator, redirect: &Redirect) -> String {
    let mut output = String::new();

    match &redirect.operator {
        RedirectOperator::Input => {
            // Input redirection: command < file
            let target = generator.perl_string_literal(&redirect.target);
            output.push_str(&format!(
                "open STDIN, '<', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                target
            ));
        }
        RedirectOperator::Output => {
            // Output redirection: command > file
            // Note: This function doesn't have access to the command name, so it can't handle echo specially
            // The special handling is done in generate_simple_command
            let target = generator.perl_string_literal(&redirect.target);
            output.push_str(&format!(
                "open STDOUT, '>', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                target
            ));
        }
        RedirectOperator::Append => {
            // Append redirection: command >> file
            let target = generator.perl_string_literal(&redirect.target);
            output.push_str(&format!(
                "open STDOUT, '>>', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                target
            ));
        }
        RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => {
            // Heredoc: command << delimiter
            if let Some(body) = &redirect.heredoc_body {
                // Create a temporary file with the heredoc content
                // Use single quotes to prevent variable interpolation in the heredoc content
                let escaped_body = body.replace("'", "\\'");
                output.push_str(&format!("my $temp_content = '{}';\n", escaped_body));
                let fh = generator.get_unique_file_handle();
                output.push_str(&format!("use File::Path qw(make_path);\n"));
                let temp_dir = get_temp_dir();
                output.push_str(&format!(
                    "if (!-d {}) {{ make_path({}); }}\n",
                    temp_dir, temp_dir
                ));
                output.push_str(&format!("open my ${}, '>', {} . '/heredoc_temp' or croak \"Cannot create temp file: $OS_ERROR\\n\";\n", fh, temp_dir));
                output.push_str(&format!("print ${} $temp_content;\n", fh));
                output.push_str(&format!(
                    "close ${} or croak \"Close failed: $OS_ERROR\\n\";\n",
                    fh
                ));
                output.push_str(&format!("open STDIN, '<', {} . '/heredoc_temp' or croak \"Cannot open temp file: $OS_ERROR\\n\";\n", temp_dir));
            }
        }
        RedirectOperator::ProcessSubstitutionInput(cmd) => {
            let global_counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let _temp_file = format!("{}/process_sub_{}.tmp", get_temp_dir(), global_counter);
            let temp_var = format!("temp_file_ps_{}", global_counter);
            let output_var = format!("output_ps_{}", global_counter);
            let fh_var = format!("fh_ps_{}", global_counter);

            output.push_str(&format!(
                "my ${} = {} . '/process_sub_{}.tmp';\n",
                temp_var,
                get_temp_dir(),
                global_counter
            ));

            if let Command::Pipeline(_) = cmd.as_ref() {
                output.push_str(&format!("my ${};\n", output_var));
                output.push_str(&format!("{{\n"));
                output.push_str(&format!("    local *STDOUT;\n"));
                output.push_str(&format!(
                    "    open STDOUT, '>', \\${} or croak \"Cannot redirect STDOUT\";\n",
                    output_var
                ));
                output.push_str(&format!("    {{\n"));

                // Use the Perl generator instead of bash execution
                let perl_code = generator.generate_command(cmd);
                for line in perl_code.lines() {
                    if !line.trim().is_empty() {
                        output.push_str(&format!("    {}\n", line));
                    }
                }

                output.push_str(&format!("    }}\n"));
                output.push_str(&format!("}}\n"));
            } else {
                let cmd_str = generate_bash_command_string(cmd);
                let cmd_literal = generator.perl_string_literal(&Word::literal(cmd_str));
                output.push_str(&format!(
                    "my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', {});
close $in or croak 'Close failed: $OS_ERROR';
my ${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <$out> }};
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;\n",
                    output_var, cmd_literal
                ));
            }

            output.push_str(&format!("use File::Path qw(make_path);\n"));
            output.push_str(&format!(
                "my $temp_dir_{} = dirname(${});\n",
                global_counter, temp_var
            ));
            output.push_str(&format!(
                "if (!-d $temp_dir_{}) {{ make_path($temp_dir_{}); }}\n",
                global_counter, global_counter
            ));
            output.push_str(&format!(
                "open my ${}, '>', ${} or croak \"Cannot create temp file: $OS_ERROR\\n\";\n",
                fh_var, temp_var
            ));
            output.push_str(&format!("print ${} ${};\n", fh_var, output_var));
            output.push_str(&format!(
                "close ${} or croak \"Close failed: $OS_ERROR\\n\";\n",
                fh_var
            ));

            generator.process_sub_files.insert(
                format!("{} . '/process_sub_{}.tmp'", get_temp_dir(), global_counter),
                temp_var.clone(),
            );

            // Store the temp_var for use by commands that need it (like grep -f)
            generator.current_process_sub_file = Some(temp_var.clone());
        }
        RedirectOperator::ProcessSubstitutionOutput(_cmd) => {
            output.push_str("# Redirect ProcessSubstitutionOutput not yet implemented\n");
        }
        RedirectOperator::HereString => {
            // Here-strings are now handled in the command dispatcher
            // This case should not be reached
            output.push_str("# Here-string handling moved to command dispatcher\n");
        }
        RedirectOperator::StderrOutput => {
            // Stderr redirection: command 2> file
            let target = generator.perl_string_literal(&redirect.target);
            output.push_str(&format!(
                "open STDERR, '>', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                target
            ));
        }
        RedirectOperator::StderrAppend => {
            // Stderr append: command 2>> file
            let target = generator.perl_string_literal(&redirect.target);
            output.push_str(&format!(
                "open STDERR, '>>', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                target
            ));
        }
        RedirectOperator::StderrInput => {
            // Stderr input: command 2< file
            let target = generator.perl_string_literal(&redirect.target);
            output.push_str(&format!(
                "open STDERR, '<', {} or croak \"Cannot open file: $OS_ERROR\\n\";\n",
                target
            ));
        }
        _ => {
            // Other redirects not yet implemented
            output.push_str(&format!(
                "# Redirect {:?} not yet implemented\n",
                redirect.operator
            ));
        }
    }

    output
}

// Helper function to generate bash command strings for process substitution
pub fn generate_bash_command_string(cmd: &Command) -> String {
    match cmd {
        Command::Simple(simple_cmd) => {
            let args: Vec<String> = simple_cmd
                .args
                .iter()
                .map(|arg| word_to_bash_string(arg))
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
                .map(|cmd| generate_bash_command_string(cmd))
                .collect();

            let mut result = String::new();
            // Handle pipeline operators
            for (i, (command, _)) in commands.iter().zip(pipeline.commands.iter()).enumerate() {
                if i > 0 {
                    result.push_str(" | "); // Default to pipe for now
                }
                result.push_str(command);
            }
            result
        }
        Command::And(left, right) => {
            format!(
                "{} && {}",
                generate_bash_command_string(left),
                generate_bash_command_string(right)
            )
        }
        Command::Or(left, right) => {
            format!(
                "{} || {}",
                generate_bash_command_string(left),
                generate_bash_command_string(right)
            )
        }
        Command::Subshell(subshell_cmd) => {
            format!("({})", generate_bash_command_string(&**subshell_cmd))
        }
        Command::Redirect(redirect_cmd) => {
            // For redirects, we need to generate the base command and redirects
            let base_cmd = if let Command::Simple(cmd) = &*redirect_cmd.command {
                if cmd.name.to_string().is_empty() {
                    // Empty command with just redirects (like process substitution)
                    String::new()
                } else {
                    generate_bash_command_string(&*redirect_cmd.command)
                }
            } else {
                generate_bash_command_string(&*redirect_cmd.command)
            };

            let mut result = base_cmd;
            for redirect in &redirect_cmd.redirects {
                match &redirect.operator {
                    RedirectOperator::Input => {
                        result.push_str(&format!(" < {}", word_to_bash_string(&redirect.target)));
                    }
                    RedirectOperator::Output => {
                        result.push_str(&format!(" > {}", word_to_bash_string(&redirect.target)));
                    }
                    RedirectOperator::Append => {
                        result.push_str(&format!(" >> {}", word_to_bash_string(&redirect.target)));
                    }
                    RedirectOperator::ProcessSubstitutionInput(cmd) => {
                        result.push_str(&format!(" <({})", generate_bash_command_string(cmd)));
                    }
                    RedirectOperator::ProcessSubstitutionOutput(cmd) => {
                        result.push_str(&format!(" >({})", generate_bash_command_string(cmd)));
                    }
                    RedirectOperator::HereString => {
                        result.push_str(&format!(" <<< {}", word_to_bash_string(&redirect.target)));
                    }
                    RedirectOperator::StderrOutput => {
                        result.push_str(&format!(" 2> {}", word_to_bash_string(&redirect.target)));
                    }
                    RedirectOperator::StderrAppend => {
                        result.push_str(&format!(" 2>> {}", word_to_bash_string(&redirect.target)));
                    }
                    RedirectOperator::StderrInput => {
                        result.push_str(&format!(" 2< {}", word_to_bash_string(&redirect.target)));
                    }
                    _ => {
                        // Skip other redirect types for now
                    }
                }
            }
            result
        }
        _ => {
            // For other complex commands, generate a reasonable bash representation
            format!("echo 'Complex command not supported in bash string generation'")
        }
    }
}

// Helper function to convert Word to bash string representation
fn word_to_bash_string(word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // If the literal contains spaces or special characters, quote it
            if s.contains(' ')
                || s.contains('"')
                || s.contains('\'')
                || s.contains(';')
                || s.contains('|')
                || s.contains('&')
                || s.contains('<')
                || s.contains('>')
            {
                format!("'{}'", s.replace("'", "'\''"))
            } else {
                s.clone()
            }
        }
        Word::BraceExpansion(expansion, _) => {
            let mut result = String::new();
            if let Some(prefix) = &expansion.prefix {
                result.push_str(&prefix);
            }
            result.push('{');

            let items_str = expansion
                .items
                .iter()
                .map(|item| match item {
                    BraceItem::Literal(s) => s.clone(),
                    BraceItem::Range(range) => {
                        if let Some(ref step) = range.step {
                            format!("{}..{}..{}", range.start, range.end, step)
                        } else {
                            format!("{}..{}", range.start, range.end)
                        }
                    }
                    BraceItem::Sequence(items) => items.join(","),
                })
                .collect::<Vec<String>>()
                .join(",");
            result.push_str(&items_str);
            result.push('}');

            if let Some(suffix) = &expansion.suffix {
                result.push_str(&suffix);
            }
            result
        }
        Word::ParameterExpansion(param, _) => {
            format!("${{{}}}", param)
        }
        Word::StringInterpolation(parts, _) => {
            let mut result = String::new();
            for part in &parts.parts {
                match part {
                    StringPart::Literal(s) => result.push_str(&s),
                    StringPart::Variable(var) => result.push_str(&format!("${{{}}}", var)),
                    _ => result.push_str("$var"), // Placeholder for other types
                }
            }
            result
        }
        Word::CommandSubstitution(_cmd, _) => {
            // This would need to be handled by the caller
            format!("$({})", "command")
        }
        _ => format!("{:?}", word),
    }
}

pub fn generate_shopt_command_impl(generator: &mut Generator, cmd: &ShoptCommand) -> String {
    let mut output = String::new();

    // Handle shopt command for shell options
    match cmd.option.as_str() {
        "extglob" => {
            generator.extglob_enabled = cmd.enable;
            output.push_str(&format!(
                "# extglob option {}\n",
                if cmd.enable { "enabled" } else { "disabled" }
            ));
        }
        "nocasematch" => {
            generator.nocasematch_enabled = cmd.enable;
            output.push_str(&format!(
                "# nocasematch option {}\n",
                if cmd.enable { "enabled" } else { "disabled" }
            ));
        }
        _ => {
            output.push_str(&format!(
                "# shopt -{} {} not implemented\n",
                if cmd.enable { "s" } else { "u" },
                cmd.option
            ));
        }
    }

    // shopt commands always succeed (return true)
    output
}

pub fn generate_builtin_command_impl(generator: &mut Generator, cmd: &BuiltinCommand) -> String {
    let mut output = String::new();

    // Handle environment variables if any
    let has_env = !cmd.env_vars.is_empty();
    if has_env {
        output.push_str("{\n");
        for (var, value) in &cmd.env_vars {
            // Check if this is an associative array assignment like map[foo]=bar
            if let Some((array_name, key)) = generator.extract_array_key(var) {
                let val = generator.perl_string_literal(value);
                // For associative array assignments, generate $array{key} = value instead of $ENV{var}
                output.push_str(&format!("${}{{{}}} = {};\n", array_name, key, val));
            } else if let Word::Literal(s, _) = value {
                if let Some(elements) = generator.extract_array_elements(s) {
                    // Check if this is an indexed array assignment like arr=(one two three)
                    let elements_perl: Vec<String> = elements
                        .iter()
                        .map(|s| format!("\"{}\"", generator.escape_perl_string(s)))
                        .collect();
                    output.push_str(&format!("@{} = ({});\n", var, elements_perl.join(", ")));
                } else {
                    // Regular string assignment
                    let val = generator.perl_string_literal(value);
                    // Declare the variable if it's not already declared
                    if !generator.declared_locals.contains(var) {
                        output.push_str(&format!("my ${} = {};\n", var, val));
                        generator.declared_locals.insert(var.clone());
                    } else {
                        // Variable already declared, just assign the value
                        output.push_str(&format!("${} = {};\n", var, val));
                    }
                    // Don't set environment variable immediately - only set it when export command is encountered
                    // This matches bash behavior where variables are only exported to environment after export command
                }
            } else {
                let val = generator.perl_string_literal(value);
                // Declare the variable if it's not already declared
                if !generator.declared_locals.contains(var) {
                    output.push_str(&format!("my ${} = {};\n", var, val));
                    generator.declared_locals.insert(var.clone());
                } else {
                    // Variable already declared, just assign the value
                    output.push_str(&format!("${} = {};\n", var, val));
                }
                output.push_str(&format!("local $ENV{{{}}} = {};;\n", var, val));
            }
        }
    }

    // Generate the builtin command
    match cmd.name.as_str() {
        "set" => {
            // Convert shell set options to Perl equivalents
            for arg in &cmd.args {
                if let Word::Literal(opt, _) = arg {
                    match opt.as_str() {
                        "-e" => output.push_str("$SIG{__DIE__} = sub { exit 1 };\n"),
                        "-u" => output.push_str("use strict;\n"),
                        "-o" => {
                            // Handle pipefail and other options
                            if let Some(next_arg) = cmd
                                .args
                                .get(cmd.args.iter().position(|a| a == arg).unwrap() + 1)
                            {
                                if let Word::Literal(opt_name, _) = next_arg {
                                    match opt_name.as_str() {
                                        "pipefail" => output.push_str(
                                            "# set -o pipefail not implemented in Perl\n",
                                        ),
                                        _ => output.push_str(&format!(
                                            "# set -o {} not implemented\n",
                                            opt_name
                                        )),
                                    }
                                }
                            }
                        }
                        _ => output.push_str(&format!("# set {} not implemented\n", opt)),
                    }
                }
            }
        }
        "unset" => {
            // Handle unset command
            for arg in &cmd.args {
                if let Word::Literal(var_name, _) = arg {
                    if let Some((array_name, key)) = generator.extract_array_key(var_name) {
                        // Unset array element
                        output.push_str(&format!("delete ${}{{{}}};\n", array_name, key));
                    } else {
                        // Unset variable - ensure it's declared first
                        if !generator.declared_locals.contains(var_name) {
                            output.push_str(&format!("my ${};\n", var_name));
                            generator.declared_locals.insert(var_name.clone());
                        }
                        output.push_str(&format!("undef ${};\n", var_name));
                        output.push_str(&format!("delete $ENV{{{}}};\n", var_name));
                    }
                }
            }
        }
        "export" => {
            // Handle export command
            for arg in &cmd.args {
                if let Word::Literal(var_name, _) = arg {
                    if let Some((array_name, key)) = generator.extract_array_key(var_name) {
                        // Export array element
                        output.push_str(&format!(
                            "$ENV{{{}}} = ${}{{{}}};\n",
                            var_name, array_name, key
                        ));
                    } else {
                        // Export variable
                        output.push_str(&format!("$ENV{{{}}} = ${};\n", var_name, var_name));
                    }
                }
            }
        }
        "readonly" => {
            // Handle readonly command (not directly supported in Perl)
            for arg in &cmd.args {
                if let Word::Literal(var_name, _) = arg {
                    output.push_str(&format!(
                        "# readonly {} not implemented in Perl\n",
                        var_name
                    ));
                }
            }
        }
        "declare" => {
            // Handle declare command
            for arg in &cmd.args {
                if let Word::Literal(opt, _) = arg {
                    match opt.as_str() {
                        "-a" => {
                            // Declare array
                            if let Some(next_arg) = cmd
                                .args
                                .get(cmd.args.iter().position(|a| a == arg).unwrap() + 1)
                            {
                                if let Word::Literal(var_name, _) = next_arg {
                                    if !generator.declared_locals.contains(var_name) {
                                        output.push_str(&format!("my @{} = ();\n", var_name));
                                        generator.declared_locals.insert(var_name.clone());
                                    }
                                }
                            }
                        }
                        "-A" => {
                            // Declare associative array
                            if let Some(next_arg) = cmd
                                .args
                                .get(cmd.args.iter().position(|a| a == arg).unwrap() + 1)
                            {
                                if let Word::Literal(var_name, _) = next_arg {
                                    if !generator.declared_locals.contains(var_name) {
                                        output.push_str(&format!("my %{} = ();\n", var_name));
                                        generator.declared_locals.insert(var_name.clone());
                                    }
                                }
                            }
                        }
                        _ => output.push_str(&format!("# declare {} not implemented\n", opt)),
                    }
                }
            }
        }
        "local" => {
            // Handle local command - convert to my declarations
            let mut i = 0;
            while i < cmd.args.len() {
                match &cmd.args[i] {
                    Word::Literal(var_name, _) => {
                        // Check if it's an assignment (var=value)
                        if var_name.contains('=') {
                            let parts: Vec<&str> = var_name.splitn(2, '=').collect();
                            if parts.len() == 2 {
                                let var = parts[0];
                                let value = parts[1];
                                if !generator.declared_locals.contains(var) {
                                    // Check if the next argument is a CommandSubstitution
                                    if i + 1 < cmd.args.len() {
                                        match &cmd.args[i + 1] {
                                            Word::CommandSubstitution(cmd_sub, _) => {
                                                // Handle command substitution
                                                let perl_command = generator.word_to_perl(
                                                    &Word::CommandSubstitution(
                                                        cmd_sub.clone(),
                                                        None,
                                                    ),
                                                );
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!(
                                                    "my ${} = {};\n",
                                                    var, perl_command
                                                ));
                                                i += 1; // Skip the CommandSubstitution argument
                                            }
                                            _ => {
                                                // Regular assignment without command substitution
                                                let perl_value = if value.starts_with('$') {
                                                    // Handle shell variables like $1, $2, etc.
                                                    if value.chars().skip(1).all(|c| c.is_digit(10))
                                                    {
                                                        // Convert $1 to $_[0], $2 to $_[1], etc.
                                                        let index = value[1..]
                                                            .parse::<usize>()
                                                            .unwrap_or(0);
                                                        format!("$_[{}]", index - 1)
                                                    // Perl arrays are 0-indexed
                                                    } else {
                                                        // Regular variable
                                                        value.to_string()
                                                    }
                                                } else {
                                                    // Literal value - quote it
                                                    format!("\"{}\"", value)
                                                };
                                                output.push_str(&generator.indent());
                                                output.push_str(&format!(
                                                    "my ${} = {};\n",
                                                    var, perl_value
                                                ));
                                            }
                                        }
                                    } else {
                                        // Regular assignment without command substitution
                                        let perl_value = if value.starts_with('$') {
                                            // Handle shell variables like $1, $2, etc.
                                            if value.chars().skip(1).all(|c| c.is_digit(10)) {
                                                // Convert $1 to $_[0], $2 to $_[1], etc.
                                                let index =
                                                    value[1..].parse::<usize>().unwrap_or(0);
                                                format!("$_[{}]", index - 1) // Perl arrays are 0-indexed
                                            } else {
                                                // Regular variable
                                                value.to_string()
                                            }
                                        } else {
                                            // Literal value - quote it
                                            format!("\"{}\"", value)
                                        };
                                        output.push_str(&generator.indent());
                                        output
                                            .push_str(&format!("my ${} = {};\n", var, perl_value));
                                    }
                                    generator.declared_locals.insert(var.to_string());
                                }
                            }
                        } else {
                            // Just declaration without assignment
                            if !generator.declared_locals.contains(var_name) {
                                output.push_str(&generator.indent());
                                output.push_str(&format!("my ${};\n", var_name));
                                generator.declared_locals.insert(var_name.clone());
                            }
                        }
                    }
                    Word::CommandSubstitution(cmd_sub, _) => {
                        // Handle standalone command substitution (shouldn't happen in local command)
                        let perl_command = generator
                            .word_to_perl(&Word::CommandSubstitution(cmd_sub.clone(), None));
                        output.push_str(&generator.indent());
                        output.push_str(&format!(
                            "my $result_{} = {};\n",
                            generator.get_unique_id(),
                            perl_command
                        ));
                    }
                    _ => {
                        // For other word types, check if it's a command-related argument
                        if let Word::Literal(s, _) = &cmd.args[i] {
                            // Skip command-related arguments like "local", "size=", etc.
                            if s == "local" || s.ends_with('=') {
                                // Skip these arguments as they're part of the command structure
                                continue;
                            }
                        }

                        // Skip processing of command-related arguments
                        // The local command parsing should handle assignments properly
                        continue;
                    }
                }
                i += 1;
            }
        }
        _ => {
            // Other builtin commands
            output.push_str(&format!(
                "# Builtin command '{}' not implemented\n",
                cmd.name
            ));
        }
    }

    if has_env {
        output.push_str("}\n");
    }

    output
}

// Helper method for escaping Perl strings
pub fn escape_perl_string(s: &str) -> String {
    s.replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\n", "\\n")
        .replace("\t", "\\t")
        .replace("\r", "\\r")
}
