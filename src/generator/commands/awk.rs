use crate::ast::*;
use crate::generator::Generator;
use std::collections::HashSet;

/// Translate a single awk token/expression to its Perl equivalent.
/// `acc_vars` is the set of user-defined accumulation variables detected
/// in the awk program (e.g. "sum").
fn translate_awk_expr(expr: &str, acc_vars: &HashSet<String>) -> String {
    let expr = expr.trim();
    // $N field references (when the whole expr is just $N)
    if expr.starts_with('$') {
        let rest = &expr[1..];
        if let Ok(n) = rest.parse::<usize>() {
            if n == 0 {
                return "$line".to_string();
            } else {
                return format!("$fields[{}]", n - 1);
            }
        }
    }
    // length($N) / length($0) when the whole expr is a length() call
    if expr.starts_with("length(") && expr.ends_with(')') {
        let inner = &expr["length(".len()..expr.len() - 1];
        let inner_perl = translate_awk_expr(inner, acc_vars);
        return format!("length({})", inner_perl);
    }
    // Special awk variables (standalone)
    match expr {
        "NR" => return "$NR".to_string(),
        "NF" => return "scalar(@fields)".to_string(),
        _ => {}
    }
    // User-defined accumulation variable (exact match)
    if acc_vars.contains(expr) {
        return format!("${}", expr);
    }
    // General expression: apply substitutions for $N, acc_vars, NR, NF
    let mut result = expr.to_string();
    // Replace $0 with $line
    if let Ok(re) = regex::Regex::new(r"\$0\b") {
        result = re.replace_all(&result, "$$line").to_string();
    }
    // Replace $N (N > 0) with $fields[N-1] using a closure
    if let Ok(re) = regex::Regex::new(r"\$([1-9][0-9]*)") {
        result = re.replace_all(&result, |caps: &regex::Captures| {
            if let Ok(n) = caps[1].parse::<usize>() {
                format!("$fields[{}]", n - 1)
            } else {
                caps[0].to_string()
            }
        }).to_string();
    }
    // Replace accumulation variables (whole word)
    for var in acc_vars {
        let pattern = format!("\\b{}\\b", var);
        if let Ok(re) = regex::Regex::new(&pattern) {
            // Use $$ in replacement to produce a literal $ in the output
            // (the regex crate treats $name as a capture group reference)
            result = re.replace_all(&result, format!("$${}", var).as_str()).to_string();
        }
    }
    // Replace NR and NF in expressions
    if let Ok(re) = regex::Regex::new(r"\bNR\b") {
        result = re.replace_all(&result, "$$NR").to_string();
    }
    if let Ok(re) = regex::Regex::new(r"\bNF\b") {
        result = re.replace_all(&result, "scalar(@fields)").to_string();
    }
    result
}

/// Parse the "print" tokens from an awk print statement body and translate
/// each token to its Perl equivalent.
fn translate_awk_print_args(rem: &str, acc_vars: &HashSet<String>, generator: &mut Generator) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let chars: Vec<char> = rem.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let c = chars[i];
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != quote {
                i += 1;
            }
            let s = rem[start..i].to_string();
            parts.push(generator.perl_string_literal(&Word::literal(s)));
            if i < chars.len() { i += 1; } // skip closing quote
        } else if c == '$' {
            // $N variable
            i += 1;
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let num_str = &rem[start..i];
            if let Ok(n) = num_str.parse::<usize>() {
                if n == 0 {
                    parts.push("$line".to_string());
                } else {
                    parts.push(format!("$fields[{}]", n - 1));
                }
            } else {
                parts.push(format!("${}", num_str));
            }
        } else {
            // Bare token: could be NR, NF, a variable name, or a literal
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '"' && chars[i] != '\'' && chars[i] != ',' {
                i += 1;
            }
            let tok = rem[start..i].trim_end_matches(|c: char| !c.is_alphanumeric() && c != ')' && c != '_');
            if !tok.is_empty() {
                parts.push(translate_awk_expr(tok, acc_vars));
            }
        }
        // Skip optional commas/spaces
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
    }
    parts
}

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
    let mut end_block = String::new();  // END { ... } block
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            // Strip one layer of surrounding quotes if present and decode
            // common shell escapes (for example: \$ -> $, \\n
            // -> \, \n -> newline). Many awk programs in examples are
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

            // Check for END block: "... END { ... }"
            if let Some(end_pos) = lit.find("END") {
                let after_end = lit[end_pos + 3..].trim();
                if after_end.starts_with('{') {
                    if let Some(end_close) = after_end.rfind('}') {
                        end_block = after_end[1..end_close].trim().to_string();
                        // The main program is before "END"
                        lit = lit[..end_pos].trim().to_string();
                    }
                }
            }

            if let Some(start) = lit.find('{') {
                if let Some(end) = lit.rfind('}') {
                    // Extract everything between the braces as the action
                    if end > start + 1 {
                        action_block = lit[start + 1..end].to_string();
                    } else {
                        action_block = String::new();
                    }
                    // Anything before the opening brace is the condition
                    let cond = lit[..start].trim();
                    condition_str = cond.to_string();
                    break;
                }
            }
        }
    }

    // Scan the full program to detect:
    // 1. Accumulation variables (pattern: "identifier +=" or "identifier -=")
    // 2. Whether NR is referenced
    let full_program = format!("{} {} {}", condition_str, action_block, end_block);
    let mut acc_vars: HashSet<String> = HashSet::new();
    {
        // Match "word +=" patterns to find accumulation variables
        if let Ok(re) = regex::Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\+=") {
            for cap in re.captures_iter(&full_program) {
                let var_name = cap[1].to_string();
                if var_name != "NR" && var_name != "NF" {
                    acc_vars.insert(var_name);
                }
            }
        }
    }
    let uses_nr = full_program.contains("NR");

    if input_var.starts_with('$') {
        output.push_str(&format!("my @lines = split /\\n/msx, {};\n", input_var));
    } else {
        output.push_str(&format!("my @lines = split /\\n/msx, ${};\n", input_var));
    }
    output.push_str("my @result;\n");

    // Declare accumulation variables before the loop
    for var in &acc_vars {
        output.push_str(&format!("my ${} = 0;\n", var));
    }
    // Declare NR counter if used
    if uses_nr {
        output.push_str("my $NR = 0;\n");
    }

    output.push_str("foreach my $line (@lines) {\n");
    output.push_str("    chomp $line;\n");
    output.push_str(&format!(
        "    if ($line =~ {}) {{ next; }}\n",
        generator.format_regex_pattern(r"^\s*$")
    )); // Skip empty lines

    // Increment NR at the top of the loop if it's used
    if uses_nr {
        output.push_str("    $NR++;\n");
    }

    // Build split pattern based on -F if provided, otherwise default to whitespace
    let split_pat = if let Some(token) = field_sep_token {
        // Strip quotes and convert to a safe regex pattern
        let raw = generator.strip_shell_quotes_for_regex(&token);
        generator.format_regex_pattern(&raw)
    } else {
        generator.format_regex_pattern(r"\s+")
    };

    output.push_str(&format!("    my @fields = split {}, $line;\n", split_pat));

    // If there is a condition before the action (eg. "$4 > 90", "NF < 3", "length($0) > 20"),
    // translate awk expressions to Perl
    if !condition_str.is_empty() {
        let conv = translate_awk_expr(&condition_str, &acc_vars);
        output.push_str(&format!("    if (!({})) {{ next; }}\n", conv));
    }

    // Parse the action block: handle accumulation statements, printf, print, toupper/tolower
    let action = action_block.trim();

    // Check for accumulation statement like "sum += length($0)" or "sum += NF"
    if acc_vars.iter().any(|v| action.contains(&format!("{} +=", v)) || action.contains(&format!("{}+=", v))) {
        // Generate accumulation code
        for var in &acc_vars {
            let pat1 = format!("{} +=", var);
            let pat2 = format!("{}+=", var);
            let rhs_raw = if let Some(pos) = action.find(&pat1) {
                action[pos + pat1.len()..].trim().trim_end_matches(';')
            } else if let Some(pos) = action.find(&pat2) {
                action[pos + pat2.len()..].trim().trim_end_matches(';')
            } else {
                continue;
            };
            let rhs_perl = translate_awk_expr(rhs_raw, &acc_vars);
            output.push_str(&format!("    ${} += {};\n", var, rhs_perl));
        }
    } else if action.starts_with("printf") {
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

                        // Split args by commas and translate each token
                        let mut args: Vec<String> = Vec::new();
                        for raw in args_str.split(',') {
                            let tok = raw.trim().trim_end_matches(')').trim_end_matches(';');
                            if !tok.is_empty() {
                                args.push(translate_awk_expr(tok, &acc_vars));
                            }
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
        // Extract everything after the "print" token and tokenize
        if let Some(pos) = action.find("print") {
            let mut rem = action[pos + "print".len()..].trim().to_string();
            // Remove trailing semicolon if present
            if rem.ends_with(';') {
                rem.pop();
            }

            let parts = translate_awk_print_args(&rem, &acc_vars, generator);

            if parts.is_empty() {
                // AWK `print` appends ORS (usually a newline).
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

    // Generate END block code (runs after the loop)
    if !end_block.is_empty() {
        let end_action = end_block.trim();
        if let Some(pos) = end_action.find("print") {
            let mut rem = end_action[pos + "print".len()..].trim().to_string();
            if rem.ends_with(';') {
                rem.pop();
            }
            let parts = translate_awk_print_args(&rem, &acc_vars, generator);
            if parts.is_empty() {
                output.push_str("push @result, \"\\n\";\n");
            } else {
                // Wrap numeric expressions (those containing division) with
                // sprintf("%.6g", ...) to match AWK's default OFMT formatting.
                let formatted_parts: Vec<String> = parts.iter().map(|p| {
                    let is_quoted = p.starts_with('\'') || p.starts_with('"');
                    if !is_quoted && p.contains('/') {
                        format!("sprintf(\"%.6g\", {})", p)
                    } else {
                        p.clone()
                    }
                }).collect();
                output.push_str(&format!(
                    "push @result, ({} . \"\\n\");\n",
                    formatted_parts.join(" . ")
                ));
            }
        }
    }

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
