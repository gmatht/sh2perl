use crate::ast::*;
use super::Generator;

pub fn generate_parameter_expansion_impl(_generator: &mut Generator, pe: &ParameterExpansion) -> String {
    match &pe.operator {
        ParameterExpansionOperator::None => {
            // ${var} - just the variable
            // Check if this contains array access patterns like arr[1] or map[foo]
            if pe.variable.contains('[') && pe.variable.contains(']') {
                if let Some(bracket_start) = pe.variable.find('[') {
                    if let Some(bracket_end) = pe.variable.rfind(']') {
                        let var_name = &pe.variable[..bracket_start];
                        let key = &pe.variable[bracket_start + 1..bracket_end];
                        
                        // Check if the key is numeric (indexed array) or string (associative array)
                        if key.parse::<usize>().is_ok() {
                            // Indexed array access: arr[1] -> $arr[1]
                            format!("${}[{}]", var_name, key)
                        } else {
                            // Associative array access: map[foo] -> $map{foo}
                            format!("${}{{{}}}", var_name, key)
                        }
                    } else {
                        format!("${{{}}}", pe.variable)
                    }
                } else {
                    format!("${{{}}}", pe.variable)
                }
            } else {
                format!("${{{}}}", pe.variable)
            }
        }
        ParameterExpansionOperator::DefaultValue(default) => {
            // ${var:-default} - use default if var is empty
            format!("defined ${{{}}} && ${{{}}} ne q{{}} ? ${{{}}} : '{}'", 
                   pe.variable, pe.variable, pe.variable, default)
        }
        ParameterExpansionOperator::AssignDefault(default) => {
            // ${var:=default} - assign default if var is empty
            format!("defined ${{{}}} && ${{{}}} ne q{{}} ? ${{{}}} : do {{ ${{{}}} = '{}'; ${{{}}} }}", 
                   pe.variable, pe.variable, pe.variable, pe.variable, default, pe.variable)
        }
        ParameterExpansionOperator::ErrorIfUnset(error) => {
            // ${var:?error} - error if var is empty
            format!("defined ${{{}}} && ${{{}}} ne q{{}} ? ${{{}}} : die('{}')", 
                   pe.variable, pe.variable, pe.variable, error)
        }
        ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
            // ${var%suffix} - remove shortest suffix
            format!("${{{}}} =~ s/{}$//r", pe.variable, escape_regex_pattern(pattern))
        }
        ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
            // ${var%%suffix} - remove longest suffix
            format!("${{{}}} =~ s/{}$//grs", pe.variable, escape_regex_pattern(pattern))
        }
        ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
            // ${var#prefix} - remove shortest prefix
            format!("${{{}}} =~ s/^{}//r", pe.variable, escape_regex_pattern(pattern))
        }
        ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
            // ${var##prefix} - remove longest prefix
            format!("${{{}}} =~ s/^{}//grs", pe.variable, escape_regex_pattern(pattern))
        }
        ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
            // ${var//pattern/replacement} - substitute all occurrences
            format!("${{{}}} =~ s/{}/{}/grs", 
                   pe.variable, 
                   escape_regex_pattern(pattern),
                   escape_regex_replacement(replacement))
        }
        ParameterExpansionOperator::UppercaseAll => {
            // ${var^^} - uppercase all characters
            format!("uc(${{{}}})", pe.variable)
        }
        ParameterExpansionOperator::LowercaseAll => {
            // ${var,,} - lowercase all characters
            format!("lc(${{{}}})", pe.variable)
        }
        ParameterExpansionOperator::UppercaseFirst => {
            // ${var^} - uppercase first character
            format!("ucfirst(${{{}}})", pe.variable)
        }
        ParameterExpansionOperator::Basename => {
            // ${var##*/} - get basename
            format!("basename(${{{}}})", pe.variable)
        }
        ParameterExpansionOperator::Dirname => {
            // ${var%/*} - get dirname
            format!("dirname(${{{}}})", pe.variable)
        }
        ParameterExpansionOperator::ArraySlice(offset, length) => {
            // Special case: ${#arr[@]} should be array length, not array slice
            if pe.variable.starts_with('#') && offset == "@" && length.is_none() {
                // ${#arr[@]} -> scalar(@arr)
                let array_name = &pe.variable[1..]; // Remove the '#' prefix
                format!("scalar(@{})", array_name)
            } else if offset == "@" && length.is_none() {
                // ${map[@]} or ${!map[@]} - this represents array/map values or keys
                if pe.variable.starts_with('!') {
                    // ${!map[@]} -> keys %map (map keys iteration)
                    let map_name = &pe.variable[1..]; // Remove ! prefix
                    format!("keys %{}", map_name)
                } else {
                    // ${map[@]} -> @map (array iteration)
                    format!("@{}", pe.variable)
                }
            } else {
                // ${var:offset:length} - array slice
                if let Some(length_str) = length {
                    format!("@${{{}}}[{}..{}]", pe.variable, offset, length_str)
                } else {
                    format!("@${{{}}}[{}..]", pe.variable, offset)
                }
            }
        }
    }
}

// Helper methods for regex escaping
fn escape_regex_pattern(pattern: &str) -> String {
    // Escape special regex characters in the pattern
    pattern.replace("\\", "\\\\")
           .replace(".", "\\.")
           .replace("+", "\\+")
           .replace("*", "\\*")
           .replace("?", "\\?")
           .replace("^", "\\^")
           .replace("$", "\\$")
           .replace("[", "\\[")
           .replace("]", "\\]")
           .replace("(", "\\(")
           .replace(")", "\\)")
           .replace("|", "\\|")
}

fn escape_regex_replacement(replacement: &str) -> String {
    // Escape special regex characters in the replacement string
    replacement.replace("\\", "\\\\")
               .replace("$", "\\$")
               .replace("&", "\\&")
               .replace("`", "\\`")
               .replace("'", "\\'")
}
