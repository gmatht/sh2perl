use crate::ast::*;
use crate::generator::Generator;

/// Extract a simple literal value from a Word, stripping surrounding quotes
/// if present. Handles both Word::Literal and Word::StringInterpolation
/// containing a single Literal part.
fn extract_literal_from_word(word: &Word) -> Option<String> {
    match word {
        Word::Literal(s, _) => {
            Some(s.trim_matches('"').trim_matches('\'').to_string())
        }
        Word::StringInterpolation(interp, _) => {
            if interp.parts.len() == 1 {
                if let StringPart::Literal(s) = &interp.parts[0] {
                    return Some(s.trim_matches('"').trim_matches('\'').to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// Convert a glob pattern (like "*.sh") to a Perl regex pattern.
fn escape_glob_to_regex(pattern: &str) -> String {
    let mut result = String::new();
    result.push('^');
    for c in pattern.chars() {
        match c {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '.' => result.push_str("\\."),
            '[' => result.push_str("["),
            ']' => result.push_str("]"),
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '+' => result.push_str("\\+"),
            '^' => result.push_str("\\^"),
            '$' => result.push_str("$"),
            '|' => result.push_str("\\|"),
            '{' => result.push_str("\\{"),
            '}' => result.push_str("\\}"),
            '/' => result.push_str("\\/"),
            '\\' => result.push_str("\\\\"),
            _ => result.push(c),
        }
    }
    result.push('$');
    result
}

/// Parse common find flags from a SimpleCommand. Returns (start_dir, name_pattern, file_type, maxdepth).
pub fn generate_find_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    generate_output: bool,
    input_var: &str,
) -> String {
    let mut output = String::new();

    // Parse find arguments
    let mut start_dir = String::from(".");
    let mut name_pattern: Option<String> = None;
    let mut file_type: Option<String> = None;
    let mut maxdepth: Option<String> = None;

    let mut args_iter = cmd.args.iter();
    while let Some(arg) = args_iter.next() {
        if let Some(lit) = extract_literal_from_word(arg) {
            match lit.as_str() {
                "-name" => {
                    if let Some(next_arg) = args_iter.next() {
                        name_pattern = extract_literal_from_word(next_arg);
                    }
                }
                "-type" => {
                    if let Some(next_arg) = args_iter.next() {
                        file_type = extract_literal_from_word(next_arg);
                    }
                }
                "-maxdepth" => {
                    if let Some(next_arg) = args_iter.next() {
                        maxdepth = extract_literal_from_word(next_arg);
                    }
                }
                s if s.starts_with('-') => {
                    // Skip other flags
                }
                _ => {
                    // First non-flag argument is the start directory
                    if start_dir == "." || start_dir == "'./'" {
                        start_dir = generator.perl_string_literal(arg);
                    }
                }
            }
        }
    }

    // Build the Perl code
    output.push_str(&generator.indent());
    output.push_str("require File::Find;\n");

    // Build the wanted function
    let mut wanted_lines = Vec::new();

    // Add type filter
    if let Some(ref ftype) = file_type {
        let test = match ftype.as_str() {
            "f" => "-f",
            "d" => "-d",
            "l" => "-l",
            _ => "-e",
        };
        wanted_lines.push(format!("    next unless {} $_;", test));
    }

    // Add name filter
    if let Some(ref pat) = name_pattern {
        let regex = escape_glob_to_regex(pat);
        wanted_lines.push(format!("    next unless $_ =~ /{}/msx;", regex));
    }

    // Add maxdepth handling
    if let Some(ref depth) = maxdepth {
        if let Ok(d) = depth.parse::<usize>() {
            wanted_lines.push(format!(
                "    my $depth = ($File::Find::dir =~ tr/\\///) + 1; next if $depth > {};",
                d
            ));
        }
    }

    // Add the result collection
    if generate_output && !input_var.is_empty() {
        // Store results in the output variable
        wanted_lines.push(format!(
            "    push @{}, $File::Find::name;",
            input_var
        ));

        output.push_str(&generator.indent());
        output.push_str(&format!("my @{} = ();\n", input_var));

        let start_dir_expr = if start_dir == "." || start_dir == "'./'" { "'./'" } else { &start_dir };
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "File::Find::find(sub {{ {} }}, {});\n",
            wanted_lines.join(" "),
            start_dir_expr
        ));

        output.push_str(&generator.indent());
        output.push_str(&format!(
            "@{} = sort @{};\n",
            input_var, input_var
        ));
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "${} = join \"\\n\", @{};\n",
            input_var, input_var
        ));
        output.push_str(&generator.indent());
        output.push_str(&format!("chomp ${};\n", input_var));
    } else {
        // Print results directly
        wanted_lines.push("    print \"$File::Find::name\\n\";".to_string());

        let start_dir_expr = if start_dir == "." || start_dir == "'./'" { "'./'" } else { &start_dir };
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "File::Find::find(sub {{ {} }}, {});\n",
            wanted_lines.join(" "),
            start_dir_expr
        ));
    }

    output
}

/// Generate native Perl find for substitution (backtick/capture) context.
/// Returns a do-block expression that evaluates to the find output.
pub fn generate_find_for_substitution(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    _input_var: &str,
) -> String {
    // Parse find arguments
    let mut start_dir = String::from(".");
    let mut name_pattern: Option<String> = None;
    let mut file_type: Option<String> = None;
    let mut maxdepth: Option<String> = None;

    let mut args_iter = cmd.args.iter();
    while let Some(arg) = args_iter.next() {
        if let Some(lit) = extract_literal_from_word(arg) {
            match lit.as_str() {
                "-name" => {
                    if let Some(next_arg) = args_iter.next() {
                        name_pattern = extract_literal_from_word(next_arg);
                    }
                }
                "-type" => {
                    if let Some(next_arg) = args_iter.next() {
                        file_type = extract_literal_from_word(next_arg);
                    }
                }
                "-maxdepth" => {
                    if let Some(next_arg) = args_iter.next() {
                        maxdepth = extract_literal_from_word(next_arg);
                    }
                }
                s if s.starts_with('-') => {
                    // Skip other flags
                }
                _ => {
                    // First non-flag argument is the start directory
                    if start_dir == "." || start_dir == "'./'" {
                        start_dir = generator.perl_string_literal(arg);
                    }
                }
            }
        }
    }

    let start_dir_expr = if start_dir == "." || start_dir == "'./'" {
        "'./'".to_string()
    } else {
        start_dir.clone()
    };

    // Build conditions
    let mut conditions = Vec::new();

    // Type filter
    if let Some(ref ftype) = file_type {
        let test = match ftype.as_str() {
            "f" => "-f $_",
            "d" => "-d $_",
            "l" => "-l $_",
            _ => "-e $_",
        };
        conditions.push(test.to_string());
    }

    // Name filter (convert glob pattern to regex)
    if let Some(ref pat) = name_pattern {
        let regex = escape_glob_to_regex(pat);
        conditions.push(format!("$_ =~ /{}/msx", regex));
    }

    let condition_code = if conditions.is_empty() {
        String::from("1")
    } else {
        conditions.join(" && ")
    };

    // Maxdepth
    let maxdepth_code = if let Some(ref depth) = maxdepth {
        if let Ok(d) = depth.parse::<usize>() {
            format!(
                "my $maxdepth = {}; my $depth = ($File::Find::dir =~ tr/\\///) + 1; return if $depth > $maxdepth;",
                d
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    format!(
        "do {{\n    require File::Find;\n    my @find_results;\n    {maxdepth_code}\n    File::Find::find(sub {{ if ({condition_code}) {{ push @find_results, $File::Find::name; }} }}, {start_dir});\n    @find_results = sort @find_results;\n    my $result = join \"\\n\", @find_results;\n    if ($result ne q{{}}) {{ $result .= \"\\n\"; }}\n    $CHILD_ERROR = 0;\n    $result;\n}}",
        condition_code = condition_code,
        start_dir = start_dir_expr,
        maxdepth_code = maxdepth_code,
    )
}
