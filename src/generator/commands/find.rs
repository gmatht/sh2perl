use crate::ast::*;
use crate::generator::Generator;

fn escape_glob_pattern(pattern: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = pattern.chars().collect();

    for (i, c) in chars.iter().enumerate() {
        match c {
            '*' => {
                if i == 0 {
                    // At start of pattern, * means "any characters"
                    result.push_str(".*");
                } else {
                    // In middle/end, * means "any characters"
                    result.push_str(".*");
                }
            }
            '?' => result.push_str("."),
            '.' => result.push_str("[.]"),
            '[' => result.push_str("\\["),
            ']' => result.push_str("\\]"),
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '+' => result.push_str("\\+"),
            '^' => result.push_str("\\^"),
            '$' => result.push_str("\\$"),
            '|' => result.push_str("\\|"),
            '{' => result.push_str("\\{"),
            '}' => result.push_str("\\}"),
            '/' => result.push_str("\\/"),
            _ => result.push(*c),
        }
    }

    // Add end anchor for proper matching
    result.push('$');
    result
}

pub fn generate_find_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    generate_output: bool,
    input_var: &str,
) -> String {
    let mut output = String::new();

    // Check if -ls is present - if so, use system fallback for better compatibility
    let has_ls = cmd.args.iter().any(|arg| {
        if let Word::Literal(s, _) = arg {
            s == "-ls"
        } else {
            false
        }
    });

    if has_ls {
        return generate_system_find_fallback(generator, cmd, generate_output, input_var);
    }

    eprintln!(
        "DEBUG: generate_find_command called with generate_output: {}, input_var: '{}'",
        generate_output, input_var
    );
    if generate_output && !input_var.is_empty() {
        eprintln!(
            "DEBUG: Using generate_find_for_substitution with input_var: '{}'",
            input_var
        );
        return generate_find_for_substitution(generator, cmd, input_var);
    }

    eprintln!("DEBUG: Using system find fallback instead");
    generate_system_find_fallback(generator, cmd, generate_output, input_var)
}

fn generate_system_find_fallback(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    generate_output: bool,
    input_var: &str,
) -> String {
    let mut output = String::new();

    // Build the find command arguments for open3
    let mut find_args = Vec::new();
    for arg in &cmd.args {
        match arg {
            Word::Literal(s, _) => {
                let word = Word::Literal(s.clone(), Default::default());
                find_args.push(generator.perl_string_literal(&word));
            }
            Word::StringInterpolation(interp, _) => {
                // Use the convert_string_interpolation_to_perl function directly
                find_args.push(generator.convert_string_interpolation_to_perl(interp));
            }
            _ => {
                // For other word types, convert to Perl
                find_args.push(generator.perl_string_literal(arg));
            }
        }
    }

    if generate_output {
        // For pipeline context, capture output to variable
        output.push_str(&generator.indent());
        let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
        let formatted_args = find_args.join(", ");
        output.push_str(&format!(
            "my ({}, {}, {});
my {} = open3({}, {}, {}, 'find', {});
close {} or croak 'Close failed: $OS_ERROR';
${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};
close {} or croak 'Close failed: $OS_ERROR';
waitpid {}, 0;\n",
            in_var,
            out_var,
            err_var,
            pid_var,
            in_var,
            out_var,
            err_var,
            formatted_args,
            in_var,
            input_var,
            out_var,
            out_var,
            pid_var
        ));
        output.push_str(&generator.indent());
        output.push_str(&format!("chomp ${};\n", input_var));
    } else {
        // For standalone commands, execute directly
        output.push_str(&generator.indent());
        let formatted_args = find_args.join(", ");
        output.push_str(&format!("system 'find', {};\n", formatted_args));
    }

    output
}

pub fn generate_find_for_substitution(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    _input_var: &str,
) -> String {
    // For command substitution, defer to the real `find` command so its
    // traversal order and filtering match the shell exactly.
    let command = Command::Simple(cmd.clone());
    let command_str = generator.generate_command_string_for_system(&command);
    let command_lit = generator.perl_string_literal_no_interp(&Word::literal(command_str));
    format!(
        "do {{ my $command = {}; my $result = qx{{$command}}; $CHILD_ERROR = $? >> 8; $result; }}",
        command_lit
    )
}
