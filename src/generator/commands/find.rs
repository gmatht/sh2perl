use crate::ast::*;
use crate::mir::*;
use crate::generator::Generator;

fn escape_glob_pattern(pattern: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    
    for (i, c) in chars.iter().enumerate() {
        match c {
            '*' => {
                if i == 0 {
                    // At start of pattern, * means "any characters"
                    result.push_str(".*");
                } else {
                    // In middle/end, * means "any characters"
                    result.push_str(".*");
                }
            },
            '?' => result.push_str("."),
            '.' => result.push_str("\\."),
            '[' => result.push_str("\\["),
            ']' => result.push_str("\\]"),
            '(' => result.push_str("\\("),
            ')' => result.push_str("\\)"),
            '+' => result.push_str("\\+"),
            '^' => result.push_str("\\^"),
            '$' => result.push_str("\\$"),
            '|' => result.push_str("\\|"),
            '{' => result.push_str("\\{"),
            '}' => result.push_str("\\}"),
            _ => result.push(*c)
        }
    }
    
    result
}

pub fn generate_find_command(generator: &mut Generator, cmd: &SimpleCommand, generate_output: bool, input_var: &str) -> String {
    let mut output = String::new();
    
    let mut path = ".";
    let mut pattern = "*".to_string(); // Default to all files
    let mut file_type = "f"; // Default to files only
    let mut has_name_filter = false;
    
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
                                    StringPart::Literal(s) => s,
                                    _ => "*"
                                })
                                .collect::<Vec<_>>()
                                .join("")
                        },
                        _ => generator.word_to_perl(next_arg)
                    };
                    has_name_filter = true;
                    i += 1; // Skip the pattern argument
                }
            } else if arg == "-type" && i + 1 < cmd.args.len() {
                if let Some(next_arg) = cmd.args.get(i + 1) {
                    if let Word::Literal(type_arg) = next_arg {
                        file_type = type_arg;
                    }
                    i += 1; // Skip the type argument
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
    
    // Check file type and pattern
    if file_type == "f" {
        output.push_str("} elsif (-f $full_path");
    } else if file_type == "d" {
        output.push_str("} elsif (-d $full_path");
    } else {
        output.push_str("} elsif (1"); // Accept all types
    }
    
    if has_name_filter {
        output.push_str(&format!(" && $file =~ /^{}$/", escape_glob_pattern(&pattern)));
    }
    output.push_str(") {\n");
    
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
        output.push_str(&format!("{}('.', '{}');\n", find_func, pattern));
    } else {
        output.push_str(&format!("{}('{}', '{}');\n", find_func, path, pattern));
    }
    
    if generate_output {
        output.push_str(&generator.indent());
        output.push_str(&format!("${} = join(\"\\n\", @{});\n", input_var, find_var));
        // Ensure output ends with newline to match shell behavior
        output.push_str(&generator.indent());
        output.push_str(&format!("${} .= \"\\n\" unless ${} =~ /\\n$/;\n", input_var, input_var));
    } else {
        // For standalone find commands, print results directly
        output.push_str(&generator.indent());
        output.push_str(&format!("for my $file (@{}) {{\n", find_var));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("print \"$file\\n\";\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }
    output.push_str("\n");
    
    output
}
