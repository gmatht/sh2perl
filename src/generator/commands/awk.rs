use crate::ast::*;
use crate::generator::Generator;

pub fn generate_awk_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    _command_index: usize,
) -> String {
    let mut output = String::new();

    // Parse awk expression from command arguments.
    // Args are often provided quoted (for example "'{print toupper($0)}'"),
    // so strip a single layer of surrounding single- or double-quotes before
    // checking for the { ... } awk program form.
    let mut awk_expr = String::new();
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            // Clone so we can strip without modifying the original
            let mut lit = s.clone();
            if (lit.starts_with('\'') && lit.ends_with('\''))
                || (lit.starts_with('"') && lit.ends_with('"'))
            {
                if lit.len() >= 2 {
                    lit = lit[1..lit.len() - 1].to_string();
                }
            }

            if lit.starts_with('{') && lit.ends_with('}') {
                awk_expr = lit;
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
    output.push_str(&format!(
        "if ($line =~ {}) {{ next; }}\n",
        generator.format_regex_pattern(r"^\\s*$")
    )); // Skip empty lines
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
    } else if awk_expr.contains("toupper(") {
        // Handle common toupper usage (e.g. print toupper($0) or print toupper($1)).
        // Map to Perl's uc() on the appropriate field or whole line.
        if awk_expr.contains("$0") {
            output.push_str("push @result, uc($line);\n");
        } else if awk_expr.contains("$1") {
            output.push_str("push @result, uc($fields[0]);\n");
        } else if awk_expr.contains("$2") {
            output.push_str("push @result, uc($fields[1]);\n");
        } else {
            // Fallback: uppercase whole line
            output.push_str("push @result, uc($line);\n");
        }
    } else if awk_expr.contains("tolower(") {
        // Handle tolower similarly
        if awk_expr.contains("$0") {
            output.push_str("push @result, lc($line);\n");
        } else if awk_expr.contains("$1") {
            output.push_str("push @result, lc($fields[0]);\n");
        } else if awk_expr.contains("$2") {
            output.push_str("push @result, lc($fields[1]);\n");
        } else {
            output.push_str("push @result, lc($line);\n");
        }
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
