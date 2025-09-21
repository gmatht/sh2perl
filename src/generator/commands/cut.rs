use crate::ast::*;
use crate::generator::Generator;

pub fn generate_cut_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, _command_index: usize) -> String {
    let mut output = String::new();
    
    // cut command syntax: cut -d delimiter -f fields
    let mut delimiter = "\\t".to_string(); // Default tab delimiter
    let mut fields = "1".to_string(); // Default to first field
    
    // Parse cut options
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(arg, _) = &cmd.args[i] {
            if arg == "-d" && i + 1 < cmd.args.len() {
                if let Some(next_arg) = cmd.args.get(i + 1) {
                    delimiter = generator.word_to_perl(next_arg);
                    i += 1; // Skip the delimiter argument
                }
            } else if arg == "-f" && i + 1 < cmd.args.len() {
                if let Some(next_arg) = cmd.args.get(i + 1) {
                    fields = generator.word_to_perl(next_arg);
                    i += 1; // Skip the fields argument
                }
            }
        }
        i += 1;
    }
    
    let unique_id = generator.get_unique_id();
    output.push_str(&format!("my @lines_{} = split /\\n/msx, ${};\n", unique_id, input_var));
    output.push_str(&format!("my @result_{};\n", unique_id));
    output.push_str(&format!("foreach my $line (@lines_{}) {{\n", unique_id));
    output.push_str("chomp $line;\n");
    output.push_str(&format!("my @fields = split /{}/msx, $line;\n", delimiter));
    
    // Handle field selection - convert field number from 1-based to 0-based indexing
    let field_index = if fields.trim_matches('"').trim_matches('\'').parse::<usize>().unwrap_or(1) > 0 {
        fields.trim_matches('"').trim_matches('\'').parse::<usize>().unwrap_or(1) - 1
    } else {
        0
    };
    output.push_str(&format!("if (@fields > {}) {{\n", field_index));
    output.push_str(&format!("push @result_{}, $fields[{}];\n", unique_id, field_index));
    output.push_str("}\n");
    output.push_str("}\n");
    output.push_str(&format!("${} = join \"\\n\", @result_{};\n", input_var, unique_id));
    output.push_str("\n");
    
    output
}
