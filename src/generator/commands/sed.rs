use crate::ast::*;
use crate::generator::Generator;

pub fn generate_sed_command(_generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, command_index: usize) -> String {
    let mut output = String::new();
    
    // For now, implement basic sed-like functionality
    // This can be extended to handle more complex sed patterns
    output.push_str(&format!("my @sed_lines_{} = split(/\\n/, ${});\n", command_index, input_var));
    output.push_str(&format!("my @sed_result_{};\n", command_index));
    output.push_str(&format!("foreach my $line (@sed_lines_{}) {{\n", command_index));
    output.push_str("chomp($line);\n");
    
    // Handle common sed operations
    if !cmd.args.is_empty() {
        if let Word::Literal(first_arg, _) = &cmd.args[0] {
            if first_arg.starts_with("s/") {
                // Basic substitution: s/pattern/replacement/
                if cmd.args.len() >= 3 {
                    // Arguments are split: "s/pattern/", replacement, "/"
                    if let (Word::Literal(pattern_part, _), replacement_arg, Word::Literal(end_part, _)) = 
                        (&cmd.args[0], &cmd.args[1], &cmd.args[2]) {
                        
                        // Extract pattern from "s/pattern/"
                        let pattern = pattern_part.strip_prefix("s/").unwrap_or("");
                        let pattern = pattern.strip_suffix("/").unwrap_or(pattern);
                        
                        // Generate replacement - handle variables
                        let replacement = match replacement_arg {
                            Word::Variable(var_name, _, _) => format!("${}", var_name),
                            Word::Literal(lit, _) => lit.clone(),
                            _ => "".to_string(),
                        };
                        
                        output.push_str(&format!("$line =~ s/{}/{}/g;\n", pattern, replacement));
                    }
                } else {
                    // Single argument case: s/pattern/replacement/
                    let parts: Vec<&str> = first_arg.split('/').collect();
                    if parts.len() >= 3 {
                        let pattern = parts[1];
                        let replacement = parts[2];
                        output.push_str(&format!("$line =~ s/{}/{}/g;\n", pattern, replacement));
                    }
                }
            } else if first_arg == "d" {
                // Delete lines
                output.push_str("next;\n");
            }
        }
    }
    
    output.push_str(&format!("push @sed_result_{}, $line;\n", command_index));
    output.push_str("}\n");
    output.push_str(&format!("${} = join(\"\\n\", @sed_result_{});\n", input_var, command_index));
    output.push_str("\n");
    
    output
}
