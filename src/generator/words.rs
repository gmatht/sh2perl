use crate::ast::*;
use crate::mir::*;
use super::Generator;
use regex::Regex;

pub fn word_to_perl_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s) => {
            // Handle literal strings
            if s.contains("..") {
                generator.handle_range_expansion(s)
            } else if s.contains(',') {
                generator.handle_comma_expansion(s)
            } else {
                s.clone()
            }
        },
        Word::ParameterExpansion(pe) => generator.generate_parameter_expansion(pe),
        Word::Array(name, elements) => {
            let elements_str = elements.iter()
                .map(|e| format!("'{}'", e.replace("'", "\\'")))
                .collect::<Vec<_>>()
                .join(", ");
            format!("@{} = ({});", name, elements_str)
        },
        Word::StringInterpolation(interp) => generator.convert_string_interpolation_to_perl(interp),
        Word::Arithmetic(expr) => generator.convert_arithmetic_to_perl(&expr.expression),
        Word::BraceExpansion(expansion) => {
            let expanded = generator.handle_brace_expansion(expansion);
            // Quote the result since it's used in contexts where quotes are needed
            format!("\"{}\"", expanded)
        },
        Word::CommandSubstitution(_cmd) => {
            // TEMPORARY DEBUG: Just return a simple string to test if this case is being matched
            "\"[COMMAND_SUBSTITUTION_MATCHED]\"".to_string()
        },
        Word::Variable(var) => {
            // Handle special shell variables
            match var.as_str() {
                "#" => "scalar(@ARGV)".to_string(),  // $# -> scalar(@ARGV) for argument count
                "@" => "@ARGV".to_string(),          // $@ -> @ARGV for arguments array
                "*" => "@ARGV".to_string(),          // $* -> @ARGV for arguments array
                _ => format!("${}", var)             // Regular variable
            }
        },
        Word::MapAccess(map_name, key) => {
            // Handle array/map access like arr[1] or map[foo]
            // Check if the key is numeric (indexed array) or string (associative array)
            if key.parse::<usize>().is_ok() {
                // Indexed array access: arr[1] -> $arr[1]
                format!("${}[{}]", map_name, key)
            } else {
                // Associative array access: map[foo] -> $map{foo}
                format!("${}{{{}}}", map_name, key)
            }
        },
        Word::MapKeys(map_name) => {
            // Handle map keys like !map[@] -> keys %map
            format!("keys %{}", map_name)
        },
        Word::MapLength(map_name) => {
            // Handle array length like #arr[@] -> scalar(@arr)
            format!("scalar(@{})", map_name)
        },
        Word::ArraySlice(array_name, offset, length) => {
            // Handle array slicing like arr[@]:1:3 -> @arr[1..3]
            if let Some(length_str) = length {
                format!("@{}[{}..{}]", array_name, offset, length_str)
            } else {
                format!("@{}[{}..]", array_name, offset)
            }
        },
        _ => format!("{:?}", word)
    }
}

pub fn word_to_perl_for_test_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s) => s.clone(),
        Word::ParameterExpansion(pe) => generator.generate_parameter_expansion(pe),
        _ => format!("{:?}", word)
    }
}

// Helper methods
pub fn handle_range_expansion_impl(_generator: &Generator, s: &str) -> String {
    let parts: Vec<&str> = s.split("..").collect();
    if parts.len() == 2 {
        if let (Ok(start), Ok(end)) = (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
            let values: Vec<String> = (start..=end)
                .map(|i| i.to_string())
                .collect();
            // Format as Perl array: (1, 2, 3, 4, 5)
            format!("({})", values.join(", "))
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    }
}

pub fn handle_comma_expansion_impl(_generator: &Generator, s: &str) -> String {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() > 1 {
        parts.join(" ")
    } else {
        s.to_string()
    }
}

pub fn handle_brace_expansion_impl(generator: &mut Generator, expansion: &BraceExpansion) -> String {
    // Handle prefix and suffix
    let prefix = expansion.prefix.as_deref().unwrap_or("");
    let suffix = expansion.suffix.as_deref().unwrap_or("");
    
    if expansion.items.len() == 1 {
        let expanded = generator.word_to_perl(&generator.brace_item_to_word(&expansion.items[0]));
        if !prefix.is_empty() || !suffix.is_empty() {
            // Split the expanded items and add prefix/suffix to each
            let items: Vec<String> = expanded.split_whitespace()
                .map(|item| format!("{}{}{}", prefix, item, suffix))
                .collect();
            items.join(" ")
        } else {
            expanded
        }
    } else {
        // Handle cartesian product for multiple brace items
        let expanded_items: Vec<Vec<String>> = expansion.items.iter()
            .map(|item| {
                let word = generator.brace_item_to_word(item);
                match word {
                    Word::Literal(s) => vec![s],
                    _ => vec![generator.word_to_perl(&word)],
                }
            })
            .collect();
        
        // Generate cartesian product
        let cartesian = generate_cartesian_product(&expanded_items);
        
        // Add prefix and suffix to each item
        let items: Vec<String> = cartesian.iter()
            .map(|item| format!("{}{}{}", prefix, item, suffix))
            .collect();
        
        // Join all combinations with spaces
        items.join(" ")
    }
}

fn generate_cartesian_product(items: &[Vec<String>]) -> Vec<String> {
    if items.is_empty() {
        return vec![];
    }
    if items.len() == 1 {
        return items[0].clone();
    }
    
    let mut result = Vec::new();
    let first = &items[0];
    let rest = generate_cartesian_product(&items[1..]);
    
    for item in first {
        for rest_item in &rest {
            result.push(format!("{}{}", item, rest_item));
        }
    }
    
    result
}

pub fn brace_item_to_word_impl(_generator: &Generator, item: &BraceItem) -> Word {
    match item {
        BraceItem::Literal(s) => Word::Literal(s.clone()),
        BraceItem::Range(range) => {
            // Expand the range to actual values
            let expanded = expand_range(range);
            Word::Literal(expanded)
        },
        BraceItem::Sequence(seq) => Word::Literal(seq.join(" ")),
    }
}

fn expand_range(range: &BraceRange) -> String {
    // Check if this is a numeric range
    if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i64>(), range.end.parse::<i64>()) {
        let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
        
        let mut values = Vec::new();
        let mut current = start_num;
        
        if step > 0 {
            while current <= end_num {
                // Preserve leading zeros by formatting with the same width as the original
                let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                    format!("{:0width$}", current, width = range.start.len())
                } else {
                    current.to_string()
                };
                values.push(formatted);
                current += step;
            }
        } else {
            while current >= end_num {
                // Preserve leading zeros by formatting with the same width as the original
                let formatted = if range.start.starts_with('0') && range.start.len() > 1 {
                    format!("{:0width$}", current, width = range.start.len())
                } else {
                    current.to_string()
                };
                values.push(formatted);
                current += step;
            }
        }
        
        values.join(" ")
    } else {
        // Character range (e.g., a..c)
        if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
            let step = range.step.as_ref().and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
            
            let mut values = Vec::new();
            let mut current = start_char as i64;
            let end = end_char as i64;
            
            if step > 0 {
                while current <= end {
                    values.push((current as u8 as char).to_string());
                    current += step;
                }
            } else {
                while current >= end {
                    values.push((current as u8 as char).to_string());
                    current += step;
                }
            }
            
            values.join(" ")
        } else {
            // Fallback: just return the range as-is
            format!("{}..{}", range.start, range.end)
        }
    }
}

pub fn convert_string_interpolation_to_perl_impl(generator: &Generator, interp: &StringInterpolation) -> String {
    // Convert string interpolation to a single Perl interpolated string
    let mut combined_string = String::new();
    
    for part in &interp.parts {
        match part {
            StringPart::Literal(s) => {
                // Add the literal text directly to the interpolated string
                combined_string.push_str(s);
            },
            StringPart::Variable(var) => {
                // Handle special shell variables
                match var.as_str() {
                    "#" => combined_string.push_str("scalar(@ARGV)"),  // $# -> scalar(@ARGV) for argument count
                    "@" => combined_string.push_str("@ARGV"),          // Arrays don't need $ in interpolation
                    "*" => combined_string.push_str("@ARGV"),          // Arrays don't need $ in interpolation
                    _ => {
                        // Check if this is a shell positional parameter ($1, $2, etc.)
                        if var.chars().all(|c| c.is_digit(10)) {
                            // Convert $1 to $_[0], $2 to $_[1], etc.
                            let index = var.parse::<usize>().unwrap_or(0);
                            combined_string.push_str(&format!("$_[{}]", index - 1)); // Perl arrays are 0-indexed
                        } else {
                            // Regular variable - add directly for interpolation
                            combined_string.push_str(&format!("${}", var));
                        }
                    }
                }
            },
            StringPart::MapAccess(map_name, key) => {
                if map_name == "map" {
                    combined_string.push_str(&format!("$map{{{}}}", key));
                } else {
                    combined_string.push_str(&format!("${}{{{}}}", map_name, key));
                }
            }
            StringPart::ParameterExpansion(pe) => {
                // Handle parameter expansions like ${arr[1]}, ${#arr[@]}, etc.
                // We need to convert the ParameterExpansion to Perl code
                // For now, let's handle the common cases directly
                
                // Check for special array operations first
                match &pe.operator {
                    ParameterExpansionOperator::ArraySlice(offset, length) => {
                        if offset == "@" {
                            // This is ${#arr[@]} or ${arr[@]} - array length or array iteration
                            if pe.variable.starts_with('#') {
                                // ${#arr[@]} -> scalar(@arr)
                                let array_name = &pe.variable[1..];
                                combined_string.push_str(&format!("scalar(@{})", array_name));
                            } else if pe.variable.starts_with('!') {
                                // ${!map[@]} -> keys %map (map keys iteration)
                                let map_name = &pe.variable[1..]; // Remove ! prefix
                                combined_string.push_str(&format!("keys %{}", map_name));
                            } else {
                                // ${arr[@]} -> @arr (for array iteration)
                                let array_name = &pe.variable;
                                combined_string.push_str(&format!("@{}", array_name));
                            }
                        } else {
                            // Regular array slice
                            if let Some(length_str) = length {
                                combined_string.push_str(&format!("@${{{}}}[{}..{}]", pe.variable, offset, length_str));
                            } else {
                                combined_string.push_str(&format!("@${{{}}}[{}..]", pe.variable, offset));
                            }
                        }
                    }
                    _ => {
                        // Handle other cases
                        if pe.variable.contains('[') && pe.variable.contains(']') {
                            if let Some(bracket_start) = pe.variable.find('[') {
                                if let Some(bracket_end) = pe.variable.rfind(']') {
                                    let var_name = &pe.variable[..bracket_start];
                                    let key = &pe.variable[bracket_start + 1..bracket_end];
                                    
                                    // Check if the key is numeric (indexed array) or string (associative array)
                                    if key.parse::<usize>().is_ok() {
                                        // Indexed array access: arr[1] -> $arr[1]
                                        combined_string.push_str(&format!("${}[{}]", var_name, key));
                                    } else {
                                        // Associative array access: map[foo] -> $map{foo}
                                        combined_string.push_str(&format!("${}{{{}}}", var_name, key));
                                    }
                                } else {
                                    combined_string.push_str(&format!("${{{}}}", pe.variable));
                                }
                            } else {
                                combined_string.push_str(&format!("${{{}}}", pe.variable));
                            }
                        } else {
                            // Simple variable reference
                            combined_string.push_str(&format!("${{{}}}", pe.variable));
                        }
                    }
                }
            }
            _ => {
                // Handle other StringPart variants by converting them to debug format for now
                combined_string.push_str(&format!("{:?}", part));
            }
        }
    }
    
    // Return as a single interpolated string
    format!("\"{}\"", combined_string)
}

pub fn convert_arithmetic_to_perl_impl(_generator: &Generator, expr: &str) -> String {
    // Convert shell arithmetic expression to Perl syntax
    let result = expr.to_string();
    
    // Convert shell variables to Perl variables (e.g., i -> $i)
    // Use regex to find variable names and replace them with Perl variable syntax
    
    // Create a regex to match variable names (letters followed by alphanumeric/underscore)
    let var_regex = Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\b").unwrap();
    
    // Replace variable names with Perl variable syntax
    let converted = var_regex.replace_all(&result, |caps: &regex::Captures| {
        let var_name = &caps[1];
        format!("${}", var_name)
    });
    
    converted.to_string()
}
