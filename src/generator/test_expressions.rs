use super::Generator;
use crate::ast::*;

/// Replace every `$((expr))` arithmetic expansion in `s` with the equivalent
/// Perl expression `(perl_expr)` so the surrounding test-expression logic
/// produces syntactically-valid Perl.
fn convert_arith_subexprs(s: &str, generator: &Generator) -> String {
    let mut result = s.to_string();
    loop {
        if let Some(start) = result.find("$((") {
            let after = start + 3;
            if let Some(rel_end) = result[after..].find("))") {
                let inner = result[after..after + rel_end].to_string();
                let perl = generator.convert_arithmetic_to_perl(&inner);
                result = format!(
                    "{}({}){}",
                    &result[..start],
                    perl,
                    &result[after + rel_end + 2..]
                );
            } else {
                break;
            }
        } else {
            break;
        }
    }
    result
}

// Helper function to convert shell variables to Perl equivalents
fn convert_shell_var_to_perl(var: &str) -> String {
    match var {
        "$#" => "scalar(@ARGV)".to_string(), // $# -> scalar(@ARGV) for argument count
        "$@" => "@ARGV".to_string(),         // $@ -> @ARGV for arguments array
        "$*" => "@ARGV".to_string(),         // $* -> @ARGV for arguments array
        _ if var.starts_with('$') => {
            // Regular variable - just return as is for now
            var.to_string()
        }
        _ => {
            // Not a variable - return as is
            var.to_string()
        }
    }
}

pub fn generate_test_expression_impl(
    generator: &mut Generator,
    test_expr: &TestExpression,
) -> String {
    // Pre-process: convert any $((expr)) arithmetic subexpressions to Perl
    let preprocessed = convert_arith_subexprs(&test_expr.expression, generator);
    let expr = &preprocessed;
    let modifiers = &test_expr.modifiers;

    // Helper closure: check if expr starts with an operator optionally followed by " or $
    let starts_with_op = |expr: &str, op: &str| -> bool {
        expr.starts_with(op) || expr.starts_with(&format!(r#"{}""#, op)) || expr.starts_with(&format!("{}$", op))
    };

    // Parse the expression to determine the type of test.
    // Order matters: logical operators (-a, -o) must be checked FIRST because
    // they combine sub-expressions that may themselves contain comparison operators.
    // Then grouping/parentheses and NOT. Then pattern/comparison operators,
    // then unary file/string tests.
    if expr.contains(" -a ") {
        // Logical AND: [[ expr1 -a expr2 ]]
        let parts: Vec<&str> = expr.split(" -a ").collect();
        if parts.len() == 2 {
            let expr1 = parts[0].trim();
            let expr2 = parts[1].trim();
            // Recursively parse each expression
            let parsed_expr1 = generator.generate_test_expression(&TestExpression {
                expression: expr1.to_string(),
                modifiers: modifiers.clone(),
            });
            let parsed_expr2 = generator.generate_test_expression(&TestExpression {
                expression: expr2.to_string(),
                modifiers: modifiers.clone(),
            });
            format!("({} && {})", parsed_expr1, parsed_expr2)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -o ") {
        // Logical OR: [[ expr1 -o expr2 ]]
        let parts: Vec<&str> = expr.split(" -o ").collect();
        if parts.len() == 2 {
            let expr1 = parts[0].trim();
            let expr2 = parts[1].trim();
            // Recursively parse each expression
            let parsed_expr1 = generator.generate_test_expression(&TestExpression {
                expression: expr1.to_string(),
                modifiers: modifiers.clone(),
            });
            let parsed_expr2 = generator.generate_test_expression(&TestExpression {
                expression: expr2.to_string(),
                modifiers: modifiers.clone(),
            });
            format!("({} || {})", parsed_expr1, parsed_expr2)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" ! ") {
        // Logical NOT: [[ ! expr ]]
        let subexpr = expr.replace("! ", "").trim().to_string();
        let parsed_subexpr = generator.generate_test_expression(&TestExpression {
            expression: subexpr,
            modifiers: modifiers.clone(),
        });
        format!("(!{})", parsed_subexpr)
    } else if expr.contains(" ( ") && expr.contains(" ) ") {
        // Parenthesized expression: [[ ( expr ) ]]
        let start = expr.find(" ( ").unwrap();
        let end = expr.rfind(" ) ").unwrap();
        if start < end {
            let subexpr = &expr[start + 3..end];
            let parsed_subexpr = generator.generate_test_expression(&TestExpression {
                expression: subexpr.to_string(),
                modifiers: modifiers.clone(),
            });
            format!("({})", parsed_subexpr)
        } else {
            "0".to_string()
        }
    } else if expr.contains("=~") {
        // Regex matching: [[ $var =~ pattern ]]
        let parts: Vec<&str> = expr.split("=~").collect();
        if parts.len() == 2 {
            let mut var = parts[0].trim();
            let pattern = parts[1].trim();
            // Fix ${array[@]} which is invalid Perl -> use q{} (empty string) instead
            if var.contains("[@]") || var.contains("[*]") {
                var = "q{}";
            }
            // Convert to Perl regex matching, using $ENV{var} for undeclared variables
            let var_ref = if var.starts_with('$') && !var.starts_with("$ENV") {
                let var_name = var.trim_start_matches('$').trim_start_matches('{').trim_end_matches('}');
                if !generator.declared_locals.contains(var_name)
                    && !generator.function_level_vars.contains(var_name)
                {
                    format!("$ENV{{{}}}", var_name)
                } else {
                    var.to_string()
                }
            } else {
                var.to_string()
            };
            format!("{} =~ {}", var_ref, generator.format_regex_pattern(pattern))
        } else {
            "0".to_string()
        }
    } else if expr.contains("==") {
        // Pattern matching: [[ $var == pattern ]]
        let parts: Vec<&str> = expr.split("==").collect();
        if parts.len() == 2 {
            let var = parts[0].trim();
            let pattern = parts[1].trim();
            if modifiers.extglob {
                let regex_pattern = generator.convert_extglob_to_perl_regex(pattern);
                if modifiers.nocasematch {
                    format!("{} =~ {}i", var, generator.format_regex_pattern(&regex_pattern))
                } else {
                    format!("{} =~ {}", var, generator.format_regex_pattern(&regex_pattern))
                }
            } else {
                let regex_pattern = generator.convert_glob_to_regex(pattern);
                if modifiers.nocasematch {
                    format!("{} =~ {}i", var, generator.format_regex_pattern(&format!("^{}$", regex_pattern)))
                } else {
                    format!("{} =~ {}", var, generator.format_regex_pattern(&format!("^{}$", regex_pattern)))
                }
            }
        } else {
            "0".to_string()
        }
    } else if expr.contains(" != ") {
        // String inequality: [[ $var != value ]]
        let parts: Vec<&str> = expr.split(" != ").collect();
        if parts.len() == 2 {
            let var = parts[0].trim();
            let value = parts[1].trim();
            format!("{} ne {}", var, value)
        } else {
            "0".to_string()
        }
    } else if expr.contains("!=") {
        // String inequality without spaces: [[ $var!=value ]]
        let parts: Vec<&str> = expr.split("!=").collect();
        if parts.len() == 2 {
            let var = parts[0].trim();
            let value = parts[1].trim();
            format!("{} ne {}", var, value)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" = ") || expr.contains("=") {
        // String equality: [[ $var = value ]] or [[ $var=value ]]
        let parts: Vec<&str> = if expr.contains(" = ") {
            expr.split(" = ").collect()
        } else {
            expr.split("=").collect()
        };
        if parts.len() == 2 {
            let var = parts[0].trim();
            let value = parts[1].trim();
            // Handle tilde expansion for home directory
            if var == "~" {
                let clean_value = if value.starts_with('"') && value.ends_with('"') && value.contains('$') {
                    let unquoted = value[1..value.len() - 1].to_string();
                    if unquoted == "$HOME" { "$ENV{'HOME'}".to_string() } else { unquoted }
                } else { value.to_string() };
                format!("$ENV{{'HOME'}} eq {}", clean_value)
            } else if var.starts_with("~/") {
                let path = var[2..].to_string();
                let clean_value = if value.starts_with('"') && value.ends_with('"') && value.contains('$') {
                    let unquoted = value[1..value.len() - 1].to_string();
                    if unquoted == "$HOME" { "$ENV{'HOME'}".to_string() } else { unquoted }
                } else { value.to_string() };
                if clean_value.contains('/') && clean_value.starts_with('$') {
                    let clean_path = clean_value.replace("$HOME", "$ENV{'HOME'}");
                    if clean_path.contains('/') {
                        let path_parts: Vec<&str> = clean_path.split('/').collect();
                        if path_parts.len() == 2 && path_parts[0] == "$ENV{'HOME'}" {
                            format!("($ENV{{'HOME'}} . '/{}') eq ($ENV{{'HOME'}} . '/{}')", path, path_parts[1])
                        } else {
                            format!("($ENV{{'HOME'}} . '/{}') eq {}", path, clean_path)
                        }
                    } else {
                        format!("($ENV{{'HOME'}} . '/{}') eq {}", path, clean_path)
                    }
                } else {
                    format!("($ENV{{'HOME'}} . '/{}') eq {}", path, clean_value)
                }
            } else {
                format!("{} eq {}", var, value)
            }
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -lt ") {
        // Numeric less than: [[ $var -lt 2 ]]
        let parts: Vec<&str> = expr.split(" -lt ").collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();
            let left_perl = convert_shell_var_to_perl(left);
            let mut right_perl = convert_shell_var_to_perl(right);

            // Replace magic numbers with constants
            for (const_name, value) in &generator.constants {
                let value_str = value.to_string();
                right_perl = right_perl.replace(&value_str, &format!("${}", const_name));
            }

            format!("({} < {})", left_perl, right_perl)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -le ") {
        // Numeric less than or equal: [[ $var -le 2 ]]
        let parts: Vec<&str> = expr.split(" -le ").collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();
            let left_perl = convert_shell_var_to_perl(left);
            let mut right_perl = convert_shell_var_to_perl(right);

            // Replace magic numbers with constants
            for (const_name, value) in &generator.constants {
                let value_str = value.to_string();
                right_perl = right_perl.replace(&value_str, &format!("${}", const_name));
            }

            format!("({} <= {})", left_perl, right_perl)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -gt ") {
        // Numeric greater than: [[ $var -gt 2 ]]
        let parts: Vec<&str> = expr.split(" -gt ").collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();
            let left_perl = convert_shell_var_to_perl(left);
            let mut right_perl = convert_shell_var_to_perl(right);

            // Replace magic numbers with constants
            for (const_name, value) in &generator.constants {
                let value_str = value.to_string();
                right_perl = right_perl.replace(&value_str, &format!("${}", const_name));
            }

            format!("({} > {})", left_perl, right_perl)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -ge ") {
        // Numeric greater than or equal: [[ $var -ge 2 ]]
        let parts: Vec<&str> = expr.split(" -ge ").collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();
            let left_perl = convert_shell_var_to_perl(left);
            let mut right_perl = convert_shell_var_to_perl(right);

            // Replace magic numbers with constants
            for (const_name, value) in &generator.constants {
                let value_str = value.to_string();
                right_perl = right_perl.replace(&value_str, &format!("${}", const_name));
            }

            format!("({} >= {})", left_perl, right_perl)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -eq ") {
        // Numeric equality: [[ $var -eq 2 ]]
        let parts: Vec<&str> = expr.split(" -eq ").collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();
            let left_perl = convert_shell_var_to_perl(left);
            let mut right_perl = convert_shell_var_to_perl(right);

            // Replace magic numbers with constants
            for (const_name, value) in &generator.constants {
                let value_str = value.to_string();
                right_perl = right_perl.replace(&value_str, &format!("${}", const_name));
            }

            format!("({} == {})", left_perl, right_perl)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -ne ") {
        // Numeric inequality: [[ $var -ne 2 ]]
        let parts: Vec<&str> = expr.split(" -ne ").collect();
        if parts.len() == 2 {
            let left = parts[0].trim();
            let right = parts[1].trim();
            let left_perl = convert_shell_var_to_perl(left);
            let mut right_perl = convert_shell_var_to_perl(right);

            // Replace magic numbers with constants
            for (const_name, value) in &generator.constants {
                let value_str = value.to_string();
                right_perl = right_perl.replace(&value_str, &format!("${}", const_name));
            }

            format!("({} != {})", left_perl, right_perl)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -z ") || starts_with_op(expr, "-z") {
        // String is empty: [[ -z $var ]]
        let var = if expr.starts_with("-z ") {
            expr.replacen("-z ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-z""#) || expr.starts_with("-z$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-z ", "", 1).trim().to_string()
        };
        format!("{} eq q{{}}", var)
    } else if expr.contains(" -n ") || starts_with_op(expr, "-n") {
        // String is not empty: [[ -n $var ]]
        let var = if expr.starts_with("-n ") {
            expr.replacen("-n ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-n""#) || expr.starts_with("-n$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-n ", "", 1).trim().to_string()
        };
        format!("{} ne q{{}}", var)
    } else if expr.contains(" -f ") || starts_with_op(expr, "-f") {
        // File exists and is regular file: [[ -f $var ]]
        let var = if expr.starts_with("-f ") {
            expr.replacen("-f ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-f""#) || expr.starts_with("-f$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-f ", "", 1).trim().to_string()
        };
        format!("(-f {})", var)
    } else if expr.contains(" -d ") || starts_with_op(expr, "-d") {
        // File exists and is directory: [[ -d $var ]]
        let var = if expr.starts_with("-d ") {
            expr.replacen("-d ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-d""#) || expr.starts_with("-d$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-d ", "", 1).trim().to_string()
        };
        format!("(-d {})", var)
    } else if expr.contains(" -e ") || starts_with_op(expr, "-e") {
        // File exists: [[ -e $var ]]
        let var = if expr.starts_with("-e ") {
            expr.replacen("-e ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-e""#) || expr.starts_with("-e$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-e ", "", 1).trim().to_string()
        };
        format!("(-e {})", var)
    } else if expr.contains(" -r ") || starts_with_op(expr, "-r") {
        // File is readable: [[ -r $var ]]
        let var = if expr.starts_with("-r ") {
            expr.replacen("-r ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-r""#) || expr.starts_with("-r$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-r ", "", 1).trim().to_string()
        };
        format!("(-r {})", var)
    } else if expr.contains(" -w ") || starts_with_op(expr, "-w") {
        // File is writable: [[ -w $var ]]
        let var = if expr.starts_with("-w ") {
            expr.replacen("-w ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-w""#) || expr.starts_with("-w$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-w ", "", 1).trim().to_string()
        };
        format!("(-w {})", var)
    } else if expr.contains(" -x ") || starts_with_op(expr, "-x") {
        // File is executable: [[ -x $var ]]
        let var = if expr.starts_with("-x ") {
            expr.replacen("-x ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-x""#) || expr.starts_with("-x$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-x ", "", 1).trim().to_string()
        };
        format!("(-x {})", var)
    } else if expr.contains(" -s ") || starts_with_op(expr, "-s") {
        // File exists and has size greater than 0: [[ -s $var ]]
        let var = if expr.starts_with("-s ") {
            expr.replacen("-s ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-s""#) || expr.starts_with("-s$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-s ", "", 1).trim().to_string()
        };
        format!("((-s {}) > 0)", var)
    } else if expr.contains(" -L ") || starts_with_op(expr, "-L") {
        // File exists and is symbolic link: [[ -L $var ]]
        let var = if expr.starts_with("-L ") {
            expr.replacen("-L ", "", 1).trim().to_string()
        } else if expr.starts_with(r#"-L""#) || expr.starts_with("-L$") {
            expr[2..].trim().to_string()
        } else {
            expr.replacen("-L ", "", 1).trim().to_string()
        };
        format!("(-l {})", var)
    } else {
        // Default case: treat as a simple boolean expression
        // This handles cases like [[ $var ]] which should check if $var is non-empty

        // Replace magic numbers with constants first
        let mut result = expr.to_string();
        for (const_name, value) in &generator.constants {
            let value_str = value.to_string();
            result = result.replace(&value_str, const_name);
        }

        // Check for $(...) command substitution patterns and convert to Perl qx{}
        // This handles cases like [ "$(cmd)" ] where the command substitution
        // was parsed as literal text inside the test expression.
        if result.contains("$(") && !result.contains("$((") {
            // Extract the command inside $(...) and wrap in qx{}
            let trimmed = result.trim();
            // Remove surrounding quotes if any
            let inner = if (trimmed.starts_with('"') && trimmed.ends_with('"'))
                || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
            {
                &trimmed[1..trimmed.len()-1]
            } else {
                trimmed
            };
            // Replace $(cmd) with qx'cmd' (single-quote delimiters to prevent
            // Perl interpolation of shell variable references like $var)
            let mut qx_expr = inner.to_string();
            // Simple replacement: find $( and matching )
            let mut depth = 0i32;
            let mut start = None;
            let mut result_chars: Vec<char> = Vec::new();
            for ch in qx_expr.chars() {
                if ch == '$' && start.is_none() {
                    start = Some(result_chars.len());
                    result_chars.push(ch);
                } else if ch == '(' && start.is_some() {
                    depth += 1;
                    result_chars.push(ch);
                } else if ch == ')' && start.is_some() {
                    depth -= 1;
                    result_chars.push(ch);
                    if depth == 0 {
                        // Found matching $(...) - replace from start to here with qx'...'
                        let cmd_start = start.unwrap() + 2; // skip $(
                        let cmd_end = result_chars.len() - 1; // skip )
                        let cmd: String = result_chars[cmd_start..cmd_end].iter().collect();
                        // Replace $(cmd) with qx'cmd'
                        // Use qx'...' (single-quote delimiters) so that
                        // shell variables like $var are NOT interpolated
                        // by Perl but are passed literally to the shell.
                        let replacement = format!("qx'{}'", cmd);
                        result_chars.truncate(start.unwrap());
                        for c in replacement.chars() {
                            result_chars.push(c);
                        }
                        start = None;
                    }
                } else {
                    result_chars.push(ch);
                }
            }
            let final_expr: String = result_chars.iter().collect();
            format!("({} ne q{{}})", final_expr)
        } else if result.trim().starts_with('$') {
            format!("({} ne q{{}})", result.trim())
        } else {
            format!("({})", result)
        }
    }
}

pub fn generate_test_command_impl(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    output: &mut String,
) {
    // Handle test command: test expression or [ expression ]
    if cmd.name == "test" || cmd.name == "[" {
        if cmd.args.is_empty() {
            output.push_str("0");
            return;
        }

        // Convert test arguments to a test expression
        let test_expr = generator.convert_test_args_to_expression(&cmd.args);
        let result = generator.generate_test_expression(&test_expr);
        output.push_str(&result);
    } else {
        // Not a test command
        output.push_str("0");
    }
}

// Helper methods for test expressions
pub fn convert_extglob_to_perl_regex_impl(generator: &Generator, pattern: &str) -> String {
    // Convert extglob patterns to Perl regex
    let mut result = pattern.to_string();

    // Debug output
    //     eprintln!("DEBUG: convert_extglob_to_perl_regex called with pattern: '{}'", pattern);

    // Handle @(pattern1|pattern2) - one of the patterns
    result = result.replace("@(", "(?:");
    result = result.replace(")", ")");

    // Handle *(pattern1|pattern2) - zero or more of the patterns
    result = result.replace("*(pattern1|pattern2)", "(?:pattern1|pattern2)*");

    // Handle +(pattern1|pattern2) - one or more of the patterns
    result = result.replace("+(pattern1|pattern2)", "(?:pattern1|pattern2)+");

    // Handle ?(pattern1|pattern2) - zero or one of the patterns
    result = result.replace("?(pattern1|pattern2)", "(?:pattern1|pattern2)?");

    // Handle !(pattern1|pattern2) - anything except the patterns
    // This is the key fix: !(*.min).js should become (?!.*\.min\.js).*\.js
    // Handle patterns with extra spaces like "! ( * . min . js"
    if result.contains("!") && result.contains("(") {
        //         eprintln!("DEBUG: Found ! and ( in pattern: '{}'", result);
        // Find the ! and ( positions, handling extra spaces
        if let Some(bang_pos) = result.find("!") {
            // Look for ( after !, allowing for spaces
            let after_bang = &result[bang_pos..];
            if let Some(paren_open) = after_bang.find("(") {
                let actual_open = bang_pos + paren_open;

                // Look for the closing parenthesis, but be more flexible
                // The pattern might be incomplete due to parser issues
                if let Some(paren_close) = result[actual_open..].find(")") {
                    let actual_close = actual_open + paren_close;

                    //                     eprintln!("DEBUG: Found ! at {}, ( at {}, ) at {}", bang_pos, actual_open, actual_close);

                    // Extract the pattern inside !(...) and after it
                    let negated_pattern = &result[actual_open + 1..actual_close];
                    let after_pattern = &result[actual_close + 1..];

                    //                     eprintln!("DEBUG: negated_pattern: '{}', after_pattern: '{}'", negated_pattern, after_pattern);

                    // Convert the negated pattern to regex
                    let negated_regex = convert_glob_to_regex_impl(generator, negated_pattern);
                    let after_regex = convert_glob_to_regex_impl(generator, after_pattern);

                    //                     eprintln!("DEBUG: negated_regex: '{}', after_regex: '{}'", negated_regex, after_regex);

                    // Create negative lookahead: (?!negated_pattern)after_pattern
                    result = format!("(?!{}){}", negated_regex, after_regex);
                    //                     eprintln!("DEBUG: Final result: '{}'", result);
                    return result;
                } else {
                    // No closing parenthesis found, but we have !(... pattern
                    // This suggests the parser didn't complete the pattern
                    //                     eprintln!("DEBUG: No closing parenthesis found, treating as incomplete extglob");

                    // Try to split the pattern to find the negated part and the after part
                    // For example: "*.min.js" should be split into "*.min" and ".js"
                    let pattern_after_open = &result[actual_open + 1..];

                    // Look for common patterns like "*.min.js" -> split into "*.min" and ".js"
                    if let Some(last_dot_pos) = pattern_after_open.rfind('.') {
                        let negated_pattern = &pattern_after_open[..last_dot_pos];
                        let after_pattern = &pattern_after_open[last_dot_pos..];

                        //                         eprintln!("DEBUG: Split pattern - negated_pattern: '{}', after_pattern: '{}'", negated_pattern, after_pattern);

                        // Convert the negated pattern to regex
                        let negated_regex = convert_glob_to_regex_impl(generator, negated_pattern);
                        let after_regex = convert_glob_to_regex_impl(generator, after_pattern);

                        //                         eprintln!("DEBUG: negated_regex: '{}', after_regex: '{}'", negated_regex, after_regex);

                        // Create negative lookahead with after pattern: ^(?!negated_pattern).*after_pattern$
                        result = format!("^(?!{}).*{}$", negated_regex, after_regex);
                        //                         eprintln!("DEBUG: Final result: '{}'", result);
                        return result;
                    } else {
                        // No dot found, treat the whole pattern as negated
                        let negated_pattern = pattern_after_open;
                        let _after_pattern = "";

                        //                         eprintln!("DEBUG: No dot found - negated_pattern: '{}', after_pattern: '{}'", negated_pattern, after_pattern);

                        // Convert the negated pattern to regex
                        let negated_regex = convert_glob_to_regex_impl(generator, negated_pattern);

                        //                         eprintln!("DEBUG: negated_regex: '{}'", negated_regex);

                        // Create negative lookahead: ^(?!negated_pattern).*$
                        result = format!("^(?!{}).*$", negated_regex);
                        //                         eprintln!("DEBUG: Final result: '{}'", result);
                        return result;
                    }
                }
            }
        }
    }

    //     eprintln!("DEBUG: No extglob pattern found, escaping special characters");

    // Escape regex special characters
    result = result.replace(".", "\\.");
    result = result.replace("+", "\\+");
    result = result.replace("*", "\\*");
    result = result.replace("?", "\\?");
    result = result.replace("^", "\\^");
    result = result.replace("$", "\\$");
    result = result.replace("[", "\\[");
    result = result.replace("]", "\\]");
    result = result.replace("(", "\\(");
    result = result.replace(")", "\\)");
    result = result.replace("|", "\\|");

    //     eprintln!("DEBUG: Final escaped result: '{}'", result);
    result
}

pub fn convert_glob_to_regex_impl(_generator: &Generator, pattern: &str) -> String {
    let mut result = pattern.to_string();

    // Debug output
    //     eprintln!("DEBUG: convert_glob_to_regex called with pattern: '{}'", pattern);

    // Normalize the pattern by removing extra spaces around glob characters
    // This handles cases where the parser adds spaces like "* . txt" -> "*.txt"
    result = result.replace(" * ", "*");
    result = result.replace(" *", "*");
    result = result.replace("* ", "*");
    result = result.replace(" ? ", "?");
    result = result.replace(" ?", "?");
    result = result.replace("? ", "?");
    result = result.replace(" . ", ".");
    result = result.replace(" .", ".");
    result = result.replace(". ", ".");

    //     eprintln!("DEBUG: After normalization: '{}'", result);

    // Escape regex special characters BEFORE converting glob patterns
    // This ensures that literal dots and other special chars are escaped first
    result = result.replace(".", "\\.");
    result = result.replace("+", "\\+");
    result = result.replace("(", "\\(");
    result = result.replace(")", "\\)");
    result = result.replace("[", "\\[");
    result = result.replace("]", "\\]");
    result = result.replace("^", "\\^");
    result = result.replace("$", "\\$");
    result = result.replace("|", "\\|");

    //     eprintln!("DEBUG: After escaping: '{}'", result);

    // Convert glob patterns to regex AFTER escaping
    // This ensures that * and ? are converted to regex patterns, not escaped
    result = result.replace("*", ".*");
    result = result.replace("?", ".");

    //     eprintln!("DEBUG: After glob conversion: '{}'", result);

    result
}

pub fn convert_test_args_to_expression_impl(
    _generator: &Generator,
    args: &[Word],
) -> TestExpression {
    // Convert test command arguments to a test expression string
    let mut expr_parts = Vec::new();

    for arg in args {
        match arg {
            Word::Literal(s, _) => expr_parts.push(s.clone()),
            Word::Array(_, elements, _) => {
                // Handle array arguments
                let array_expr = format!("@{{{}}}", elements.join(", "));
                expr_parts.push(array_expr);
            }
            _ => expr_parts.push(format!("{:?}", arg)),
        }
    }

    let expression = expr_parts.join(" ");

    TestExpression {
        expression,
        modifiers: TestModifiers {
            extglob: false,
            nocasematch: false,
            dotglob: false,
            failglob: false,
            globstar: false,
            nullglob: false,
        },
    }
}
