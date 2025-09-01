use crate::ast::*;
use crate::generator::Generator;

fn escape_glob_pattern(pattern: &str) -> String {
    pattern.chars().map(|c| match c {
        '*' => ".*".to_string(),
        '?' => ".".to_string(),
        '.' => "\\.".to_string(),
        '[' => "\\[".to_string(),
        ']' => "\\]".to_string(),
        '(' => "\\(".to_string(),
        ')' => "\\)".to_string(),
        '+' => "\\+".to_string(),
        '^' => "\\^".to_string(),
        '$' => "\\$".to_string(),
        '|' => "\\|".to_string(),
        '{' => "\\{".to_string(),
        '}' => "\\}".to_string(),
        _ => c.to_string()
    }).collect()
}

pub fn generate_find_command(generator: &mut Generator, cmd: &SimpleCommand, generate_output: bool, input_var: &str) -> String {
    let mut output = String::new();
    
    let mut path = ".";
    let mut pattern = "*.sh".to_string();
    
    // Parse find arguments
    let mut i = 0;
    while i < cmd.args.len() {
        if let Word::Literal(arg) = &cmd.args[i] {
            if arg == "." {
                // For find . command, search from the directory where the script is located
                // This ensures we find files relative to the script location, not current working directory
                path = ".";
            } else if arg == "-name" && i + 1 < cmd.args.len() {
                if let Some(next_arg) = cmd.args.get(i + 1) {
                    pattern = match next_arg {
                        Word::StringInterpolation(interp) => {
                            interp.parts.iter()
                                .map(|part| match part {
                                    crate::ast::StringPart::Literal(s) => s,
                                    _ => "*"
                                })
                                .collect::<Vec<_>>()
                                .join("")
                        },
                        _ => generator.word_to_perl(next_arg)
                    };
                    i += 1; // Skip the pattern argument
                }
            }
        }
        i += 1;
    }
    
    // Use unique variable names to prevent cross-contamination between pipelines
    let unique_id = generator.get_unique_id();
    let find_var = format!("find_files_{}", unique_id);
    let find_func = format!("find_files_{}", unique_id);
    
    output.push_str(&generator.indent());
    output.push_str(&format!("my @{};\n", find_var));
    output.push_str(&generator.indent());
    output.push_str(&format!("sub {} {{\n", find_func));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("my ($dir, $pattern) = @_;\n");
    output.push_str(&generator.indent());
    output.push_str("if (opendir(my $dh, $dir)) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("while (my $file = readdir($dh)) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("next if $file eq '.' || $file eq '..';\n");
    output.push_str(&generator.indent());
    output.push_str("my $full_path = $dir eq '.' ? \"./$file\" : \"$dir/$file\";\n");
    output.push_str(&generator.indent());
    output.push_str("if (-d $full_path) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("{}($full_path, $pattern);\n", find_func));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("} elsif ($file =~ /^$pattern$/) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("push @{}, $full_path;\n", find_var));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("closedir($dh);\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    // For find . command, search from current directory
    if path == "." {
        output.push_str(&format!("{}('.', '{}');\n", find_func, escape_glob_pattern(&pattern)));
    } else {
        output.push_str(&format!("{}('{}', '{}');\n", find_func, path, escape_glob_pattern(&pattern)));
    }
    
    if generate_output {
        output.push_str(&generator.indent());
        output.push_str(&format!("${} = join(\"\\n\", @{});\n", input_var, find_var));
        // Ensure output ends with newline to match shell behavior
        output.push_str(&generator.indent());
        output.push_str(&format!("${} .= \"\\n\" unless ${} =~ /\\n$/;\n", input_var, input_var));
    }
    output.push_str("\n");
    
    output
}
