use crate::ast::*;
use super::Generator;
use regex::Regex;

/// Get the appropriate temporary directory for the current platform
pub fn get_temp_dir() -> &'static str {
    // On Windows, use $TEMP, otherwise use /tmp
    if cfg!(target_os = "windows") {
        "($ENV{TEMP} || $ENV{TMP} || \"C:\\\\temp\")"
    } else {
        "/tmp"
    }
}

pub fn extract_array_key_impl(var: &str) -> Option<(String, String)> {
    // Check if this is an associative array assignment like map[foo]=bar
    if let Some(bracket_start) = var.find('[') {
        if let Some(bracket_end) = var.find(']') {
            if bracket_start < bracket_end {
                let array_name = var[..bracket_start].to_string();
                let key = var[bracket_start + 1..bracket_end].to_string();
                return Some((array_name, key));
            }
        }
    }
    None
}

pub fn extract_array_elements_impl(value: &str) -> Option<Vec<String>> {
    // Check if this is an indexed array assignment like arr=(one two three)
    if value.starts_with('(') && value.ends_with(')') {
        let content = &value[1..value.len() - 1];
        if !content.is_empty() {
            let elements: Vec<String> = content
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            return Some(elements);
        }
    }
    None
}

pub fn perl_string_literal_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Handle empty strings with q{}
            if s.is_empty() {
                return "q{}".to_string();
            }
            
            // Check if string needs interpolation (contains variables or special chars)
            let needs_interpolation = s.contains('$') || s.contains('@') || s.contains('\\');
            
            if needs_interpolation {
                // Escape quotes and backslashes for Perl string literals
                let escaped = s.replace("\\", "\\\\")
                              .replace("\"", "\\\"")
                              .replace("\n", "\\n")
                              .replace("\t", "\\t")
                              .replace("\r", "\\r");
                format!("\"{}\"", escaped)
            } else {
                // Use single quotes for strings that don't need interpolation
                let escaped = s.replace("\\", "\\\\")
                              .replace("'", "\\'");
                format!("'{}'", escaped)
            }
        }
        Word::Variable(var, _, _) => {
            // Handle special shell variables
            match var.as_str() {
                "#" => "scalar(@ARGV)".to_string(),  // $# -> scalar(@ARGV) for argument count
                "@" => "@ARGV".to_string(),          // $@ -> @ARGV for arguments array
                _ => format!("${}", var)             // Regular variables
            }
        }
        Word::Arithmetic(expr, _) => {
            // Handle arithmetic expressions by converting them to Perl
            generator.convert_arithmetic_to_perl(&expr.expression)
        }
        Word::ParameterExpansion(pe, _) => {
            // Handle parameter expansion
            generator.generate_parameter_expansion(pe)
        }
        Word::StringInterpolation(interp, _) => {
            // Handle string interpolation
            generator.convert_string_interpolation_to_perl(interp)
        }
        Word::CommandSubstitution(cmd, _) => {
            // Handle command substitution
            match cmd.as_ref() {
                Command::Simple(simple_cmd) => {
                    let cmd_name = generator.word_to_perl(&simple_cmd.name);
                    let args: Vec<String> = simple_cmd.args.iter()
                        .map(|arg| generator.word_to_perl(arg))
                        .collect();
                    
                    // For simple commands, fall back to system command for now
                    if args.is_empty() {
                        format!("`{}`", cmd_name)
                    } else {
                        format!("`{} {}`", cmd_name, args.join(" "))
                    }
                },
                Command::Pipeline(pipeline) => {
                    // For command substitution pipelines, we need to execute the pipeline
                    // and capture its output instead of printing it
                    let pipeline_code = generator.generate_command(&Command::Pipeline(pipeline.clone()));
                    
                    // Find the actual output variable name that was generated
                    let re = Regex::new(r"\$output_(\d+)").unwrap();
                    let output_var = if let Some(cap) = re.captures(&pipeline_code) {
                        format!("$output_{}", cap.get(1).unwrap().as_str())
                    } else {
                        "$output_0".to_string()
                    };
                    
                    // Find the pipeline success variable
                    let success_var = if pipeline_code.contains("$pipeline_success_") {
                        let re = Regex::new(r"\$pipeline_success_(\d+)").unwrap();
                        if let Some(cap) = re.captures(&pipeline_code) {
                            format!("$pipeline_success_{}", cap.get(1).unwrap().as_str())
                        } else {
                            "$pipeline_success_0".to_string()
                        }
                    } else {
                        "$pipeline_success_0".to_string()
                    };
                    
                    // Remove the print statements and exit code assignment using the actual variable names
                    let mut captured_pipeline = pipeline_code
                        .replace(&format!("print {};", output_var), "")
                        .replace("print \"\\n\";", "")
                        .replace(&format!("print \"\\n\" unless {} =~ {};", output_var, generator.newline_end_regex()), "")
                        .replace(&format!("if (!{}) {{ $main_exit_code = 1; }}", success_var), "");
                    
                    // Remove conditional print blocks that are common in pipelines
                    // Use a simpler approach with string replacement for the specific pattern
                    let output_var_num = output_var.trim_start_matches("$output_");
                    let print_block_to_remove = format!(
                        "if ({} ne q{} && !defined $output_printed_{}) {{\n\n        print {};\n        print \"\\n\" unless {} =~ {};\n    }}", 
                        output_var, "", output_var_num, output_var, output_var, generator.newline_end_regex()
                    );
                    captured_pipeline = captured_pipeline.replace(&print_block_to_remove, "");
                    
                    // Also try without the extra newlines in case formatting is different
                    let print_block_compact = format!(
                        "if ({} ne q{} && !defined $output_printed_{}) {{ print {}; print \"\\n\" unless {} =~ {}; }}", 
                        output_var, "", output_var_num, output_var, output_var, generator.newline_end_regex()
                    );
                    captured_pipeline = captured_pipeline.replace(&print_block_compact, "");
                    
                    // Remove the outer braces if they exist, as we'll wrap in our own do block
                    captured_pipeline = captured_pipeline.trim().to_string();
                    if captured_pipeline.starts_with('{') && captured_pipeline.ends_with('}') {
                        captured_pipeline = captured_pipeline[1..captured_pipeline.len()-1].to_string();
                    }
                    
                    // Return the code that executes the pipeline and captures output
                    // Command substitution should convert newlines to spaces (bash behavior)
                    format!("do {{ {} chomp({}); {} =~ s/\\n/ /g; {} }}", captured_pipeline.trim(), output_var, output_var, output_var)
                },
                _ => {
                    // For other command types, use system command fallback
                    format!("`{}`", generator.generate_command_string_for_system(cmd))
                }
            }
        }
        _ => format!("{:?}", word)
    }
}

pub fn strip_shell_quotes_and_convert_to_perl_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Strip shell quotes if present and convert to Perl string literal
            let stripped = if (s.starts_with("'") && s.ends_with("'")) || (s.starts_with("\"") && s.ends_with("\"")) {
                // Remove the outer quotes
                &s[1..s.len()-1]
            } else {
                s
            };
            
            // Handle empty strings with q{}
            if stripped.is_empty() {
                return "q{}".to_string();
            }
            
            // Check if string needs interpolation (contains variables or special chars)
            let needs_interpolation = stripped.contains('$') || stripped.contains('@') || stripped.contains('\\');
            
            if needs_interpolation {
                // Escape quotes and backslashes for Perl string literals
                let escaped = stripped.replace("\\", "\\\\")
                                    .replace("\"", "\\\"")
                                    .replace("\n", "\\n")
                                    .replace("\t", "\\t")
                                    .replace("\r", "\\r");
                format!("\"{}\"", escaped)
            } else {
                // Use single quotes for strings that don't need interpolation
                let escaped = stripped.replace("\\", "\\\\")
                                    .replace("'", "\\'");
                format!("'{}'", escaped)
            }
        }
        Word::Arithmetic(expr, _) => {
            // Handle arithmetic expressions by converting them to Perl
            generator.convert_arithmetic_to_perl(&expr.expression)
        }
        Word::ParameterExpansion(pe, _) => {
            // Handle parameter expansion
            generator.generate_parameter_expansion(pe)
        }
        Word::StringInterpolation(interp, _) => {
            // Handle string interpolation
            generator.convert_string_interpolation_to_perl(interp)
        }
        _ => format!("{:?}", word)
    }
}

pub fn strip_shell_quotes_for_regex_impl(generator: &mut Generator, word: &Word) -> String {
    match word {
        Word::Literal(s, _) => {
            // Strip shell quotes if present and return the raw string for regex
            if (s.starts_with("'") && s.ends_with("'")) || (s.starts_with("\"") && s.ends_with("\"")) {
                // Remove the outer quotes
                s[1..s.len()-1].to_string()
            } else {
                s.clone()
            }
        }
        Word::Arithmetic(expr, _) => {
            // Handle arithmetic expressions by converting them to Perl
            generator.convert_arithmetic_to_perl(&expr.expression)
        }
        Word::ParameterExpansion(pe, _) => {
            // Handle parameter expansion
            generator.generate_parameter_expansion(pe)
        }
        Word::StringInterpolation(interp, _) => {
            // For regex, we need the raw content without quotes
            // For simple string interpolations with just literals, extract the raw content
            if interp.parts.len() == 1 {
                if let StringPart::Literal(s) = &interp.parts[0] {
                    // Convert shell regex patterns to Perl regex patterns
                    let mut regex_pattern = s.clone();
                    
                    // Convert shell extended regex patterns to Perl patterns
                    // Convert \+ to + (shell extended regex to Perl)
                    regex_pattern = regex_pattern.replace("\\+", "+");
                    // Convert \? to ? (shell extended regex to Perl)
                    regex_pattern = regex_pattern.replace("\\?", "?");
                    // Convert \( and \) to ( and ) (shell extended regex to Perl)
                    regex_pattern = regex_pattern.replace("\\(", "(");
                    regex_pattern = regex_pattern.replace("\\)", ")");
                    // Convert \{ and \} to { and } (shell extended regex to Perl)
                    regex_pattern = regex_pattern.replace("\\{", "{");
                    regex_pattern = regex_pattern.replace("\\}", "}");
                    // Convert \| to | (shell extended regex to Perl)
                    regex_pattern = regex_pattern.replace("\\|", "|");
                    
                    // Return the converted regex pattern
                    regex_pattern
                } else {
                    // Fall back to normal string interpolation handling
                    generator.convert_string_interpolation_to_perl(interp)
                }
            } else {
                // Fall back to normal string interpolation handling
                generator.convert_string_interpolation_to_perl(interp)
            }
        }
        _ => format!("{:?}", word)
    }
}

pub fn get_unique_file_handle_impl(generator: &mut Generator) -> String {
    generator.file_handle_counter += 1;
    format!("fh_{}", generator.file_handle_counter)
}

/// Generate a properly formatted regex pattern with appropriate flags
pub fn format_regex_pattern(pattern: &str) -> String {
    // Convert escaped metacharacters to character classes for better Perl::Critic compliance
    let converted_pattern = convert_escaped_metacharacters(pattern);
    // Add common flags: /s for dot matching newlines, /x for extended formatting, /m for multiline
    format!("/{}/msx", converted_pattern)
}

/// Convert escaped metacharacters to character classes for better Perl::Critic compliance
pub fn convert_escaped_metacharacters(pattern: &str) -> String {
    pattern.replace("\\.", "[.]")
           .replace("\\+", "[+]")
           .replace("\\*", "[*]")
           .replace("\\?", "[?]")
           .replace("\\^", "[^]")
           .replace("\\$", "[$]")
           .replace("\\[", "[\\[]")
           .replace("\\]", "[\\]]")
           .replace("\\(", "[(]")
           .replace("\\)", "[)]")
           .replace("\\|", "[|]")
}

/// Generate a regex pattern for checking if string ends with newline
pub fn newline_end_regex() -> String {
    format_regex_pattern(r"\\n$")
}

/// Convert postfix unless statement to block form
pub fn convert_postfix_unless_to_block(condition: &str, statement: &str) -> String {
    format!("if (!({}) ) {{\n    {};\n}}", condition, statement)
}

/// Convert postfix unless statement to block form with proper indentation
pub fn convert_postfix_unless_to_block_with_indent(condition: &str, statement: &str, indent: &str) -> String {
    format!("{}if (!({}) ) {{\n{}    {};\n{}}}", indent, condition, indent, statement, indent)
}

/// Convert postfix unless statement to block form without adding indentation (for use within already indented blocks)
pub fn convert_postfix_unless_to_block_no_indent(condition: &str, statement: &str) -> String {
    format!("if (!({}) ) {{\n    {};\n}}", condition, statement)
}
