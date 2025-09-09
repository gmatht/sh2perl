use crate::ast::*;
use super::Generator;

// Helper function to convert shell variables to Perl equivalents
fn convert_shell_var_to_perl(var: &str) -> String {
    match var {
        "$#" => "scalar(@ARGV)".to_string(),  // $# -> scalar(@ARGV) for argument count
        "$@" => "@ARGV".to_string(),          // $@ -> @ARGV for arguments array
        "$*" => "@ARGV".to_string(),          // $* -> @ARGV for arguments array
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

pub fn generate_test_expression_impl(generator: &mut Generator, test_expr: &TestExpression) -> String {
    // Parse the test expression to extract components
    let expr = &test_expr.expression;
    let modifiers = &test_expr.modifiers;
    
    // Parse the expression to determine the type of test
    if expr.contains("=~") {
        // Regex matching: [[ $var =~ pattern ]]
        let parts: Vec<&str> = expr.split("=~").collect();
        if parts.len() == 2 {
            let var = parts[0].trim();
            let pattern = parts[1].trim();
            
            // Convert to Perl regex matching
            format!("{} =~ {}", var, generator.format_regex_pattern(pattern))
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
                // Handle extglob patterns
                let regex_pattern = generator.convert_extglob_to_perl_regex(pattern);
                if modifiers.nocasematch {
                    format!("{} =~ {}i", var, generator.format_regex_pattern(&regex_pattern))
                } else {
                    format!("{} =~ {}", var, generator.format_regex_pattern(&regex_pattern))
                }
            } else {
                // Regular glob pattern matching - convert glob to regex
                let regex_pattern = generator.convert_glob_to_regex(pattern);
                if modifiers.nocasematch {
                    // Case-insensitive matching
                    format!("{} =~ {}i", var, generator.format_regex_pattern(&format!("^{}$", regex_pattern)))
                } else {
                    // Case-sensitive matching
                    format!("{} =~ {}", var, generator.format_regex_pattern(&format!("^{}$", regex_pattern)))
                }
            }
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
                // Remove quotes from value if it's a shell variable reference
                let clean_value = if value.starts_with('"') && value.ends_with('"') && value.contains('$') {
                    let unquoted = value[1..value.len()-1].to_string();
                    // Convert shell variables to Perl environment variables
                    if unquoted == "$HOME" {
                        "$ENV{'HOME'}".to_string()
                    } else {
                        unquoted
                    }
                } else {
                    value.to_string()
                };
                format!("$ENV{{'HOME'}} eq {}", clean_value)
            } else if var.starts_with("~/") {
                let path = var[2..].to_string();
                // Remove quotes from value if it's a shell variable reference
                let clean_value = if value.starts_with('"') && value.ends_with('"') && value.contains('$') {
                    let unquoted = value[1..value.len()-1].to_string();
                    // Convert shell variables to Perl environment variables
                    if unquoted == "$HOME" {
                        "$ENV{'HOME'}".to_string()
                    } else {
                        unquoted
                    }
                } else {
                    value.to_string()
                };
                
                // Handle the case where the value is a path that should be concatenated
                if clean_value.contains('/') && clean_value.starts_with('$') {
                    // Convert $HOME/Documents to $ENV{'HOME'} . '/Documents'
                    let clean_path = clean_value.replace("$HOME", "$ENV{'HOME'}");
                    // Split the path and reconstruct it properly
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
                // Regular variable equality
                format!("{} eq {}", var, value)
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
    } else if expr.contains(" -eq ") {
        // Numeric equality: [[ $var -eq value ]]
        let parts: Vec<&str> = expr.split(" -eq ").collect();
        if parts.len() == 2 {
            let var = parts[0].trim();
            let value = parts[1].trim();
            format!("{} == {}", var, value)
        } else {
            "0".to_string()
        }
    } else if expr.contains(" -ne ") {
        // Numeric inequality: [[ $var -ne value ]]
        let parts: Vec<&str> = expr.split(" -ne ").collect();
        if parts.len() == 2 {
            let var = parts[0].trim();
            let value = parts[1].trim();
            format!("{} != {}", var, value)
        } else {
            "0".to_string()
        }

    } else if expr.contains(" -z ") {
        // String is empty: [[ -z $var ]]
        let var = expr.replace("-z ", "").trim().to_string();
        format!("{} eq q{{}}", var)
    } else if expr.contains(" -n ") {
        // String is not empty: [[ -n $var ]]
        let var = expr.replace("-n ", "").trim().to_string();
        format!("{} ne q{{}}", var)
    } else if expr.contains(" -f ") || expr.starts_with("-f ") {
        // File exists and is regular file: [[ -f $var ]]
        let var = expr.replace("-f ", "").trim().to_string();
        format!("(-f {})", var)
    } else if expr.contains(" -d ") || expr.starts_with("-d ") {
        // File exists and is directory: [[ -d $var ]]
        let var = expr.replace("-d ", "").trim().to_string();
        format!("(-d {})", var)
    } else if expr.contains(" -e ") || expr.starts_with("-e ") {
        // File exists: [[ -e $var ]]
        let var = expr.replace("-e ", "").trim().to_string();
        format!("(-e {})", var)
    } else if expr.contains(" -r ") || expr.starts_with("-r ") {
        // File is readable: [[ -r $var ]]
        let var = expr.replace("-r ", "").trim().to_string();
        format!("(-r {})", var)
    } else if expr.contains(" -w ") || expr.starts_with("-w ") {
        // File is writable: [[ -w $var ]]
        let var = expr.replace("-w ", "").trim().to_string();
        format!("(-w {})", var)
    } else if expr.contains(" -x ") || expr.starts_with("-x ") {
        // File is executable: [[ -x $var ]]
        let var = expr.replace("-x ", "").trim().to_string();
        format!("(-x {})", var)
    } else if expr.contains(" -s ") || expr.starts_with("-s ") {
        // File exists and has size greater than 0: [[ -s $var ]]
        let var = expr.replace("-s ", "").trim().to_string();
        format!("((-s {}) > 0)", var)
    } else if expr.contains(" -L ") {
        // File exists and is symbolic link: [[ -L $var ]]
        let var = expr.replace("-L ", "").trim().to_string();
        format!("(-l {})", var)
    } else if expr.contains(" -S ") {
        // File exists and is socket: [[ -S $var ]]
        let var = expr.replace("-S ", "").trim().to_string();
        format!("(-S {})", var)
    } else if expr.contains(" -p ") {
        // File exists and is named pipe: [[ -p $var ]]
        let var = expr.replace("-p ", "").trim().to_string();
        format!("(-p {})", var)
    } else if expr.contains(" -b ") {
        // File exists and is block device: [[ -b $var ]]
        let var = expr.replace("-b ", "").trim().to_string();
        format!("(-b {})", var)
    } else if expr.contains(" -c ") {
        // File exists and is character device: [[ -c $var ]]
        let var = expr.replace("-c ", "").trim().to_string();
        format!("(-c {})", var)
    } else if expr.contains(" -t ") {
        // File descriptor is terminal: [[ -t $var ]]
        let var = expr.replace("-t ", "").trim().to_string();
        format!("(-t {})", var)
    } else if expr.contains(" -u ") {
        // File exists and set-user-id bit is set: [[ -u $var ]]
        let var = expr.replace("-u ", "").trim().to_string();
        format!("(-u {})", var)
    } else if expr.contains(" -g ") {
        // File exists and set-group-id bit is set: [[ -g $var ]]
        let var = expr.replace("-g ", "").trim().to_string();
        format!("(-g {})", var)
    } else if expr.contains(" -k ") {
        // File exists and sticky bit is set: [[ -k $var ]]
        let var = expr.replace("-k ", "").trim().to_string();
        format!("(-k {})", var)
    } else if expr.contains(" -O ") {
        // File exists and is owned by effective user ID: [[ -O $var ]]
        let var = expr.replace("-O ", "").trim().to_string();
        format!("(-O {})", var)
    } else if expr.contains(" -G ") {
        // File exists and is owned by effective group ID: [[ -G $var ]]
        let var = expr.replace("-G ", "").trim().to_string();
        format!("(-G {})", var)
    } else if expr.contains(" -N ") {
        // File exists and has been modified since it was last read: [[ -N $var ]]
        let var = expr.replace("-N ", "").trim().to_string();
        format!("(-N {})", var)
    } else if expr.contains(" -h ") || expr.contains(" -L ") {
        // File exists and is symbolic link: [[ -h $var ]] or [[ -L $var ]]
        let var = if expr.contains("-h ") {
            expr.replace("-h ", "").trim().to_string()
        } else {
            expr.replace("-L ", "").trim().to_string()
        };
        format!("(-l {})", var)
    } else if expr.contains(" -a ") {
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
    } else {
        // Default case: treat as a simple boolean expression
        // This handles cases like [[ $var ]] which should check if $var is non-empty
        
        // Replace magic numbers with constants first
        let mut result = expr.to_string();
        for (const_name, value) in &generator.constants {
            let value_str = value.to_string();
            result = result.replace(&value_str, const_name);
        }
        
        if result.trim().starts_with('$') {
            format!("({} ne q{{}})", result.trim())
        } else {
            format!("({})", result)
        }
    }
}

pub fn generate_test_command_impl(generator: &mut Generator, cmd: &SimpleCommand, output: &mut String) {
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

pub fn convert_test_args_to_expression_impl(_generator: &Generator, args: &[Word]) -> TestExpression {
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
