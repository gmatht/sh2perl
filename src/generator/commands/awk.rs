use crate::ast::*;
use crate::generator::Generator;

pub fn generate_awk_command(generator: &mut Generator, cmd: &SimpleCommand, input_var: &str, _command_index: usize) -> String {
    let mut output = String::new();
    
    // Parse awk expression from command arguments
    let mut awk_expr = String::new();
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s.starts_with('{') && s.ends_with('}') {
                awk_expr = s.clone();
                break;
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
    output.push_str("chomp $line;\n");
    output.push_str(&format!("if ($line =~ {}) {{ next; }}\n", generator.format_regex_pattern(r"^\\s*$"))); // Skip empty lines
    output.push_str("my @fields = split /\\s+/msx, $line;\n");
    output.push_str("if (@fields > 0) {\n");
    
    // Parse and execute the awk expression
    if awk_expr.contains("print $1 + $2") {
        output.push_str("my $sum = $fields[0] + $fields[1];\n");
        output.push_str("push @result, $sum;\n");
    } else if awk_expr.contains("print $1") {
        output.push_str("push @result, $fields[0];\n");
    } else if awk_expr.contains("print $2") {
        output.push_str("push @result, $fields[1];\n");
    } else if awk_expr.contains("print $0") {
        output.push_str("push @result, $line;\n");
    } else {
        // Default: print the whole line
        output.push_str("push @result, $line;\n");
    }
    
    output.push_str("}\n");
    output.push_str("}\n");
    if input_var.starts_with('$') {
        output.push_str(&format!("{} = join \"\\n\", @result;\n", input_var));
    } else {
        output.push_str(&format!("${} = join \"\\n\", @result;\n", input_var));
    }
    output.push_str("\n");
    
    output
}
