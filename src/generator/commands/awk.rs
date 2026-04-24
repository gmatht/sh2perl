use crate::ast::*;
use crate::generator::Generator;

pub fn generate_awk_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    _command_index: usize,
) -> String {
    let mut output = String::new();

    // Parse awk arguments conservatively. Support an optional -F<sep>
    // field separator and extract the action block {...} along with an
    // optional condition before the block (eg. "$4 > 90 { print ... }").
    let mut field_sep_token: Option<Word> = None;
    for i in 0..cmd.args.len() {
        if let Word::Literal(s, _) = &cmd.args[i] {
            if s.starts_with("-F") {
                // -Fsep or -F sep
                let rest = s[2..].to_string();
                if !rest.is_empty() {
                    field_sep_token = Some(Word::literal(rest));
                    break;
                } else if i + 1 < cmd.args.len() {
                    field_sep_token = Some(cmd.args[i + 1].clone());
                    break;
                }
            }
        }
    }

    // Find the awk program text (the argument that contains a { ... } block).
    let mut action_block = String::new();
    let mut condition_str = String::new();
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            // Strip one layer of surrounding quotes if present and decode
            // common shell escapes (for example: \$ -> $, \\n+            // -> \, \n -> newline). Many awk programs in examples are
            // embedded inside shell/perl literals and therefore contain
            // backslash-escapes that must be decoded before parsing.
            let mut lit = s.clone();
            if (lit.starts_with('\'') && lit.ends_with('\''))
                || (lit.starts_with('"') && lit.ends_with('"'))
            {
                if lit.len() >= 2 {
                    lit = lit[1..lit.len() - 1].to_string();
                }
            }

            // Decode shell-style escapes so sequences like "\$1" become "$1"
            // which simplifies detection of $N variables below.
            lit = crate::generator::utils::decode_shell_escapes_impl(&lit);

            if let Some(start) = lit.find('{') {
                if let Some(end) = lit.rfind('}') {
                    // Extract everything between the braces as the action
                    if end > start + 1 {
                        action_block = lit[start + 1..end].to_string();
                    }
                    // Anything before the opening brace is the condition
                    let cond = lit[..start].trim();
                    condition_str = cond.to_string();
                    break;
                }
            }
        }
    }

    if input_var.starts_with('$') {
        output.push_str(&format!("my @lines = split /\\n/msx, {};\n", input_var));
    } else {
        output.push_str(&format!("my @lines = split /\\n/msx, ${};\n", input_var));
    }
    output.push_str("my @result;\n");
    output.push_str("foreach my $line (@lines) {\n");
    output.push_str("    chomp $line;\n");
    output.push_str(&format!(
        "    if ($line =~ {}) {{ next; }}\n",
        generator.format_regex_pattern(r"^\s*$")
    )); // Skip empty lines

    // Build split pattern based on -F if provided, otherwise default to whitespace
    let split_pat = if let Some(token) = field_sep_token {
        // Strip quotes and convert to a safe regex pattern
        let raw = generator.strip_shell_quotes_for_regex(&token);
        generator.format_regex_pattern(&raw)
    } else {
        generator.format_regex_pattern(r"\s+")
    };

    output.push_str(&format!("    my @fields = split {}, $line;\n", split_pat));

    // If there is a condition before the action (eg. "$4 > 90"), translate $N into $fields[N-1]
    if !condition_str.is_empty() {
        // Simple conversion: replace $N with $fields[N-1]
        let mut conv = String::new();
        let mut chars = condition_str.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '$' {
                // collect digits following $
                let mut num = String::new();
                while let Some(peek) = chars.peek() {
                    if peek.is_digit(10) {
                        num.push(*peek);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !num.is_empty() {
                    if let Ok(n) = num.parse::<usize>() {
                        // AWK $0 refers to the whole record; map it to $line.
                        if n == 0 {
                            conv.push_str("$line");
                            continue;
                        }
                        if n > 0 {
                            conv.push_str(&format!("$fields[{}]", n - 1));
                            continue;
                        }
                    }
                }
                // If we reach here, we couldn't parse a number - emit literal $
                conv.push('$');
            } else {
                conv.push(ch);
            }
        }
        output.push_str(&format!("    if (!({})) {{ next; }}\n", conv));
    }

    // Parse the action block minimally to handle common patterns: printf(...) and print <tokens>
    let action = action_block.trim();
    if action.starts_with("printf") {
        // Very small parser: printf "format", $1, $2, ...
        if let Some(rest) = action.strip_prefix("printf") {
            let rest = rest.trim();
            if !rest.is_empty() {
                // Expect a quoted format string followed by comma-separated args
                let first_char = rest.chars().next().unwrap();
                if first_char == '"' || first_char == '\'' {
                    // find matching closing quote (naive, does not handle escapes)
                    if let Some(end_idx) = rest[1..].find(first_char) {
                        let fmt = &rest[1..1 + end_idx];
                        let after = rest[1 + end_idx + 1..].trim();
                        // Strip leading comma
                        let args_str = after.strip_prefix(',').unwrap_or(after).trim();

                        // Split args by commas but be tolerant of stray trailing
                        // punctuation (for example a closing ')' after the arg
                        // list). Collect only the meaningful tokens.
                        let mut args: Vec<String> = Vec::new();
                        for raw in args_str.split(',') {
                            let tok = raw.trim();
                            if tok.is_empty() {
                                continue;
                            }
                            // Extract leading $N if present
                            if tok.starts_with('$') {
                                let mut digits = String::new();
                                for ch in tok.chars().skip(1) {
                                    if ch.is_digit(10) {
                                        digits.push(ch);
                                    } else {
                                        break;
                                    }
                                }
                                if !digits.is_empty() {
                                    if let Ok(n) = digits.parse::<usize>() {
                                        if n == 0 {
                                            args.push("$line".to_string());
                                        } else {
                                            args.push(format!("$fields[{}]", n - 1));
                                        }
                                        continue;
                                    }
                                }
                            }
                            // Fallback: emit the token verbatim (could be a literal or expression)
                            args.push(tok.to_string());
                        }

                        let fmt_lit =
                            generator.perl_string_literal(&Word::literal(fmt.to_string()));
                        let args_join = if args.is_empty() {
                            String::new()
                        } else {
                            format!(", {}", args.join(", "))
                        };
                        output.push_str(&format!(
                            "    push @result, sprintf({}{});\n",
                            fmt_lit, args_join
                        ));
                    } else {
                        // Fallback - treat as entire line
                        output.push_str("    push @result, $line;\n");
                    }
                } else {
                    // Fallback - unknown format
                    output.push_str("    push @result, $line;\n");
                }
            }
        }
    } else if action.contains("toupper(") {
        // Handle common toupper usage (e.g. print toupper($0) or print toupper($1)).
        // Map to Perl's uc() on the appropriate field or whole line and
        // preserve AWK's print newline semantics used elsewhere in this generator
        // (we append "\n" and join with an empty separator later).
        if action.contains("$0") {
            output.push_str("    push @result, (uc($line) . \"\\n\");\n");
        } else if action.contains("$1") {
            output.push_str("    push @result, (uc($fields[0]) . \"\\n\");\n");
        } else if action.contains("$2") {
            output.push_str("    push @result, (uc($fields[1]) . \"\\n\");\n");
        } else {
            output.push_str("    push @result, (uc($line) . \"\\n\");\n");
        }
    } else if action.contains("tolower(") {
        // Handle common tolower usage similarly
        if action.contains("$0") {
            output.push_str("    push @result, (lc($line) . \"\\n\");\n");
        } else if action.contains("$1") {
            output.push_str("    push @result, (lc($fields[0]) . \"\\n\");\n");
        } else if action.contains("$2") {
            output.push_str("    push @result, (lc($fields[1]) . \"\\n\");\n");
        } else {
            output.push_str("    push @result, (lc($line) . \"\\n\");\n");
        }
    } else if action.contains("print") {
        // Extract everything after the "print" token and tokenize into quoted strings and $N variables
        if let Some(pos) = action.find("print") {
            let mut rem = action[pos + "print".len()..].trim().to_string();
            // Remove trailing semicolon if present
            if rem.ends_with(';') {
                rem.pop();
            }

            // Tokenize
            let mut parts: Vec<String> = Vec::new();
            let mut i = 0usize;
            let chars: Vec<char> = rem.chars().collect();
            while i < chars.len() {
                // skip whitespace
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                if i >= chars.len() {
                    break;
                }

                let c = chars[i];
                if c == '"' || c == '\'' {
                    // quoted string
                    let quote = c;
                    i += 1;
                    let start = i;
                    while i < chars.len() {
                        if chars[i] == quote {
                            break;
                        }
                        i += 1;
                    }
                    let s = rem[start..i].to_string();
                    parts.push(generator.perl_string_literal(&Word::literal(s)));
                    // skip closing quote
                    if i < chars.len() && chars[i] == quote {
                        i += 1;
                    }
                } else if c == '$' {
                    // variable like $1
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i].is_digit(10) {
                        i += 1;
                    }
                    let num = rem[start..i].to_string();
                    if let Ok(n) = num.parse::<usize>() {
                        if n == 0 {
                            parts.push("$line".to_string());
                        } else {
                            parts.push(format!("$fields[{}]", n - 1));
                        }
                    } else {
                        parts.push(format!("${}", num));
                    }
                } else {
                    // bare token (unlikely in our examples) - collect until whitespace or end
                    let start = i;
                    while i < chars.len() && !chars[i].is_whitespace() {
                        i += 1;
                    }
                    let s = rem[start..i].to_string();
                    // emit as a literal
                    parts.push(generator.perl_string_literal(&Word::literal(s)));
                }
                // skip optional commas between print args
                while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
                    i += 1;
                }
            }

            if parts.is_empty() {
                // AWK `print` appends ORS (usually a newline). Preserve that
                // behaviour by pushing a newline-terminated element into
                // @result. Later we will join the results with an empty
                // separator so printf/print semantics are preserved exactly.
                output.push_str("    push @result, ($line . \"\\n\");\n");
            } else {
                // Join concatenated tokens with Perl concatenation and append a newline
                output.push_str(&format!(
                    "    push @result, ({} . \"\\n\");\n",
                    parts.join(" . ")
                ));
            }
        } else {
            output.push_str("    push @result, $line;\n");
        }
    } else {
        // Default: push whole line (preserve AWK's print-like newline semantics)
        output.push_str("    push @result, ($line . \"\\n\");\n");
    }

    output.push_str("}\n");

    // Join results without inserting extra separators; each @result element
    // already contains the correct termination (print appends ORS/newline,
    // printf includes formatting-controlled newlines). Using join "" preserves
    // the exact output bytes produced by the AWK program.
    if input_var.starts_with('$') {
        output.push_str(&format!("{} = join \"\", @result;\n", input_var));
    } else {
        output.push_str(&format!("${} = join \"\", @result;\n", input_var));
    }

    output.push_str("\n");

    output
}
